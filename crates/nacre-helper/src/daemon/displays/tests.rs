//! Pure mapping tests; the controller, the compositor fakes and the rollback unit are exercised
//! end to end in `tests/displays.rs`.
use super::*;
use std::time::Duration;

fn hub() -> Shared {
    Hub::new("displays".into(), false, Duration::from_millis(1)).shared()
}
fn niri_row(name: &str, x: i64) -> Value {
    json!({"id":name,"name":name,"x":x,"y":0,"width":2560,"height":1440,"scale":1.25,
        "transform":1,"refreshRate":59.951,"make":"Dell Inc.","model":"U2723QE","vrr":true,
        "availableModes":["2560x1440@59.951Hz","1920x1080@60.000Hz","bad mode"]})
}
fn compositor(rows: &[Value], caps: Value) -> Value {
    let outputs: serde_json::Map<String, Value> = rows
        .iter()
        .map(|row| (row["name"].as_str().unwrap().to_owned(), row.clone()))
        .collect();
    let order: Vec<Value> = rows.iter().map(|row| row["name"].clone()).collect();
    json!({"backend":"niri","available":true,"outputs":outputs,"outputOrder":order,"capabilities":caps})
}

#[test]
fn a_niri_row_becomes_an_output_with_its_current_mode_resolved() {
    let row = niri_row("DP-1", 1920);
    let out = output(&row, Some(&row)).unwrap();
    assert_eq!(
        (out.width, out.height, out.x, out.y, out.transform),
        (2560, 1440, 1920, 0, 1)
    );
    assert_eq!(out.mode.as_deref(), Some("2560x1440@59.951Hz"));
    // A mode that is not WxH@RHz never reaches a client.
    assert_eq!(out.modes.len(), 2);
    assert_eq!(
        (out.make.as_deref(), out.model.as_deref(), out.vrr),
        (Some("Dell Inc."), Some("U2723QE"), Some(true))
    );
    assert!(out.enabled && out.scale == 1.25);
}
#[test]
fn a_hyprland_row_keeps_disabled_and_missing_facts_honest() {
    let row = json!({"name":"eDP-1","width":1920,"height":1200,"refreshRate":60.0,"scale":1.0,
        "x":0,"y":0,"transform":0,"disabled":true,"make":"\u{7}BOE\n","model":"  ",
        "availableModes":["1920x1200@60.00Hz","1920x1200@48.00Hz"]});
    let out = output(&row, Some(&row)).unwrap();
    assert!(!out.enabled);
    assert_eq!(out.mode.as_deref(), Some("1920x1200@60.00Hz"));
    assert_eq!(
        (out.make.as_deref(), out.model, out.vrr),
        (Some("BOE"), None, None)
    );
    // No current mode in the list: no `mode`, never a guess.
    let mut odd = row.clone();
    odd["refreshRate"] = json!(75.0);
    assert_eq!(output(&odd, None).unwrap().mode, None);
}
#[test]
fn rows_without_a_usable_name_or_size_are_dropped() {
    assert!(output(&json!({"name":"","width":1,"height":1}), None).is_none());
    assert!(output(&json!({"name":"a\nb","width":1,"height":1}), None).is_none());
    assert!(output(&json!({"name":"DP-1","height":1}), None).is_none());
    let long: Vec<String> = (0..400).map(|i| format!("1920x1080@{i}.000Hz")).collect();
    let row = json!({"name":"DP-1","width":1920,"height":1080,"availableModes":long});
    assert_eq!(output(&row, None).unwrap().modes.len(), MAX_MODES);
}

#[test]
fn the_controllers_live_answer_wins_over_the_compositor_topic() {
    let hub = hub();
    let caps = json!({"monitorConfiguration":true,"monitorMirroring":false});
    hub.borrow_mut()
        .publish(
            "compositor",
            compositor(&[niri_row("DP-1", 0)], caps.clone()),
        )
        .unwrap();
    let state = build(&hub.borrow());
    assert!(state.health.available && state.can_configure && !state.can_mirror);
    assert_eq!(state.scales, SCALES.to_vec());
    assert_eq!(state.outputs.len(), 1);
    assert_eq!(state.outputs[0].scale, 1.25);
    // The controller's `monitors` (what `state` reports after a change) replace the rows, and
    // still pick make/model up from the compositor topic by name.
    let live = json!({"name":"DP-1","width":1920,"height":1080,"refreshRate":60.0,"scale":2.0,
        "x":0,"y":0,"transform":0,"availableModes":["1920x1080@60.000Hz"]});
    hub.borrow_mut()
        .publish(
            "settings.desktop",
            json!({"available":true,"monitors":[live],"pending":false}),
        )
        .unwrap();
    let state = build(&hub.borrow());
    assert_eq!(state.outputs[0].scale, 2.0);
    assert_eq!(state.outputs[0].model.as_deref(), Some("U2723QE"));
    assert!(state.validate().is_ok());
    // A compositor that is down keeps the list, visibly stale.
    let mut down = compositor(&[niri_row("DP-1", 0)], caps);
    down["available"] = json!(false);
    hub.borrow_mut().publish("compositor", down).unwrap();
    let state = build(&hub.borrow());
    assert!(!state.health.available && state.health.stale && !state.can_configure);
    assert_eq!(state.outputs.len(), 1);
}
#[test]
fn methods_are_advertised_only_with_a_configurable_running_compositor() {
    let hub = hub();
    assert!(available_methods(&hub.borrow()).is_empty());
    hub.borrow_mut().service("settings", "running", 0).unwrap();
    let publish = |caps: Value| {
        hub.borrow_mut()
            .publish("compositor", compositor(&[niri_row("DP-1", 0)], caps))
            .unwrap();
    };
    publish(json!({"monitorConfiguration":false}));
    assert!(available_methods(&hub.borrow()).is_empty());
    publish(json!({"monitorConfiguration":true}));
    assert_eq!(available_methods(&hub.borrow()).len(), 3);
    // Hyprland lists no `monitorConfiguration` flag and does configure.
    publish(json!({}));
    assert_eq!(available_methods(&hub.borrow()).len(), 3);
    hub.borrow_mut().safe = true;
    assert!(available_methods(&hub.borrow()).is_empty());
}
#[test]
fn the_signature_follows_outputs_and_ignores_windows() {
    let hub = hub();
    let caps = json!({"monitorConfiguration":true});
    let publish = |rows: &[Value], extra: Value| {
        let mut state = compositor(rows, caps.clone());
        state["windows"] = extra;
        hub.borrow_mut().publish("compositor", state).unwrap();
        signature(&hub.borrow())
    };
    let first = publish(&[niri_row("DP-1", 0)], json!({}));
    assert_eq!(first, publish(&[niri_row("DP-1", 0)], json!({"w":{}})));
    assert_ne!(first, publish(&[niri_row("DP-1", 10)], json!({})));
    assert_ne!(
        first,
        publish(&[niri_row("DP-1", 0), niri_row("DP-2", 2560)], json!({}))
    );
}

#[test]
fn requests_map_onto_the_controller_arguments_the_qml_page_used() {
    let args = |method: &str, params: Value| arguments(method, params).unwrap();
    assert_eq!(
        args(
            "display.apply",
            json!({"output":"DP-1","arrangement":"extend-right"})
        ),
        ["display", "extend-right", "DP-1"]
    );
    assert_eq!(
        args(
            "display.apply",
            json!({"output":"DP-1","arrangement":"mirror"})
        ),
        ["display", "mirror", "DP-1"]
    );
    assert_eq!(
        args("display.apply", json!({"output":"DP-2","scale":1.25})),
        ["display-edit", "DP-2", "1.25", ""]
    );
    assert_eq!(
        args(
            "display.apply",
            json!({"output":"DP-2","scale":2,"mode":"1920x1080@60.000Hz"})
        ),
        ["display-edit", "DP-2", "2", "1920x1080@60.000Hz"]
    );
    assert_eq!(args("display.confirm", json!({})), ["confirm"]);
    assert_eq!(args("display.revert", json!({})), ["revert"]);
    for (method, params) in [
        ("display.apply", json!({"output":"DP-1","scale":1.1})),
        ("display.apply", json!({"output":"DP-1"})),
        (
            "display.apply",
            json!({"output":"DP-1","scale":1,"mode":"x"}),
        ),
        ("display.confirm", json!({"token":"x"})),
    ] {
        let error = arguments(method, params).unwrap_err();
        assert_eq!(
            (error.code, error.dispatched),
            (ErrorCode::Validation, Some(false))
        );
    }
    assert_eq!(
        arguments("display.nope", json!({})).unwrap_err().code,
        ErrorCode::UnknownMethod
    );
}
#[test]
fn controller_refusals_keep_their_sentence_and_unknown_errors_stay_generic() {
    let busy = map_error("Keep or revert the current display change first");
    assert_eq!((busy.code, busy.dispatched), (ErrorCode::Busy, Some(false)));
    let unarmed = map_error("Automatic display rollback could not be armed");
    assert_eq!(
        (unarmed.code, unarmed.retry),
        (ErrorCode::Unavailable, true)
    );
    for known in [
        "Selected display is disconnected",
        "Unavailable display mode",
        "Unsupported display scale",
        "Connect a second display first",
        "Output mirroring is unavailable on this compositor",
    ] {
        let error = map_error(known);
        assert_eq!(
            (error.code, error.message.as_str()),
            (ErrorCode::Validation, known)
        );
    }
    let other = map_error("No such file or directory (os error 2) /home/someone/.config");
    assert_eq!(other.code, ErrorCode::Io);
    assert!(!other.message.contains("/home"));
    assert_eq!(other.dispatched, Some(true));
}
#[test]
fn the_deadline_is_twenty_seconds_after_the_journal_was_written() {
    let dir = tempfile::tempdir().unwrap();
    let journal = dir.path().join("display-pending.json");
    assert_eq!(pending_at(&journal, 5), None);
    std::fs::write(&journal, "{}").unwrap();
    let written = std::fs::metadata(&journal)
        .unwrap()
        .modified()
        .unwrap()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64;
    assert_eq!(
        pending_at(&journal, 5),
        Some(DisplayPending {
            seconds: 20,
            deadline_ms: written + 20_000
        })
    );
}
