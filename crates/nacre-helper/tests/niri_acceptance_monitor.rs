use nacre_helper::{niri_acceptance_monitor::UNITS, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    borrow::Cow,
    collections::HashMap,
    fs,
    io::{BufRead, BufReader, Read},
    os::unix::fs::PermissionsExt,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Model {
    active: [bool; 5],
    environment: Vec<String>,
}
struct Manager(Arc<Mutex<Model>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn subscribe(&self) {}
    fn get_unit(&self, name: &str) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
        let index = UNITS
            .iter()
            .position(|unit| *unit == name)
            .ok_or_else(|| zbus::fdo::Error::InvalidArgs("unknown fixture unit".into()))?;
        zbus::zvariant::OwnedObjectPath::try_from(format!(
            "/org/freedesktop/systemd1/unit/test{index}"
        ))
        .map_err(|error| zbus::fdo::Error::Failed(error.to_string()))
    }
    #[zbus(property(emits_changed_signal = "false"))]
    fn environment(&self) -> Vec<String> {
        self.0.lock().unwrap().environment.clone()
    }
}
struct Unit {
    model: Arc<Mutex<Model>>,
    index: usize,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
impl Unit {
    #[zbus(property)]
    fn id(&self) -> String {
        UNITS[self.index].into()
    }
    #[zbus(property)]
    fn active_state(&self) -> String {
        if self.model.lock().unwrap().active[self.index] {
            "active"
        } else {
            "inactive"
        }
        .into()
    }
}
struct Service {
    model: Arc<Mutex<Model>>,
    index: usize,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Service")]
impl Service {
    #[zbus(property, name = "MainPID")]
    fn main_pid(&self) -> u32 {
        if self.model.lock().unwrap().active[self.index] {
            42 + self.index as u32
        } else {
            0
        }
    }
}
struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Err(error) = self.0.kill().and_then(|()| self.0.wait().map(|_| ())) {
            if error.kind() != std::io::ErrorKind::InvalidInput {
                eprintln!("Private fixture cleanup failed: {error}");
            }
        }
    }
}
fn wait_record(path: &std::path::Path, predicate: impl Fn(&Value) -> bool) -> Result<Value> {
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        if let Ok(bytes) = fs::read(path) {
            if let Ok(record) = serde_json::from_slice::<Value>(&bytes) {
                if predicate(&record) {
                    return Ok(record);
                }
            }
        }
        if Instant::now() >= deadline {
            return Err("Private observation did not reach its expected state".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
}
fn announce(connection: &zbus::blocking::Connection, index: usize) -> Result<()> {
    let path = format!("/org/freedesktop/systemd1/unit/test{index}");
    let emitter = zbus::object_server::SignalEmitter::new(connection.inner(), path.as_str())?;
    futures_lite::future::block_on(zbus::fdo::Properties::properties_changed(
        &emitter,
        "org.freedesktop.systemd1.Unit".try_into()?,
        HashMap::new(),
        Cow::Borrowed(&["ActiveState"]),
    ))?;
    Ok(())
}
fn fixture_manager(address: &str, model: &Arc<Mutex<Model>>) -> Result<zbus::blocking::Connection> {
    let mut builder = zbus::blocking::connection::Builder::address(address)?
        .name("org.freedesktop.systemd1")?
        .serve_at("/org/freedesktop/systemd1", Manager(model.clone()))?;
    for index in 0..5 {
        let path = format!("/org/freedesktop/systemd1/unit/test{index}");
        builder = builder
            .serve_at(
                path.clone(),
                Unit {
                    model: model.clone(),
                    index,
                },
            )?
            .serve_at(
                path,
                Service {
                    model: model.clone(),
                    index,
                },
            )?;
    }
    Ok(builder.build()?)
}
#[test]
fn frozen_parser_cases_run_without_python_or_external_tools() -> Result<()> {
    let home = tempfile::tempdir()?;
    let reference: Value = serde_json::from_str(include_str!(
        "fixtures/niri-acceptance-monitor-cli-reference.json"
    ))?;
    for case in reference["cases"].as_array().ok_or("Missing CLI cases")? {
        let args: Vec<String> = serde_json::from_value(case["args"].clone())?;
        let output = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .arg("niri-acceptance-monitor")
            .args(args)
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", home.path().join("absent-bin"))
            .output()?;
        assert_eq!(
            output.status.code(),
            case["exit"].as_i64().map(|code| code as i32)
        );
        assert_eq!(String::from_utf8(output.stdout)?, case["stdout"]);
        assert_eq!(String::from_utf8(output.stderr)?, case["stderr"]);
    }
    Ok(())
}
#[test]
fn absent_bus_is_reported_and_custom_xdg_observation_times_out_without_claims() -> Result<()> {
    let home = tempfile::tempdir()?;
    let backup = home.path().join("backup");
    fs::create_dir(&backup)?;
    fs::write(home.path().join("protected"), "unchanged\n")?;
    fs::write(
        backup.join("baseline-hashes.json"),
        serde_json::to_vec(
            &serde_json::json!({"protected":format!("{:x}",Sha256::digest(b"unchanged\n"))}),
        )?,
    )?;
    let state = home.path().join("custom-state/nacre/backups");
    fs::create_dir_all(&state)?;
    fs::write(
        state.join("niri-on-pc-latest"),
        backup.to_string_lossy().as_bytes(),
    )?;
    let runtime = home.path().join("runtime");
    fs::create_dir(&runtime)?;
    let output = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["niri-acceptance-monitor", "--seconds", "10"])
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", home.path().join("absent-bin"))
        .env("XDG_STATE_HOME", home.path().join("custom-state"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}/absent-bus", runtime.display()),
        )
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8(output.stderr)?.contains("Systemd observation connection failed"));
    let record: Value =
        serde_json::from_slice(&fs::read(backup.join("acceptance-observed.json"))?)?;
    assert_eq!(record["compositor"], "none");
    assert_eq!(record["niriSessionObserved"], false);
    assert_eq!(record["returnedHyprland"], false);
    assert_eq!(record["protectedFilesMatch"], true);
    chrono::DateTime::parse_from_rfc3339(
        record["checked"]
            .as_str()
            .ok_or("Missing observation time")?,
    )?;
    assert!(record.get("backendReady").is_none());
    assert!(record.get("unlockVerified").is_none());
    assert!(!home.path().join(".local/state/nacre").exists());
    Ok(())
}
#[test]
fn private_systemd_events_complete_cycle_without_process_bridges_or_readiness_claims() -> Result<()>
{
    let home = tempfile::tempdir()?;
    let runtime = home.path().join("runtime");
    fs::create_dir(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    let address = format!("unix:path={}/bus", runtime.display());
    let configuration = home.path().join("bus.conf");
    fs::write(&configuration,format!("<busconfig><type>session</type><listen>{address}</listen><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>"))?;
    let mut daemon = ChildGuard(
        Command::new("dbus-daemon")
            .args(["--nofork", "--print-address=1", "--nosyslog"])
            .arg(format!("--config-file={}", configuration.display()))
            .env_clear()
            .env("HOME", home.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut announced = String::new();
    BufReader::new(daemon.0.stdout.take().ok_or("Missing private bus output")?)
        .read_line(&mut announced)?;
    assert!(!announced.trim().is_empty());
    let model = Arc::new(Mutex::new(Model {
        active: [true, false, true, false, false],
        environment: vec![
            "XDG_CURRENT_DESKTOP=Hyprland".into(),
            "WAYLAND_DISPLAY=private-display".into(),
            "UNRELATED_PRIVATE=not persisted".into(),
        ],
    }));
    let mut connection = fixture_manager(&address, &model)?;
    let backup = home.path().join("backup");
    fs::create_dir(&backup)?;
    fs::write(home.path().join("protected"), "unchanged\n")?;
    fs::write(
        backup.join("baseline-hashes.json"),
        serde_json::to_vec(
            &serde_json::json!({"protected":format!("{:x}",Sha256::digest(b"unchanged\n"))}),
        )?,
    )?;
    let state = home.path().join(".local/state/nacre/backups");
    fs::create_dir_all(&state)?;
    fs::write(
        state.join("niri-on-pc-latest"),
        backup.to_string_lossy().as_bytes(),
    )?;
    let child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["niri-acceptance-monitor", "--seconds", "20"])
        .env_clear()
        .env("HOME", home.path())
        .env("DBUS_SESSION_BUS_ADDRESS", &address)
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}/absent-system", runtime.display()),
        )
        .env("PATH", home.path().join("absent-bin"))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut child = ChildGuard(child);
    let record = backup.join("acceptance-observed.json");
    wait_record(&record, |value| value["compositor"] == "hyprland")?;
    {
        let mut state = model.lock().unwrap();
        state.active = [false, true, false, false, false];
    }
    announce(&connection, 1)?;
    wait_record(&record, |value| {
        value["compositor"] == "niri" && value["desktopIsNiri"] == false
    })?;
    {
        let mut state = model.lock().unwrap();
        state.environment = vec![
            "XDG_CURRENT_DESKTOP=niri".into(),
            "WAYLAND_DISPLAY=private-display".into(),
            "NIRI_SOCKET=private-socket".into(),
        ];
    }
    // Environment explicitly emits no signal: the bounded refresh must see it.
    wait_record(&record, |value| value["niriSessionObserved"] == true)?;
    connection.release_name("org.freedesktop.systemd1")?;
    wait_record(&record, |value| value["compositor"] == "none")?;
    connection.request_name("org.freedesktop.systemd1")?;
    wait_record(&record, |value| value["compositor"] == "niri")?;
    daemon.0.kill()?;
    daemon.0.wait()?;
    drop(connection);
    wait_record(&record, |value| value["compositor"] == "none")?;
    match fs::remove_file(runtime.join("bus")) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut replacement = ChildGuard(
        Command::new("dbus-daemon")
            .args(["--nofork", "--print-address=1", "--nosyslog"])
            .arg(format!("--config-file={}", configuration.display()))
            .env_clear()
            .env("HOME", home.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut address_line = String::new();
    BufReader::new(
        replacement
            .0
            .stdout
            .take()
            .ok_or("Missing restarted bus output")?,
    )
    .read_line(&mut address_line)?;
    assert!(!address_line.trim().is_empty());
    connection = fixture_manager(&address, &model)?;
    wait_record(&record, |value| value["compositor"] == "niri")?;

    {
        let mut state = model.lock().unwrap();
        state.active = [true, false, true, false, false];
        state.environment = vec![
            "XDG_CURRENT_DESKTOP=Hyprland".into(),
            "WAYLAND_DISPLAY=private-display".into(),
        ];
    }
    announce(&connection, 0)?;
    let final_record = wait_record(&record, |value| value["returnedHyprland"] == true)?;
    let status = child.0.wait()?;
    let mut stdout = Vec::new();
    child
        .0
        .stdout
        .take()
        .ok_or("Missing observer output")?
        .read_to_end(&mut stdout)?;
    let mut stderr = Vec::new();
    child
        .0
        .stderr
        .take()
        .ok_or("Missing observer errors")?
        .read_to_end(&mut stderr)?;
    assert!(status.success(), "{}", String::from_utf8_lossy(&stderr));
    assert!(stdout.is_empty());
    assert!(String::from_utf8_lossy(&stderr).contains("reconnect"));
    assert_eq!(final_record["plainNiriIsolated"], true);
    assert_eq!(final_record["protectedFilesMatch"], true);
    for name in ["acceptance-observed.json", "acceptance-events.jsonl"] {
        let path = backup.join(name);
        let text = fs::read_to_string(&path)?;
        for forbidden in [
            "private-socket",
            "private-display",
            "not persisted",
            "unlockVerified",
            "backendReady",
        ] {
            assert!(!text.contains(forbidden));
        }
        assert_eq!(fs::metadata(path)?.permissions().mode() & 0o777, 0o600);
    }
    Ok(())
}
