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
    if let Some(input) = input {
        if let Some(mut stream) = child.stdin.take() {
            stream.write_all(input)?;
        }
    }
    let status = match child.wait_timeout(timeout)? {
        Some(status) => status,
        None => {
            child.kill()?;
            child.wait()?;
            output.join().map_err(|_| "Child output reader failed")??;
            error.join().map_err(|_| "Child error reader failed")??;
            return Err(format!("Command timed out after {} seconds", timeout.as_secs()).into());
        }
    };
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
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(program);
    command.args(arguments);
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
fn restore_child_signals(command: &mut Command) {
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
