//! N0063 on a private daemon: temporary HOME, a fake niri socket that keeps its own output
//! layout and a fake systemd on a private session bus. No real output, compositor, unit or
//! desktop configuration is ever touched.
use nacre_protocol::{Id, Message, Mirror, Reply, Update};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::PermissionsExt,
        net::{UnixDatagram, UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
type Properties = Vec<(String, zbus::zvariant::OwnedValue)>;

fn outputs() -> Value {
    let modes = |rates: &[u64]| -> Vec<Value> {
        rates
            .iter()
            .map(|rate| json!({"width":1920,"height":1080,"refresh_rate":rate}))
            .collect()
    };
    json!({
        "DP-1":{"name":"DP-1","make":"Dell Inc.","model":"U2415","vrr_supported":false,"vrr_enabled":false,
            "modes":modes(&[60000]),"current_mode":0,
            "logical":{"x":0,"y":0,"width":1920,"height":1080,"scale":1.0,"transform":"Normal"}},
        "HDMI-A-1":{"name":"HDMI-A-1","make":"LG","model":"27GL850","vrr_supported":true,"vrr_enabled":true,
            "modes":modes(&[60000, 143981]),"current_mode":0,
            "logical":{"x":1920,"y":0,"width":1920,"height":1080,"scale":1.0,"transform":"Normal"}}
    })
}

/// Just enough niri IPC: queries, the event stream headers and the `Output` actions the real
/// backend sends. The layout it keeps and every action it saw are what the test inspects.
struct Niri {
    path: PathBuf,
    stop: Arc<AtomicBool>,
    layout: Arc<Mutex<Value>>,
    actions: Arc<Mutex<Vec<Value>>>,
}
impl Niri {
    fn new(path: PathBuf) -> Result<Self> {
        let listener = UnixListener::bind(&path)?;
        let stop = Arc::new(AtomicBool::new(false));
        let layout = Arc::new(Mutex::new(outputs()));
        let actions = Arc::new(Mutex::new(Vec::new()));
        let workspaces = json!([
            {"id":1,"idx":1,"name":"One","output":"DP-1","is_active":true,"is_focused":true,"is_urgent":false,"active_window_id":null},
            {"id":2,"idx":1,"name":"Two","output":"HDMI-A-1","is_active":true,"is_focused":false,"is_urgent":false,"active_window_id":null}
        ]);
        let (done, state, log) = (stop.clone(), layout.clone(), actions.clone());
        thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                if done.load(Ordering::Relaxed) {
                    break;
                }
                let (state, log, workspaces) = (state.clone(), log.clone(), workspaces.clone());
                thread::spawn(move || {
                    let mut reader = BufReader::new(stream);
                    let mut line = String::new();
                    while reader.read_line(&mut line).is_ok_and(|n| n > 0) {
                        let request: Value = serde_json::from_str(&line).unwrap();
                        line.clear();
                        if request == "EventStream" || request == "NacreOverviewHoverStream" {
                            let _ = writeln!(reader.get_mut(), "{}", json!({"Ok":"Handled"}));
                            let _ = writeln!(
                                reader.get_mut(),
                                "{}",
                                json!({"WorkspacesChanged":{"workspaces":workspaces}})
                            );
                            let _ = writeln!(
                                reader.get_mut(),
                                "{}",
                                json!({"WindowsChanged":{"windows":[]}})
                            );
                            let mut byte = [0];
                            let _ = reader.read(&mut byte);
                            break;
                        }
                        let response = if let Some(output) = request.get("Output") {
                            log.lock().unwrap().push(output.clone());
                            apply(&mut state.lock().unwrap(), output);
                            json!({"Ok":{"OutputConfigChanged":"Applied"}})
                        } else {
                            match request.as_str() {
                                Some("Outputs") => {
                                    json!({"Ok":{"Outputs":state.lock().unwrap().clone()}})
                                }
                                Some("Workspaces") => json!({"Ok":{"Workspaces":workspaces}}),
                                Some("Windows") => json!({"Ok":{"Windows":[]}}),
                                _ => json!({"Err":"Unsupported fixture request"}),
                            }
                        };
                        if writeln!(reader.get_mut(), "{response}").is_err() {
                            break;
                        }
                    }
                });
            }
        });
        Ok(Self {
            path,
            stop,
            layout,
            actions,
        })
    }
    fn logical(&self, name: &str, key: &str) -> Value {
        self.layout.lock().unwrap()[name]["logical"][key].clone()
    }
    fn mode(&self, name: &str) -> Value {
        let layout = self.layout.lock().unwrap();
        let row = &layout[name];
        row["modes"][row["current_mode"].as_u64().unwrap() as usize]["refresh_rate"].clone()
    }
    fn actions(&self) -> usize {
        self.actions.lock().unwrap().len()
    }
}
fn apply(layout: &mut Value, output: &Value) {
    let name = output["output"].as_str().unwrap();
    let action = &output["action"];
    let row = &mut layout[name];
    if let Some(mode) = action["Mode"]["mode"]["Specific"].as_object() {
        let (width, height) = (mode["width"].as_u64(), mode["height"].as_u64());
        let refresh = mode["refresh"].as_f64().unwrap();
        let found = row["modes"].as_array().unwrap().iter().position(|m| {
            m["width"].as_u64() == width
                && m["height"].as_u64() == height
                && (m["refresh_rate"].as_f64().unwrap() / 1000.0 - refresh).abs() < 0.01
        });
        row["current_mode"] = json!(found.expect("a listed mode"));
    }
    if let Some(scale) = action["Scale"]["scale"]["Specific"].as_f64() {
        row["logical"]["scale"] = json!(scale);
    }
    if let Some(position) = action["Position"]["position"]["Specific"].as_object() {
        row["logical"]["x"] = position["x"].clone();
        row["logical"]["y"] = position["y"].clone();
    }
    if let Some(transform) = action["Transform"]["transform"].as_str() {
        row["logical"]["transform"] = json!(transform);
    }
}
impl Drop for Niri {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = UnixStream::connect(&self.path);
    }
}

/// The user manager as the rollback arming sees it: it records each transient unit and either
/// accepts it or refuses it.
struct Manager {
    units: Arc<Mutex<Vec<(String, Properties)>>>,
    refuse: Arc<AtomicBool>,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn subscribe(&self) {}
    async fn start_transient_unit(
        &self,
        unit: String,
        mode: String,
        properties: Properties,
        _auxiliary: Vec<(String, Properties)>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
        assert_eq!(mode, "fail");
        if self.refuse.load(Ordering::Relaxed) {
            return Err(zbus::fdo::Error::AccessDenied("fixture refusal".into()));
        }
        self.units.lock().unwrap().push((unit.clone(), properties));
        let job =
            zbus::zvariant::OwnedObjectPath::try_from("/org/freedesktop/systemd1/job/1").unwrap();
        Self::job_removed(&emitter, 1, job.as_ref(), &unit, "done").await?;
        Ok(job)
    }
    #[zbus(signal)]
    async fn job_removed(
        emitter: &zbus::object_server::SignalEmitter<'_>,
        id: u32,
        job: zbus::zvariant::ObjectPath<'_>,
        unit: &str,
        result: &str,
    ) -> zbus::Result<()>;
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Lab {
    home: tempfile::TempDir,
    endpoint: PathBuf,
    niri: Niri,
    units: Arc<Mutex<Vec<(String, Properties)>>>,
    refuse: Arc<AtomicBool>,
    _daemon: Process,
    _manager: zbus::blocking::Connection,
    _bus: Process,
}
impl Lab {
    fn start() -> Result<Self> {
        let home = tempfile::Builder::new().prefix("dp-").tempdir()?;
        let root = home.path();
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        for dir in ["runtime", "state/nacre", "config/nacre", "private"] {
            fs::create_dir_all(root.join(dir))?;
            fs::set_permissions(root.join(dir), fs::Permissions::from_mode(0o700))?;
        }
        // A saved desktop document keeps the controller from asking the compositor for options.
        fs::write(root.join("config/nacre/desktop.json"), br#"{"version":3}"#)?;
        let mut bus = Process(
            Command::new("/usr/bin/dbus-daemon")
                .args(["--session", "--nofork", "--print-address"])
                .env_clear()
                .env("HOME", root)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .spawn()?,
        );
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().ok_or("Missing private bus address")?)
            .read_line(&mut address)?;
        let address = address.trim().to_owned();
        let units = Arc::new(Mutex::new(Vec::new()));
        let refuse = Arc::new(AtomicBool::new(false));
        let manager = zbus::blocking::connection::Builder::address(address.as_str())?
            .name("org.freedesktop.systemd1")?
            .serve_at(
                "/org/freedesktop/systemd1",
                Manager {
                    units: units.clone(),
                    refuse: refuse.clone(),
                },
            )?
            .build()?;
        let niri = Niri::new(root.join("niri.sock"))?;
        let endpoint = root.join("private/daemon.sock");
        let notify_path = root.join("notify.sock");
        let notify = UnixDatagram::bind(&notify_path)?;
        notify.set_read_timeout(Some(Duration::from_secs(20)))?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
        Self::environment(&mut command, root, &niri.path, &endpoint, &address);
        let daemon = Process(
            command
                .arg("daemon")
                .env("NOTIFY_SOCKET", &notify_path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?,
        );
        notify.recv(&mut [0; 256])?;
        Ok(Self {
            home,
            endpoint,
            niri,
            units,
            refuse,
            _daemon: daemon,
            _manager: manager,
            _bus: bus,
        })
    }
    fn environment(command: &mut Command, root: &Path, niri: &Path, endpoint: &Path, bus: &str) {
        command
            .env_clear()
            .env("HOME", root)
            .env("PATH", "/usr/bin:/bin")
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_STATE_HOME", root.join("state"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_RUNTIME_DIR", root.join("runtime"))
            .env("NACRE_DAEMON_SOCKET", endpoint)
            .env("NACRE_COMPOSITOR", "niri")
            .env("NIRI_SOCKET", niri)
            .env("NACRE_NIRI_HOVER_STREAM", "0")
            .env("DBUS_SESSION_BUS_ADDRESS", bus)
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}", root.join("absent-system").display()),
            );
    }
    /// The helper CLI against this daemon, as the armed rollback unit runs it after its wait.
    fn helper(&self, args: &[&str]) -> Result<Value> {
        let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
        Self::environment(
            &mut command,
            self.home.path(),
            &self.niri.path,
            &self.endpoint,
            "unix:path=/nonexistent",
        );
        let output = command.args(args).output()?;
        if !output.status.success() {
            return Err(String::from_utf8_lossy(&output.stderr).into_owned().into());
        }
        Ok(serde_json::from_slice(&output.stdout)?)
    }
    fn journal(&self) -> PathBuf {
        self.home
            .path()
            .join("config/nacre/display-pending-niri.json")
    }
    fn token(&self) -> Result<String> {
        let journal: Value = serde_json::from_slice(&fs::read(self.journal())?)?;
        Ok(journal["token"].as_str().ok_or("Missing token")?.to_owned())
    }
}

struct Client {
    reader: BufReader<UnixStream>,
    mirror: Mirror,
    next: u64,
    welcome: Message,
}
impl Client {
    fn connect(endpoint: &Path, topics: &[&str]) -> Result<Self> {
        let stream = UnixStream::connect(endpoint)?;
        stream.set_read_timeout(Some(Duration::from_secs(20)))?;
        let mut reader = BufReader::new(stream);
        writeln!(
            reader.get_mut(),
            "{}",
            json!({"t":"hello","proto":1,"client":"displays-test"})
        )?;
        let mut line = String::new();
        reader.read_line(&mut line)?;
        let welcome: Message = serde_json::from_str(&line)?;
        let mut mirror = Mirror::default();
        mirror.receive(&welcome);
        writeln!(reader.get_mut(), "{}", json!({"t":"sub","topics":topics}))?;
        Ok(Self {
            reader,
            mirror,
            next: 0,
            welcome,
        })
    }
    fn pump(&mut self) -> Result<Message> {
        let mut line = String::new();
        if self.reader.read_line(&mut line)? == 0 {
            return Err("Daemon closed the connection".into());
        }
        let message: Message = serde_json::from_str(&line)?;
        if let Update::Resync(topic) = self.mirror.receive(&message) {
            writeln!(
                self.reader.get_mut(),
                "{}",
                json!({"t":"resync","topics":[topic]})
            )?;
        }
        Ok(message)
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Reply> {
        self.next += 1;
        let id = self.next;
        writeln!(
            self.reader.get_mut(),
            "{}",
            json!({"t":"call","id":id,"m":method,"p":params})
        )?;
        loop {
            if let Message::Ret { id: got, reply } = self.pump()? {
                if got == Id::Integer(id as i64) {
                    return Ok(reply);
                }
            }
        }
    }
    fn ok(&mut self, method: &str, params: Value) -> Result<Value> {
        match self.call(method, params)? {
            Reply::Ok { ok } => Ok(ok),
            Reply::Err { err } => Err(format!("{method} failed: {err:?}").into()),
        }
    }
    /// The error code and message.
    fn err(&mut self, method: &str, params: Value) -> Result<(String, String)> {
        match self.call(method, params)? {
            Reply::Err { err } => Ok((
                serde_json::to_value(err.code)?.as_str().unwrap().to_owned(),
                err.message,
            )),
            Reply::Ok { ok } => Err(format!("{method} unexpectedly succeeded: {ok}").into()),
        }
    }
    fn until(&mut self, what: &str, check: impl Fn(&Value) -> bool) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some((_, state)) = self.mirror.topics.get("displays") {
                if check(state) {
                    return Ok(state.clone());
                }
            }
            if Instant::now() > deadline {
                return Err(format!("Timed out waiting for displays: {what}").into());
            }
            self.pump()?;
        }
    }
    fn methods(&self) -> Vec<String> {
        match &self.welcome {
            Message::Welcome { methods, .. } => methods.keys().cloned().collect(),
            _ => Vec::new(),
        }
    }
}
fn output<'a>(state: &'a Value, name: &str) -> &'a Value {
    state["outputs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["name"] == name)
        .unwrap_or_else(|| panic!("no output {name} in {state}"))
}
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn idle(state: &Value) -> bool {
    state["pending"].is_null() && state["outputs"].as_array().is_some_and(|o| o.len() == 2)
}

#[test]
fn displays_topic_apply_confirm_and_revert_run_against_fakes_only() -> Result {
    let lab = Lab::start()?;
    let mut client = Client::connect(&lab.endpoint, &["displays"])?;
    let methods = client.methods();
    for method in ["display.apply", "display.confirm", "display.revert"] {
        assert!(methods.iter().any(|m| m == method), "{method} advertised");
    }

    // The topic: what QML showed, plus make/model/vrr from niri.
    let state = client.until("two outputs", idle)?;
    assert_eq!(state["health"]["available"], true);
    assert_eq!(
        (state["can_configure"].clone(), state["can_mirror"].clone()),
        (json!(true), json!(false))
    );
    assert_eq!(
        state["scales"],
        json!([1.0, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0])
    );
    let hdmi = output(&state, "HDMI-A-1");
    assert_eq!(
        (hdmi["make"].clone(), hdmi["model"].clone()),
        (json!("LG"), json!("27GL850"))
    );
    assert_eq!(hdmi["vrr"], true);
    assert_eq!(
        hdmi["modes"],
        json!(["1920x1080@60.000Hz", "1920x1080@143.981Hz"])
    );
    assert_eq!(hdmi["mode"], "1920x1080@60.000Hz");
    assert_eq!(
        (hdmi["x"].clone(), hdmi["scale"].clone()),
        (json!(1920), json!(1.0))
    );
    assert_eq!(output(&state, "DP-1")["vrr"], false);

    // Refusals change nothing: bad scale, unknown output, unlisted mode, mirror on niri.
    for (params, code) in [
        (json!({"output":"HDMI-A-1","scale":1.1}), "validation"),
        (
            json!({"output":"HDMI-A-1","scale":1,"mode":"1x1@1Hz"}),
            "validation",
        ),
        (
            json!({"output":"HDMI-A-1","arrangement":"mirror","scale":1}),
            "validation",
        ),
        (json!({"output":"nope","scale":2}), "validation"),
        (
            json!({"output":"HDMI-A-1","scale":1,"mode":"1280x720@60.000Hz"}),
            "validation",
        ),
        (
            json!({"output":"DP-1","arrangement":"mirror"}),
            "validation",
        ),
    ] {
        assert_eq!(
            client.err("display.apply", params.clone())?.0,
            code,
            "{params}"
        );
    }
    assert_eq!(lab.niri.actions(), 0);
    assert!(lab.units.lock().unwrap().is_empty() && !lab.journal().exists());

    // Apply: journal, armed rollback unit, then the compositor (all of it fake).
    let before = now_ms();
    let applied = client.ok("display.apply", json!({"output":"HDMI-A-1","scale":2}))?;
    assert_eq!(applied["pending"]["seconds"], 20);
    let deadline = applied["pending"]["deadline_ms"].as_u64().unwrap();
    assert!(
        deadline >= before + 19_000 && deadline <= now_ms() + 20_000,
        "{deadline}"
    );
    assert_eq!(lab.niri.logical("HDMI-A-1", "scale"), json!(2.0));
    assert_eq!(lab.niri.logical("DP-1", "scale"), json!(1.0));
    let token = lab.token()?;
    {
        let units = lab.units.lock().unwrap();
        assert_eq!(units.len(), 1);
        assert_eq!(units[0].0, format!("nacre-display-revert-{token}.service"));
        let properties: BTreeMap<String, zbus::zvariant::OwnedValue> = units[0]
            .1
            .iter()
            .map(|(k, v)| (k.clone(), v.try_clone().unwrap()))
            .collect();
        let exec = Vec::<(String, Vec<String>, bool)>::try_from(
            properties["ExecStart"].try_clone().unwrap(),
        )?;
        assert_eq!(
            exec[0].1[1..],
            ["desktop-settings", "rollback-after", token.as_str()]
        );
    }
    let state = client.until("pending", |s| s["pending"]["seconds"] == 20)?;
    assert_eq!(output(&state, "HDMI-A-1")["scale"], 2.0);
    assert_eq!(
        state["pending"]["deadline_ms"],
        applied["pending"]["deadline_ms"]
    );

    // One change at a time; confirm/revert answer for the pending one only.
    assert_eq!(
        client
            .err("display.apply", json!({"output":"DP-1","scale":1.5}))?
            .0,
        "busy"
    );
    // A wrong token never reverts (the armed unit carries the right one).
    lab.helper(&["desktop-settings", "revert", "not-the-token"])?;
    assert_eq!(lab.niri.logical("HDMI-A-1", "scale"), json!(2.0));
    // The armed unit fires: it runs this very command after its 20 s wait.
    lab.helper(&["desktop-settings", "revert", &token])?;
    assert_eq!(lab.niri.logical("HDMI-A-1", "scale"), json!(1.0));
    assert!(!lab.journal().exists());
    let state = client.until("auto-reverted", |s| {
        s["pending"].is_null() && s["outputs"][1]["scale"] == 1.0
    })?;
    assert_eq!(output(&state, "HDMI-A-1")["scale"], 1.0);
    assert_eq!(
        client.ok("display.revert", json!({}))?,
        json!({"changed":false})
    );
    assert_eq!(
        client.ok("display.confirm", json!({}))?,
        json!({"changed":false})
    );

    // Revert by hand puts the previous layout back at once.
    client.ok(
        "display.apply",
        json!({"output":"HDMI-A-1","scale":1.5,"mode":"1920x1080@143.981Hz"}),
    )?;
    assert_eq!(lab.niri.mode("HDMI-A-1"), json!(143981));
    assert_eq!(
        client.ok("display.revert", json!({}))?,
        json!({"changed":true})
    );
    assert_eq!(
        (
            lab.niri.mode("HDMI-A-1"),
            lab.niri.logical("HDMI-A-1", "scale")
        ),
        (json!(60000), json!(1.0))
    );
    client.until("idle again", idle)?;

    // Confirm keeps it, persists it for the next start and ends the countdown.
    client.ok(
        "display.apply",
        json!({"output":"HDMI-A-1","scale":1.5,"mode":"1920x1080@143.981Hz"}),
    )?;
    assert_eq!(
        client.ok("display.confirm", json!({}))?,
        json!({"changed":true})
    );
    assert!(!lab.journal().exists());
    let state = client.until("kept", |s| {
        s["pending"].is_null() && s["outputs"][1]["mode"] == "1920x1080@143.981Hz"
    })?;
    assert_eq!(output(&state, "HDMI-A-1")["scale"], 1.5);
    let store = client.ok("settings.get", json!({"store":"desktop"}))?;
    let saved = &store["data"]["compositorDisplays"]["niri"];
    assert_eq!(saved[1]["name"], "HDMI-A-1");
    assert_eq!(
        (saved[1]["scale"].clone(), saved[1]["refreshRate"].clone()),
        (json!(1.5), json!(143.981))
    );

    // Arrangement: the primary stays at the origin and the others follow it (QML "Extend left").
    client.ok(
        "display.apply",
        json!({"output":"DP-1","arrangement":"extend-left"}),
    )?;
    assert_eq!(lab.niri.logical("DP-1", "x"), json!(0));
    assert!(lab.niri.logical("HDMI-A-1", "x").as_i64().unwrap() < 0);
    client.ok("display.confirm", json!({}))?;
    client.until("arranged", |s| {
        s["pending"].is_null() && output(s, "HDMI-A-1")["x"].as_i64().unwrap() < 0
    })?;
    Ok(())
}

#[test]
fn a_refused_rollback_arming_changes_no_output() -> Result {
    let lab = Lab::start()?;
    let mut client = Client::connect(&lab.endpoint, &["displays"])?;
    client.until("two outputs", idle)?;
    lab.refuse.store(true, Ordering::Relaxed);
    let (code, message) = client.err("display.apply", json!({"output":"HDMI-A-1","scale":2}))?;
    assert_eq!(
        (code.as_str(), message.as_str()),
        (
            "unavailable",
            "Automatic display rollback could not be armed"
        )
    );
    assert_eq!(
        lab.niri.actions(),
        0,
        "no output was touched without a safety net"
    );
    assert!(!lab.journal().exists());
    assert_eq!(lab.niri.logical("HDMI-A-1", "scale"), json!(1.0));
    lab.refuse.store(false, Ordering::Relaxed);
    client.ok("display.apply", json!({"output":"HDMI-A-1","scale":2}))?;
    client.ok("display.revert", json!({}))?;
    Ok(())
}
