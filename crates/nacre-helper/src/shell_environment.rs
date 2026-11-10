//! Wait for the Wayland socket and export compositor environment updates.
use crate::{compositor::backend::Backend, paths::Paths, signals::Signals, Result};
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fd::OwnedFd,
    fs::inotify,
    process::pidfd_send_signal,
};
use serde_json::json;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::FileTypeExt,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

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
                "helper.shell-environment.wayland-display-is-not-ready",
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
