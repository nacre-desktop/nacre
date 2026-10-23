//! Semantic compositor and session actions exposed to independent shells.
use super::hub::{failure, Shared};
use nacre_protocol::{
    shell_actions::{
        SessionAction, SessionRequest, WindowFocus, WorkspaceActivate, WorkspaceDirection,
        WorkspaceScroll, WorkspaceTarget,
    },
    ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};

fn invalid() -> Failure {
    failure(ErrorCode::Validation, "Invalid shell action", false).dispatch_state(false)
}
fn unavailable() -> Failure {
    failure(ErrorCode::Unavailable, "Compositor is unavailable", true).dispatch_state(false)
}
fn action_error() -> Failure {
    failure(
        ErrorCode::Io,
        "Desktop action failed; it may already have completed",
        false,
    )
    .dispatch_state(true)
}
fn confirmation_required(action: &SessionAction) -> bool {
    *action != SessionAction::Lock
}
fn confirm(request: &SessionRequest) -> Result<(), Failure> {
    if confirmation_required(&request.action) && !request.confirmed {
        Err(failure(
            ErrorCode::PermissionDenied,
            "Confirmation required for this session action",
            false,
        )
        .dispatch_state(false))
    } else {
        Ok(())
    }
}

pub fn methods() -> BTreeMap<String, Method> {
    nacre_protocol::shell_actions::methods()
}

pub fn available_methods(hub: &super::hub::Hub) -> BTreeMap<String, Method> {
    if hub.safe
        || hub.compositor.is_none()
        || hub
            .snapshot_ref("compositor")
            .is_none_or(|topic| topic["available"] != true)
    {
        BTreeMap::new()
    } else {
        methods()
    }
}

fn output<'a>(snapshot: &'a Value, output: &str) -> Result<&'a Value, Failure> {
    snapshot["outputs"][output]
        .as_object()
        .map(|_| &snapshot["outputs"][output])
        .ok_or_else(invalid)
}
fn workspace<'a>(
    snapshot: &'a Value,
    output_id: &str,
    workspace_id: &str,
) -> Result<&'a Value, Failure> {
    let _ = output(snapshot, output_id)?;
    snapshot["workspaces"][workspace_id]
        .as_object()
        .filter(|row| row["outputId"] == output_id)
        .map(|_| &snapshot["workspaces"][workspace_id])
        .ok_or_else(invalid)
}
/// The window's own output and native target; an optional `output` must match it.
fn window(snapshot: &Value, request: &WindowFocus) -> Result<(String, Value), Failure> {
    let row = snapshot["windows"][&request.window]
        .as_object()
        .map(|_| &snapshot["windows"][&request.window])
        .ok_or_else(invalid)?;
    let owner = row["outputId"].as_str().ok_or_else(invalid)?;
    if request
        .output
        .as_deref()
        .is_some_and(|guard| guard != owner)
    {
        return Err(invalid());
    }
    let _ = output(snapshot, owner)?;
    let target = row
        .get("nativeId")
        .filter(|id| !id.is_null())
        .or_else(|| row.get("windowId"))
        .or_else(|| row.get("address"))
        .or_else(|| row.get("id"))
        .cloned()
        .unwrap_or_else(|| json!(request.window));
    Ok((owner.to_owned(), target))
}
fn native(value: &Value) -> Value {
    value
        .get("nativeId")
        .filter(|id| !id.is_null())
        .cloned()
        .unwrap_or_else(|| value["id"].clone())
}

pub async fn compositor(hub: Shared, method: &str, params: Value) -> Result<Value, Failure> {
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::PermissionDenied,
            "Compositor actions are disabled in safe mode",
            false,
        )
        .dispatch_state(false));
    }
    let control = hub.borrow().compositor.clone().ok_or_else(unavailable)?;
    let snapshot = hub
        .borrow()
        .snapshot("compositor")
        .ok_or_else(unavailable)?;
    if snapshot["available"] != true {
        return Err(unavailable());
    }
    let result = match method {
        "workspace.activate" => {
            let request: WorkspaceActivate =
                serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            let (kind, target) = match request.workspace {
                WorkspaceTarget::Opaque(workspace_id) => {
                    let row = workspace(&snapshot, &request.output, &workspace_id)?;
                    ("workspace", native(row))
                }
                WorkspaceTarget::Index(index) => ("workspace_index", Value::from(index)),
            };
            control
                .scoped_action(kind, request.output, target)
                .await
                .map_err(|_| action_error())?
        }
        "workspace.scroll" => {
            let request: WorkspaceScroll = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            let _ = output(&snapshot, &request.output)?;
            control
                .scoped_action(
                    if request.direction == WorkspaceDirection::Next {
                        "scroll_next"
                    } else {
                        "scroll_prev"
                    },
                    request.output,
                    Value::Null,
                )
                .await
                .map_err(|_| action_error())?
        }
        "window.focus" => {
            let request: WindowFocus = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            let (window_output, target) = window(&snapshot, &request)?;
            control
                .scoped_action("window", window_output, target)
                .await
                .map_err(|_| action_error())?
        }
        _ => return Err(invalid()),
    };
    let _ = result;
    Ok(nacre_protocol::shell_actions::success(true))
}

pub async fn session(hub: Shared, params: Value) -> Result<Value, Failure> {
    let request: SessionRequest = serde_json::from_value(params).map_err(|_| invalid())?;
    request.validate()?;
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::PermissionDenied,
            "Session actions are disabled in safe mode",
            false,
        )
        .dispatch_state(false));
    }
    confirm(&request)?;
    match request.action {
        SessionAction::Lock => {
            let result = run_command("loginctl", vec!["lock-session".into()]).await?;
            if !result.status.success() {
                return Err(action_error());
            }
        }
        SessionAction::Logout => {
            let control = hub.borrow().compositor.clone().ok_or_else(unavailable)?;
            control
                .with(|backend| {
                    backend
                        .action("logout", &[], &json!({}))
                        .map_err(|_| "Logout failed".into())
                })
                .await
                .map_err(|_| action_error())?;
        }
        action => {
            let command = match action {
                SessionAction::Suspend => "suspend",
                SessionAction::Reboot => "reboot",
                SessionAction::Shutdown => "poweroff",
                _ => return Err(invalid()),
            };
            let result = run_command("systemctl", vec![command.into()]).await?;
            if !result.status.success() {
                return Err(action_error());
            }
        }
    }
    Ok(nacre_protocol::shell_actions::success(true))
}

async fn run_command(
    program: &'static str,
    arguments: Vec<String>,
) -> Result<std::process::Output, Failure> {
    tokio::task::spawn_blocking(move || {
        crate::process::run(program, &arguments, None, Duration::from_secs(10))
    })
    .await
    .map_err(|_| action_error())?
    .map_err(|_| action_error())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn window_focus_is_output_agnostic_with_an_optional_guard() {
        let state = json!({"outputs":{"A":{},"B":{}},"windows":{"w1":{"id":"w1","windowId":"niri-window:1","outputId":"A"},"w2":{"id":"w2","outputId":null}}});
        let request = |output: Option<&str>, window: &str| WindowFocus {
            output: output.map(Into::into),
            window: window.into(),
        };
        let (owner, target) = window(&state, &request(None, "w1")).unwrap();
        assert_eq!((owner.as_str(), target), ("A", json!("niri-window:1")));
        assert!(window(&state, &request(Some("A"), "w1")).is_ok());
        assert!(window(&state, &request(Some("B"), "w1")).is_err());
        assert!(window(&state, &request(None, "w2")).is_err());
        assert!(window(&state, &request(None, "missing")).is_err());
    }
    #[test]
    fn output_scope_rejects_missing_and_cross_output_entities() {
        let state = json!({"outputs":{"A":{"activeWorkspace":{"id":"w1"}}},"workspaces":{"w1":{"id":"w1","outputId":"A"},"w2":{"id":"w2","outputId":"B"}}});
        assert!(workspace(&state, "A", "w1").is_ok());
        assert!(workspace(&state, "A", "w2").is_err());
        assert!(output(&state, "B").is_err());
        for action in [
            SessionAction::Lock,
            SessionAction::Logout,
            SessionAction::Suspend,
            SessionAction::Reboot,
            SessionAction::Shutdown,
        ] {
            let request = SessionRequest {
                action: action.clone(),
                confirmed: false,
            };
            assert_eq!(confirm(&request).is_ok(), action == SessionAction::Lock);
        }
    }
}
