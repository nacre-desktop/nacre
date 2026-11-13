//! Bounded capture without a shell; drain both pipes while waiting for the child.
use crate::Result;
use std::{
    io::{Read, Write},
    process::{Command, Output, Stdio},
    thread,
    time::Duration,
};
use wait_timeout::ChildExt;

#[cfg(test)]
pub(crate) static SCOPE_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Configure only the daemon, before its runtime or backend threads start.
pub(crate) fn configure_daemon_allocator() {
    #[cfg(target_env = "gnu")]
    #[allow(unsafe_code)]
    // SAFETY: called during single-threaded daemon startup, with a valid glibc
    // parameter. Keep the default 128 KiB threshold static: its dynamic growth
    // otherwise moves later raster buffers into persistent thread arenas.
    unsafe {
        nix::libc::mallopt(nix::libc::M_MMAP_THRESHOLD, 128 * 1024);
    }
}

/// Return freed burst allocations to the OS after their owning scope has ended.
pub(crate) fn reclaim_unused_memory() {
    #[cfg(target_env = "gnu")]
    #[allow(unsafe_code)]
    // SAFETY: malloc_trim accepts no pointers and is thread-safe.
    unsafe {
        nix::libc::malloc_trim(0);
    }
}

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

/// Build a transient user scope so an application is owned by the user manager,
/// not by whichever Nacre service happened to start it.
pub fn scope_command(argv: &[String]) -> Result<Command> {
    scope_command_at(argv, None)
}

pub fn scope_command_at(argv: &[String], cwd: Option<&std::path::Path>) -> Result<Command> {
    let (program, arguments) = argv.split_first().ok_or("Empty application command")?;
    if !application_available(program, cwd) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("{program}: {}", std::io::Error::from_raw_os_error(2)),
        )
        .into());
    }
    let unit = format!("nacre-app-{}.scope", uuid::Uuid::new_v4());
    let token = crate::compositor::niri_transport::prepare_inherited_launch(Some(&unit));
    let mut command = Command::new("systemd-run");
    command.args(["--user", "--scope", "--collect", "--quiet"]);
    if let Some(token) = token {
        command.args(scope_token_args(&unit, &token));
    }
    command.args([
        "env",
        "-u",
        "LD_LIBRARY_PATH",
        "-u",
        "QML_IMPORT_PATH",
        "-u",
        "QML2_IMPORT_PATH",
        "-u",
        "QT_PLUGIN_PATH",
        "-u",
        "NACRE_LIB_DIR",
        "-u",
        "NOTIFY_SOCKET",
        "-u",
        "WATCHDOG_USEC",
        "-u",
        "WATCHDOG_PID",
        "-u",
        "LISTEN_PID",
        "-u",
        "LISTEN_FDS",
        "-u",
        "LISTEN_FDNAMES",
    ]);
    command.arg(program).args(arguments);
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    Ok(command)
}

fn scope_token_args(unit: &str, token: &str) -> [String; 3] {
    [
        format!("--unit={unit}"),
        format!("--setenv=XDG_ACTIVATION_TOKEN={token}"),
        format!("--setenv=DESKTOP_STARTUP_ID={token}"),
    ]
}

fn application_available(program: &str, cwd: Option<&std::path::Path>) -> bool {
    use std::os::unix::fs::PermissionsExt;
    let executable = |path: &std::path::Path| {
        std::fs::metadata(path)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    };
    let base = cwd
        .map(std::path::Path::to_path_buf)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_default();
    if program.contains('/') {
        let path = std::path::Path::new(program);
        let path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            base.join(path)
        };
        executable(&path)
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|directory| {
            let directory = if directory.is_absolute() {
                directory
            } else {
                base.join(directory)
            };
            executable(&directory.join(program))
        })
    }
}

pub fn launch_app_at_detached(argv: &[String], cwd: Option<&std::path::Path>) -> Result<()> {
    let mut command = scope_command_at(argv, cwd)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    restore_child_signals(&mut command);
    let mut child = command.spawn()?;
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

/// Start an application in its own transient user scope, inheriting configured IO.
pub fn launch_app(argv: &[String]) -> Result<std::process::Child> {
    launch_app_at(argv, None)
}

pub fn launch_app_at(
    argv: &[String],
    cwd: Option<&std::path::Path>,
) -> Result<std::process::Child> {
    let mut command = scope_command_at(argv, cwd)?;
    restore_child_signals(&mut command);
    Ok(command.spawn()?)
}

pub fn launch_app_detached(argv: &[String]) -> Result<()> {
    launch_app_at_detached(argv, None)
}

pub fn launch_app_quiet(argv: &[String]) -> Result<std::process::Child> {
    let mut command = scope_command(argv)?;
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    restore_child_signals(&mut command);
    Ok(command.spawn()?)
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

    #[test]
    #[allow(unsafe_code)]
    fn app_launch_uses_a_transient_user_scope() {
        let _guard = SCOPE_TEST_LOCK
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let root = tempfile::tempdir().unwrap();
        let runner = root.path().join("systemd-run");
        let log = root.path().join("argv");
        std::fs::write(
            &runner,
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$NACRE_SCOPE_TEST_LOG\"\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&runner, std::fs::Permissions::from_mode(0o700)).unwrap();
        let old_path = std::env::var_os("PATH");
        let path = std::env::join_paths(
            std::iter::once(root.path().to_path_buf())
                .chain(std::env::split_paths(&old_path.clone().unwrap_or_default())),
        )
        .unwrap();
        std::env::set_var("PATH", path);
        std::env::set_var("NACRE_SCOPE_TEST_LOG", &log);
        let mut child =
            launch_app(&["kitty".into(), "--title".into(), "scope test".into()]).unwrap();
        assert!(child.wait().unwrap().success());
        if let Some(path) = old_path {
            std::env::set_var("PATH", path);
        }
        std::env::remove_var("NACRE_SCOPE_TEST_LOG");
        let argv = std::fs::read_to_string(log).unwrap();
        assert!(argv.lines().any(|arg| arg == "--user"));
        assert!(argv.lines().any(|arg| arg == "--scope"));
        assert!(argv.lines().any(|arg| arg == "--collect"));
        assert!(argv.lines().any(|arg| arg == "kitty"));
        assert!(argv.lines().any(|arg| arg == "scope test"));
    }

    #[test]
    fn scoped_app_launch_passes_activation_token_in_systemd_argv() {
        assert_eq!(
            scope_token_args("nacre-app-test.scope", "fresh-token"),
            [
                "--unit=nacre-app-test.scope",
                "--setenv=XDG_ACTIVATION_TOKEN=fresh-token",
                "--setenv=DESKTOP_STARTUP_ID=fresh-token",
            ]
        );
    }

    #[test]
    fn relative_app_paths_are_checked_against_the_selected_directory() {
        let root = tempfile::tempdir().unwrap();
        let app = root.path().join("launch-me");
        std::fs::write(&app, "#!/bin/sh\nexit 0\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert!(application_available("./launch-me", Some(root.path())));
        assert!(!application_available(
            "./launch-me",
            Some(&root.path().join("elsewhere"))
        ));
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
