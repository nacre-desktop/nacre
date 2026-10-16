//! Actual native entry points on private sockets; no ambient desktop or bus discovery.
use nacre_helper::Result;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{symlink, PermissionsExt},
        net::{UnixDatagram, UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdout, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
use wait_timeout::ChildExt;
struct Process(Child);
impl Drop for Process {
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
        false
    }
}
struct Source {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    sockets: Arc<Mutex<Vec<UnixStream>>>,
    accept: Option<thread::JoinHandle<()>>,
    workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
    event_sockets: Arc<Mutex<Vec<UnixStream>>>,
    outputs: Arc<Mutex<Value>>,
}
fn headers() -> Vec<Value> {
    vec![
        json!({"WorkspacesChanged":{"workspaces":[{"id":1,"idx":1,"name":"Fixture","output":"fixture","is_active":true,"is_focused":true,"is_urgent":false,"active_window_id":1}]}}),
        json!({"WindowsChanged":{"windows":[{"id":1,"workspace_id":1,"app_id":"fixture","title":"DO_NOT_LOG_WINDOW_TITLE","pid":123,"is_focused":true,"is_floating":false,"is_urgent":false,"layout":{"tile_pos_in_workspace_view":[0,0],"window_offset_in_tile":[0,0],"window_size":[320,200]}}]}}),
        json!({"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":["Fixture"],"current_idx":0}}}),
    ]
}
impl Source {
    fn new(path: PathBuf) -> Result<Self> {
        let listener = UnixListener::bind(&path)?;
        let stop = Arc::new(AtomicBool::new(false));
        let sockets = Arc::new(Mutex::new(Vec::<UnixStream>::new()));
        let workers = Arc::new(Mutex::new(Vec::new()));
        let event_sockets = Arc::new(Mutex::new(Vec::<UnixStream>::new()));
        let outputs = Arc::new(Mutex::new(
            json!({"fixture":{"name":"fixture","modes":[{"width":1280,"height":720,"refresh_rate":60000}],"current_mode":0,"logical":{"x":0,"y":0,"width":1280,"height":720,"scale":1,"transform":"Normal"}}}),
        ));
        let fixture_outputs = outputs.clone();
        let event_streams = event_sockets.clone();
        let (done, streams, children) = (stop.clone(), sockets.clone(), workers.clone());
        let accept = thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                if done.load(Ordering::Relaxed) {
                    break;
                }
                streams.lock().unwrap().push(stream.try_clone().unwrap());
                let fixture_outputs = fixture_outputs.clone();
                let event_streams = event_streams.clone();
                children.lock().unwrap().push(thread::spawn(move|| {
                    let mut reader=BufReader::new(stream);let mut line=String::new();
                    while reader.read_line(&mut line).is_ok_and(|n|n>0) {
                        let request:Value=serde_json::from_str(&line).unwrap();line.clear();
                        if request=="EventStream" || request=="NacreOverviewHoverStream" {
                            writeln!(reader.get_mut(),"{}",json!({"Ok":"Handled"})).unwrap();
                            for event in headers() {writeln!(reader.get_mut(),"{event}").unwrap();}
                            event_streams.lock().unwrap().push(reader.get_ref().try_clone().unwrap());
                            let mut byte=[0];let _=reader.read(&mut byte);break;
                        }
                        let outputs=fixture_outputs.lock().unwrap().clone();
                        let response=if request=="Outputs"{json!({"Ok":{"Outputs":outputs}})}
                            else if request=="Workspaces"{json!({"Ok":{"Workspaces":headers()[0]["WorkspacesChanged"]["workspaces"]}})}
                            else if request=="Windows"{json!({"Ok":{"Windows":headers()[1]["WindowsChanged"]["windows"]}})}
                            else{json!({"Err":"Unsupported fixture request"})};
                        if writeln!(reader.get_mut(),"{response}").is_err(){break;}
                    }
                }));
            }
        });
        Ok(Self {
            path,
            stop,
            sockets,
            accept: Some(accept),
            workers,
            event_sockets,
            outputs,
        })
    }
}
impl Drop for Source {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        for socket in self.sockets.lock().unwrap().iter() {
            let _ = socket.shutdown(std::net::Shutdown::Both);
        }
        let _ = UnixStream::connect(&self.path);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
        for worker in self.workers.lock().unwrap().drain(..) {
            let _ = worker.join();
        }
    }
}
fn environment(command: &mut Command, home: &Path, source: &Path, endpoint: &Path) {
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", "")
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_RUNTIME_DIR", home.join("runtime"))
        .env("NACRE_DAEMON_SOCKET", endpoint)
        .env("NACRE_COMPOSITOR", "niri")
        .env("NIRI_SOCKET", source)
        .env("NACRE_NIRI_HOVER_STREAM", "0")
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", home.join("absent-session").display()),
        )
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", home.join("absent-system").display()),
        );
}
fn command(home: &Path, source: &Path, endpoint: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    environment(&mut command, home, source, endpoint);
    command
}
fn status(home: &Path, source: &Path, endpoint: &Path) -> Result<Value> {
    let output = command(home, source, endpoint)
        .args(["ctl", "status"])
        .output()?;
    if !output.status.success() {
        return Err("Native status call failed".into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
fn next_message(reader: &mut BufReader<ChildStdout>) -> Result<nacre_protocol::Message> {
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut line = Vec::new();
    loop {
        if reader.buffer().is_empty() {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or("Native watch response timed out")?;
            let timeout = rustix::event::Timespec {
                tv_sec: remaining.as_secs() as i64,
                tv_nsec: i64::from(remaining.subsec_nanos()),
            };
            let ready = {
                let mut fds = [rustix::event::PollFd::new(
                    reader.get_ref(),
                    rustix::event::PollFlags::IN,
                )];
                rustix::event::poll(&mut fds, Some(&timeout))?
            };
            if ready == 0 {
                return Err("Native watch response timed out".into());
            }
        }
        let bytes = reader.fill_buf()?;
        if bytes.is_empty() {
            return Err("Native watch ended".into());
        }
        let newline = bytes.iter().position(|b| *b == b'\n');
        let used = newline.map_or(bytes.len(), |n| n + 1);
        if line.len() + used > nacre_protocol::MAX_FRAME + 1 {
            return Err("Oversized native watch response".into());
        }
        line.extend_from_slice(&bytes[..used]);
        reader.consume(used);
        if newline.is_some() {
            return Ok(nacre_protocol::framing::decode(&line[..line.len() - 1])?);
        }
    }
}
#[test]
fn actual_daemon_ready_native_client_restart_safe_mode_and_cleanup() -> Result<()> {
    let home = tempfile::Builder::new().prefix("nd-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let folder = home.path().join("private");
    fs::create_dir(&folder)?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let endpoint = folder.join("daemon.sock");
    let notify = home.path().join("notify.sock");
    let notifications = UnixDatagram::bind(&notify)?;
    notifications.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut boots = Vec::new();
    let mut watcher = None;
    let mut watcher_read = None;
    let mut mirror = nacre_protocol::Mirror::default();
    for generation in 0..4 {
        let log = home.path().join(format!("daemon-{generation}.log"));
        let mut child = command(home.path(), &source_path, &endpoint);
        child
            .arg("daemon")
            .env("NOTIFY_SOCKET", &notify)
            .stdout(Stdio::null())
            .stderr(fs::File::create(&log)?);
        let mut child = Process(child.spawn()?);
        let mut packet = [0; 256];
        let size = notifications.recv(&mut packet)?;
        assert!(std::str::from_utf8(&packet[..size])?.contains("READY=1"));
        let value = status(home.path(), &source_path, &endpoint)?;
        assert_eq!(value["safe"], generation == 3);
        assert_eq!(value["services"]["compositor"]["status"], "running");
        let boot = value["boot"].as_str().ok_or("Missing boot ID")?.to_owned();
        assert!(!boots.contains(&boot));
        boots.push(boot.clone());
        if watcher.is_none() {
            let mut command = command(home.path(), &source_path, &endpoint);
            let mut process = Process(
                command
                    .args(["ctl", "watch", "compositor"])
                    .stdout(Stdio::piped())
                    .stderr(fs::File::create(home.path().join("watch.log"))?)
                    .spawn()?,
            );
            watcher_read = Some(BufReader::new(
                process.0.stdout.take().ok_or("Missing watch output")?,
            ));
            watcher = Some(process);
        }
        loop {
            let message = next_message(watcher_read.as_mut().ok_or("Missing watch reader")?)?;
            let update = mirror.receive(&message);
            assert!(!matches!(update, nacre_protocol::Update::Resync(_)));
            if matches!(update,nacre_protocol::Update::Changed(ref topic) if topic=="compositor")
                && mirror.boot.as_ref() == Some(&boot)
                && mirror
                    .topics
                    .get("compositor")
                    .is_some_and(|(_, data)| data["available"] == true)
            {
                break;
            }
        }
        assert!(mirror.topics["compositor"].1["windows"]
            .get("niri-window:1")
            .is_some());
        if generation < 3 {
            child.0.kill()?;
            child.0.wait()?;
        } else {
            nix::sys::signal::kill(
                nix::unistd::Pid::from_raw(child.0.id() as i32),
                nix::sys::signal::Signal::SIGTERM,
            )?;
            assert!(child
                .0
                .wait_timeout(Duration::from_secs(5))?
                .ok_or("Daemon did not stop")?
                .success());
            assert!(!endpoint.exists());
        }
        assert!(!fs::read_to_string(log)?.contains("DO_NOT_LOG_WINDOW_TITLE"));
    }
    drop(watcher_read);
    drop(watcher);
    Ok(())
}
#[test]
fn session_policy_uses_private_bus_shared_transport_and_existing_cli_records() -> Result<()> {
    let home = tempfile::Builder::new().prefix("ds-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let source = Source::new(source_path.clone())?;
    let config = home.path().join("bus.xml");
    fs::write(&config,format!("<busconfig><type>session</type><listen>unix:path={}</listen><auth>EXTERNAL</auth><policy context=\"default\"><allow own=\"*\"/><allow send_destination=\"*\"/><allow receive_sender=\"*\"/></policy></busconfig>",home.path().join("bus").display()))?;
    let mut bus = Process(
        Command::new("/usr/bin/dbus-daemon")
            .args(["--nofork", "--print-address", "--config-file"])
            .arg(&config)
            .env_clear()
            .env("HOME", home.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().ok_or("Missing private bus")?).read_line(&mut address)?;
    let server = zbus::blocking::connection::Builder::address(address.trim())?
        .name("org.freedesktop.login1")?
        .name("org.freedesktop.secrets")?
        .serve_at("/org/freedesktop/secrets/aliases/default", Wallet)?
        .build()?;
    let qs = home.path().join(".local/share/nacre/shell/bin/qs");
    fs::create_dir_all(qs.parent().ok_or("Missing private bin")?)?;
    let marker = home.path().join("legacy-ui-ran");
    fs::write(
        &qs,
        format!(
            "#!/bin/sh\n: > '{}'\nprintf '%s\\n' '{{\"launcherMode\":\"apps\"}}'\n",
            marker.display()
        ),
    )?;
    fs::set_permissions(&qs, fs::Permissions::from_mode(0o700))?;
    let endpoint = home.path().join("daemon.sock");
    let notify = UnixDatagram::bind(home.path().join("notify"))?;
    notify.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut launch = command(home.path(), &source_path, &endpoint);
    launch
        .arg("daemon")
        .env("NOTIFY_SOCKET", home.path().join("notify"))
        .env("RUST_LOG", "nacre_helper=debug")
        .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
        .env("DBUS_SYSTEM_BUS_ADDRESS", address.trim())
        .stdout(Stdio::null())
        .stderr(fs::File::create(home.path().join("daemon.log"))?);
    let daemon = Process(launch.spawn()?);
    let mut bytes = [0; 256];
    notify.recv(&mut bytes)?;
    let stream = UnixStream::connect(&endpoint)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let control = stream.try_clone()?;
    let mut provider = BufReader::new(stream);
    writeln!(
        provider.get_mut(),
        "{}",
        json!({"t":"hello","proto":1,"min":1,"client":"session-policy-test"})
    )?;
    let mut line = String::new();
    provider.read_line(&mut line)?;
    assert_eq!(serde_json::from_str::<Value>(&line)?["t"], "welcome");
    writeln!(
        provider.get_mut(),
        "{}",
        json!({"t":"register","provider":"ui","methods":{"ui.dispatch":1}})
    )?;
    let state_provider = thread::spawn(move || -> Result<()> {
        loop {
            let mut line = String::new();
            match provider.read_line(&mut line) {
                Ok(0) => return Ok(()),
                Err(error)
                    if matches!(
                        error.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                    ) =>
                {
                    continue
                }
                Err(_) => return Ok(()),
                Ok(_) => {}
            }
            let message: Value = serde_json::from_str(&line)?;
            if message["t"] == "call" {
                assert_eq!(message["m"], "ui.dispatch");
                assert_eq!(message["p"]["target"], "nacre");
                assert_eq!(message["p"]["method"], "state");
                writeln!(
                    provider.get_mut(),
                    "{}",
                    json!({"t":"ret","id":message["id"],"ok":{"launcherMode":"apps"}})
                )?;
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if status(home.path(), &source_path, &endpoint)?["services"]["session-watch"]["status"]
            == "running"
        {
            break;
        }
        if Instant::now() > deadline {
            return Err("Session service did not start".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    if marker.exists() {
        fs::remove_file(&marker)?;
    }
    let mut request = command(home.path(), &source_path, &endpoint);
    request
        .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
        .env("DBUS_SYSTEM_BUS_ADDRESS", address.trim());
    let result = request
        .args(["session-watch", "check", "--event", "compatibility"])
        .output()?;
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let record: Value = serde_json::from_slice(&result.stdout)?;
    assert_eq!(record["event"], "compatibility");
    assert_eq!(record["wallet"], "unlocked");
    assert_eq!(record["desktopReady"], true);
    assert!(record["monitors"].is_array());
    assert_eq!(record["monitors"][0]["name"], "fixture");
    assert!(!marker.exists());
    let ready = command(home.path(), &source_path, &endpoint)
        .args(["session-watch", "ready"])
        .output()?;
    assert!(ready.status.success());
    let ready: Value = serde_json::from_slice(&ready.stdout)?;
    assert_eq!(ready["event"], "startup-readiness");
    assert_eq!(ready["desktopReady"], true);
    let latest = home.path().join("state/nacre/session/latest.json");
    let wait = |event: &str| -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if fs::read(&latest)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|value| value["event"] == event)
            {
                return Ok(());
            }
            if Instant::now() > deadline {
                return Err("Session policy event was not recorded".into());
            }
            thread::sleep(Duration::from_millis(20));
        }
    };
    server.emit_signal(
        None::<&str>,
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
        "PrepareForSleep",
        &(true,),
    )?;
    wait("before-sleep")?;
    let resumed = Instant::now();
    server.emit_signal(
        None::<&str>,
        "/org/freedesktop/login1",
        "org.freedesktop.login1.Manager",
        "PrepareForSleep",
        &(false,),
    )?;
    wait("resume")?;
    assert!(resumed.elapsed() >= Duration::from_millis(500));
    {
        let mut outputs = source.outputs.lock().unwrap();
        outputs["fixture"]["modes"][0]["width"] = json!(1600);
        outputs["fixture"]["logical"]["width"] = json!(1600);
    }
    let hotplug = Instant::now();
    for socket in source.event_sockets.lock().unwrap().iter_mut() {
        writeln!(socket, "{}", headers()[0])?;
    }
    wait("display-hotplug")?;
    assert!(hotplug.elapsed() >= Duration::from_millis(300));
    let record: Value = serde_json::from_slice(&fs::read(&latest)?)?;
    assert_eq!(record["monitors"][0]["width"], 1600);
    assert_eq!(source.sockets.lock().unwrap().len(), 2);
    let output = command(home.path(), &source_path, &endpoint)
        .args(["session-watch", "watch"])
        .output()?;
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    // A large window event requests compositor resync, never display restore.
    let unchanged = fs::read(&latest)?;
    let mut large_window = headers()[1].clone();
    large_window["WindowsChanged"]["windows"][0]["title"] = json!("x".repeat(4096));
    for socket in source.event_sockets.lock().unwrap().iter_mut() {
        writeln!(socket, "{large_window}")?;
    }
    // The existing bounded pool retires after30s. Both D-Bus connections must
    // stay on the daemon runtime, without blocking API/executor worker threads.
    thread::sleep(Duration::from_secs(35));
    assert_eq!(fs::read(&latest)?, unchanged);
    let tasks = fs::read_dir(format!("/proc/{}/task", daemon.0.id()))?
        .collect::<std::io::Result<Vec<_>>>()?;
    let names = tasks
        .iter()
        .map(|task| {
            (
                fs::read_to_string(task.path().join("comm")).unwrap_or_default(),
                fs::read_to_string(task.path().join("wchan")).unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    let log = fs::read_to_string(home.path().join("daemon.log"))?;
    assert_eq!(
        tasks.len(),
        1,
        "Remaining threads: {names:?}; log tail: {}",
        log.lines()
            .rev()
            .take(16)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n")
    );
    control.shutdown(std::net::Shutdown::Both)?;
    state_provider.join().map_err(|_| "UI fixture failed")??;
    Ok(())
}

#[test]
fn native_niri_event_clients_share_daemon_stream_and_shutdown_without_a_timer() -> Result<()> {
    let home = tempfile::Builder::new().prefix("ne-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let source = Source::new(source_path.clone())?;
    let endpoint = home.path().join("daemon.sock");
    let notify = home.path().join("notify.sock");
    let notifications = UnixDatagram::bind(&notify)?;
    notifications.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut daemon = command(home.path(), &source_path, &endpoint);
    daemon
        .arg("daemon")
        .env("NOTIFY_SOCKET", &notify)
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let _daemon = Process(daemon.spawn()?);
    let mut packet = [0; 256];
    let size = notifications.recv(&mut packet)?;
    assert!(std::str::from_utf8(&packet[..size])?.contains("READY=1"));
    let environment = command(home.path(), &source_path, &endpoint)
        .get_envs()
        .filter_map(|(name, value)| {
            value.map(|value| {
                (
                    name.to_string_lossy().into_owned(),
                    value.to_string_lossy().into_owned(),
                )
            })
        })
        .collect();
    // The fixture emits its window only at compositor startup. Catalog indexing
    // must seed that snapshot, even when no subsequent event ever arrives.
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let output = command(home.path(), &source_path, &endpoint)
            .args([
                "ctl",
                "call",
                "menu.query",
                r#"{"query":"DO_NOT_LOG_WINDOW_TITLE","mode":"apps"}"#,
            ])
            .output()?;
        if output.status.success() {
            let result: Value = serde_json::from_slice(&output.stdout)?;
            if result["items"].as_object().is_some_and(|items| {
                items
                    .values()
                    .any(|row| row["name"] == "DO_NOT_LOG_WINDOW_TITLE")
            }) {
                break;
            }
        }
        if Instant::now() > deadline {
            return Err("Launcher missed the initial compositor window snapshot".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    let mut first = nacre_helper::daemon::niri_events::Source::connect(&environment, &source_path)?
        .ok_or("Missing event proxy")?;
    let backend = nacre_helper::compositor::niri::Backend::new(
        nacre_helper::paths::Paths::new(home.path()),
        environment.clone(),
    );
    let mut second = backend.events()?;
    let snapshot = first.next_value()?;
    assert_eq!(snapshot["type"], "snapshot");
    assert_eq!(snapshot["windows"][0]["title"], "DO_NOT_LOG_WINDOW_TITLE");
    assert_eq!(snapshot, second.next_value()?);
    // One event stream and one persistent request connection, regardless of consumers.
    assert_eq!(source.sockets.lock().unwrap().len(), 2);
    let control = first.control()?;
    let (finished, result) = std::sync::mpsc::channel();
    let reader = thread::spawn(move || finished.send(first.next_value().is_err()));
    control.shutdown(std::net::Shutdown::Both)?;
    assert!(result.recv_timeout(Duration::from_secs(3))?);
    reader.join().map_err(|_| "Event reader failed")??;
    drop(second);
    Ok(())
}

#[test]
fn native_width_clients_share_stream_scope_and_reconnect() -> Result<()> {
    if std::env::var_os("NACRE_WIDTHS_CLIENT_XVFB_CHILD").is_none() {
        let output = match Command::new("xvfb-run")
            .arg("-a")
            .arg(std::env::current_exe()?)
            .args([
                "--exact",
                "native_width_clients_share_stream_scope_and_reconnect",
                "--nocapture",
            ])
            .env_remove("DISPLAY")
            .env_remove("WAYLAND_DISPLAY")
            .env("NACRE_WIDTHS_CLIENT_XVFB_CHILD", "1")
            .output()
        {
            Ok(output) => output,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                eprintln!(
                    "native widths client check skipped: xvfb-run unavailable; CI installs it"
                );
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        assert!(
            output.status.success(),
            "native widths client check failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let display = std::env::var("DISPLAY")?;
    let authority = std::env::var("XAUTHORITY")?;
    assert!(
        authority.starts_with("/tmp/xvfb-run."),
        "Refusing a non-lab X11 display"
    );
    let home = tempfile::Builder::new().prefix("nw-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let source = Source::new(source_path.clone())?;
    let endpoint = home.path().join("daemon.sock");
    let log = home.path().join("daemon.log");
    let start = || -> Result<Process> {
        let mut cmd = command(home.path(), &source_path, &endpoint);
        cmd.arg("daemon")
            .env("DISPLAY", &display)
            .env("XAUTHORITY", &authority)
            .stdout(Stdio::null())
            .stderr(
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log)?,
            );
        Ok(Process(cmd.spawn()?))
    };
    let mut daemon = start()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if endpoint.exists() && status(home.path(), &source_path, &endpoint).is_ok() {
            break;
        }
        assert!(daemon.0.try_wait()?.is_none());
        if Instant::now() > deadline {
            return Err("Width daemon startup timed out".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    let mut watch = Process(
        command(home.path(), &source_path, &endpoint)
            .args(["ctl", "watch", "niri.widths"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut reader = BufReader::new(watch.0.stdout.take().ok_or("Missing watch output")?);
    let mut mirror = nacre_protocol::Mirror::default();
    let legacy = |width: &str| -> Result<Process> {
        Ok(Process(
            command(home.path(), &source_path, &endpoint)
                .args(["niri_widths", "--opening-width", width])
                .env("DISPLAY", &display)
                .env("XAUTHORITY", &authority)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        ))
    };
    let wait_width = |reader: &mut BufReader<ChildStdout>,
                      mirror: &mut nacre_protocol::Mirror,
                      width: f64|
     -> Result<()> {
        loop {
            let message = next_message(reader)?;
            mirror.receive(&message);
            if mirror.topics.get("niri.widths").is_some_and(|(_, data)| {
                data["available"] == true && data["opening_width"].as_f64() == Some(width)
            }) {
                return Ok(());
            }
        }
    };
    let mut first = legacy("52.5")?;
    wait_width(&mut reader, &mut mirror, 52.5)?;
    let mut second = legacy("44.5")?;
    wait_width(&mut reader, &mut mirror, 44.5)?;
    assert_eq!(
        source.sockets.lock().unwrap().len(),
        2,
        "Compatibility widths clients opened compositor sockets"
    );
    let other_path = home.path().join("other-niri.sock");
    let other = Source::new(other_path.clone())?;
    // A manually launched legacy observer still works when there is no default
    // runtime directory/broker at all. It must use only its explicit Niri socket.
    let standalone_path = home.path().join("standalone-niri.sock");
    let standalone = Source::new(standalone_path.clone())?;
    let mut direct = Process(
        command(home.path(), &standalone_path, &endpoint)
            .arg("niri_widths")
            .env_remove("NACRE_DAEMON_SOCKET")
            .env_remove("XDG_RUNTIME_DIR")
            .env("DISPLAY", &display)
            .env("XAUTHORITY", &authority)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let deadline = Instant::now() + Duration::from_secs(3);
    while standalone.event_sockets.lock().unwrap().is_empty() {
        assert!(
            direct.0.try_wait()?.is_none(),
            "Legacy observer required a daemon/runtime directory"
        );
        if Instant::now() > deadline {
            return Err("Standalone width observer did not connect".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(direct.0.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    )?;
    assert!(direct
        .0
        .wait_timeout(Duration::from_secs(3))?
        .ok_or("Standalone observer did not stop")?
        .success());
    assert_eq!(standalone.sockets.lock().unwrap().len(), 1);
    let wrong = command(home.path(), &other_path, &endpoint)
        .arg("niri_widths")
        .env("DISPLAY", &display)
        .env("XAUTHORITY", &authority)
        .output()?;
    assert!(!wrong.status.success());
    assert!(
        other.sockets.lock().unwrap().is_empty(),
        "Explicit session mismatch fell back to another compositor"
    );
    let wrong_display = command(home.path(), &source_path, &endpoint)
        .arg("niri_widths")
        .env("DISPLAY", "unrelated-display")
        .env("XAUTHORITY", &authority)
        .output()?;
    assert!(!wrong_display.status.success());
    assert_eq!(source.sockets.lock().unwrap().len(), 2);
    daemon.0.kill()?;
    daemon.0.wait()?;
    thread::sleep(Duration::from_millis(250));
    assert!(first.0.try_wait()?.is_none() && second.0.try_wait()?.is_none());
    assert_eq!(
        source.sockets.lock().unwrap().len(),
        2,
        "Disconnected client opened a standalone compositor stream"
    );
    daemon = start()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if status(home.path(), &source_path, &endpoint)
            .is_ok_and(|v| v["services"]["niri-widths"]["status"] == "running")
            && source.sockets.lock().unwrap().len() == 4
        {
            break;
        }
        assert!(daemon.0.try_wait()?.is_none());
        if Instant::now() > deadline {
            return Err("Width daemon restart timed out".into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    for client in [&mut first, &mut second] {
        nix::sys::signal::kill(
            nix::unistd::Pid::from_raw(client.0.id() as i32),
            nix::sys::signal::Signal::SIGTERM,
        )?;
        assert!(client
            .0
            .wait_timeout(Duration::from_secs(3))?
            .ok_or("Width client did not stop")?
            .success());
    }
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(daemon.0.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    )?;
    assert!(daemon
        .0
        .wait_timeout(Duration::from_secs(6))?
        .ok_or("Width daemon did not restore and stop")?
        .success());
    assert!(!fs::read_to_string(log)?.contains("DO_NOT_LOG_WINDOW_TITLE"));
    Ok(())
}

#[test]
fn native_aliases_dispatch_without_python_and_explicit_private_socket_never_falls_back(
) -> Result<()> {
    let home = tempfile::Builder::new().prefix("nc-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let client = home.path().join("nacrectl");
    symlink(env!("CARGO_BIN_EXE_nacre-helper"), &client)?;
    let endpoint = home.path().join("missing.sock");
    let source = home.path().join("niri.sock");
    let mut help = Command::new(&client);
    environment(&mut help, home.path(), &source, &endpoint);
    assert!(help.arg("--help").output()?.status.success());
    let binary = home.path().join(".local/share/nacre/shell/bin/qs");
    fs::create_dir_all(binary.parent().ok_or("Missing fixture parent")?)?;
    let marker = home.path().join("fallback-ran");
    fs::write(&binary, format!("#!/bin/sh\n: > '{}'\n", marker.display()))?;
    fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;
    let mut call = Command::new(&client);
    environment(&mut call, home.path(), &source, &endpoint);
    let result = call.args(["ui", "nacre", "launcher", ""]).output()?;
    assert!(!result.status.success());
    assert!(!marker.exists());
    Ok(())
}

#[test]
fn inherited_systemd_socket_is_adopted_and_preserved_without_a_live_manager() -> Result<()> {
    let activator = Path::new("/usr/bin/systemd-socket-activate");
    if !activator.is_file() {
        eprintln!("Skipping socket activation: systemd-socket-activate is not installed");
        return Ok(());
    }
    let home = tempfile::Builder::new().prefix("na-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let folder = home.path().join("private");
    fs::create_dir(&folder)?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let endpoint = folder.join("daemon.sock");
    let notify = home.path().join("notify.sock");
    let notifications = UnixDatagram::bind(&notify)?;
    notifications.set_read_timeout(Some(Duration::from_secs(10)))?;
    let changes = rustix::fs::inotify::init(rustix::fs::inotify::CreateFlags::CLOEXEC)?;
    rustix::fs::inotify::add_watch(&changes, &folder, rustix::fs::inotify::WatchFlags::CREATE)?;
    let mut activate = Command::new(activator);
    environment(&mut activate, home.path(), &source_path, &endpoint);
    activate.env("NOTIFY_SOCKET", &notify);
    let names: Vec<_> = activate
        .get_envs()
        .filter_map(|(name, value)| value.map(|_| name.to_owned()))
        .collect();
    for name in names {
        activate.arg("-E").arg(name);
    }
    activate
        .arg("-l")
        .arg(&endpoint)
        .arg(env!("CARGO_BIN_EXE_nacre-helper"))
        .arg("daemon")
        .stdout(Stdio::null())
        .stderr(fs::File::create(home.path().join("activation.log"))?);
    let mut daemon = Process(activate.spawn()?);
    let mut descriptors = [rustix::event::PollFd::new(
        &changes,
        rustix::event::PollFlags::IN,
    )];
    let timeout = rustix::event::Timespec {
        tv_sec: 10,
        tv_nsec: 0,
    };
    assert!(
        rustix::event::poll(&mut descriptors, Some(&timeout))? > 0,
        "Activation did not create its private socket"
    );
    fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
    assert!(status(home.path(), &source_path, &endpoint)?["boot"].is_string());
    let mut packet = [0; 256];
    let size = notifications.recv(&mut packet)?;
    assert!(std::str::from_utf8(&packet[..size])?.contains("READY=1"));
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(daemon.0.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    )?;
    assert!(daemon
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Activated daemon did not stop")?
        .success());
    assert!(
        endpoint.exists(),
        "The activated listener belongs to the socket manager"
    );
    Ok(())
}

#[test]
fn legacy_ui_fallback_requires_a_proven_undispatched_call() -> Result<()> {
    for dispatched in [false, true] {
        let home = tempfile::Builder::new().prefix("nf-").tempdir()?;
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
        let folder = home.path().join("runtime/nacre");
        fs::create_dir_all(&folder)?;
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
        let endpoint = folder.join("daemon.sock");
        let server = UnixListener::bind(&endpoint)?;
        fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
        let server = thread::spawn(move || {
            let (stream, _) = server.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            writeln!(
                reader.get_mut(),
                "{}",
                json!({"t":"welcome","proto":1,"min":1,"boot":"fixture","topics":{},"methods":{}})
            )
            .unwrap();
            line.clear();
            reader.read_line(&mut line).unwrap();
            let call: Value = serde_json::from_str(&line).unwrap();
            writeln!(reader.get_mut(),"{}",json!({"t":"ret","id":call["id"],"err":{"code":"unavailable","message":"Synthetic unavailable UI","retry":true,"dispatched":dispatched}})).unwrap();
        });
        let binary = home.path().join(".local/share/nacre/shell/bin/qs");
        fs::create_dir_all(binary.parent().ok_or("Missing fixture parent")?)?;
        fs::write(&binary, "#!/bin/sh\n: > \"$FALLBACK_MARKER\"\n")?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;
        let marker = home.path().join("fallback-ran");
        let mut call = command(home.path(), &home.path().join("niri.sock"), &endpoint);
        call.env_remove("NACRE_DAEMON_SOCKET")
            .env("FALLBACK_MARKER", &marker);
        let result = call.args(["ctl", "ui", "nacre", "launcher", ""]).output()?;
        assert_eq!(result.status.success(), !dispatched);
        assert_eq!(marker.exists(), !dispatched);
        server.join().map_err(|_| "Private UI fixture failed")?;
    }
    Ok(())
}

#[test]
fn watch_exits_when_its_output_is_closed_instead_of_reconnecting_forever() -> Result<()> {
    let home = tempfile::Builder::new().prefix("nw-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let endpoint = home.path().join("daemon.sock");
    let server = UnixListener::bind(&endpoint)?;
    fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
    let (ready, resume) = std::sync::mpsc::channel();
    let (subscribed, subscription) = std::sync::mpsc::channel();
    let server = thread::spawn(move || {
        let (stream, _) = server.accept().unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        writeln!(reader.get_mut(),"{}",json!({"t":"welcome","proto":1,"min":1,"boot":"fixture","topics":{"compositor":1},"methods":{}})).unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        assert_eq!(serde_json::from_str::<Value>(&line).unwrap()["t"], "sub");
        subscribed.send(()).unwrap();
        // Hold an idle connection: detecting the closed output must not require
        // another daemon message or a timeout/reconnect to provoke a write.
        let _ = resume.recv_timeout(Duration::from_secs(5));
    });
    let mut watch = command(home.path(), &home.path().join("niri.sock"), &endpoint);
    let mut watch = Process(
        watch
            .args(["ctl", "watch", "compositor"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut output = BufReader::new(watch.0.stdout.take().ok_or("Missing watch pipe")?);
    assert!(matches!(
        next_message(&mut output)?,
        nacre_protocol::Message::Welcome { .. }
    ));
    subscription.recv_timeout(Duration::from_secs(5))?;
    drop(output);
    assert!(watch
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Closed-output watch did not exit")?
        .success());
    ready.send(())?;
    server.join().map_err(|_| "Private watch fixture failed")?;
    Ok(())
}

#[test]
fn daemon_serve_replays_frozen_dialogue_and_large_packets_without_an_interpreter() -> Result<()> {
    let home = tempfile::Builder::new().prefix("ns-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let endpoint = home.path().join("daemon.sock");
    let notify = UnixDatagram::bind(home.path().join("notify"))?;
    notify.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut launch = command(home.path(), &source_path, &endpoint);
    let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools");
    launch
        .arg("daemon")
        .env("NACRE_HELPER_TOOLS", &tools)
        .env("NOTIFY_SOCKET", home.path().join("notify"))
        .stdout(Stdio::null())
        .stderr(fs::File::create(home.path().join("daemon.log"))?);
    let mut daemon = Process(launch.spawn()?);
    let mut bytes = [0; 256];
    notify.recv(&mut bytes)?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let output = command(home.path(), &source_path, &endpoint)
            .args(["ctl", "call", "menu.preferences", "{\"action\":\"load\"}"])
            .output()?;
        if output.status.success() {
            break;
        }
        if Instant::now() > deadline {
            return Err("Launcher writer did not initialize".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    let mut bridge = Process(
        command(home.path(), &source_path, &endpoint)
            .arg("serve")
            .env("NACRE_HELPER_TOOLS", &tools)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(fs::File::create(home.path().join("bridge.log"))?)
            .spawn()?,
    );
    let mut input = bridge.0.stdin.take().ok_or("Missing bridge input")?;
    let output = bridge.0.stdout.take().ok_or("Missing bridge output")?;
    let (sender, receiver) = std::sync::mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            if sender.send(line).is_err() {
                break;
            }
        }
    });
    let receive = || -> Result<Value> {
        Ok(serde_json::from_str(
            &receiver
                .recv_timeout(Duration::from_secs(15))
                .map_err(|error| {
                    format!(
                        "Bridge response {error}: {}",
                        fs::read_to_string(home.path().join("bridge.log")).unwrap_or_default()
                    )
                })??,
        )?)
    };
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/helpers/helper-service");
    let case: Value =
        serde_json::from_slice(&fs::read(fixtures.join("case-all-hosted-operations.json"))?)?;
    let expected: Value = serde_json::from_slice(&fs::read(
        fixtures.join("expected-all-hosted-operations.json"),
    )?)?;
    assert_eq!(receive()?, expected["stdout"][0]);
    for (index, step) in case["dialogue"]
        .as_array()
        .ok_or("Missing frozen dialogue")?
        .iter()
        .skip(1)
        .enumerate()
    {
        input.write_all(
            step["send"]
                .as_str()
                .ok_or("Missing frozen request")?
                .as_bytes(),
        )?;
        input.flush()?;
        assert_eq!(receive()?, expected["stdout"][index + 1]);
    }
    // Both directions exceed the inline threshold, while retaining the old
    // history format and large payload limits. No frozen output is regenerated.
    let summary = "L".repeat(600_000);
    writeln!(
        input,
        "{}",
        json!({"id":20,"tool":"notification-history","arguments":["save"],"input":[{"summary":summary}]})
    )?;
    writeln!(
        input,
        "{}",
        json!({"id":21,"tool":"notification-history","arguments":["load"],"input":null})
    )?;
    input.flush()?;
    let saved = receive()?;
    assert_eq!(saved, json!({"id":20,"code":0,"output":""}));
    let loaded = receive()?;
    assert_eq!(loaded["id"], 21);
    assert_eq!(loaded["code"], 0);
    assert_eq!(
        serde_json::from_str::<Value>(loaded["output"].as_str().ok_or("Missing history output")?)?,
        json!([{"summary":summary}])
    );
    // Unsupported operations retain the legacy error packet on all six lanes.
    for (index, tool) in nacre_helper::service::TOOLS.iter().enumerate() {
        let id = 30 + index;
        writeln!(
            input,
            "{}",
            json!({"id":id,"tool":tool,"arguments":[],"input":false})
        )?;
        input.flush()?;
        assert_eq!(
            receive()?,
            json!({"id":id,"code":1,"output":"{\"error\": \"Unsupported helper operation\"}"})
        );
    }
    // A real tool subprocess gates one lane. Other lanes must run before it is
    // released; further requests in the gated lane preserve FIFO.
    let brain = home.path().join(".local/bin/nacre-brain");
    fs::create_dir_all(brain.parent().ok_or("Missing private tools")?)?;
    fs::write(
        &brain,
        r#"#!/bin/sh
if [ "$1" = note ];then
 /bin/cat >/dev/null
 if [ "$5" = crash ];then kill -KILL "$PPID";exit 0;fi
 printf '%s\n' "$5" >> "$HOME/started"
 while [ ! -f "$HOME/release" ];do /bin/sleep 0.01;done
fi
printf '{}\n'
"#,
    )?;
    fs::set_permissions(&brain, fs::Permissions::from_mode(0o700))?;
    for id in 40..44 {
        writeln!(
            input,
            "{}",
            json!({"id":id,"tool":"dashboard-data","arguments":["capture",id.to_string()],"input":null})
        )?;
    }
    input.flush()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while !home.path().join("started").exists() {
        if Instant::now() > deadline {
            return Err("Gated worker did not start".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    writeln!(
        input,
        "{}",
        json!({"id":49,"tool":"notification-history","arguments":["load"],"input":null})
    )?;
    input.flush()?;
    let independent = receive()?;
    assert_eq!(independent["id"], 49);
    assert_eq!(independent["code"], 0);
    fs::write(home.path().join("release"), b"go")?;
    for id in 40..44 {
        let result = receive()?;
        assert_eq!(result["id"], id);
        assert_eq!(result["code"], 0);
    }
    assert_eq!(
        fs::read_to_string(home.path().join("started"))?,
        "40\n41\n42\n43\n"
    );
    // Kill only the fixture's own provider parent. The shared daemon survives,
    // reports failure and can service the next request without any replay.
    writeln!(
        input,
        "{}",
        json!({"id":60,"tool":"dashboard-data","arguments":["capture","crash"],"input":null})
    )?;
    input.flush()?;
    let crashed = receive()?;
    assert_eq!(crashed["id"], 60);
    assert_eq!(crashed["code"], 1);
    writeln!(
        input,
        "{}",
        json!({"id":61,"tool":"notification-history","arguments":["load"],"input":null})
    )?;
    input.flush()?;
    let recovered = receive()?;
    assert_eq!(recovered["id"], 61);
    assert_eq!(recovered["code"], 0);
    let raw = UnixStream::connect(&endpoint)?;
    raw.set_read_timeout(Some(Duration::from_secs(15)))?;
    let mut raw = BufReader::new(raw);
    let mut line = String::new();
    writeln!(
        raw.get_mut(),
        "{}",
        json!({"t":"hello","proto":1,"client":"helper-crash-policy"})
    )?;
    raw.read_line(&mut line)?;
    assert_eq!(serde_json::from_str::<Value>(&line)?["t"], "welcome");
    writeln!(
        raw.get_mut(),
        "{}",
        json!({"t":"call","id":90,"m":"helper.execute","p":{"request":{"id":90,"tool":"dashboard-data","arguments":["capture","crash"],"input":null}}})
    )?;
    line.clear();
    raw.read_line(&mut line)?;
    let failed: Value = serde_json::from_str(&line)?;
    assert_eq!(failed["id"], 90);
    assert_eq!(failed["err"]["retry"], false);
    assert_eq!(failed["err"]["dispatched"], true);
    // Publishing can fail after work finished too; a full file lease must not
    // advertise that the operation is safe to replay.
    writeln!(
        raw.get_mut(),
        "{}",
        json!({"t":"call","id":91,"m":"helper.transfer","p":{}})
    )?;
    line.clear();
    raw.read_line(&mut line)?;
    let lease: Value = serde_json::from_str(&line)?;
    let directory = Path::new(
        lease["ok"]["directory"]
            .as_str()
            .ok_or("Missing transfer lease")?,
    );
    for index in 0..128 {
        let path = directory.join(format!("packet-fixture-{index}"));
        fs::write(&path, b"{}")?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    writeln!(
        raw.get_mut(),
        "{}",
        json!({"t":"call","id":92,"m":"helper.execute","p":{"request":{"id":92,"tool":"notification-history","arguments":["load"],"input":null}}})
    )?;
    line.clear();
    raw.read_line(&mut line)?;
    let failed: Value = serde_json::from_str(&line)?;
    assert_eq!(failed["id"], 92);
    assert_eq!(failed["err"]["message"], "Cannot publish helper response");
    assert_eq!(failed["err"]["retry"], false);
    assert_eq!(failed["err"]["dispatched"], true);
    drop(raw);
    drop(input);
    assert!(bridge
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Bridge did not drain EOF")?
        .success());
    reader.join().map_err(|_| "Bridge reader failed")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let remains = fs::read_dir(home.path())?.filter_map(|e| e.ok()).any(|e| {
            e.file_name()
                .to_string_lossy()
                .starts_with("helper-packets-")
        });
        if !remains {
            break;
        }
        if Instant::now() > deadline {
            return Err("Disconnected bridge leaked its transfer lease".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    // A live stdin cannot keep a disconnected bridge alive or cause a
    // standalone replay. Restart also removes the killed daemon's stale lease.
    let mut connected = Process(
        command(home.path(), &source_path, &endpoint)
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let _held_input = connected
        .0
        .stdin
        .take()
        .ok_or("Missing held bridge input")?;
    let mut connected_output = BufReader::new(
        connected
            .0
            .stdout
            .take()
            .ok_or("Missing connected output")?,
    );
    let mut ready = String::new();
    connected_output.read_line(&mut ready)?;
    assert_eq!(
        serde_json::from_str::<Value>(&ready)?,
        json!({"ready":true,"version":1})
    );
    daemon.0.kill()?;
    daemon.0.wait()?;
    assert!(!connected
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Disconnected bridge stayed alive with stdin open")?
        .success());
    ready.clear();
    assert_eq!(connected_output.read_line(&mut ready)?, 0);
    let _restarted = Process(launch.spawn()?);
    notify.recv(&mut bytes)?;
    assert!(!fs::read_dir(home.path())?.filter_map(|e| e.ok()).any(|e| e
        .file_name()
        .to_string_lossy()
        .starts_with("helper-packets-")));
    Ok(())
}
