//! N0063: the `displays` topic and `display.apply|confirm|revert` for the Bead Settings Displays
//! page. No display logic lives here: a change runs through the desktop settings controller the
//! QML page used (`settings.desktop` with `display`, `display-edit`, `confirm`, `revert`), which
//! journals the previous layout, arms the independent systemd rollback unit, applies the layout
//! through the compositor backend (niri IPC `Output` actions, Hyprland `hyprctl eval`) and then
//! persists it (`desktop.json` plus the compositor's monitor configuration file). This module
//! maps typed requests onto those arguments, turns the controller's answers into protocol
//! errors and publishes what it reports. It never spawns a process and never starts at READY.
use super::{
    behaviours::changed,
    hub::{failure, Hub, Shared},
};
use crate::Result;
use nacre_protocol::{
    core_services::Health,
    displays::{
        mode_valid, output_name_valid, DisplayApply, DisplayArrangement, DisplayOutput,
        DisplayPending, DisplaysState, CONFIRM_SECONDS, MAX_MODES, MAX_OUTPUTS, SCALES, TOPIC,
    },
    pill_controls::PillEmpty,
    ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

pub fn handles(method: &str) -> bool {
    matches!(
        method,
        "display.apply" | "display.confirm" | "display.revert"
    )
}
/// The compositor answers and can configure outputs (QML: `monitorConfiguration` is not false).
fn configurable(hub: &Hub) -> bool {
    !hub.safe
        && hub
            .services
            .get("settings")
            .is_some_and(|service| service["status"] == "running")
        && hub.snapshot_ref("compositor").is_some_and(|compositor| {
            compositor["available"] == true
                && compositor["capabilities"]["monitorConfiguration"] != false
        })
}
pub fn available_methods(hub: &Hub) -> BTreeMap<String, Method> {
    nacre_protocol::displays::methods()
        .into_iter()
        .filter(|_| configurable(hub))
        .collect()
}

fn unavailable() -> Failure {
    failure(
        ErrorCode::Unavailable,
        "Display settings are unavailable",
        true,
    )
    .dispatch_state(false)
}
fn invalid() -> Failure {
    failure(ErrorCode::Validation, "Invalid display request", false).dispatch_state(false)
}
/// The controller reports a refusal as a fixed English sentence (the QML page showed it as is).
/// Known ones keep their text; anything else becomes one generic message, so no path or system
/// error text reaches a client. Only the arming and compositor steps can have touched an output,
/// and the controller reverts those itself.
fn map_error(message: &str) -> Failure {
    let (code, text, retry, dispatched) = match message {
        "Keep or revert the current display change first" => {
            (ErrorCode::Busy, message, false, false)
        }
        "Automatic display rollback could not be armed" => {
            (ErrorCode::Unavailable, message, true, false)
        }
        "Selected display is disconnected"
        | "Unavailable display mode"
        | "Unsupported mode format"
        | "Unsupported display scale"
        | "Unsupported display arrangement"
        | "Connect a second display first"
        | "Output mirroring is unavailable on this compositor" => {
            (ErrorCode::Validation, message, false, false)
        }
        _ => (
            ErrorCode::Io,
            "The display change failed and was reverted",
            false,
            true,
        ),
    };
    failure(code, text, retry).dispatch_state(dispatched)
}

/// The controller arguments of one request (see `desktop_settings::Settings::action`).
fn arguments(method: &str, params: Value) -> std::result::Result<Vec<String>, Failure> {
    match method {
        "display.apply" => {
            let request: DisplayApply = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            Ok(match (request.arrangement, request.scale) {
                (Some(arrangement), _) => vec![
                    "display".into(),
                    match arrangement {
                        DisplayArrangement::ExtendRight => "extend-right",
                        DisplayArrangement::ExtendLeft => "extend-left",
                        DisplayArrangement::Mirror => "mirror",
                    }
                    .into(),
                    request.output,
                ],
                (None, scale) => vec![
                    "display-edit".into(),
                    request.output,
                    scale.map(|scale| scale.to_string()).unwrap_or_default(),
                    request.mode.unwrap_or_default(),
                ],
            })
        }
        "display.confirm" | "display.revert" => {
            serde_json::from_value::<PillEmpty>(params).map_err(|_| invalid())?;
            Ok(vec![method.trim_start_matches("display.").into()])
        }
        _ => Err(failure(ErrorCode::UnknownMethod, "Unknown method", false)),
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}
/// A change is pending while the controller's journal exists; the armed rollback fires
/// `CONFIRM_SECONDS` after the journal was written, so its mtime gives the deadline (and survives
/// a daemon restart in between).
fn pending_at(journal: &Path, now: u64) -> Option<DisplayPending> {
    let metadata = std::fs::metadata(journal).ok()?;
    let written = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(now, |elapsed| elapsed.as_millis() as u64);
    Some(DisplayPending {
        seconds: CONFIRM_SECONDS,
        deadline_ms: written + u64::from(CONFIRM_SECONDS) * 1000,
    })
}
fn pending(hub: &Hub) -> Option<DisplayPending> {
    let niri = hub.snapshot_ref("compositor")?["backend"] == "niri";
    let config = &hub.settings.as_ref()?.paths.config;
    pending_at(
        &config.join(if niri {
            "display-pending-niri.json"
        } else {
            "display-pending.json"
        }),
        now_ms(),
    )
}

/// `WIDTHxHEIGHT@RATEHz` to its numbers.
fn parse_mode(mode: &str) -> Option<(u64, u64, f64)> {
    let (size, rate) = mode.split_once('@')?;
    let (width, height) = size.split_once('x')?;
    Some((
        width.parse().ok()?,
        height.parse().ok()?,
        rate.strip_suffix("Hz")?.parse().ok()?,
    ))
}
fn text(value: &Value) -> Option<String> {
    let text: String = value
        .as_str()?
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    (!text.trim().is_empty()).then_some(text)
}
/// One output from a controller `monitors` row (or a compositor topic row), with the facts only
/// the compositor topic carries (`extra`: make, model, vrr).
fn output(row: &Value, extra: Option<&Value>) -> Option<DisplayOutput> {
    let name = row["name"].as_str().filter(|n| output_name_valid(n))?;
    let number = |key: &str| row[key].as_f64().filter(|n| n.is_finite());
    let (width, height) = (number("width")?.round(), number("height")?.round());
    let refresh_hz = number("refreshRate").unwrap_or(0.0);
    let modes: Vec<String> = row["availableModes"]
        .as_array()
        .map(|modes| {
            modes
                .iter()
                .filter_map(Value::as_str)
                .filter(|mode| mode_valid(mode))
                .take(MAX_MODES)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default();
    // QML: the entry that starts with "WxH@" and whose rate is within 0.01 of the current one.
    let mode = modes
        .iter()
        .find(|mode| {
            parse_mode(mode).is_some_and(|(w, h, rate)| {
                w as f64 == width && h as f64 == height && (rate - refresh_hz).abs() < 0.01
            })
        })
        .cloned();
    Some(DisplayOutput {
        name: name.to_owned(),
        make: extra.and_then(|e| text(&e["make"])),
        model: extra.and_then(|e| text(&e["model"])),
        enabled: row["disabled"] != true,
        width: width.clamp(0.0, f64::from(u32::MAX)) as u32,
        height: height.clamp(0.0, f64::from(u32::MAX)) as u32,
        refresh_hz,
        mode,
        modes,
        scale: number("scale").unwrap_or(1.0),
        x: number("x").unwrap_or(0.0).round() as i32,
        y: number("y").unwrap_or(0.0).round() as i32,
        transform: number("transform").map_or(0, |t| t.clamp(0.0, 7.0) as u8),
        vrr: extra.and_then(|e| e["vrr"].as_bool()),
    })
}

/// Outputs as the desktop controller last reported them (live), else as the compositor topic
/// lists them, in the compositor's order.
fn build(hub: &Hub) -> DisplaysState {
    let compositor = hub.snapshot_ref("compositor").unwrap_or(&Value::Null);
    let controller = hub.snapshot_ref("settings.desktop").unwrap_or(&Value::Null);
    let extras = &compositor["outputs"];
    let rows: Vec<Value> = if controller["available"] == true {
        controller["monitors"]
            .as_array()
            .cloned()
            .unwrap_or_default()
    } else {
        compositor["outputOrder"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|key| key.as_str().map(|key| extras[key].clone()))
            .collect()
    };
    let outputs = rows
        .iter()
        .filter_map(|row| output(row, row["name"].as_str().map(|name| &extras[name])))
        .take(MAX_OUTPUTS)
        .collect();
    let available = compositor["available"] == true;
    let capabilities = &compositor["capabilities"];
    DisplaysState {
        health: Health {
            available,
            stale: !available && compositor["backend"].is_string(),
            error: (!available).then(unavailable),
        },
        can_configure: available && capabilities["monitorConfiguration"] != false,
        can_mirror: available && capabilities["monitorMirroring"] != false,
        scales: SCALES.to_vec(),
        outputs,
        pending: pending(hub),
    }
}
fn publish(hub: &Shared) -> Result<()> {
    let state = build(&hub.borrow());
    debug_assert!(state.validate().is_ok());
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(&state)?)?;
    Ok(())
}

/// A first value, so a client that subscribes at once finds the topic.
pub fn initial(hub: &Shared) -> Result<()> {
    publish(hub)
}

/// The compositor outputs that matter to this page; a change means the controller's live answer
/// is out of date (hotplug, a change made elsewhere).
fn signature(hub: &Hub) -> Option<String> {
    let compositor = hub.snapshot_ref("compositor")?;
    if compositor["available"] != true {
        return None;
    }
    let rows: Vec<Value> = compositor["outputOrder"]
        .as_array()?
        .iter()
        .filter_map(|key| {
            let row = &compositor["outputs"][key.as_str()?];
            Some(json!([
                row["name"],
                row["width"],
                row["height"],
                row["scale"],
                row["refreshRate"],
                row["x"],
                row["y"],
                row["transform"]
            ]))
        })
        .collect();
    Some(Value::Array(rows).to_string())
}
/// Ask the controller for the live layout (`state` changes nothing); it publishes `settings.desktop`.
async fn refresh(hub: &Shared) {
    if super::settings::dispatch(
        hub.clone(),
        "settings.desktop",
        json!({"arguments":["state"]}),
    )
    .await
    .is_err()
    {
        tracing::debug!("Display layout refresh failed");
    }
}
pub async fn run(hub: Shared) -> Result<()> {
    let mut notices = hub.borrow().observe();
    hub.borrow_mut().service("displays", "running", 0)?;
    let mut seen = None;
    loop {
        let current = signature(&hub.borrow());
        if current.is_some() && current != seen {
            seen = current;
            refresh(&hub).await;
        }
        publish(&hub)?;
        changed(&mut notices, &["compositor", "settings.desktop"]).await?;
    }
}

/// One controller call under the settings owner; a revision that moved meanwhile is retried once.
async fn controller(hub: &Shared, arguments: Vec<String>) -> std::result::Result<Value, Failure> {
    let mut last = None;
    for _ in 0..2 {
        let store =
            super::settings::dispatch(hub.clone(), "settings.get", json!({"store":"desktop"}))
                .await?;
        match super::settings::dispatch(
            hub.clone(),
            "settings.desktop",
            json!({"arguments":arguments,"rev":store["rev"]}),
        )
        .await
        {
            Err(error) if error.code == ErrorCode::StaleRevision => last = Some(error),
            other => return other,
        }
    }
    Err(last.unwrap_or_else(unavailable))
}
pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    if !configurable(&hub.borrow()) {
        return Err(unavailable());
    }
    let arguments = arguments(method, params)?;
    let before = pending(&hub.borrow());
    if method != "display.apply" && before.is_none() {
        return Ok(json!({"changed":false}));
    }
    let result = controller(&hub, arguments).await?;
    if let Some(message) = result["error"].as_str() {
        publish(&hub).ok();
        return Err(map_error(message));
    }
    publish(&hub).ok();
    if method != "display.apply" {
        return Ok(json!({"changed":true}));
    }
    let pending = pending(&hub.borrow()).ok_or_else(|| {
        failure(
            ErrorCode::Internal,
            "The display change was not left pending",
            false,
        )
        .dispatch_state(true)
    })?;
    Ok(json!({"pending":pending}))
}

#[cfg(test)]
mod tests;
