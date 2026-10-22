//! Shared native request transport for compositor observation and session policy.
use crate::{compositor::backend::Backend, paths::Paths, Result};
use serde_json::Value;
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::Mutex;

fn entity_on_output(
    rows: &[Value],
    kind: &str,
    output: &str,
    native_output: &Value,
    target: &Value,
) -> bool {
    rows.iter().any(|row| {
        let entity = if kind == "window" {
            [&row["windowId"], &row["id"], &row["address"]]
                .into_iter()
                .any(|id| id == target)
        } else {
            [&row["id"], &row["workspaceId"], &row["name"]]
                .into_iter()
                .any(|id| id == target)
        };
        let owner = [&row["outputId"], &row["monitor"]]
            .into_iter()
            .any(|id| id.as_str() == Some(output) || id == native_output);
        entity && owner
    })
}

fn workspace_index(row: &Value) -> Option<i64> {
    row["index"]
        .as_i64()
        .or_else(|| row["id"].as_i64())
        .or_else(|| row["name"].as_str()?.parse().ok())
}

fn scroll_target(
    rows: &[Value],
    output: &str,
    native_output: &Value,
    active: &Value,
    forward: bool,
) -> Result<Value> {
    if active.is_null() {
        return Err("Active workspace is unavailable".into());
    }
    let current = rows
        .iter()
        .find(|row| row["id"] == *active || row["workspaceId"] == *active)
        .and_then(workspace_index)
        .ok_or("Active workspace is unavailable")?;
    let mut candidates = rows
        .iter()
        .filter_map(|row| {
            let id = row.get("id")?;
            let index = workspace_index(row)?;
            if index <= 0 {
                return None;
            }
            (entity_on_output(
                std::slice::from_ref(row),
                "workspace",
                output,
                native_output,
                id,
            ) && if forward {
                index > current
            } else {
                index < current
            })
            .then_some((index, id))
        })
        .collect::<Vec<_>>();
    candidates.sort_by_key(|(index, _)| *index);
    let target = if forward {
        candidates.first()
    } else {
        candidates.last()
    };
    target
        .map(|(_, id)| (*id).clone())
        .ok_or_else(|| "No workspace in that direction".into())
}

enum WorkspaceIndexPlan {
    Existing(Value),
    Create,
}

fn workspace_index_plan(
    rows: &[Value],
    output: &str,
    native_output: &Value,
    index: u16,
    can_create: bool,
) -> Result<WorkspaceIndexPlan> {
    let matching = rows
        .iter()
        .filter(|row| workspace_index(row) == Some(i64::from(index)))
        .collect::<Vec<_>>();
    if let Some(row) = matching.iter().find(|row| {
        let row = **row;
        let Some(id) = row
            .get("id")
            .filter(|id| !id.is_null())
            .or_else(|| row.get("workspaceId").filter(|id| !id.is_null()))
            .or_else(|| row.get("name").filter(|id| !id.is_null()))
        else {
            return false;
        };
        entity_on_output(
            std::slice::from_ref(row),
            "workspace",
            output,
            native_output,
            id,
        )
    }) {
        let row = *row;
        let id = row
            .get("id")
            .filter(|id| !id.is_null())
            .or_else(|| row.get("workspaceId").filter(|id| !id.is_null()))
            .or_else(|| row.get("name").filter(|id| !id.is_null()))
            .ok_or("Workspace ID is unavailable")?;
        return Ok(WorkspaceIndexPlan::Existing(id.clone()));
    }
    if !matching.is_empty() {
        return Err("Workspace index exists on another output".into());
    }
    if can_create {
        Ok(WorkspaceIndexPlan::Create)
    } else {
        Err("Niri does not create missing workspaces".into())
    }
}

#[derive(Clone)]
pub struct Control {
    backend: Arc<Mutex<Option<Backend>>>,
    paths: Paths,
    environment: BTreeMap<String, String>,
}
impl Control {
    pub async fn scoped_action(
        &self,
        kind: &'static str,
        output: String,
        target: Value,
    ) -> Result<serde_json::Value> {
        self.with(move |backend| {
            let monitors = backend.query(
                "monitors",
                std::time::Duration::from_secs(5),
                Some(4 * 1024 * 1024),
            )?;
            let monitors = monitors
                .as_array()
                .ok_or("Malformed compositor monitor list")?;
            let monitor = monitors
                .iter()
                .find(|row| row["name"].as_str() == Some(&output))
                .ok_or("Output disappeared")?;
            let native_output = monitor
                .get("id")
                .filter(|id| !id.is_null())
                .cloned()
                .unwrap_or_else(|| Value::String(output.clone()));
            let rows = backend.query(
                if kind == "window" {
                    "windows"
                } else {
                    "workspaces"
                },
                std::time::Duration::from_secs(5),
                Some(4 * 1024 * 1024),
            )?;
            let rows = rows.as_array().ok_or("Malformed compositor entity list")?;
            if kind == "workspace_index" {
                let index = target
                    .as_u64()
                    .filter(|value| (1..=255).contains(value))
                    .ok_or("Invalid workspace index")? as u16;
                let can_create = matches!(backend, Backend::Hyprland(_));
                match workspace_index_plan(rows, &output, &native_output, index, can_create)? {
                    WorkspaceIndexPlan::Existing(target) => {
                        return Ok(backend.action(
                            "switch_workspace",
                            &[],
                            &serde_json::json!({"workspace":target,"output":output}),
                        )?);
                    }
                    WorkspaceIndexPlan::Create => {
                        return Ok(backend.action(
                            "activate_workspace_index_on_output",
                            &[Value::from(index)],
                            &serde_json::json!({"output":output}),
                        )?);
                    }
                }
            }
            let target = if kind.starts_with("scroll_") {
                scroll_target(
                    rows,
                    &output,
                    &native_output,
                    &monitor["activeWorkspace"]["id"],
                    kind == "scroll_next",
                )?
            } else {
                target
            };
            let found = entity_on_output(
                rows,
                if kind == "window" {
                    "window"
                } else {
                    "workspace"
                },
                &output,
                &native_output,
                &target,
            );
            if !found {
                return Err("Compositor entity moved or disappeared".into());
            }
            if kind == "window" {
                let selector = if target.is_string() {
                    target
                } else {
                    Value::String(target.to_string())
                };
                Ok(backend.action(
                    "focus_window",
                    &[selector],
                    &serde_json::json!({"output":output}),
                )?)
            } else {
                Ok(backend.action(
                    "switch_workspace",
                    &[],
                    &serde_json::json!({"workspace":target,"output":output}),
                )?)
            }
        })
        .await
    }
    pub async fn niri_action(
        &self,
        identity: String,
        argv: Vec<String>,
    ) -> Result<serde_json::Value> {
        let path = self
            .environment
            .get("NIRI_SOCKET")
            .cloned()
            .ok_or("Missing Niri socket")?;
        self.with(move |backend| {
            // Check again after waiting for the shared request connection.
            if super::niri_events::identity(std::path::Path::new(&path))? != identity {
                return Err("Compositor session mismatch".into());
            }
            Ok(crate::compositor::cli::action_response(
                backend,
                &serde_json::json!({"argv":argv}),
            ))
        })
        .await
    }
    /// Verify both socket identities without opening another compositor connection.
    pub async fn niri_identity(&self, selected: Option<String>) -> Result<String> {
        let environment = self.environment.clone();
        tokio::task::spawn_blocking(move || {
            if crate::compositor::backend::select(&environment)? != "niri" {
                return Err("Selected compositor is not niri".into());
            }
            let path = environment
                .get("NIRI_SOCKET")
                .ok_or("Missing Niri socket")?;
            let identity = super::niri_events::identity(std::path::Path::new(path))?;
            if let Some(selected) = selected {
                if super::niri_events::identity(std::path::Path::new(&selected))? != identity {
                    return Err("Compositor session mismatch".into());
                }
            }
            Ok(identity)
        })
        .await?
    }
    pub fn new(paths: Paths, environment: BTreeMap<String, String>) -> Self {
        Self {
            backend: Arc::new(Mutex::new(None)),
            paths,
            environment,
        }
    }
    /// Wait for transport ownership asynchronously, occupying a blocking worker
    /// only while the existing synchronous native implementation does its work.
    pub async fn with<T, F>(&self, operation: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&mut Backend) -> Result<T> + Send + 'static,
    {
        let mut backend = self.backend.clone().lock_owned().await;
        let paths = self.paths.clone();
        let environment = self.environment.clone();
        tokio::task::spawn_blocking(move || {
            if backend.is_none() {
                *backend = Some(Backend::new(paths, environment)?);
            }
            operation(
                backend
                    .as_mut()
                    .ok_or("Compositor transport is unavailable")?,
            )
        })
        .await?
    }
}

#[cfg(test)]
mod shell_action_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn scroll_follows_live_output_order_across_gaps_without_wrapping() {
        let rows = vec![
            json!({"id":-99,"name":"special:scratchpad","monitor":"DP-1"}),
            json!({"id":1,"monitor":"DP-1"}),
            json!({"id":3,"monitor":"DP-2"}),
            json!({"id":5,"monitor":"DP-1"}),
        ];
        assert_eq!(
            scroll_target(&rows, "DP-1", &json!(0), &json!(1), true).unwrap(),
            json!(5)
        );
        assert_eq!(
            scroll_target(&rows, "DP-1", &json!(0), &json!(5), false).unwrap(),
            json!(1)
        );
        assert!(scroll_target(&rows, "DP-1", &json!(0), &json!(5), true).is_err());
        assert!(scroll_target(&rows, "DP-1", &json!(0), &json!(1), false).is_err());
        assert!(scroll_target(&rows, "DP-1", &json!(0), &Value::Null, true).is_err());
    }

    #[test]
    fn output_ownership_matches_niri_and_hyprland_native_records() {
        let snapshot = json!({
            "monitors":[{"name":"DP-1","id":1},{"name":"DP-2","id":2}],
            "workspaces":[{"id":"niri-workspace:1","name":"one","outputId":"DP-1","index":1},{"id":"niri-workspace:2","name":"two","outputId":"DP-2","index":1}],
            "windows":[{"windowId":"niri-window:2","workspaceId":"niri-workspace:2","outputId":"DP-2"}],
            "activeWindow":{},"activeWorkspace":{},"focusedMonitor":{"name":"DP-1"}
        });
        let niri = crate::daemon::compositor::project(&snapshot, "niri", &json!({})).unwrap();
        let workspaces = niri["workspaces"]
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let windows = niri["windows"]
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        assert!(entity_on_output(
            &workspaces,
            "workspace",
            "DP-1",
            &Value::String("DP-1".into()),
            &json!("niri-workspace:1")
        ));
        assert!(!entity_on_output(
            &workspaces,
            "workspace",
            "DP-2",
            &Value::String("DP-2".into()),
            &json!("niri-workspace:1")
        ));
        assert!(entity_on_output(
            &windows,
            "window",
            "DP-2",
            &Value::String("DP-2".into()),
            &json!("niri-window:2")
        ));

        let snapshot = json!({
            "monitors":[{"name":"DP-1","id":1},{"name":"DP-2","id":2}],
            "workspaces":[{"id":4,"name":"4","monitor":"DP-1"},{"id":8,"name":"8","monitor":"DP-2"}],
            "windows":[{"address":"0xabc","monitor":2,"outputId":2,"workspaceId":8}],
            "activeWindow":{},"activeWorkspace":{},"focusedMonitor":{"name":"DP-1"}
        });
        let hypr = crate::daemon::compositor::project(&snapshot, "hyprland", &json!({})).unwrap();
        let windows = hypr["windows"]
            .as_object()
            .unwrap()
            .values()
            .cloned()
            .collect::<Vec<_>>();
        assert_eq!(windows.len(), 1);
        assert!(entity_on_output(
            &windows,
            "window",
            "DP-2",
            &json!(2),
            &json!("0xabc")
        ));
        assert!(!entity_on_output(
            &windows,
            "window",
            "DP-1",
            &json!(1),
            &json!("0xabc")
        ));
    }

    #[test]
    fn numeric_workspace_targets_resolve_or_create_only_within_policy() {
        let niri = vec![json!({"id":"niri-workspace:6","index":6,"outputId":"DP-1"})];
        assert!(
            matches!(workspace_index_plan(&niri, "DP-1", &json!("DP-1"), 6, false).unwrap(), WorkspaceIndexPlan::Existing(Value::String(id)) if id == "niri-workspace:6")
        );
        assert!(workspace_index_plan(&niri, "DP-2", &json!("DP-2"), 6, false).is_err());
        let hypr = vec![json!({"id":6,"name":"6","monitor":"DP-2"})];
        assert!(workspace_index_plan(&hypr, "DP-1", &json!(1), 6, true).is_err());
        assert!(matches!(
            workspace_index_plan(&[], "DP-1", &json!(1), 6, true).unwrap(),
            WorkspaceIndexPlan::Create
        ));
        assert!(workspace_index_plan(&[], "DP-1", &json!(1), 6, false).is_err());
    }
}
