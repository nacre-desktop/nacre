//! Canned collector documents; nothing is read from the machine.
use super::*;
use serde_json::json;

fn state() -> Value {
    json!({
        "checked":"2026-10-23T18:00:00.000000+00:00",
        "release":{"revision":"0123456789abcdef0123456789abcdef01234567","extra":1},
        "services":{"nacre-shell.service":"active","xdg-desktop-portal.service":"ready on demand","n.service":7},
        "serviceDetails":{"nacre-shell.service":{"Id":"secret detail"}},
        "failedServices":["foo.service loaded failed failed Foo","bar.service"],
        "drift":[".config/niri/config.kdl"],
        "configErrors":"",
        "brainSync":{"state":"ok","message":"Synced a minute ago"},
        "performance":{"label":"manual","cpuCorePercent":1.25,"memoryMiB":21.5},
        "session":{"event":"login"},
        "updatesAgeSeconds":3600,
        "versions":{"Python":"Python 3.13"}
    })
}
fn policy() -> Value {
    json!({"mode":"required","provenance":{"source":"general-profile-default","store":"profile","key":"aurReview","profile":"default"}})
}

#[test]
fn the_projection_carries_what_the_page_shows_and_nothing_more() {
    let status = project(&state(), Some(&policy()));
    status.validate().unwrap();
    assert_eq!(
        status.release.as_deref(),
        Some("0123456789abcdef0123456789abcdef01234567")
    );
    assert_eq!(status.services.len(), 2, "a non-text state is dropped");
    assert_eq!(
        status.services["xdg-desktop-portal.service"],
        "ready on demand"
    );
    assert_eq!(status.failed_services.len(), 2);
    assert_eq!(status.drift, [".config/niri/config.kdl"]);
    assert_eq!(status.config_errors, "");
    assert_eq!(status.brain_sync.as_deref(), Some("Synced a minute ago"));
    let sample = status.performance.clone().unwrap();
    assert_eq!((sample.cpu_core_percent, sample.memory_mib), (1.25, 21.5));
    assert_eq!(status.session_event.as_deref(), Some("login"));
    assert_eq!(status.updates_age_seconds, Some(3600));
    let aur = status.aur_review.clone().unwrap();
    assert_eq!(
        (aur.mode.as_str(), aur.source.as_str(), aur.profile.as_str()),
        ("required", "general-profile-default", "default")
    );
    assert!(!serde_json::to_string(&status)
        .unwrap()
        .contains("secret detail"));
}

#[test]
fn missing_or_hostile_documents_give_a_bounded_status() {
    let empty = project(&json!({}), None);
    empty.validate().unwrap();
    assert!(empty.release.is_none() && empty.performance.is_none() && empty.aur_review.is_none());
    assert_eq!(empty.updates_age_seconds, None, "no cache is not age zero");
    let hostile = json!({
        "checked":"x\u{1b}[31m".repeat(100),
        "services":(0..100).map(|n| (format!("u{n}.service"), json!("active\nfailed"))).collect::<serde_json::Map<_,_>>(),
        "failedServices":(0..200).map(|n| format!("unit{n}")).collect::<Vec<_>>(),
        "drift":(0..400).map(|n| format!("f{n}\n")).collect::<Vec<_>>(),
        "configErrors":"e\n".repeat(5000),
        "performance":{"cpuCorePercent":-5.0,"memoryMiB":-1.0},
        "updatesAgeSeconds":-30
    });
    let status = project(&hostile, None);
    status.validate().unwrap();
    assert_eq!(status.checked.chars().count(), 64);
    assert_eq!(status.services.len(), 32);
    assert!(status.services.values().all(|s| !s.contains('\n')));
    assert_eq!(
        (status.failed_services.len(), status.drift.len()),
        (64, 256)
    );
    assert!(status.config_errors.chars().count() <= 2048);
    let sample = status.performance.unwrap();
    assert_eq!((sample.cpu_core_percent, sample.memory_mib), (0.0, 0.0));
    assert_eq!(status.updates_age_seconds, Some(0));
}

#[test]
fn the_method_is_read_only_and_off_in_safe_mode() {
    let mut hub = Hub::new("status".into(), false, Duration::from_millis(1));
    assert!(
        available_methods(&hub).is_empty(),
        "not before the Settings services run"
    );
    hub.service("capabilities", "running", 0).unwrap();
    assert!(available_methods(&hub).contains_key("system.status"));
    hub.safe = true;
    assert!(available_methods(&hub).is_empty());
}
