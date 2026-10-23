//! N0039: always-on behaviours that only QML timers and scripts performed
//! (wallpaper rotation, automatic power profile, low-battery notices).
//!
//! Startup authority, like managed notifications: only `nacre-helper daemon
//! --behaviours=daemon` runs this module. The `behaviours` entry of the `daemon`
//! topic's `services` map is the signal for UIs to stand down.
use super::{
    hub::{Notice, Shared},
    menu,
};
use crate::{paths::Paths, state, wallpaper_media::Media, Result};
use futures_util::{
    future::LocalBoxFuture,
    stream::{LocalBoxStream, SelectAll},
    StreamExt,
};
use nacre_protocol::settings::{SettingsGet, SettingsPublish};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    rc::Rc,
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    time::Instant,
};
use zbus::zvariant::{OwnedValue, Value as Variant};

const UPOWER: &str = "org.freedesktop.UPower";
const UPOWER_PATH: &str = "/org/freedesktop/UPower";
const DISPLAY_DEVICE: &str = "/org/freedesktop/UPower/devices/DisplayDevice";
const NOTIFICATIONS: &str = "org.freedesktop.Notifications";
// Current and legacy power-profiles-daemon names, as powerprofilesctl tries them.
pub(super) const PROFILE_SERVICES: [(&str, &str); 2] = [
    (
        "org.freedesktop.UPower.PowerProfiles",
        "/org/freedesktop/UPower/PowerProfiles",
    ),
    ("net.hadess.PowerProfiles", "/net/hadess/PowerProfiles"),
];
const CATALOG_LIMIT: usize = 16 * 1024 * 1024;
const RETRY_AFTER_FAILURE_MS: i64 = 60_000;

// ---------------------------------------------------------------- shared

pub(super) type Streams = SelectAll<LocalBoxStream<'static, ()>>;

/// Wake-ups for property changes of the named services plus bus name changes.
pub(super) async fn signals(
    connection: &zbus::Connection,
    senders: &[&str],
    path: Option<&str>,
    namespace: Option<&str>,
) -> Result<Streams> {
    let mut streams = Streams::new();
    for sender in senders {
        let mut rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(*sender)?
            .interface("org.freedesktop.DBus.Properties")?
            .member("PropertiesChanged")?;
        if let Some(path) = path {
            rule = rule.path(path)?;
        }
        if let Some(namespace) = namespace {
            rule = rule.path_namespace(namespace)?;
        }
        streams.push(
            zbus::MessageStream::for_match_rule(rule.build(), connection, Some(64))
                .await?
                .map(|_| ())
                .boxed_local(),
        );
    }
    streams.push(
        zbus::fdo::DBusProxy::new(connection)
            .await?
            .receive_name_owner_changed()
            .await?
            .map(|_| ())
            .boxed_local(),
    );
    Ok(streams)
}

pub(super) async fn proxy<'a>(
    connection: &zbus::Connection,
    destination: &'a str,
    path: &'a str,
    interface: &'a str,
) -> zbus::Result<zbus::Proxy<'a>> {
    zbus::proxy::Builder::new(connection)
        .destination(destination)?
        .path(path)?
        .interface(interface)?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await
}

pub(super) async fn settings_data(hub: &Shared, store: &str) -> Option<Value> {
    let settings = hub.borrow().settings.clone()?;
    let state = settings
        .get(SettingsGet {
            store: store.into(),
        })
        .await
        .ok()?;
    (state["exists"] == true && state["error"].is_null()).then(|| state["data"].clone())
}

/// Wait for a notice naming one of `topics`; a lagged receiver means "re-read".
pub(super) async fn changed(
    notices: &mut tokio::sync::broadcast::Receiver<Notice>,
    topics: &[&str],
) -> Result<()> {
    use tokio::sync::broadcast::error::RecvError;
    loop {
        match notices.recv().await {
            Ok(Notice::Changed { topic, .. }) if topics.contains(&topic.as_str()) => return Ok(()),
            Ok(_) => {}
            Err(RecvError::Lagged(_)) => return Ok(()),
            Err(RecvError::Closed) => return Err("Daemon notices ended".into()),
        }
    }
}

// ------------------------------------------------------- wallpaper rotation

/// QML `wallpaper-rotation.js` `pool`: the wallpapers a rotation kind may use.
fn rotation_pool(entries: &[Value], kind: &str) -> Vec<String> {
    let mut pool: Vec<String> = Vec::new();
    for entry in entries {
        let Some(path) = entry["path"].as_str() else {
            continue;
        };
        if (kind == "all" || entry["dynamic"].as_bool() == Some(kind == "dynamic"))
            && !pool.iter().any(|known| known == path)
        {
            pool.push(path.to_owned());
        }
    }
    pool
}

/// QML `wallpaper-rotation.js` `next`: sequential, or a shuffled bag that never
/// repeats the current image until the bag is empty. Returns the choice and the bag.
pub(super) fn rotation_next(
    entries: &[String],
    current: &str,
    shuffle: bool,
    remaining: &[String],
    random: f64,
) -> (Option<String>, Vec<String>) {
    if entries.len() < 2 {
        return (None, Vec::new());
    }
    if !shuffle {
        let next = entries
            .iter()
            .position(|path| path == current)
            .map_or(0, |index| (index + 1) % entries.len());
        return (Some(entries[next].clone()), Vec::new());
    }
    let mut bag: Vec<String> = remaining
        .iter()
        .filter(|path| entries.contains(path) && path.as_str() != current)
        .cloned()
        .collect();
    if bag.is_empty() {
        bag = entries
            .iter()
            .filter(|path| path.as_str() != current)
            .cloned()
            .collect();
    }
    if bag.is_empty() {
        return (None, Vec::new());
    }
    let index = ((random * bag.len() as f64).floor() as usize).min(bag.len() - 1);
    (Some(bag.remove(index)), bag)
}

fn local_path(value: &str) -> bool {
    value.starts_with('/') && !value.contains('\0') && value.len() <= 16384
}

fn valid_entry(entry: &Value) -> bool {
    let path = |key: &str| entry[key].as_str().is_some_and(local_path);
    let optional = |key: &str| entry[key].is_null() || entry[key].as_str().is_some_and(local_path);
    path("path")
        && path("poster")
        && entry["name"].is_string()
        && entry["dynamic"].is_boolean()
        && entry["animated"].is_boolean()
        && optional("preview")
        && optional("thumbnail")
}

/// The inputs of one rotation decision, re-read from the files the UI used.
pub(super) struct Plan {
    enabled: bool,
    pub(super) kind: String,
    pub(super) shuffle: bool,
    interval_ms: i64,
    anchor_ms: i64,
    pub(super) current: String,
}

/// Preferences come from the daemon's own `wallpaper-picker` store (the helper's
/// `Media::settings` would call back into this daemon).
pub(super) async fn read_plan(hub: &Shared, paths: &Paths, startup_ms: i64) -> Result<Plan> {
    let media = Media::new(paths.clone(), paths.assets())?;
    let mut prefs = settings_data(hub, "wallpaper-picker")
        .await
        .unwrap_or(json!({}));
    let applied = media.media_state()?;
    let presentation = state::read(&paths.state.join("presentation.json"), json!({}))?;
    let number = |value: &Value| value.as_f64().unwrap_or(0.0) as i64;
    if prefs["rotationEnabled"] == true && number(&prefs["rotationAnchorMs"]) == 0 {
        // The helper's default for a store enabled before anchors existed: its mtime.
        if let Ok(modified) = state::settings_path_in(paths, "wallpaper-picker")
            .and_then(|path| Ok(std::fs::metadata(path)?.modified()?))
        {
            let since = modified
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default();
            prefs["rotationAnchorMs"] = json!(since.as_millis() as i64);
        }
    }
    let anchor = number(&applied["appliedAtMs"])
        .max(number(&presentation["changedAtMs"]))
        .max(number(&prefs["rotationAnchorMs"]));
    let last = std::fs::read_to_string(paths.state.join("wallpaper/last.txt"))
        .map(|text| text.trim().to_owned())
        .ok()
        .filter(|text| local_path(text))
        .unwrap_or_default();
    let selected = applied["path"].as_str().unwrap_or_default();
    let minutes = prefs["rotationMinutes"]
        .as_f64()
        .filter(|value| value.is_finite() && *value != 0.0)
        .unwrap_or(30.0);
    Ok(Plan {
        enabled: prefs["rotationEnabled"] == true,
        kind: prefs["rotationKind"].as_str().unwrap_or("all").to_owned(),
        shuffle: prefs["rotationShuffle"] != false,
        interval_ms: (minutes.clamp(5.0, 1440.0) * 60_000.0) as i64,
        anchor_ms: if anchor == 0 { startup_ms } else { anchor },
        current: if !selected.is_empty() && applied["poster"].as_str() == Some(last.as_str()) {
            selected.to_owned()
        } else {
            last
        },
    })
}

pub struct Env {
    pub engine: PathBuf,
    pub now_ms: Rc<dyn Fn() -> i64>,
    /// Resolves once `now_ms()` reaches the given wall-clock millisecond.
    pub sleep: Rc<dyn Fn(i64) -> LocalBoxFuture<'static, ()>>,
    pub random: Rc<dyn Fn() -> f64>,
}

impl Env {
    pub fn system() -> Result<Self> {
        fn now() -> i64 {
            chrono::Utc::now().timestamp_millis()
        }
        Ok(Self {
            engine: std::env::current_exe()?,
            now_ms: Rc::new(now),
            sleep: Rc::new(|target| {
                // One sleep to the deadline; wake-from-suspend re-plans through the
                // session watcher. Nothing polls while idle.
                let wait = (target - now()).clamp(1, 86_400_000);
                Box::pin(tokio::time::sleep(Duration::from_millis(wait as u64)))
            }),
            random: Rc::new(|| {
                let bits = uuid::Uuid::new_v4().as_u128() >> 75;
                bits as f64 / (1u64 << 53) as f64
            }),
        })
    }
}

/// Run a helper subcommand without a shell; bounded output and time.
async fn helper(
    engine: &Path,
    args: &[&str],
    limit: usize,
    timeout: Duration,
) -> Option<(bool, Vec<u8>)> {
    let mut child = Command::new(engine)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let mut output = Vec::new();
    let status = tokio::time::timeout(timeout, async {
        (&mut stdout)
            .take(limit as u64 + 1)
            .read_to_end(&mut output)
            .await?;
        child.wait().await
    })
    .await
    .ok()?
    .ok()?;
    (output.len() <= limit).then(|| (status.success(), output))
}

pub(super) async fn catalog_pool(engine: &Path, kind: &str) -> Option<Vec<String>> {
    let (success, output) = helper(
        engine,
        &["wallpaper-media", "catalog"],
        CATALOG_LIMIT,
        Duration::from_secs(120),
    )
    .await?;
    let data: Value = serde_json::from_slice(&output).ok()?;
    let entries = data["entries"].as_array()?;
    (success
        && data["error"].is_null()
        && entries.len() <= 10_000
        && entries.iter().all(valid_entry))
    .then(|| rotation_pool(entries, kind))
}

/// The existing selection path: the picker's `wallpaper-media select`, which writes
/// the media state and presentation record the daemon's `wallpaper.active` topic
/// publishes, so the palette follows.
pub(super) async fn select(engine: &Path, path: &str) -> bool {
    let Some((true, output)) = helper(
        engine,
        &["wallpaper-media", "select", path],
        1024 * 1024,
        Duration::from_secs(300),
    )
    .await
    else {
        return false;
    };
    String::from_utf8_lossy(&output)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .any(|entry| entry["error"].is_null() && valid_entry(&entry) && entry["path"] == path)
}

/// `wallpaper-media session --watch`: logind lock and sleep state, as the QML used it.
struct Session {
    _child: tokio::process::Child,
    lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
}
impl Session {
    fn spawn(engine: &Path) -> Result<Self> {
        let mut child = Command::new(engine)
            .args(["wallpaper-media", "session", "--watch"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()?;
        let stdout = child
            .stdout
            .take()
            .ok_or("Missing session watcher output")?;
        Ok(Self {
            _child: child,
            lines: BufReader::new(stdout).lines(),
        })
    }
    /// True while the session is asleep or locked.
    async fn next(&mut self) -> Result<bool> {
        let line = self
            .lines
            .next_line()
            .await?
            .ok_or("Session watcher ended")?;
        let state: Value = serde_json::from_str(&line)?;
        Ok(state["locked"] == true || state["sleeping"] == true)
    }
}

/// QML `NacreWallpapers` rotation: every `rotationMinutes` after the last change,
/// pick the next wallpaper of the configured kind (sequential or shuffled bag).
/// Paused while asleep or locked and with fewer than two wallpapers; a failed
/// change is retried after 60 s. Open picker/Settings pauses stay UI-side.
pub async fn rotation(hub: Shared, paths: Paths, env: Env) -> Result<()> {
    let mut notices = hub.borrow().observe();
    let startup = (env.now_ms)();
    let (mut retry, mut attempted) = (0i64, 0i64);
    let (mut bag, mut key) = (Vec::<String>::new(), String::new());
    // `blocked`: fewer than two wallpapers; only a library or settings change retries.
    let (mut blocked, mut paused) = (false, false);
    let mut session: Option<Session> = None;
    loop {
        let plan = read_plan(&hub, &paths, startup).await.ok();
        if plan.as_ref().is_some_and(|plan| plan.enabled) && session.is_none() {
            session = Some(Session::spawn(&env.engine)?);
        }
        let due = plan.as_ref().map_or(i64::MAX, |plan| {
            (plan.anchor_ms.max(attempted) + plan.interval_ms).max(retry)
        });
        let armed = plan.as_ref().is_some_and(|plan| plan.enabled) && !paused && !blocked;
        tokio::select! {
            () = (env.sleep)(due), if armed => {}
            result = changed(&mut notices, &["wallpaper.watch", "wallpaper.active", "settings.store.wallpaper-picker"]) => {
                result?;
                blocked = false;
                continue;
            }
            state = async { session.as_mut().expect("watcher").next().await }, if session.is_some() => {
                paused = state?;
                continue;
            }
        }
        let (now, Some(plan)) = ((env.now_ms)(), plan) else {
            continue;
        };
        if now < due {
            continue;
        }
        let Some(pool) = catalog_pool(&env.engine, &plan.kind).await else {
            retry = now + RETRY_AFTER_FAILURE_MS;
            continue;
        };
        if pool.len() < 2 {
            blocked = true;
            continue;
        }
        let identity = format!("{}:{}:{}", plan.kind, plan.shuffle, pool.join("|"));
        if identity != key {
            bag.clear();
            key = identity;
        }
        let (choice, remaining) =
            rotation_next(&pool, &plan.current, plan.shuffle, &bag, (env.random)());
        bag = remaining;
        if let Some(path) = choice {
            if select(&env.engine, &path).await {
                retry = 0;
                attempted = (env.now_ms)();
            } else {
                retry = (env.now_ms)() + RETRY_AFTER_FAILURE_MS;
            }
        }
    }
}

// ------------------------------------------------------------ power profile

fn desired_profile(preferences: Option<&Value>, on_battery: bool) -> String {
    preferences
        .filter(|data| data["enabled"] == true)
        .and_then(|data| data[if on_battery { "battery" } else { "ac" }].as_str())
        .unwrap_or_default()
        .to_owned()
}

async fn on_battery(system: &zbus::Connection) -> Option<bool> {
    proxy(system, UPOWER, UPOWER_PATH, UPOWER)
        .await
        .ok()?
        .get_property("OnBattery")
        .await
        .ok()
}

/// The reachable power-profiles-daemon: its proxy and whether it offers "performance".
async fn profile_service(system: &zbus::Connection) -> Option<(zbus::Proxy<'static>, bool)> {
    for (destination, path) in PROFILE_SERVICES {
        let Ok(service) = proxy(system, destination, path, destination).await else {
            continue;
        };
        if service
            .get_property::<String>("ActiveProfile")
            .await
            .is_err()
        {
            continue;
        }
        let profiles = service
            .get_property::<Vec<HashMap<String, OwnedValue>>>("Profiles")
            .await
            .unwrap_or_default();
        let performance = profiles.iter().any(|profile| {
            profile
                .get("Profile")
                .and_then(|value| String::try_from(value.clone()).ok())
                .is_some_and(|name| name == "performance")
        });
        return Some((service, performance));
    }
    None
}

/// QML `NacrePowerPolicy`: apply the configured profile for the power source once per
/// change; no retry after a failure until the source or the setting changes.
pub async fn power_profile(hub: Shared, system: zbus::Connection) -> Result<()> {
    let mut notices = hub.borrow().observe();
    let mut events = signals(
        &system,
        &[UPOWER, PROFILE_SERVICES[0].0, PROFILE_SERVICES[1].0],
        None,
        None,
    )
    .await?;
    let (mut applied, mut previous) = (String::new(), None::<bool>);
    loop {
        let battery = on_battery(&system).await.unwrap_or(false);
        if previous.is_some_and(|before| before != battery) {
            applied.clear();
        }
        previous = Some(battery);
        let desired = desired_profile(settings_data(&hub, "power").await.as_ref(), battery);
        if desired.is_empty() {
            applied.clear();
        } else if desired != applied
            && ["power-saver", "balanced", "performance"].contains(&desired.as_str())
        {
            if let Some((service, performance)) = profile_service(&system).await {
                if desired != "performance" || performance {
                    match service.set_property("ActiveProfile", desired.clone()).await {
                        Ok(()) => applied = desired,
                        Err(_) => tracing::warn!("The power profile could not be applied"),
                    }
                }
            }
        }
        tokio::select! {
            event = events.next() => { event.ok_or("System bus event stream ended")?; }
            result = changed(&mut notices, &["settings.store.power"]) => result?,
        }
    }
}

// ----------------------------------------------------------- battery alerts

#[derive(Clone, Copy, Debug, PartialEq)]
struct Sample {
    ready: bool,
    present: bool,
    discharging: bool,
    fraction: f64,
}

/// Severity (1 low, 2 critical), percentage and the epoch it was made in;
/// a reset in between voids it.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Request {
    level: u8,
    percent: u32,
    epoch: u64,
}

/// QML `NacreBatteryAlertPolicy`, unchanged: one notice per threshold crossing
/// (20 % low, 15 % critical) per discharge, re-armed by charging or recovery.
#[derive(Default)]
struct AlertPolicy {
    initialized: bool,
    severity: u8,
    epoch: u64,
    rearmed: bool,
}
impl AlertPolicy {
    fn initialize(&mut self, level: u8) {
        self.severity = level.min(2);
        self.initialized = true;
    }
    fn reset(&mut self) {
        self.severity = 0;
        self.epoch += 1;
        self.rearmed = true;
    }
    fn request(&mut self, sample: &Sample) -> Option<Request> {
        if !self.initialized || !sample.ready {
            return None;
        }
        if !sample.present {
            self.reset();
            return None;
        }
        if !sample.fraction.is_finite() || !(0.0..=1.0).contains(&sample.fraction) {
            return None;
        }
        if !sample.discharging || sample.fraction > 0.20 {
            self.reset();
            return None;
        }
        let level = if sample.fraction <= 0.15 { 2 } else { 1 };
        (level > self.severity).then(|| Request {
            level,
            percent: (sample.fraction * 100.0).round() as u32,
            epoch: self.epoch,
        })
    }
    fn delivered(&mut self, request: &Request) {
        if request.epoch == self.epoch {
            self.severity = self.severity.max(request.level);
        }
    }
}

async fn read_sample(system: &zbus::Connection) -> Sample {
    let unready = Sample {
        ready: false,
        present: false,
        discharging: false,
        fraction: f64::NAN,
    };
    let (Some(battery), Ok(device)) = (
        on_battery(system).await,
        proxy(
            system,
            UPOWER,
            DISPLAY_DEVICE,
            "org.freedesktop.UPower.Device",
        )
        .await,
    ) else {
        return unready;
    };
    let (Ok(kind), Ok(state), Ok(percentage), Ok(is_present)) = (
        device.get_property::<u32>("Type").await,
        device.get_property::<u32>("State").await,
        device.get_property::<f64>("Percentage").await,
        device.get_property::<bool>("IsPresent").await,
    ) else {
        return unready;
    };
    // Quickshell's isLaptopBattery: a power-supply battery device.
    let supply = device
        .get_property::<bool>("PowerSupply")
        .await
        .unwrap_or(true);
    let laptop = kind == 2 && supply;
    Sample {
        ready: !laptop || !is_present || state != 0,
        present: laptop && is_present,
        // Discharging, PendingDischarge, Empty.
        discharging: battery && matches!(state, 2 | 3 | 6),
        fraction: percentage / 100.0,
    }
}

pub struct Timing {
    pub settle: Duration,
    pub retry: Duration,
    pub watchdog: Duration,
}
impl Default for Timing {
    fn default() -> Self {
        Self {
            settle: Duration::from_millis(150),
            retry: Duration::from_secs(10),
            watchdog: Duration::from_secs(5),
        }
    }
}

async fn alerts_enabled(hub: &Shared) -> bool {
    settings_data(hub, "power")
        .await
        .is_none_or(|data| data["alerts"] != false)
}

async fn ledger_load(hub: &Shared) -> u8 {
    settings_data(hub, "battery-alerts")
        .await
        .and_then(|data| data["severity"].as_u64())
        .filter(|level| *level <= 2)
        .unwrap_or(0) as u8
}

async fn ledger_save(hub: &Shared, severity: u8) {
    let Some(settings) = hub.borrow().settings.clone() else {
        return;
    };
    for _ in 0..2 {
        let Ok(state) = settings
            .get(SettingsGet {
                store: "battery-alerts".into(),
            })
            .await
        else {
            return;
        };
        let request = SettingsPublish {
            store: "battery-alerts".into(),
            value: json!({"version":1,"severity":severity}),
            rev: state["rev"].as_u64().unwrap_or(0),
        };
        match settings.publish_owned(request).await {
            Ok(_) => return,
            Err(error) if error.code == nacre_protocol::ErrorCode::StaleRevision => {}
            Err(_) => {
                tracing::warn!("Battery warning state could not be saved");
                return;
            }
        }
    }
}

/// Post through the notification service on the session bus: the daemon's own
/// managed server when it owns the name, the shell's otherwise. Never claims the name.
async fn post_notice(
    hub: &Shared,
    paths: &Paths,
    session: &zbus::Connection,
    request: Request,
) -> bool {
    let desktop = settings_data(hub, "desktop").await.unwrap_or(json!({}));
    let language = menu::message_language(&desktop);
    let messages = menu::translations(paths, &language).unwrap_or(json!({}));
    let title = nacre_i18n::translate(
        &messages,
        &language,
        "battery.low.title",
        "Battery Low",
        &Value::Null,
    );
    let body = nacre_i18n::translate(
        &messages,
        &language,
        "battery.low.body",
        "Remaining: {percent}%",
        &json!({"percent": request.percent}),
    );
    let Ok(service) = proxy(
        session,
        NOTIFICATIONS,
        "/org/freedesktop/Notifications",
        NOTIFICATIONS,
    )
    .await
    else {
        return false;
    };
    let urgency = if request.level == 2 { 2u8 } else { 1u8 };
    let hints = HashMap::from([("urgency", Variant::U8(urgency))]);
    service
        .call::<_, _, u32>(
            "Notify",
            &(
                "Nacre Battery",
                0u32,
                "nacre-notifications",
                title,
                body,
                Vec::<&str>::new(),
                hints,
                -1i32,
            ),
        )
        .await
        .is_ok()
}

/// QML `NacreBatteryAlerts`: evaluate 150 ms after the latest battery change; a
/// failed delivery is retried every 10 s up to three failures; a delivery that
/// was overtaken by a reset does not count. The severity ledger is the
/// `battery-alerts` store the QML already wrote through the daemon.
pub async fn battery_alerts(
    hub: Shared,
    paths: Paths,
    system: zbus::Connection,
    session: zbus::Connection,
    timing: Timing,
) -> Result<()> {
    let mut events = signals(&system, &[UPOWER], None, Some(UPOWER_PATH)).await?;
    let mut notices = hub.borrow().observe();
    let mut policy = AlertPolicy::default();
    policy.initialize(ledger_load(&hub).await);
    let (mut failures, mut saved) = (0u8, policy.severity);
    let mut settle = Some(Instant::now() + timing.settle);
    let mut retry: Option<Instant> = None;
    let mut sending: Option<(Request, LocalBoxFuture<'static, bool>)> = None;
    loop {
        let timer = settle.into_iter().chain(retry).min();
        let evaluate = tokio::select! {
            event = events.next() => {
                event.ok_or("System bus event stream ended")?;
                settle = Some(Instant::now() + timing.settle);
                false
            }
            result = changed(&mut notices, &["settings.store.power"]) => {
                result?;
                settle = Some(Instant::now() + timing.settle);
                false
            }
            () = async { tokio::time::sleep_until(timer.expect("timer")).await }, if timer.is_some() => {
                let now = Instant::now();
                if settle.is_some_and(|at| at <= now) { settle = None; }
                if retry.is_some_and(|at| at <= now) { retry = None; }
                true
            }
            success = async { sending.as_mut().expect("send").1.as_mut().await }, if sending.is_some() => {
                let (request, _) = sending.take().expect("send");
                if request.epoch != policy.epoch {
                    settle = Some(Instant::now() + timing.settle);
                } else if success {
                    policy.delivered(&request);
                    failures = 0;
                    settle = Some(Instant::now() + timing.settle);
                } else {
                    failures = (failures + 1).min(3);
                    if failures < 3 {
                        retry = Some(Instant::now() + timing.retry);
                    }
                }
                false
            }
        };
        if evaluate {
            let request = policy.request(&read_sample(&system).await);
            if std::mem::take(&mut policy.rearmed) {
                failures = 0;
                retry = None;
            }
            match request {
                None => retry = None,
                // Completion reevaluates the latest snapshot instead of queueing.
                Some(_) if sending.is_some() => {}
                // The Power page's "Battery alerts" switch (default on) silences notices.
                Some(_) if !alerts_enabled(&hub).await => retry = None,
                Some(request) => {
                    let (hub, paths, session) = (hub.clone(), paths.clone(), session.clone());
                    let watchdog = timing.watchdog;
                    sending = Some((
                        request,
                        Box::pin(async move {
                            tokio::time::timeout(
                                watchdog,
                                post_notice(&hub, &paths, &session, request),
                            )
                            .await
                            .unwrap_or(false)
                        }),
                    ));
                }
            }
        }
        if policy.severity != saved {
            saved = policy.severity;
            ledger_save(&hub, saved).await;
        }
    }
}

// ------------------------------------------------------------------- start

/// Production composition; the caller supervises each behaviour separately.
pub async fn run_rotation(hub: Shared, paths: Paths) -> Result<()> {
    rotation(hub, paths, Env::system()?).await
}
pub async fn run_power_profile(hub: Shared) -> Result<()> {
    power_profile(hub, zbus::Connection::system().await?).await
}
pub async fn run_battery_alerts(hub: Shared, paths: Paths) -> Result<()> {
    battery_alerts(
        hub,
        paths,
        zbus::Connection::system().await?,
        zbus::Connection::session().await?,
        Timing::default(),
    )
    .await
}

#[cfg(test)]
mod tests;
