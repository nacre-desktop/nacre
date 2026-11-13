//! Status-pill actions restricted to the current user's default devices.
use super::{
    audio::{Action, Target},
    hub::{failure, Shared},
};
use nacre_protocol::{
    pill_controls::{BluetoothEnabled, PillEmpty, VolumeAdjust, VolumeDirection, VolumeStep},
    ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

static LAST_STEP_MARKER_MS: AtomicU64 = AtomicU64::new(0);

pub fn methods() -> BTreeMap<String, Method> {
    nacre_protocol::pill_controls::methods()
        .into_iter()
        .filter(|(name, _)| backend_present(name))
        .collect()
}

/// Advertisement only: the PipeWire socket for the audio methods, `bluetoothctl` plus the
/// system bus for Bluetooth. Whether the daemon's client is connected is checked per call.
fn backend_present(method: &str) -> bool {
    match method {
        "audio.adjust_volume" | "audio.toggle_mute" | "audio.step" => {
            std::env::var_os("XDG_RUNTIME_DIR").is_some_and(|runtime| {
                socket_present(&std::path::Path::new(&runtime).join("pipewire-0"))
            })
        }
        "bluetooth.set_enabled" => {
            executable("bluetoothctl")
                && socket_present(&std::path::Path::new("/run/dbus").join("system_bus_socket"))
        }
        _ => false,
    }
}

fn socket_present(path: &std::path::Path) -> bool {
    use std::os::unix::fs::FileTypeExt;
    std::fs::metadata(path).is_ok_and(|metadata| metadata.file_type().is_socket())
}

fn executable(program: &str) -> bool {
    super::control_center::executable(program)
}

fn audio(hub: &Shared) -> Result<super::audio::Handle, Failure> {
    hub.borrow()
        .audio
        .clone()
        .filter(super::audio::Handle::connected)
        .ok_or_else(unavailable)
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
            audio(&hub)?.step(request.delta, None).await?;
        }
        "audio.step" => {
            let request: VolumeStep = serde_json::from_value(params).map_err(|_| invalid())?;
            let delta = match request.direction {
                VolumeDirection::Up => 0.05,
                VolumeDirection::Down => -0.05,
            };
            audio(&hub)?.step(delta, request.key_ms).await?;
        }
        "audio.toggle_mute" => {
            let _: PillEmpty = serde_json::from_value(params).map_err(|_| invalid())?;
            audio(&hub)?
                .apply(Target::DefaultSink, Action::ToggleMute)
                .await?;
        }
        "bluetooth.set_enabled" => {
            let request: BluetoothEnabled =
                serde_json::from_value(params).map_err(|_| invalid())?;
            if !backend_present(method) {
                return Err(unavailable());
            }
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
/// also when the level did not change at 0 % or 100 %. The audio topic remains owned by the
/// PipeWire client: this step result precedes the server's echo of the write.
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

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct AudioStepOutcome {
    pub(crate) node_name: String,
    /// `node.description` (else `node.nick`), the label the PipeWire client reports.
    pub(crate) label: String,
    /// Linear level of the loudest channel, what `AudioNode.volume` reports.
    pub(crate) linear: f64,
    /// Perceptual (cubic) level the OSD and the sliders show.
    pub(crate) volume: f64,
    pub(crate) muted: bool,
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

    #[tokio::test(flavor = "current_thread")]
    async fn audio_pill_actions_go_to_the_pipewire_client() {
        let hub =
            super::super::hub::Hub::new("pill".into(), false, Duration::from_millis(1)).shared();
        assert_eq!(
            dispatch(hub.clone(), "audio.toggle_mute", json!({}))
                .await
                .unwrap_err()
                .code,
            ErrorCode::Unavailable,
            "no client, no action"
        );
        let (audio, commands) = super::super::audio::Handle::fake(|_| Ok(()));
        hub.borrow_mut().audio = Some(audio);
        dispatch(hub.clone(), "audio.adjust_volume", json!({"delta":0.2}))
            .await
            .unwrap();
        assert_eq!(
            dispatch(hub.clone(), "audio.adjust_volume", json!({"delta":0.0}))
                .await
                .unwrap(),
            json!({"changed":false})
        );
        dispatch(
            hub.clone(),
            "audio.step",
            json!({"direction":"down","key_ms":5}),
        )
        .await
        .unwrap();
        dispatch(hub.clone(), "audio.toggle_mute", json!({}))
            .await
            .unwrap();
        assert!(
            dispatch(hub.clone(), "audio.adjust_volume", json!({"delta":0.5}))
                .await
                .is_err()
        );
        assert_eq!(
            commands.lock().unwrap().as_slice(),
            ["step 0.2", "step -0.05", "toggle-mute sink"]
        );
    }

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
    fn step_at_the_limit_publishes_osd_without_overwriting_audio_readback() {
        let hub = super::super::hub::Hub::new("step-limit".into(), false, Duration::from_millis(1))
            .shared();
        hub.borrow_mut()
            .publish(
                "audio",
                json!({
                    "default_output": "alsa_output.pro-output-0",
                    "nodes": {
                        "items": {
                            "alsa_output.pro-output-0": {
                                "volume": 0.2,
                                "readback": {"linear_volume": 0.2}
                            }
                        },
                        "order": ["alsa_output.pro-output-0"]
                    }
                }),
            )
            .unwrap();
        let prior_audio = hub.borrow().snapshot("audio").unwrap();
        let mut notices = hub.borrow().observe();
        let outcome = AudioStepOutcome {
            node_name: "alsa_output.pro-output-0".into(),
            label: String::new(),
            linear: 1.0,
            volume: 1.0,
            muted: true,
        };
        publish_audio_step(&hub, &outcome, Some(1)).unwrap();
        let (mut seen, mut audio_changed) = (false, false);
        while let Ok(notice) = notices.try_recv() {
            match notice {
                super::super::hub::Notice::Event { topic, data, .. } => {
                    seen |= topic == "osd" && data["value"] == 1.0 && data["muted"] == true;
                }
                super::super::hub::Notice::Changed { topic, .. } => {
                    audio_changed |= topic == "audio";
                }
                super::super::hub::Notice::Resync(_) => {}
            }
        }
        assert!(seen, "a step at 100 % shows the pill");
        assert!(
            !audio_changed,
            "a cached key step never publishes audio state"
        );
        assert_eq!(hub.borrow().snapshot("audio").unwrap(), prior_audio);
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
