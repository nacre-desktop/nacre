//! The existing read-only update counter, off the daemon readiness path.
use super::hub::{failure, Shared};
use nacre_protocol::{
    core_services::{Health, UpdatesState},
    Capability, ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const TOPIC: &str = "dashboard.updates";
const INTERVAL: Duration = Duration::from_secs(1800);
const DATABASE_DEBOUNCE: Duration = Duration::from_secs(2);
static LANE: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

fn refresh_due_at(checked: Option<f64>, now: f64) -> bool {
    checked.is_none_or(|checked| !checked.is_finite() || now - checked >= INTERVAL.as_secs_f64())
}

fn refresh_due(hub: &Shared) -> bool {
    let checked = hub
        .borrow()
        .snapshot(TOPIC)
        .and_then(|state| state["checked"].as_f64());
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64());
    refresh_due_at(checked, now)
}

pub fn methods() -> BTreeMap<String, Method> {
    BTreeMap::from([(
        "dashboard.updates.refresh".into(),
        Method {
            v: 1,
            class: Capability::Desktop,
            timeout: 10000,
            params: json!({"type":"object","additionalProperties":false}),
            result: serde_json::from_str(include_str!(
                "../../../../protocol/schema/core-service-result.json"
            ))
            .expect("core result schema"),
        },
    )])
}

pub fn initial(hub: &Shared) -> crate::Result<()> {
    publish(
        hub,
        &UpdatesState {
            health: Health {
                available: false,
                stale: false,
                error: None,
            },
            refreshing: false,
            checked: None,
            count: None,
            text: String::new(),
            packages: Vec::new(),
            truncated: false,
        },
    )
}
fn publish(hub: &Shared, state: &UpdatesState) -> crate::Result<()> {
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

pub(super) fn start(hub: Shared) -> Result<(), Failure> {
    let guard = LANE
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone()
        .try_lock_owned()
        .map_err(|_| failure(ErrorCode::Busy, "An update check is already running", true))?;
    let mut state: UpdatesState =
        serde_json::from_value(hub.borrow().snapshot(TOPIC).unwrap_or(Value::Null))
            .map_err(|_| failure(ErrorCode::Unavailable, "Update status is unavailable", true))?;
    state.refreshing = true;
    state.health.stale = state.checked.is_some();
    publish(&hub, &state)
        .map_err(|_| failure(ErrorCode::Internal, "Cannot publish update status", false))?;
    tokio::task::spawn_local(async move {
        // The guard belongs to the blocking job: cancelling a client cannot release
        // the lane while package tools are still running. Each command has a 90 s deadline.
        let result = crate::process::offload(move || {
            let data = crate::updates::dashboard_count().map_err(|_| ());
            (data, guard)
        })
        .await;
        // Keep the lane through publication too: a new request must not capture
        // the previous snapshot between command completion and its topic update.
        let (data, _guard) = match result {
            Ok((data, guard)) => (data.ok(), Some(guard)),
            Err(_) => (None, None),
        };
        complete(&mut state, data);
        if publish(&hub, &state).is_err() {
            tracing::warn!("Cannot publish update check result");
        }
    });
    Ok(())
}

fn complete(state: &mut UpdatesState, data: Option<Value>) {
    state.refreshing = false;
    if let Some(data) = data {
        state.count = data["count"].as_u64().and_then(|n| n.try_into().ok());
        state.checked = data["checked"].as_f64();
        state.text = data["text"].as_str().unwrap_or("").to_owned();
        state.packages = serde_json::from_value(data["packages"].clone()).unwrap_or_default();
        state.truncated = data["truncated"].as_bool().unwrap_or(false);
        state.health = Health {
            available: state.count.is_some(),
            stale: false,
            error: None,
        };
    } else {
        state.count = None;
        state.text.clear();
        state.packages.clear();
        state.truncated = false;
        state.health = Health {
            available: false,
            stale: true,
            error: Some(failure(
                ErrorCode::Unavailable,
                "Update check failed or timed out",
                true,
            )),
        };
    }
}

pub fn dispatch(hub: Shared, params: Value) -> Result<Value, Failure> {
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
            "Update checks are disabled in safe mode",
            false,
        ));
    }
    let (subscribed, wake) = {
        let mut state = hub.borrow_mut();
        let interest = state.watch_interest(TOPIC);
        let subscribed = *interest.borrow();
        let wake = state.updates_wake.clone();
        (subscribed, wake)
    };
    start(hub.clone())?;
    if subscribed {
        if let Some(wake) = wake {
            wake.notify_one();
        }
    }
    Ok(json!({"changed":false}))
}

async fn wait_settled_change(
    watch: &mut super::dashboard_extras::watch::Watch,
    quiet: Duration,
) -> crate::Result<()> {
    watch.next().await?;
    loop {
        match tokio::time::timeout(quiet, watch.next()).await {
            Ok(Ok(_)) => continue,
            Ok(Err(error)) => return Err(error),
            Err(_) => return Ok(()),
        }
    }
}

fn local_database_watch(path: PathBuf) -> crate::Result<super::dashboard_extras::watch::Watch> {
    use rustix::fs::inotify::WatchFlags;
    super::dashboard_extras::watch::Watch::new(vec![(
        path,
        WatchFlags::CLOSE_WRITE
            | WatchFlags::MOVED_TO
            | WatchFlags::MOVED_FROM
            | WatchFlags::CREATE
            | WatchFlags::DELETE,
    )])
}

async fn watch_local_database(path: PathBuf, changed: tokio::sync::mpsc::UnboundedSender<()>) {
    let mut watch = match local_database_watch(path) {
        Ok(watch) => watch,
        Err(error) => {
            tracing::warn!(%error, "Cannot watch pacman local database");
            return;
        }
    };
    loop {
        if let Err(error) = wait_settled_change(&mut watch, DATABASE_DEBOUNCE).await {
            tracing::warn!(%error, "Pacman local database watch stopped");
            return;
        }
        if changed.send(()).is_err() {
            return;
        }
    }
}

pub async fn run(hub: Shared, wake: Arc<tokio::sync::Notify>) -> crate::Result<()> {
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    let (changed, mut changes) = tokio::sync::mpsc::unbounded_channel();
    tokio::task::spawn_local(watch_local_database(
        PathBuf::from("/var/lib/pacman/local"),
        changed,
    ));
    let mut watch_open = true;
    let mut timer = None;
    let mut pending = false;
    let mut force = false;
    loop {
        if !*interest.borrow() {
            tokio::select! {
                result = interest.changed() => result?,
                _ = wake.notified() => { pending = true; force = true; },
                changed = changes.recv(), if watch_open => {
                    if changed.is_some() { pending = true; force = true; }
                    else { watch_open = false; }
                }
            }
            continue;
        }
        if timer.is_none() {
            timer = Some(tokio::time::interval_at(
                tokio::time::Instant::now() + INTERVAL,
                INTERVAL,
            ));
            timer
                .as_mut()
                .expect("updates timer was just created")
                .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            pending = true;
        }
        if pending {
            if force || refresh_due(&hub) {
                let _ = start(hub.clone());
            }
            pending = false;
            force = false;
            timer = Some(tokio::time::interval_at(
                tokio::time::Instant::now() + INTERVAL,
                INTERVAL,
            ));
            timer
                .as_mut()
                .expect("updates timer was just reset")
                .set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        }
        tokio::select! {
            result = interest.changed() => result?,
            _ = timer.as_mut().expect("active updates lease has a timer").tick() => { pending = true; },
            _ = wake.notified() => { pending = true; force = true; },
            changed = changes.recv(), if watch_open => {
                if changed.is_none() {
                    watch_open = false;
                } else {
                    pending = true;
                    force = true;
                }
            }
        }
        // Only an active dashboard lease triggers checks; package changes and updater
        // completion are retained while idle so the next subscriber gets fresh data.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn topic_contract_unknown_zero_success_and_failure() {
        let hub =
            super::super::hub::Hub::new("updates-test".into(), false, Duration::from_millis(16))
                .shared();
        initial(&hub).unwrap();
        let mut state: UpdatesState =
            serde_json::from_value(hub.borrow().snapshot(TOPIC).unwrap()).unwrap();
        assert!(!state.health.available);
        assert_eq!(state.count, None);
        for count in [0, 129] {
            complete(
                &mut state,
                Some(json!({"count":count,"checked":1800000000.25,"text":count.to_string()})),
            );
            assert!(state.health.available);
            assert!(!state.health.stale);
            assert_eq!(state.count, Some(count));
            publish(&hub, &state).unwrap();
            let snapshot: nacre_protocol::core_services::CoreSnapshot = serde_json::from_value(
                json!({"topic":TOPIC,"data":hub.borrow().snapshot(TOPIC).unwrap()}),
            )
            .unwrap();
            assert!(snapshot.valid());
        }
        complete(&mut state, None);
        assert_eq!(state.count, None);
        assert_eq!(state.checked, Some(1800000000.25));
        assert!(!state.health.available);
        assert!(state.health.stale);
        assert!(state.health.error.unwrap().retry);
        assert!(dispatch(hub, json!({"unexpected":true})).is_err());
    }

    #[test]
    fn periodic_refresh_waits_at_least_thirty_minutes() {
        assert!(refresh_due_at(None, 10_000.0));
        assert!(!refresh_due_at(Some(10_000.0), 10_000.0 + 1_799.0));
        assert!(refresh_due_at(Some(10_000.0), 10_000.0 + 1_800.0));
        assert!(!refresh_due_at(Some(10_001.0), 10_000.0));
    }
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    #[test]
    fn overlapping_refresh_and_safe_mode_are_rejected() {
        let _serial = super::gating_tests::SERIAL
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let hub =
            super::super::hub::Hub::new("updates-busy".into(), false, Duration::from_millis(16))
                .shared();
        initial(&hub).unwrap();
        let _guard = LANE
            .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
            .clone()
            .try_lock_owned()
            .unwrap();
        assert_eq!(dispatch(hub, json!({})).unwrap_err().code, ErrorCode::Busy);
        let safe =
            super::super::hub::Hub::new("updates-safe".into(), true, Duration::from_millis(16))
                .shared();
        assert_eq!(
            dispatch(safe, json!({})).unwrap_err().code,
            ErrorCode::PermissionDenied
        );
    }
}

#[cfg(test)]
mod pacman_watch_tests {
    use super::*;
    use std::time::Instant;

    #[tokio::test]
    async fn fake_local_database_changes_are_debounced() {
        let directory = tempfile::tempdir().unwrap();
        let mut watch = local_database_watch(directory.path().to_owned()).unwrap();
        let first = directory.path().join("pkg-1");
        std::fs::write(&first, "one").unwrap();
        let started = Instant::now();
        wait_settled_change(&mut watch, Duration::from_millis(80))
            .await
            .unwrap();
        assert!(started.elapsed() >= Duration::from_millis(70));
    }
}

#[cfg(test)]
mod gating_tests {
    use super::*;

    /// Tests that start a check share the process-wide lane; run them one at a time.
    pub(super) static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn fixture(checked: Option<f64>) -> (Shared, Arc<tokio::sync::Notify>) {
        let hub =
            super::super::hub::Hub::new("updates-gate".into(), false, Duration::from_millis(1))
                .shared();
        initial(&hub).unwrap();
        if let Some(checked) = checked {
            let mut state: UpdatesState =
                serde_json::from_value(hub.borrow().snapshot(TOPIC).unwrap()).unwrap();
            state.checked = Some(checked);
            publish(&hub, &state).unwrap();
        }
        let wake = Arc::new(tokio::sync::Notify::new());
        hub.borrow_mut().updates_wake = Some(wake.clone());
        (hub, wake)
    }
    fn refreshing(hub: &Shared) -> bool {
        hub.borrow().snapshot(TOPIC).unwrap()["refreshing"] == true
    }
    async fn settle() {
        tokio::time::sleep(Duration::from_millis(60)).await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn no_check_runs_without_a_subscriber_and_a_subscriber_triggers_one() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        tokio::task::LocalSet::new()
            .run_until(async {
                let (hub, wake) = fixture(None);
                let task = tokio::task::spawn_local(run(hub.clone(), wake.clone()));
                settle().await;
                wake.notify_one();
                settle().await;
                assert!(!refreshing(&hub), "idle daemon never starts a check");
                let (client, _) = hub.borrow_mut().add_client().unwrap();
                hub.borrow_mut()
                    .subscribe(client, &[TOPIC.into()], false)
                    .unwrap();
                tokio::time::timeout(Duration::from_secs(2), async {
                    while !refreshing(&hub)
                        && hub.borrow().snapshot(TOPIC).unwrap()["checked"].is_null()
                    {
                        tokio::time::sleep(Duration::from_millis(5)).await;
                    }
                })
                .await
                .expect("a visible widget makes the daemon check");
                task.abort();
                let _ = task.await;
            })
            .await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn a_fresh_count_is_not_rechecked_on_subscribe() {
        let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        tokio::task::LocalSet::new()
            .run_until(async {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_secs_f64();
                let (hub, wake) = fixture(Some(now));
                let task = tokio::task::spawn_local(run(hub.clone(), wake));
                let (client, _) = hub.borrow_mut().add_client().unwrap();
                hub.borrow_mut()
                    .subscribe(client, &[TOPIC.into()], false)
                    .unwrap();
                settle().await;
                assert!(!refreshing(&hub));
                task.abort();
                let _ = task.await;
            })
            .await;
    }
}
