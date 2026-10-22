//! Dashboard extras on a private daemon: temporary HOME, fake compositor socket and
//! fake desktop tools. No ambient desktop, bus, clipboard or session is touched.
use nacre_protocol::{Id, Message, Mirror, Reply, Update};
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    os::unix::{
        fs::{symlink, PermissionsExt},
        net::{UnixDatagram, UnixListener, UnixStream},
    },
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Fake {
    path: PathBuf,
    stop: Arc<AtomicBool>,
}
impl Fake {
    /// Just enough niri IPC for the daemon's compositor, session and settings paths.
    fn niri(path: PathBuf) -> Result<Self> {
        let listener = UnixListener::bind(&path)?;
        let stop = Arc::new(AtomicBool::new(false));
        let done = stop.clone();
        let workspaces = json!([{"id":1,"idx":1,"name":"Fixture","output":"fixture","is_active":true,"is_focused":true,"is_urgent":false,"active_window_id":null}]);
        let outputs = json!({"fixture":{"name":"fixture","modes":[{"width":1280,"height":720,"refresh_rate":60000}],"current_mode":0,"logical":{"x":0,"y":0,"width":1280,"height":720,"scale":1,"transform":"Normal"}}});
        thread::spawn(move || {
            while let Ok((stream, _)) = listener.accept() {
                if done.load(Ordering::Relaxed) {
                    break;
                }
                let (workspaces, outputs) = (workspaces.clone(), outputs.clone());
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
                        let response = match request.as_str() {
                            Some("Outputs") => json!({"Ok":{"Outputs":outputs}}),
                            Some("Workspaces") => json!({"Ok":{"Workspaces":workspaces}}),
                            Some("Windows") => json!({"Ok":{"Windows":[]}}),
                            _ => json!({"Err":"Unsupported fixture request"}),
                        };
                        if writeln!(reader.get_mut(), "{response}").is_err() {
                            break;
                        }
                    }
                });
            }
        });
        Ok(Self { path, stop })
    }
}
impl Drop for Fake {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = UnixStream::connect(&self.path);
    }
}

struct Daemon {
    child: Child,
    endpoint: PathBuf,
    home: tempfile::TempDir,
    _niri: Fake,
}
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

const TOOLS: [(&str, &str); 5] = [
    (
        "cliphist",
        r#"d="$FAKE_DIR/clip"; t=$(printf '\t')
case "$1" in
list) cat "$d/rows" ;;
decode) IFS= read -r line; cat "$d/data-${line%%"$t"*}" ;;
delete) IFS= read -r line; grep -v "^${line%%"$t"*}$t" "$d/rows" > "$d/rows.new"; mv "$d/rows.new" "$d/rows"; : >> "$CLIPHIST_DB_PATH" ;;
esac
"#,
    ),
    (
        "wl-copy",
        "cat > \"$FAKE_DIR/copied\"; printf '%s\\n' \"$*\" > \"$FAKE_DIR/copied.args\"\n",
    ),
    (
        "xdg-open",
        "printf '%s\\n' \"$*\" >> \"$FAKE_DIR/opened\"\n",
    ),
    (
        "notify-send",
        "printf '%s\\n' \"$*\" >> \"$FAKE_DIR/notified\"\n",
    ),
    (
        "usage",
        r#"case "$4" in
codex) echo '{"rows":["Codex · Default","codex · 5h: 12% used · resets Mon 12:00"]}' ;;
claude) [ -f "$FAKE_DIR/claude-fail" ] && exit 1; echo '{"rows":["Claude Code · Default","5 hour: 3% used","Cached report: now"]}' ;;
esac
"#,
    ),
];

impl Daemon {
    fn start(home: tempfile::TempDir, prepare: impl FnOnce(&Path)) -> Result<Self> {
        let root = home.path();
        fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
        let fake = root.join("fake");
        fs::create_dir_all(fake.join("bin"))?;
        fs::create_dir_all(fake.join("clip"))?;
        fs::create_dir_all(root.join("cache/cliphist"))?;
        fs::write(root.join("cache/cliphist/db"), b"")?;
        for (name, body) in TOOLS {
            let path = fake.join("bin").join(name);
            fs::write(&path, format!("#!/bin/sh\n{body}"))?;
            fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        }
        let private = root.join("private");
        fs::create_dir(&private)?;
        fs::set_permissions(&private, fs::Permissions::from_mode(0o700))?;
        fs::create_dir_all(root.join("state/nacre"))?;
        fs::set_permissions(root.join("state/nacre"), fs::Permissions::from_mode(0o700))?;
        fs::create_dir_all(root.join("config/nacre"))?;
        prepare(root);
        let niri_path = root.join("niri.sock");
        let niri = Fake::niri(niri_path.clone())?;
        let endpoint = private.join("daemon.sock");
        let notify_path = root.join("notify.sock");
        let notify = UnixDatagram::bind(&notify_path)?;
        notify.set_read_timeout(Some(Duration::from_secs(20)))?;
        let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
        command
            .arg("daemon")
            .env_clear()
            .env("HOME", root)
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", fake.join("bin").display()),
            )
            .env("FAKE_DIR", &fake)
            .env("CLIPHIST_DB_PATH", root.join("cache/cliphist/db"))
            .env("NACRE_HELPER_BINARY", fake.join("bin/usage"))
            .env("NACRE_BRAIN", root.join("Brain"))
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_STATE_HOME", root.join("state"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_RUNTIME_DIR", root.join("runtime"))
            .env("NACRE_DAEMON_SOCKET", &endpoint)
            .env("NOTIFY_SOCKET", &notify_path)
            .env("NACRE_COMPOSITOR", "niri")
            .env("NIRI_SOCKET", &niri_path)
            .env("NACRE_NIRI_HOVER_STREAM", "0")
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}", root.join("absent-session").display()),
            )
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}", root.join("absent-system").display()),
            )
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let child = command.spawn()?;
        let daemon = Self {
            child,
            endpoint,
            home,
            _niri: niri,
        };
        notify.recv(&mut [0; 256])?;
        Ok(daemon)
    }
    fn root(&self) -> &Path {
        self.home.path()
    }
    fn client(&self, topics: &[&str]) -> Result<Client> {
        Client::connect(&self.endpoint, topics)
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
            json!({"t":"hello","proto":1,"client":"dashboard-extras-test"})
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
    fn err(&mut self, method: &str, params: Value) -> Result<String> {
        match self.call(method, params)? {
            Reply::Err { err } => Ok(serde_json::to_value(err.code)?.as_str().unwrap().to_owned()),
            Reply::Ok { ok } => Err(format!("{method} unexpectedly succeeded: {ok}").into()),
        }
    }
    fn until(&mut self, topic: &str, what: &str, check: impl Fn(&Value) -> bool) -> Result<Value> {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Some((_, state)) = self.mirror.topics.get(topic) {
                if check(state) {
                    return Ok(state.clone());
                }
            }
            if Instant::now() > deadline {
                return Err(format!("Timed out waiting for {topic}: {what}").into());
            }
            self.pump()?;
        }
    }
    fn state(&self, topic: &str) -> Value {
        self.mirror.topics[topic].1.clone()
    }
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}
fn lines(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}
fn wait_for(what: &str, check: impl Fn() -> bool) -> Result {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !check() {
        if Instant::now() > deadline {
            return Err(format!("Timed out waiting for {what}").into());
        }
        thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
fn home() -> tempfile::TempDir {
    tempfile::Builder::new().prefix("de-").tempdir().unwrap()
}
const FOCUS: &str = "dashboard.focus";
const DESKTOP: &str = "settings.store.desktop";
fn dnd(state: &Value) -> bool {
    state["data"]["dnd"] == true
}

#[test]
fn methods_and_topics_are_advertised_and_never_expose_private_state_by_default() -> Result {
    let daemon = Daemon::start(home(), |_| {})?;
    let client = daemon.client(&[
        FOCUS,
        "clipboard",
        "screenshots",
        "ai.usage",
        "brain.recent",
    ])?;
    let Message::Welcome {
        topics, methods, ..
    } = &client.welcome
    else {
        panic!("welcome")
    };
    for topic in [
        FOCUS,
        "clipboard",
        "screenshots",
        "ai.usage",
        "brain.recent",
    ] {
        assert!(topics.contains_key(topic), "{topic}");
    }
    for method in [
        "focus.start",
        "focus.pause",
        "focus.resume",
        "focus.stop",
        "clipboard.copy",
        "clipboard.clear",
        "screenshots.open",
        "ai.usage.refresh",
    ] {
        assert_eq!(methods[method].v, 1, "{method}");
    }
    let mut client = client;
    let idle = client.until(FOCUS, "idle", |s| s["status"] == "idle")?;
    assert_eq!(
        (idle["total_s"].clone(), idle["health"]["available"].clone()),
        (json!(0), json!(true))
    );
    assert_eq!(
        client.until("ai.usage", "unread", |s| s["health"]["available"] == false)?["providers"]
            ["order"],
        json!([])
    );
    Ok(())
}

#[test]
fn focus_session_owns_and_restores_dnd_and_survives_bad_calls() -> Result {
    let daemon = Daemon::start(home(), |_| {})?;
    let mut client = daemon.client(&[FOCUS, DESKTOP])?;
    client.until(DESKTOP, "desktop", |s| s["data"].is_object())?;
    for bad in [
        json!({"minutes":0}),
        json!({"minutes":181}),
        json!({"minutes":5,"x":1}),
        json!({}),
    ] {
        assert_eq!(client.err("focus.start", bad)?, "validation");
    }
    assert_eq!(client.ok("focus.pause", json!({}))?["changed"], false);
    assert_eq!(
        client.ok("focus.start", json!({"minutes":25}))?["changed"],
        true
    );
    let running = client.until(FOCUS, "running", |s| s["status"] == "running")?;
    let deadline = running["deadline_ms"].as_u64().unwrap();
    assert!((now_ms() + 24 * 60_000..=now_ms() + 25 * 60_000).contains(&deadline));
    assert_eq!(
        (
            running["total_s"].clone(),
            running["owns_dnd"].clone(),
            running["previous_dnd"].clone()
        ),
        (json!(1500), json!(true), json!(false))
    );
    assert!(running.get("remaining_s").is_none());
    client.until(DESKTOP, "dnd on", dnd)?;
    assert_eq!(
        client.err("focus.start", json!({"minutes":5}))?,
        "invalid_request"
    );
    client.ok("focus.pause", json!({}))?;
    let paused = client.until(FOCUS, "paused", |s| s["status"] == "paused")?;
    assert!(paused.get("deadline_ms").is_none());
    let remaining = paused["remaining_s"].as_u64().unwrap();
    assert!((1498..=1500).contains(&remaining), "{remaining}");
    client.ok("focus.resume", json!({}))?;
    client.until(FOCUS, "resumed", |s| {
        s["status"] == "running" && s["deadline_ms"].is_u64()
    })?;
    let saved: Value = serde_json::from_str(&lines(
        &daemon.root().join("state/nacre/dashboard-focus.json"),
    ))?;
    assert_eq!(
        (
            saved["status"].clone(),
            saved["ownsDnd"].clone(),
            saved["owner"].clone()
        ),
        (json!("running"), json!(true), json!("nacre-daemon"))
    );
    client.ok("focus.stop", json!({}))?;
    let idle = client.until(FOCUS, "idle", |s| s["status"] == "idle")?;
    assert_eq!(
        (idle["owns_dnd"].clone(), idle["total_s"].clone()),
        (json!(false), json!(0))
    );
    client.until(DESKTOP, "dnd restored", |s| !dnd(s))?;
    assert_eq!(client.ok("focus.stop", json!({}))?["changed"], false);
    Ok(())
}

#[test]
fn focus_keeps_a_user_dnd_and_releases_when_the_user_turns_it_off() -> Result {
    let daemon = Daemon::start(home(), |_| {})?;
    let mut client = daemon.client(&[FOCUS, DESKTOP])?;
    let rev = client.until(DESKTOP, "desktop", |s| s["data"].is_object())?["rev"].clone();
    client.ok(
        "settings.desktop",
        json!({"arguments":["set","dnd","true"],"rev":rev}),
    )?;
    client.until(DESKTOP, "user dnd", dnd)?;
    client.ok("focus.start", json!({"minutes":10}))?;
    let running = client.until(FOCUS, "running", |s| s["status"] == "running")?;
    assert_eq!(
        (running["owns_dnd"].clone(), running["previous_dnd"].clone()),
        (json!(false), json!(true))
    );
    client.ok("focus.stop", json!({}))?;
    client.until(FOCUS, "idle", |s| s["status"] == "idle")?;
    assert!(
        dnd(&client.state(DESKTOP)),
        "a DND the user set before the session is never turned off"
    );
    // Turn DND off, start (owns it), then the user turns it off themselves.
    let rev = client.state(DESKTOP)["rev"].clone();
    client.ok(
        "settings.desktop",
        json!({"arguments":["set","dnd","false"],"rev":rev}),
    )?;
    client.until(DESKTOP, "dnd off", |s| !dnd(s))?;
    client.ok("focus.start", json!({"minutes":10}))?;
    client.until(DESKTOP, "owned dnd", dnd)?;
    client.until(FOCUS, "owns", |s| s["owns_dnd"] == true)?;
    let rev = client.state(DESKTOP)["rev"].clone();
    client.ok(
        "settings.desktop",
        json!({"arguments":["set","dnd","false"],"rev":rev}),
    )?;
    client.until(FOCUS, "released", |s| {
        s["status"] == "running" && s["owns_dnd"] == false
    })?;
    Ok(())
}

fn focus_file(owner: bool, deadline_s: f64, owns: bool) -> Value {
    let mut state = json!({"status":"running","total":60,"remaining":60,"deadline":deadline_s,"previousDnd":false,"ownsDnd":owns});
    if owner {
        state["owner"] = json!("nacre-daemon");
    }
    state
}
fn unix_s() -> f64 {
    now_ms() as f64 / 1000.0
}

#[test]
fn daemon_owned_session_completes_notifies_and_restores_dnd_after_a_restart() -> Result {
    let daemon = Daemon::start(home(), |root| {
        fs::write(
            root.join("config/nacre/desktop.json"),
            r#"{"version":3,"dnd":true}"#,
        )
        .unwrap();
        fs::write(
            root.join("state/nacre/dashboard-focus.json"),
            focus_file(true, unix_s() + 1.5, true).to_string(),
        )
        .unwrap();
    })?;
    let mut client = daemon.client(&[FOCUS, DESKTOP])?;
    client.until(FOCUS, "running", |s| {
        s["status"] == "running" || s["status"] == "done"
    })?;
    let done = client.until(FOCUS, "done", |s| s["status"] == "done")?;
    assert_eq!(
        (done["owns_dnd"].clone(), done["total_s"].clone()),
        (json!(false), json!(60))
    );
    client.until(DESKTOP, "dnd restored", |s| !dnd(s))?;
    wait_for("notification", || {
        lines(&daemon.root().join("fake/notified")).contains("Focus complete")
    })?;
    let saved: Value = serde_json::from_str(&lines(
        &daemon.root().join("state/nacre/dashboard-focus.json"),
    ))?;
    assert_eq!(
        (saved["status"].clone(), saved["ownsDnd"].clone()),
        (json!("done"), json!(false))
    );
    Ok(())
}

#[test]
fn a_legacy_session_is_mirrored_and_controlled_but_never_auto_completed() -> Result {
    let daemon = Daemon::start(home(), |root| {
        fs::write(
            root.join("state/nacre/dashboard-focus.json"),
            focus_file(false, unix_s() - 5.0, false).to_string(),
        )
        .unwrap();
    })?;
    let mut client = daemon.client(&[FOCUS])?;
    client.until(FOCUS, "mirrored", |s| s["status"] == "running")?;
    thread::sleep(Duration::from_millis(600));
    assert_eq!(client.state(FOCUS)["status"], "running");
    assert!(
        lines(&daemon.root().join("fake/notified")).is_empty(),
        "the legacy owner notifies, not the daemon"
    );
    // An external writer's change is picked up without any call.
    fs::write(
        daemon.root().join("state/nacre/dashboard-focus.json"),
        json!({"status":"idle","total":0}).to_string(),
    )?;
    client.until(FOCUS, "external stop", |s| s["status"] == "idle")?;
    // Damaged state is reported, never replaced by defaults.
    fs::write(
        daemon.root().join("state/nacre/dashboard-focus.json"),
        "not json",
    )?;
    client.until(FOCUS, "unavailable", |s| {
        s["health"]["available"] == false && s["health"]["stale"] == true
    })?;
    assert_eq!(
        client.err("focus.start", json!({"minutes":5}))?,
        "unavailable"
    );
    Ok(())
}

#[test]
fn clipboard_topic_copy_and_clear_keep_pins_and_stay_bounded_and_private() -> Result {
    let daemon = Daemon::start(home(), |root| {
        let clip = root.join("fake/clip");
        let tab = '\t';
        let mut rows = String::new();
        for id in 1..=60u32 {
            rows.push_str(&format!("{id}{tab}note {id}\n"));
            fs::write(clip.join(format!("data-{id}")), format!("SECRET-{id}")).unwrap();
        }
        rows.push_str(&format!("61{tab}[[ binary data 12 KiB png 100x100 ]]\n"));
        fs::write(clip.join("data-61"), b"\x89PNG\r\n").unwrap();
        // Rows are stored newest first.
        let reversed: String = rows.lines().rev().map(|l| format!("{l}\n")).collect();
        fs::write(clip.join("rows"), reversed).unwrap();
        fs::write(clip.join("long"), "x").unwrap();
    })?;
    let mut client = daemon.client(&["clipboard"])?;
    let state = client.until("clipboard", "rows", |s| s["health"]["available"] == true)?;
    let order = state["entries"]["order"].as_array().unwrap();
    assert_eq!(order.len(), 50);
    assert_eq!(order[0], "c61");
    assert_eq!(state["entries"]["items"]["c61"]["kind"], "image");
    assert_eq!(state["entries"]["items"]["c60"]["kind"], "text");
    assert_eq!(state["entries"]["items"]["c60"]["preview"], "note 60");
    assert!(
        state["entries"]["items"]["c60"]["created_ms"]
            .as_u64()
            .unwrap()
            > 1_000_000_000_000
    );
    assert!(
        !state.to_string().contains("SECRET"),
        "clipboard content never leaves cliphist"
    );
    assert_eq!(
        client.err("clipboard.copy", json!({"id":"c9999"}))?,
        "stale_revision"
    );
    assert_eq!(
        client.err("clipboard.copy", json!({"id":"../etc"}))?,
        "validation"
    );
    client.ok("clipboard.copy", json!({"id":"c60"}))?;
    assert_eq!(
        fs::read_to_string(daemon.root().join("fake/copied"))?,
        "SECRET-60"
    );
    assert!(lines(&daemon.root().join("fake/copied.args")).contains("text/plain"));
    // A new copy (database change) refreshes the topic without any call.
    let clip = daemon.root().join("fake/clip");
    let rows = lines(&clip.join("rows"));
    fs::write(clip.join("data-62"), "SECRET-62")?;
    fs::write(clip.join("rows"), format!("62\tfresh copy\n{rows}"))?;
    fs::OpenOptions::new()
        .append(true)
        .open(daemon.root().join("cache/cliphist/db"))?
        .write_all(b"x")?;
    client.until("clipboard", "fresh", |s| s["entries"]["order"][0] == "c62")?;
    // Pin one entry (fingerprint format of the native pin store), then clear.
    let pinned_row = "5\tnote 5";
    let fingerprint = {
        use sha2::{Digest, Sha256};
        let label = format!("{:x}", Sha256::digest("note 5".as_bytes()));
        let content = format!("{:x}", Sha256::digest(b"SECRET-5"));
        json!({"label":label,"content":content})
    };
    let _ = pinned_row;
    let extras = daemon.root().join("state/nacre/shell/extras");
    fs::create_dir_all(&extras)?;
    fs::write(extras.join("pins.json"), json!([fingerprint]).to_string())?;
    assert_eq!(client.ok("clipboard.clear", json!({}))?["changed"], true);
    let remaining = lines(&clip.join("rows"));
    assert_eq!(remaining.lines().count(), 1, "{remaining}");
    assert!(remaining.starts_with("5\t"));
    let after = client.until("clipboard", "cleared", |s| {
        s["entries"]["order"]
            .as_array()
            .is_some_and(|o| o.len() == 1)
    })?;
    assert_eq!(after["entries"]["order"][0], "c5");
    Ok(())
}

#[test]
fn screenshots_list_only_safe_recent_images_and_open_through_the_viewer() -> Result {
    let daemon = Daemon::start(home(), |root| {
        let folder = root.join("Pictures/Screenshots");
        fs::create_dir_all(&folder).unwrap();
        for index in 0..8u64 {
            let path = folder.join(format!("shot-{index}.png"));
            fs::write(&path, b"png").unwrap();
            let file = fs::File::options().write(true).open(&path).unwrap();
            file.set_modified(
                std::time::UNIX_EPOCH + Duration::from_secs(1_700_000_000 + index * 10),
            )
            .unwrap();
        }
        fs::write(folder.join("notes.txt"), b"not an image").unwrap();
        symlink("/etc/hostname", folder.join("link.png")).unwrap();
        fs::create_dir(folder.join("dir.png")).unwrap();
    })?;
    let mut client = daemon.client(&["screenshots"])?;
    let state = client.until("screenshots", "rows", |s| s["health"]["available"] == true)?;
    let order = state["items"]["order"].as_array().unwrap().clone();
    assert_eq!(order.len(), 6);
    let paths: Vec<_> = order
        .iter()
        .map(|id| {
            state["items"]["items"][id.as_str().unwrap()]["path"]
                .as_str()
                .unwrap()
                .to_owned()
        })
        .collect();
    let folder = daemon.root().join("Pictures/Screenshots");
    assert_eq!(paths[0], folder.join("shot-7.png").to_string_lossy());
    assert_eq!(paths[5], folder.join("shot-2.png").to_string_lossy());
    assert!(!state.to_string().contains("link.png") && !state.to_string().contains("notes.txt"));
    assert_eq!(
        state["items"]["items"][order[0].as_str().unwrap()]["created_ms"],
        1_700_000_070_000_u64
    );
    client.ok("screenshots.open", json!({"id":order[0]}))?;
    wait_for("viewer", || {
        lines(&daemon.root().join("fake/opened")).trim() == paths[0]
    })?;
    assert_eq!(
        client.err("screenshots.open", json!({"id":"s0000000000000000"}))?,
        "stale_revision"
    );
    // A new capture and a removal refresh the topic; a vanished file is stale.
    fs::write(folder.join("shot-9.png"), b"png")?;
    let newest = client.until("screenshots", "new capture", |s| {
        s["items"]["order"].as_array().is_some_and(|o| o.len() == 6)
            && s["items"]["items"].to_string().contains("shot-9.png")
    })?;
    assert!(!newest.to_string().contains("shot-2.png"));
    let id = client.state("screenshots")["items"]["order"][0].clone();
    fs::remove_file(folder.join("shot-9.png"))?;
    assert_eq!(
        client.err("screenshots.open", json!({"id":id}))?,
        "stale_revision"
    );
    Ok(())
}

#[test]
fn ai_usage_reads_on_request_and_keeps_last_good_rows_when_a_provider_fails() -> Result {
    let daemon = Daemon::start(home(), |_| {})?;
    let mut client = daemon.client(&["ai.usage"])?;
    client.until("ai.usage", "unread", |s| {
        s["health"]["available"] == false && s["health"].get("error").is_none()
    })?;
    client.ok("ai.usage.refresh", json!({}))?;
    let state = client.until("ai.usage", "read", |s| s["health"]["available"] == true)?;
    assert_eq!(state["providers"]["order"], json!(["codex", "claude"]));
    assert_eq!(
        state["providers"]["items"]["codex"]["lines"],
        json!(["codex · 5h: 12% used · resets Mon 12:00"])
    );
    assert_eq!(state["providers"]["items"]["claude"]["name"], "Claude Code");
    assert_eq!(
        state["providers"]["items"]["claude"]["lines"],
        json!(["5 hour: 3% used", "Cached report: now"])
    );
    fs::write(daemon.root().join("fake/claude-fail"), b"")?;
    client.ok("ai.usage.refresh", json!({}))?;
    let stale = client.until("ai.usage", "stale", |s| s["health"]["stale"] == true)?;
    assert_eq!(stale["health"]["available"], false);
    assert_eq!(
        stale["providers"]["items"]["claude"]["lines"],
        state["providers"]["items"]["claude"]["lines"]
    );
    Ok(())
}

#[test]
fn brain_recent_lists_regular_notes_only_and_follows_changes() -> Result {
    let daemon = Daemon::start(home(), |root| {
        let vault = root.join("Brain");
        fs::create_dir_all(vault.join("projects")).unwrap();
        fs::create_dir_all(vault.join(".git")).unwrap();
        fs::write(
            vault.join("projects/a.md"),
            "# Alpha note\n- [ ] first task\n- [x] done\n",
        )
        .unwrap();
        fs::write(vault.join(".git/hidden.md"), "# Hidden\n- [ ] no\n").unwrap();
        fs::write(root.join("outside.md"), "# Outside secret\n- [ ] leak\n").unwrap();
        symlink(root.join("outside.md"), vault.join("projects/link.md")).unwrap();
    })?;
    let mut client = daemon.client(&["brain.recent"])?;
    let state = client.until("brain.recent", "notes", |s| {
        s["health"]["available"] == true
    })?;
    assert_eq!(state["notes"]["order"].as_array().unwrap().len(), 1);
    let id = state["notes"]["order"][0].as_str().unwrap();
    assert_eq!(state["notes"]["items"][id]["title"], "Alpha note");
    assert!(state["notes"]["items"][id]["path"]
        .as_str()
        .unwrap()
        .ends_with("projects/a.md"));
    let task = state["tasks"]["order"][0].as_str().unwrap();
    assert_eq!(state["tasks"]["items"][task]["text"], "first task");
    assert!(
        !state.to_string().contains("Outside")
            && !state.to_string().contains("Hidden")
            && !state.to_string().contains("leak")
    );
    fs::write(
        daemon.root().join("Brain/projects/b.md"),
        "# Beta note\n- [ ] second task\n",
    )?;
    client.until("brain.recent", "new note", |s| {
        s["notes"]["order"].as_array().is_some_and(|o| o.len() == 2)
    })?;
    Ok(())
}

#[test]
fn dashboard_extras_settings_writes_use_the_registered_revision_fenced_writer() -> Result {
    let daemon = Daemon::start(home(), |_| {})?;
    let mut client = daemon.client(&["settings.store.dashboard-extras"])?;
    let topic = "settings.store.dashboard-extras";
    let rev = client.until(topic, "store", |s| s["store"] == "dashboard-extras")?["rev"].clone();
    let set = |key: &str, value: Value, rev: &Value| json!({"store":"dashboard-extras","key":[key],"value":value,"rev":rev});
    let written = client.ok(
        "settings.set",
        set("enabled", json!(["focus", "ai-usage"]), &rev),
    )?;
    assert_eq!(written["data"]["enabled"], json!(["focus", "ai-usage"]));
    assert_eq!(written["rev"], rev.as_u64().unwrap() + 1);
    assert_eq!(
        client.err("settings.set", set("order", json!(["focus"]), &rev))?,
        "stale_revision"
    );
    let rev = written["rev"].clone();
    for bad in [
        json!(["Focus"]),
        json!([{"x":1}]),
        json!([""]),
        json!(vec!["a"; 33]),
        json!("focus"),
    ] {
        assert_eq!(
            client.err("settings.set", set("enabled", bad.clone(), &rev))?,
            "invalid_request",
            "{bad}"
        );
    }
    client.ok("settings.set", set("weatherPhotos", json!(false), &rev))?;
    assert_eq!(
        client.err("settings.set", set("unknown", json!(true), &json!(2)))?,
        "permission_denied"
    );
    client.until(topic, "mirrored", |s| {
        s["data"]["weatherPhotos"] == false && s["data"]["enabled"] == json!(["focus", "ai-usage"])
    })?;
    Ok(())
}
