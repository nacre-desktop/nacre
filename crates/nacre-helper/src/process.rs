//! Bounded capture without a shell; drain both pipes while waiting for the child.
use crate::Result;
use std::{
    io::{Read, Write},
    process::{Command, Output, Stdio},
    thread,
    time::Duration,
};
use wait_timeout::ChildExt;
pub fn run(
    program: &str,
    arguments: &[String],
    input: Option<&str>,
    timeout: Duration,
) -> Result<Output> {
    run_bytes(program, arguments, input.map(str::as_bytes), timeout)
}
pub fn run_bytes(
    program: &str,
    arguments: &[String],
    input: Option<&[u8]>,
    timeout: Duration,
) -> Result<Output> {
    let mut command = Command::new(program);
    command.args(arguments);
    capture(command, input, timeout)
}
pub fn capture(mut command: Command, input: Option<&[u8]>, timeout: Duration) -> Result<Output> {
    command
        .stdin(if input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    restore_child_signals(&mut command);
    let mut child = command.spawn()?;
    let mut stdout = child.stdout.take().ok_or("Missing child output")?;
    let mut stderr = child.stderr.take().ok_or("Missing child error output")?;
    let output = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let error = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    // Start the deadline while stdin is being written too. A sync peer that
    // stops reading must not hold a large vault request past its timeout.
    let input_writer = input.and_then(|input| {
        child.stdin.take().map(|mut stream| {
            let input = input.to_vec();
            thread::spawn(move || stream.write_all(&input))
        })
    });
    let status = match child.wait_timeout(timeout)? {
        Some(status) => status,
        None => {
            child.kill()?;
            child.wait()?;
            if let Some(writer) = input_writer {
                let _ = writer.join();
            }
            output.join().map_err(|_| "Child output reader failed")??;
            error.join().map_err(|_| "Child error reader failed")??;
            return Err(format!("Command timed out after {} seconds", timeout.as_secs()).into());
        }
    };
    if let Some(writer) = input_writer {
        if let Err(error) = writer.join().map_err(|_| "Child input writer failed")? {
            // Like subprocess.communicate, preserve an early child's own
            // exit status and stderr when it closes stdin without reading it.
            if error.kind() != std::io::ErrorKind::BrokenPipe {
                return Err(error.into());
            }
        }
    }
    Ok(Output {
        status,
        stdout: output.join().map_err(|_| "Child output reader failed")??,
        stderr: error.join().map_err(|_| "Child error reader failed")??,
    })
}

pub fn checked(
    program: &str,
    arguments: &[String],
    input: Option<&str>,
    timeout: Duration,
) -> Result<Output> {
    let output = run(program, arguments, input, timeout)?;
    if !output.status.success() {
        return Err(format!(
            "Command failed with exit status {}",
            output.status.code().unwrap_or(1)
        )
        .into());
    }
    Ok(output)
}

/// Preserve Python's start_new_session for explicit user-launched desktop apps.
#[allow(unsafe_code)]
pub fn spawn_detached(program: &str, arguments: &[String]) -> Result<std::process::Child> {
    spawn_detached_io(program, arguments, false)
}

/// A long-lived launch must not inherit a one-shot helper response pipe.
pub fn spawn_detached_quiet(program: &str, arguments: &[String]) -> Result<std::process::Child> {
    spawn_detached_io(program, arguments, true)
}

#[allow(unsafe_code)]
fn spawn_detached_io(
    program: &str,
    arguments: &[String],
    quiet: bool,
) -> Result<std::process::Child> {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(program);
    command.args(arguments);
    if quiet {
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
    }
    restore_child_signals(&mut command);
    // SAFETY: after fork the hook calls only the async-signal-safe setsid syscall.
    // It captures nothing and performs no allocation, locking or Rust callbacks.
    unsafe {
        command.pre_exec(|| {
            nix::unistd::setsid()
                .map(|_| ())
                .map_err(|error| std::io::Error::from_raw_os_error(error as i32))
        });
    }
    Ok(command.spawn()?)
}

// Selector signalfd handling blocks these signals in its own thread; child
// commands must retain the previous Python helper's unblocked signal behavior.
#[allow(unsafe_code)]
pub(crate) fn restore_child_signals(command: &mut Command) {
    use std::os::unix::process::CommandExt;
    // SAFETY: only async-signal-safe sigemptyset/sigaddset/pthread_sigmask calls,
    // with stack data, no heap allocation, locks or arbitrary user callbacks.
    unsafe {
        command.pre_exec(|| {
            use nix::sys::signal::{pthread_sigmask, SigSet, SigmaskHow, Signal};
            let mut signals = SigSet::empty();
            signals.add(Signal::SIGINT);
            signals.add(Signal::SIGTERM);
            pthread_sigmask(SigmaskHow::SIG_UNBLOCK, Some(&signals), None)
                .map_err(|error| std::io::Error::from_raw_os_error(error as i32))
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_includes_a_child_that_never_reads_stdin() {
        let mut command = Command::new("sleep");
        command.arg("5");
        let start = std::time::Instant::now();
        let error = capture(
            command,
            Some(&vec![b'x'; 512 * 1024]),
            Duration::from_millis(20),
        )
        .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        assert!(start.elapsed() < Duration::from_secs(2));
    }
}

/// The off-loaded job panicked or the daemon is shutting down.
#[derive(Debug)]
pub struct Offloaded;
impl std::fmt::Display for Offloaded {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Off-loaded task did not finish")
    }
}
impl std::error::Error for Offloaded {}

/// Run blocking work that may wait on an external backend (a child process, a sysfs scan) on its
/// own short-lived thread instead of tokio's two-thread blocking pool. A hung backend can then
/// occupy at most `OFFLOAD_LIMIT` threads and never starves the pool that settings, files and
/// the compositor reader use, so daemon readiness cannot wait on it (N0053).
pub async fn offload<T: Send + 'static>(
    work: impl FnOnce() -> T + Send + 'static,
) -> std::result::Result<T, Offloaded> {
    const OFFLOAD_LIMIT: usize = 8;
    static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(OFFLOAD_LIMIT);
    let permit = SLOTS.acquire().await.map_err(|_| Offloaded)?;
    let (send, receive) = tokio::sync::oneshot::channel();
    thread::Builder::new()
        .name("nacre-offload".into())
        .stack_size(512 * 1024)
        .spawn(move || {
            let _ = send.send(work());
        })
        .map_err(|_| Offloaded)?;
    let result = receive.await.map_err(|_| Offloaded);
    drop(permit);
    result
}
