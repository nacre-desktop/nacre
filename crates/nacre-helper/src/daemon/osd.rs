//! N0043: the `osd` event topic. While at least one client subscribes, producers watch
//! caps lock, night light and power profile and report changes after a silent baseline;
//! brightness and keyboard backlight are reported by their methods (`bar_services`), the
//! way the QML OSD reacted to its own adjustments, and the audio kinds by the PipeWire
//! client (`audio`). Nothing runs, polls or is retained while nobody subscribes.
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
    collections::BTreeSet,
    ffi::OsString,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::time::Instant;

/// Bursts within this window become one event (QML `Qt.callLater`): the last wins,
/// but an output change is never replaced by the volume notification it causes.
const SETTLE: Duration = Duration::from_millis(40);
/// An identical event this soon after another (a method report and the bus signal for
/// the same key press) is dropped.
const DUPLICATE: Duration = Duration::from_millis(250);
const KEYBOARD_PATH: &str = "/org/freedesktop/UPower/KbdBacklight";
const KEYBOARD_IFACE: &str = "org.freedesktop.UPower.KbdBacklight";
const UPOWER: &str = "org.freedesktop.UPower";

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

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

thread_local! {
    static LAST: RefCell<Option<(OsdEvent, std::time::Instant)>> = const { RefCell::new(None) };
    /// When the last volume key step published its level, and its mute state.
    static KEY_STEP: std::cell::Cell<Option<(Instant, bool)>> = const { std::cell::Cell::new(None) };
}

/// The watcher reads the sink after our writes, so during a held key its level lags the
/// key steps; such a reading would pull the pill back a step. A mute change still shows.
const KEY_STEP_ECHO: Duration = Duration::from_millis(600);
pub(super) fn key_step_echo(event: &OsdEvent) -> bool {
    event.kind == OsdKind::Volume
        && KEY_STEP.with(|step| {
            step.get().is_some_and(|(at, muted)| {
                at.elapsed() < KEY_STEP_ECHO && event.muted == Some(muted)
            })
        })
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

/// A volume key step on the default sink: published even when the level is unchanged (at
/// 0 % or 100 %) so the pill shows at the limits. The fields match the PipeWire watcher's
/// event for the same state, which the duplicate filter then drops.
pub fn volume(hub: &Shared, level: f64, muted: bool, label: &str, device: &str) {
    if !level.is_finite() || !(0.0..=1.0).contains(&level) {
        return;
    }
    KEY_STEP.with(|step| step.set(Some((Instant::now(), muted))));
    let mut event = OsdEvent::new(OsdKind::Volume, now_ms());
    event.value = Some(level);
    event.muted = Some(muted);
    event.label = Some(label.chars().take(200).collect()).filter(|l: &String| !l.is_empty());
    event.device = Some(device.to_owned()).filter(|d| !d.is_empty() && d.len() <= 128);
    report(hub, event);
}

/// NN0036: Caps Lock as the compositor reports it (nacre-niri `KeyboardLockStateChanged`),
/// `None` once its event stream ends. While it is `Some` the session never reads the LED.
pub fn capslock(hub: &Shared, on: Option<bool>) {
    hub.borrow()
        .caps_lock
        .send_if_modified(|current| std::mem::replace(current, on) != on);
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
    /// `/sys/class/leds`
    pub leds: PathBuf,
    /// The night-light ownership records of the two compositor backends.
    pub night: Vec<PathBuf>,
    pub tick: Duration,
}
impl Env {
    pub fn production(paths: &Paths) -> Self {
        Self {
            leds: crate::light_devices::sys_root().join("class/leds"),
            night: ["control-tools.json", "control-tools-niri.json"]
                .map(|name| paths.state.join(name))
                .into(),
            tick: Duration::from_millis(500),
        }
    }
}

fn read_flag(path: &Path) -> Option<bool> {
    match std::fs::read_to_string(path).ok()?.trim() {
        "0" => Some(false),
        "1" => Some(true),
        _ => None,
    }
}

/// Every keyboard's LED shows the same compositor state; prefer the lowest `inputN` (the
/// keyboard registered first, normally the built-in one) so a hot-plugged keyboard is not
/// picked over it. A plain name sort put `input12` before `input3`.
fn find_capslock(leds: &Path) -> Option<PathBuf> {
    std::fs::read_dir(leds)
        .ok()?
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            let name = entry.file_name().into_string().ok()?;
            let index = name
                .strip_suffix("::capslock")?
                .strip_prefix("input")
                .and_then(|index| index.parse::<u64>().ok())
                .unwrap_or(u64::MAX);
            Some(((index, name), entry.path().join("brightness")))
        })
        .min()
        .map(|(_, path)| path)
}

/// N0067: one caps-lock LED read (discovery included), run off the async thread.
type CapsRead = (Option<PathBuf>, Option<bool>);
fn read_caps(leds: PathBuf, known: Option<PathBuf>) -> CapsRead {
    if let Some(value) = known.as_deref().and_then(read_flag) {
        return (known, Some(value));
    }
    // No LED yet, or the known one vanished (its keyboard was unplugged): discover again.
    let path = find_capslock(&leds);
    let value = path.as_deref().and_then(read_flag);
    (path.filter(|_| value.is_some()), value)
}
/// N0321: a known LED is polled every `tick` on the job's own offload thread, which returns
/// only when the reading differs from `last` (or the LED vanished) or the session dropped
/// `alive`. An unchanged LED then wakes that one thread, not the daemon's main thread too.
fn watch_caps(
    leds: PathBuf,
    known: Option<PathBuf>,
    last: Option<bool>,
    tick: Duration,
    alive: std::sync::Weak<()>,
) -> CapsRead {
    if known.is_none() {
        return read_caps(leds, known);
    }
    loop {
        std::thread::sleep(tick);
        if alive.strong_count() == 0 {
            return (known, None); // no reading: never a baseline or an event
        }
        let read = read_caps(leds.clone(), known.clone());
        if read != (known.clone(), last) {
            return read;
        }
    }
}
/// At most one read is in flight; a LED that hangs only stops its own updates.
fn start_caps(
    job: &mut Option<tokio::task::JoinHandle<Option<CapsRead>>>,
    env: &Env,
    known: &Option<PathBuf>,
    last: Option<bool>,
    alive: &std::sync::Arc<()>,
) {
    if job.is_none() {
        let (leds, known, tick) = (env.leds.clone(), known.clone(), env.tick);
        let alive = std::sync::Arc::downgrade(alive);
        *job = Some(tokio::task::spawn_local(async move {
            crate::process::offload(move || watch_caps(leds, known, last, tick, alive))
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

fn watch_inputs(night: &[PathBuf]) -> crate::Result<super::dashboard_extras::watch::Watch> {
    use rustix::fs::inotify::WatchFlags;
    let flags = WatchFlags::CLOSE_WRITE
        | WatchFlags::CREATE
        | WatchFlags::DELETE
        | WatchFlags::MOVED_FROM
        | WatchFlags::MOVED_TO
        | WatchFlags::DELETE_SELF
        | WatchFlags::MOVE_SELF;
    let dirs: BTreeSet<_> = night
        .iter()
        .filter_map(|path| path.parent().map(Path::to_owned))
        .collect();
    super::dashboard_extras::watch::Watch::new(dirs.into_iter().map(|dir| (dir, flags)).collect())
}

fn night_changed(names: &[OsString], records: &[PathBuf]) -> bool {
    names.is_empty()
        || names.iter().any(|name| {
            records.iter().any(|path| {
                path.file_name()
                    .is_some_and(|file| file == name.as_os_str())
            })
        })
}

/// Without a known LED the tick only re-discovers one (sysfs sends no inotify events for LED
/// devices): 2 s at the production 500 ms tick.
const REDISCOVER: u32 = 4;
/// The fallback tick: it looks for a LED unless the compositor reports Caps Lock (a known LED
/// is polled by its read job, `watch_caps`), and polls night light when its records cannot be
/// watched.
fn cap_ticker(
    compositor: bool,
    has_caps_led: bool,
    watches_inputs: bool,
    fallback: Duration,
) -> Option<tokio::time::Interval> {
    let period = if !watches_inputs {
        fallback
    } else if !compositor && !has_caps_led {
        fallback * REDISCOVER
    } else {
        return None;
    };
    let mut ticker = tokio::time::interval_at(Instant::now() + period, period);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    Some(ticker)
}

pub(super) async fn next_watch(
    watch: &mut Option<super::dashboard_extras::watch::Watch>,
) -> crate::Result<Vec<OsString>> {
    match watch {
        Some(watch) => watch.next().await,
        None => std::future::pending().await,
    }
}

async fn next_cap_tick(ticker: &mut Option<tokio::time::Interval>) {
    if let Some(ticker) = ticker {
        let _ = ticker.tick().await;
    } else {
        std::future::pending().await
    }
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

/// One subscribed period. Runs until dropped.
pub async fn session(hub: Shared, env: Env, system: Option<zbus::Connection>) -> Result<()> {
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
    // NN0036: while the compositor reports Caps Lock the LED is never read.
    let mut lock_keys = hub.borrow().caps_lock.subscribe();
    let reported = *lock_keys.borrow_and_update();
    let mut compositor = reported.is_some();
    let (mut capslock_path, mut caps) = match reported {
        Some(on) => (None, Some(on)),
        None => {
            let (leds, known) = (env.leds.clone(), None);
            crate::process::offload(move || read_caps(leds, known))
                .await
                .unwrap_or((None, None))
        }
    };
    let mut caps_job = None;
    // Dropped (and replaced) to end a known LED's poll job.
    let mut alive = std::sync::Arc::new(());
    if !compositor && capslock_path.is_some() {
        start_caps(&mut caps_job, &env, &capslock_path, caps, &alive);
    }
    let mut input_watch = watch_inputs(&env.night).ok();
    let mut night = read_night(&env.night);
    let mut profile = match &system {
        Some(system) => active_profile(system).await,
        None => None,
    };

    let mut pending = Pending::default();
    let mut ticker = cap_ticker(
        compositor,
        capslock_path.is_some(),
        input_watch.is_some(),
        env.tick,
    );
    loop {
        tokio::select! {
            _ = next_cap_tick(&mut ticker) => {
                if !compositor {
                    start_caps(&mut caps_job, &env, &capslock_path, caps, &alive);
                }
                if input_watch.is_none() {
                    let next = read_night(&env.night);
                    if next != night {
                        let mut event = OsdEvent::new(OsdKind::NightLight, now_ms());
                        event.value = Some(f64::from(u8::from(next)));
                        pending.add(event);
                        night = next;
                    }
                }
            }
            changed = next_watch(&mut input_watch) => {
                match changed {
                    Ok(names) => {
                        if night_changed(&names, &env.night) {
                            let next = read_night(&env.night);
                            if next != night {
                                let mut event = OsdEvent::new(OsdKind::NightLight, now_ms());
                                event.value = Some(f64::from(u8::from(next)));
                                pending.add(event);
                                night = next;
                            }
                        }
                    }
                    Err(_) => {
                        input_watch = None;
                        ticker = cap_ticker(compositor, capslock_path.is_some(), false, env.tick);
                    }
                }
            }
            read = caps_done(&mut caps_job) => {
                if compositor {
                    continue; // a read started before the compositor reported
                }
                // A failed job counts as a lost LED: the rediscovery tick reads again.
                let (path, value) = read.unwrap_or((None, None));
                let had_path = capslock_path.is_some();
                capslock_path = path;
                if let Some(next) = value {
                    if caps.is_some_and(|before| before != next) {
                        let mut event = OsdEvent::new(OsdKind::Capslock, now_ms());
                        event.value = Some(f64::from(u8::from(next)));
                        pending.add(event);
                    }
                    caps = Some(next);
                }
                if had_path != capslock_path.is_some() {
                    ticker = cap_ticker(
                        false,
                        capslock_path.is_some(),
                        input_watch.is_some(),
                        env.tick,
                    );
                }
                if capslock_path.is_some() {
                    start_caps(&mut caps_job, &env, &capslock_path, caps, &alive);
                }
            }
            Ok(()) = lock_keys.changed() => {
                let reported = *lock_keys.borrow_and_update();
                match reported {
                    // The first compositor report is a silent baseline, like the first LED read.
                    Some(next) => {
                        if compositor && caps.is_some_and(|before| before != next) {
                            let mut event = OsdEvent::new(OsdKind::Capslock, now_ms());
                            event.value = Some(f64::from(u8::from(next)));
                            pending.add(event);
                        }
                        caps = Some(next);
                        alive = std::sync::Arc::new(());
                    }
                    // The stream ended: back to the LED, from a silent baseline.
                    None => {
                        caps = None;
                        start_caps(&mut caps_job, &env, &capslock_path, caps, &alive);
                    }
                }
                compositor = reported.is_some();
                ticker = cap_ticker(
                    compositor,
                    capslock_path.is_some(),
                    input_watch.is_some(),
                    env.tick,
                );
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
    hub.borrow_mut().service("osd", "running", 0)?;
    loop {
        while !*interest.borrow() {
            interest.changed().await?;
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
                while *interest.borrow() {
                    interest.changed().await?;
                }
                Ok::<(), crate::Error>(())
            } => result?,
        }
    }
}

#[cfg(test)]
mod tests;
