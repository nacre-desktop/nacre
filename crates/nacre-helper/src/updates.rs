//! Read-only bounded update status worker. This helper never upgrades packages.
use crate::{paths::Paths, process, Result};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    process::Command,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn available(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|directory| {
        let candidate = directory.join(name);
        fs::metadata(candidate)
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
}
pub fn needs_host_runtime(mapping: &str) -> bool {
    let fields = mapping.split_whitespace().collect::<Vec<_>>();
    fields != ["0", "0", "4294967295"]
}
fn package_command_for(command: &[String], namespaced: bool) -> Vec<String> {
    if namespaced {
        let mut c = vec![
            "systemd-run".into(),
            "--user".into(),
            "--wait".into(),
            "--pipe".into(),
            "--collect".into(),
            "--quiet".into(),
            "--".into(),
        ];
        c.extend_from_slice(command);
        c
    } else {
        command.to_vec()
    }
}
fn package_command(command: &[String]) -> Vec<String> {
    let namespaced = fs::read_to_string("/proc/self/uid_map")
        .ok()
        .is_some_and(|s| needs_host_runtime(&s));
    package_command_for(command, namespaced)
}
fn run_command(command: &[String], timeout: Duration, cause: &str) -> Result<std::process::Output> {
    eprintln!(
        "nacre-helper updates: starting {} for {}",
        command.first().map(String::as_str).unwrap_or("command"),
        cause
    );
    let wrapped = package_command(command);
    let (exe, args) = wrapped.split_first().ok_or("empty update command")?;
    process::run(exe, args, None, timeout)
}
fn bounded_output(
    program: &str,
    args: &[&str],
    timeout: Duration,
    cause: &str,
) -> Option<std::process::Output> {
    let owned = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let command = std::iter::once(program.to_owned())
        .chain(owned)
        .collect::<Vec<_>>();
    eprintln!("nacre-helper updates: starting {program} for {cause}");
    let wrapped = if program == "pacman" {
        package_command(&command)
    } else {
        command.clone()
    };
    let (executable, arguments) = wrapped.split_first()?;
    match process::run(executable, arguments, None, timeout) {
        Ok(output) => Some(output),
        Err(error) => {
            eprintln!("nacre-helper updates: {program} for {cause} failed: {error}");
            None
        }
    }
}
fn count_result(command: &str, code: i32, stdout: &str, stderr: &str) -> Result<usize> {
    let allowed = if command == "checkupdates" {
        [0, 2].contains(&code)
    } else {
        [0, 1].contains(&code)
    };
    if !allowed || (code == 1 && !stderr.trim().is_empty()) {
        return Err("Update check failed".into());
    }
    Ok(stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count())
}
fn count_updates() -> Result<Value> {
    let mut commands = Vec::<Vec<String>>::new();
    if available("checkupdates") {
        commands.push(vec!["checkupdates".into()]);
    } else {
        commands.push(vec!["pacman".into(), "-Qu".into()]);
    }
    for name in ["paru", "yay"] {
        if available(name) {
            commands.push(vec![name.into(), "-Qua".into()]);
            break;
        }
    }
    let mut count = 0;
    for cmd in commands {
        if !available(&cmd[0]) {
            continue;
        }
        let out = run_command(&cmd, Duration::from_secs(90), "dashboard update count")?;
        count += count_result(
            &cmd[0],
            out.status.code().unwrap_or(1),
            &String::from_utf8_lossy(&out.stdout),
            &String::from_utf8_lossy(&out.stderr),
        )?;
    }
    Ok(
        json!({"text":count.to_string(),"count":count,"checked":SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64()}),
    )
}
fn pkg_release(s: &str) -> String {
    s.rsplit_once('-').map(|x| x.0).unwrap_or(s).to_owned()
}
pub fn pending_qt_release_change(pending: &str) -> Result<bool> {
    for line in pending.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields
            .first()
            .is_none_or(|s| !["qt6-base", "qt6-declarative", "qt6-wayland"].contains(s))
        {
            continue;
        }
        if fields.len() != 4 || fields[2] != "->" {
            return Err("Could not parse the pending Qt update".into());
        }
        if pkg_release(fields[1]) != pkg_release(fields[3]) {
            return Ok(true);
        }
    }
    Ok(false)
}
fn status(pending: &str) -> Result<Value> {
    let version = bounded_output(
        "quickshell",
        &["--version", "-v"],
        Duration::from_secs(10),
        "checking the active Quickshell build",
    ).ok_or("Could not run the Quickshell version probe")?;
    let local = String::from_utf8_lossy(&version.stdout).contains("Nacre local Qt rebuild");
    let blocked = local && pending_qt_release_change(pending)?;
    let mut message = if blocked {
        "Quickshell rebuild needed before updating"
    } else {
        ""
    }
    .to_owned();
    let candidate = bounded_output(
        "pacman",
        &["-Sp", "--print-format", "%f", "quickshell"],
        Duration::from_secs(10),
        "checking the Quickshell distribution candidate",
    );
    let installed = bounded_output(
        "pacman",
        &["-Q", "qt6-base"],
        Duration::from_secs(10),
        "checking the installed Qt release",
    );
    let qt = installed
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.split_whitespace().last().map(str::to_owned))
        .map(|s| s.rsplit_once('-').map(|x| x.0).unwrap_or(&s).to_owned())
        .unwrap_or_default();
    let archive = candidate
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| {
            s.lines()
                .last()
                .map(|s| PathBuf::from("/var/cache/pacman/pkg").join(s))
        });
    let ready = if local {
        if let Some(a) = archive.filter(|p| p.is_file()) {
            bounded_output(
                "bsdtar",
                &["-xOf", a.to_str().unwrap_or(""), ".BUILDINFO"],
                Duration::from_secs(10),
                "reading cached Quickshell build metadata",
            )
            .filter(|x| x.status.success())
            .is_some_and(|x| {
                String::from_utf8_lossy(&x.stdout)
                    .lines()
                    .any(|l| l.starts_with(&format!("installed = qt6-base-{qt}-")))
            })
        } else {
            false
        }
    } else {
        false
    };
    if ready {
        if !message.is_empty() {
            message.push_str(". ");
        }
        message
            .push_str("A cached distribution Quickshell matches installed Qt; you can switch back");
    }
    Ok(json!({"message":message,"rebuildNeeded":blocked,"distributionReady":ready}))
}
fn preflight() -> Result<Value> {
    let cmd = if available("checkupdates") {
        vec!["checkupdates".into()]
    } else {
        vec!["pacman".into(), "-Qu".into()]
    };
    let out = run_command(
        &cmd,
        Duration::from_secs(90),
        "Quickshell/Qt update preflight",
    )?;
    let code = out.status.code().unwrap_or(1);
    if ![0, 2].contains(&code) && !(cmd[0] == "pacman" && code == 1 && out.stderr.is_empty()) {
        return Err(format!(
            "Update preflight failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )
        .into());
    }
    status(&String::from_utf8_lossy(&out.stdout))
}
fn cache(p: &Paths) -> PathBuf {
    p.cache.join("updates.json")
}
fn refresh(p: &Paths) -> Result<()> {
    fs::create_dir_all(&p.cache)?;
    fs::set_permissions(&p.cache, fs::Permissions::from_mode(0o700))?;
    let target = cache(p);
    if target.exists() && fs::symlink_metadata(&target)?.file_type().is_symlink() {
        return Err("refusing linked update cache".into());
    }
    let lock = target.with_extension("lock");
    if lock.exists() && fs::symlink_metadata(&lock)?.file_type().is_symlink() {
        return Err("refusing linked update cache lock".into());
    }
    let file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .open(lock)?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    let _guard = match nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusiveNonblock) {
        Ok(lock) => lock,
        Err(_) => return Ok(()),
    };
    let mut data = count_updates()?;
    let pre = preflight()?;
    if let (Some(a), Some(b)) = (data.as_object_mut(), pre.as_object()) {
        a.extend(b.clone());
    }
    let tmp = target.with_extension("next");
    if tmp.exists() && fs::symlink_metadata(&tmp)?.file_type().is_symlink() {
        return Err("refusing linked update cache temporary".into());
    }
    if tmp.exists() {
        fs::remove_file(&tmp)?;
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    use std::io::Write;
    file.write_all(serde_json::to_string(&data)?.as_bytes())?;
    file.sync_all()?;
    file.set_permissions(fs::Permissions::from_mode(0o600))?;
    fs::rename(tmp, target)?;
    Ok(())
}
pub fn run(args: &[String]) -> Result<i32> {
    let p = Paths::environment()?;
    if args.iter().any(|a| a == "--needs-host") {
        return Ok(
            if fs::read_to_string("/proc/self/uid_map")
                .ok()
                .is_some_and(|s| needs_host_runtime(&s))
            {
                0
            } else {
                1
            },
        );
    }
    if args.iter().any(|a| a == "--preflight") {
        let d = preflight()?;
        if !d["message"].as_str().unwrap_or("").is_empty() {
            println!("{}", d["message"].as_str().unwrap());
        }
        if d["rebuildNeeded"] == true {
            println!("Prepare a matching Quickshell package together with the pending Qt release; see docs/features/runtime.md");
            return Ok(3);
        }
        return Ok(0);
    }
    if args.iter().any(|a| a == "--refresh") {
        refresh(&p)?;
        return Ok(0);
    }
    let data = fs::read_to_string(cache(&p))
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or(json!({"text":"0","count":0,"checked":0}));
    println!("{}", serde_json::to_string(&data)?);
    if SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64()
        - data["checked"].as_f64().unwrap_or(0.)
        > 1800.
    {
        let exe = std::env::current_exe()?;
        let _ = Command::new(exe)
            .arg("updates")
            .arg("--refresh")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
    Ok(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_mapping_and_qt_release_assertions() {
        assert!(!needs_host_runtime("0 0 4294967295"));
        assert!(needs_host_runtime("1000 1000 1"));
        assert!(pending_qt_release_change("qt6-base 6.12 -> 6.13\n").unwrap());
        assert!(!pending_qt_release_change("qt6-base 6.12.0-2 -> 6.12.0-2.1\n").unwrap());
        assert!(!pending_qt_release_change("fish 4 -> 5\n").unwrap());
        assert!(pending_qt_release_change("qt6-base unknown update format").is_err());
        assert!(pending_qt_release_change("qt6-wayland 6.12.0-2 -> 1:6.12.0-1").unwrap());
    }
    #[test]
    fn count_and_error_status_contracts_are_explicit() {
        assert_eq!(count_result("paru", 1, "", "").unwrap(), 0);
        assert_eq!(count_result("paru", 0, "chrome 1 -> 2\n", "").unwrap(), 1);
        assert!(count_result("paru", 1, "", "error: network failure").is_err());
        assert_eq!(count_result("checkupdates", 2, "", "").unwrap(), 0);
        assert!(pending_qt_release_change("qt6-declarative 6.12.0-1.1 -> 6.12.1-1").unwrap());
    }
    #[test]
    fn upgrade_flow_remains_unchanged() {
        let script = include_str!("../../../nacre/shell-tools/updates.sh");
        assert!(script.contains("paru -Syu --skipreview"));
        assert!(script.contains("flatpak update"));
    }
}
#[cfg(test)]
mod package_command_tests {
    use super::*;
    #[test]
    fn runtime_wrapper_preserves_direct_and_namespaced_argv() {
        let command = vec!["checkupdates".to_owned()];
        assert_eq!(package_command_for(&command, false), command);
        assert_eq!(
            package_command_for(&["checkupdates".to_owned()], true),
            vec![
                "systemd-run",
                "--user",
                "--wait",
                "--pipe",
                "--collect",
                "--quiet",
                "--",
                "checkupdates"
            ]
        );
    }
}
#[cfg(test)]
mod script_contract_tests {
    use std::{fs, os::unix::fs::PermissionsExt, process::Command};

    fn executable(directory: &std::path::Path, name: &str, body: &str) -> crate::Result<()> {
        let path = directory.join(name);
        fs::write(&path, format!("#!/usr/bin/env bash\n{body}\n"))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    #[test]
    fn explicit_upgrade_failure_keeps_step_exit_and_skipreview() -> crate::Result<()> {
        let root = tempfile::tempdir()?;
        executable(
            root.path(),
            "python3",
            "case \"$*\" in *--needs-host) exit 1;; *--preflight) exit 0;; esac",
        )?;
        executable(root.path(), "gum", "exit 0")?;
        executable(root.path(), "paru", "printf '%s\\n' \"$*\"; exit 7")?;
        let output = Command::new("bash")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../nacre/shell-tools/updates.sh"),
            )
            .env("PATH", format!("{}:/usr/bin:/bin", root.path().display()))
            .env("HOME", root.path())
            .stdin(std::process::Stdio::null())
            .output()?;
        assert_eq!(output.status.code(), Some(7));
        assert!(String::from_utf8_lossy(&output.stdout).contains("-Syu --skipreview"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("system package update (exit 7)"));
        Ok(())
    }

    #[test]
    fn namespaced_update_routes_before_confirmation() -> crate::Result<()> {
        let root = tempfile::tempdir()?;
        executable(
            root.path(),
            "python3",
            "case \"$*\" in *--needs-host) exit 0;; esac",
        )?;
        executable(root.path(), "gum", "echo unexpected-confirmation; exit 99")?;
        executable(root.path(), "systemd-run", "printf '%s\\n' \"$*\"; exit 7")?;
        let output = Command::new("bash")
            .arg(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../nacre/shell-tools/updates.sh"),
            )
            .env("PATH", format!("{}:/usr/bin:/bin", root.path().display()))
            .env("HOME", root.path())
            .stdin(std::process::Stdio::null())
            .output()?;
        assert_eq!(output.status.code(), Some(7));
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(stdout.contains("--user --wait --pipe --collect --quiet -- /usr/bin/bash"));
        assert!(!stdout.contains("unexpected-confirmation"));
        assert!(String::from_utf8_lossy(&output.stderr)
            .contains("starting the host update service (exit 7)"));
        Ok(())
    }
}
