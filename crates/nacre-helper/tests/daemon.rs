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
struct Source {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    sockets: Arc<Mutex<Vec<UnixStream>>>,
    accept: Option<thread::JoinHandle<()>>,
    workers: Arc<Mutex<Vec<thread::JoinHandle<()>>>>,
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
        let (done, streams, children) = (stop.clone(), sockets.clone(), workers.clone());
        let accept = thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                if done.load(Ordering::Relaxed) {
                    break;
                }
                streams.lock().unwrap().push(stream.try_clone().unwrap());
                children.lock().unwrap().push(thread::spawn(move|| {
                    let mut reader=BufReader::new(stream);let mut line=String::new();
                    while reader.read_line(&mut line).is_ok_and(|n|n>0) {
                        let request:Value=serde_json::from_str(&line).unwrap();line.clear();
                        if request=="EventStream" || request=="NacreOverviewHoverStream" {
                            writeln!(reader.get_mut(),"{}",json!({"Ok":"Handled"})).unwrap();
                            for event in headers() {writeln!(reader.get_mut(),"{event}").unwrap();}
                            let mut byte=[0];let _=reader.read(&mut byte);break;
                        }
                        let outputs=json!({"fixture":{"name":"fixture","modes":[{"width":1280,"height":720,"refresh_rate":60000}],"current_mode":0,"logical":{"x":0,"y":0,"width":1280,"height":720,"scale":1,"transform":"Normal"}}});
                        let response=if request=="Outputs"{json!({"Ok":{"Outputs":outputs}})}else{json!({"Err":"Unsupported fixture request"})};
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
    let server = thread::spawn(move || {
        let (stream, _) = server.accept().unwrap();
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        writeln!(reader.get_mut(),"{}",json!({"t":"welcome","proto":1,"min":1,"boot":"fixture","topics":{"compositor":1},"methods":{}})).unwrap();
        line.clear();
        reader.read_line(&mut line).unwrap();
        resume.recv_timeout(Duration::from_secs(5)).unwrap();
        writeln!(
            reader.get_mut(),
            "{}",
            json!({"t":"snap","topic":"compositor","seq":0,"data":{"available":true}})
        )
        .unwrap();
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
    drop(output);
    ready.send(())?;
    assert!(watch
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Closed-output watch did not exit")?
        .success());
    server.join().map_err(|_| "Private watch fixture failed")?;
    Ok(())
}
