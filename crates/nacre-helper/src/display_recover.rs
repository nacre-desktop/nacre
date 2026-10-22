//! Restart the renderer after a geometry change and restore its public UI state.
use crate::{i18n::tr, paths::Paths, Result};
use serde_json::Value;
use std::{
    fs,
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    process::Command,
    thread,
    time::{Duration, Instant},
};

fn runtime() -> Result<PathBuf> {
    Ok(std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", nix::unistd::getuid()))))
}
fn private_dir(path: &std::path::Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != nix::unistd::getuid().as_raw()
    {
        return Err("Refusing non-private display recovery directory".into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
fn write_snapshot(folder: &std::path::Path, view: &Value) -> Result<PathBuf> {
    private_dir(folder)?;
    let path = folder.join(format!("{}.json", uuid::Uuid::new_v4()));
    let bytes = serde_json::to_vec(view)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    use std::io::Write;
    file.write_all(&bytes)?;
    file.sync_all()?;
    Ok(path)
}
fn lock_recovery(runtime: &std::path::Path) -> Result<Option<nix::fcntl::Flock<fs::File>>> {
    let path = runtime.join("nacre-display-recovery.lock");
    if fs::symlink_metadata(&path).is_ok_and(|metadata| {
        metadata.file_type().is_symlink() || metadata.uid() != nix::unistd::getuid().as_raw()
    }) {
        return Err("Refusing unsafe display recovery lock".into());
    }
    let lock = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)?;
    match nix::fcntl::Flock::lock(lock, nix::fcntl::FlockArg::LockExclusiveNonblock) {
        Ok(guard) => Ok(Some(guard)),
        Err((_, nix::errno::Errno::EAGAIN)) => Ok(None),
        Err((_, error)) => Err(error.into()),
    }
}
fn load_saved(runtime: &std::path::Path, value: &str) -> Result<(Value, Option<PathBuf>)> {
    let Some(saved) = value.strip_prefix('@') else {
        return Ok((serde_json::from_str(value)?, None));
    };
    let path = PathBuf::from(saved);
    let base = runtime.join("nacre-display-recovery");
    let metadata = fs::symlink_metadata(&path)?;
    if !path.starts_with(&base)
        || metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.uid() != nix::unistd::getuid().as_raw()
    {
        return Err(tr(
            "helper.display-recover.invalid-private-display-snapshot",
            "Invalid private display snapshot",
        )
        .into());
    }
    Ok((serde_json::from_slice(&fs::read(&path)?)?, Some(path)))
}
fn run_command(program: &str, args: &[&str], timeout: Duration) -> Result<String> {
    let arguments = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let output = crate::process::run(program, &arguments, None, timeout)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_owned()
            .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}
fn queue(view: &Value) -> Result<i32> {
    let path = write_snapshot(&runtime()?.join("nacre-display-recovery"), view)?;
    let executable = std::env::current_exe()?;
    let unit = format!(
        "--unit=nacre-display-fallback-{}",
        uuid::Uuid::new_v4().simple()
    );
    if !Command::new("systemd-run")
        .args(["--user", "--collect", "--quiet", &unit])
        .arg(executable)
        .arg("display-recover")
        .arg(format!("@{}", path.display()))
        .status()?
        .success()
    {
        let _ = fs::remove_file(path);
        return Err("Could not queue display recovery".into());
    }
    Ok(0)
}
fn recover(view: Value, saved: Option<PathBuf>) -> Result<i32> {
    let paths = Paths::environment()?;
    let Some(_guard) = lock_recovery(&runtime()?)? else {
        return Ok(0);
    };
    if run_command(
        &format!("{}/nacre-helper", paths.tools().display()),
        &["lock-config"],
        Duration::from_secs(4),
    )
    .is_err()
    {
        eprintln!("Could not refresh lock layout; preserving the previous configuration");
    }
    if !Command::new("systemctl")
        .args(["--user", "restart", "nacre-shell.service"])
        .status()?
        .success()
    {
        return Err("Could not restart display renderer".into());
    }
    let qs = format!("{}/bin/qs", paths.share.display());
    let call = |args: &[&str], timeout: Duration| {
        let all = [
            vec!["-c", "nacre", "ipc", "call", "nacre"],
            args.iter().copied().collect(),
        ]
        .concat();
        run_command(&qs, &all, timeout)
    };
    let deadline = Instant::now() + Duration::from_secs(15);
    while Instant::now() < deadline {
        if let Ok(output) = call(&["state"], Duration::from_secs(2)) {
            if serde_json::from_str::<Value>(&output)
                .is_ok_and(|state| state.get("launcherMode").is_some())
            {
                let encoded = serde_json::to_string(&view)?;
                let _ = run_command(
                    &qs,
                    &[
                        "-c",
                        "nacre",
                        "ipc",
                        "call",
                        "nacre",
                        "restoreViews",
                        &encoded,
                    ],
                    Duration::from_secs(3),
                )?;
                if let Some(path) = saved {
                    let _ = fs::remove_file(path);
                }
                if view["dashboard"] == true && view["tab"] == 4 {
                    let _ = run_command(
                        &qs,
                        &[
                            "-c",
                            "nacre",
                            "ipc",
                            "call",
                            "settingsView",
                            "open",
                            "displays",
                        ],
                        Duration::from_secs(3),
                    );
                }
                return Ok(0);
            }
        }
        thread::sleep(Duration::from_millis(200));
    }
    Err(tr(
        "helper.display-recover.display-renderer-did-not-become-ready",
        "Display renderer did not become ready",
    )
    .into())
}
pub fn run(args: &[String]) -> Result<i32> {
    let queue_mode = args.iter().any(|arg| arg == "--queue");
    let value = args
        .iter()
        .find(|arg| *arg != "--queue")
        .map(String::as_str);
    if queue_mode {
        use std::io::Read;
        let mut input = String::new();
        std::io::stdin()
            .take(2 * 1024 * 1024)
            .read_to_string(&mut input)?;
        let view: Value = serde_json::from_str(&input)?;
        if !view.is_object() {
            return Err("Invalid display state".into());
        }
        return queue(&view);
    }
    let runtime = runtime()?;
    let (view, saved) = load_saved(&runtime, value.unwrap_or("{}"))?;
    recover(view, saved)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::{os::unix::fs::symlink, os::unix::fs::PermissionsExt};
    #[test]
    fn queue_snapshot_directory_and_file_are_private() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("nacre-display-recovery");
        let path = write_snapshot(&folder, &serde_json::json!({"dashboard":true})).unwrap();
        assert_eq!(
            fs::metadata(folder).unwrap().permissions().mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(path).unwrap()).unwrap()["dashboard"],
            true
        );
    }
    #[test]
    fn a_second_recovery_lock_is_a_noop() {
        let root = tempfile::tempdir().unwrap();
        let _first = lock_recovery(root.path()).unwrap().unwrap();
        assert!(lock_recovery(root.path()).unwrap().is_none());
    }
    #[test]
    fn malformed_and_linked_saved_snapshots_are_rejected() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("nacre-display-recovery");
        fs::create_dir(&folder).unwrap();
        let bad = folder.join("bad.json");
        fs::write(&bad, "{").unwrap();
        assert!(load_saved(root.path(), &format!("@{}", bad.display())).is_err());
        let target = folder.join("target.json");
        fs::write(&target, "{}").unwrap();
        let link = folder.join("link.json");
        symlink(&target, &link).unwrap();
        assert!(load_saved(root.path(), &format!("@{}", link.display())).is_err());
        assert!(load_saved(root.path(), "{broken").is_err());
    }
}
