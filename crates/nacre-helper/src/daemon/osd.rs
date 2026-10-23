//! N0043: the `osd` event topic. While at least one client subscribes, producers watch
//! the audio defaults, caps lock, night light and power profile and report changes
//! after a silent baseline; brightness and keyboard backlight are reported by their
//! methods (`bar_services`), the way the QML OSD reacted to its own adjustments.
//! Nothing runs, polls or is retained while nobody subscribes.
use super::{
    behaviours::{proxy, signals, Streams, PROFILE_SERVICES},
    hub::Shared,
};
use crate::{paths::Paths, Result};
use futures_util::{stream::LocalBoxStream, Stream, StreamExt};
use nacre_protocol::{
    core_services::{CoreSnapshot, Health, OsdState},
    osd::{OsdEvent, OsdKind, EVENT, TOPIC},
};
use serde_json::Value;
use std::{
    cell::RefCell,
    ffi::OsString,
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, BufReader},
    process::Command,
    time::Instant,
};

/// Bursts within this window become one event (QML `Qt.callLater`): the last wins,
/// but an output change is never replaced by the volume notification it causes.
const SETTLE: Duration = Duration::from_millis(40);
const AUDIO_DEBOUNCE: Duration = Duration::from_millis(25);
/// An identical event this soon after another (a method report and the bus signal for
/// the same key press) is dropped.
const DUPLICATE: Duration = Duration::from_millis(250);
const KEYBOARD_PATH: &str = "/org/freedesktop/UPower/KbdBacklight";
const KEYBOARD_IFACE: &str = "org.freedesktop.UPower.KbdBacklight";
const UPOWER: &str = "org.freedesktop.UPower";
const SINK: &str = "@DEFAULT_AUDIO_SINK@";
const SOURCE: &str = "@DEFAULT_AUDIO_SOURCE@";

pub fn initial(hub: &Shared) -> Result<()> {
    let state = OsdState {
        health: Health {
            available: true,
            stale: false,
            error: None,
        },
        current: None,
    };
    debug_assert!(CoreSnapshot::Osd(state.clone()).valid());
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

thread_local! {
    static LAST: RefCell<Option<(OsdEvent, std::time::Instant)>> = const { RefCell::new(None) };
}

/// Publish one OSD event. Invalid events and quick duplicates are dropped.
pub fn report(hub: &Shared, event: OsdEvent) {
    if !event.valid() {
        tracing::warn!("Dropped an invalid OSD event");
        return;
    }
    let now = std::time::Instant::now();
    let duplicate = LAST.with(|last| {
        let mut last = last.borrow_mut();
        let same = last.as_ref().is_some_and(|(before, at)| {
            now.duration_since(*at) < DUPLICATE
                && *before
                    == OsdEvent {
                        ts: before.ts,
                        ..event.clone()
                    }
        });
        if !same {
            *last = Some((event.clone(), now));
        }
        same
    });
    if duplicate {
        return;
    }
    match serde_json::to_value(&event) {
        Ok(data) => {
            if hub.borrow_mut().event(TOPIC, EVENT, data).is_err() {
                tracing::warn!("OSD event could not be published");
            }
        }
        Err(_) => tracing::warn!("OSD event could not be encoded"),
    }
}

/// `brightness.set` / `brightness.adjust` reply: `{output, level_percent}`.
pub fn brightness(hub: &Shared, reply: &Value) {
    let (Some(output), Some(percent)) = (reply["output"].as_str(), reply["level_percent"].as_i64())
    else {
        return;
    };
    let mut event = OsdEvent::new(OsdKind::Brightness, now_ms());
    event.value = Some((percent as f64 / 100.0).clamp(0.0, 1.0));
    event.device = Some(output.to_owned());
    report(hub, event);
}

/// `keyboard_light.adjust` reply level and maximum (raw steps).
pub fn keyboard(hub: &Shared, level: i64, max: i64) {
    if let (Ok(max), true) = (u32::try_from(max), level >= 0 && max > 0) {
        let mut event = OsdEvent::new(OsdKind::Keyboard, now_ms());
        event.value = Some((level as f64 / f64::from(max)).clamp(0.0, 1.0));
        event.max = Some(max);
        report(hub, event);
    }
}

// ---------------------------------------------------------------- sources

#[derive(Clone)]
pub struct Env {
    pub wpctl: OsString,
    pub pactl: OsString,
    /// `/sys/class/leds`
    pub leds: PathBuf,
    /// The night-light ownership records of the two compositor backends.
    pub night: Vec<PathBuf>,
    pub tick: Duration,
}
impl Env {
    pub fn production(paths: &Paths) -> Self {
        Self {
            wpctl: "wpctl".into(),
            pactl: "pactl".into(),
            leds: "/sys/class/leds".into(),
            night: ["control-tools.json", "control-tools-niri.json"]
                .map(|name| paths.state.join(name))
                .into(),
            tick: Duration::from_millis(250),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Node {
    name: String,
    description: String,
    volume: f64,
    muted: bool,
}

async fn tool(program: &OsString, args: &[&str]) -> Option<String> {
    let mut child = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut out = String::new();
    let mut stdout = child.stdout.take()?.take(16 * 1024);
    tokio::time::timeout(Duration::from_secs(2), async {
        stdout.read_to_string(&mut out).await.ok()?;
        child.wait().await.ok().filter(|s| s.success())
    })
    .await
    .ok()??;
    Some(out)
}

/// `wpctl get-volume` ("Volume: 0.50 [MUTED]") plus the node identity from `wpctl inspect`.
async fn read_node(wpctl: &OsString, target: &str) -> Option<Node> {
    let volume = tool(wpctl, &["get-volume", target]).await?;
    let value: f64 = volume
        .strip_prefix("Volume:")?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    if !(0.0..=4.0).contains(&value) {
        return None;
    }
    let inspect = tool(wpctl, &["inspect", target]).await?;
    let property = |key: &str| {
        inspect.lines().find_map(|line| {
            let line = line.trim_start_matches([' ', '*', '\t']);
            let rest = line
                .strip_prefix(key)?
                .trim_start()
                .strip_prefix('=')?
                .trim();
            Some(rest.trim_matches('"').to_owned())
        })
    };
    let name = property("node.name").filter(|n| !n.is_empty() && n.len() <= 128)?;
    let description = property("node.description")
        .or_else(|| property("node.nick"))
        .unwrap_or_default();
    Some(Node {
        name,
        description: description.chars().take(200).collect(),
        volume: (value * 1000.0).round() / 1000.0,
        muted: volume.contains("[MUTED]"),
    })
}

/// A change of the default device is `switched` (when it has an OSD); the same device
/// with another volume or mute state is a `level` change. The first read is the baseline.
fn audio_event(
    before: Option<&Node>,
    now: &Node,
    level: OsdKind,
    switched: Option<OsdKind>,
) -> Option<OsdEvent> {
    let before = before?;
    let kind = if before.name != now.name {
        switched?
    } else if before.muted != now.muted || (before.volume - now.volume).abs() > 0.0005 {
        level
    } else {
        return None;
    };
    let mut event = OsdEvent::new(kind, now_ms());
    event.value = Some(now.volume);
    event.muted = Some(now.muted);
    event.label = Some(now.description.clone()).filter(|d| !d.is_empty());
    event.device = Some(now.name.clone());
    Some(event)
}

fn read_flag(path: &Path) -> Option<bool> {
    match std::fs::read_to_string(path).ok()?.trim() {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

fn find_capslock(leds: &Path) -> Option<PathBuf> {
    let mut found = std::fs::read_dir(leds)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy().ends_with("::capslock"))
        .map(|entry| entry.path().join("brightness"))
        .collect::<Vec<_>>();
    found.sort();
    found.into_iter().next()
}

/// The ownership record `control-tools night-light` writes: on only while its owner runs.
fn read_night(records: &[PathBuf]) -> bool {
    records.iter().any(|path| {
        std::fs::metadata(path).is_ok_and(|m| m.len() <= 64 * 1024)
            && std::fs::read(path)
                .ok()
                .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                .is_some_and(|data| {
                    data["enabled"] == true && data["owner"].as_str().is_some_and(|o| !o.is_empty())
                })
    })
}

async fn active_profile(system: &zbus::Connection) -> Option<String> {
    for (destination, path) in PROFILE_SERVICES {
        if let Ok(service) = proxy(system, destination, path, destination).await {
            if let Ok(profile) = service.get_property::<String>("ActiveProfile").await {
                return ["power-saver", "balanced", "performance"]
                    .contains(&profile.as_str())
                    .then_some(profile);
            }
        }
    }
    None
}

async fn next_or_pending<T>(stream: &mut (impl Stream<Item = T> + Unpin)) -> T {
    match stream.next().await {
        Some(item) => item,
        None => std::future::pending().await,
    }
}

#[derive(Default)]
struct Pending {
    event: Option<OsdEvent>,
    at: Option<Instant>,
}
impl Pending {
    fn add(&mut self, event: OsdEvent) {
        if event.kind == OsdKind::Volume
            && self
                .event
                .as_ref()
                .is_some_and(|e| e.kind == OsdKind::Output)
        {
            return;
        }
        self.event = Some(event);
        self.at.get_or_insert(Instant::now() + SETTLE);
    }
}

async fn due(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// One subscribed period. Returns only on a lost audio subscription (the supervisor
/// restarts it, which takes a fresh baseline) or when dropped.
pub async fn session(hub: Shared, env: Env, system: Option<zbus::Connection>) -> Result<()> {
    let mut audio_child = Command::new(&env.pactl)
        .arg("subscribe")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok();
    let mut lines: LocalBoxStream<'static, String> =
        match audio_child.as_mut().and_then(|child| child.stdout.take()) {
            Some(stdout) => {
                futures_util::stream::unfold(BufReader::new(stdout).lines(), |mut l| async move {
                    let line = l.next_line().await.ok().flatten()?;
                    Some((line, l))
                })
                .boxed_local()
            }
            None => futures_util::stream::pending().boxed_local(),
        };
    let audio_live = audio_child.is_some();

    let mut power: Streams = Streams::new();
    let mut backlight: LocalBoxStream<'static, i32> = futures_util::stream::pending().boxed_local();
    let mut keyboard_max = 0i32;
    if let Some(system) = &system {
        if let Ok(streams) = signals(
            system,
            &[PROFILE_SERVICES[0].0, PROFILE_SERVICES[1].0],
            None,
            None,
        )
        .await
        {
            power = streams;
        }
        if let Ok(light) = proxy(system, UPOWER, KEYBOARD_PATH, KEYBOARD_IFACE).await {
            if let Ok(max) = light.call::<_, _, i32>("GetMaxBrightness", &()).await {
                if let Ok(changes) = light.receive_signal("BrightnessChanged").await {
                    keyboard_max = max;
                    backlight = changes
                        .filter_map(
                            |message| async move { message.body().deserialize::<i32>().ok() },
                        )
                        .boxed_local();
                }
            }
        }
    }

    // Baseline: nothing below is reported until it differs from what is read here.
    let mut sink = read_node(&env.wpctl, SINK).await;
    let mut source = read_node(&env.wpctl, SOURCE).await;
    let mut capslock_path = find_capslock(&env.leds);
    let mut caps = capslock_path.as_deref().and_then(read_flag);
    let mut night = read_night(&env.night);
    let mut profile = match &system {
        Some(system) => active_profile(system).await,
        None => None,
    };

    let mut pending = Pending::default();
    let (mut dirty_sink, mut dirty_source, mut audio_at) = (false, false, None::<Instant>);
    let mut ticker = tokio::time::interval_at(Instant::now() + env.tick, env.tick);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut count = 0u32;
    loop {
        tokio::select! {
            line = lines.next() => {
                let Some(line) = line else {
                    if audio_live { return Err("Audio event stream ended".into()); }
                    continue;
                };
                let server = line.ends_with(" on server");
                let relevant = |what: &str| line.contains(&format!(" on {what} #"));
                let (s, m) = (server || relevant("sink"), server || relevant("source"));
                if s || m {
                    dirty_sink |= s;
                    dirty_source |= m;
                    audio_at.get_or_insert(Instant::now() + AUDIO_DEBOUNCE);
                }
            }
            () = due(audio_at) => {
                audio_at = None;
                if std::mem::take(&mut dirty_sink) {
                    if let Some(now) = read_node(&env.wpctl, SINK).await {
                        if let Some(event) = audio_event(sink.as_ref(), &now, OsdKind::Volume, Some(OsdKind::Output)) {
                            pending.add(event);
                        }
                        sink = Some(now);
                    }
                }
                if std::mem::take(&mut dirty_source) {
                    if let Some(now) = read_node(&env.wpctl, SOURCE).await {
                        if let Some(event) = audio_event(source.as_ref(), &now, OsdKind::Microphone, None) {
                            pending.add(event);
                        }
                        source = Some(now);
                    }
                }
            }
            _ = ticker.tick() => {
                count = count.wrapping_add(1);
                if capslock_path.is_none() {
                    capslock_path = find_capslock(&env.leds);
                }
                if let Some(next) = capslock_path.as_deref().and_then(read_flag) {
                    if caps.is_some_and(|before| before != next) {
                        let mut event = OsdEvent::new(OsdKind::Capslock, now_ms());
                        event.value = Some(f64::from(u8::from(next)));
                        pending.add(event);
                    }
                    caps = Some(next);
                } else {
                    capslock_path = None;
                }
                if count.is_multiple_of(4) {
                    let next = read_night(&env.night);
                    if next != night {
                        let mut event = OsdEvent::new(OsdKind::NightLight, now_ms());
                        event.value = Some(f64::from(u8::from(next)));
                        pending.add(event);
                        night = next;
                    }
                }
            }
            () = next_or_pending(&mut power) => {
                if let Some(system) = &system {
                    if let Some(next) = active_profile(system).await {
                        if profile.as_ref().is_some_and(|before| *before != next) {
                            let mut event = OsdEvent::new(OsdKind::PowerProfile, now_ms());
                            event.label = Some(next.clone());
                            pending.add(event);
                        }
                        profile = Some(next);
                    }
                }
            }
            level = next_or_pending(&mut backlight) => {
                if keyboard_max > 0 && level >= 0 {
                    keyboard(&hub, i64::from(level), i64::from(keyboard_max));
                }
            }
            () = due(pending.at) => {
                pending.at = None;
                if let Some(event) = pending.event.take() {
                    report(&hub, event);
                }
            }
        }
    }
}

/// Wait until the last subscriber is gone.
async fn idle(interest: &mut tokio::sync::watch::Receiver<bool>) -> Result<()> {
    while *interest.borrow() {
        interest.changed().await?;
    }
    Ok(())
}

pub async fn run(hub: Shared, paths: Paths) -> Result<()> {
    serve(hub, Env::production(&paths), true).await
}

async fn serve(hub: Shared, env: Env, system_bus: bool) -> Result<()> {
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    hub.borrow_mut().service("osd", "running", 0)?;
    loop {
        while !*interest.borrow() {
            interest.changed().await?;
        }
        let system = if system_bus {
            zbus::Connection::system().await.ok()
        } else {
            None
        };
        tokio::select! {
            result = session(hub.clone(), env.clone(), system) => result?,
            result = idle(&mut interest) => result?,
        }
    }
}

#[cfg(test)]
mod tests;
