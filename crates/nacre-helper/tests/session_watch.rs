//! Immutable Python contracts and explicit private-state security cases.
use nacre_helper::{
    session_watch::{self as session, Event, Events, Observer},
    Result,
};
use serde_json::{json, Value};
use std::{
    cell::RefCell,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::Path,
    rc::Rc,
    time::Duration,
};
fn snapshot(root: &Path) -> Value {
    let mut result = serde_json::Map::new();
    for entry in fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        result.insert(
            entry.file_name().to_string_lossy().into_owned(),
            json!({
            "mode":format!("0o{:o}",entry.metadata().unwrap().permissions().mode() & 0o777),
            "text":fs::read_to_string(entry.path()).unwrap()}),
        );
    }
    Value::Object(result)
}
#[test]
fn frozen_records_and_every_written_file() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/session-watch-reference.json")).unwrap();
    for case in reference["records"].as_array().unwrap() {
        let input = &case["inputs"];
        let record = session::record(
            "fixture",
            reference["clock"].as_str().unwrap(),
            input["desktop"].as_str().unwrap(),
            input["wallet"].as_str().unwrap(),
            &input["monitors"],
            &input["windows"],
            &input["configErrors"],
        )
        .unwrap();
        assert_eq!(record, case["result"], "{}", case["name"]);
        let home = tempfile::tempdir().unwrap();
        let state = home.path().join(".local/state/nacre/session");
        session::publish(&state, &record).unwrap();
        assert_eq!(snapshot(&state), case["files"], "{}", case["name"]);
        assert_eq!(
            fs::metadata(state).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
}
struct Policy {
    states: Vec<Value>,
    elapsed: Duration,
    trace: Rc<RefCell<Vec<Value>>>,
}
impl Observer for Policy {
    fn check(&mut self, event: &str) -> Result<Value> {
        let index = self
            .trace
            .borrow()
            .iter()
            .filter(|v| v[0] == "check")
            .count();
        self.trace.borrow_mut().push(json!(["check", event]));
        let mut record = self.states[index.min(self.states.len() - 1)].clone();
        record["event"] = json!(event);
        Ok(record)
    }
    fn restore(&mut self) -> Result<()> {
        self.trace.borrow_mut().push(json!(["restore"]));
        Ok(())
    }
    fn elapsed(&self) -> Duration {
        self.elapsed
    }
    fn sleep(&mut self, duration: Duration) {
        self.elapsed += duration;
        self.trace
            .borrow_mut()
            .push(json!(["sleep", duration.as_secs_f64()]));
    }
}
#[test]
fn frozen_readiness_policy() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/session-watch-reference.json")).unwrap();
    for case in reference["readiness"].as_array().unwrap() {
        let trace = Rc::new(RefCell::new(Vec::new()));
        let mut policy = Policy {
            states: case["states"].as_array().unwrap().clone(),
            elapsed: Duration::ZERO,
            trace: trace.clone(),
        };
        assert_eq!(
            session::readiness(&mut policy, Duration::from_secs(1)).unwrap(),
            case["result"]
        );
        let events: Vec<_> = trace
            .borrow()
            .iter()
            .filter(|v| v[0] == "check")
            .map(|v| v[1].clone())
            .collect();
        assert_eq!(json!(events), case["events"]);
    }
}
#[test]
fn frozen_saved_display_filter() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/session-watch-policy-reference.json")).unwrap();
    for case in reference["restore"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let data = json!({"displays":case["saved"]});
        let saved = session::connected_displays(
            &json!({"monitorConfiguration":name!="niri"}),
            if name == "missing" { None } else { Some(&data) },
            &if name == "disconnected" {
                json!([])
            } else {
                json!([{"name":"DP-1"}])
            },
        )
        .unwrap();
        let effects = if saved.as_array().unwrap().is_empty() {
            json!([])
        } else {
            json!([saved])
        };
        assert_eq!(effects, case["result"], "{name}");
    }
}
struct FixtureEvents {
    trace: Rc<RefCell<Vec<Value>>>,
    index: usize,
}
impl Events for FixtureEvents {
    fn next(&mut self) -> Result<Event> {
        self.index += 1;
        match self.index {
            1 => Ok(Event::Sleep(true)),
            2 => Ok(Event::Sleep(false)),
            3 => Ok(Event::Outputs),
            _ => Err("Session observer ended".into()),
        }
    }
    fn close(&mut self) -> Result<()> {
        self.trace
            .borrow_mut()
            .extend([json!(["unsubscribe-logind"]), json!(["close-events"])]);
        Ok(())
    }
}
#[test]
fn frozen_ordered_watch_effects() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/session-watch-policy-reference.json")).unwrap();
    let trace = Rc::new(RefCell::new(Vec::new()));
    let mut policy = Policy {
        states: vec![json!({})],
        elapsed: Duration::ZERO,
        trace: trace.clone(),
    };
    let error = session::observe(&mut policy, |_| {
        Ok(FixtureEvents {
            trace: trace.clone(),
            index: 0,
        })
    })
    .unwrap_err();
    trace.borrow_mut().push(json!(["error", error.to_string()]));
    assert_eq!(json!(*trace.borrow()), reference["watch"]);
}
#[test]
fn frozen_cli_stdout_stderr_exit() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/session-watch-cli-reference.json")).unwrap();
    for case in reference["cases"].as_array().unwrap() {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .arg("session-watch")
            .args(
                case["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            case["stdout"],
            "{:?}",
            case["args"]
        );
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            case["stderr"],
            "{:?}",
            case["args"]
        );
        assert_eq!(
            output.status.code(),
            case["exit"].as_i64().map(|v| v as i32)
        );
    }
}
#[test]
fn state_links_are_rejected_and_existing_private_modes_repaired() {
    for name in [
        "latest.next",
        "latest.json",
        "latest.json.schema.lock",
        "events.jsonl",
    ] {
        let home = tempfile::tempdir().unwrap();
        let state = home.path().join("session");
        fs::create_dir(&state).unwrap();
        let target = home.path().join("untouched");
        fs::write(&target, "private").unwrap();
        symlink(&target, state.join(name)).unwrap();
        assert!(session::publish(&state, &json!({})).is_err(), "{name}");
        assert_eq!(fs::read_to_string(target).unwrap(), "private");
    }
    let home = tempfile::tempdir().unwrap();
    let real = home.path().join("real");
    fs::create_dir(&real).unwrap();
    symlink(&real, home.path().join("linked")).unwrap();
    assert!(session::publish(&home.path().join("linked/session"), &json!({})).is_err());
    assert!(!real.join("session").exists());
    let state = home.path().join(".local/state/nacre/session");
    session::publish(&state, &json!({"event":"manual"})).unwrap();
    fs::set_permissions(&state, fs::Permissions::from_mode(0o755)).unwrap();
    for entry in fs::read_dir(&state).unwrap() {
        fs::set_permissions(entry.unwrap().path(), fs::Permissions::from_mode(0o644)).unwrap();
    }
    session::publish(&state, &json!({"event":"manual"})).unwrap();
    assert_eq!(
        fs::metadata(&state).unwrap().permissions().mode() & 0o777,
        0o700
    );
    for entry in fs::read_dir(state).unwrap() {
        assert_eq!(
            entry.unwrap().metadata().unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Wallet;
#[zbus::interface(name = "org.freedesktop.Secret.Collection")]
impl Wallet {
    #[zbus(property)]
    fn locked(&self) -> bool {
        true
    }
}
#[test]
fn private_dbus_sleep_and_socket_hotplug_are_observed_without_process_bridges() {
    use std::{
        io::{BufRead, BufReader, Read, Write},
        os::unix::net::{UnixListener, UnixStream},
        process::{Command, Stdio},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        thread,
        time::Instant,
    };
    let home = tempfile::tempdir().unwrap();
    let mut daemon = ChildGuard(
        Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .arg(format!(
                "--address=unix:path={}",
                home.path().join("bus").display()
            ))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut address = String::new();
    BufReader::new(daemon.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let server = zbus::blocking::connection::Builder::address(address.trim())
        .unwrap()
        .name("org.freedesktop.secrets")
        .unwrap()
        .name("org.freedesktop.login1")
        .unwrap()
        .serve_at("/org/freedesktop/secrets/aliases/default", Wallet)
        .unwrap()
        .build()
        .unwrap();
    let folder = home.path().join("runtime/hypr/fixture");
    fs::create_dir_all(&folder).unwrap();
    let control = UnixListener::bind(folder.join(".socket.sock")).unwrap();
    control.set_nonblocking(true).unwrap();
    let events = UnixListener::bind(folder.join(".socket2.sock")).unwrap();
    events.set_nonblocking(true).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    let peer = thread::spawn(move || {
        while !stopped.load(Ordering::Acquire) {
            let (mut stream, _) = match control.accept() {
                Ok(v) => v,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(2));
                    continue;
                }
                Err(e) => panic!("{e}"),
            };
            let mut text = String::new();
            stream.read_to_string(&mut text).unwrap();
            stream
                .write_all(if text.contains("activewindow") {
                    b"{}"
                } else if text.contains("configerrors") {
                    b""
                } else {
                    b"[]"
                })
                .unwrap();
        }
    });
    let qs = home.path().join(".local/share/nacre/shell/bin/qs");
    fs::create_dir_all(qs.parent().unwrap()).unwrap();
    fs::write(&qs, "#!/bin/sh\nprintf '{\"launcherMode\":\"apps\"}\\n'\n").unwrap();
    fs::set_permissions(&qs, fs::Permissions::from_mode(0o700)).unwrap();
    let mut observer = ChildGuard(
        Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .args(["session-watch", "watch"])
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", "/usr/bin:/bin")
            .env("XDG_CONFIG_HOME", home.path().join(".config"))
            .env("XDG_STATE_HOME", home.path().join(".local/state"))
            .env("XDG_CACHE_HOME", home.path().join(".cache"))
            .env("XDG_DATA_HOME", home.path().join(".local/share"))
            .env("XDG_RUNTIME_DIR", home.path().join("runtime"))
            .env("NACRE_COMPOSITOR", "hyprland")
            .env("HYPRLAND_INSTANCE_SIGNATURE", "fixture")
            .env("DBUS_SYSTEM_BUS_ADDRESS", address.trim())
            .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut event_socket: UnixStream = loop {
        match events.accept() {
            Ok((v, _)) => break v,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(5))
            }
            Err(e) => panic!("Observer subscription: {e}"),
        }
    };
    server
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "PrepareForSleep",
            &(true,),
        )
        .unwrap();
    server
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/login1",
            "org.freedesktop.login1.Manager",
            "PrepareForSleep",
            &(false,),
        )
        .unwrap();
    let log = home.path().join(".local/state/nacre/session/events.jsonl");
    while fs::read_to_string(&log).unwrap_or_default().lines().count() < 3 {
        assert!(Instant::now() < deadline, "Observer did not record resume");
        thread::sleep(Duration::from_millis(10));
    }
    event_socket.write_all(b"monitoradded>>DP-1\n").unwrap();
    loop {
        if fs::read_to_string(&log).unwrap_or_default().lines().count() >= 4 {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "Observer failed to record all events"
        );
        thread::sleep(Duration::from_millis(10));
    }
    drop(event_socket);
    let exit = loop {
        if let Some(status) = observer.0.try_wait().unwrap() {
            break status;
        }
        assert!(Instant::now() < deadline, "Observer cleanup hung");
        thread::sleep(Duration::from_millis(10));
    };
    assert_eq!(exit.code(), Some(1));
    let rows: Vec<Value> = fs::read_to_string(log)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows.iter()
            .map(|row| row["event"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec![
            "observer-start",
            "before-sleep",
            "resume",
            "display-hotplug"
        ]
    );
    assert!(rows
        .iter()
        .all(|row| row["desktopReady"] == true && row["wallet"] == "locked"));
    stop.store(true, Ordering::Release);
    peer.join().unwrap();
}

#[test]
fn custom_absolute_xdg_state_is_an_intended_difference_and_needs_no_python() {
    let home = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["session-watch", "check", "--event", "fixture-xdg"])
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", home.path().join("empty-bin"))
        .env("XDG_STATE_HOME", home.path().join("private-state"))
        .env("XDG_CONFIG_HOME", home.path().join("private-config"))
        .env("XDG_CACHE_HOME", home.path().join("private-cache"))
        .env("XDG_DATA_HOME", home.path().join("private-data"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", home.path().join("absent-session").display()),
        )
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", home.path().join("absent-system").display()),
        )
        .env("NACRE_COMPOSITOR", "niri")
        .env("NIRI_SOCKET", home.path().join("absent-niri"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(home
        .path()
        .join("private-state/nacre/session/latest.json")
        .is_file());
    assert!(!home.path().join(".local/state").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("Session wallet unavailable"));
}

#[test]
fn hard_linked_evidence_is_rejected_without_appending_to_the_target() {
    let home = tempfile::tempdir().unwrap();
    let state = home.path().join("session");
    fs::create_dir(&state).unwrap();
    let target = home.path().join("untouched");
    fs::write(&target, "private").unwrap();
    fs::hard_link(&target, state.join("events.jsonl")).unwrap();
    assert!(session::publish(&state, &json!({})).is_err());
    assert_eq!(fs::read_to_string(target).unwrap(), "private");
}

#[test]
fn frozen_unregistered_state_directory_still_writes_latest_evidence() {
    let reference: Value = serde_json::from_str(include_str!(
        "fixtures/session-watch-unregistered-reference.json"
    ))
    .unwrap();
    let home = tempfile::tempdir().unwrap();
    let state = home.path().join("override");
    session::publish(&state, &reference["record"]).unwrap();
    assert_eq!(snapshot(&state), reference["files"]);
}
