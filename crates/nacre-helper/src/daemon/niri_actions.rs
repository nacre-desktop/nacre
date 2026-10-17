//! The existing action parser and shared transport, with a session identity guard.
use super::{
    hub::{failure, Shared},
    niri_events,
};
use nacre_protocol::{
    compositor::{NiriAction, NiriConnection, NiriSource},
    Capability, ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn methods() -> BTreeMap<String, Method> {
    [
        (
            "niri.source",
            serde_json::from_str(include_str!("../../../../protocol/schema/niri-source.json"))
                .expect("source schema"),
            serde_json::from_str(include_str!(
                "../../../../protocol/schema/niri-connection.json"
            ))
            .expect("connection schema"),
        ),
        (
            "niri.action",
            serde_json::from_str(include_str!("../../../../protocol/schema/niri-action.json"))
                .expect("action schema"),
            json!({"type":"object"}),
        ),
    ]
    .into_iter()
    .map(|(name, params, result)| {
        (
            name.into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 5000,
                params,
                result,
            },
        )
    })
    .collect()
}

pub async fn dispatch(hub: Shared, method: &str, params: Value) -> Result<Value, Failure> {
    let control = hub.borrow().compositor.clone().ok_or_else(unavailable)?;
    let (selected, action) = if method == "niri.source" {
        let request: NiriSource = serde_json::from_value(params).map_err(|_| invalid())?;
        request.validate()?;
        (Some(request.socket), None)
    } else {
        let request: NiriAction = serde_json::from_value(params).map_err(|_| invalid())?;
        request.validate()?;
        // Validate the existing allowlist before acquiring the shared transport.
        if !matches!(
            crate::compositor::cli::parse(&request.argv),
            Ok(crate::compositor::cli::Command::Action { .. })
        ) {
            return Err(invalid());
        }
        (None, Some(request))
    };
    let identity = control
        .niri_identity(selected)
        .await
        .map_err(|_| mismatch())?;
    {
        let state = hub.borrow();
        let topic = state
            .snapshot_ref(niri_events::TOPIC)
            .ok_or_else(unavailable)?;
        if topic["available"] != true || topic["connectionId"].as_str() != Some(&identity) {
            return Err(unavailable());
        }
    }
    let Some(request) = action else {
        return serde_json::to_value(NiriConnection {
            connection_id: identity,
        })
        .map_err(|_| invalid());
    };
    if request.connection_id != identity {
        return Err(mismatch());
    }
    control
        .niri_action(identity, request.argv)
        .await
        .map_err(|_| {
            failure(
                ErrorCode::Io,
                "Compositor action failed; it may already have completed",
                false,
            )
            .dispatch_state(true)
        })
}
fn invalid() -> Failure {
    failure(ErrorCode::Validation, "Invalid compositor action", false).dispatch_state(false)
}
fn unavailable() -> Failure {
    failure(ErrorCode::Unavailable, "Compositor is unavailable", true).dispatch_state(false)
}
fn mismatch() -> Failure {
    failure(
        ErrorCode::PermissionDenied,
        "Compositor session mismatch",
        false,
    )
    .dispatch_state(false)
}
