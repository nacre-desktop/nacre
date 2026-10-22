//! Startup health and one last-known-good restore; runtime failures belong to systemd.
use crate::{compositor::backend::Backend, paths::Paths, process, signals::Signals, Result};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fd::OwnedFd,
    fs::inotify,
    process::{pidfd_open, pidfd_send_signal, Pid, PidfdFlags},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::{fs::FileTypeExt, process::ExitStatusExt},
    path::{Path, PathBuf},
    process::{Child, Command},
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use wait_timeout::ChildExt;

pub type Environment = BTreeMap<String, String>;
fn sockets(runtime: &Path) -> Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(runtime) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    let mut result = Vec::new();
    for entry in entries {
        let entry = entry?;
        if entry.file_name().to_string_lossy().starts_with("wayland-")
            && fs::metadata(entry.path())?.file_type().is_socket()
        {
            result.push(entry.path());
        }
    }
    Ok(result)
}
fn choose(environment: &mut Environment, runtime: &Path) -> Result<bool> {
    let selected = PathBuf::from(
        environment
            .get("WAYLAND_DISPLAY")
            .map(String::as_str)
            .unwrap_or(""),
    );
    let selected = if selected.is_absolute() {
        selected
    } else {
        runtime.join(selected)
    };
    let valid = fs::metadata(selected).is_ok_and(|value| value.file_type().is_socket());
    let candidates = sockets(runtime)?;
    if valid {
        return Ok(true);
    }
    if candidates.len() == 1 {
        environment.insert(
            "WAYLAND_DISPLAY".into(),
            candidates[0]
                .file_name()
                .ok_or("Missing display name")?
                .to_string_lossy()
                .into_owned(),
        );
        return Ok(true);
    }
    Ok(false)
}
fn receive_stop(signals: &Signals, stopping: &mut bool, child: Option<&OwnedFd>) -> Result<()> {
    while signals.fd.read_signal()?.is_some() {
        *stopping = true;
        if let Some(child) = child {
            match pidfd_send_signal(child, rustix::process::Signal::TERM) {
                Ok(()) | Err(rustix::io::Errno::SRCH) => (),
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(())
}
/// Explicit deadline also lets developer fixtures exercise timeout without a 20s wait.
pub fn environment(
    mut environment: Environment,
    timeout: Duration,
    signals: Option<(&Signals, &mut bool)>,
) -> Result<Environment> {
    let runtime = environment
        .get("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", nix::unistd::getuid())));
    let runtime = if runtime.as_os_str().is_empty() {
        PathBuf::from(".")
    } else {
        runtime
    };
    let watcher = inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK)?;
    let deadline = Instant::now() + timeout;
    let mut signals = signals;
    loop {
        let mut watched = BTreeSet::new();
        // Subscribe before checking: socket creation between scan and poll cannot be lost.
        let mut parents = vec![runtime.clone()];
        if let Some(selected) = environment
            .get("WAYLAND_DISPLAY")
            .map(Path::new)
            .filter(|p| p.is_absolute())
        {
            if let Some(parent) = selected.parent() {
                parents.push(parent.to_owned());
            }
        }
        for mut parent in parents {
            while !parent.is_dir() {
                parent = parent.parent().ok_or("No runtime ancestor")?.to_owned();
            }
            if watched.insert(parent.clone()) {
                inotify::add_watch(
                    &watcher,
                    &parent,
                    inotify::WatchFlags::CREATE
                        | inotify::WatchFlags::MOVED_TO
                        | inotify::WatchFlags::ATTRIB
                        | inotify::WatchFlags::DELETE_SELF
                        | inotify::WatchFlags::MOVE_SELF,
                )?;
            }
        }
        if choose(&mut environment, &runtime)? {
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(crate::i18n::tr(
                "helper.shell-supervisor.wayland-display-is-not-ready",
                "Wayland display is not ready",
            )
            .into());
        }
        let mut fds = vec![PollFd::new(&watcher, PollFlags::IN)];
        if let Some((signals, _)) = &signals {
            fds.push(PollFd::new(&signals.fd, PollFlags::IN));
        }
        match poll(&mut fds, Some(&Timespec::try_from(remaining)?)) {
            Ok(_) => (),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
        if let Some((signals, stopping)) = signals.as_mut() {
            receive_stop(signals, stopping, None)?;
        }
        let mut buffer = [0u8; 8192];
        match rustix::io::read(&watcher, &mut buffer) {
            Ok(_) | Err(rustix::io::Errno::AGAIN) => (),
            Err(error) => return Err(error.into()),
        }
    }
    let paths = Paths::environment()?;
    let mut backend = Backend::new(paths, environment.clone())?;
    let updates = backend.module_call("environment_updates", &[json!(environment)], &json!({}))?;
    for (key, value) in updates
        .as_object()
        .ok_or("Invalid compositor environment")?
    {
        environment.insert(
            key.clone(),
            value
                .as_str()
                .ok_or("Invalid compositor environment value")?
                .to_owned(),
        );
    }
    environment
        .entry("XDG_SESSION_TYPE".into())
        .or_insert_with(|| "wayland".into());
    Ok(environment)
}
pub fn health(root: &Path, environment: &Environment) -> Result<bool> {
    // The windows-only host (beside Bead, `nacre ui-shell`) has no `nacre` IPC target by design; it is
    // ready once a window host answers. Without this it was killed and restarted every ~22 s (B0033).
    let windows_only = environment
        .get("NACRE_QML_WINDOWS_ONLY")
        .is_some_and(|value| value == "1");
    let mut command = Command::new(root.join("bin/qs"));
    command
        .args(["-c", "nacre", "ipc", "call"])
        .args(if windows_only {
            ["settingsApp", "state"]
        } else {
            ["nacre", "state"]
        })
        .env_clear()
        .envs(environment);
    let output = match process::capture(command, None, Duration::from_secs(2)) {
        Ok(value) if value.status.success() => value,
        _ => return Ok(false),
    };
    if windows_only {
        return Ok(true);
    }
    let value: Value = match serde_json::from_slice(&output.stdout) {
        Ok(value) => value,
        Err(_) => return Ok(false),
    };
    let contains = |key: &str| -> Result<bool> {
        match &value {
            Value::Object(map) => Ok(map.contains_key(key)),
            Value::Array(values) => Ok(values.contains(&json!(key))),
            Value::String(text) => Ok(text.contains(key)),
            _ => Err("Invalid shell health response shape".into()),
        }
    };
    Ok(contains("reveal")? && contains("launcherMode")?)
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        if fs::metadata(entry.path())?.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), &destination)?;
            let metadata = fs::metadata(entry.path())?;
            fs::File::open(&destination)?.set_times(
                fs::FileTimes::new()
                    .set_accessed(metadata.accessed()?)
                    .set_modified(metadata.modified()?),
            )?;
        }
    }
    fs::set_permissions(target, fs::metadata(source)?.permissions())?;
    let metadata = fs::metadata(source)?;
    fs::File::open(target)?.set_times(
        fs::FileTimes::new()
            .set_accessed(metadata.accessed()?)
            .set_modified(metadata.modified()?),
    )?;
    Ok(())
}
pub fn restore(root: &Path, stamp: &str) -> Result<bool> {
    let good = root.join("source.good");
    if !good.join("shell.qml").is_file() {
        return Ok(false);
    }
    let source = root.join("source");
    if source.exists() {
        fs::rename(&source, root.join(format!("source.failed-{stamp}")))?;
    }
    copy_tree(&good, &source)?;
    println!("Recovered the last validated Nacre desktop source.");
    Ok(true)
}
fn wait_event(
    child: &OwnedFd,
    signals: &Signals,
    stopping: &mut bool,
    timeout: Option<Duration>,
) -> Result<()> {
    let mut fds = [
        PollFd::new(child, PollFlags::IN),
        PollFd::new(&signals.fd, PollFlags::IN),
    ];
    let timeout = timeout.map(Timespec::try_from).transpose()?;
    match poll(&mut fds, timeout.as_ref()) {
        Ok(_) | Err(rustix::io::Errno::INTR) => (),
        Err(error) => return Err(error.into()),
    }
    receive_stop(signals, stopping, Some(child))
}
fn startup_health(
    root: &Path,
    environment: &Environment,
    child: &OwnedFd,
    signals: &Signals,
    stopping: &mut bool,
) -> Result<bool> {
    let wake = Arc::new(rustix::event::eventfd(
        0,
        rustix::event::EventfdFlags::CLOEXEC,
    )?);
    let worker_wake = wake.clone();
    let root = root.to_owned();
    let environment = environment.clone();
    let worker = thread::spawn(move || {
        let result = health(&root, &environment);
        let notified = rustix::io::write(&*worker_wake, &1u64.to_ne_bytes());
        (result, notified)
    });
    loop {
        let mut fds = [
            PollFd::new(&*wake, PollFlags::IN),
            PollFd::new(&signals.fd, PollFlags::IN),
        ];
        match poll(&mut fds, None) {
            Ok(_) => (),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
        receive_stop(signals, stopping, Some(child))?;
        if fds[0].revents().contains(PollFlags::IN) {
            break;
        }
    }
    let (result, notified) = worker.join().map_err(|_| "Health reader failed")?;
    notified?;
    result
}
pub fn run(stamp: Option<&str>) -> Result<i32> {
    let signals = Signals::new()?;
    let mut stopping = false;
    let environment = environment(
        std::env::vars().collect(),
        Duration::from_secs(20),
        Some((&signals, &mut stopping)),
    )?;
    let root = Paths::environment()?.share.join("shell");
    let mut recovered = false;
    while !stopping {
        let mut command = Command::new(root.join("bin/qs"));
        command
            .args(["-c", "nacre", "-n"])
            .env_clear()
            .envs(&environment);
        process::restore_child_signals(&mut command);
        let mut child: Child = command.spawn()?;
        let process = pidfd_open(
            Pid::from_raw(child.id() as i32).ok_or("Invalid shell PID")?,
            PidfdFlags::empty(),
        )?;
        receive_stop(&signals, &mut stopping, Some(&process))?;
        let mut ready = false;
        let deadline = Instant::now() + Duration::from_secs(20);
        while !stopping && child.try_wait()?.is_none() && Instant::now() < deadline {
            if startup_health(&root, &environment, &process, &signals, &mut stopping)? {
                ready = true;
                break;
            }
            wait_event(
                &process,
                &signals,
                &mut stopping,
                Some(Duration::from_millis(200)),
            )?;
        }
        if !ready && !stopping {
            if child.try_wait()?.is_none() {
                match pidfd_send_signal(&process, rustix::process::Signal::TERM) {
                    Ok(()) | Err(rustix::io::Errno::SRCH) => (),
                    Err(error) => return Err(error.into()),
                }
                if child.wait_timeout(Duration::from_secs(5))?.is_none() {
                    return Err("Shell termination timed out after 5 seconds".into());
                }
            }
            let now = stamp
                .map(str::to_owned)
                .unwrap_or_else(|| chrono::Local::now().format("%Y%m%dT%H%M%S").to_string());
            if !recovered && restore(&root, &now)? {
                recovered = true;
                continue;
            }
            return Ok(1);
        }
        while child.try_wait()?.is_none() {
            wait_event(&process, &signals, &mut stopping, None)?;
        }
        let status = child.wait()?;
        if stopping || status.success() {
            return Ok(0);
        }
        return Ok(status
            .code()
            .unwrap_or_else(|| -status.signal().unwrap_or(1)));
    }
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A fake `bin/qs`: only the named IPC target answers, like a quickshell instance hosting it.
    fn root_answering(target: &str, reply: &str) -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("bin")).unwrap();
        let script = root.path().join("bin/qs");
        fs::write(
            &script,
            format!(
                "#!/bin/sh\n[ \"$5\" = {target} ] || {{ echo 'Target not found.' >&2; exit 1; }}\necho '{reply}'\n"
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        root
    }

    #[test]
    fn full_shell_needs_the_nacre_target_with_its_keys() {
        let environment = Environment::new();
        let ready = root_answering("nacre", r#"{"reveal":false,"launcherMode":""}"#);
        assert!(health(ready.path(), &environment).unwrap());
        let windows_host = root_answering("settingsApp", r#"{"visible":false}"#);
        assert!(!health(windows_host.path(), &environment).unwrap());
    }

    #[test]
    fn windows_only_host_is_ready_when_a_window_host_answers() {
        let environment = Environment::from([("NACRE_QML_WINDOWS_ONLY".into(), "1".into())]);
        let windows_host = root_answering("settingsApp", r#"{"visible":false}"#);
        assert!(health(windows_host.path(), &environment).unwrap());
        let full_shell_only = root_answering("nacre", r#"{"reveal":false,"launcherMode":""}"#);
        assert!(!health(full_shell_only.path(), &environment).unwrap());
    }
}
