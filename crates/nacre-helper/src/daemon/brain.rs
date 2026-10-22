//! On-demand Brain actions. The daemon only starts the native launcher worker; the
//! loopback web server stays its own idle-exiting process (N0024), so nothing here
//! is resident and no Brain unit is needed.
use super::hub::{failure, Shared};
use nacre_protocol::{Capability, ErrorCode, Failure, Method};
use serde_json::{json, Value};
use std::{collections::BTreeMap, process::Stdio, time::Duration};
use tokio::process::Command;

pub const TOPIC: &str = "brain";
/// Public method, launcher action, state key, worker deadline.
const ACTIONS: [(&str, &str, &str, u64); 2] = [
    ("brain.open", "brain", "open", 60),
    ("brain.capture", "capture", "capture", 3700),
];

/// Keep the daemon's exact image even if an upgrade replaced its pathname.
/// Unit tests run a no-op instead of re-running the test binary.
#[cfg(not(test))]
const IMAGE: &str = "/proc/self/exe";
#[cfg(test)]
const IMAGE: &str = "/bin/true";

fn state(open: bool, capture: bool, error: Option<&str>) -> Value {
    json!({"open":open,"capture":capture,"error":error})
}

pub fn initial(hub: &Shared) -> Result<(), Failure> {
    hub.borrow_mut().publish(TOPIC, state(false, false, None))
}

pub fn methods() -> BTreeMap<String, Method> {
    ACTIONS
        .iter()
        .map(|(name, ..)| {
            (
                (*name).to_owned(),
                Method {
                    v: 1,
                    class: Capability::Desktop,
                    timeout: 2000,
                    params: json!({"type":"object","additionalProperties":false}),
                    result: json!({"type":"object"}),
                },
            )
        })
        .collect()
}

fn publish(hub: &Shared, key: &str, busy: bool, error: Option<&str>) {
    let mut next = hub.borrow().snapshot(TOPIC).unwrap_or(Value::Null);
    next[key] = json!(busy);
    next["error"] = json!(error);
    let _ = hub.borrow_mut().publish(TOPIC, next);
}

/// Start the worker and return at once; one run per action at a time.
pub fn dispatch(hub: Shared, method: &str, params: Value) -> Result<Value, Failure> {
    let (_, action, key, deadline) = *ACTIONS
        .iter()
        .find(|entry| entry.0 == method)
        .ok_or_else(|| failure(ErrorCode::UnknownMethod, "Unknown method", false))?;
    if params.as_object().is_none_or(|object| !object.is_empty()) {
        return Err(
            failure(ErrorCode::Validation, "Unexpected parameters", false).dispatch_state(false),
        );
    }
    if hub.borrow().snapshot(TOPIC).is_some_and(|v| v[key] == true) {
        return Ok(json!({"started":false,"busy":true}));
    }
    let mut command = Command::new(IMAGE);
    command
        .args(["brain-control", "action", action])
        .env("NACRE_BRAIN_DIRECT", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .kill_on_drop(true);
    for name in [
        "NOTIFY_SOCKET",
        "WATCHDOG_USEC",
        "WATCHDOG_PID",
        "LISTEN_PID",
        "LISTEN_FDS",
        "LISTEN_FDNAMES",
    ] {
        command.env_remove(name);
    }
    let mut child = command.spawn().map_err(|_| {
        failure(
            ErrorCode::Unavailable,
            "Brain launcher could not start",
            true,
        )
        .dispatch_state(false)
    })?;
    publish(&hub, key, true, None);
    tokio::task::spawn_local(async move {
        let result = tokio::time::timeout(Duration::from_secs(deadline), child.wait()).await;
        let error = match result {
            Ok(Ok(status)) if status.success() => None,
            Ok(_) => Some("Brain action failed"),
            Err(_) => {
                let _ = child.kill().await;
                Some("Brain action timed out")
            }
        };
        publish(&hub, key, false, error);
    });
    Ok(json!({"started":true,"busy":false}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn hub() -> Shared {
        let hub =
            super::super::hub::Hub::new("boot".into(), false, Duration::from_millis(16)).shared();
        initial(&hub).unwrap();
        hub
    }

    #[test]
    fn registry_lists_both_actions_and_rejects_parameters() {
        assert_eq!(
            methods().keys().map(String::as_str).collect::<Vec<_>>(),
            ["brain.capture", "brain.open"]
        );
        let error = dispatch(hub(), "brain.open", json!({"x":1})).unwrap_err();
        assert_eq!(error.code, ErrorCode::Validation);
        assert_eq!(
            dispatch(hub(), "brain.other", json!({})).unwrap_err().code,
            ErrorCode::UnknownMethod
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn running_action_is_single_flight_and_clears_when_the_worker_exits() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let hub = hub();
                assert_eq!(
                    dispatch(hub.clone(), "brain.open", json!({})).unwrap()["started"],
                    true
                );
                assert_eq!(
                    dispatch(hub.clone(), "brain.open", json!({})).unwrap()["busy"],
                    true
                );
                let until = Instant::now() + Duration::from_secs(10);
                while hub.borrow().snapshot(TOPIC).unwrap()["open"] == true {
                    assert!(Instant::now() < until);
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                let done = hub.borrow().snapshot(TOPIC).unwrap();
                assert_eq!(done["capture"], false);
                assert!(done["error"].is_null() || done["error"] == "Brain action failed");
            })
            .await;
    }
}
