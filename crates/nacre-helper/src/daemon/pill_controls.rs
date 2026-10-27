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

pub(crate) struct AudioStepCompletion {
    pub(crate) result: Result<AudioStepOutcome, Failure>,
    pub(crate) key_ms: Option<u64>,
}

pub(crate) struct AudioStepWorker {
    stop: Arc<std::sync::atomic::AtomicBool>,
    sender: Option<SyncSender<FastAudioStep>>,
    thread: Option<JoinHandle<()>>,
}

impl AudioStepWorker {
    pub(crate) fn start(
        completed: tokio::sync::mpsc::UnboundedSender<AudioStepCompletion>,
    ) -> crate::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<FastAudioStep>(64);
        let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = std::thread::Builder::new()
            .name("nacre-audio-step".into())
            .stack_size(512 * 1024)
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        tracing::error!(%error, "Audio step worker runtime failed");
                        return;
                    }
                };
                while !thread_stop.load(Ordering::Acquire) {
                    let request = match receiver.recv_timeout(Duration::from_millis(100)) {
                        Ok(request) => request,
                        Err(mpsc::RecvTimeoutError::Timeout) => continue,
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    };
                    let guard = runtime.block_on(audio_lock().lock_owned());
                    let result = step_audio_sink(request.delta);
                    drop(guard);
                    let _ = completed.send(AudioStepCompletion {
                        result,
                        key_ms: request.key_ms,
                    });
                }
            })?;
        Ok(Self {
            stop,
            sender: Some(sender),
            thread: Some(thread),
        })
    }

    pub(crate) fn sender(&self) -> SyncSender<FastAudioStep> {
        self.sender
            .as_ref()
            .expect("audio worker is active")
            .clone()
    }
}

impl Drop for AudioStepWorker {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            if thread.join().is_err() {
                tracing::warn!("Audio step worker panicked");
            }
        }
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
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| {
            let executable = directory.join(program);
            executable.is_file()
                && std::fs::metadata(executable)
                    .is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
        })
    })
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

pub(crate) fn publish_audio_step(
    hub: &Shared,
    outcome: &AudioStepOutcome,
    key_ms: Option<u64>,
) -> Result<(), Failure> {
    let value = outcome.channels[0];
    let Some(mut state) = hub.borrow().snapshot("audio") else {
        super::osd::volume(hub, value);
        return Ok(());
    };
    let is_default = state["default_output"].as_str() == Some(outcome.node_name.as_str());
    if let Some(node) = state["nodes"]["items"].get_mut(&outcome.node_name) {
        node["volume"] = json!(value);
        if let Some(readback) = node.get_mut("readback") {
            readback["linear_volume"] = json!(value);
            readback["perceptual_volume"] = json!(value.cbrt());
        }
        hub.borrow_mut()
            .publish("audio", state)
            .map_err(|_| action_error())?;
    }
    if is_default {
        super::osd::volume(hub, value);
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
    pub(crate) channels: Vec<f64>,
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
    param: VolumeParam,
    current: Vec<f64>,
    channel_count: usize,
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

fn current_param<'a>(object: &'a Value, name: &str) -> Option<&'a Value> {
    let params = object.get("info")?.get("params")?.get(name)?;
    params.as_array()?.last()
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
    let device_id = node_props.get("device.id").and_then(object_id);
    let route_device = node_props.get("card.profile.device").and_then(object_id);

    if let (Some(device_id), Some(route_device)) = (device_id, route_device) {
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
        let route_index = object_id(&route["index"])?;
        let route_device = object_id(&route["device"])?;
        let (current, channel_count) = volume_data(route.get("props")?)?;
        return Some(SinkVolume {
            node_name,
            param: VolumeParam::DeviceRoute {
                device_id,
                route_index,
                route_device,
            },
            current,
            channel_count,
        });
    }

    let props = current_param(node, "Props")?;
    let (current, channel_count) = volume_data(props)?;
    Some(SinkVolume {
        node_name,
        param: VolumeParam::NodeProps { node_id },
        current,
        channel_count,
    })
}

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
                let previous = current.get(index).copied().unwrap_or(current[0]);
                ((previous + delta).clamp(0.0, 1.0) * 1000.0).round() / 1000.0
            })
            .collect(),
    )
}

fn step_audio_sink(delta: f64) -> Result<AudioStepOutcome, Failure> {
    let mut run = |program: &str, args: &[&str], mutation: bool| {
        if program == "wpctl" {
            run_action(program, args, mutation)
        } else {
            run_pipewire_action(program, args, mutation)
        }
    };
    step_audio_sink_with(delta, &mut run)
}

fn step_audio_sink_with(
    delta: f64,
    run: &mut impl FnMut(&str, &[&str], bool) -> Result<String, Failure>,
) -> Result<AudioStepOutcome, Failure> {
    let inspect = run("wpctl", &["inspect", "@DEFAULT_AUDIO_SINK@"], false)?;
    let node_id = parse_object_id(&inspect).ok_or_else(unavailable)?;
    let dump = run("pw-dump", &[], false)?;
    let dump: Value = serde_json::from_str(&dump).map_err(|_| unavailable())?;
    let sink = sink_volume_from_dump(node_id, &dump).ok_or_else(unavailable)?;
    let channels = stepped_channel_volumes(&sink.current, sink.channel_count, delta)
        .ok_or_else(unavailable)?;
    let (object_id, param, payload) = match sink.param {
        VolumeParam::NodeProps { node_id } => (
            node_id.to_string(),
            "Props",
            json!({"channelVolumes":channels.clone()}),
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
                "props":{"channelVolumes":channels.clone()},
                "save":true
            }),
        ),
    };
    let payload = serde_json::to_string(&payload).map_err(|_| action_error())?;
    run("pw-cli", &["set-param", &object_id, param, &payload], true)?;
    Ok(AudioStepOutcome {
        node_name: sink.node_name,
        channels,
    })
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
                                "channelVolumes": [0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80]
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
        assert_eq!(
            result.channels,
            vec![0.15, 0.25, 0.35, 0.45, 0.55, 0.65, 0.75, 0.85]
        );
        let second = step_audio_sink_with(0.05, &mut run).unwrap();
        assert_eq!(
            second.channels,
            vec![0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90]
        );
        let first = serde_json::from_str::<Value>(&set[0][3]).unwrap();
        assert_eq!(
            first["props"]["channelVolumes"],
            json!([0.15, 0.25, 0.35, 0.45, 0.55, 0.65, 0.75, 0.85])
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
            payload["props"]["channelVolumes"],
            json!([0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80, 0.90])
        );
        assert_eq!(payload["save"], true);
    }

    #[test]
    fn step_normalizes_short_volume_vector_to_the_full_channel_map() {
        let stepped = stepped_channel_volumes(&[0.40, 0.25], 8, 0.05).unwrap();
        assert_eq!(
            stepped,
            vec![0.45, 0.30, 0.45, 0.45, 0.45, 0.45, 0.45, 0.45]
        );
        assert_eq!(
            stepped_channel_volumes(&[0.0, 0.5], 2, -0.05),
            Some(vec![0.0, 0.45])
        );
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
