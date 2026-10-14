//! Assertions retained from the retired hosted-helper Python unit tests.
use nacre_helper::{
    dashboard_data, dashboard_media, launcher_preferences as prefs, notification_history as history,
};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf, process::Command};

// Run environment-dependent assertions in a child, rather than mutating the
// test runner's process-wide HOME while other tests are executing.
fn isolated(name: &str) -> bool {
    if std::env::var("NACRE_HOSTED_ASSERTION").as_deref() == Ok(name) {
        return false;
    }
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", name, "--nocapture"])
        .env("NACRE_HOSTED_ASSERTION", name)
        .env("HOME", home.path())
        .env("TZ", "UTC")
        .env(
            "DBUS_SESSION_BUS_ADDRESS",
            "unix:path=/nonexistent-parity-bus",
        )
        .env_remove("XDG_CONFIG_HOME")
        .env_remove("XDG_STATE_HOME")
        .env_remove("XDG_CACHE_HOME")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    true
}
fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap())
}
fn launcher() -> PathBuf {
    home().join("launcher.json")
}
fn policy() -> Value {
    serde_json::from_str(include_str!(
        "../../../nacre/shell-tools/notification-policy.json"
    ))
    .unwrap()
}

#[test]
fn favorites_and_hidden_updates_preserve_other_choices() {
    if isolated("favorites_and_hidden_updates_preserve_other_choices") {
        return;
    }
    let path = launcher();
    prefs::change(
        &path,
        "favorite",
        &json!({"id":"one.desktop","enabled":true}),
    )
    .unwrap();
    prefs::change(&path, "hide", &json!({"id":"two.desktop","enabled":true})).unwrap();
    prefs::change(
        &path,
        "favorite",
        &json!({"id":"one.desktop","enabled":true}),
    )
    .unwrap();
    assert_eq!(
        prefs::load(&path).unwrap(),
        json!({"favorites":["one.desktop"],"hidden":["two.desktop"],"recent":[],"usage":{}})
    );
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    prefs::change(&path, "hide", &json!({"id":"two.desktop","enabled":false})).unwrap();
    assert_eq!(prefs::load(&path).unwrap()["hidden"], json!([]));
}
#[test]
fn action_favorites_roundtrip_alongside_apps() {
    if isolated("action_favorites_roundtrip_alongside_apps") {
        return;
    }
    let path = launcher();
    let ids = [
        "music.desktop",
        "nacre-action:toggle-setting:blur",
        "nacre-action:toggle-setting:animations",
    ];
    for id in ids {
        prefs::change(&path, "favorite", &json!({"id":id,"enabled":true})).unwrap();
    }
    assert_eq!(prefs::load(&path).unwrap()["favorites"], json!(ids));
    prefs::change(&path, "favorite", &json!({"id":ids[1],"enabled":false})).unwrap();
    assert_eq!(
        prefs::load(&path).unwrap()["favorites"],
        json!([ids[0], ids[2]])
    );
}
#[test]
fn invalid_preference_is_rejected_without_writes() {
    if isolated("invalid_preference_is_rejected_without_writes") {
        return;
    }
    let path = launcher();
    assert!(prefs::change(&path, "favorite", &json!({"id":"bad\nname","enabled":true})).is_err());
    assert!(!path.exists());
}
#[test]
fn malformed_existing_preferences_are_preserved() {
    if isolated("malformed_existing_preferences_are_preserved") {
        return;
    }
    let path = launcher();
    let original = r#"{"favorites":42,"hidden":[]}"#;
    fs::write(&path, original).unwrap();
    assert!(prefs::change(&path, "favorite", &json!({"id":"one","enabled":true})).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), original);
}
#[test]
fn recent_ids_are_bounded_without_reordering_favorites() {
    if isolated("recent_ids_are_bounded_without_reordering_favorites") {
        return;
    }
    let path = launcher();
    prefs::change(
        &path,
        "favorite",
        &json!({"id":"kept.desktop","enabled":true}),
    )
    .unwrap();
    for index in 0..15 {
        prefs::change(
            &path,
            "record",
            &json!({"id":format!("nacre-menu:actions.item-{index}"),"enabled":true}),
        )
        .unwrap();
    }
    prefs::change(
        &path,
        "record",
        &json!({"id":"nacre-menu:actions.item-10","enabled":true}),
    )
    .unwrap();
    let data = prefs::load(&path).unwrap();
    assert_eq!(data["favorites"], json!(["kept.desktop"]));
    let recent = data["recent"].as_array().unwrap();
    assert_eq!(recent.len(), 12);
    assert_eq!(recent[0], "nacre-menu:actions.item-10");
    assert_eq!(
        recent
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect::<std::collections::HashSet<_>>()
            .len(),
        12
    );
    assert_eq!(data["usage"]["nacre-menu:actions.item-10"]["count"], 2);
    assert!(
        data["usage"]["nacre-menu:actions.item-10"]["last"]
            .as_f64()
            .unwrap()
            > 0.0
    );
}
#[test]
fn private_roundtrip_keeps_only_display_fields_and_clear_is_durable() {
    if isolated("private_roundtrip_keeps_only_display_fields_and_clear_is_durable") {
        return;
    }
    let path = home().join("notifications/history.json");
    history::save(&path, &json!([{"key":"fixture","time":"2026-10-03T21:00:00Z","summary":"Test","body":"Body","appName":"Fixture","actions":["stale native action"],"notification":5}]), &policy()).unwrap();
    let rows = history::load(&path, &policy()).unwrap();
    assert_eq!(rows[0]["body"], "Body");
    assert!(rows[0].get("actions").is_none());
    assert!(rows[0].get("notification").is_none());
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(path.parent().unwrap())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    history::save(&path, &json!([]), &policy()).unwrap();
    assert_eq!(history::load(&path, &policy()).unwrap(), json!([]));
}
#[test]
fn feedback_is_removed_but_messages_and_completion_are_kept() {
    if isolated("feedback_is_removed_but_messages_and_completion_are_kept") {
        return;
    }
    let path = home().join("history.json");
    let rows = json!([
        {"key":"shot","appName":"nacre_shell-cli","summary":"Screenshot saved"},
        {"key":"trash","appName":"notify-send","summary":"Window moved to trash"},
        {"key":"transient","appName":"Desktop","summary":"Volume","transient":true},
        {"key":"message","appName":"Discord","summary":"Screenshot saved"},
        {"key":"done","appName":"Nacre AI","summary":"Task complete"}
    ]);
    fs::write(&path, serde_json::to_vec(&rows).unwrap()).unwrap();
    for save in [false, true] {
        if save {
            history::save(&path, &rows, &policy()).unwrap();
        }
        assert_eq!(
            history::load(&path, &policy())
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["key"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["message", "done"]
        );
    }
}
#[test]
fn invalid_input_cannot_overwrite_existing_history() {
    if isolated("invalid_input_cannot_overwrite_existing_history") {
        return;
    }
    let path = home().join("notifications/history.json");
    history::save(&path, &json!([{"key":"saved","summary":"Kept"}]), &policy()).unwrap();
    let before = fs::read(&path).unwrap();
    for bad in [json!({}), json!([null])] {
        assert!(history::save(&path, &bad, &policy()).is_err());
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}
#[test]
fn tabs_and_preferences_reject_invalid_values() {
    if isolated("tabs_and_preferences_reject_invalid_values") {
        return;
    }
    assert_eq!(
        dashboard_data::validate(&json!({"tabs":["focus","today"]})).unwrap()["tabs"],
        json!(["focus", "today"])
    );
    for value in [
        json!([]),
        json!(["today", "today"]),
        json!(["unknown"]),
        json!([{}]),
    ] {
        assert!(dashboard_data::validate(&json!({"tabs":value})).is_err());
    }
    for value in [json!(0), json!(181), json!(true), json!("25")] {
        assert!(dashboard_data::validate(&json!({"focusMinutes":value})).is_err());
    }
    assert!(dashboard_data::validate(&json!({"calendarFile":"relative.ics"})).is_err());
    assert!(dashboard_data::validate(&json!({"homeZone":"bad/zone"})).is_err());
}
#[test]
fn private_writes_and_focus_deadline_survive_pause_and_resume() {
    if isolated("private_writes_and_focus_deadline_survive_pause_and_resume") {
        return;
    }
    let action =
        |kind, clock| dashboard_data::focus_action(kind, &json!(25), &json!(false), clock).unwrap();
    let state = action("start", 100.0);
    assert_eq!(state["deadline"], json!(1600.0));
    assert_eq!(state["ownsDnd"], true);
    assert_eq!(action("pause", 400.0)["remaining"], json!(1200.0));
    assert_eq!(action("resume", 900.0)["deadline"], json!(2100.0));
    assert_eq!(action("release", 1000.0)["ownsDnd"], false);
    let state = action("complete", 2200.0);
    assert_eq!(state["status"], "done");
    assert_eq!(state["ownsDnd"], false);
    let path = home().join(".local/state/nacre/dashboard-focus.json");
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(fs::read_dir(path.parent().unwrap())
        .unwrap()
        .all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".dashboard-focus.json")));
}
#[test]
fn local_calendar_unfolds_titles_dates_and_excludes_cancelled() {
    if isolated("local_calendar_unfolds_titles_dates_and_excludes_cancelled") {
        return;
    }
    let path = home().join("events.ics");
    fs::write(&path,"BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:20991011T100000Z\nSUMMARY:Project\\,\n review\nEND:VEVENT\nBEGIN:VEVENT\nDTSTART:20991011T120000Z\nSTATUS:CANCELLED\nSUMMARY:Cancelled\nEND:VEVENT\nEND:VCALENDAR").unwrap();
    let now = chrono::DateTime::parse_from_rfc3339("2099-10-11T00:00:00Z")
        .unwrap()
        .with_timezone(&chrono::Utc);
    let result = dashboard_data::calendar(path.to_str().unwrap(), now);
    assert_eq!(result["events"].as_array().unwrap().len(), 1);
    assert_eq!(result["events"][0]["title"], "Project,review");
    fs::write(&path, "x".repeat(2_000_001)).unwrap();
    assert!(
        dashboard_data::calendar(path.to_str().unwrap(), now)["error"]
            .as_str()
            .unwrap()
            .contains("2MB")
    );
}
#[test]
fn lrc_orders_multiple_timestamps_and_ignores_tags() {
    let result = dashboard_media::lyrics(&json!(
        "[ar:artist]\n[00:02.50][00:10.000]Two\n[00:01.1]One"
    ));
    assert_eq!(
        result
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["time"].as_f64().unwrap())
            .collect::<Vec<_>>(),
        [1.1, 2.5, 10.0]
    );
    assert_eq!(result[1]["text"], "Two");
}
#[test]
fn cached_media_does_not_fetch_or_publish() {
    if isolated("cached_media_does_not_fetch_or_publish") {
        return;
    }
    let meta = json!({"title":"Test","artist":"Test","lyrics":false,"palette":false});
    assert_eq!(
        dashboard_media::track_data(&meta).unwrap(),
        dashboard_media::track_data(&meta).unwrap()
    );
    let cache = home().join(".cache/nacre/dashboard-media");
    assert_eq!(
        fs::read_dir(cache)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|v| v == "json"))
            .count(),
        1
    );
}
