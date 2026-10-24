//! Local ICS projection. All filesystem work is offloaded, never part of READY.
use super::{
    dashboard_extras::watch::Watch,
    hub::{failure, Notice, Shared},
};
use crate::{dashboard_data, paths::Paths, Result};
use nacre_protocol::{
    core_services::{CalendarEvent, CalendarState, Entities, EntityId, Health},
    Capability, ErrorCode, Failure, Method,
};
use rustix::fs::inotify::WatchFlags;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, rc::Rc, time::Duration};
use tokio::sync::Notify;

pub const TOPIC: &str = "dashboard.calendar";
const SETTINGS: &str = "settings.store.dashboard";

pub fn methods() -> BTreeMap<String, Method> {
    BTreeMap::from([(
        "dashboard.calendar.refresh".into(),
        Method {
            v: 1,
            class: Capability::Desktop,
            timeout: 5000,
            params: json!({"type":"object","additionalProperties":false}),
            result: serde_json::from_str(include_str!(
                "../../../../protocol/schema/core-service-result.json"
            ))
            .expect("core result schema"),
        },
    )])
}
pub fn initial(hub: &Shared) -> Result<()> {
    publish(
        hub,
        project(&json!({"configured":false,"events":[],"error":""}), false),
    )
}
fn publish(hub: &Shared, state: CalendarState) -> Result<()> {
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}
fn project(data: &Value, loaded: bool) -> CalendarState {
    let mut events = Entities {
        items: BTreeMap::new(),
        order: vec![],
    };
    for row in data["events"].as_array().into_iter().flatten().take(8) {
        let Some(start) = row["start"].as_f64().filter(|n| *n >= 0.0) else {
            continue;
        };
        let mut id = EntityId(format!("e{:x}", Sha256::digest(row.to_string().as_bytes())));
        let base = id.0.clone();
        let mut occurrence = 1;
        while events.items.contains_key(&id) {
            id = EntityId(format!("{base}:{occurrence}"));
            occurrence += 1;
        }
        events.order.push(id.clone());
        events.items.insert(
            id,
            CalendarEvent {
                title: row["title"].as_str().unwrap_or("").into(),
                start: start as u64,
                time: row["time"].as_str().unwrap_or("").into(),
                date: row["date"].as_str().unwrap_or("").into(),
            },
        );
    }
    let error = data["error"].as_str().unwrap_or("");
    CalendarState {
        health: Health {
            available: loaded && (error.is_empty() || error.starts_with("Recurring events")),
            stale: false,
            error: (!error.is_empty()).then(|| failure(ErrorCode::Unavailable, error, true)),
        },
        configured: data["configured"].as_bool().unwrap_or(false),
        events,
    }
}
fn source(hub: &Shared) -> Option<String> {
    let settings = hub.borrow().snapshot(SETTINGS)?;
    if !settings["error"].is_null() {
        return None;
    }
    Some(
        settings["data"]["calendarFile"]
            .as_str()
            .unwrap_or("")
            .into(),
    )
}
pub fn dispatch(hub: Shared, params: Value) -> std::result::Result<Value, Failure> {
    if params.as_object().is_none_or(|p| !p.is_empty()) {
        return Err(failure(
            ErrorCode::Validation,
            "Expected an empty refresh request",
            false,
        ));
    }
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::PermissionDenied,
            "Calendar is disabled in safe mode",
            false,
        ));
    }
    let wake = hub
        .borrow()
        .calendar
        .clone()
        .ok_or_else(|| failure(ErrorCode::Unavailable, "Calendar is unavailable", true))?;
    wake.notify_one();
    Ok(json!({"changed":false}))
}
fn load(paths: Paths, path: String) -> (Value, Option<Watch>, Vec<std::ffi::OsString>) {
    let source = dashboard_data::calendar_path(&path, &paths).ok();
    let mut dirs = vec![];
    let mut names = vec![];
    // Parent watches catch atomic replacement. The target's parent also catches
    // updates to a calendar reached through a symlink.
    for file in [source.clone(), source.and_then(|p| p.canonicalize().ok())]
        .into_iter()
        .flatten()
    {
        if let Some(name) = file.file_name() {
            names.push(name.to_owned());
        }
        if let Some(parent) = file.parent().filter(|p| p.is_absolute()) {
            let flags = WatchFlags::CLOSE_WRITE
                | WatchFlags::MOVED_TO
                | WatchFlags::MOVED_FROM
                | WatchFlags::CREATE
                | WatchFlags::DELETE
                | WatchFlags::DELETE_SELF
                | WatchFlags::MOVE_SELF;
            if !dirs.iter().any(|(p, _)| p == parent) {
                dirs.push((parent.to_owned(), flags));
            }
        }
    }
    let watch = Watch::new(dirs).ok();
    (
        dashboard_data::calendar_at(&path, chrono::Utc::now(), &paths),
        watch,
        names,
    )
}
pub async fn run(hub: Shared, paths: Paths, wake: Rc<Notify>) -> Result<()> {
    let mut notices = hub.borrow().observe();
    let failures = hub
        .borrow()
        .services
        .get("dashboard-calendar")
        .and_then(|s| s["failures"].as_u64())
        .unwrap_or(0) as usize;
    hub.borrow_mut()
        .service("dashboard-calendar", "running", failures)?;
    let mut timer = tokio::time::interval(Duration::from_secs(60));
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut watch: Option<Watch> = None;
    let mut names = vec![];
    loop {
        tokio::select! {
            _ = timer.tick() => {},
            _ = wake.notified() => {},
            result = notices.recv() => {
                if !matches!(result, Ok(Notice::Changed {ref topic,..}) if topic == SETTINGS) {continue;}
            },
            result = async {watch.as_mut().expect("watch").next().await}, if watch.is_some() => {
                let Ok(changed) = result else {watch = None; continue;};
                if !changed.iter().any(|name| name.is_empty() || names.contains(name)) {continue;}
                tokio::time::sleep(Duration::from_millis(150)).await;
            },
        }
        let Some(path) = source(&hub) else {
            publish(
                &hub,
                project(
                    &json!({"configured":false,"events":[],"error":"Calendar settings unavailable"}),
                    true,
                ),
            )?;
            watch = None;
            continue;
        };
        let expected = path.clone();
        let paths = paths.clone();
        let runtime = tokio::runtime::Handle::current();
        let (data, next, next_names) = crate::process::offload(move || {
            let _enter = runtime.enter();
            load(paths, path)
        })
        .await?;
        if source(&hub).as_ref() != Some(&expected) {
            wake.notify_one();
            continue;
        }
        watch = next;
        names = next_names;
        publish(&hub, project(&data, true))?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::path::PathBuf;
    fn current() -> chrono::DateTime<chrono::Utc> {
        chrono::Utc.with_ymd_and_hms(2030, 1, 1, 12, 0, 0).unwrap()
    }
    #[test]
    fn local_fixtures_and_contract() {
        let paths = Paths::new("/unused-fixture-home");
        let relative = project(
            &dashboard_data::calendar_at("relative.ics", current(), &paths),
            true,
        );
        assert!(!relative.health.available);
        assert!(relative.health.error.unwrap().message.contains("absolute"));
        let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/calendar");
        let data = dashboard_data::calendar_at(
            fixtures.join("events.ics").to_str().unwrap(),
            current(),
            &paths,
        );
        let state = project(&data, true);
        assert!(state.health.available);
        assert!(state
            .health
            .error
            .as_ref()
            .unwrap()
            .message
            .contains("Recurring"));
        assert_eq!(state.events.order.len(), 3);
        let events: Vec<_> = state
            .events
            .order
            .iter()
            .map(|id| &state.events.items[id])
            .collect();
        assert_eq!(events[0].title, "All day");
        assert_eq!(events[0].time, "All day");
        assert_eq!(
            events[1].start,
            chrono::Utc
                .with_ymd_and_hms(2030, 1, 3, 11, 0, 0)
                .unwrap()
                .timestamp() as u64
        );
        assert!(nacre_protocol::core_services::CoreSnapshot::Calendar(state.clone()).valid());
        assert_eq!(state, project(&data, true));
        let duplicates = project(
            &json!({"configured":true,"events":[data["events"][0],data["events"][0]],"error":""}),
            true,
        );
        assert_eq!(duplicates.events.order.len(), 2);
        assert_ne!(duplicates.events.order[0], duplicates.events.order[1]);
        let malformed = dashboard_data::calendar_at(
            fixtures.join("malformed.ics").to_str().unwrap(),
            current(),
            &paths,
        );
        assert!(project(&malformed, true).events.order.is_empty());
        let root = tempfile::tempdir().unwrap();
        let huge = root.path().join("huge.ics");
        std::fs::File::create(&huge)
            .unwrap()
            .set_len(2_000_001)
            .unwrap();
        let huge = project(
            &dashboard_data::calendar_at(huge.to_str().unwrap(), current(), &paths),
            true,
        );
        assert!(!huge.health.available);
        assert!(huge.health.error.unwrap().message.contains("2MB"));
        let absent = project(
            &dashboard_data::calendar_at(
                root.path().join("missing.ics").to_str().unwrap(),
                current(),
                &paths,
            ),
            true,
        );
        assert!(!absent.health.available);
        assert!(absent.configured);
        assert!(!project(&dashboard_data::calendar_at("", current(), &paths), true).configured);
    }
    #[tokio::test(flavor = "current_thread")]
    async fn watcher_catches_atomic_replacement_and_refresh_is_validated() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("events.ics");
        std::fs::write(&path, "").unwrap();
        let (_, mut watch, _) = load(Paths::new(root.path()), path.to_string_lossy().into_owned());
        let replacement = root.path().join("replacement");
        std::fs::write(&replacement, "BEGIN:VCALENDAR").unwrap();
        std::fs::rename(replacement, path).unwrap();
        tokio::time::timeout(Duration::from_secs(2), watch.as_mut().unwrap().next())
            .await
            .unwrap()
            .unwrap();
        let hub =
            super::super::hub::Hub::new("calendar-test".into(), false, Duration::from_millis(16))
                .shared();
        initial(&hub).unwrap();
        let wake = Rc::new(Notify::new());
        hub.borrow_mut().calendar = Some(wake.clone());
        assert!(dispatch(hub.clone(), json!({"unexpected":true})).is_err());
        dispatch(hub.clone(), json!({})).unwrap();
        tokio::time::timeout(Duration::from_millis(50), wake.notified())
            .await
            .unwrap();
        hub.borrow_mut().safe = true;
        assert_eq!(
            dispatch(hub, json!({})).unwrap_err().code,
            ErrorCode::PermissionDenied
        );
    }
    #[tokio::test(flavor = "current_thread")]
    async fn producer_publishes_file_and_settings_changes() {
        tokio::task::LocalSet::new().run_until(async {
            let root = tempfile::tempdir().unwrap();
            let path = root.path().join("events.ics");
            let stamp = (chrono::Utc::now() + chrono::Duration::hours(1)).format("%Y%m%dT%H%M%SZ").to_string();
            let content = |title| format!("BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:{stamp}\nSUMMARY:{title}\nEND:VEVENT\nEND:VCALENDAR\n");
            std::fs::write(&path, content("First")).unwrap();
            let hub = super::super::hub::Hub::new("calendar-live-fixture".into(), false, Duration::from_millis(16)).shared();
            initial(&hub).unwrap();
            hub.borrow_mut().publish(SETTINGS, json!({"error":null,"data":{"calendarFile":path}})).unwrap();
            let task = tokio::task::spawn_local(run(hub.clone(), Paths::new(root.path()), Rc::new(Notify::new())));
            let wait_title = |title: &'static str| {
                let hub = hub.clone();
                async move {
                    tokio::time::timeout(Duration::from_secs(2), async {
                        loop {
                            let found = hub.borrow().snapshot(TOPIC).unwrap()["events"]["items"].as_object().unwrap().values().any(|e| e["title"] == title);
                            if found {break;}
                            tokio::task::yield_now().await;
                        }
                    }).await.unwrap();
                }
            };
            wait_title("First").await;
            let replacement = root.path().join("replacement");
            std::fs::write(&replacement, content("Second")).unwrap();
            std::fs::rename(replacement, &path).unwrap();
            wait_title("Second").await;
            hub.borrow_mut().publish(SETTINGS, json!({"error":null,"data":{"calendarFile":""}})).unwrap();
            tokio::time::timeout(Duration::from_secs(2), async {
                while hub.borrow().snapshot(TOPIC).unwrap()["configured"] != false {tokio::task::yield_now().await;}
            }).await.unwrap();
            assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["events"]["order"], json!([]));
            task.abort(); let _ = task.await;
        }).await;
    }
}
