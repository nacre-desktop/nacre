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
        Arc, OnceLock,
    },
    time::Duration,
};

static AUDIO_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();
static STEP_STATE_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();
static LAST_STEP_MARKER_MS: AtomicU64 = AtomicU64::new(0);
static AUDIO_STEP_REVISION: AtomicU64 = AtomicU64::new(0);
static LATEST_AUDIO_STEP_REVISION: AtomicU64 = AtomicU64::new(0);

pub fn methods() -> BTreeMap<String, Method> {
    nacre_protocol::pill_controls::methods()
        .into_iter()
        .filter(|(name, _)| backend_present(name))
        .collect()
}

fn backend_present(method: &str) -> bool {
    let (program, service) = match method {
        "audio.adjust_volume" | "audio.step" | "audio.toggle_mute" => ("wpctl", "pipewire-0"),
        "bluetooth.set_enabled" => ("bluetoothctl", "system_bus_socket"),
        _ => return false,
    };
    executable(program)
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
            // Update the published desired state and OSD before waiting for PipeWire.
            let state_guard = STEP_STATE_LOCK
                .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone()
                .lock_owned()
                .await;
            let mut state = hub.borrow().snapshot("audio").ok_or_else(unavailable)?;
            let output = state["default_output"]
                .as_str()
                .ok_or_else(unavailable)?
                .to_owned();
            let node = state["nodes"]["items"]
                .get_mut(&output)
                .ok_or_else(unavailable)?;
            let current = node["volume"]
                .as_f64()
                .filter(|v| v.is_finite() && *v >= 0.0)
                .ok_or_else(unavailable)?;
            let previous = current;
            let value = (current + delta).clamp(0.0, 1.0);
            node["volume"] = json!(value);
            if let Some(readback) = node.get_mut("readback") {
                readback["linear_volume"] = json!(value);
                readback["perceptual_volume"] = json!(value.cbrt());
            }
            hub.borrow_mut()
                .publish("audio", state)
                .map_err(|_| action_error())?;
            let revision = AUDIO_STEP_REVISION
                .fetch_add(1, Ordering::Relaxed)
                .wrapping_add(1);
            LATEST_AUDIO_STEP_REVISION.store(revision, Ordering::Relaxed);
            super::osd::volume(&hub, value);
            if let Some(key_ms) = request.key_ms {
                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |elapsed| elapsed.as_millis() as u64);
                if step_marker_allowed(now_ms) {
                    tracing::info!("OSD_STEP kind=volume value={value:.3} ms_since_key={} key_ms={key_ms} timebase=unix_ms", now_ms.saturating_sub(key_ms));
                }
            }
            drop(state_guard);
            let guard = audio_lock().lock_owned().await;
            let latest = hub
                .borrow()
                .snapshot("audio")
                .and_then(|state| {
                    let output = state["default_output"].as_str()?.to_owned();
                    let value = state["nodes"]["items"].get(&output)?["volume"].as_f64()?;
                    Some((output, value))
                })
                .ok_or_else(unavailable)?;
            let result = crate::process::offload(move || {
                let _guard = guard;
                set_volume_value(latest.1)
            })
            .await
            .map_err(|_| action_error())?;
            if let Err(error) = result {
                rollback_audio_step(&hub, &output, revision, previous).await;
                return Err(error);
            }
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

async fn rollback_audio_step(hub: &Shared, output: &str, revision: u64, previous: f64) {
    if LATEST_AUDIO_STEP_REVISION.load(Ordering::Relaxed) != revision {
        return;
    }
    let lane = STEP_STATE_LOCK
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    let _guard = lane.lock_owned().await;
    if LATEST_AUDIO_STEP_REVISION.load(Ordering::Relaxed) != revision {
        return;
    }
    if let Some(mut state) = hub.borrow().snapshot("audio") {
        if state["default_output"].as_str() != Some(output) {
            return;
        }
        if let Some(node) = state["nodes"]["items"].get_mut(output) {
            node["volume"] = json!(previous);
            if let Some(readback) = node.get_mut("readback") {
                readback["linear_volume"] = json!(previous);
                readback["perceptual_volume"] = json!(previous.cbrt());
            }
            let _ = hub.borrow_mut().publish("audio", state);
            super::osd::volume(hub, previous);
        }
    }
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

fn set_volume_value(value: f64) -> Result<f64, Failure> {
    let text = format!("{:.3}", value.clamp(0.0, 1.0));
    run_action(
        "wpctl",
        &["set-volume", "-l", "1.0", "@DEFAULT_AUDIO_SINK@", &text],
        true,
    )?;
    Ok(value.clamp(0.0, 1.0))
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
