//! Actual native entry points on private sockets; no ambient desktop or bus discovery.
use nacre_helper::{daemon::writer_lease::WriterLease, Result};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{symlink, PermissionsExt},
        net::{UnixDatagram, UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
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
struct PythonLease {
    child: Child,
    input: Option<ChildStdin>,
    output: Option<BufReader<ChildStdout>>,
}
impl Drop for PythonLease {
    fn drop(&mut self) {
        self.input.take();
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn poll_until_ready(
    descriptors: &mut [rustix::event::PollFd<'_>],
    deadline: Instant,
) -> Result<bool> {
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Ok(false);
        }
        let timeout = rustix::event::Timespec {
            tv_sec: remaining.as_secs() as _,
            tv_nsec: remaining.subsec_nanos() as _,
        };
        match rustix::event::poll(descriptors, Some(&timeout)) {
            Ok(ready) => return Ok(ready > 0),
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
    }
}

fn read_until_complete(
    mut read: impl FnMut(&mut [u8], Duration) -> std::io::Result<usize>,
    deadline: Instant,
) -> Result<usize> {
    let mut response = [0];
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "activated daemon response deadline expired",
            )
            .into());
        }
        match read(&mut response, remaining) {
            Ok(bytes) => return Ok(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => return Ok(0),
            Err(error) => {
                return Err(std::io::Error::new(
                    error.kind(),
                    format!("reading activated daemon response: {error}"),
                )
                .into());
            }
        }
    }
}

#[test]
fn activated_daemon_response_retries_only_interrupted_reads_with_deadline() {
    let mut calls = 0;
    let bytes = read_until_complete(
        |_, _| {
            calls += 1;
            if calls == 1 {
                Err(std::io::Error::from(std::io::ErrorKind::Interrupted))
            } else {
                Ok(0)
            }
        },
        Instant::now() + Duration::from_secs(1),
    )
    .unwrap();
    assert_eq!(bytes, 0);
    assert_eq!(calls, 2);

    let mut interrupted_calls = 0;
    let persistent = read_until_complete(
        |_, _| {
            interrupted_calls += 1;
            Err(std::io::Error::from(std::io::ErrorKind::Interrupted))
        },
        Instant::now() + Duration::from_millis(2),
    );
    assert!(
        persistent.is_err(),
        "persistent EINTR must exhaust its deadline"
    );
    assert!(interrupted_calls > 0);
}

fn hold_python_lease(config: &Path) -> Result<PythonLease> {
    let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools");
    let script = r#"
import sys
from pathlib import Path
sys.path.insert(0, sys.argv[1])
from writer_lease import writer_lease
with writer_lease(Path(sys.argv[2])):
    print("HELD", flush=True)
    sys.stdin.readline()
"#;
    let child = Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(tools)
        .arg(config)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut lease = PythonLease {
        child,
        input: None,
        output: None,
    };
    lease.input = Some(
        lease
            .child
            .stdin
            .take()
            .ok_or("Missing Python lease stdin")?,
    );
    lease.output = Some(BufReader::new(
        lease
            .child
            .stdout
            .take()
            .ok_or("Missing Python lease stdout")?,
    ));
    let output = lease.output.as_mut().ok_or("Missing Python lease stdout")?;
    let mut descriptors = [rustix::event::PollFd::new(
        output.get_ref(),
        rustix::event::PollFlags::IN,
    )];
    if !poll_until_ready(&mut descriptors, Instant::now() + Duration::from_secs(5))? {
        return Err("Python lease barrier timed out".into());
    }
    let mut ready = String::new();
    output.read_line(&mut ready)?;
    if ready.trim() != "HELD" {
        return Err("Python lease barrier did not become ready".into());
    }
    Ok(lease)
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
                            else if request==json!({"Action":{"FocusColumnRight":{}}}){json!({"Ok":"Handled"})}
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

#[test]
fn provider_boot_change_and_lost_mutation_reply_never_start_a_disk_writer() -> Result<()> {
    for changed_boot in [true, false] {
        let home = tempfile::tempdir()?;
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
        let private = home.path().join("private");
        fs::create_dir(&private)?;
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
        let endpoint = private.join("daemon.sock");
        let listener = UnixListener::bind(&endpoint)?;
        fs::set_permissions(&endpoint, fs::Permissions::from_mode(0o600))?;
        let path = home.path().join("config/nacre/colors.json");
        fs::create_dir_all(path.parent().ok_or("Missing colors parent")?)?;
        let original = b"{\"version\":1,\"favorites\":[{\"id\":\"fixture\"}]}";
        fs::write(&path, original)?;
        let peer = thread::spawn(move || -> Result<()> {
            for index in 0..2 {
                let mut incoming = [rustix::event::PollFd::new(
                    &listener,
                    rustix::event::PollFlags::IN,
                )];
                assert_eq!(
                    rustix::event::poll(
                        &mut incoming,
                        Some(&rustix::event::Timespec {
                            tv_sec: 5,
                            tv_nsec: 0
                        })
                    )?,
                    1,
                    "Provider did not connect"
                );
                let (stream, _) = listener.accept()?;
                stream.set_read_timeout(Some(Duration::from_secs(5)))?;
                let mut reader = BufReader::new(stream);
                let mut line = String::new();
                reader.read_line(&mut line)?;
                assert_eq!(serde_json::from_str::<Value>(&line)?["t"], "hello");
                let boot = if changed_boot && index == 1 {
                    "provider-new"
                } else {
                    "provider-old"
                };
                writeln!(
                    reader.get_mut(),
                    "{}",
                    json!({"t":"welcome","proto":1,"min":1,"boot":boot,"topics":{},"methods":{}})
                )?;
                line.clear();
                let count = reader.read_line(&mut line)?;
                if index == 0 {
                    let request: Value = serde_json::from_str(&line)?;
                    assert_eq!(request["m"], "settings.get");
                    writeln!(
                        reader.get_mut(),
                        "{}",
                        json!({"t":"ret","id":1,"ok":{"store":"colors","rev":0,"exists":true,"dirty":false,"data":{"version":1,"favorites":[{"id":"fixture"}]}}})
                    )?;
                } else if changed_boot {
                    assert_eq!(count, 0, "A revision cannot cross daemon boots");
                } else {
                    let request: Value = serde_json::from_str(&line)?;
                    assert_eq!(request["m"], "settings.provider");
                    assert_eq!(request["p"]["value"]["favorites"], json!([]));
                    // The action may be accepted: lose the reply deliberately.
                }
            }
            Ok(())
        });
        let mut child = Process(
            command(home.path(), &home.path().join("absent-niri"), &endpoint)
                .env(
                    "NACRE_HELPER_TOOLS",
                    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
                )
                .args(["colors", "delete"])
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()?,
        );
        writeln!(
            child.0.stdin.take().ok_or("Missing colors input")?,
            "{}",
            json!({"id":"fixture"})
        )?;
        assert!(child
            .0
            .wait_timeout(Duration::from_secs(10))?
            .is_some_and(|status| !status.success()));
        peer.join().unwrap()?;
        let mut diagnostic = String::new();
        child
            .0
            .stderr
            .take()
            .ok_or("Missing provider error")?
            .read_to_string(&mut diagnostic)?;
        assert!(
            diagnostic.contains(if changed_boot {
                "Daemon restarted"
            } else {
                "Daemon disconnected"
            }),
            "{diagnostic}"
        );
        assert_eq!(fs::read(&path)?, original);
    }
    Ok(())
}

#[test]
fn settings_writer_preserves_developer_pseudo_language_without_enabling_it_normally() -> Result<()>
{
    for developer in [false, true] {
        let home = tempfile::tempdir()?;
        fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
        let source_path = home.path().join("niri.sock");
        let _source = Source::new(source_path.clone())?;
        let private = home.path().join("private");
        fs::create_dir(&private)?;
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
        let endpoint = private.join("daemon.sock");
        let config = home.path().join("config/nacre");
        fs::create_dir_all(&config)?;
        let desktop = config.join("desktop.json");
        fs::write(&desktop, br#"{"version":3,"language":"pseudo"}"#)?;
        let notify = home.path().join("notify.sock");
        let notifications = UnixDatagram::bind(&notify)?;
        notifications.set_read_timeout(Some(Duration::from_secs(10)))?;
        let _daemon = Process(
            command(home.path(), &source_path, &endpoint)
                .arg("daemon")
                .env("NACRE_DEVELOPER_MODE", if developer { "1" } else { "0" })
                .env("NOTIFY_SOCKET", &notify)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        notifications.recv(&mut [0; 256])?;
        let read = || -> Result<Value> {
            let output = command(home.path(), &source_path, &endpoint)
                .args(["ctl", "call", "settings.get", r#"{"store":"desktop"}"#])
                .output()?;
            assert!(output.status.success());
            Ok(serde_json::from_slice(&output.stdout)?)
        };
        let loaded = read()?;
        assert_eq!(loaded["error"].is_null(), developer, "{loaded}");
        if developer {
            assert_eq!(loaded["data"]["language"], "pseudo");
            let changed = command(home.path(), &source_path, &endpoint)
                .args(["desktop-settings", "set", "dnd", "true"])
                .output()?;
            assert!(
                changed.status.success(),
                "{}",
                String::from_utf8_lossy(&changed.stderr)
            );
            let saved: Value = serde_json::from_slice(&fs::read(&desktop)?)?;
            assert_eq!(saved["language"], "pseudo");
            assert_eq!(saved["dnd"], true);
            assert!(read()?["error"].is_null());
        } else {
            assert_ne!(loaded["data"]["language"], "pseudo");
            assert_eq!(fs::read(&desktop)?, br#"{"version":3,"language":"pseudo"}"#);
        }
    }
    Ok(())
}

#[test]
fn settings_writer_native_concurrency_hand_edits_and_killed_publication_keep_valid_state(
) -> Result<()> {
    use nacre_protocol::{Message, Reply};
    fn link(endpoint: &Path) -> Result<BufReader<UnixStream>> {
        let mut stream = UnixStream::connect(endpoint)?;
        stream.set_read_timeout(Some(Duration::from_secs(5)))?;
        writeln!(
            stream,
            "{}",
            json!({"t":"hello","proto":1,"client":"settings-fixture"})
        )?;
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let welcome: Message = serde_json::from_str(&line)?;
        assert!(
            matches!(welcome, Message::Welcome{methods,topics,..} if methods.contains_key("settings.set") && topics.contains_key("settings.store.power"))
        );
        Ok(reader)
    }
    fn call(reader: &mut BufReader<UnixStream>, method: &str, params: Value) -> Result<Reply> {
        writeln!(
            reader.get_mut(),
            "{}",
            json!({"t":"call","id":1,"m":method,"p":params,"timeout":5000})
        )?;
        let mut line = String::new();
        reader.read_line(&mut line)?;
        match serde_json::from_str::<Message>(&line)? {
            Message::Ret { reply, .. } => Ok(reply),
            _ => Err("Expected a settings reply".into()),
        }
    }
    fn get(reader: &mut BufReader<UnixStream>) -> Result<Value> {
        match call(reader, "settings.get", json!({"store":"power"}))? {
            Reply::Ok { ok } => Ok(ok),
            _ => Err("Settings read failed".into()),
        }
    }
    let home = tempfile::Builder::new()
        .prefix("settings-daemon-")
        .tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let private = home.path().join("private");
    fs::create_dir(&private)?;
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
    let endpoint = private.join("daemon.sock");
    let notify = home.path().join("notify.sock");
    let notifications = UnixDatagram::bind(&notify)?;
    notifications.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut daemon = Process(
        command(home.path(), &source_path, &endpoint)
            .arg("daemon")
            .env("NOTIFY_SOCKET", &notify)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut packet = [0; 256];
    notifications.recv(&mut packet)?;
    // Legacy QML save commands use the same daemon revision/cache and return
    // only after publication, rather than racing its delayed writer on disk.
    let publisher_path = home.path().join("config/nacre/control-center.json");
    let mut publisher = command(home.path(), &source_path, &endpoint)
        .args([
            "state-file",
            "save",
            publisher_path.to_str().ok_or("Invalid test path")?,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    writeln!(
        publisher.stdin.take().ok_or("Missing publisher input")?,
        "{}",
        json!({"version":2,"tiles":["vpn"]})
    )?;
    let published = publisher.wait_with_output()?;
    assert!(
        published.status.success(),
        "{}",
        String::from_utf8_lossy(&published.stdout)
    );
    let saved: Value = serde_json::from_slice(&fs::read(&publisher_path)?)?;
    assert_eq!(saved["tiles"], json!(["vpn"]));
    let mut first = link(&endpoint)?;
    assert!(
        matches!(call(&mut first,"settings.get",json!({"store":"control-center"}))?,Reply::Ok {ok} if ok["rev"]==1 && ok["dirty"]==false)
    );
    let mut second = link(&endpoint)?;
    let desktop = command(home.path(), &source_path, &endpoint)
        .args(["desktop-settings", "set", "dnd", "true"])
        .output()?;
    assert!(
        desktop.status.success(),
        "{}",
        String::from_utf8_lossy(&desktop.stdout)
    );
    let durable: Value =
        serde_json::from_slice(&fs::read(home.path().join("config/nacre/desktop.json"))?)?;
    assert_eq!(durable["dnd"], true);
    let mut preferences = command(home.path(), &source_path, &endpoint)
        .args(["launcher-preferences", "favorite"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    writeln!(
        preferences.stdin.take().ok_or("Missing preference input")?,
        "{}",
        json!({"id":"fixture.desktop","enabled":true})
    )?;
    let favorite = preferences.wait_with_output()?;
    assert!(
        favorite.status.success(),
        "{}",
        String::from_utf8_lossy(&favorite.stderr)
    );
    let durable: Value =
        serde_json::from_slice(&fs::read(home.path().join("config/nacre/launcher.json"))?)?;
    assert_eq!(durable["favorites"], json!(["fixture.desktop"]));
    assert!(matches!(
        call(
            &mut second,
            "settings.provider",
            json!({"store":"colors","value":{"version":1,"favorites":[{"id":"fixture"}]},"rev":0})
        )?,
        Reply::Ok { .. }
    ));
    let mut colors = command(home.path(), &source_path, &endpoint)
        .env(
            "NACRE_HELPER_TOOLS",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
        )
        .args(["colors", "delete"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    writeln!(
        colors.stdin.take().ok_or("Missing colors input")?,
        "{}",
        json!({"id":"fixture"})
    )?;
    let deleted = colors.wait_with_output()?;
    assert!(
        deleted.status.success(),
        "{}",
        String::from_utf8_lossy(&deleted.stderr)
    );
    let choices: Value =
        serde_json::from_slice(&fs::read(home.path().join("config/nacre/colors.json"))?)?;
    assert_eq!(choices["favorites"], json!([]));
    assert!(
        matches!(call(&mut second, "settings.get", json!({"store":"colors"}))?, Reply::Ok {ok} if ok["rev"]==2 && ok["dirty"]==false && ok["data"]["favorites"]==json!([]))
    );
    assert_eq!(get(&mut first)?["rev"], 0);
    let (one, two) = thread::scope(|scope| {
        let a = scope.spawn(|| {
            call(
                &mut first,
                "settings.set",
                json!({"store":"power","key":["enabled"],"value":true,"rev":0}),
            )
        });
        let b = scope.spawn(|| {
            call(
                &mut second,
                "settings.set",
                json!({"store":"power","key":["enabled"],"value":false,"rev":0}),
            )
        });
        (a.join().unwrap(), b.join().unwrap())
    });
    let replies = [one?, two?];
    assert_eq!(
        replies
            .iter()
            .filter(|r| matches!(r, Reply::Ok { .. }))
            .count(),
        1
    );
    assert_eq!(replies.iter().filter(|r| matches!(r, Reply::Err{err} if err.code == nacre_protocol::ErrorCode::StaleRevision)).count(),1);
    let path = home.path().join("config/nacre/power.json");
    let deadline = Instant::now() + Duration::from_secs(5);
    while get(&mut first)?["dirty"] == true {
        if Instant::now() > deadline {
            return Err("Settings publication did not finish".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    let old: Value = serde_json::from_slice(&fs::read(&path)?)?;
    fs::write(&path, "{\n invalid\n}")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let snapshot = get(&mut first)?;
        if snapshot["error"]["line"] == 2 {
            assert_eq!(snapshot["data"]["enabled"], old["enabled"]);
            break;
        }
        if Instant::now() > deadline {
            return Err("Settings hand edit was not observed".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    fs::write(&path, "{\"enabled\":false,\"unknown\":42}")?;
    let deadline = Instant::now() + Duration::from_secs(5);
    let restored = loop {
        let value = get(&mut first)?;
        if value["data"]["unknown"] == 42 && value.get("error").is_none() {
            break value;
        }
        if Instant::now() > deadline {
            return Err("Valid settings repair was not observed".into());
        }
        thread::sleep(Duration::from_millis(10));
    };
    assert!(matches!(
        call(
            &mut first,
            "settings.set",
            json!({"store":"power","key":["enabled"],"value":true,"rev":restored["rev"]})
        )?,
        Reply::Ok { .. }
    ));
    daemon.0.kill()?;
    daemon.0.wait()?;
    // Kill after the in-memory acknowledgement, before or during deferred
    // publication. The only acceptable disk states are complete old/new JSON.
    let saved: Value = serde_json::from_slice(&fs::read(&path)?)?;
    assert_eq!(saved["unknown"], 42);
    assert!(saved["enabled"].is_boolean());
    let mut restarted = Process(
        command(home.path(), &source_path, &endpoint)
            .arg("daemon")
            .env("NOTIFY_SOCKET", &notify)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    notifications.recv(&mut packet)?;
    let snapshot = get(&mut link(&endpoint)?)?;
    assert_eq!(snapshot["data"]["unknown"], 42);
    assert_eq!(snapshot["dirty"], false);
    restarted.0.kill()?;
    restarted.0.wait()?;
    Ok(())
}
fn next_message(reader: &mut BufReader<ChildStdout>) -> Result<nacre_protocol::Message> {
    let line = next_line(reader)?;
    Ok(nacre_protocol::framing::decode(&line[..line.len() - 1])?)
}
fn next_line(reader: &mut BufReader<ChildStdout>) -> Result<Vec<u8>> {
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
            return Ok(line);
        }
    }
}
fn next_line_context(reader: &mut BufReader<ChildStdout>, context: &str) -> Result<Vec<u8>> {
    next_line(reader).map_err(|error| std::io::Error::other(format!("{context}: {error}")).into())
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
    let rpc = |method: &str, params: Value| -> Result<std::process::Output> {
        Ok(command(home.path(), &source_path, &endpoint)
            .args(["ctl", "call", method, &params.to_string()])
            .output()?)
    };
    let binding = rpc("niri.source", json!({"socket":source_path}))?;
    assert!(
        binding.status.success(),
        "{}",
        String::from_utf8_lossy(&binding.stderr)
    );
    let binding: Value = serde_json::from_slice(&binding.stdout)?;
    let identity = &binding["connection_id"];
    assert_eq!(
        identity.as_str(),
        Some(nacre_helper::daemon::niri_events::identity(&source_path)?.as_str())
    );
    let alias = home.path().join("same-session.sock");
    symlink(&source_path, &alias)?;
    assert!(rpc("niri.source", json!({"socket":alias}))?
        .status
        .success());
    let wrong = home.path().join("other-session.sock");
    let _wrong = UnixListener::bind(&wrong)?;
    assert!(!rpc("niri.source", json!({"socket":wrong}))?
        .status
        .success());
    for params in [
        json!({"connection_id":"wrong","argv":["action","reload"]}),
        json!({"connection_id":identity,"argv":["query","windows"]}),
        json!({"connection_id":identity,"argv":["action","exec","--command","false"]}),
    ] {
        assert!(!rpc("niri.action", params)?.status.success());
    }
    let action = rpc(
        "niri.action",
        json!({"connection_id":identity,"argv":["action","focus_direction","--direction","right"]}),
    )?;
    assert!(
        action.status.success(),
        "{}",
        String::from_utf8_lossy(&action.stderr)
    );
    assert_eq!(serde_json::from_slice::<Value>(&action.stdout)?["ok"], true);
    // Rebinding the same pathname cannot silently authorize the new session.
    let original = home.path().join("original.sock");
    fs::rename(&source_path, &original)?;
    let replacement = UnixListener::bind(&source_path)?;
    assert!(!rpc("niri.source", json!({"socket":source_path}))?
        .status
        .success());
    assert!(!rpc(
        "niri.action",
        json!({"connection_id":identity,"argv":["action","reload"]})
    )?
    .status
    .success());
    drop(replacement);
    fs::remove_file(&source_path)?;
    fs::rename(&original, &source_path)?;
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
fn python_writer_lease_blocks_normal_daemon_startup_for_each_endpoint() -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let home = tempfile::tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let config = home.path().join("config/nacre");
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let baseline_folder = home.path().join("baseline");
    fs::create_dir(&baseline_folder)?;
    fs::set_permissions(&baseline_folder, fs::Permissions::from_mode(0o700))?;
    let baseline_endpoint = baseline_folder.join("daemon.sock");
    let ready_path = home.path().join("baseline-ready.sock");
    let ready = UnixDatagram::bind(&ready_path)?;
    ready.set_read_timeout(Some(Duration::from_secs(10)))?;
    let mut baseline = Process(
        command(home.path(), &source_path, &baseline_endpoint)
            .arg("daemon")
            .env("NOTIFY_SOCKET", &ready_path)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut ready_message = [0; 256];
    let ready_size = ready.recv(&mut ready_message)?;
    assert!(std::str::from_utf8(&ready_message[..ready_size])?.contains("READY=1"));
    assert!(status(home.path(), &source_path, &baseline_endpoint).is_ok());
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(baseline.0.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    )?;
    assert!(baseline
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Unleased daemon did not stop")?
        .success());

    let _python = hold_python_lease(&config)?;
    let lock = config.join(".writer.lock");
    let before = fs::metadata(&lock)?;
    for name in ["first", "second"] {
        let folder = home.path().join(name);
        fs::create_dir(&folder)?;
        fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
        let endpoint = folder.join("daemon.sock");
        let output = command(home.path(), &source_path, &endpoint)
            .arg("daemon")
            .output()?;
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains("Nacre destination busy or unsafe"),
            "startup failed for an unrelated reason: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(!endpoint.exists(), "busy startup created its endpoint");
        let after = fs::metadata(&lock)?;
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    }
    Ok(())
}

#[test]
fn python_writer_lease_blocks_activated_daemon_before_service_publication() -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let activator = Path::new("/usr/bin/systemd-socket-activate");
    if !activator.is_file() {
        eprintln!("Skipping activated lease barrier: systemd-socket-activate is not installed");
        return Ok(());
    }
    let home = tempfile::tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let config = home.path().join("config/nacre");
    let _python = hold_python_lease(&config)?;
    let lock = config.join(".writer.lock");
    let before = fs::metadata(&lock).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!("reading initial writer lock identity: {error}"),
        )
    })?;
    let folder = home.path().join("private");
    fs::create_dir(&folder)?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let endpoint = folder.join("daemon.sock");
    let source = home.path().join("unused-niri.sock");
    let activation_log = home.path().join("activation.log");
    let changes = rustix::fs::inotify::init(rustix::fs::inotify::CreateFlags::CLOEXEC)?;
    rustix::fs::inotify::add_watch(&changes, &folder, rustix::fs::inotify::WatchFlags::CREATE)?;
    let mut activate = Command::new(activator);
    environment(&mut activate, home.path(), &source, &endpoint);
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
        .stderr(fs::File::create(&activation_log)?);
    let mut daemon = Process(activate.spawn()?);
    let mut descriptors = [rustix::event::PollFd::new(
        &changes,
        rustix::event::PollFlags::IN,
    )];
    assert!(
        poll_until_ready(&mut descriptors, Instant::now() + Duration::from_secs(10)).map_err(
            |error| std::io::Error::other(format!(
                "waiting for activated endpoint creation: {error}"
            )),
        )?,
        "Socket activator did not create its private endpoint"
    );
    let connect_deadline = Instant::now() + Duration::from_secs(10);
    let mut client = loop {
        match UnixStream::connect(&endpoint) {
            Ok(client) => break client,
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::ConnectionRefused | std::io::ErrorKind::Interrupted
                ) =>
            {
                if let Some(status) = daemon.0.try_wait().map_err(|wait_error| {
                    std::io::Error::new(
                        wait_error.kind(),
                        format!("checking activated daemon before retrying connect: {wait_error}"),
                    )
                })? {
                    return Err(format!(
                        "Socket activator exited before accepting the connection ({status}): {}",
                        fs::read_to_string(&activation_log).map_err(|log_error| {
                            std::io::Error::new(
                                log_error.kind(),
                                format!(
                                    "reading socket activation log after early exit: {log_error}"
                                ),
                            )
                        })?
                    )
                    .into());
                }
                if Instant::now() >= connect_deadline {
                    return Err(format!(
                        "Socket activator did not begin listening before the deadline: {}",
                        fs::read_to_string(&activation_log).map_err(|log_error| {
                            std::io::Error::new(
                                log_error.kind(),
                                format!("reading socket activation log at connect deadline: {log_error}"),
                            )
                        })?
                    )
                    .into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => {
                return Err(std::io::Error::new(
                    error.kind(),
                    format!("connecting to activated daemon socket: {error}"),
                )
                .into());
            }
        }
    };
    let bytes = read_until_complete(
        |response, remaining| {
            client.set_read_timeout(Some(remaining))?;
            client.read(response)
        },
        Instant::now() + Duration::from_secs(5),
    )?;
    assert_eq!(
        bytes, 0,
        "busy daemon published a service ({bytes} response bytes)"
    );
    assert!(
        fs::read_to_string(&activation_log)
            .map_err(|error| {
                std::io::Error::new(
                    error.kind(),
                    format!("reading socket activation log: {error}"),
                )
            })?
            .contains("Nacre destination busy or unsafe"),
        "activated daemon failed for an unrelated reason"
    );
    let after = fs::metadata(&lock).map_err(|error| {
        std::io::Error::new(
            error.kind(),
            format!("reading final writer lock identity: {error}"),
        )
    })?;
    assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    if daemon
        .0
        .try_wait()
        .map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("checking activated daemon exit: {error}"),
            )
        })?
        .is_none()
    {
        daemon.0.kill().map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("stopping activated daemon fixture: {error}"),
            )
        })?;
        daemon.0.wait().map_err(|error| {
            std::io::Error::new(
                error.kind(),
                format!("reaping activated daemon fixture: {error}"),
            )
        })?;
    }
    Ok(())
}

#[test]
fn native_writer_lease_blocks_python_profile_install_without_replacing_files() -> Result<()> {
    use std::os::unix::fs::MetadataExt;

    let home = tempfile::tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let config = home.path().join(".config/nacre");
    let state = home.path().join(".local/state/nacre");
    let _native = WriterLease::acquire_for_paths(&config, &state)?;
    let lock = config.join(".writer.lock");
    let before_lock = fs::metadata(&lock)?;
    let profile = config.join("profile.json");
    let original = br#"{"version":1,"id":"existing"}"#;
    fs::write(&profile, original)?;
    let before_profile = fs::metadata(&profile)?;
    let tools = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools");
    let script = r#"
import importlib.util
import sys
from pathlib import Path
tools, home = Path(sys.argv[1]), Path(sys.argv[2])
sys.path.insert(0, str(tools))
spec = importlib.util.spec_from_file_location("profile_policy", tools / "profile-policy.py")
profile = importlib.util.module_from_spec(spec)
spec.loader.exec_module(profile)
profile.selected = lambda: dict(profile.DEFAULT)
profile.lua = lambda value: "-- controlled fixture\n"
profile.install(home)
"#;
    let output = Command::new("python3")
        .arg("-c")
        .arg(script)
        .arg(tools)
        .arg(home.path())
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Nacre destination busy or unsafe"),
        "installer failed for an unrelated reason: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let after_profile = fs::metadata(&profile)?;
    assert_eq!(fs::read(&profile)?, original);
    assert_eq!(
        (before_profile.dev(), before_profile.ino()),
        (after_profile.dev(), after_profile.ino())
    );
    let after_lock = fs::metadata(&lock)?;
    assert_eq!(
        (before_lock.dev(), before_lock.ino()),
        (after_lock.dev(), after_lock.ino()),
        "installer replaced the native lease file"
    );
    Ok(())
}

#[test]
fn legacy_ui_fallback_requires_a_proven_undispatched_call() -> Result<()> {
    // `nacre settings PAGE` is the provider-neutral route; QML's legacy handler is `settingsApp open`.
    let cases: [(&[&str], &str); 2] = [
        (
            &["nacre", "launcher", ""],
            "-c nacre ipc call nacre launcher  ",
        ),
        (
            &["nacre", "settings", "sound"],
            "-c nacre ipc call settingsApp open sound ",
        ),
    ];
    for (dispatched, (route, recorded)) in cases
        .iter()
        .flat_map(|case| [false, true].map(|dispatched| (dispatched, *case)))
    {
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
        fs::write(
            &binary,
            "#!/bin/sh\nprintf '%s ' \"$@\" > \"$FALLBACK_MARKER\"\n",
        )?;
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o700))?;
        let marker = home.path().join("fallback-ran");
        let mut call = command(home.path(), &home.path().join("niri.sock"), &endpoint);
        call.env_remove("NACRE_DAEMON_SOCKET")
            .env("FALLBACK_MARKER", &marker);
        let result = call.args(["ctl", "ui"]).args(route).output()?;
        assert_eq!(result.status.success(), !dispatched);
        assert_eq!(marker.exists(), !dispatched);
        if !dispatched {
            assert_eq!(fs::read_to_string(&marker)?, recorded);
        }
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

#[test]
fn network_watch_shares_signals_preserves_frozen_lines_and_fails_without_replay() -> Result<()> {
    use std::os::unix::process::ExitStatusExt;
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/network-watch-reference.json"))?;
    let home = tempfile::Builder::new().prefix("nw-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let endpoint = home.path().join("daemon.sock");
    let mut bus = Process(
        Command::new("/usr/bin/dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .arg(format!(
                "--address=unix:path={}",
                home.path().join("bus").display()
            ))
            .env_clear()
            .env("HOME", home.path())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().ok_or("Missing private bus")?).read_line(&mut address)?;
    let address = address.trim();
    let server = zbus::blocking::connection::Builder::address(address)?.build()?;
    let mut absent = command(
        home.path(),
        &source_path,
        &home.path().join("absent-daemon.sock"),
    );
    let mut absent = Process(
        absent
            .args(["network-state", "watch"])
            .env("DBUS_SYSTEM_BUS_ADDRESS", address)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    assert!(!absent
        .0
        .wait_timeout(Duration::from_secs(2))?
        .ok_or("Explicit missing daemon silently started a direct watcher")?
        .success());
    let notify = home.path().join("notify.sock");
    let notifications = UnixDatagram::bind(&notify)?;
    notifications.set_read_timeout(Some(Duration::from_secs(10)))?;
    let start_daemon = || -> Result<Process> {
        let mut cmd = command(home.path(), &source_path, &endpoint);
        let process = Process(
            cmd.arg("daemon")
                .env("NOTIFY_SOCKET", &notify)
                .env("DBUS_SYSTEM_BUS_ADDRESS", address)
                .stdout(Stdio::null())
                .stderr(
                    fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(home.path().join("network-daemon.log"))?,
                )
                .spawn()?,
        );
        let mut packet = [0; 256];
        let size = notifications.recv(&mut packet)?;
        assert!(std::str::from_utf8(&packet[..size])?.contains("READY=1"));
        Ok(process)
    };
    let mut daemon = start_daemon()?;
    let start_watch = |name: &str, fallback: bool| -> Result<(Process, BufReader<ChildStdout>)> {
        let mut cmd = command(home.path(), &source_path, &endpoint);
        if fallback {
            cmd.env_remove("NACRE_DAEMON_SOCKET")
                .env("DBUS_SYSTEM_BUS_ADDRESS", address);
        }
        // Explicit clients have an absent system bus. A successful refresh proves
        // they share the daemon rather than establishing duplicate signal sources.
        let mut process = Process(
            cmd.args(["network-state", "watch"])
                .stdout(Stdio::piped())
                .stderr(fs::File::create(home.path().join(name))?)
                .spawn()?,
        );
        let mut reader = BufReader::new(
            process
                .0
                .stdout
                .take()
                .ok_or("Missing network watch output")?,
        );
        assert_eq!(
            next_line_context(&mut reader, &format!("{name} startup"))?,
            reference["startup_stdout"].as_str().unwrap().as_bytes()
        );
        Ok((process, reader))
    };
    let (mut first, mut first_read) = start_watch("network-first.log", false)?;
    let (mut second, mut second_read) = start_watch("network-second.log", false)?;
    let (mut standalone, mut standalone_read) = start_watch("network-standalone.log", true)?;
    server.request_name("org.freedesktop.NetworkManager")?;
    assert!(server.release_name("org.freedesktop.NetworkManager")?);
    for (name, reader) in [
        ("first", &mut first_read),
        ("second", &mut second_read),
        ("standalone", &mut standalone_read),
    ] {
        let mut lines = next_line_context(reader, &format!("{name} owner acquire"))?;
        lines.extend(next_line_context(reader, &format!("{name} owner release"))?);
        assert_eq!(
            lines,
            reference["owner_change_stdout"]
                .as_str()
                .unwrap()
                .as_bytes()
        );
    }
    nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(standalone.0.id() as i32),
        nix::sys::signal::Signal::SIGTERM,
    )?;
    let exit = standalone
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Standalone watcher did not stop")?;
    assert_eq!(
        exit.signal(),
        Some(-reference["termination_exit"].as_i64().unwrap() as i32)
    );
    assert_eq!(
        fs::read_to_string(home.path().join("network-standalone.log"))?,
        reference["stderr"].as_str().unwrap()
    );
    drop(standalone_read);
    // Closing a pipe with no new events must release an idle compatibility link.
    drop(first_read);
    assert!(first
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Idle network client retained its pipe")?
        .success());
    server.request_name("org.freedesktop.NetworkManager")?;
    server.emit_signal(
        None::<&str>,
        "/org/freedesktop/NetworkManager",
        "org.freedesktop.NetworkManager",
        "DeviceAdded",
        &(zbus::zvariant::OwnedObjectPath::try_from(
            "/org/freedesktop/NetworkManager/Devices/1",
        )?,),
    )?;
    assert!(server.release_name("org.freedesktop.NetworkManager")?);
    for _ in 0..3 {
        assert_eq!(
            next_line_context(&mut second_read, "second device event")?,
            b"changed\n"
        );
    }
    // A lost dispatched stream cannot silently start a second direct watcher.
    daemon.0.kill()?;
    daemon.0.wait()?;
    assert!(!second
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Network client did not notice daemon death")?
        .success());
    drop(second_read);
    let mut daemon = start_daemon()?;
    let (mut third, third_read) = start_watch("network-third.log", false)?;
    bus.0.kill()?;
    bus.0.wait()?;
    assert!(!third
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Network client did not notice service failure")?
        .success());
    drop(third_read);
    assert!(daemon.0.try_wait()?.is_none());
    assert_eq!(
        status(home.path(), &source_path, &endpoint)?["services"]["compositor"]["status"],
        "running"
    );
    for name in [
        "network-daemon.log",
        "network-first.log",
        "network-second.log",
        "network-third.log",
    ] {
        assert!(!fs::read_to_string(home.path().join(name))?.contains("DO_NOT_LOG_WINDOW_TITLE"));
    }
    Ok(())
}

#[test]
fn wallpaper_watch_shares_inotify_preserves_frozen_lines_and_never_replays_after_disconnect(
) -> Result<()> {
    let home = tempfile::Builder::new().prefix("ww-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let endpoint = home.path().join("daemon.sock");
    let notify = UnixDatagram::bind(home.path().join("notify.sock"))?;
    notify.set_read_timeout(Some(Duration::from_secs(10)))?;
    let config = home.path().join("config/nacre");
    let installed = home
        .path()
        .join(".local/share/nacre/shell/tools/nacre-helper");
    let bridge = home.path().join(".local/share/nacre/shell/bin/nacre_shell");
    for path in [
        config.as_path(),
        installed.parent().unwrap(),
        bridge.parent().unwrap(),
    ] {
        fs::create_dir_all(path)?;
    }
    fs::write(&installed, "fixture helper")?;
    fs::write(&bridge, "fixture bridge")?;
    let log = home.path().join("daemon.log");
    let start_daemon = || -> Result<Process> {
        let process = Process(
            command(home.path(), &source_path, &endpoint)
                .arg("daemon")
                .env("NOTIFY_SOCKET", home.path().join("notify.sock"))
                .stderr(
                    fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&log)?,
                )
                .spawn()?,
        );
        notify.recv(&mut [0; 256])?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while status(home.path(), &source_path, &endpoint)?["services"]["wallpaper-watch"]["status"]
            != "running"
        {
            if Instant::now() >= deadline {
                return Err("Wallpaper source did not become ready".into());
            }
            thread::sleep(Duration::from_millis(10));
        }
        Ok(process)
    };
    let mut daemon = start_daemon()?;
    let start_watch = |engine: Option<&Path>| -> Result<(Process, BufReader<ChildStdout>)> {
        let mut cmd = command(home.path(), &source_path, &endpoint);
        cmd.arg("wallpaper-watch");
        if let Some(engine) = engine {
            cmd.arg("--engine").arg(engine);
        }
        let mut child = Process(cmd.stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?);
        let reader = BufReader::new(child.0.stdout.take().ok_or("Missing wallpaper output")?);
        Ok((child, reader))
    };
    let (mut first, mut first_read) = start_watch(None)?;
    let (mut second, mut second_read) = start_watch(Some(&installed))?;
    let quiet = |reader: &BufReader<ChildStdout>| -> Result<()> {
        assert!(
            reader.buffer().is_empty(),
            "Unexpected buffered wallpaper line"
        );
        let mut fds = [rustix::event::PollFd::new(
            reader.get_ref(),
            rustix::event::PollFlags::IN,
        )];
        assert_eq!(
            rustix::event::poll(
                &mut fds,
                Some(&rustix::event::Timespec {
                    tv_sec: 0,
                    tv_nsec: 800_000_000
                })
            )?,
            0,
            "Unexpected wallpaper line"
        );
        Ok(())
    };
    quiet(&first_read)?;
    quiet(&second_read)?;
    // A thin client has no inotify descriptor; only the daemon owns subscriptions.
    for child in [&first, &second] {
        for entry in fs::read_dir(format!("/proc/{}/fdinfo", child.0.id()))? {
            assert!(!fs::read_to_string(entry?.path())?
                .lines()
                .any(|line| line.starts_with("inotify ")));
        }
    }
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/wallpaper-watch-reference.json"))?;
    let expected = |name: &str| -> Result<Vec<u8>> {
        Ok(reference["dialogue"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|step| step["name"] == name)
            .ok_or("Missing frozen step")?["stdout"]
            .as_str()
            .unwrap()
            .as_bytes()
            .to_vec())
    };
    let root = home.path().join("Pictures/Wallpapers");
    let scheme = home.path().join("state/nacre/scheme.json");
    fs::write(root.join("ignored.txt"), "ignored")?;
    fs::write(&scheme, r#"{"mode":"dark","colours":{}}"#)?;
    quiet(&first_read)?;
    quiet(&second_read)?;
    fs::write(&scheme, r#"{"mode":"light"}"#)?;
    assert_eq!(next_line(&mut first_read)?, expected("changed-mode")?);
    assert_eq!(next_line(&mut second_read)?, expected("changed-mode")?);
    fs::write(
        &scheme,
        r#"{"mode":"light","colours":{"primary":"ffffff"}}"#,
    )?;
    quiet(&first_read)?;
    quiet(&second_read)?;
    fs::write(config.join("wallpaper-picker.json"), "{}")?;
    assert_eq!(next_line(&mut first_read)?, expected("picker-preference")?);
    assert_eq!(next_line(&mut second_read)?, expected("picker-preference")?);
    fs::write(&bridge, "bridge replacement")?;
    assert_eq!(next_line(&mut first_read)?, expected("engine-replacement")?);
    quiet(&second_read)?;
    fs::write(&installed, "helper replacement")?;
    assert_eq!(
        next_line(&mut second_read)?,
        expected("engine-replacement")?
    );
    quiet(&first_read)?;
    let nested = root.join("collection");
    fs::create_dir(&nested)?;
    // Match the frozen dialogue: wait for the new directory subscription before
    // creating its file. IN_CREATE itself deliberately does not emit a change.
    use std::os::unix::fs::MetadataExt;
    let inode = fs::metadata(&nested)?.ino();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let mut watched = false;
        for entry in fs::read_dir(format!("/proc/{}/fdinfo", daemon.0.id()))? {
            let info = match fs::read_to_string(entry?.path()) {
                Ok(info) => info,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error.into()),
            };
            watched |= info
                .lines()
                .filter(|line| line.starts_with("inotify "))
                .flat_map(str::split_whitespace)
                .filter_map(|field| field.strip_prefix("ino:"))
                .any(|hex| u64::from_str_radix(hex, 16).ok() == Some(inode));
        }
        if watched {
            break;
        }
        if Instant::now() >= deadline {
            return Err("Nested wallpaper subscription timed out".into());
        }
        thread::sleep(Duration::from_millis(10));
    }
    for text in ["first", "second", "final"] {
        fs::write(nested.join("scene.PNG"), text)?;
    }
    assert_eq!(next_line(&mut first_read)?, expected("nested-burst")?);
    assert_eq!(next_line(&mut second_read)?, expected("nested-burst")?);
    quiet(&first_read)?;
    quiet(&second_read)?;
    let outside = home.path().join("outside");
    fs::create_dir(&outside)?;
    symlink(&outside, root.join("alias"))?;
    fs::write(outside.join("scene.jpg"), "not followed")?;
    quiet(&first_read)?;
    quiet(&second_read)?;
    let invalid_root = home.path().join("must-not-create");
    let mut rejected = Process(
        command(home.path(), &source_path, &endpoint)
            .arg("wallpaper-watch")
            .arg("--root")
            .arg(&invalid_root)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    assert!(!rejected
        .0
        .wait_timeout(Duration::from_secs(3))?
        .ok_or("Mismatched explicit source stayed alive")?
        .success());
    assert!(!invalid_root.exists());
    daemon.0.kill()?;
    daemon.0.wait()?;
    for child in [&mut first, &mut second] {
        assert!(!child
            .0
            .wait_timeout(Duration::from_secs(5))?
            .ok_or("Disconnected wallpaper client stayed alive")?
            .success());
    }
    let _restarted = start_daemon()?;
    let (mut closed, closed_read) = start_watch(None)?;
    drop(closed_read);
    assert!(closed
        .0
        .wait_timeout(Duration::from_secs(5))?
        .ok_or("Closed wallpaper pipe stayed alive")?
        .success());
    assert!(!fs::read_to_string(log)?.contains("DO_NOT_LOG_WINDOW_TITLE"));
    Ok(())
}

#[test]
fn wallpaper_active_publishes_atomic_pair_and_retains_stale_revision() -> Result<()> {
    use nacre_protocol::{wallpaper::WallpaperActive, Message, Mirror, Reply, Update};

    let home = tempfile::Builder::new().prefix("wa-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let private = home.path().join("private");
    fs::create_dir(&private)?;
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
    let endpoint = private.join("daemon.sock");
    let notify_path = home.path().join("notify.sock");
    let notify = UnixDatagram::bind(&notify_path)?;
    notify.set_read_timeout(Some(Duration::from_secs(10)))?;

    let state_dir = home.path().join("state/nacre");
    let config = home.path().join("config/nacre");
    let pictures = home.path().join("Pictures");
    let installed = home
        .path()
        .join(".local/share/nacre/shell/tools/nacre-helper");
    let bridge = home.path().join(".local/share/nacre/shell/bin/nacre_shell");
    for dir in [
        state_dir.as_path(),
        config.as_path(),
        installed.parent().unwrap(),
        bridge.parent().unwrap(),
        pictures.as_path(),
    ] {
        fs::create_dir_all(dir)?;
    }
    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o700))?;
    fs::write(&installed, "fixture helper")?;
    fs::write(&bridge, "fixture bridge")?;
    let image_a = pictures.join("a.png");
    let image_b = pictures.join("b.png");
    image::DynamicImage::new_rgb8(2, 2).save(&image_a)?;
    image::DynamicImage::new_rgb8(2, 2).save(&image_b)?;
    let mut record: Value = serde_json::from_str(include_str!(
        "../../../nacre/shell-tools/reference-style.json"
    ))?;
    record["mode"] = json!("dark");
    record["poster"] = json!(image_a);
    let presentation = state_dir.join("presentation.json");
    fs::write(&presentation, serde_json::to_vec(&record)?)?;
    let scheme = state_dir.join("scheme.json");
    fs::write(&scheme, r#"{"mode":"dark"}"#)?;

    let log = home.path().join("daemon.log");
    let mut daemon = Process(
        command(home.path(), &source_path, &endpoint)
            .arg("daemon")
            .env("NOTIFY_SOCKET", &notify_path)
            .stderr(
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log)?,
            )
            .spawn()?,
    );
    notify.recv(&mut [0; 256])?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while status(home.path(), &source_path, &endpoint)?["services"]["wallpaper-watch"]["status"]
        != "running"
    {
        if Instant::now() >= deadline {
            return Err("Wallpaper source did not become ready".into());
        }
        thread::sleep(Duration::from_millis(10));
    }

    let stream = UnixStream::connect(&endpoint)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    let mut reader = BufReader::new(stream);
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"t":"hello","proto":1,"client":"active-wallpaper-fixture"})
    )?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let welcome: Message = serde_json::from_str(&line)?;
    assert!(
        matches!(welcome, Message::Welcome { ref topics, ref methods, .. } if topics.contains_key("wallpaper.active") && methods.contains_key("wallpaper.activate"))
    );
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"t":"sub","topics":["wallpaper.active"]})
    )?;
    let mut mirror = Mirror::default();
    mirror.receive(&welcome);
    let read_active =
        |reader: &mut BufReader<UnixStream>, mirror: &mut Mirror| -> Result<WallpaperActive> {
            loop {
                let mut line = String::new();
                reader.read_line(&mut line)?;
                let message: Message = serde_json::from_str(&line)?;
                let update = mirror.receive(&message);
                if matches!(update, Update::Changed(ref topic) if topic == "wallpaper.active") {
                    let state: WallpaperActive =
                        serde_json::from_value(mirror.topics["wallpaper.active"].1.clone())?;
                    state.validate()?;
                    return Ok(state);
                }
            }
        };
    let call = |reader: &mut BufReader<UnixStream>, id: u64, revision: u64| -> Result<Reply> {
        writeln!(
            reader.get_mut(),
            "{}",
            json!({"t":"call","id":id,"m":"wallpaper.activate","p":{"revision":revision}})
        )?;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line)?;
            if let Message::Ret { reply, .. } = serde_json::from_str::<Message>(&line)? {
                return Ok(reply);
            }
        }
    };
    let active = read_active(&mut reader, &mut mirror)?;
    assert!(active.available && !active.stale);
    assert_eq!(
        active.current.as_ref().unwrap().presentation,
        nacre_protocol::wallpaper::Presentation::Crop
    );
    assert_eq!(active.outputs.len(), 1);
    assert!(matches!(
        call(&mut reader, 1, active.revision)?,
        Reply::Ok { .. }
    ));

    record["mode"] = json!("light");
    record["poster"] = json!(image_b);
    fs::write(&presentation, serde_json::to_vec(&record)?)?;
    let changed = read_active(&mut reader, &mut mirror)?;
    assert!(changed.available && changed.revision > active.revision);
    assert_eq!(
        changed.current.as_ref().unwrap().palette.mode,
        nacre_protocol::wallpaper::PaletteMode::Light
    );
    assert!(matches!(
        call(&mut reader, 2, changed.revision)?,
        Reply::Ok { .. }
    ));

    fs::write(&presentation, b"invalid")?;
    let stale = read_active(&mut reader, &mut mirror)?;
    assert!(!stale.available && stale.stale);
    assert_eq!(stale.current, changed.current);
    assert!(matches!(
        call(&mut reader, 3, stale.revision)?,
        Reply::Err { .. }
    ));
    fs::write(&presentation, serde_json::to_vec(&record)?)?;
    let recovered = read_active(&mut reader, &mut mirror)?;
    assert!(recovered.available && !recovered.stale);
    assert_eq!(recovered.revision, changed.revision);
    fs::remove_file(&image_b)?;
    let removed = read_active(&mut reader, &mut mirror)?;
    assert!(!removed.available && removed.stale);
    assert_eq!(removed.current, recovered.current);
    daemon.0.kill()?;
    daemon.0.wait()?;
    Ok(())
}

#[test]
fn ai_chat_is_hosted_by_a_worker_and_published_as_a_bounded_snapshot() -> Result<()> {
    use nacre_protocol::{ai_chat::AiChat, Message, Mirror, Reply};

    let home = tempfile::Builder::new().prefix("ac-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let private = home.path().join("private");
    fs::create_dir(&private)?;
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
    let endpoint = private.join("daemon.sock");
    let runtime = home.path().join("runtime");
    fs::create_dir(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    let notify_path = home.path().join("notify.sock");
    let notify = UnixDatagram::bind(&notify_path)?;
    notify.set_read_timeout(Some(Duration::from_secs(10)))?;
    let state_dir = home.path().join("state/nacre");
    let config = home.path().join("config/nacre");
    let installed = home
        .path()
        .join(".local/share/nacre/shell/tools/nacre-helper");
    let bridge = home.path().join(".local/share/nacre/shell/bin/nacre_shell");
    let bin = home.path().join(".local/bin");
    let ai = home.path().join(".config/nacre/ai");
    for dir in [
        state_dir.as_path(),
        config.as_path(),
        installed.parent().unwrap(),
        bridge.parent().unwrap(),
        bin.as_path(),
        ai.as_path(),
    ] {
        fs::create_dir_all(dir)?;
    }
    fs::set_permissions(&state_dir, fs::Permissions::from_mode(0o700))?;
    fs::write(&installed, "fixture helper")?;
    fs::write(&bridge, "fixture bridge")?;
    // Synthetic Claude: one streamed answer per invocation, no real credentials.
    let provider = r#"#!/usr/bin/python3
import json, sys
args = sys.argv[1:]
sys.stdin.read()
flag = '--resume' if '--resume' in args else '--session-id'
session = args[args.index(flag) + 1]
def emit(row):
    sys.stdout.write(json.dumps(row) + '\n')
    sys.stdout.flush()
emit({'type':'stream_event','event':{'type':'message_start','message':{'id':'fixture-answer'}}})
emit({'type':'stream_event','event':{'type':'content_block_delta','delta':{'type':'text_delta','text':'daemon hosted answer'}}})
emit({'type':'assistant','message':{'content':[{'type':'text','text':'daemon hosted answer'}]},'session_id':session})
"#;
    for (name, body) in [
        ("claude", provider),
        ("nacre-ai", "#!/bin/sh\nexit 0\n"),
        ("nacre-ai-claude", "#!/bin/sh\nexit 0\n"),
    ] {
        fs::write(bin.join(name), body)?;
        fs::set_permissions(bin.join(name), fs::Permissions::from_mode(0o700))?;
    }
    fs::write(
        ai.join("settings.json"),
        r#"{"default_assistant":"claude","version":1}"#,
    )?;
    fs::write(
        home.path().join(".config/nacre/profile.json"),
        r#"{"aiFullAccess":true}"#,
    )?;

    let log = home.path().join("daemon.log");
    // Cargo shows captured stderr only when the test fails.
    struct Dump(PathBuf);
    impl Drop for Dump {
        fn drop(&mut self) {
            eprintln!(
                "daemon log:\n{}",
                fs::read_to_string(&self.0).unwrap_or_default()
            );
        }
    }
    let _dump = Dump(log.clone());
    let mut daemon = Process(
        command(home.path(), &source_path, &endpoint)
            .arg("daemon")
            .env("NOTIFY_SOCKET", &notify_path)
            .env("PATH", "/usr/bin:/bin")
            .stderr(
                fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&log)?,
            )
            .spawn()?,
    );
    notify.recv(&mut [0; 256])?;

    let stream = UnixStream::connect(&endpoint)?;
    stream.set_read_timeout(Some(Duration::from_secs(20)))?;
    let mut reader = BufReader::new(stream);
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"t":"hello","proto":1,"client":"ai-chat-fixture"})
    )?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let welcome: Message = serde_json::from_str(&line)?;
    assert!(
        matches!(welcome, Message::Welcome { ref topics, ref methods, .. } if topics.contains_key("ai.chat") && methods.contains_key("ai.chat.send") && methods.contains_key("ai.chat.open"))
    );
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"t":"sub","topics":["ai.chat"]})
    )?;
    let mut mirror = Mirror::default();
    mirror.receive(&welcome);
    // Every frame goes through the mirror, so a state change that arrives
    // while a call is waiting for its reply is never missed.
    let pump = |reader: &mut BufReader<UnixStream>, mirror: &mut Mirror| -> Result<Message> {
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let message: Message = serde_json::from_str(&line)?;
        mirror.receive(&message);
        Ok(message)
    };
    let next_state = |reader: &mut BufReader<UnixStream>,
                      mirror: &mut Mirror,
                      done: &dyn Fn(&AiChat) -> bool|
     -> Result<AiChat> {
        loop {
            if let Some((_, value)) = mirror.topics.get("ai.chat") {
                let chat: AiChat = serde_json::from_value(value.clone())?;
                chat.validate()?;
                if done(&chat) {
                    return Ok(chat);
                }
            }
            pump(reader, mirror)?;
        }
    };
    let call = |reader: &mut BufReader<UnixStream>,
                mirror: &mut Mirror,
                id: u64,
                method: &str,
                params: Value|
     -> Result<Reply> {
        writeln!(
            reader.get_mut(),
            "{}",
            json!({"t":"call","id":id,"m":method,"p":params})
        )?;
        loop {
            if let Message::Ret { reply, .. } = pump(reader, mirror)? {
                return Ok(reply);
            }
        }
    };
    let empty = next_state(&mut reader, &mut mirror, &|chat| chat.available)?;
    assert!(empty.channels.is_empty());
    assert!(matches!(
        call(&mut reader, &mut mirror, 1, "ai.chat.send", json!({"channel":"p1","text":"early"}))?,
        Reply::Err { ref err } if err.dispatched == Some(false)
    ));
    assert!(matches!(
        call(
            &mut reader,
            &mut mirror,
            2,
            "ai.chat.open",
            json!({"channel":"p1"})
        )?,
        Reply::Ok { .. }
    ));
    let opened = next_state(&mut reader, &mut mirror, &|chat| {
        chat.channels.contains_key("p1")
    })?;
    assert_eq!(
        opened.channels["p1"].state.provider,
        nacre_protocol::ai_chat::Provider::Claude
    );
    // The fixture providers list nothing; the call still answers with a valid list.
    match call(
        &mut reader,
        &mut mirror,
        9,
        "ai.chat.saved",
        json!({"channel":"p1"}),
    )? {
        Reply::Ok { ok } => nacre_protocol::ai_chat::AiSavedList::validate(
            &serde_json::from_value(ok).expect("saved list shape"),
        )?,
        other => panic!("saved failed: {other:?}"),
    }
    assert!(matches!(
        call(
            &mut reader,
            &mut mirror,
            3,
            "ai.chat.send",
            json!({"channel":"p1","text":"hello daemon"})
        )?,
        Reply::Ok { .. }
    ));
    let answered = next_state(&mut reader, &mut mirror, &|chat| {
        chat.channels.get("p1").is_some_and(|c| {
            !c.state.busy
                && c.messages
                    .values()
                    .any(|m| m.text == "daemon hosted answer")
                && c.messages.values().any(|m| m.text == "hello daemon")
        })
    })?;
    let mut order: Vec<_> = answered.channels["p1"]
        .messages
        .values()
        .map(|m| m.text.as_str())
        .collect();
    order.truncate(2);
    assert_eq!(order, ["hello daemon", "daemon hosted answer"]);
    daemon.0.kill()?;
    daemon.0.wait()?;
    Ok(())
}

#[test]
fn i18n_topic_and_catalog_follow_the_desktop_language_setting() -> Result<()> {
    let home = tempfile::Builder::new().prefix("i18n-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let private = home.path().join("private");
    fs::create_dir(&private)?;
    fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
    let endpoint = private.join("daemon.sock");
    let config = home.path().join("config/nacre");
    let tools = home.path().join("tree/tools");
    let catalog = home.path().join("tree/shell/i18n");
    for directory in [&config, &tools, &catalog] {
        fs::create_dir_all(directory)?;
    }
    let desktop = config.join("desktop.json");
    fs::write(&desktop, br#"{"version":3,"language":"en"}"#)?;
    fs::write(
        catalog.join("en.json"),
        br#"{"bead.bar.clock":"Clock {zone}","bead.panel.windows":{"one":"{count} window","other":"{count} windows"},"launcher.search":"Search"}"#,
    )?;
    let notify = home.path().join("notify.sock");
    let notifications = UnixDatagram::bind(&notify)?;
    notifications.set_read_timeout(Some(Duration::from_secs(10)))?;
    let _daemon = Process(
        command(home.path(), &source_path, &endpoint)
            .arg("daemon")
            .env("NACRE_DEVELOPER_MODE", "1")
            .env("NACRE_HELPER_TOOLS", &tools)
            .env("NOTIFY_SOCKET", &notify)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?,
    );
    notifications.recv(&mut [0; 256])?;
    let call = |method: &str, params: Value| -> Result<Option<Value>> {
        let output = command(home.path(), &source_path, &endpoint)
            .args(["ctl", "call", method, &params.to_string()])
            .output()?;
        Ok(output
            .status
            .success()
            .then(|| serde_json::from_slice(&output.stdout))
            .transpose()?)
    };
    let until = |what: &str, mut check: Box<dyn FnMut() -> Result<bool> + '_>| -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !check()? {
            if Instant::now() > deadline {
                return Err(format!("Timed out waiting for {what}").into());
            }
            thread::sleep(Duration::from_millis(20));
        }
        Ok(())
    };
    let mut english = None;
    until(
        "the English catalog",
        Box::new(|| {
            english = call("i18n.catalog", json!({"namespace":"bead","revision":1}))?;
            Ok(english.is_some())
        }),
    )?;
    let english = english.unwrap();
    assert_eq!(english["revision"], 1);
    assert_eq!(english["entries"]["bead.bar.clock"], "Clock {zone}");
    assert_eq!(
        english["entries"]["bead.panel.windows"]["other"],
        "{count} windows"
    );
    assert!(english["entries"].get("launcher.search").is_none() && english.get("next").is_none());

    fs::write(&desktop, br#"{"version":3,"language":"pseudo"}"#)?;
    until(
        "the revision bump",
        Box::new(|| {
            Ok(
                call("i18n.catalog", json!({"namespace":"bead","revision":1}))?.is_none()
                    && call("i18n.catalog", json!({"namespace":"bead","revision":2}))?.is_some(),
            )
        }),
    )?;
    let pseudo = call("i18n.catalog", json!({"namespace":"bead"}))?.unwrap();
    assert_eq!(pseudo["revision"], 2);
    let clock = pseudo["entries"]["bead.bar.clock"].as_str().unwrap();
    assert!(
        clock.starts_with('[') && clock.contains("{zone}"),
        "{clock}"
    );
    for bad in [
        json!({"namespace":"../x"}),
        json!({"namespace":"bead","extra":1}),
        json!({"namespace":"bead","revision":0}),
    ] {
        assert!(call("i18n.catalog", bad)?.is_none());
    }
    Ok(())
}

/// N0053: readiness (sd_notify READY and the socket) never waits on an external backend. Every
/// command the probes run hangs forever and both D-Bus buses accept a connection and never reply.
#[test]
fn daemon_ready_within_two_seconds_while_every_external_backend_hangs() -> Result<()> {
    let home = tempfile::Builder::new().prefix("nh-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let folder = home.path().join("private");
    fs::create_dir(&folder)?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let endpoint = folder.join("daemon.sock");
    let bin = home.path().join("hang-bin");
    fs::create_dir(&bin)?;
    for name in [
        "powerprofilesctl",
        "pactl",
        "wpctl",
        "rfkill",
        "ddcutil",
        "brightnessctl",
        "systemctl",
        "timedatectl",
        "localectl",
        "loginctl",
        "busctl",
        "nmcli",
        "pacman",
        "checkupdates",
    ] {
        let path = bin.join(name);
        fs::write(&path, "#!/bin/sh\nexec /usr/bin/sleep 3000\n")?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755))?;
    }
    let audio_started = home.path().join("pactl-started");
    fs::write(
        bin.join("pactl"),
        format!(
            "#!/bin/sh\necho started > '{}'\nexec /usr/bin/sleep 3000\n",
            audio_started.display()
        ),
    )?;
    let check_started = home.path().join("checkupdates-started");
    fs::write(
        bin.join("checkupdates"),
        format!(
            "#!/bin/sh\necho started > '{}'\nexec /usr/bin/sleep 3000\n",
            check_started.display()
        ),
    )?;
    // A bus that accepts and never answers the authentication.
    let mut silent = Vec::new();
    let mut held = Vec::new();
    for name in ["session", "system"] {
        let listener = UnixListener::bind(home.path().join(format!("silent-{name}")))?;
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        listener.set_nonblocking(true)?;
        held.push(thread::spawn(move || {
            let mut streams = Vec::new();
            while !done.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => streams.push(stream),
                    Err(_) => thread::sleep(Duration::from_millis(20)),
                }
            }
        }));
        silent.push(stop);
    }
    // N0057: a local HTTP proxy accepts HTTPS CONNECT and never replies. The real
    // weather HTTP stack must hang off-thread, without making a real network request.
    let proxy = std::net::TcpListener::bind("127.0.0.1:0")?;
    let proxy_address = format!("http://{}", proxy.local_addr()?);
    proxy.set_nonblocking(true)?;
    let weather_started = Arc::new(AtomicBool::new(false));
    let weather_seen = weather_started.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let done = stop.clone();
    held.push(thread::spawn(move || {
        let mut streams = Vec::new();
        while !done.load(Ordering::Relaxed) {
            match proxy.accept() {
                Ok((stream, _)) => {
                    weather_seen.store(true, Ordering::Relaxed);
                    streams.push(stream);
                }
                Err(_) => thread::sleep(Duration::from_millis(10)),
            }
        }
    }));
    silent.push(stop);
    let config = home.path().join("config/nacre");
    fs::create_dir_all(&config)?;
    fs::write(
        config.join("desktop.json"),
        br#"{"weatherLocation":"Fixture city","weatherIpLookup":false}"#,
    )?;
    let notify = home.path().join("notify.sock");
    let notifications = UnixDatagram::bind(&notify)?;
    notifications.set_read_timeout(Some(Duration::from_secs(2)))?;
    let log = home.path().join("daemon.log");
    let started = Instant::now();
    let mut child = command(home.path(), &source_path, &endpoint);
    child
        .arg("daemon")
        .env("NOTIFY_SOCKET", &notify)
        .env("HTTPS_PROXY", &proxy_address)
        .env("https_proxy", &proxy_address)
        .env("ALL_PROXY", &proxy_address)
        .env("NO_PROXY", "")
        .env("PATH", &bin)
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", home.path().join("silent-session").display()),
        )
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", home.path().join("silent-system").display()),
        )
        .stdout(Stdio::null())
        .stderr(fs::File::create(&log)?);
    let _child = Process(child.spawn()?);
    let mut packet = [0; 256];
    let size = notifications
        .recv(&mut packet)
        .map_err(|_| "No READY within two seconds while backends hang")?;
    assert!(std::str::from_utf8(&packet[..size])?.contains("READY=1"));
    assert!(started.elapsed() < Duration::from_secs(2));
    let deadline = Instant::now() + Duration::from_secs(2);
    while !check_started.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        check_started.exists(),
        "The hanging update check must actually start"
    );
    let deadline = Instant::now() + Duration::from_secs(2);
    while !weather_started.load(Ordering::Relaxed) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        weather_started.load(Ordering::Relaxed),
        "Hanging weather HTTP must actually start"
    );
    let value = status(home.path(), &source_path, &endpoint)?;
    assert_eq!(value["services"]["compositor"]["status"], "running");
    for service in ["capabilities", "controls"] {
        assert_ne!(
            value["services"][service]["status"], "running",
            "{service} must not claim health while its backend hangs"
        );
    }
    // N0059: leasing audio starts the same hung pactl used by OSD. All three
    // topics remain honest unavailable snapshots, and the hub still serves status.
    let raw = UnixStream::connect(&endpoint)?;
    raw.set_read_timeout(Some(Duration::from_secs(2)))?;
    let mut reader = BufReader::new(raw);
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"t":"hello","proto":1,"client":"hung-system-state"})
    )?;
    let mut line = String::new();
    reader.read_line(&mut line)?;
    writeln!(
        reader.get_mut(),
        "{}",
        json!({"t":"sub","topics":["audio","battery","power.profiles"]})
    )?;
    let mut mirror = nacre_protocol::Mirror::default();
    mirror.receive(&serde_json::from_str::<nacre_protocol::Message>(
        line.trim(),
    )?);
    for _ in 0..3 {
        line.clear();
        reader.read_line(&mut line)?;
        let message: nacre_protocol::Message = serde_json::from_str(line.trim())?;
        mirror.receive(&message);
    }
    for topic in ["audio", "battery", "power.profiles"] {
        assert_eq!(mirror.topics[topic].1["health"]["available"], false);
    }
    let deadline = Instant::now() + Duration::from_secs(8);
    while !audio_started.exists() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
    assert!(
        audio_started.exists(),
        "audio lease must start the hanging pactl"
    );
    let responsive = Instant::now();
    assert_eq!(
        status(home.path(), &source_path, &endpoint)?["services"]["compositor"]["status"],
        "running"
    );
    assert!(responsive.elapsed() < Duration::from_secs(2));
    for stop in silent {
        stop.store(true, Ordering::Relaxed);
    }
    for handle in held {
        let _ = handle.join();
    }
    Ok(())
}

struct HungBattery(Arc<AtomicBool>);
#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl HungBattery {
    #[zbus(property)]
    async fn percentage(&self) -> f64 {
        self.0.store(true, Ordering::Relaxed);
        std::future::pending().await
    }
}

/// A functioning bus with a nonresponding service is distinct from the N0053
/// nonresponding bus-authentication fixture. Neither may delay daemon readiness.
#[tokio::test(flavor = "current_thread")]
async fn daemon_ready_within_two_seconds_while_upower_property_never_replies() -> Result<()> {
    let home = tempfile::Builder::new().prefix("nh-").tempdir()?;
    fs::set_permissions(home.path(), fs::Permissions::from_mode(0o700))?;
    let source_path = home.path().join("niri.sock");
    let _source = Source::new(source_path.clone())?;
    let folder = home.path().join("private");
    fs::create_dir(&folder)?;
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let endpoint = folder.join("daemon.sock");
    let mut bus = Process(
        Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address",
                &format!("--address=unix:path={}", home.path().join("bus").display()),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().ok_or("Missing bus stdout")?).read_line(&mut address)?;
    let address = address.trim();
    let requested = Arc::new(AtomicBool::new(false));
    let _service = zbus::connection::Builder::address(address)?
        .name("org.freedesktop.UPower")?
        .serve_at(
            "/org/freedesktop/UPower/devices/DisplayDevice",
            HungBattery(requested.clone()),
        )?
        .build()
        .await?;
    let notify = home.path().join("notify.sock");
    let notifications = tokio::net::UnixDatagram::bind(&notify)?;
    let started = Instant::now();
    let _child = Process(
        command(home.path(), &source_path, &endpoint)
            .arg("daemon")
            .env("NOTIFY_SOCKET", &notify)
            .env("DBUS_SYSTEM_BUS_ADDRESS", address)
            .env("DBUS_SESSION_BUS_ADDRESS", address)
            // No ambient tools or real devices are discoverable by the fixture.
            .env("PATH", home.path().join("no-tools"))
            .stdout(Stdio::null())
            .stderr(fs::File::create(home.path().join("daemon.log"))?)
            .spawn()?,
    );
    let mut packet = [0; 256];
    let size =
        tokio::time::timeout(Duration::from_secs(2), notifications.recv(&mut packet)).await??;
    assert!(std::str::from_utf8(&packet[..size])?.contains("READY=1"));
    assert!(started.elapsed() < Duration::from_secs(2));
    tokio::time::timeout(Duration::from_secs(2), async {
        while !requested.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await?;
    // This command only connects to the task-private daemon socket.
    assert_eq!(
        status(home.path(), &source_path, &endpoint)?["services"]["compositor"]["status"],
        "running"
    );
    Ok(())
}
