//! Status-pill actions restricted to the current user's default devices.
use super::hub::{failure, Shared};
use nacre_protocol::{
    pill_controls::{BluetoothEnabled, PillEmpty, VolumeAdjust, VolumeDirection, VolumeStep},
    ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc::{self, SyncSender},
        Arc, OnceLock,
    },
    thread::JoinHandle,
    time::Duration,
};

static AUDIO_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();
static LAST_STEP_MARKER_MS: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, Debug)]
pub(crate) struct FastAudioStep {
    pub(crate) delta: f64,
    pub(crate) key_ms: Option<u64>,
}

pub(crate) enum AudioStepCommand {
    Apply(FastAudioStep),
    /// The default output as the `audio` topic last published it.
    Observe(AudioObservation),
    Stop,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AudioObservation {
    pub(crate) default: Option<String>,
    /// Linear level of the default output's loudest channel.
    pub(crate) linear: Option<f64>,
    pub(crate) muted: Option<bool>,
}

/// The default output in the published `audio` snapshot.
pub(crate) fn audio_observation(hub: &Shared) -> Option<AudioObservation> {
    let hub = hub.borrow();
    let state = hub.snapshot_ref("audio")?;
    let default = state["default_output"].as_str().map(str::to_owned);
    let node = default
        .as_deref()
        .and_then(|name| state["nodes"]["items"].get(name));
    Some(AudioObservation {
        default,
        linear: node.and_then(|node| node["volume"].as_f64()),
        muted: node.and_then(|node| node["muted"].as_bool()),
    })
}

pub(crate) struct AudioStepCompletion {
    pub(crate) result: Result<AudioStepOutcome, Failure>,
    pub(crate) key_ms: Option<u64>,
}

pub(crate) struct AudioStepWorker {
    stop: Arc<std::sync::atomic::AtomicBool>,
    sender: Option<SyncSender<AudioStepCommand>>,
    thread: Option<JoinHandle<()>>,
}

/// One worker iteration: every queued command is taken at once, so key presses that
/// arrived while a write was running become one net step (N0102: a held key never
/// builds a backlog that keeps moving the volume after release).
fn drain_commands(
    first: AudioStepCommand,
    receiver: &mpsc::Receiver<AudioStepCommand>,
    steps: &mut AudioSteps,
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
) -> Option<Option<(f64, Option<u64>)>> {
    let mut batch = None::<(f64, Option<u64>)>;
    for command in std::iter::once(first).chain(receiver.try_iter()) {
        match command {
            AudioStepCommand::Stop => return None,
            AudioStepCommand::Observe(observation) => steps.observe(observation, run),
            AudioStepCommand::Apply(step) => {
                let (delta, key_ms) = batch.get_or_insert((0.0, None));
                *delta += step.delta;
                // The oldest key of the batch, so OSD_STEP reports the full wait.
                *key_ms = key_ms.or(step.key_ms);
            }
        }
    }
    Some(batch)
}

/// Runs one coalesced step: the OSD level is sent before the write (from the cached
/// level), and again only when the write's result differs.
fn run_batch(
    steps: &mut AudioSteps,
    delta: f64,
    key_ms: Option<u64>,
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
    send: &mut impl FnMut(AudioStepCompletion),
) {
    let mut previewed = None;
    let result = steps.step(delta, run, &mut |outcome: &AudioStepOutcome| {
        previewed = Some(outcome.clone());
        send(AudioStepCompletion {
            result: Ok(outcome.clone()),
            key_ms,
        });
    });
    if !matches!((&result, &previewed), (Ok(outcome), Some(shown)) if outcome == shown) {
        send(AudioStepCompletion { result, key_ms });
    }
}

/// The worker loop: blocks for a command, then runs everything queued as one batch.
fn serve_steps(
    receiver: &mpsc::Receiver<AudioStepCommand>,
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
    stop: &std::sync::atomic::AtomicBool,
    send: &mut impl FnMut(AudioStepCompletion),
) {
    let mut steps = AudioSteps::default();
    while !stop.load(Ordering::Acquire) {
        let Ok(first) = receiver.recv() else { break };
        let Some(batch) = drain_commands(first, receiver, &mut steps, run) else {
            break;
        };
        if stop.load(Ordering::Acquire) {
            break;
        }
        if let Some((delta, key_ms)) = batch {
            let _guard = audio_lock().blocking_lock_owned();
            run_batch(&mut steps, delta, key_ms, run, send);
        }
    }
}

impl AudioStepWorker {
    pub(crate) fn start(
        completed: tokio::sync::mpsc::UnboundedSender<AudioStepCompletion>,
    ) -> crate::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<AudioStepCommand>(256);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name("nacre-audio-step".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                serve_steps(&receiver, &mut production_run, &thread_stop, &mut |completion| {
                    let _ = completed.send(completion);
                });
            })?;
        Ok(Self {
            stop,
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    pub(crate) fn sender(&self) -> SyncSender<AudioStepCommand> {
        self.sender
            .as_ref()
            .expect("audio worker is active")
            .clone()
    }
}

impl Drop for AudioStepWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(sender) = self.sender.take() {
            let _ = sender.try_send(AudioStepCommand::Stop);
        }
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::warn!("Audio step worker panicked");
            }
        }
    }
}

/// Own writes are echoed by the PipeWire watcher this long after them; a different level
/// seen inside this window is our own earlier step, not an external change.
const OWN_WRITE_ECHO: Duration = Duration::from_millis(500);

/// The default sink's volume object, read once (`wpctl inspect` + `pw-dump`, ~0.1 s) and
/// then kept current from our own writes and the `audio` topic, so a key step costs one
/// `pw-cli set-param` and its OSD needs no read at all.
#[derive(Default)]
struct AudioSteps {
    cache: Option<SinkVolume>,
    /// The default output name of the last observation (a change re-reads the sink).
    seen_default: Option<Option<String>>,
    last_write: Option<std::time::Instant>,
}

enum SinkRead {
    PipeWire(SinkVolume),
    /// The volume object could not be read; `wpctl inspect` output for the wpctl path.
    Wpctl(String),
}

fn read_sink(
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
) -> Result<SinkRead, Failure> {
    let inspect = run("wpctl", &["inspect", "@DEFAULT_AUDIO_SINK@"], false)?;
    let node_id = parse_object_id(&inspect).ok_or_else(unavailable)?;
    let sink = run("pw-dump", &[], false)
        .ok()
        .and_then(|dump| serde_json::from_str::<Value>(&dump).ok())
        .and_then(|dump| sink_volume_from_dump(node_id, &dump));
    Ok(match sink {
        Some(sink) => SinkRead::PipeWire(sink),
        None => SinkRead::Wpctl(inspect),
    })
}

impl AudioSteps {
    fn observe(
        &mut self,
        observation: AudioObservation,
        run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
    ) {
        let changed_default = self.seen_default.as_ref() != Some(&observation.default);
        self.seen_default = Some(observation.default.clone());
        let reread = if changed_default {
            observation.default.is_some()
        } else if let Some(sink) = &mut self.cache {
            if let Some(muted) = observation.muted {
                sink.muted = muted;
            }
            let echo = self
                .last_write
                .is_some_and(|at| at.elapsed() < OWN_WRITE_ECHO);
            match observation.linear {
                Some(linear) if !echo && (linear - sink.level()).abs() > 0.002 => {
                    // Another client changed it: follow a balanced sink, re-read otherwise.
                    let balanced = sink
                        .current
                        .iter()
                        .all(|volume| (volume - sink.current[0]).abs() < 1e-3);
                    if balanced {
                        sink.current.iter_mut().for_each(|volume| *volume = linear);
                    }
                    !balanced
                }
                _ => false,
            }
        } else {
            false
        };
        if reread {
            self.cache = match read_sink(run) {
                Ok(SinkRead::PipeWire(sink)) => Some(sink),
                _ => None,
            };
        }
    }

    fn step(
        &mut self,
        delta: f64,
        run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
        preview: &mut impl FnMut(&AudioStepOutcome),
    ) -> Result<AudioStepOutcome, Failure> {
        let cached = self.cache.is_some();
        if !cached {
            match read_sink(run)? {
                SinkRead::PipeWire(sink) => self.cache = Some(sink),
                SinkRead::Wpctl(inspect) => return step_wpctl_sink(&inspect, delta, run),
            }
        }
        let sink = self.cache.as_mut().ok_or_else(unavailable)?;
        let channels = stepped_channel_volumes(&sink.current, sink.channel_count, delta)
            .ok_or_else(unavailable)?;
        let outcome = sink.outcome(&channels);
        // At 0 % or 100 % there is nothing to write, but the OSD is still reported.
        if sink.current.len() == channels.len()
            && sink
                .current
                .iter()
                .zip(&channels)
                .all(|(now, next)| (now - next).abs() < 1e-6)
        {
            return Ok(outcome);
        }
        preview(&outcome);
        match write_sink(sink, &channels, run) {
            Ok(()) => {
                sink.current = channels;
                self.last_write = Some(std::time::Instant::now());
                Ok(outcome)
            }
            // The cached object may be gone (sink re-created): re-read once.
            Err(_) if cached => {
                self.cache = None;
                self.step(delta, run, preview)
            }
            Err(error) => {
                self.cache = None;
                Err(error)
            }
        }
    }
}

fn production_run(program: &str, args: &[&str], mutation: bool) -> Result<String, Failure> {
    if program == "wpctl" {
        run_action(program, args, mutation)
    } else {
        run_pipewire_action(program, args, mutation)
    }
}

pub fn methods() -> BTreeMap<String, Method> {
    nacre_protocol::pill_controls::methods()
        .into_iter()
        .filter(|(name, _)| backend_present(name))
        .collect()
}

fn backend_present(method: &str) -> bool {
    let (program, service) = match method {
        "audio.adjust_volume" | "audio.toggle_mute" | "audio.step" => ("wpctl", "pipewire-0"),
        "bluetooth.set_enabled" => ("bluetoothctl", "system_bus_socket"),
        _ => return false,
    };
    executable(program)
        && (method != "audio.step" || (executable("pw-dump") && executable("pw-cli")))
        && match method {
            "audio.adjust_volume" | "audio.step" | "audio.toggle_mute" => {
                std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|runtime| {
                    socket_present(&std::path::Path::new(&runtime).join(service))
                })
            }
            "bluetooth.set_enabled" => {
                socket_present(&std::path::Path::new("/run/dbus").join(service))
            }
            _ => false,
        }
}

pub(crate) fn fast_backend_present(method: &str) -> bool {
    backend_present(method)
}

fn socket_present(path: &std::path::Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata(path).is_ok_and(|metadata| metadata.file_type().is_socket())
}

fn executable(program: &str) -> bool {
    super::control_center::executable(program)
}

pub async fn dispatch(hub: Shared, method: &str, params: Value) -> Result<Value, Failure> {
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::PermissionDenied,
            "Status pill actions are disabled in safe mode",
            false,
        )
        .dispatch_state(false));
    }
    match method {
        "audio.adjust_volume" => {
            let request: VolumeAdjust = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            if request.delta == 0.0 {
                return Ok(json!({"changed":false}));
            }
            ensure_backend(method)?;
            let guard = audio_lock().try_lock_owned().map_err(|_| {
                failure(ErrorCode::Busy, "Audio control is busy", true).dispatch_state(false)
            })?;
            crate::process::offload(move || {
                let _guard = guard;
                adjust_volume(request.delta)
            })
            .await
            .map_err(|_| action_error())??;
        }
        "audio.step" => {
            let request: VolumeStep = serde_json::from_value(params).map_err(|_| invalid())?;
            let delta = match request.direction {
                VolumeDirection::Up => 0.05,
                VolumeDirection::Down => -0.05,
            };
            ensure_backend(method)?;
            let guard = audio_lock().lock_owned().await;
            let outcome = crate::process::offload(move || {
                let _guard = guard;
                step_audio_sink(delta)
            })
            .await
            .map_err(|_| action_error())??;
            publish_audio_step(&hub, &outcome, request.key_ms)?;
        }
        "audio.toggle_mute" => {
            let _: PillEmpty = serde_json::from_value(params).map_err(|_| invalid())?;
            ensure_backend(method)?;
            let guard = audio_lock().try_lock_owned().map_err(|_| {
                failure(ErrorCode::Busy, "Audio control is busy", true).dispatch_state(false)
            })?;
            crate::process::offload(move || {
                let _guard = guard;
                toggle_mute()
            })
            .await
            .map_err(|_| action_error())??;
        }
        "bluetooth.set_enabled" => {
            let request: BluetoothEnabled =
                serde_json::from_value(params).map_err(|_| invalid())?;
            ensure_backend(method)?;
            crate::process::offload(move || set_bluetooth(request.enabled))
                .await
                .map_err(|_| action_error())??;
        }
        _ => {
            return Err(failure(
                ErrorCode::UnknownMethod,
                "Unknown status pill method",
                false,
            ))
        }
    }
    Ok(json!({"changed":true}))
}

/// The step always ran on `@DEFAULT_AUDIO_SINK@`, so the OSD is published unconditionally,
/// also when the level did not change at 0 % or 100 %.
pub(crate) fn publish_audio_step(
    hub: &Shared,
    outcome: &AudioStepOutcome,
    key_ms: Option<u64>,
) -> Result<(), Failure> {
    let value = outcome.volume;
    super::osd::volume(
        hub,
        value,
        outcome.muted,
        &outcome.label,
        &outcome.node_name,
    );
    let snapshot = hub.borrow().snapshot("audio");
    if let Some(mut state) = snapshot {
        if let Some(node) = state["nodes"]["items"].get_mut(&outcome.node_name) {
            node["volume"] = json!(outcome.linear);
            if let Some(readback) = node.get_mut("readback") {
                readback["linear_volume"] = json!(outcome.linear);
                readback["perceptual_volume"] = json!(value);
            }
            hub.borrow_mut()
                .publish("audio", state)
                .map_err(|_| action_error())?;
        }
    }
    if let Some(key_ms) = key_ms {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis() as u64);
        if step_marker_allowed(now_ms) {
            tracing::info!("OSD_STEP kind=volume value={value:.3} ms_since_key={} key_ms={key_ms} timebase=unix_ms", now_ms.saturating_sub(key_ms));
        }
    }
    Ok(())
}

fn step_marker_allowed(now_ms: u64) -> bool {
    let mut previous = LAST_STEP_MARKER_MS.load(Ordering::Relaxed);
    loop {
        if now_ms.saturating_sub(previous) < 20 {
            return false;
        }
        match LAST_STEP_MARKER_MS.compare_exchange_weak(
            previous,
            now_ms,
            Ordering::Relaxed,
            Ordering::Relaxed,
        ) {
            Ok(_) => return true,
            Err(current) => previous = current,
        }
    }
}

pub(super) fn audio_lock() -> Arc<tokio::sync::Mutex<()>> {
    AUDIO_LOCK
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
}

fn adjust_volume(delta: f64) -> Result<(), Failure> {
    adjust_volume_with(delta, &mut run_action)
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AudioStepOutcome {
    pub(crate) node_name: String,
    /// `node.description` (else `node.nick`), the label the PipeWire watcher reports.
    pub(crate) label: String,
    /// Linear level of the loudest channel, what `AudioNode.volume` reports.
    pub(crate) linear: f64,
    /// Perceptual (cubic, `wpctl get-volume`) level the OSD and the sliders show.
    pub(crate) volume: f64,
    pub(crate) muted: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum VolumeParam {
    NodeProps {
        node_id: u64,
    },
    DeviceRoute {
        device_id: u64,
        route_index: u64,
        route_device: u64,
    },
}

#[derive(Clone, Debug, PartialEq)]
struct SinkVolume {
    node_name: String,
    label: String,
    param: VolumeParam,
    current: Vec<f64>,
    channel_count: usize,
    muted: bool,
}

fn parse_object_id(output: &str) -> Option<u64> {
    output.lines().find_map(|line| {
        line.trim()
            .strip_prefix("id ")?
            .split(',')
            .next()?
            .trim()
            .parse()
            .ok()
    })
}

fn object_id(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str()?.parse::<u64>().ok())
}

/// The newest entry of `name` that carries volumes. An ALSA sink lists a second `Props`
/// object (device/latency settings, no `channelVolumes`) after the volume one.
fn current_param<'a>(object: &'a Value, name: &str) -> Option<&'a Value> {
    let params = object.get("info")?.get("params")?.get(name)?;
    params
        .as_array()?
        .iter()
        .rev()
        .find(|entry| entry.get("channelVolumes").is_some())
}

fn volume_data(props: &Value) -> Option<(Vec<f64>, usize)> {
    let current = props
        .get("channelVolumes")?
        .as_array()?
        .iter()
        .map(Value::as_f64)
        .collect::<Option<Vec<_>>>()?;
    let channel_count = props
        .get("channelMap")
        .and_then(Value::as_array)
        .map_or(current.len(), Vec::len);
    if current.is_empty()
        || current.len() > 64
        || channel_count == 0
        || channel_count > 64
        || current.len() > channel_count
        || current
            .iter()
            .any(|volume| !volume.is_finite() || !(0.0..=4.0).contains(volume))
    {
        return None;
    }
    Some((current, channel_count))
}

fn sink_volume_from_dump(node_id: u64, dump: &Value) -> Option<SinkVolume> {
    let objects = dump.as_array()?;
    let node = objects.iter().find(|object| {
        object["type"] == "PipeWire:Interface:Node" && object["id"].as_u64() == Some(node_id)
    })?;
    let node_props = node.get("info")?.get("props")?;
    let node_name = node_props.get("node.name")?.as_str()?.to_owned();
    let label = ["node.description", "node.nick"]
        .iter()
        .find_map(|key| {
            node_props
                .get(*key)?
                .as_str()
                .filter(|text| !text.is_empty())
        })
        .unwrap_or_default()
        .to_owned();
    let device_id = node_props.get("device.id").and_then(object_id);
    let route_device = node_props.get("card.profile.device").and_then(object_id);

    // A device route owns the volume when the active profile has one for this node; a
    // pro-audio profile (`device.routes = 0`) has none and the node's Props hold it.
    let route = (|| {
        let (device_id, route_device) = (device_id?, route_device?);
        let device = objects.iter().find(|object| {
            object["type"] == "PipeWire:Interface:Device"
                && object["id"].as_u64() == Some(device_id)
        })?;
        let route = device
            .get("info")?
            .get("params")?
            .get("Route")?
            .as_array()?
            .iter()
            .find(|route| object_id(&route["device"]) == Some(route_device))?;
        let param = VolumeParam::DeviceRoute {
            device_id,
            route_index: object_id(&route["index"])?,
            route_device: object_id(&route["device"])?,
        };
        Some((param, route.get("props")?))
    })();
    let (param, props) = match route {
        Some(route) => route,
        None => (
            VolumeParam::NodeProps { node_id },
            current_param(node, "Props")?,
        ),
    };
    let (current, channel_count) = volume_data(props)?;
    Some(SinkVolume {
        node_name,
        label,
        param,
        current,
        channel_count,
        muted: props.get("mute").and_then(Value::as_bool).unwrap_or(false),
    })
}

/// Steps every channel by `delta` on the perceptual (cubic) scale `wpctl set-volume 5%+`
/// uses, snapped to whole percent and capped at 100 %; returns linear channel volumes.
fn stepped_channel_volumes(current: &[f64], count: usize, delta: f64) -> Option<Vec<f64>> {
    if current.is_empty()
        || current.len() > count
        || count == 0
        || count > 64
        || !delta.is_finite()
        || current
            .iter()
            .any(|volume| !volume.is_finite() || !(0.0..=4.0).contains(volume))
    {
        return None;
    }
    Some(
        (0..count)
            .map(|index| {
                let previous = current.get(index).copied().unwrap_or(current[0]).cbrt();
                let target = ((previous + delta).clamp(0.0, 1.0) * 100.0).round() / 100.0;
                (target.powi(3) * 1e6).round() / 1e6
            })
            .collect(),
    )
}

fn step_audio_sink(delta: f64) -> Result<AudioStepOutcome, Failure> {
    step_audio_sink_with(delta, &mut production_run)
}

/// One uncached step (the `audio.step` method): read the sink, then write.
fn step_audio_sink_with(
    delta: f64,
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
) -> Result<AudioStepOutcome, Failure> {
    AudioSteps::default().step(delta, run, &mut |_| {})
}

fn step_wpctl_sink(
    inspect: &str,
    delta: f64,
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
) -> Result<AudioStepOutcome, Failure> {
    let property = |key: &str| {
        inspect.lines().find_map(|line| {
            let rest = line
                .trim_start_matches([' ', '*', '\t'])
                .strip_prefix(key)?;
            Some(
                rest.trim_start()
                    .strip_prefix('=')?
                    .trim()
                    .trim_matches('"')
                    .to_owned(),
            )
        })
    };
    let node_name = property("node.name").ok_or_else(unavailable)?;
    let label = property("node.description")
        .filter(|text| !text.is_empty())
        .or_else(|| property("node.nick"))
        .unwrap_or_default();
    let step = format!("{:.2}{}", delta.abs(), if delta < 0.0 { "-" } else { "+" });
    run(
        "wpctl",
        &["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", &step],
        true,
    )?;
    let readback =
        run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"], false).map_err(|_| action_error())?;
    let volume = parse_volume(&readback).ok_or_else(action_error)?.min(1.0);
    Ok(AudioStepOutcome {
        node_name,
        label,
        linear: volume.powi(3),
        volume: (volume * 1000.0).round() / 1000.0,
        muted: readback.contains("[MUTED]"),
    })
}

impl SinkVolume {
    /// Linear level of the loudest channel, as the PipeWire watcher reports it.
    fn level(&self) -> f64 {
        self.current.iter().copied().fold(0.0, f64::max)
    }

    fn outcome(&self, channels: &[f64]) -> AudioStepOutcome {
        let linear = channels.iter().copied().fold(0.0, f64::max);
        AudioStepOutcome {
            node_name: self.node_name.clone(),
            label: self.label.clone(),
            linear,
            volume: (linear.cbrt() * 1000.0).round() / 1000.0,
            muted: self.muted,
        }
    }
}

fn write_sink(
    sink: &SinkVolume,
    channels: &[f64],
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
) -> Result<(), Failure> {
    let (object_id, param, payload) = match sink.param {
        VolumeParam::NodeProps { node_id } => (
            node_id.to_string(),
            "Props",
            json!({"channelVolumes":channels}),
        ),
        VolumeParam::DeviceRoute {
            device_id,
            route_index,
            route_device,
        } => (
            device_id.to_string(),
            "Route",
            json!({
                "index":route_index,
                "device":route_device,
                "props":{"channelVolumes":channels},
                "save":true
            }),
        ),
    };
    let payload = serde_json::to_string(&payload).map_err(|_| action_error())?;
    run("pw-cli", &["set-param", &object_id, param, &payload], true)?;
    Ok(())
}

fn run_pipewire_action(program: &str, args: &[&str], mutation: bool) -> Result<String, Failure> {
    let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let error = || {
        if mutation {
            action_error()
        } else {
            unavailable()
        }
    };
    let mut command = std::process::Command::new(program);
    command.args(&args).env_remove("NOTIFY_SOCKET");
    let output =
        crate::process::capture(command, None, Duration::from_secs(3)).map_err(|_| error())?;
    if !output.status.success() {
        return Err(error());
    }
    if output.stdout.len() > 4 * 1024 * 1024 || output.stderr.len() > 4096 {
        return Err(error());
    }
    String::from_utf8(output.stdout).map_err(|_| error())
}

#[cfg(test)]
fn adjust_volume_value_with(
    delta: f64,
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
) -> Result<f64, Failure> {
    let current = run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"], false)?;
    let current = parse_volume(&current).ok_or_else(unavailable)?;
    let target = (current + delta).clamp(0.0, 1.0);
    let target = (target * 1000.0).round() / 1000.0;
    let text = format!("{target:.3}");
    run(
        "wpctl",
        &["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", &text],
        true,
    )?;
    Ok(target)
}

fn toggle_mute() -> Result<(), Failure> {
    let current = run_action("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"], false)?;
    parse_volume(&current).ok_or_else(unavailable)?;
    run_action(
        "wpctl",
        &["set-mute", "@DEFAULT_AUDIO_SINK@", "toggle"],
        true,
    )?;
    Ok(())
}

fn set_bluetooth(enabled: bool) -> Result<(), Failure> {
    set_bluetooth_with(enabled, &mut run_action)
}

fn set_bluetooth_with(
    enabled: bool,
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
) -> Result<(), Failure> {
    // `show` verifies BlueZ exposes a default controller before issuing a power change.
    let state = run("bluetoothctl", &["show"], false)?;
    if !state
        .lines()
        .any(|line| line.trim_start().starts_with("Controller "))
    {
        return Err(unavailable());
    }
    run(
        "bluetoothctl",
        &["power", if enabled { "on" } else { "off" }],
        true,
    )?;
    let state = run("bluetoothctl", &["show"], false).map_err(|_| action_error())?;
    let powered = state
        .lines()
        .find_map(|line| line.trim().strip_prefix("Powered:").map(str::trim));
    if powered != Some(if enabled { "yes" } else { "no" }) {
        return Err(action_error());
    }
    Ok(())
}

fn adjust_volume_with(
    delta: f64,
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
) -> Result<(), Failure> {
    let current = run("wpctl", &["get-volume", "@DEFAULT_AUDIO_SINK@"], false)?;
    let current = parse_volume(&current).ok_or_else(unavailable)?;
    let target = (current + delta).clamp(0.0, 1.0);
    let target = format!("{target:.3}");
    run(
        "wpctl",
        &["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", &target],
        true,
    )?;
    Ok(())
}

fn run_action(program: &str, args: &[&str], mutation: bool) -> Result<String, Failure> {
    let args = args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
    let output =
        crate::process::checked(program, &args, None, Duration::from_secs(5)).map_err(|_| {
            if mutation {
                action_error()
            } else {
                unavailable()
            }
        })?;
    let error = || {
        if mutation {
            action_error()
        } else {
            unavailable()
        }
    };
    if output.stdout.len() > 4096 || output.stderr.len() > 4096 {
        return Err(error());
    }
    String::from_utf8(output.stdout).map_err(|_| error())
}

fn ensure_backend(method: &str) -> Result<(), Failure> {
    if backend_present(method) {
        Ok(())
    } else {
        Err(unavailable())
    }
}

fn parse_volume(output: &str) -> Option<f64> {
    let value = output
        .strip_prefix("Volume:")?
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()?;
    (value.is_finite() && (0.0..=4.0).contains(&value)).then_some(value)
}

fn invalid() -> Failure {
    failure(ErrorCode::Validation, "Invalid status pill action", false).dispatch_state(false)
}
fn unavailable() -> Failure {
    failure(
        ErrorCode::Unavailable,
        "Default device is unavailable",
        false,
    )
    .dispatch_state(false)
}
fn action_error() -> Failure {
    failure(
        ErrorCode::Io,
        "Status pill action failed; it may already have completed",
        false,
    )
    .dispatch_state(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_step_publishes_the_osd_in_the_same_call_without_any_poll() {
        let hub = super::super::hub::Hub::new("step-osd".into(), false, Duration::from_millis(1))
            .shared();
        let mut notices = hub.borrow().observe();
        let outcome = AudioStepOutcome {
            node_name: "alsa_output.speakers".into(),
            label: "Speakers".into(),
            linear: 0.35_f64.powi(3),
            volume: 0.35,
            muted: false,
        };
        publish_audio_step(&hub, &outcome, None).unwrap();
        let mut seen = false;
        while let Ok(notice) = notices.try_recv() {
            if let super::super::hub::Notice::Event { topic, data, .. } = notice {
                seen |= topic == "osd"
                    && data["value"] == 0.35
                    && data["label"] == "Speakers"
                    && data["device"] == "alsa_output.speakers";
            }
        }
        assert!(
            seen,
            "the OSD topic is published synchronously with the step"
        );
    }

    #[test]
    fn step_at_the_limit_still_publishes_the_osd_without_an_audio_snapshot_entry() {
        let hub = super::super::hub::Hub::new("step-limit".into(), false, Duration::from_millis(1))
            .shared();
        hub.borrow_mut()
            .publish(
                "audio",
                json!({"default_output":"other","nodes":{"items":{},"order":[]}}),
            )
            .unwrap();
        let mut notices = hub.borrow().observe();
        let outcome = AudioStepOutcome {
            node_name: "alsa_output.pro-output-0".into(),
            label: String::new(),
            linear: 1.0,
            volume: 1.0,
            muted: true,
        };
        publish_audio_step(&hub, &outcome, Some(1)).unwrap();
        let mut seen = false;
        while let Ok(notice) = notices.try_recv() {
            if let super::super::hub::Notice::Event { topic, data, .. } = notice {
                seen |= topic == "osd" && data["value"] == 1.0 && data["muted"] == true;
            }
        }
        assert!(seen, "a step at 100 % shows the pill");
    }

    #[test]
    fn idle_audio_step_worker_waits_for_commands_and_stops_on_drop() {
        let (completed, _results) = tokio::sync::mpsc::unbounded_channel();
        let worker = AudioStepWorker::start(completed).unwrap();
        let sender = worker.sender();
        drop(worker);
        assert!(matches!(
            sender.try_send(AudioStepCommand::Stop),
            Err(mpsc::TrySendError::Disconnected(_))
        ));
    }

    #[test]
    fn volume_parser_accepts_only_bounded_normalized_values() {
        assert_eq!(parse_volume("Volume: 0.65\n"), Some(0.65));
        assert_eq!(parse_volume("Volume: 1.00 [MUTED]\n"), Some(1.0));
        assert_eq!(parse_volume("Volume: 5\n"), None);
        assert_eq!(parse_volume("Volume: NaN\n"), None);
        assert_eq!(parse_volume("noise"), None);
    }

    #[test]
    fn step_updates_all_eight_current_pipewire_channels() {
        let mut dump = json!([
            {
                "id": 17,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "props": {
                        "node.name": "pro_audio_sink",
                        "media.class": "Audio/Sink",
                        "device.id": "30",
                        "card.profile.device": "2"
                    },
                    "params": {}
                }
            },
            {
                "id": 30,
                "type": "PipeWire:Interface:Device",
                "info": {
                    "params": {
                        "Route": [{
                            "index": 4,
                            "device": 2,
                            "props": {
                                "channelMap": ["FL", "FR", "FC", "LFE", "RL", "RR", "SL", "SR"],
                                "channelVolumes": cubes(&[0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80])
                            }
                        }]
                    }
                }
            }
        ]);
        let mut set = Vec::new();
        let mut run = |program: &str, args: &[&str], mutation: bool| {
            if program == "wpctl" {
                assert_eq!(args, ["inspect", "@DEFAULT_AUDIO_SINK@"]);
                return Ok("id 17, type PipeWire:Interface:Node\n".into());
            }
            if program == "pw-dump" {
                assert!(args.is_empty());
                return Ok(dump.to_string());
            }
            assert_eq!(program, "pw-cli");
            assert!(mutation);
            let payload: Value = serde_json::from_str(args[3]).unwrap();
            dump[1]["info"]["params"]["Route"][0]["props"]["channelVolumes"] =
                payload["props"]["channelVolumes"].clone();
            set.push(args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>());
            Ok(String::new())
        };
        let result = step_audio_sink_with(0.05, &mut run).unwrap();
        assert_eq!(result.node_name, "pro_audio_sink");
        step_audio_sink_with(0.05, &mut run).unwrap();
        let first = serde_json::from_str::<Value>(&set[0][3]).unwrap();
        assert_eq!(
            perceptual(&first["props"]["channelVolumes"]),
            vec![15, 25, 35, 45, 55, 65, 75, 85]
        );
        let final_set = set.last().unwrap();
        assert_eq!(&final_set[..3], ["set-param", "30", "Route"]);
        let payload: Value = serde_json::from_str(&final_set[3]).unwrap();
        assert_eq!(payload["index"], 4);
        assert_eq!(payload["device"], 2);
        assert_eq!(
            payload["props"]["channelVolumes"].as_array().unwrap().len(),
            8
        );
        assert_eq!(
            perceptual(&payload["props"]["channelVolumes"]),
            vec![20, 30, 40, 50, 60, 70, 80, 90]
        );
        assert_eq!(payload["save"], true);
    }

    fn cubes(levels: &[f64]) -> Value {
        json!(levels.iter().map(|level| level.powi(3)).collect::<Vec<_>>())
    }

    /// Linear channel volumes as whole perceptual percent.
    fn perceptual(volumes: &Value) -> Vec<i64> {
        volumes
            .as_array()
            .unwrap()
            .iter()
            .map(|v| (v.as_f64().unwrap().cbrt() * 100.0).round() as i64)
            .collect()
    }

    /// The owner's laptop sink (2026-10-29): a pro-audio profile node whose device has no
    /// routes (`device.routes = 0`) and whose last `Props` entry holds ALSA settings only.
    /// Both made every key step `Unavailable` before N0102.
    fn pro_audio_dump(volume: f64, muted: bool) -> Value {
        json!([
            {
                "id": 53,
                "type": "PipeWire:Interface:Node",
                "info": {
                    "props": {
                        "node.name": "alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.pro-output-0",
                        "node.description": "HD Audio Pro",
                        "media.class": "Audio/Sink",
                        "device.id": "42",
                        "card.profile.device": "7",
                        "device.routes": "0"
                    },
                    "params": {
                        "Props": [
                            {"volume": 1.0, "mute": muted, "channelVolumes": [volume, volume],
                             "channelMap": ["AUX0", "AUX1"]},
                            {"device": "hw:0,0", "deviceName": "", "params": []}
                        ]
                    }
                }
            },
            {"id": 42, "type": "PipeWire:Interface:Device", "info": {"params": {"Route": [], "EnumRoute": []}}}
        ])
    }

    fn pro_audio_run<'a>(
        dump: &'a mut Value,
        calls: &'a mut Vec<Vec<String>>,
    ) -> impl FnMut(&str, &[&str], bool) -> Result<String, Failure> + 'a {
        move |program: &str, args: &[&str], _mutation: bool| {
            calls.push(
                std::iter::once(program)
                    .chain(args.iter().copied())
                    .map(str::to_owned)
                    .collect(),
            );
            match program {
                "wpctl" => Ok("id 53, type PipeWire:Interface:Node\n".into()),
                "pw-dump" => Ok(dump.to_string()),
                _ => {
                    assert_eq!(&args[..3], ["set-param", "53", "Props"]);
                    let payload: Value = serde_json::from_str(args[3]).unwrap();
                    dump[0]["info"]["params"]["Props"][0]["channelVolumes"] =
                        payload["channelVolumes"].clone();
                    Ok(String::new())
                }
            }
        }
    }

    #[test]
    fn pro_audio_sink_without_routes_steps_its_node_props() {
        let mut dump = pro_audio_dump(0.5_f64.powi(3), false);
        let mut calls = Vec::new();
        let mut run = pro_audio_run(&mut dump, &mut calls);
        let up = step_audio_sink_with(0.05, &mut run).unwrap();
        assert_eq!(up.volume, 0.55);
        assert_eq!(up.label, "HD Audio Pro");
        assert!(!up.muted);
        let down = step_audio_sink_with(-0.05, &mut run).unwrap();
        assert_eq!(down.volume, 0.5);
        drop(run);
        assert_eq!(calls.iter().filter(|c| c[0] == "pw-cli").count(), 2);
        assert_eq!(
            perceptual(&dump[0]["info"]["params"]["Props"][0]["channelVolumes"]),
            vec![50, 50]
        );
    }

    #[test]
    fn steps_at_the_limits_report_the_level_without_writing() {
        // The live readback: wpctl 0.98 = linear 0.938314; one step up caps at 100 %.
        let mut dump = pro_audio_dump(0.938314, false);
        let mut calls = Vec::new();
        let mut run = pro_audio_run(&mut dump, &mut calls);
        assert_eq!(step_audio_sink_with(0.05, &mut run).unwrap().volume, 1.0);
        let held = step_audio_sink_with(0.05, &mut run).unwrap();
        assert_eq!((held.volume, held.linear), (1.0, 1.0));
        drop(run);
        assert_eq!(calls.iter().filter(|c| c[0] == "pw-cli").count(), 1);

        let mut dump = pro_audio_dump(0.0, true);
        let mut calls = Vec::new();
        let mut run = pro_audio_run(&mut dump, &mut calls);
        let floor = step_audio_sink_with(-0.05, &mut run).unwrap();
        assert_eq!((floor.volume, floor.muted), (0.0, true));
        drop(run);
        assert!(calls.iter().all(|c| c[0] != "pw-cli"));
    }

    /// A fake PipeWire for the worker: `pw-dump` is the live pro-audio fixture, every
    /// `pw-cli` write takes `write_cost` and lands in the fixture; calls are counted.
    #[derive(Clone)]
    struct FakeSink {
        dump: Arc<std::sync::Mutex<Value>>,
        calls: Arc<std::sync::Mutex<Vec<String>>>,
        write_cost: Duration,
    }
    impl FakeSink {
        fn new(perceptual: f64, write_cost: Duration) -> Self {
            Self {
                dump: Arc::new(std::sync::Mutex::new(pro_audio_dump(
                    perceptual.powi(3),
                    false,
                ))),
                calls: Arc::default(),
                write_cost,
            }
        }
        fn run(&self) -> impl FnMut(&str, &[&str], bool) -> Result<String, Failure> {
            let this = self.clone();
            move |program: &str, args: &[&str], _mutation: bool| {
                this.calls.lock().unwrap().push(program.to_owned());
                match program {
                    "wpctl" => Ok("id 53, type PipeWire:Interface:Node\n".into()),
                    "pw-dump" => Ok(this.dump.lock().unwrap().to_string()),
                    _ => {
                        std::thread::sleep(this.write_cost);
                        let payload: Value = serde_json::from_str(args[3]).unwrap();
                        this.dump.lock().unwrap()[0]["info"]["params"]["Props"][0]
                            ["channelVolumes"] = payload["channelVolumes"].clone();
                        Ok(String::new())
                    }
                }
            }
        }
        fn count(&self, program: &str) -> usize {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| *call == program)
                .count()
        }
        fn level(&self) -> Vec<i64> {
            perceptual(&self.dump.lock().unwrap()[0]["info"]["params"]["Props"][0]["channelVolumes"])
        }
    }

    fn observed(default: &str, perceptual: f64) -> AudioStepCommand {
        AudioStepCommand::Observe(AudioObservation {
            default: Some(default.into()),
            linear: Some(perceptual.powi(3)),
            muted: Some(false),
        })
    }

    const SINK: &str = "alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.pro-output-0";

    /// Starts the worker loop on a fake sink; returns the command sender, the completions
    /// with their arrival time, and the loop thread.
    fn fake_worker(
        sink: &FakeSink,
    ) -> (
        SyncSender<AudioStepCommand>,
        mpsc::Receiver<(std::time::Instant, AudioStepCompletion)>,
        std::thread::JoinHandle<()>,
    ) {
        let (sender, receiver) = mpsc::sync_channel(256);
        let (done, completions) = mpsc::channel();
        let mut run = sink.run();
        let thread = std::thread::spawn(move || {
            let stop = std::sync::atomic::AtomicBool::new(false);
            serve_steps(&receiver, &mut run, &stop, &mut |completion| {
                let _ = done.send((std::time::Instant::now(), completion));
            });
        });
        (sender, completions, thread)
    }

    fn press(sender: &SyncSender<AudioStepCommand>, delta: f64) {
        sender
            .try_send(AudioStepCommand::Apply(FastAudioStep {
                delta,
                key_ms: Some(1),
            }))
            .unwrap();
    }

    #[test]
    fn held_key_on_a_slow_sink_coalesces_into_at_most_two_writes_and_stops_at_release() {
        // Each write costs 100 ms (the old per-step cost was ~300 ms); 10 presses 5 ms apart.
        let sink = FakeSink::new(0.30, Duration::from_millis(100));
        let (sender, completions, thread) = fake_worker(&sink);
        sender.try_send(observed(SINK, 0.30)).unwrap();
        let started = std::time::Instant::now();
        for _ in 0..10 {
            press(&sender, 0.05);
            std::thread::sleep(Duration::from_millis(5));
        }
        let released = std::time::Instant::now();
        drop(sender);
        thread.join().unwrap();
        let after_release = released.elapsed();
        let completions: Vec<_> = completions.try_iter().collect();
        assert_eq!(sink.level(), vec![80, 80], "ten steps of 5 % from 30 %");
        assert!(sink.count("pw-cli") <= 2, "writes: {}", sink.count("pw-cli"));
        assert_eq!(sink.count("pw-dump"), 1, "the sink is read once");
        let last = completions.last().unwrap();
        assert_eq!(last.1.result.as_ref().unwrap().volume, 0.8);
        // No backlog: the volume stops at most one write (100 ms) after the release.
        assert!(
            after_release < Duration::from_millis(250),
            "still writing {after_release:?} after release ({:?} in all)",
            started.elapsed()
        );
        assert!(last.0 < released + Duration::from_millis(250));
        assert!(completions.len() <= 2, "one OSD per batch, no replay");
    }

    #[test]
    fn cached_step_shows_the_osd_before_the_write_and_costs_one_call() {
        let sink = FakeSink::new(0.50, Duration::from_millis(80));
        let (sender, completions, thread) = fake_worker(&sink);
        sender.try_send(observed(SINK, 0.50)).unwrap();
        // The observation fills the cache off the key path.
        while sink.count("pw-dump") == 0 {
            std::thread::sleep(Duration::from_millis(1));
        }
        std::thread::sleep(Duration::from_millis(5));
        let calls = sink.calls.lock().unwrap().len();
        let key = std::time::Instant::now();
        press(&sender, 0.05);
        let (shown, completion) = completions.recv().unwrap();
        let latency = shown.duration_since(key);
        assert_eq!(completion.result.unwrap().volume, 0.55);
        assert!(latency < Duration::from_millis(10), "OSD after {latency:?}");
        drop(sender);
        thread.join().unwrap();
        assert_eq!(sink.calls.lock().unwrap().len() - calls, 1, "one pw-cli write");
        assert!(completions.try_recv().is_err(), "the write agreed: no second OSD");
        eprintln!("cached step: OSD {latency:?} after the key");
    }

    #[test]
    fn observations_follow_external_changes_and_ignore_our_own_echo() {
        let sink = FakeSink::new(0.50, Duration::ZERO);
        let mut run = sink.run();
        let mut steps = AudioSteps::default();
        let observe = |steps: &mut AudioSteps, run: &mut _, default: &str, level: f64| {
            let AudioStepCommand::Observe(observation) = observed(default, level) else {
                unreachable!()
            };
            steps.observe(observation, run);
        };
        observe(&mut steps, &mut run, SINK, 0.50);
        assert_eq!(sink.count("pw-dump"), 1);
        let step = steps.step(0.05, &mut run, &mut |_| {}).unwrap();
        assert_eq!(step.volume, 0.55);
        // The watcher's late reading of an earlier level is our own echo.
        observe(&mut steps, &mut run, SINK, 0.50);
        assert_eq!(steps.step(0.05, &mut run, &mut |_| {}).unwrap().volume, 0.6);
        // A slider change elsewhere, after the echo window, is adopted without a read.
        steps.last_write = None;
        observe(&mut steps, &mut run, SINK, 0.20);
        assert_eq!(steps.step(0.05, &mut run, &mut |_| {}).unwrap().volume, 0.25);
        assert_eq!(sink.count("pw-dump"), 1);
        // A new default output is read again.
        observe(&mut steps, &mut run, "bluez_output.headset", 0.20);
        assert_eq!(sink.count("pw-dump"), 2);
    }

    /// Measures one step against a real PipeWire. It CHANGES the default sink's volume, so
    /// run it only in a private session: `cargo test ... live_pipewire_step_cost -- --ignored`.
    #[test]
    #[ignore = "changes the default sink of the running PipeWire"]
    fn live_pipewire_step_cost() {
        let median = |mut samples: Vec<Duration>| {
            samples.sort();
            samples[samples.len() / 2]
        };
        let uncached = (0..6)
            .map(|index| {
                let started = std::time::Instant::now();
                step_audio_sink(if index % 2 == 0 { -0.05 } else { 0.05 }).unwrap();
                started.elapsed()
            })
            .collect();
        let mut steps = AudioSteps::default();
        let mut run = production_run;
        steps.step(0.0, &mut run, &mut |_| {}).unwrap();
        let (mut osd, mut write) = (Vec::new(), Vec::new());
        for index in 0..20 {
            let started = std::time::Instant::now();
            let mut shown = None;
            steps
                .step(if index % 2 == 0 { -0.05 } else { 0.05 }, &mut run, &mut |_| {
                    shown = Some(started.elapsed())
                })
                .unwrap();
            write.push(started.elapsed());
            osd.push(shown.unwrap());
        }
        eprintln!(
            "STEP_COST uncached={:?} cached_write={:?} cached_osd={:?}",
            median(uncached),
            median(write),
            median(osd)
        );
    }

    #[test]
    fn unreadable_pipewire_object_falls_back_to_wpctl_and_reads_the_level_back() {
        let mut calls = Vec::new();
        let mut run = |program: &str, args: &[&str], _mutation: bool| {
            calls.push(
                std::iter::once(program)
                    .chain(args.iter().copied())
                    .map(str::to_owned)
                    .collect::<Vec<_>>(),
            );
            Ok(match (program, args.first().copied()) {
                ("wpctl", Some("inspect")) => "id 9, type PipeWire:Interface:Node\n    node.description = \"Speakers\"\n  * node.name = \"alsa_output.speakers\"\n".into(),
                ("wpctl", Some("get-volume")) => "Volume: 1.00 [MUTED]\n".into(),
                ("pw-dump", _) => "[]".into(),
                _ => String::new(),
            })
        };
        let outcome = step_audio_sink_with(0.05, &mut run).unwrap();
        assert_eq!(
            outcome,
            AudioStepOutcome {
                node_name: "alsa_output.speakers".into(),
                label: "Speakers".into(),
                linear: 1.0,
                volume: 1.0,
                muted: true,
            }
        );
        assert_eq!(
            calls[2],
            [
                "wpctl",
                "set-volume",
                "-l",
                "1.0",
                "@DEFAULT_AUDIO_SINK@",
                "0.05+"
            ]
        );
    }

    #[test]
    fn step_normalizes_short_volume_vector_to_the_full_channel_map() {
        let stepped = stepped_channel_volumes(&[0.064, 0.125], 8, 0.05).unwrap();
        assert_eq!(
            perceptual(&json!(stepped)),
            vec![45, 55, 45, 45, 45, 45, 45, 45]
        );
        let stepped = stepped_channel_volumes(&[0.0, 0.125], 2, -0.05).unwrap();
        assert_eq!(perceptual(&json!(stepped)), vec![0, 45]);
    }

    #[test]
    fn adjustment_uses_only_default_sink_and_caps_volume() {
        let mut calls = Vec::new();
        let mut run = |program: &str, args: &[&str], mutation: bool| {
            calls.push((
                program.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
                mutation,
            ));
            Ok(if mutation {
                String::new()
            } else {
                "Volume: 0.95".into()
            })
        };
        adjust_volume_with(0.2, &mut run).unwrap();
        assert_eq!(
            calls,
            vec![
                (
                    "wpctl".into(),
                    vec!["get-volume".into(), "@DEFAULT_AUDIO_SINK@".into()],
                    false
                ),
                (
                    "wpctl".into(),
                    vec![
                        "set-volume".into(),
                        "-l".into(),
                        "1.0".into(),
                        "@DEFAULT_AUDIO_SINK@".into(),
                        "1.000".into()
                    ],
                    true
                ),
            ]
        );
    }

    #[test]
    fn held_step_returns_the_clamped_value_for_immediate_osd() {
        let mut calls = Vec::new();
        let mut run = |program: &str, args: &[&str], mutation: bool| {
            calls.push((
                program.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
                mutation,
            ));
            Ok(if mutation {
                String::new()
            } else {
                "Volume: 0.95".into()
            })
        };
        assert_eq!(adjust_volume_value_with(0.05, &mut run).unwrap(), 1.0);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[1].1.last().map(String::as_str), Some("1.000"));
    }

    #[test]
    fn malformed_readback_never_dispatches_a_mutation() {
        let mut calls = Vec::new();
        let mut run = |program: &str, args: &[&str], mutation: bool| {
            calls.push((
                program.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
                mutation,
            ));
            Ok("not a volume".into())
        };
        assert!(adjust_volume_with(0.1, &mut run).is_err());
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].2);
    }

    #[test]
    fn failed_volume_mutation_is_reported_as_dispatched() {
        let mut calls = Vec::new();
        let mut run = |program: &str, args: &[&str], mutation: bool| {
            calls.push((
                program.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
                mutation,
            ));
            if mutation {
                Err(action_error())
            } else {
                Ok("Volume: 0.5".into())
            }
        };
        let error = adjust_volume_with(0.1, &mut run).unwrap_err();
        assert_eq!(error.code, ErrorCode::Io);
        assert_eq!(error.dispatched, Some(true));
        assert_eq!(calls.len(), 2);
        assert!(calls[1].2);
    }

    #[test]
    fn bluetooth_reports_success_only_after_verified_power_state() {
        let mut states = [
            "Controller AA:BB:CC:DD:EE:FF (public)\nPowered: no\n",
            "",
            "Controller AA:BB:CC:DD:EE:FF (public)\nPowered: yes\n",
        ]
        .into_iter();
        let mut calls = Vec::new();
        let mut run = |program: &str, args: &[&str], mutation: bool| {
            calls.push((
                program.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
                mutation,
            ));
            Ok(states.next().unwrap().to_owned())
        };
        assert!(set_bluetooth_with(true, &mut run).is_ok());
        assert_eq!(
            calls[1],
            (
                "bluetoothctl".into(),
                vec!["power".into(), "on".into()],
                true
            )
        );
    }

    #[test]
    fn missing_bluetooth_controller_never_dispatches_power_change() {
        let mut calls = Vec::new();
        let mut run = |program: &str, args: &[&str], mutation: bool| {
            calls.push((
                program.to_owned(),
                args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>(),
                mutation,
            ));
            Ok("No default controller available\n".into())
        };
        assert!(set_bluetooth_with(true, &mut run).is_err());
        assert_eq!(calls.len(), 1);
        assert!(!calls[0].2);
    }
}
