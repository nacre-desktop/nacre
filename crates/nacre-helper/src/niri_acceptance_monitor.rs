//! Private observation records; never grants backend readiness or password unlock.
use crate::{json, Result};
use serde_json::{json as value, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Component, Path},
};
pub const UNITS: &[&str] = &[
    "wayland-wm@hyprland.desktop.service",
    "wayland-wm@niri.desktop.service",
    "nacre-shell.service",
    "nacre-session-watch.service",
    "hypridle.service",
];

pub fn snapshot_records(records: &[Value], environment: &[String]) -> Result<Value> {
    let mut units = serde_json::Map::new();
    for record in records {
        let Some(id) = record
            .get("Id")
            .and_then(Value::as_str)
            .filter(|id| UNITS.contains(id))
        else {
            continue;
        };
        let active = record.get("ActiveState").and_then(Value::as_str) == Some("active")
            && record.get("MainPID").and_then(Value::as_str).unwrap_or("0") != "0";
        units.insert(id.into(), value!(active));
    }
    let mut desktop = "";
    let mut socket = false;
    let mut display = false;
    for entry in environment {
        let Some((name, text)) = entry.split_once('=') else {
            continue;
        };
        match name {
            "XDG_CURRENT_DESKTOP" => desktop = text,
            "NIRI_SOCKET" => socket = !text.is_empty(),
            "WAYLAND_DISPLAY" => display = !text.is_empty(),
            _ => {}
        }
    }
    let niri = units.get(UNITS[1]) == Some(&Value::Bool(true));
    let hyprland = units.get(UNITS[0]) == Some(&Value::Bool(true));
    let compositor = match (niri, hyprland) {
        (true, false) => "niri",
        (false, true) => "hyprland",
        (false, false) => "none",
        (true, true) => "ambiguous",
    };
    let desktop = desktop.to_lowercase();
    Ok(
        value!({"compositor":compositor,"units":units,"niriSocketPresent":socket,"waylandDisplayPresent":display,"desktopIsNiri":desktop.split(':').any(|part|part=="niri"),"desktopIsHyprland":desktop.split(':').any(|part|part=="hyprland")}),
    )
}

#[derive(Default)]
pub struct Cycle {
    pub saw_niri: bool,
    pub plain_niri_isolated: bool,
    pub returned_hyprland: bool,
    isolation_violation: bool,
}
impl Cycle {
    pub fn observe(&mut self, sample: &Value) -> Value {
        if sample["compositor"] == "niri"
            && sample["niriSocketPresent"] == true
            && sample["desktopIsNiri"] == true
        {
            self.saw_niri = true;
            self.isolation_violation |=
                UNITS[2..].iter().any(|name| sample["units"][*name] == true);
            self.plain_niri_isolated = !self.isolation_violation;
        }
        if self.saw_niri
            && sample["compositor"] == "hyprland"
            && sample["desktopIsHyprland"] == true
            && sample["niriSocketPresent"] == false
            && sample["units"]["nacre-shell.service"] == true
        {
            self.returned_hyprland = true;
        }
        value!({"niriSessionObserved":self.saw_niri,"plainNiriIsolated":self.plain_niri_isolated,"returnedHyprland":self.returned_hyprland})
    }
}
pub fn protected_files_match(backup: &Path, home: &Path) -> Result<bool> {
    let original: Value = serde_json::from_slice(&fs::read(backup.join("baseline-hashes.json"))?)?;
    let Some(hashes) = original.as_object().filter(|hashes| !hashes.is_empty()) else {
        return Ok(false);
    };
    for (name, expected) in hashes {
        let path = Path::new(name);
        if path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
            return Ok(false);
        }
        let actual = format!("{:x}", Sha256::digest(fs::read(home.join(path))?));
        if expected.as_str() != Some(&actual) {
            return Ok(false);
        }
    }
    Ok(true)
}
pub fn write(path: &Path, data: &Value) -> Result<()> {
    let temporary = path.with_extension("next");
    fs::write(&temporary, json::pretty(data)?)?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
    fs::rename(temporary, path)?;
    Ok(())
}

#[derive(Default)]
pub struct Observation {
    pub cycle: Cycle,
    previous: Option<Value>,
}
fn checked_files(backup: &Path, home: &Path) -> bool {
    match protected_files_match(backup, home) {
        Ok(matches) => matches,
        Err(error) => {
            eprintln!("Protected-file verification failed: {error}");
            false
        }
    }
}
impl Observation {
    pub fn sample(
        &mut self,
        backup: &Path,
        home: &Path,
        sample: Value,
        checked: &str,
    ) -> Result<Option<i32>> {
        let result = self.cycle.observe(&sample);
        let mut current = sample;
        current
            .as_object_mut()
            .ok_or("Invalid observation snapshot")?
            .extend(result.as_object().ok_or("Invalid cycle result")?.clone());
        if self.previous.as_ref() != Some(&current) {
            let previous = current.clone();
            current["protectedFilesMatch"] = value!(checked_files(backup, home));
            current["checked"] = value!(checked);
            write(&backup.join("acceptance-observed.json"), &current)?;
            use std::io::Write;
            let events = backup.join("acceptance-events.jsonl");
            let mut stream = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&events)?;
            writeln!(stream, "{}", json::encode(&current)?)?;
            fs::set_permissions(events, fs::Permissions::from_mode(0o600))?;
            self.previous = Some(previous);
        }
        if self.cycle.returned_hyprland {
            return Ok(Some(
                if self.cycle.plain_niri_isolated && checked_files(backup, home) {
                    0
                } else {
                    1
                },
            ));
        }
        Ok(None)
    }
}

pub fn arguments(args: &[String]) -> std::result::Result<Option<u64>, String> {
    let error = |message: &str| {
        format!("usage: niri-acceptance-monitor.py [-h] [--seconds SECONDS]\nniri-acceptance-monitor.py: error: {message}\n")
    };
    let mut seconds = 3600u64;
    let mut index = 0;
    let mut unknown = Vec::new();
    while index < args.len() {
        let arg = &args[index];
        if arg == "-h" || arg.starts_with("--") && arg.len() > 2 && "--help".starts_with(arg) {
            return Ok(None);
        }
        let (option, inline) = arg
            .split_once('=')
            .map_or((arg.as_str(), None), |(option, value)| {
                (option, Some(value))
            });
        if option.starts_with("--") && option.len() > 2 && "--seconds".starts_with(option) {
            let text = if let Some(text) = inline {
                text
            } else {
                index += 1;
                args.get(index)
                    .filter(|arg| !arg.starts_with("--"))
                    .ok_or_else(|| error("argument --seconds: expected one argument"))?
            };
            let number = text
                .parse::<i64>()
                .map_err(|_| error(&format!("argument --seconds: invalid int value: '{text}'")))?;
            if !(10..=7200).contains(&number) {
                return Err(error(
                    "Observation duration must be between 10 and 7200 seconds",
                ));
            }
            seconds = number as u64;
        } else {
            unknown.push(arg.clone());
        }
        index += 1;
    }
    if !unknown.is_empty() {
        return Err(error(&format!(
            "unrecognized arguments: {}",
            unknown.join(" ")
        )));
    }
    Ok(Some(seconds))
}
fn timestamp() -> String {
    let now = chrono::Utc::now();
    now.to_rfc3339_opts(
        if now.timestamp_subsec_micros() == 0 {
            chrono::SecondsFormat::Secs
        } else {
            chrono::SecondsFormat::Micros
        },
        false,
    )
}
pub fn run(args: &[String]) -> Result<i32> {
    let seconds = match arguments(args) {
        Ok(None) => {
            print!(
                "{}",
                include_str!("../assets/niri-acceptance-monitor-help.txt")
            );
            return Ok(0);
        }
        Ok(Some(seconds)) => seconds,
        Err(error) => {
            eprint!("{error}");
            return Ok(2);
        }
    };
    let paths = crate::paths::Paths::environment()?;
    let backup = std::path::PathBuf::from(
        fs::read_to_string(paths.state.join("backups/niri-on-pc-latest"))?.trim(),
    );
    futures_lite::future::block_on(observe(&paths, &backup, seconds))
}
async fn observe(paths: &crate::paths::Paths, backup: &Path, seconds: u64) -> Result<i32> {
    use std::time::{Duration, Instant};
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut observation = Observation::default();
    let mut previous_error = None;
    loop {
        if Instant::now() >= deadline {
            return Ok(2);
        }
        let bus_change = bus_change_watch()?;
        match crate::niri_acceptance_systemd::Watch::connect().await {
            Ok(mut watch) => {
                previous_error = None;
                loop {
                    if let Some(code) =
                        observation.sample(backup, &paths.home, watch.snapshot()?, &timestamp())?
                    {
                        return Ok(code);
                    }
                    match watch.next(deadline).await {
                        Ok(crate::niri_acceptance_systemd::Update::Timeout) => return Ok(2),
                        Ok(crate::niri_acceptance_systemd::Update::Changed) => {}
                        Ok(crate::niri_acceptance_systemd::Update::Disconnected) => {
                            eprintln!("Systemd observation bus disconnected; awaiting reconnect");
                            break;
                        }
                        Err(error) => {
                            eprintln!("Systemd observation failed; awaiting reconnect: {error}");
                            break;
                        }
                    }
                }
            }
            Err(error) => {
                let message = error.to_string();
                if previous_error.as_ref() != Some(&message) {
                    eprintln!("Systemd observation connection failed: {message}");
                    previous_error = Some(message);
                }
            }
        }
        let empty = snapshot_records(&[], &[])?;
        observation.sample(backup, &paths.home, empty, &timestamp())?;
        // Socket creation is observable; listening readiness and abstract socket
        // recovery have no filesystem event, so retain bounded failure backoff.
        let retry = (Instant::now() + Duration::from_secs(2)).min(deadline);
        if let Some(watch) = bus_change {
            let mut bytes = [0u8; 4096];
            let changed = async {
                watch
                    .read_with(|fd| rustix::io::read(fd, &mut bytes).map_err(std::io::Error::from))
                    .await
                    .map(|_| ())
            };
            let timeout = async {
                async_io::Timer::at(retry).await;
                Ok(())
            };
            futures_lite::future::race(changed, timeout).await?;
        } else {
            async_io::Timer::at(retry).await;
        }
    }
}
fn bus_change_watch() -> Result<Option<async_io::Async<rustix::fd::OwnedFd>>> {
    let Ok(address) = std::env::var("DBUS_SESSION_BUS_ADDRESS") else {
        return Ok(None);
    };
    let address: zbus::Address = address.parse()?;
    let zbus::address::Transport::Unix(unix) = address.transport() else {
        return Ok(None);
    };
    let zbus::address::transport::UnixSocket::File(path) = unix.path() else {
        return Ok(None);
    };
    let Some(parent) = path.ancestors().skip(1).find(|parent| parent.is_dir()) else {
        return Ok(None);
    };
    let watch = rustix::fs::inotify::init(
        rustix::fs::inotify::CreateFlags::NONBLOCK | rustix::fs::inotify::CreateFlags::CLOEXEC,
    )?;
    rustix::fs::inotify::add_watch(
        &watch,
        parent,
        rustix::fs::inotify::WatchFlags::CREATE
            | rustix::fs::inotify::WatchFlags::DELETE
            | rustix::fs::inotify::WatchFlags::MOVED_TO
            | rustix::fs::inotify::WatchFlags::MOVED_FROM
            | rustix::fs::inotify::WatchFlags::DELETE_SELF
            | rustix::fs::inotify::WatchFlags::MOVE_SELF,
    )?;
    Ok(Some(async_io::Async::new(watch)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_cli_help_range_and_parse_errors() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/niri-acceptance-monitor-cli-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing CLI cases")? {
            let args: Vec<String> = serde_json::from_value(case["args"].clone())?;
            match arguments(&args) {
                Ok(None) => {
                    assert_eq!(case["exit"], 0);
                    assert_eq!(
                        case["stdout"],
                        include_str!("../assets/niri-acceptance-monitor-help.txt")
                    );
                }
                Err(error) => {
                    assert_eq!(case["exit"], 2);
                    assert_eq!(error, case["stderr"]);
                }
                Ok(Some(_)) => panic!("Expected terminal CLI outcome"),
            }
        }
        Ok(())
    }
    fn reference() -> Result<Value> {
        Ok(serde_json::from_str(include_str!(
            "../tests/fixtures/niri-acceptance-monitor-reference.json"
        ))?)
    }
    #[test]
    fn frozen_activation_projection_excludes_private_values() -> Result<()> {
        for case in reference()?["cases"]
            .as_array()
            .ok_or("Missing cases")?
            .iter()
            .filter(|case| case["operation"] == "snapshot")
        {
            let environment = case["environment"]
                .as_object()
                .ok_or("Missing environment")?
                .iter()
                .map(|(name, text)| {
                    Ok(format!(
                        "{name}={}",
                        text.as_str().ok_or("Invalid activation")?
                    ))
                })
                .collect::<Result<Vec<_>>>()?;
            let result = snapshot_records(
                case["units"].as_array().ok_or("Missing records")?,
                &environment,
            )?;
            assert_eq!(result, case["observed"]);
            assert!(!json::encode(&result)?.contains("never serialize"));
        }
        Ok(())
    }
    #[test]
    fn frozen_cycle_latches_isolation_failure_and_requires_clean_return() -> Result<()> {
        for case in reference()?["cases"]
            .as_array()
            .ok_or("Missing cases")?
            .iter()
            .filter(|case| case["operation"] == "cycle")
        {
            let mut cycle = Cycle::default();
            let results: Vec<_> = case["samples"]
                .as_array()
                .ok_or("Missing samples")?
                .iter()
                .map(|sample| cycle.observe(sample))
                .collect();
            assert_eq!(value!(results), case["observed"]);
            assert!(!json::encode(&results)?.contains("unlockVerified"));
        }
        Ok(())
    }
    #[test]
    fn frozen_protected_hashes_never_accept_escape_or_changed_files() -> Result<()> {
        for case in reference()?["cases"]
            .as_array()
            .ok_or("Missing cases")?
            .iter()
            .filter(|case| case["operation"] == "protected_files_match")
        {
            let home = tempfile::tempdir()?;
            let backup = home.path().join("backup");
            fs::create_dir(&backup)?;
            fs::write(
                home.path().join("protected"),
                if case["variant"] == "changed" {
                    "changed\n"
                } else {
                    "original\n"
                },
            )?;
            fs::write(
                backup.join("baseline-hashes.json"),
                case["manifest"]
                    .as_str()
                    .ok_or("Missing hashes")?
                    .replace("$HOME", home.path().to_str().ok_or("Invalid home")?),
            )?;
            let result = match protected_files_match(&backup, home.path()) {
                Ok(result) => result,
                Err(error) => {
                    eprintln!("Protected-file verification failed: {error}");
                    false
                }
            };
            assert_eq!(value!(result), case["observed"]);
        }
        Ok(())
    }
    #[test]
    fn frozen_observation_files_modes_privacy_and_exit_decisions() -> Result<()> {
        for case in reference()?["cases"]
            .as_array()
            .ok_or("Missing cases")?
            .iter()
            .filter(|case| case["operation"] == "run")
        {
            let home = tempfile::tempdir()?;
            let backup = home.path().join("backup");
            fs::create_dir(&backup)?;
            fs::write(
                home.path().join("protected"),
                if case["variant"] == "changed-file" {
                    "changed\n"
                } else {
                    "original\n"
                },
            )?;
            fs::write(
                backup.join("baseline-hashes.json"),
                json::encode(&value!({"protected":format!("{:x}",Sha256::digest(b"original\n"))}))?,
            )?;
            let samples = case["samples"].as_array().ok_or("Missing samples")?;
            let mut observation = Observation::default();
            let mut exit = 2;
            for index in 0..5 {
                if let Some(code) = observation.sample(
                    &backup,
                    home.path(),
                    samples[index.min(samples.len() - 1)].clone(),
                    case["checked"].as_str().ok_or("Missing clock")?,
                )? {
                    exit = code;
                    break;
                }
            }
            assert_eq!(value!(exit), case["observed"]["exit"]);
            let mut files = serde_json::Map::new();
            for name in ["acceptance-observed.json", "acceptance-events.jsonl"] {
                let path = backup.join(name);
                let text = fs::read_to_string(&path)?;
                assert!(!text.contains("unlockVerified"));
                assert!(!text.contains("private-socket"));
                files.insert(
                    name.into(),
                    value!({"text":text,"mode":fs::metadata(path)?.permissions().mode() & 0o777}),
                );
            }
            assert_eq!(
                Value::Object(files),
                case["observed"]["files"],
                "{}",
                case["variant"]
            );
            assert!(!backup.join("acceptance-observed.next").exists());
        }
        Ok(())
    }
}
