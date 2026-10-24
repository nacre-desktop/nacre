//! The existing read-only update counter, off the daemon readiness path.
use super::hub::{failure, Shared};
use nacre_protocol::{
    core_services::{Health, UpdatesState},
    Capability, ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
    time::Duration,
};

pub const TOPIC: &str = "dashboard.updates";
const INTERVAL: Duration = Duration::from_secs(1800);
static LANE: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

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
        },
    )
}
fn publish(hub: &Shared, state: &UpdatesState) -> crate::Result<()> {
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

fn start(hub: Shared) -> Result<(), Failure> {
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
        state.health = Health {
            available: state.count.is_some(),
            stale: false,
            error: None,
        };
    } else {
        state.count = None;
        state.text.clear();
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
    start(hub)?;
    Ok(json!({"changed":false}))
}

pub async fn run(hub: Shared) -> crate::Result<()> {
    let mut timer = tokio::time::interval(INTERVAL);
    timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        timer.tick().await;
        // Manual and timer refreshes share the same lane; busy ticks are skipped.
        let _ = start(hub.clone());
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
}

#[cfg(test)]
mod refresh_tests {
    use super::*;
    #[test]
    fn overlapping_refresh_and_safe_mode_are_rejected() {
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
