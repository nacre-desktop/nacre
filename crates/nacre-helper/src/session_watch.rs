//! Readiness and login/resume/output evidence. No lock, power or launch policy.
use crate::{compositor::backend::Backend, json, paths::Paths, process, state, Result};
use serde_json::{json as value, Value};
use std::{
    fs,
    io::Write,
    net::Shutdown,
    os::unix::{
        fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
        net::UnixStream,
    },
    path::Path,
    process::Command,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub fn record(
    event: &str,
    checked: &str,
    desktop: &str,
    wallet: &str,
    monitors: &Value,
    windows: &Value,
    errors: &Value,
) -> Result<Value> {
    let desktop: Value = serde_json::from_str(desktop).unwrap_or(Value::Null);
    let ready = match &desktop {
        Value::Object(object) => object.contains_key("launcherMode"),
        Value::Array(array) => array.contains(&value!("launcherMode")),
        Value::String(text) => text.contains("launcherMode"),
        _ => false,
    };
    let monitors = monitors
        .as_array()
        .ok_or("Invalid monitor records")?
        .iter()
        .map(|monitor| {
            let mut result = serde_json::Map::new();
            for key in ["name", "width", "height", "scale"] {
                result.insert(
                    key.into(),
                    monitor.get(key).ok_or("Incomplete monitor record")?.clone(),
                );
            }
            Ok(Value::Object(result))
        })
        .collect::<Result<Vec<_>>>()?;
    let count = windows
        .as_array()
        .ok_or("Invalid window records")?
        .iter()
        .filter(|window| {
            matches!(
                window["class"].as_str(),
                Some("nacre-brain" | "chrome-127.0.0.1__-Default")
            )
        })
        .count();
    Ok(
        value!({"event":event,"checked":checked,"desktopReady":ready,
        "wallet":match wallet {"b false"=>"unlocked","b true"=>"locked",_=>"unavailable"},
        "monitors":monitors,"brainWindows":count,"configErrors":errors}),
    )
}

fn private_directory(path: &Path) -> Result<()> {
    let mut current = std::path::PathBuf::new();
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) if !meta.is_dir() => {
                return Err("Refusing linked or non-directory session state".into())
            }
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new()
                    .mode(if current == path { 0o700 } else { 0o777 })
                    .create(&current)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    let meta = fs::metadata(path)?;
    if meta.uid() != nix::unistd::getuid().as_raw() {
        return Err("Session state is owned by another user".into());
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}
pub fn publish(directory: &Path, record: &Value) -> Result<()> {
    private_directory(directory)?;
    // Validate all destinations before writing; no transient broad permissions.
    for name in [
        "latest.next",
        "latest.json",
        "latest.json.schema.lock",
        "events.jsonl",
    ] {
        match fs::symlink_metadata(directory.join(name)) {
            Ok(meta)
                if !meta.is_file()
                    || meta.nlink() != 1
                    || meta.uid() != nix::unistd::getuid().as_raw() =>
            {
                return Err("Refusing linked or foreign session evidence".into());
            }
            Ok(_) => fs::set_permissions(directory.join(name), fs::Permissions::from_mode(0o600))?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => return Err(error.into()),
        }
    }
    // latest.next was an unobserved scratch file; atomic schema publication owns
    // the real transaction. Remove an old regular scratch file after success.
    let mut stored = record.clone();
    stored["version"] = value!(1);
    if !state::write_text(
        &directory.join("latest.json"),
        &format!("{}\n", json::dumps(&stored, false)?),
    )? {
        crate::files::atomic(
            &directory.join("latest.json"),
            format!("{}\n", json::dumps(record, false)?).as_bytes(),
        )?;
    }
    match fs::remove_file(directory.join("latest.next")) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let mut stream = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(directory.join("events.jsonl"))?;
    let mut event = record.clone();
    event["version"] = value!(1);
    writeln!(stream, "{}", json::dumps(&event, false)?)?;
    Ok(())
}

pub fn connected_displays(
    capabilities: &Value,
    data: Option<&Value>,
    monitors: &Value,
) -> Result<Value> {
    if capabilities["monitorConfiguration"] == false || data.is_none() {
        return Ok(value!([]));
    }
    let monitors = monitors.as_array().ok_or("Invalid monitor records")?;
    let mut saved = Vec::new();
    if let Some(displays) = data.unwrap().get("displays") {
        for display in displays.as_array().ok_or("Invalid saved displays")? {
            let name = display.get("name").ok_or("Missing saved display name")?;
            if monitors
                .iter()
                .any(|monitor| monitor.get("name") == Some(name))
            {
                saved.push(display.clone());
            }
        }
    }
    Ok(value!(saved))
}

/// Test seam for the fixed policy, also used by the real event observer.
pub trait Observer {
    fn check(&mut self, event: &str) -> Result<Value>;
    fn restore(&mut self) -> Result<()>;
    fn elapsed(&self) -> Duration;
    fn sleep(&mut self, duration: Duration) {
        thread::sleep(duration);
    }
}
pub fn readiness(observer: &mut impl Observer, timeout: Duration) -> Result<Value> {
    let deadline = observer.elapsed() + timeout;
    while observer.elapsed() < deadline {
        let record = observer.check("startup-readiness")?;
        if record["desktopReady"] == true
            && matches!(record["wallet"].as_str(), Some("unlocked" | "unavailable"))
        {
            return Ok(record);
        }
        observer.sleep(Duration::from_millis(500));
    }
    observer.check("startup-timeout")
}
pub enum Event {
    Sleep(bool),
    Outputs,
}
pub trait Events {
    fn next(&mut self) -> Result<Event>;
    fn close(&mut self) -> Result<()>;
}
pub fn observe<O: Observer, E: Events>(
    observer: &mut O,
    connect: impl FnOnce(&mut O) -> Result<E>,
) -> Result<()> {
    observer.check("observer-start")?;
    observer.restore()?;
    let mut events = connect(observer)?;
    let result = (|| -> Result<()> {
        loop {
            match events.next()? {
                Event::Sleep(true) => {
                    observer.check("before-sleep")?;
                }
                Event::Sleep(false) => {
                    observer.sleep(Duration::from_millis(500));
                    observer.restore()?;
                    observer.check("resume")?;
                }
                Event::Outputs => {
                    observer.sleep(Duration::from_millis(300));
                    observer.restore()?;
                    observer.check("display-hotplug")?;
                }
            }
        }
    })();
    let cleanup = events.close();
    if let Err(error) = &cleanup {
        eprintln!("Session observer cleanup: {error}");
    }
    result.and(cleanup)
}

pub struct Session {
    pub paths: Paths,
    backend: Backend,
    started: Instant,
    wallet: Option<zbus::blocking::Connection>,
}
impl Session {
    pub fn new(paths: Paths) -> Result<Self> {
        Ok(Self {
            backend: Backend::new(paths.clone(), std::env::vars().collect())?,
            paths,
            started: Instant::now(),
            wallet: None,
        })
    }
    fn wallet(&mut self) -> Result<&'static str> {
        if self.wallet.is_none() {
            self.wallet = Some(
                zbus::blocking::connection::Builder::session()?
                    .method_timeout(Duration::from_secs(5))
                    .build()?,
            );
        }
        let proxy = zbus::blocking::Proxy::new(
            self.wallet.as_ref().unwrap(),
            "org.freedesktop.secrets",
            "/org/freedesktop/secrets/aliases/default",
            "org.freedesktop.Secret.Collection",
        )?;
        let locked: bool = proxy.get_property("Locked")?;
        Ok(if locked { "b true" } else { "b false" })
    }
    fn query(&mut self, kind: &str) -> Value {
        match self.backend.query(kind, Duration::from_secs(5), None) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Session {kind}: {error}");
                value!([])
            }
        }
    }
}
impl Observer for Session {
    fn check(&mut self, event: &str) -> Result<Value> {
        let mut command = Command::new(self.paths.share.join("shell/bin/qs"));
        command.args(["-c", "nacre", "ipc", "call", "nacre", "state"]);
        let desktop = match process::capture(command, None, Duration::from_secs(5)) {
            Ok(output) if output.status.success() => {
                String::from_utf8(output.stdout)?.trim().to_owned()
            }
            Ok(output) => {
                eprintln!("Session shell state: {}", output.status);
                String::new()
            }
            Err(error) => {
                eprintln!("Session shell state: {error}");
                String::new()
            }
        };
        let wallet = match self.wallet() {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Session wallet unavailable: {error}");
                self.wallet = None;
                ""
            }
        };
        let monitors = self.query("monitors");
        let windows = self.query("windows");
        let errors = match self
            .backend
            .module_call("config_errors", &[], &value!({"timeout":5}))
        {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Session config errors unavailable: {error}");
                value!("")
            }
        };
        let record = record(
            event,
            &chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.6f+00:00")
                .to_string(),
            &desktop,
            wallet,
            &monitors,
            &windows,
            &errors,
        )?;
        publish(&self.paths.state.join("session"), &record)?;
        Ok(record)
    }
    fn restore(&mut self) -> Result<()> {
        if self.backend.capabilities()["monitorConfiguration"] == false {
            return Ok(());
        }
        let data = match fs::read(self.paths.config.join("desktop.json")) {
            Ok(bytes) => serde_json::from_slice::<Value>(&bytes)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let monitors = self.query("monitors");
        let saved = connected_displays(&self.backend.capabilities(), Some(&data), &monitors)?;
        if saved.as_array().is_some_and(|saved| !saved.is_empty()) {
            self.backend
                .action("configure_monitors", &[saved], &value!({"timeout":null}))?;
        }
        Ok(())
    }
    fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }
}

enum CompositorEvents {
    Hyprland(crate::compositor::hyprland_events::Events),
    Niri(
        Box<crate::compositor::niri_events::Events<crate::compositor::niri_transport::Connection>>,
    ),
}
impl CompositorEvents {
    fn next(&mut self) -> Result<Value> {
        match self {
            Self::Hyprland(events) => events.next_value()?.ok_or_else(|| ended().into()),
            Self::Niri(events) => Ok(events.next_value()?),
        }
    }
}
fn ended() -> String {
    crate::i18n::tr(
        "helper.session-watch.session-observer-ended",
        "Session observer ended",
    )
}
struct LiveEvents {
    receiver: mpsc::Receiver<std::result::Result<Event, String>>,
    connection: Option<zbus::blocking::Connection>,
    socket: UnixStream,
    workers: Vec<thread::JoinHandle<()>>,
}
impl LiveEvents {
    fn new(backend: &mut Backend) -> Result<Self> {
        use zbus::{
            blocking::{Connection, MessageIterator},
            message::Type,
            MatchRule,
        };
        let connection = Connection::system()?;
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.login1")?
            .path("/org/freedesktop/login1")?
            .interface("org.freedesktop.login1.Manager")?
            .member("PrepareForSleep")?
            .build();
        let messages = MessageIterator::for_match_rule(rule, &connection, Some(128))?;
        let (mut events, socket) = match backend {
            Backend::Hyprland(backend) => {
                let events = backend.events()?;
                let socket = events.control()?;
                (CompositorEvents::Hyprland(events), socket)
            }
            Backend::Niri(backend) => {
                let events = backend.events()?;
                let socket = events.control()?;
                (CompositorEvents::Niri(Box::new(events)), socket)
            }
        };
        let (sender, receiver) = mpsc::sync_channel(128);
        let logind = sender.clone();
        let workers = vec![
            thread::spawn(move || {
                let result = (|| -> Result<()> {
                    for message in messages {
                        let (sleeping,): (bool,) = message?.body().deserialize()?;
                        if logind.send(Ok(Event::Sleep(sleeping))).is_err() {
                            return Ok(());
                        }
                    }
                    Err(ended().into())
                })();
                if let Err(error) = result {
                    let _ = logind.send(Err(error.to_string()));
                }
            }),
            thread::spawn(move || {
                let result = (|| -> Result<()> {
                    loop {
                        if events.next()?["type"] == "outputs-changed"
                            && sender.send(Ok(Event::Outputs)).is_err()
                        {
                            return Ok(());
                        }
                    }
                })();
                if let Err(error) = result {
                    let _ = sender.send(Err(error.to_string()));
                }
            }),
        ];
        Ok(Self {
            receiver,
            connection: Some(connection),
            socket,
            workers,
        })
    }
}
impl Events for LiveEvents {
    fn next(&mut self) -> Result<Event> {
        self.receiver.recv().map_err(|_| ended())?.map_err(|error| {
            eprintln!("Session event stream: {error}");
            ended().into()
        })
    }
    fn close(&mut self) -> Result<()> {
        drop(std::mem::replace(&mut self.receiver, mpsc::channel().1));
        let mut failure = None;
        if let Some(connection) = self.connection.take() {
            if let Err(error) = connection.close() {
                failure = Some(error.to_string());
            }
        }
        if let Err(error) = self.socket.shutdown(Shutdown::Both) {
            if error.kind() != std::io::ErrorKind::NotConnected {
                failure = Some(error.to_string());
            }
        }
        for worker in self.workers.drain(..) {
            if worker.join().is_err() {
                failure = Some("Session event worker failed".into());
            }
        }
        match failure {
            Some(error) => Err(error.into()),
            None => Ok(()),
        }
    }
}

const USAGE: &str = "usage: session-watch.py [-h] [--event EVENT] {check,ready,watch}\n";
pub fn run(args: &[String]) -> Result<i32> {
    let mut action = None;
    let mut event = "manual".to_owned();
    let mut unknown = Vec::new();
    let mut index = 0;
    let error = |text: String| {
        eprintln!("{USAGE}session-watch.py: error: {text}");
        2
    };
    while index < args.len() {
        let arg = &args[index];
        if arg == "--help" || arg == "-h" {
            print!("{USAGE}\npositional arguments:\n  {{check,ready,watch}}\n\noptions:\n  -h, --help           show this help message and exit\n  --event EVENT\n");
            return Ok(0);
        } else if let Some(value) = arg.strip_prefix("--help=") {
            return Ok(error(format!(
                "argument -h/--help: ignored explicit argument '{value}'"
            )));
        } else if arg.starts_with("--") && "--event".starts_with(arg.split('=').next().unwrap()) {
            if let Some((_, value)) = arg.split_once('=') {
                event = value.to_owned();
            } else {
                index += 1;
                let Some(value) = args.get(index).filter(|v| !v.starts_with('-')) else {
                    return Ok(error("argument --event: expected one argument".into()));
                };
                event = value.clone();
            }
        } else if !arg.starts_with('-') && action.is_none() {
            if !["check", "ready", "watch"].contains(&arg.as_str()) {
                return Ok(error(format!("argument action: invalid choice: '{arg}' (choose from 'check', 'ready', 'watch')")));
            }
            action = Some(arg.as_str());
        } else {
            unknown.push(arg.as_str());
        }
        index += 1;
    }
    let Some(action) = action else {
        return Ok(error("the following arguments are required: action".into()));
    };
    if !unknown.is_empty() {
        return Ok(error(format!(
            "unrecognized arguments: {}",
            unknown.join(" ")
        )));
    }
    let mut session = Session::new(Paths::environment()?)?;
    if action == "watch" {
        observe(&mut session, |session| {
            LiveEvents::new(&mut session.backend)
        })?;
    } else {
        let result = if action == "ready" {
            readiness(&mut session, Duration::from_secs(15))?
        } else {
            session.check(&event)?
        };
        println!("{}", json::dumps(&result, false)?);
    }
    Ok(0)
}
