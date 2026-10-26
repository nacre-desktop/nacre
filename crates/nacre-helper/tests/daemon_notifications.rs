//! Managed notification ownership and restart behavior on a private session bus.
use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use wait_timeout::ChildExt;
use zbus::blocking::{Connection, Proxy};

const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
const NOTIFICATION_PATH: &str = "/org/freedesktop/Notifications";
const DBUS: &str = "org.freedesktop.DBus";

struct NiriSource {
    stop: Arc<AtomicBool>,
    sockets: Arc<Mutex<Vec<UnixStream>>>,
    accept: Option<thread::JoinHandle<()>>,
    workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
}

impl NiriSource {
    fn new(path: &Path) -> std::io::Result<Self> {
        let listener = UnixListener::bind(path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let stop = Arc::new(AtomicBool::new(false));
        let sockets = Arc::new(Mutex::new(Vec::new()));
        let workers = Arc::new(Mutex::new(Vec::new()));
        let (done, active, tasks) = (stop.clone(), sockets.clone(), workers.clone());
        let accept = thread::spawn(move || {
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if let Ok(copy) = stream.try_clone() {
                            active.lock().unwrap().push(copy);
                        }
                        let done = done.clone();
                        tasks.lock().unwrap().push(thread::spawn(move || {
                        let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                        let mut reader = BufReader::new(stream);
                        let mut line = String::new();
                        while !done.load(Ordering::Relaxed) {
                            match reader.read_line(&mut line) {
                                Ok(0) => break,
                                Ok(_) => {
                                    let request = serde_json::from_str::<Value>(&line).ok();
                                    line.clear();
                                    let response = if request == Some(Value::String("EventStream".into())) {
                                        let _ = writeln!(reader.get_mut(), r#"{{"Ok":"Handled"}}"#);
                                        for event in [
                                            serde_json::json!({"WorkspacesChanged":{"workspaces":[]}}),
                                            serde_json::json!({"WindowsChanged":{"windows":[]}}),
                                            serde_json::json!({"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":[],"current_idx":0}}}),
                                        ] { if writeln!(reader.get_mut(), "{event}").is_err() { break; } }
                                        continue;
                                    } else if request == Some(Value::String("Outputs".into())) {
                                        serde_json::json!({"Ok":{"Outputs":{}}})
                                    } else if request == Some(Value::String("Workspaces".into())) {
                                        serde_json::json!({"Ok":{"Workspaces":[]}})
                                    } else if request == Some(Value::String("Windows".into())) {
                                        serde_json::json!({"Ok":{"Windows":[]}})
                                    } else {
                                        serde_json::json!({"Err":"Unsupported fixture request"})
                                    };
                                    if writeln!(reader.get_mut(), "{response}").is_err() { break; }
                                }
                                Err(error) if matches!(error.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut | std::io::ErrorKind::Interrupted) => continue,
                                Err(_) => break,
                            }
                        }
                    }));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(10))
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(Self {
            stop,
            sockets,
            accept: Some(accept),
            workers,
        })
    }
}

impl Drop for NiriSource {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for stream in self.sockets.lock().unwrap().iter() {
            let _ = stream.shutdown(std::net::Shutdown::Both);
        }
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
        for worker in self.workers.lock().unwrap().drain(..) {
            let _ = worker.join();
        }
    }
}

struct OwnedChild(Child);

impl OwnedChild {
    fn stop(&mut self) -> ExitStatus {
        if self.0.try_wait().ok().flatten().is_some() {
            return self.0.wait().expect("collect child exit status");
        }
        let pid = nix::unistd::Pid::from_raw(self.0.id() as i32);
        let _ = nix::sys::signal::kill(pid, nix::sys::signal::Signal::SIGTERM);
        if let Some(status) = self.0.wait_timeout(Duration::from_secs(10)).ok().flatten() {
            return status;
        }
        // This is the exact child PID started by this fixture.
        let _ = self.0.kill();
        self.0.wait().expect("collect timed-out child status")
    }
}

impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn daemon(home: &Path, endpoint: &Path, managed: bool, client_opt_in: bool) -> OwnedChild {
    daemon_with_tools(
        home,
        endpoint,
        managed,
        client_opt_in,
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
    )
}

fn daemon_with_tools(
    home: &Path,
    endpoint: &Path,
    managed: bool,
    client_opt_in: bool,
    tools: &Path,
) -> OwnedChild {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env("NACRE_HELPER_TOOLS", tools)
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_RUNTIME_DIR", home.join("runtime"))
        .env("NACRE_DAEMON_SOCKET", endpoint)
        .env("NACRE_COMPOSITOR", "niri")
        .env("NIRI_SOCKET", home.join("runtime/niri.sock"))
        .env("NACRE_NIRI_HOVER_STREAM", "0");
    if let Some(address) = std::env::var_os("DBUS_SESSION_BUS_ADDRESS") {
        command.env("DBUS_SESSION_BUS_ADDRESS", address);
    }
    command.env(
        "DBUS_SYSTEM_BUS_ADDRESS",
        "unix:path=/nacre-test-no-system-bus",
    );
    if client_opt_in {
        command.env("NACRE_DAEMON_CLIENT", "1");
    }
    command.arg("daemon");
    if managed {
        command.arg("--notifications=daemon");
    }
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        // In dbus-run-session this reaches the captured child-test stderr,
        // preserving startup and ownership diagnostics on fixture failure.
        .stderr(Stdio::inherit());
    OwnedChild(command.spawn().expect("start isolated Nacre daemon"))
}

fn native_proxy(connection: &Connection) -> Proxy<'_> {
    Proxy::new(connection, NOTIFICATIONS, NOTIFICATION_PATH, NOTIFICATIONS)
        .expect("notification proxy")
}

fn has_owner(connection: &Connection) -> bool {
    Proxy::new(connection, DBUS, "/org/freedesktop/DBus", DBUS)
        .expect("D-Bus control proxy")
        .call("NameHasOwner", &(NOTIFICATIONS,))
        .expect("query private bus owner")
}

fn wait_owner(connection: &Connection, expected: bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if has_owner(connection) == expected {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "notification owner state timed out"
        );
        thread::sleep(Duration::from_millis(20));
    }
}

fn directories(home: &Path) -> PathBuf {
    fs::set_permissions(home, fs::Permissions::from_mode(0o700)).unwrap();
    for name in ["config", "state", "cache", "data", "runtime"] {
        let path = home.join(name);
        fs::create_dir(&path).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700)).unwrap();
    }
    let socket_dir = home.join("runtime/nacre");
    fs::create_dir(&socket_dir).unwrap();
    fs::set_permissions(&socket_dir, fs::Permissions::from_mode(0o700)).unwrap();
    socket_dir.join("daemon.sock")
}

fn notifications_snapshot(home: &Path) -> Value {
    let endpoint = home.join("runtime/nacre/daemon.sock");
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env(
            "NACRE_HELPER_TOOLS",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
        )
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_RUNTIME_DIR", home.join("runtime"))
        .env("NACRE_DAEMON_SOCKET", endpoint)
        .arg("ctl")
        .arg("watch")
        .arg("notifications")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    if let Some(address) = std::env::var_os("DBUS_SESSION_BUS_ADDRESS") {
        command.env("DBUS_SESSION_BUS_ADDRESS", address);
    }
    let mut child = OwnedChild(
        command
            .spawn()
            .expect("start isolated notifications observer"),
    );
    let stdout = child.0.stdout.take().expect("observer stdout");
    let (send, receive) = mpsc::channel();
    thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if line.contains("\"t\":\"snap\"") {
                let _ = send.send(line);
                break;
            }
        }
    });
    let line = receive
        .recv_timeout(Duration::from_secs(8))
        .expect("notification snapshot deadline");
    child.stop();
    serde_json::from_str(&line).expect("valid notification snapshot")
}

fn call_dnd(home: &Path, enabled: bool) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env(
            "NACRE_HELPER_TOOLS",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
        )
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_RUNTIME_DIR", home.join("runtime"))
        .env(
            "NACRE_DAEMON_SOCKET",
            home.join("runtime/nacre/daemon.sock"),
        );
    if let Some(address) = std::env::var_os("DBUS_SESSION_BUS_ADDRESS") {
        command.env("DBUS_SESSION_BUS_ADDRESS", address);
    }
    let output = command
        .args([
            "ctl",
            "call",
            "notifications.set_dnd",
            if enabled {
                r#"{"enabled":true}"#
            } else {
                r#"{"enabled":false}"#
            },
        ])
        .output()
        .expect("set DND through daemon settings writer");
    assert!(
        output.status.success(),
        "DND call failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).expect("DND reply JSON")
}

fn advertised_methods(home: &Path) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env(
            "NACRE_HELPER_TOOLS",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
        )
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_RUNTIME_DIR", home.join("runtime"))
        .env(
            "NACRE_DAEMON_SOCKET",
            home.join("runtime/nacre/daemon.sock"),
        );
    let output = command
        .args(["ctl", "call", "--list"])
        .output()
        .expect("list daemon methods");
    assert!(output.status.success(), "method list failed");
    serde_json::from_slice(&output.stdout).expect("method list JSON")
}

fn legacy_history_save(home: &Path) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("HOME", home)
        .env(
            "NACRE_HELPER_TOOLS",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
        )
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_RUNTIME_DIR", home.join("runtime"))
        .arg("notification-history")
        .arg("save")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("start legacy notification history writer");
    use std::io::Write;
    child
        .stdin
        .take()
        .expect("history writer stdin")
        .write_all(b"[]\n")
        .expect("send empty history");
    let output = child
        .wait_with_output()
        .expect("wait for legacy history writer");
    assert!(
        !output.status.success(),
        "legacy notification-history save bypassed the daemon writer lease"
    );
}

#[test]
fn managed_notifications_private_bus() {
    if std::env::var_os("NACRE_NOTIFICATION_TEST_CHILD").is_some() {
        private_bus_scenario();
        return;
    }

    let home = tempfile::tempdir().expect("private test HOME");
    let endpoint = directories(home.path());
    let output = Command::new("dbus-run-session")
        .args([
            "--",
            std::env::current_exe()
                .expect("test executable")
                .to_str()
                .expect("UTF-8 test executable"),
            "--exact",
            "managed_notifications_private_bus",
            "--nocapture",
        ])
        .env("NACRE_NOTIFICATION_TEST_CHILD", "1")
        .env("NACRE_NOTIFICATION_TEST_HOME", home.path())
        .env("NACRE_NOTIFICATION_TEST_SOCKET", endpoint)
        .output()
        .expect("run fixture on a private D-Bus session");
    assert!(
        output.status.success(),
        "private-bus child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn private_bus_scenario() {
    let home = PathBuf::from(std::env::var_os("NACRE_NOTIFICATION_TEST_HOME").expect("test HOME"));
    let endpoint =
        PathBuf::from(std::env::var_os("NACRE_NOTIFICATION_TEST_SOCKET").expect("test socket"));
    let _niri = NiriSource::new(&home.join("runtime/niri.sock"))
        .expect("start isolated Niri socket fixture");
    let connection = Connection::session().expect("connect to dbus-run-session bus");

    // NACRE_DAEMON_CLIENT is transport policy only; it must not enable ownership.
    let mut ordinary = daemon(&home, &endpoint, false, true);
    let deadline = Instant::now() + Duration::from_secs(8);
    while !endpoint.exists() {
        assert!(
            Instant::now() < deadline,
            "default daemon socket did not appear"
        );
        thread::sleep(Duration::from_millis(20));
    }
    wait_owner(&connection, false);
    assert!(advertised_methods(&home)
        .as_object()
        .is_some_and(|methods| {
            !methods
                .keys()
                .any(|method| method.starts_with("notifications."))
        }));
    assert!(
        ordinary.stop().success(),
        "default daemon did not stop cleanly"
    );

    // A pre-existing owner makes managed startup refuse ownership. It must not
    // replace or queue behind that owner.
    let dbus =
        Proxy::new(&connection, DBUS, "/org/freedesktop/DBus", DBUS).expect("D-Bus control proxy");
    let acquired: u32 = dbus
        .call("RequestName", &(NOTIFICATIONS, 0u32))
        .expect("claim private notification name");
    assert_eq!(acquired, 1, "fixture must become the primary owner");
    let mut refused = daemon(&home, &endpoint, true, false);
    thread::sleep(Duration::from_millis(500));
    assert!(
        has_owner(&connection),
        "managed daemon replaced fixture owner"
    );
    let current_owner: String = dbus
        .call("GetNameOwner", &(NOTIFICATIONS,))
        .expect("read fixture owner");
    assert_eq!(
        current_owner,
        connection
            .unique_name()
            .expect("fixture bus connection has a unique name")
            .as_str()
    );
    assert!(
        refused.stop().success(),
        "refusing daemon did not stop cleanly"
    );
    let _: u32 = dbus
        .call("ReleaseName", &(NOTIFICATIONS,))
        .expect("release fixture-owned name");

    let mut managed = daemon(&home, &endpoint, true, false);
    wait_owner(&connection, true);
    let expected_methods = [
        "notifications.clear",
        "notifications.dismiss",
        "notifications.invoke_action",
        "notifications.set_dnd",
        "notifications.set_hover",
        "notifications.set_inhibited",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect::<std::collections::BTreeSet<_>>();
    // The Freedesktop name is acquired before the daemon publishes readiness
    // and its dynamic method set. Wait for that separate startup milestone.
    let deadline = Instant::now() + Duration::from_secs(8);
    let method_names = loop {
        let methods = advertised_methods(&home)
            .as_object()
            .expect("daemon method map")
            .keys()
            .filter(|method| method.starts_with("notifications."))
            .cloned()
            .collect::<std::collections::BTreeSet<_>>();
        if methods == expected_methods || Instant::now() >= deadline {
            break methods;
        }
        thread::sleep(Duration::from_millis(20));
    };
    assert_eq!(method_names, expected_methods);
    legacy_history_save(&home);
    let native = native_proxy(&connection);
    let started = Instant::now();
    let native_id: u32 = native
        .call(
            "Notify",
            &(
                "N0017 fixture",
                0u32,
                "",
                "N0017 retained fixture",
                "Private bus only",
                Vec::<String>::new(),
                std::collections::HashMap::<String, zbus::zvariant::OwnedValue>::new(),
                0i32,
            ),
        )
        .expect("FDO Notify");
    assert_ne!(native_id, 0);
    assert!(
        started.elapsed() < Duration::from_secs(2),
        "Notify waited for durable history IO"
    );
    native
        .call::<_, _, ()>("CloseNotification", &(native_id,))
        .expect("close FDO notice into retained history");

    let dnd = call_dnd(&home, true);
    assert_eq!(dnd["changed"], true);
    let settings = home.join("config/nacre/desktop.json");
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if fs::read(&settings)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
            .is_some_and(|value| value["dnd"] == true)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "DND was not persisted by settings service"
        );
        thread::sleep(Duration::from_millis(20));
    }

    // Allow the specified short coalesced history flush, then verify shutdown
    // confirms the latest retained row before relinquishing the bus name.
    thread::sleep(Duration::from_millis(250));
    assert!(
        managed.stop().success(),
        "managed shutdown did not exit cleanly"
    );
    wait_owner(&connection, false);
    let history = home.join("state/nacre/shell/notifications/history.json");
    let rows: Value = serde_json::from_slice(&fs::read(&history).expect("flushed history"))
        .expect("history JSON");
    assert!(rows["items"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|row| row["summary"] == "N0017 retained fixture")
    }));

    let mut restarted = daemon(&home, &endpoint, true, false);
    wait_owner(&connection, true);
    let snapshot = notifications_snapshot(&home);
    assert_eq!(snapshot["topic"], "notifications");
    assert_eq!(snapshot["data"]["popup_order"], serde_json::json!([]));
    assert_eq!(snapshot["data"]["active"]["order"], serde_json::json!([]));
    let restored_ids = snapshot["data"]["history"]["order"]
        .as_array()
        .expect("restored history order")
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect::<Vec<_>>();
    assert!(!restored_ids.is_empty());
    let second: u32 = native_proxy(&connection)
        .call(
            "Notify",
            &(
                "N0017 fixture",
                0u32,
                "",
                "N0017 new generation",
                "Restarted daemon",
                Vec::<String>::new(),
                std::collections::HashMap::<String, zbus::zvariant::OwnedValue>::new(),
                0i32,
            ),
        )
        .expect("Notify after restart");
    assert_ne!(second, 0);
    let after_notify = notifications_snapshot(&home);
    let active_ids = after_notify["data"]["active"]["order"]
        .as_array()
        .expect("active order after restart Notify");
    assert_eq!(active_ids.len(), 1);
    let active_id = active_ids[0].as_str().expect("opaque active ID");
    assert!(
        !restored_ids.iter().any(|id| id == active_id),
        "new active ID collided with a restored history ID"
    );
    assert!(
        restarted.stop().success(),
        "restarted daemon did not exit cleanly"
    );
    wait_owner(&connection, false);
}

/// N0068: the installed layout (shell/tools + shell/source/config), a history file written by
/// the QML shell, a name owned by another server that is released later, and a daemon that
/// is stopped while managed notifications cannot run must all behave without a restart or abort.
#[test]
fn managed_notifications_takeover_private_bus() {
    if std::env::var_os("NACRE_NOTIFICATION_TEST_CHILD").is_some() {
        takeover_scenario();
        return;
    }
    let home = tempfile::tempdir().expect("private test HOME");
    let endpoint = directories(home.path());
    let output = Command::new("dbus-run-session")
        .args([
            "--",
            std::env::current_exe()
                .expect("test executable")
                .to_str()
                .expect("UTF-8 test executable"),
            "--exact",
            "managed_notifications_takeover_private_bus",
            "--nocapture",
        ])
        .env("NACRE_NOTIFICATION_TEST_CHILD", "1")
        .env("NACRE_NOTIFICATION_TEST_HOME", home.path())
        .env("NACRE_NOTIFICATION_TEST_SOCKET", endpoint)
        .output()
        .expect("run fixture on a private D-Bus session");
    assert!(
        output.status.success(),
        "private-bus child failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn takeover_scenario() {
    let home = PathBuf::from(std::env::var_os("NACRE_NOTIFICATION_TEST_HOME").expect("test HOME"));
    let endpoint =
        PathBuf::from(std::env::var_os("NACRE_NOTIFICATION_TEST_SOCKET").expect("test socket"));
    let _niri = NiriSource::new(&home.join("runtime/niri.sock"))
        .expect("start isolated Niri socket fixture");
    let connection = Connection::session().expect("connect to dbus-run-session bus");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre");

    // Installed layout: tools next to source/config, no shell/config folder.
    let tools = home.join("data/nacre/shell/tools");
    let config = home.join("data/nacre/shell/source/config");
    fs::create_dir_all(&tools).unwrap();
    fs::create_dir_all(&config).unwrap();
    fs::copy(
        source.join("shell-tools/notification-policy.json"),
        tools.join("notification-policy.json"),
    )
    .unwrap();
    fs::copy(
        source.join("shell/config/NacreNotifications.qml"),
        config.join("NacreNotifications.qml"),
    )
    .unwrap();
    // A history file as the QML shell writes it.
    let history = home.join("state/nacre/shell/notifications");
    fs::create_dir_all(&history).unwrap();
    fs::set_permissions(&history, fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        history.join("history.json"),
        format!(
            r#"[{{"key":"k1","time":"{}","summary":"From QML","body":"","appName":"kitty","appIcon":"","image":"","urgency":1}}]"#,
            chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
        ),
    )
    .unwrap();
    fs::set_permissions(
        history.join("history.json"),
        fs::Permissions::from_mode(0o600),
    )
    .unwrap();

    // Another server owns the name first; the daemon must wait, then take it over unrestarted.
    let dbus =
        Proxy::new(&connection, DBUS, "/org/freedesktop/DBus", DBUS).expect("D-Bus control proxy");
    let acquired: u32 = dbus
        .call("RequestName", &(NOTIFICATIONS, 0u32))
        .expect("claim private notification name");
    assert_eq!(acquired, 1, "fixture must become the primary owner");
    let mut managed = daemon_with_tools(&home, &endpoint, true, false, &tools);
    thread::sleep(Duration::from_millis(800));
    assert_eq!(
        dbus.call::<_, _, String>("GetNameOwner", &(NOTIFICATIONS,))
            .expect("fixture still owns the name"),
        connection.unique_name().unwrap().as_str()
    );
    let _: u32 = dbus
        .call("ReleaseName", &(NOTIFICATIONS,))
        .expect("release fixture-owned name");
    wait_owner(&connection, true);
    let rows = notifications_snapshot(&home).to_string();
    assert!(
        rows.contains("From QML"),
        "QML history was not loaded: {rows}"
    );
    assert!(managed.stop().success(), "daemon did not stop cleanly");

    // Managed notifications that cannot start (no policy) must not crash the daemon on stop.
    let empty = home.join("data/empty-tools");
    fs::create_dir_all(&empty).unwrap();
    let mut broken = daemon_with_tools(&home, &endpoint, true, false, &empty);
    thread::sleep(Duration::from_millis(1000));
    let status = broken.stop();
    assert!(
        status.code().is_some(),
        "daemon aborted on stop: {status:?}"
    );
}
