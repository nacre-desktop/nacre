//! Native event reducer and compositor-neutral snapshots; no IPC or UI side effects.
use serde_json::{json, Map, Value};
use std::{collections::BTreeSet, fmt};

#[derive(Debug)]
pub struct Error {
    pub kind: &'static str,
    pub message: String,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for Error {}
type Result<T> = std::result::Result<T, Error>;
fn invalid(message: impl Into<String>) -> Error {
    Error {
        kind: "ValueError",
        message: message.into(),
    }
}
fn get<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value.get(key).ok_or_else(|| Error {
        kind: "KeyError",
        message: format!("'{key}'"),
    })
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
fn equal(a: &Value, b: &Value) -> bool {
    if a == b {
        return true;
    }
    match (a, b) {
        (Value::Bool(v), Value::Number(n)) | (Value::Number(n), Value::Bool(v)) => {
            n.as_f64() == Some(if *v { 1.0 } else { 0.0 })
        }
        _ => false,
    }
}
pub fn identity(value: &Value, kind: &str) -> Result<String> {
    value
        .as_u64()
        .map(|v| format!("niri-{kind}:{v}"))
        .ok_or_else(|| invalid(format!("Malformed native {kind} ID")))
}
pub fn native_id(value: &str, kind: &str) -> Result<u64> {
    let digits = value
        .strip_prefix(&format!("niri-{kind}:"))
        .filter(|s| {
            !s.is_empty()
                && s.len() <= 20
                && s.bytes().all(|b| b.is_ascii_digit())
                && (s.len() == 1 || !s.starts_with('0'))
        })
        .ok_or_else(|| invalid(format!("Invalid opaque {kind} ID")))?;
    digits
        .parse()
        .map_err(|_| invalid("Invalid native ID range"))
}
pub fn capabilities() -> Value {
    json!({"stableWorkspaceIndices":false,"globalWorkspaces":false,"perOutputWorkspaces":true,"scrollingLayout":true,"floatingPlacement":true,"pinWindow":false,"windowCapture":false,"nativeWindowScreenshot":true,"shapedBackgroundBlur":false,"cursorPosition":false,"nativePanelDrag":false,"windowHide":false,"windowTrash":false,"scratchpad":false,"windowGroups":true,"windowForceKill":false,"pseudoTiling":false,"layoutSplit":false,"keymaps":false,"nonConsumingShortcuts":false,"bindingIntrospection":false,"fullscreenState":false,"shortcutRelease":false,"desktopOptions":["animations","blur","shadow","followMouse","naturalScroll","gapsIn","borderSize","rounding"],"monitorConfiguration":true,"monitorMirroring":false,"nightLightController":true})
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct State {
    pub outputs: Map<String, Value>,
    pub workspaces: Map<String, Value>,
    pub windows: Map<String, Value>,
    pub ready: BTreeSet<String>,
    pub config_failed: Value,
    pub overview_open: Value,
    pub overview_hover_supported: bool,
    pub overview_hover_id: Option<String>,
}
impl State {
    pub fn new(outputs: Map<String, Value>) -> Self {
        Self {
            outputs,
            config_failed: json!(false),
            overview_open: json!(false),
            ..Self::default()
        }
    }
    pub fn apply(&mut self, event: &Value) -> Result<Value> {
        if event.is_string() {
            return Ok(json!({"type":"other"}));
        }
        let object = event
            .as_object()
            .filter(|v| v.len() == 1)
            .ok_or_else(|| invalid("Malformed niri event"))?;
        let (name, payload) = object
            .iter()
            .next()
            .ok_or_else(|| invalid("Malformed niri event"))?;
        const KNOWN: &[&str] = &[
            "WorkspacesChanged",
            "WindowsChanged",
            "WorkspaceActivated",
            "WorkspaceUrgencyChanged",
            "WorkspaceActiveWindowChanged",
            "WindowOpenedOrChanged",
            "WindowClosed",
            "WindowFocusChanged",
            "WindowUrgencyChanged",
            "WindowLayoutsChanged",
            "WindowFocusTimestampChanged",
            "ConfigLoaded",
            "OverviewOpenedOrClosed",
            "NacreOverviewHoverChanged",
        ];
        if !KNOWN.contains(&name.as_str()) {
            return Ok(json!({"type":"other"}));
        }
        if !payload.is_object() {
            return Err(invalid("Malformed niri event payload"));
        }
        match name.as_str() {
            "WorkspacesChanged" | "WindowsChanged" => {
                let (kind, key) = if name == "WorkspacesChanged" {
                    ("workspace", "workspaces")
                } else {
                    ("window", "windows")
                };
                let rows = get(payload, key)?
                    .as_array()
                    .filter(|rows| rows.iter().all(Value::is_object))
                    .ok_or_else(|| invalid("Malformed niri records"))?;
                let mut replacement = Map::new();
                for row in rows {
                    replacement.insert(identity(get(row, "id")?, kind)?, row.clone());
                }
                if kind == "workspace" {
                    self.workspaces = replacement;
                } else {
                    self.windows = replacement;
                }
                self.ready.insert(kind.into());
                Ok(
                    json!({"type":if kind=="workspace"{"workspace-monitor-change"}else{"windows-changed"}}),
                )
            }
            "WorkspaceActivated" => {
                let id = get(payload, "id")?;
                let key = identity(id, "workspace")?;
                if let Some(current) = self.workspaces.get(&key).filter(|v| truthy(v)).cloned() {
                    for row in self.workspaces.values_mut() {
                        if row.get("output") == current.get("output") {
                            row["is_active"] = json!(equal(&row["id"], id));
                        }
                        if payload.get("focused").is_some_and(truthy) {
                            row["is_focused"] = json!(equal(&row["id"], id));
                        }
                    }
                }
                Ok(json!({"type":"workspace-monitor-change","workspaceId":key}))
            }
            "WorkspaceUrgencyChanged" | "WorkspaceActiveWindowChanged" => {
                let id = payload
                    .get("id")
                    .unwrap_or_else(|| &payload["workspace_id"]);
                let key = identity(id, "workspace")?;
                if let Some(row) = self.workspaces.get_mut(&key) {
                    if name == "WorkspaceUrgencyChanged" {
                        row["is_urgent"] = get(payload, "urgent")?.clone();
                    } else {
                        row["active_window_id"] = get(payload, "active_window_id")?.clone();
                    }
                }
                Ok(json!({"type":"workspace-monitor-change","workspaceId":key}))
            }
            "WindowOpenedOrChanged" => {
                let row = get(payload, "window")?;
                if !row.is_object() {
                    return Err(invalid("Malformed window event"));
                }
                let key = identity(get(row, "id")?, "window")?;
                let previous = self.windows.get(&key);
                let mut changes = Vec::new();
                for (field, name) in [
                    ("title", "window-title"),
                    ("workspace_id", "window-workspace"),
                    ("is_floating", "window-floating"),
                ] {
                    if previous.is_some_and(|previous| previous.get(field) != row.get(field)) {
                        changes.push(name);
                    }
                }
                let event_type = changes.first().copied().unwrap_or(if previous.is_some() {
                    "window-changed"
                } else {
                    "window-open"
                });
                self.windows.insert(key.clone(), row.clone());
                if row.get("is_focused").is_some_and(truthy) {
                    for other in self.windows.values_mut() {
                        other["is_focused"] = json!(equal(&other["id"], &row["id"]));
                    }
                }
                Ok(json!({"type":event_type,"changes":changes,"windowId":key}))
            }
            "WindowClosed" => {
                let key = identity(get(payload, "id")?, "window")?;
                self.windows.shift_remove(&key);
                Ok(json!({"type":"window-close","windowId":key}))
            }
            "WindowFocusChanged" => {
                let focused = get(payload, "id")?;
                for row in self.windows.values_mut() {
                    row["is_focused"] = json!(equal(&row["id"], focused));
                }
                let key = if focused.is_null() {
                    None
                } else {
                    Some(identity(focused, "window")?)
                };
                Ok(json!({"type":"window-focus","windowId":key}))
            }
            "WindowUrgencyChanged" => {
                let key = identity(get(payload, "id")?, "window")?;
                if let Some(row) = self.windows.get_mut(&key) {
                    row["is_urgent"] = get(payload, "urgent")?.clone();
                }
                Ok(json!({"type":"window-urgent","windowId":key}))
            }
            "WindowLayoutsChanged" => {
                let changes = get(payload, "changes")?
                    .as_array()
                    .ok_or_else(|| invalid("Malformed niri layout changes"))?;
                for change in changes {
                    let pair = change
                        .as_array()
                        .filter(|v| v.len() == 2)
                        .ok_or_else(|| invalid("Malformed niri layout changes"))?;
                    let key = identity(&pair[0], "window")?;
                    if let Some(row) = self.windows.get_mut(&key) {
                        row["layout"] = pair[1].clone();
                    }
                }
                Ok(json!({"type":"window-geometry"}))
            }
            "WindowFocusTimestampChanged" => {
                let key = identity(get(payload, "id")?, "window")?;
                if let Some(row) = self.windows.get_mut(&key) {
                    row["focus_timestamp"] = get(payload, "focus_timestamp")?.clone();
                }
                Ok(json!({"type":"other"}))
            }
            "ConfigLoaded" => {
                self.config_failed = get(payload, "failed")?.clone();
                Ok(json!({"type":"configuration-reloaded","failed":self.config_failed}))
            }
            "OverviewOpenedOrClosed" => {
                self.overview_open = get(payload, "is_open")?.clone();
                if !truthy(&self.overview_open) {
                    self.overview_hover_id = None;
                }
                Ok(json!({"type":"overview-changed"}))
            }
            "NacreOverviewHoverChanged" => {
                let id = get(payload, "id")?;
                let key = if id.is_null() {
                    None
                } else {
                    Some(identity(id, "window")?)
                };
                self.overview_hover_id = if truthy(&self.overview_open) {
                    key
                } else {
                    None
                };
                self.overview_hover_supported = true;
                Ok(json!({"type":"overview-hover"}))
            }
            _ => Err(invalid("Malformed niri event")),
        }
    }
    pub fn snapshot(&self) -> Result<Value> {
        let mut workspaces = Vec::new();
        for (key, row) in &self.workspaces {
            let index = get(row, "idx")?;
            let name = row
                .get("name")
                .filter(|v| truthy(v))
                .cloned()
                .unwrap_or_else(|| json!(text(index)));
            let active_window = match row.get("active_window_id") {
                Some(v) if !v.is_null() => Some(identity(v, "window")?),
                _ => None,
            };
            workspaces.push(json!({"id":key,"workspaceId":key,"index":index,"name":name,"outputId":row["output"],"monitor":row["output"],"active":truthy(&row["is_active"]),"focused":truthy(&row["is_focused"]),"urgent":truthy(&row["is_urgent"]),"activeWindowId":active_window}));
        }
        workspaces.sort_by(|a, b| {
            a["outputId"]
                .as_str()
                .unwrap_or("")
                .cmp(b["outputId"].as_str().unwrap_or(""))
                .then_with(|| {
                    a["index"]
                        .as_f64()
                        .partial_cmp(&b["index"].as_f64())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        let focused = workspaces.iter().find(|row| row["focused"] == true);
        let mut monitors = Vec::new();
        for (name, row) in &self.outputs {
            let Some(logical) = row.get("logical").filter(|v| truthy(v)) else {
                continue;
            };
            let empty = Vec::new();
            let modes = row.get("modes").and_then(Value::as_array).unwrap_or(&empty);
            let empty_mode = json!({});
            let mode = row["current_mode"]
                .as_u64()
                .and_then(|index| usize::try_from(index).ok())
                .and_then(|index| modes.get(index))
                .unwrap_or(&empty_mode);
            let transform = match logical["transform"].as_str() {
                Some("90") => 1,
                Some("180") => 2,
                Some("270") => 3,
                Some("Flipped") => 4,
                Some("Flipped90") => 5,
                Some("Flipped180") => 6,
                Some("Flipped270") => 7,
                _ => 0,
            };
            let mut available = Vec::new();
            for mode in modes {
                available.push(format!(
                    "{}x{}@{:.3}Hz",
                    text(get(mode, "width")?),
                    text(get(mode, "height")?),
                    number(get(mode, "refresh_rate")?)? / 1000.0
                ));
            }
            monitors.push(json!({"id":name,"name":name,"outputId":name,"x":get(logical,"x")?,"y":get(logical,"y")?,"width":mode.get("width").unwrap_or(get(logical,"width")?),"height":mode.get("height").unwrap_or(get(logical,"height")?),"logicalWidth":get(logical,"width")?,"logicalHeight":get(logical,"height")?,"scale":logical.get("scale").cloned().unwrap_or(json!(1)),"transform":transform,"refreshRate":number(mode.get("refresh_rate").unwrap_or(&json!(0)))?/1000.0,"reserved":[0,0,0,0],"reservedKnown":false,"focused":focused.is_some_and(|row|row["outputId"]==name.as_str()),"activeWorkspace":workspaces.iter().find(|workspace|workspace["outputId"]==name.as_str()&&workspace["active"]==true),"availableModes":available}));
        }
        let mut windows = Vec::new();
        for (key, row) in &self.windows {
            let workspace_id = match row.get("workspace_id") {
                Some(v) if !v.is_null() => Some(identity(v, "workspace")?),
                _ => None,
            };
            let workspace = workspaces
                .iter()
                .find(|item| Some(item["id"].as_str().unwrap_or("")) == workspace_id.as_deref());
            let monitor = monitors.iter().find(|item| {
                workspace.is_some_and(|workspace| item["name"] == workspace["outputId"])
            });
            let empty_layout = json!({});
            let layout = row
                .get("layout")
                .filter(|v| truthy(v))
                .unwrap_or(&empty_layout);
            let position = layout
                .get("tile_pos_in_workspace_view")
                .filter(|v| !v.is_null());
            let offset = layout
                .get("window_offset_in_tile")
                .cloned()
                .unwrap_or(json!([0, 0]));
            let size = layout.get("window_size").cloned().unwrap_or(json!([0, 0]));
            let known = position.is_some() && monitor.is_some();
            let coordinate = |axis: usize, field: &str| -> Result<Value> {
                if let (Some(position), Some(monitor)) = (position, monitor) {
                    add(&[
                        get(monitor, field)?,
                        at(position, axis)?,
                        at(&offset, axis)?,
                    ])
                } else {
                    Ok(Value::Null)
                }
            };
            let x = coordinate(0, "x")?;
            let y = coordinate(1, "y")?;
            let geometry = json!({"x":x,"y":y,"width":at(&size,0)?,"height":at(&size,1)?});
            let app = row
                .get("app_id")
                .filter(|v| truthy(v))
                .cloned()
                .unwrap_or(json!(""));
            let title = row
                .get("title")
                .filter(|v| truthy(v))
                .cloned()
                .unwrap_or(json!(""));
            let pid = row
                .get("pid")
                .filter(|v| truthy(v))
                .cloned()
                .unwrap_or(json!(0));
            let workspace_value = workspace
                .cloned()
                .unwrap_or_else(|| json!({"id":workspace_id}));
            let output = workspace
                .map(|v| v["outputId"].clone())
                .unwrap_or(Value::Null);
            let urgent = if truthy(&row["is_urgent"]) {
                json!(!truthy(&row["is_focused"]))
            } else {
                json!(false)
            };
            windows.push(json!({"id":key,"windowId":key,"address":key,"appId":app,"class":app,"title":title,"pid":pid,"workspaceId":workspace_id,"workspace":workspace_value,"outputId":output,"monitor":output,"geometry":geometry,"geometryKnown":known,"moving":row["workspace_id"].is_null()&&position.is_none(),"at":[x,y],"size":size,"floating":truthy(&row["is_floating"]),"focused":truthy(&row["is_focused"]),"urgent":urgent,"hidden":false,"pinned":false,"fullscreen":0,"fullscreenMode":0,"fullscreenKnown":false}));
        }
        Ok(
            json!({"type":"snapshot","backend":"niri","capabilities":capabilities(),"monitors":monitors,"workspaces":workspaces,"windows":windows,"activeWindow":windows.iter().find(|row|row["focused"]==true),"activeWorkspace":focused,"focusedMonitor":monitors.iter().find(|row|row["focused"]==true),"overviewOpen":self.overview_open,"overviewHoverSupported":self.overview_hover_supported,"overviewHoveredWindowId":self.overview_hover_id,"configFailed":self.config_failed}),
        )
    }
}
fn text(value: &Value) -> String {
    match value {
        Value::String(v) => v.clone(),
        Value::Bool(v) => if *v { "True" } else { "False" }.into(),
        Value::Null => "None".into(),
        _ => value.to_string(),
    }
}
fn number(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .ok_or_else(|| invalid("Invalid geometry value"))
}
fn at(value: &Value, index: usize) -> Result<&Value> {
    value.get(index).ok_or_else(|| Error {
        kind: "IndexError",
        message: "list index out of range".into(),
    })
}
fn add(values: &[&Value]) -> Result<Value> {
    if values.iter().all(|v| v.as_i64().is_some()) {
        let sum = values
            .iter()
            .try_fold(0_i64, |sum, v| sum.checked_add(v.as_i64().unwrap_or(0)))
            .ok_or_else(|| invalid("Geometry integer range exceeded"))?;
        Ok(json!(sum))
    } else {
        let sum = values
            .iter()
            .try_fold(0.0, |sum, v| number(v).map(|n| sum + n))?;
        if !sum.is_finite() {
            return Err(invalid("Invalid geometry value"));
        }
        Ok(json!(sum))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_frozen_python_event_results_and_snapshots() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-state-reference.json"
        ))?;
        let mut count = 0;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let request = &case["request"];
            let mut state = State::new(
                request["outputs"]
                    .as_object()
                    .ok_or("Missing outputs")?
                    .clone(),
            );
            let events = request["events"].as_array().ok_or("Missing events")?;
            let observations = case["observations"]
                .as_array()
                .ok_or("Missing observations")?;
            assert_eq!(events.len(), observations.len());
            for (event, expected) in events.iter().zip(observations) {
                let (result, code) = match state.apply(event) {
                    Ok(value) => (value, 0),
                    Err(error) => (json!({"error":error.message,"type":error.kind}), 1),
                };
                assert_eq!(
                    crate::json::encode(&result)? + "\n",
                    expected["stdout"].as_str().ok_or("Missing stdout")?,
                    "{} event {}",
                    request["name"],
                    count
                );
                assert_eq!(json!(code), expected["exit"]);
                let snapshot = state.snapshot()?;
                assert_eq!(
                    crate::json::encode(&snapshot)?,
                    crate::json::encode(&expected["snapshot"])?,
                    "{} event {}",
                    request["name"],
                    count
                );
                assert_eq!(json!(state.ready), expected["ready"]);
                count += 1;
            }
        }
        assert_eq!(count, 79);
        Ok(())
    }
}
