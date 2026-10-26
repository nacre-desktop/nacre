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
    core_services::{
        AudioKind, AudioNode, AudioReadback, AudioState, CoreSnapshot, Entities, EntityId, Health,
        OsdState,
    },
    osd::{OsdEvent, OsdKind, EVENT, TOPIC},
};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::BTreeMap,
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
    publish_audio(hub, unavailable_audio())?;
    Ok(())
}

fn unavailable_audio() -> AudioState {
    AudioState {
        health: Health {
            available: false,
            stale: false,
            error: None,
        },
        nodes: Entities {
            items: Default::default(),
            order: Vec::new(),
        },
        default_output: None,
        default_input: None,
    }
}

fn publish_audio(hub: &Shared, state: AudioState) -> Result<()> {
    debug_assert!(CoreSnapshot::Audio(state.clone()).valid());
    hub.borrow_mut()
        .publish("audio", serde_json::to_value(state)?)?;
    Ok(())
}

/// `(perceptual, linear, muted)` of a pactl sink, source or stream entry.
fn levels(entry: &Value) -> Option<(f64, f64, bool)> {
    let perceptual = entry["volume"]
        .as_object()?
        .values()
        .filter_map(|channel| channel["value"].as_f64())
        .reduce(f64::max)?
        / 65536.0;
    let linear = perceptual.powi(3);
    (perceptual.is_finite() && perceptual >= 0.0 && linear.is_finite()).then_some((
        perceptual,
        linear,
        entry["mute"].as_bool()?,
    ))
}

/// The most application streams one kind publishes.
const MAX_STREAMS: usize = 64;
/// The peak monitor's own capture stream (see `peak`); it is not an application.
pub(super) const PEAK_CLIENT: &str = "nacre-peak";

/// N0061: application playback (`sink-input`) or capture (`source-output`) streams. The id
/// `<prefix>:<pactl index>` is what `audio.set_volume`/`audio.set_mute` take as a `stream`
/// target; `name` is the application's name. A stream that cannot be read is skipped.
fn audio_streams(data: &str, kind: AudioKind, prefix: &str, state: &mut AudioState) {
    let Ok(entries) = serde_json::from_str::<Vec<Value>>(data) else {
        return;
    };
    for entry in entries.into_iter().take(MAX_STREAMS) {
        let properties = &entry["properties"];
        if properties["application.name"] == PEAK_CLIENT
            || properties["application.name"] == super::audio_spectrum::CLIENT
            || state.nodes.items.len() >= 2048
        {
            continue;
        }
        let (Some(index), Some((perceptual, linear, muted))) =
            (entry["index"].as_u64(), levels(&entry))
        else {
            continue;
        };
        let id = EntityId(format!("{prefix}:{index}"));
        let text = |key: &str| properties[key].as_str().filter(|text| !text.is_empty());
        let name = text("application.name")
            .or_else(|| text("media.name"))
            .unwrap_or_default();
        state.nodes.items.insert(
            id.clone(),
            AudioNode {
                name: name.chars().take(200).collect(),
                kind: kind.clone(),
                volume: linear,
                muted,
                ready: true,
                readback: Some(AudioReadback {
                    linear_volume: Some(linear),
                    perceptual_volume: Some(perceptual),
                    muted: Some(muted),
                }),
                icon: text("application.icon_name").map(|icon| icon.chars().take(128).collect()),
                target: None,
            },
        );
        state.nodes.order.push(id);
    }
}

// Pulse volumes are the cube root of PipeWire's linear volume. Preserve both
// readings so clients use the same perceptual slider as the existing shell.
fn audio_nodes(
    data: &str,
    kind: AudioKind,
    state: &mut AudioState,
) -> Option<BTreeMap<String, EntityId>> {
    let mut aliases = BTreeMap::new();
    let entries: Vec<Value> = serde_json::from_str(data).ok()?;
    for entry in entries.into_iter().take(2048 - state.nodes.items.len()) {
        // Pulse monitor pseudo-sources are not selectable PipeWire input nodes.
        let monitor = entry
            .get("monitor_of_sink")
            .is_some_and(|value| !value.is_null() && value.as_u64() != Some(u64::from(u32::MAX)));
        if kind == AudioKind::Input && (monitor || entry["properties"]["device.class"] == "monitor")
        {
            continue;
        }
        let Some(name) = entry["properties"]["node.name"].as_str() else {
            continue;
        };
        let pulse_name = entry["name"].as_str()?;
        let id = EntityId(name.to_owned());
        if !id.valid() || state.nodes.items.contains_key(&id) {
            continue;
        }
        let (perceptual, linear, muted) = levels(&entry)?;
        let label = entry["description"]
            .as_str()
            .unwrap_or(name)
            .chars()
            .take(200)
            .collect();
        state.nodes.items.insert(
            id.clone(),
            AudioNode {
                name: label,
                kind: kind.clone(),
                volume: linear,
                muted,
                ready: true,
                readback: Some(AudioReadback {
                    linear_volume: Some(linear),
                    perceptual_volume: Some(perceptual),
                    muted: Some(muted),
                }),
                icon: None,
                target: None,
            },
        );
        aliases.insert(pulse_name.to_owned(), id.clone());
        state.nodes.order.push(id);
    }
    Some(aliases)
}

async fn read_audio(env: &Env) -> Option<AudioState> {
    let mut state = unavailable_audio();
    let sinks = tool(&env.pactl, &["--format=json", "list", "sinks"]).await?;
    let sources = tool(&env.pactl, &["--format=json", "list", "sources"]).await?;
    let sink_names = audio_nodes(&sinks, AudioKind::Output, &mut state)?;
    let source_names = audio_nodes(&sources, AudioKind::Input, &mut state)?;
    for (command, names, default) in [
        ("get-default-sink", &sink_names, &mut state.default_output),
        (
            "get-default-source",
            &source_names,
            &mut state.default_input,
        ),
    ] {
        if let Some(name) = tool(&env.pactl, &[command]).await {
            *default = names.get(name.trim()).cloned();
        }
    }
    // Streams are optional: an old server without them still has its devices.
    for (list, kind, prefix) in [
        ("sink-inputs", AudioKind::Playback, "sink-input"),
        ("source-outputs", AudioKind::Capture, "source-output"),
    ] {
        if let Some(data) = tool(&env.pactl, &["--format=json", "list", list]).await {
            audio_streams(&data, kind, prefix, &mut state);
        }
    }
    state.health.available = !state.nodes.items.is_empty();
    Some(state)
}

async fn refresh_audio(hub: &Shared, env: &Env) -> Result<()> {
    publish_audio(hub, read_audio(env).await.unwrap_or_else(unavailable_audio))
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

/// Optimistic value for a held volume key; the PipeWire signal is allowed to follow.
pub fn volume(hub: &Shared, level: f64) {
    if !level.is_finite() || !(0.0..=1.0).contains(&level) {
        return;
    }
    let mut event = OsdEvent::new(OsdKind::Volume, now_ms());
    event.value = Some(level);
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
    /// First wait before restarting a failed `pactl subscribe`; doubles up to 32 times this.
    pub retry: Duration,
}
impl Env {
    pub fn production(paths: &Paths) -> Self {
        Self {
            wpctl: "wpctl".into(),
            pactl: "pactl".into(),
            leds: crate::light_devices::sys_root().join("class/leds"),
            night: ["control-tools.json", "control-tools-niri.json"]
                .map(|name| paths.state.join(name))
                .into(),
            tick: Duration::from_millis(250),
            retry: Duration::from_secs(2),
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
    let mut stdout = child.stdout.take()?.take(1024 * 1024);
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

/// N0067: one caps-lock LED read (discovery included), run off the async thread.
type CapsRead = (Option<PathBuf>, Option<bool>);
fn read_caps(leds: PathBuf, known: Option<PathBuf>) -> CapsRead {
    let path = known.or_else(|| find_capslock(&leds));
    let value = path.as_deref().and_then(read_flag);
    (path.filter(|_| value.is_some()), value)
}
/// At most one read is in flight; a LED that hangs only stops its own updates.
fn start_caps(
    job: &mut Option<tokio::task::JoinHandle<Option<CapsRead>>>,
    leds: &Path,
    known: &Option<PathBuf>,
) {
    if job.is_none() {
        let (leds, known) = (leds.to_owned(), known.clone());
        *job = Some(tokio::task::spawn_local(async move {
            crate::process::offload(move || read_caps(leds, known))
                .await
                .ok()
        }));
    }
}
async fn caps_done(
    job: &mut Option<tokio::task::JoinHandle<Option<CapsRead>>>,
) -> Option<CapsRead> {
    match job {
        Some(handle) => {
            let result = handle.await.ok().flatten();
            *job = None;
            result
        }
        None => std::future::pending().await,
    }
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
    tokio::time::timeout(Duration::from_secs(2), async {
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
    })
    .await
    .ok()
    .flatten()
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

fn spawn_audio(
    pactl: &OsString,
) -> (
    Option<tokio::process::Child>,
    LocalBoxStream<'static, String>,
) {
    let mut child = Command::new(pactl)
        .arg("subscribe")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .ok();
    let lines = match child.as_mut().and_then(|child| child.stdout.take()) {
        Some(stdout) => {
            futures_util::stream::unfold(BufReader::new(stdout).lines(), |mut l| async move {
                let line = l.next_line().await.ok().flatten()?;
                Some((line, l))
            })
            .boxed_local()
        }
        None => futures_util::stream::pending().boxed_local(),
    };
    (child, lines)
}

/// One subscribed period. Runs until dropped; a lost audio subscription is retried in place.
pub async fn session(hub: Shared, env: Env, system: Option<zbus::Connection>) -> Result<()> {
    let (mut _audio_child, mut lines) = spawn_audio(&env.pactl);
    // An audio failure only degrades the audio kinds: retry with backoff, never end the session.
    let mut backoff = env.retry;
    let mut retry_at = _audio_child.is_none().then(|| Instant::now() + backoff);

    let mut power: Streams = Streams::new();
    let mut backlight: LocalBoxStream<'static, i32> = futures_util::stream::pending().boxed_local();
    let mut keyboard_max = 0i32;
    if let Some(system) = &system {
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
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
                            .filter_map(|message| async move {
                                message.body().deserialize::<i32>().ok()
                            })
                            .boxed_local();
                    }
                }
            }
        })
        .await;
    }

    // Baseline: nothing below is reported until it differs from what is read here.
    refresh_audio(&hub, &env).await?;
    let mut sink = read_node(&env.wpctl, SINK).await;
    let mut source = read_node(&env.wpctl, SOURCE).await;
    let mut capslock_path: Option<PathBuf> = None;
    let mut caps: Option<bool> = None;
    let mut caps_job = None;
    start_caps(&mut caps_job, &env.leds, &capslock_path);
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
                    _audio_child = None;
                    publish_audio(&hub, unavailable_audio())?;
                    lines = futures_util::stream::pending().boxed_local();
                    retry_at = Some(Instant::now() + backoff);
                    backoff = (backoff * 2).min(env.retry * 32);
                    continue;
                };
                backoff = env.retry;
                let server = line.ends_with(" on server");
                let relevant = |what: &str| line.contains(&format!(" on {what} #"));
                let (s, m) = (server || relevant("sink"), server || relevant("source"));
                // Stream events only refresh the audio topic (N0061).
                let streams = relevant("sink-input") || relevant("source-output");
                if s || m || streams {
                    dirty_sink |= s;
                    dirty_source |= m;
                    audio_at.get_or_insert(Instant::now() + AUDIO_DEBOUNCE);
                }
            }
            () = due(retry_at) => {
                retry_at = None;
                (_audio_child, lines) = spawn_audio(&env.pactl);
                if _audio_child.is_none() {
                    retry_at = Some(Instant::now() + backoff);
                    backoff = (backoff * 2).min(env.retry * 32);
                }
                refresh_audio(&hub, &env).await?;
                // A silent fresh baseline: changes while audio was down are not replayed.
                sink = read_node(&env.wpctl, SINK).await;
                source = read_node(&env.wpctl, SOURCE).await;
            }
            () = due(audio_at) => {
                audio_at = None;
                refresh_audio(&hub, &env).await?;
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
                start_caps(&mut caps_job, &env.leds, &capslock_path);
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
            Some((path, value)) = caps_done(&mut caps_job) => {
                capslock_path = path;
                if let Some(next) = value {
                    if caps.is_some_and(|before| before != next) {
                        let mut event = OsdEvent::new(OsdKind::Capslock, now_ms());
                        event.value = Some(f64::from(u8::from(next)));
                        pending.add(event);
                    }
                    caps = Some(next);
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

pub async fn run(hub: Shared, paths: Paths) -> Result<()> {
    serve(hub, Env::production(&paths), true).await
}

async fn serve(hub: Shared, env: Env, system_bus: bool) -> Result<()> {
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    let mut audio_interest = hub.borrow_mut().watch_interest("audio");
    let mut spectrum_interest = hub
        .borrow_mut()
        .watch_interest(super::audio_spectrum::TOPIC);
    hub.borrow_mut().service("osd", "running", 0)?;
    loop {
        while !*interest.borrow() && !*audio_interest.borrow() && !*spectrum_interest.borrow() {
            tokio::select! { result = interest.changed() => result?, result = audio_interest.changed() => result?, result = spectrum_interest.changed() => result? }
        }
        let system = if system_bus {
            tokio::time::timeout(Duration::from_secs(2), zbus::Connection::system())
                .await
                .ok()
                .and_then(std::result::Result::ok)
        } else {
            None
        };
        tokio::select! {
            result = session(hub.clone(), env.clone(), system) => result?,
            result = async {
                loop {
                    if !*interest.borrow() && !*audio_interest.borrow() && !*spectrum_interest.borrow() { return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(()); }
                    tokio::select! { result = interest.changed() => result?, result = audio_interest.changed() => result?, result = spectrum_interest.changed() => result? }
                }
            } => result?,
        }
        publish_audio(&hub, unavailable_audio())?;
    }
}

#[cfg(test)]
mod tests;
