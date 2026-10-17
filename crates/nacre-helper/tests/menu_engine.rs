//! Real daemon calls and immutable-source catalog edits in a disposable environment.
use nacre_helper::Result;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
struct Daemon(Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Lab {
    home: tempfile::TempDir,
    socket: PathBuf,
    daemon: Daemon,
}
impl Lab {
    fn new() -> Result<Self> {
        Self::with_bus(None)
    }
    fn with_bus(bus: Option<&str>) -> Result<Self> {
        Self::with_fonts(bus, false)
    }
    fn with_fonts(bus: Option<&str>, blocked: bool) -> Result<Self> {
        let home = tempfile::tempdir()?;
        let root = home.path();
        for path in [
            "runtime/private",
            "config/nacre",
            "data/applications",
            "system/applications",
        ] {
            fs::create_dir_all(root.join(path))?;
        }
        fs::set_permissions(
            root.join("runtime/private"),
            fs::Permissions::from_mode(0o700),
        )?;
        for i in 0..320 {
            fs::write(root.join(format!("data/applications/fixture-{i:04}.desktop")),format!("[Desktop Entry]\nType=Application\nName=Application {i:04}\nComment=Synthetic fixture\nExec=fixture-{i:04} '$HOME;$(touch forbidden)'\nIcon=fixture\n"))?;
        }
        fs::create_dir(root.join("working"))?;
        fs::write(root.join("data/applications/fixture-0000.desktop"),format!("[Desktop Entry]\nType=Application\nName=Application 0000\nExec=/bin/true '$HOME;$(touch forbidden)'\nPath={}\n",root.join("working").display()))?;
        let socket = root.join("runtime/private/daemon.sock");
        let binary = std::env::var_os("NACRE_MENU_TEST_BINARY")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_nacre-helper")));
        let mut command = Command::new(binary);
        environment(&mut command, root, &socket);
        command.env("QML_IMPORT_PATH", "private-test-no-import");
        if blocked {
            let bin = root.join("bin");
            fs::create_dir(&bin)?;
            let script = bin.join("fc-list");
            fs::write(
                &script,
                r#"#!/bin/sh
printf 'start\n' >> "$HOME/fonts-started"
while [ -d "$HOME" ] && [ ! -f "$HOME/release-fonts" ]; do /bin/sleep 0.01; done
printf 'Fixture Font\n'
"#,
            )?;
            fs::set_permissions(&script, fs::Permissions::from_mode(0o755))?;
            command.env("PATH", bin);
        }
        if let Some(bus) = bus {
            command.env("DBUS_SESSION_BUS_ADDRESS", bus);
        }
        let daemon = Daemon(
            command
                .arg("daemon")
                .stdout(Stdio::null())
                .stderr(fs::File::create(root.join("daemon.log"))?)
                .spawn()?,
        );
        let lab = Self {
            home,
            socket,
            daemon,
        };
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(mut client) = lab.client() {
                if client
                    .call(
                        1,
                        "menu.query",
                        json!({"query":"","mode":"apps","route":"apps"}),
                    )?
                    .get("ok")
                    .is_some()
                {
                    break;
                }
            }
            if Instant::now() > deadline {
                return Err(format!(
                    "Private launcher failed to index: {}",
                    fs::read_to_string(lab.home.path().join("daemon.log"))?
                        .chars()
                        .take(500)
                        .collect::<String>()
                )
                .into());
            }
            thread::sleep(Duration::from_millis(20));
        }
        Ok(lab)
    }
    fn client(&self) -> Result<Client> {
        Client::new(&self.socket)
    }
}
fn environment(command: &mut Command, home: &Path, socket: &Path) {
    command
        .env_clear()
        .env("HOME", home)
        .env("PATH", "")
        .env("LANG", "en_US.UTF-8")
        .env("XDG_CONFIG_HOME", home.join("config"))
        .env("XDG_STATE_HOME", home.join("state"))
        .env("XDG_CACHE_HOME", home.join("cache"))
        .env("XDG_DATA_HOME", home.join("data"))
        .env("XDG_DATA_DIRS", home.join("system"))
        .env("XDG_RUNTIME_DIR", home.join("runtime"))
        .env("NACRE_DAEMON_SOCKET", socket)
        .env("NACRE_COMPOSITOR", "niri")
        .env("NIRI_SOCKET", home.join("absent-niri"))
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            format!("unix:path={}", home.join("absent-bus").display()),
        )
        .env(
            "DBUS_SYSTEM_BUS_ADDRESS",
            format!("unix:path={}", home.join("absent-system").display()),
        );
}
struct Client {
    input: UnixStream,
    output: BufReader<UnixStream>,
}
impl Client {
    fn new(path: &Path) -> Result<Self> {
        let input = UnixStream::connect(path)?;
        input.set_read_timeout(Some(Duration::from_secs(10)))?;
        let output = BufReader::new(input.try_clone()?);
        let mut c = Self { input, output };
        c.send(json!({"t":"hello","proto":1,"min":1,"client":"menu-fixture"}))?;
        assert_eq!(c.next()?["t"], "welcome");
        Ok(c)
    }
    fn send(&mut self, value: Value) -> Result<()> {
        writeln!(self.input, "{value}")?;
        self.input.flush()?;
        Ok(())
    }
    fn next(&mut self) -> Result<Value> {
        let mut line = String::new();
        self.output.read_line(&mut line)?;
        Ok(serde_json::from_str(&line)?)
    }
    fn call(&mut self, id: u64, method: &str, params: Value) -> Result<Value> {
        self.send(json!({"t":"call","id":id,"m":method,"p":params,"timeout":10000}))?;
        loop {
            let value = self.next()?;
            if value["t"] == "ret" && value["id"] == id {
                return Ok(value);
            }
        }
    }
}
#[test]
fn native_calculator_query_and_ticket_forward_the_exact_literal_result() -> Result<()> {
    let lab = Lab::new()?;
    let mut provider = lab.client()?;
    provider.send(json!({"t":"register","provider":"ui","methods":{"ui.menu.activate":1}}))?;
    assert!(provider
        .call(1, "daemon.ping", json!({}))?
        .get("ok")
        .is_some());
    let mut caller = lab.client()?;
    let answer = caller.call(
        2,
        "menu.query",
        json!({"query":"= .250030517578125","mode":"search"}),
    )?;
    assert_eq!(
        answer["ok"]["conversion"],
        json!({"ok":true,"value":"0.25003051757813","error":""})
    );
    assert_eq!(answer["ok"]["order"], json!(["nacre-menu:calculation"]));
    caller.send(json!({"t":"call","id":3,"m":"menu.activate","p":{
        "id":"nacre-menu:calculation",
        "activation":answer["ok"]["items"]["nacre-menu:calculation"]["activation"],
        "catalog_rev":answer["ok"]["catalog_rev"],
        "row":{"operation":"command","arguments":["forged-command"]}
    }}))?;
    let selected = provider.next()?;
    assert_eq!(selected["t"], "call");
    assert_eq!(selected["m"], "ui.menu.activate");
    let action: nacre_protocol::menu::MenuUiActivate =
        serde_json::from_value(selected["p"].clone())?;
    action.validate()?;
    assert_eq!(selected["p"]["row"]["operation"], "copy");
    assert_eq!(selected["p"]["row"]["value"], "0.25003051757813");
    assert!(selected["p"]["row"].get("arguments").is_none());
    provider.send(json!({"t":"ret","id":selected["id"],"ok":{"keep_open":false}}))?;
    let reply = caller.next()?;
    assert_eq!(reply["id"], 3);
    assert_eq!(reply["ok"], json!({"keep_open":false}));
    let failed = caller.call(4, "menu.query", json!({"query":"= 1/0","mode":"search"}))?;
    assert_eq!(failed["ok"]["order"], json!([]));
    assert_eq!(
        failed["ok"]["conversion"]["error"],
        "The result is undefined or outside the numeric range"
    );
    Ok(())
}
#[test]
fn native_queries_inotify_preferences_tickets_and_failure_recovery() -> Result<()> {
    let lab = Lab::new()?;
    let mut client = lab.client()?;
    let query = || json!({"query":"","mode":"apps","route":"apps"});
    let first = client.call(2, "menu.query", query())?;
    assert_eq!(first["ok"]["order"].as_array().unwrap().len(), 320);
    let warm = client.call(
        20,
        "menu.query",
        json!({"query":"application","mode":"apps","route":"apps"}),
    )?;
    assert!(warm["ok"]["items"].get("fixture-0000").is_some());
    let rev = first["ok"]["catalog_rev"].clone();
    let ticket = first["ok"]["items"]["fixture-0000"]["activation"].clone();
    let bad = client.call(
        3,
        "menu.activate",
        json!({"id":"fixture-0001","activation":ticket,"catalog_rev":rev}),
    )?;
    assert_eq!(bad["err"]["code"], "invalid_request");
    let status = client.call(
        4,
        "menu.preferences",
        json!({"action":"hide","value":{"id":"fixture-0000","enabled":true}}),
    )?;
    assert!(status["ok"]["hidden"]
        .as_array()
        .unwrap()
        .contains(&json!("fixture-0000")));
    let next = client.call(5, "menu.query", query())?;
    assert_eq!(next["ok"]["order"].as_array().unwrap().len(), 319);
    let filtered = client.call(
        21,
        "menu.query",
        json!({"query":"application","mode":"apps","route":"apps"}),
    )?;
    assert!(
        filtered["ok"]["items"].get("fixture-0000").is_none(),
        "A hidden application must leave an already prepared candidate generation"
    );
    let stale = client.call(
        6,
        "menu.activate",
        json!({"id":"fixture-0000","activation":ticket,"catalog_rev":rev}),
    )?;
    assert_eq!(stale["err"]["code"], "stale_revision");
    let mut legacy = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    environment(&mut legacy, lab.home.path(), &lab.socket);
    let output = legacy.args(["launcher-preferences", "load"]).output()?;
    assert!(output.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&output.stdout)?["hidden"],
        json!(["fixture-0000"])
    );
    let apps = lab.home.path().join("data/applications");
    fs::rename(&apps, lab.home.path().join("data/old-applications"))?;
    fs::create_dir(&apps)?;
    fs::write(
        apps.join("new.desktop"),
        "[Desktop Entry]\nType=Application\nName=New application\nExec=fixture\n",
    )?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let current = client.call(7, "menu.query", query())?;
        if current["ok"]["order"] == json!(["new"]) {
            break;
        }
        if Instant::now() > deadline {
            return Err("Directory replacement was not indexed".into());
        }
        thread::sleep(Duration::from_millis(25));
    }
    fs::write(
        lab.home.path().join("config/nacre/launcher.json"),
        "{bad json",
    )?;
    thread::sleep(Duration::from_millis(400));
    assert_eq!(
        client.call(8, "menu.query", query())?["ok"]["order"],
        json!(["new"])
    );
    let rejected = client.call(
        90,
        "menu.preferences",
        json!({"action":"favorite","value":{"id":"new","enabled":true}}),
    )?;
    assert_eq!(rejected["err"]["code"], "validation");
    assert_eq!(
        fs::read_to_string(lab.home.path().join("config/nacre/launcher.json"))?,
        "{bad json"
    );
    let invalid = client.call(9, "menu.query", json!({"limit":0}))?;
    assert_eq!(invalid["err"]["code"], "invalid_request");
    assert_eq!(
        client.call(10, "daemon.ping", json!({}))?["ok"],
        Value::Null
    );
    assert!(!lab.home.path().join("forbidden").exists());
    // No real bus/compositor/desktop is available to these processes.
    assert!(lab.daemon.0.id() > 0);
    Ok(())
}

#[test]
fn provider_worker_reuses_its_index_is_nice_and_exits_after_idle() -> Result<()> {
    use wait_timeout::ChildExt;
    let home = tempfile::tempdir()?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    environment(&mut command, home.path(), &home.path().join("absent.sock"));
    let mut process = Daemon(
        command
            .args(["daemon-menu-worker", &std::process::id().to_string()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut input = process.0.stdin.take().ok_or("Missing worker input")?;
    let mut output = BufReader::new(process.0.stdout.take().ok_or("Missing worker output")?);
    for query in ["rocket", "heart", "rocket"] {
        writeln!(input, "{}", json!({"action":"emoji","query":query}))?;
        input.flush()?;
        let timeout = rustix::event::Timespec::try_from(Duration::from_secs(5))?;
        let mut fds = [rustix::event::PollFd::new(
            output.get_ref(),
            rustix::event::PollFlags::IN,
        )];
        assert_eq!(rustix::event::poll(&mut fds, Some(&timeout))?, 1);
        let mut line = String::new();
        output.read_line(&mut line)?;
        let result: Value = serde_json::from_str(&line)?;
        assert!(!result["ok"]["entries"].as_array().unwrap().is_empty());
    }
    let pid = rustix::process::Pid::from_raw(process.0.id() as i32).ok_or("Invalid worker PID")?;
    assert!(rustix::process::getpriority_process(Some(pid))? >= 10);
    let status = process
        .0
        .wait_timeout(Duration::from_secs(33))?
        .ok_or("Idle worker did not exit")?;
    assert!(status.success());
    Ok(())
}

type Properties = Vec<(String, zbus::zvariant::OwnedValue)>;
struct Manager(std::sync::Arc<std::sync::Mutex<Vec<Properties>>>);
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl Manager {
    fn subscribe(&self) {}
    async fn start_transient_unit(
        &self,
        unit: String,
        mode: String,
        properties: Properties,
        auxiliary: Vec<(String, Properties)>,
        #[zbus(signal_emitter)] emitter: zbus::object_server::SignalEmitter<'_>,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
        assert!(unit.starts_with("nacre-app-") && unit.ends_with(".service"));
        assert_eq!(mode, "fail");
        assert!(auxiliary.is_empty());
        self.0.lock().unwrap().push(properties);
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
#[test]
fn app_activation_uses_private_manager_literal_argv_clean_environment_and_existing_history(
) -> Result<()> {
    use std::sync::{Arc, Mutex};
    use zbus::zvariant::OwnedValue;
    let home = tempfile::tempdir()?;
    let mut bus = Daemon(
        Command::new("/usr/bin/dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .env_clear()
            .env("HOME", home.path())
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?,
    );
    let mut address = String::new();
    BufReader::new(bus.0.stdout.take().ok_or("Missing private bus address")?)
        .read_line(&mut address)?;
    let calls = Arc::new(Mutex::new(Vec::new()));
    let _manager = zbus::blocking::connection::Builder::address(address.trim())?
        .name("org.freedesktop.systemd1")?
        .serve_at("/org/freedesktop/systemd1", Manager(calls.clone()))?
        .build()?;
    let lab = Lab::with_bus(Some(address.trim()))?;
    let mut client = lab.client()?;
    let result = client.call(
        50,
        "menu.query",
        json!({"mode":"apps","route":"apps","query":""}),
    )?;
    let item = &result["ok"]["items"]["fixture-0000"];
    let result=client.call(51,"menu.activate",json!({"id":"fixture-0000","activation":item["activation"],"catalog_rev":result["ok"]["catalog_rev"]}))?;
    assert_eq!(result["ok"], json!({"keep_open":false}));
    let mut calls = calls.lock().unwrap();
    assert_eq!(calls.len(), 1);
    let mut properties: std::collections::BTreeMap<String, OwnedValue> =
        calls.remove(0).into_iter().collect();
    let exec =
        Vec::<(String, Vec<String>, bool)>::try_from(properties.remove("ExecStart").unwrap())?;
    assert_eq!(
        exec,
        vec![(
            "/bin/true".into(),
            vec!["/bin/true".into(), "$HOME;$(touch forbidden)".into()],
            false
        )]
    );
    assert_eq!(
        String::try_from(properties.remove("WorkingDirectory").unwrap())?,
        lab.home.path().join("working").to_str().unwrap()
    );
    assert_eq!(
        String::try_from(properties.remove("Type").unwrap())?,
        "exec"
    );
    let environment = Vec::<String>::try_from(properties.remove("Environment").unwrap())?;
    assert!(
        !environment.iter().any(|e| e.starts_with("QML_IMPORT_PATH=")
            || e.starts_with("TOKIO_WORKER_THREADS=")
            || e.starts_with("NOTIFY_SOCKET="))
    );
    assert!(environment
        .iter()
        .any(|e| e == &format!("HOME={}", lab.home.path().display())));
    let history = client.call(52, "menu.preferences", json!({"action":"load"}))?;
    assert_eq!(history["ok"]["recent"][0], "fixture-0000");
    assert_eq!(history["ok"]["usage"]["fixture-0000"]["count"], 1);
    assert!(!lab.home.path().join("working/forbidden").exists());
    Ok(())
}

#[test]
fn catalog_refresh_preserves_interactive_capacity_and_notifies_menu_clients() -> Result<()> {
    let lab = Lab::with_fonts(None, true)?;
    let mut client = lab.client()?;
    let mut observer = lab.client()?;
    observer.send(json!({"t":"sub","topics":["menu"]}))?;
    let initial = observer.next()?;
    assert_eq!(initial["t"], "snap");
    let revision = initial["data"]["catalog_rev"].as_u64().unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    while !lab.home.path().join("fonts-started").exists() {
        if Instant::now() > deadline {
            return Err("Private font provider did not start".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
    // Keep initial metadata busy until after an application refresh is observed.
    // A second background load must not take the lane needed by this UI query.
    let entry = lab
        .home
        .path()
        .join("data/applications/fixture-0001.desktop");
    let mut text = fs::read_to_string(&entry)?;
    text.push_str("\nComment=Changed fixture catalog\n");
    fs::write(&entry, text)?;
    loop {
        let result = client.call(
            1,
            "menu.query",
            json!({"query":"","mode":"apps","route":"apps"}),
        )?;
        if result["ok"]["catalog_rev"]
            .as_u64()
            .is_some_and(|r| r > revision)
        {
            break;
        }
        if Instant::now() > deadline {
            return Err("Changed application catalog was not indexed".into());
        }
        thread::sleep(Duration::from_millis(5));
    }
    let result = client.call(
        2,
        "menu.query",
        json!({"query":"happy","mode":"apps","route":"apps"}),
    )?;
    // Always release the private fixture, including before reporting a failure.
    fs::write(lab.home.path().join("release-fonts"), "ready")?;
    assert!(
        result.get("ok").is_some(),
        "Interactive query starved by background refresh: {}",
        result["err"]
    );
    loop {
        let update = observer.next()?;
        let revision = &update["data"]["providers_rev"];
        if revision.as_u64().is_some_and(|r| r > 0) {
            break;
        }
        if Instant::now() > deadline {
            return Err("Provider readiness did not notify menu subscribers".into());
        }
    }
    Ok(())
}
