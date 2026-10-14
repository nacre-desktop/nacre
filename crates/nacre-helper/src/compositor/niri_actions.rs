//! Semantic niri actions; transport/state providers are shared by IPC and parity.
use super::niri_state::{native_id, Error};
use serde_json::{json, Value};
use std::path::Path;

type Result<T> = std::result::Result<T, Error>;
const MAX_BYTES: usize = 4 * 1024 * 1024;
fn invalid(message: impl Into<String>) -> Error {
    Error {
        kind: "ValueError",
        message: message.into(),
    }
}
fn failure(message: impl Into<String>) -> Error {
    Error {
        kind: "RuntimeError",
        message: message.into(),
    }
}
fn member<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
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
fn finite(value: &Value) -> Result<f64> {
    value
        .as_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("Invalid geometry value"))
}

pub trait Transport {
    fn query_override(
        &mut self,
        _name: &str,
        _timeout: f64,
        _max_bytes: usize,
    ) -> Option<Result<Value>> {
        None
    }
    fn snapshot(&mut self, timeout: f64, max_bytes: usize) -> Result<Value>;
    fn request(&mut self, value: Value, timeout: f64, max_bytes: usize) -> Result<Value>;
    fn window(&mut self, value: &str, timeout: f64) -> Result<Value> {
        if !value.is_empty() {
            native_id(value, "window")?;
        }
        let state = self.snapshot(timeout, MAX_BYTES)?;
        let row = if value.is_empty() {
            state["activeWindow"].clone()
        } else {
            state["windows"]
                .as_array()
                .and_then(|rows| rows.iter().find(|row| row["windowId"] == value))
                .cloned()
                .unwrap_or(Value::Null)
        };
        if !truthy(&row) {
            return Err(invalid("Window no longer exists or no window is focused"));
        }
        Ok(row)
    }
}
pub struct Backend<T> {
    pub transport: T,
}
impl<T: Transport> Backend<T> {
    fn action(&mut self, name: &str, values: Value, timeout: f64) -> Result<Value> {
        let mut body = serde_json::Map::new();
        body.insert(name.into(), values);
        let response = self
            .transport
            .request(json!({"Action":body}), timeout, MAX_BYTES)?;
        if response != "Handled" {
            return Err(invalid("Unexpected niri action reply"));
        }
        Ok(json!({"code":0,"stdout":"Handled","stderr":""}))
    }
    pub fn query_bounded(&mut self, name: &str, timeout: f64, max_bytes: usize) -> Result<Value> {
        if let Some(result) = self.transport.query_override(name, timeout, max_bytes) {
            return result;
        }
        let field = match name {
            "windows" | "monitors" | "workspaces" => name,
            "all_monitors" => "monitors",
            "active_window" => "activeWindow",
            "active_workspace" => "activeWorkspace",
            "cursor" | "instances" => {
                return Err(failure(format!(
                    "niri does not expose {name} through this interface"
                )))
            }
            _ => return Err(invalid("Unknown compositor query")),
        };
        let state = self.transport.snapshot(timeout, max_bytes)?;
        let value = state[field].clone();
        Ok(if truthy(&value) {
            value
        } else if name.starts_with("active_") {
            json!({})
        } else {
            json!([])
        })
    }
    fn query(&mut self, name: &str, timeout: f64) -> Result<Value> {
        self.query_bounded(name, timeout, MAX_BYTES)
    }
    fn window(&mut self, value: &str, timeout: f64) -> Result<Value> {
        self.transport.window(value, timeout)
    }
    fn window_id(&mut self, value: &str, timeout: f64) -> Result<u64> {
        if !value.is_empty() {
            native_id(value, "window")
        } else {
            let row = self.window(value, timeout)?;
            native_id(
                row["windowId"]
                    .as_str()
                    .ok_or_else(|| invalid("Invalid opaque window ID"))?,
                "window",
            )
        }
    }
    pub fn workspace_reference(&mut self, value: &Value, timeout: f64) -> Result<Value> {
        let state = self.transport.snapshot(timeout, MAX_BYTES)?;
        let rows = state["workspaces"]
            .as_array()
            .ok_or_else(|| invalid("Malformed niri records"))?;
        let output = state["focusedMonitor"]
            .get("name")
            .cloned()
            .unwrap_or(Value::Null);
        let matched = if value.is_object() {
            if let Some(offset) = value.get("relative") {
                let active = &state["activeWorkspace"];
                let step = offset
                    .as_i64()
                    .filter(|v| *v == 1 || *v == -1)
                    .ok_or_else(|| invalid("Invalid relative workspace reference"))?;
                if !truthy(active) {
                    return Err(invalid("Invalid relative workspace reference"));
                }
                let index = active["index"]
                    .as_i64()
                    .ok_or_else(|| invalid("Invalid relative workspace reference"))?
                    + step;
                rows.iter().find(|row| {
                    row["outputId"] == active["outputId"] && row["index"].as_i64() == Some(index)
                })
            } else if let Some(id) = value.get("id") {
                let id = id
                    .as_str()
                    .ok_or_else(|| invalid("Invalid opaque workspace ID"))?;
                native_id(id, "workspace")?;
                rows.iter().find(|row| row["id"] == id)
            } else if let Some(name) = value.get("name") {
                rows.iter().find(|row| {
                    row["name"] == *name
                        && (value.get("output").is_none() || row["outputId"] == value["output"])
                })
            } else {
                let index = value["index"]
                    .as_u64()
                    .filter(|v| (1..=255).contains(v))
                    .ok_or_else(|| invalid("Invalid workspace index"))?;
                let output = value.get("output").unwrap_or(&output);
                rows.iter()
                    .find(|row| row["outputId"] == *output && row["index"].as_u64() == Some(index))
            }
        } else if let Some(text) = value.as_str().filter(|v| v.starts_with("niri-workspace:")) {
            native_id(text, "workspace")?;
            rows.iter().find(|row| row["id"] == text)
        } else if value.as_i64().is_some()
            || value.as_u64().is_some()
            || value
                .as_str()
                .is_some_and(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
        {
            let index = if let Some(text) = value.as_str() {
                text.parse::<u64>().ok()
            } else {
                value.as_u64()
            };
            rows.iter().find(|row| {
                row["outputId"] == output && index.is_some() && row["index"].as_u64() == index
            })
        } else if let Some(name) = value
            .as_str()
            .filter(|s| !s.is_empty() && s.chars().count() <= 200)
        {
            rows.iter().find(|row| row["name"] == name)
        } else {
            return Err(invalid("Invalid workspace reference"));
        };
        let row =
            matched.ok_or_else(|| invalid("Workspace does not exist on the requested output"))?;
        Ok(
            json!({"Id":native_id(row["id"].as_str().ok_or_else(||invalid("Invalid opaque workspace ID"))?,"workspace")?}),
        )
    }
    pub fn execute(&mut self, method: &str, args: &[Value], kwargs: &Value) -> Result<Value> {
        let timeout = kwargs
            .get("timeout")
            .and_then(Value::as_f64)
            .unwrap_or(10.0);
        let argument = |index: usize, key: &str| {
            args.get(index)
                .or_else(|| kwargs.get(key))
                .cloned()
                .unwrap_or(Value::Null)
        };
        let window = argument(0, "window");
        let window = window.as_str().unwrap_or("");
        match method {
            "workspace_reference" => self.workspace_reference(&argument(0, "value"), timeout),
            "switch_workspace" => {
                let reference = self.workspace_reference(&argument(0, "workspace"), timeout)?;
                self.action("FocusWorkspace", json!({"reference":reference}), timeout)
            }
            "focus_direction" | "move_direction" => {
                let direction = argument(0, "direction");
                let name = match (method, direction.as_str()) {
                    ("focus_direction", Some("left")) => "FocusColumnLeft",
                    ("focus_direction", Some("right")) => "FocusColumnRight",
                    ("focus_direction", Some("up")) => "FocusWindowUp",
                    ("focus_direction", Some("down")) => "FocusWindowDown",
                    ("move_direction", Some("left")) => "MoveColumnLeft",
                    ("move_direction", Some("right")) => "MoveColumnRight",
                    ("move_direction", Some("up")) => "MoveWindowUp",
                    ("move_direction", Some("down")) => "MoveWindowDown",
                    _ => {
                        return Err(invalid(if method == "focus_direction" {
                            "Invalid focus direction"
                        } else {
                            "Invalid move direction"
                        }))
                    }
                };
                self.action(name, json!({}), timeout)
            }
            "cycle_window" => {
                let direction = args
                    .first()
                    .or_else(|| kwargs.get("direction"))
                    .and_then(Value::as_str)
                    .unwrap_or("next");
                if !["next", "previous"].contains(&direction) {
                    return Err(invalid("Invalid cycle direction"));
                }
                let state = self.transport.snapshot(timeout, MAX_BYTES)?;
                let active = &state["activeWorkspace"];
                let mut windows = state["windows"]
                    .as_array()
                    .ok_or_else(|| invalid("Malformed niri records"))?
                    .iter()
                    .filter(|row| truthy(active) && row["workspaceId"] == active["id"])
                    .collect::<Vec<_>>();
                let mut ordered = windows
                    .iter()
                    .map(|row| {
                        Ok((
                            native_id(
                                row["windowId"]
                                    .as_str()
                                    .ok_or_else(|| invalid("Invalid opaque window ID"))?,
                                "window",
                            )?,
                            *row,
                        ))
                    })
                    .collect::<Result<Vec<_>>>()?;
                ordered.sort_by_key(|(id, _)| *id);
                windows = ordered.into_iter().map(|(_, row)| row).collect();
                if windows.is_empty() {
                    return Err(invalid("No windows in the active workspace"));
                }
                let current = &state["activeWindow"];
                let index = windows
                    .iter()
                    .position(|row| truthy(current) && row["windowId"] == current["windowId"])
                    .map(|i| i as i64)
                    .unwrap_or(-1);
                let index = (index + if direction == "next" { 1 } else { -1 })
                    .rem_euclid(windows.len() as i64) as usize;
                let id = native_id(
                    windows[index]["windowId"]
                        .as_str()
                        .ok_or_else(|| invalid("Invalid opaque window ID"))?,
                    "window",
                )?;
                self.action("FocusWindow", json!({"id":id}), timeout)
            }
            "focus_window" | "close_window" => {
                let id = self.window_id(window, timeout)?;
                self.action(
                    if method == "focus_window" {
                        "FocusWindow"
                    } else {
                        "CloseWindow"
                    },
                    json!({"id":id}),
                    timeout,
                )
            }
            "screenshot_window" => {
                let path = argument(0, "path");
                let path = path
                    .as_str()
                    .ok_or_else(|| invalid("Screenshot path must be absolute"))?;
                if !Path::new(path).is_absolute() {
                    return Err(invalid("Screenshot path must be absolute"));
                }
                let window = argument(1, "window");
                let id = self.window_id(window.as_str().unwrap_or(""), timeout)?;
                self.action(
                    "ScreenshotWindow",
                    json!({"id":id,"path":path,"write_to_disk":true,"show_pointer":false}),
                    timeout,
                )
            }
            "move_workspace" => {
                let monitor = argument(1, "monitor");
                let outputs = self.query("monitors", timeout)?;
                if !outputs
                    .as_array()
                    .is_some_and(|rows| rows.iter().any(|row| row["name"] == monitor))
                {
                    return Err(invalid("Unknown output"));
                }
                let reference = self.workspace_reference(&argument(0, "workspace"), timeout)?;
                self.action(
                    "MoveWorkspaceToMonitor",
                    json!({"reference":reference,"output":monitor}),
                    timeout,
                )
            }
            "float_window" => {
                let action = kwargs
                    .get("action")
                    .and_then(Value::as_str)
                    .unwrap_or("toggle");
                let name = match action {
                    "toggle" => "ToggleWindowFloating",
                    "enable" | "set" => "MoveWindowToFloating",
                    "disable" | "unset" => "MoveWindowToTiling",
                    _ => return Err(invalid("Invalid floating action")),
                };
                let id = self.window_id(window, timeout)?;
                self.action(name, json!({"id":id}), timeout)
            }
            "fullscreen_window" => {
                let mode = kwargs
                    .get("mode")
                    .and_then(Value::as_str)
                    .unwrap_or("fullscreen");
                if !["fullscreen", "maximized"].contains(&mode) {
                    return Err(invalid("Invalid fullscreen mode"));
                }
                let id = self.window_id(window, timeout)?;
                self.action(
                    if mode == "fullscreen" {
                        "FullscreenWindow"
                    } else {
                        "MaximizeWindowToEdges"
                    },
                    json!({"id":id}),
                    timeout,
                )
            }
            "resize_window" => {
                let width = argument(1, "width");
                let height = argument(2, "height");
                let relative = kwargs.get("relative").is_some_and(truthy);
                for value in [&width, &height] {
                    if finite(value)? <= 0.0 && !relative {
                        return Err(invalid("Invalid window size"));
                    }
                }
                let id = self.window_id(window, timeout)?;
                let change = if relative { "AdjustFixed" } else { "SetFixed" };
                let values = |number: f64| {
                    let mut result = serde_json::Map::new();
                    result.insert(change.into(), json!(number.round_ties_even() as i64));
                    Value::Object(result)
                };
                self.action(
                    "SetWindowWidth",
                    json!({"id":id,"change":values(finite(&width)?)}),
                    timeout,
                )?;
                self.action(
                    "SetWindowHeight",
                    json!({"id":id,"change":values(finite(&height)?)}),
                    timeout,
                )
            }
            "move_window" => self.move_window(window, kwargs, timeout),
            "place_window" => {
                finite(&argument(1, "x"))?;
                finite(&argument(2, "y"))?;
                let width = kwargs.get("width").filter(|v| !v.is_null());
                let height = kwargs.get("height").filter(|v| !v.is_null());
                if width.is_some() != height.is_some() {
                    return Err(invalid("Specify both size dimensions"));
                }
                if let (Some(width), Some(height)) = (width, height) {
                    if finite(width)? <= 0.0 || finite(height)? <= 0.0 {
                        return Err(invalid("Invalid window size"));
                    }
                }
                self.execute(
                    "float_window",
                    &[json!(window)],
                    &json!({"action":"enable","timeout":timeout}),
                )?;
                if let (Some(width), Some(height)) = (width, height) {
                    self.execute(
                        "resize_window",
                        &[json!(window), width.clone(), height.clone()],
                        &json!({"timeout":timeout}),
                    )?;
                }
                self.move_window(window, kwargs, timeout)
            }
            "dpms" => {
                if args
                    .get(1)
                    .or_else(|| kwargs.get("monitor"))
                    .is_some_and(|v| !v.is_null())
                {
                    return Err(failure("niri DPMS controls all outputs together"));
                }
                let action = argument(0, "action");
                let name = match action.as_str() {
                    Some("enable" | "on") => "PowerOnMonitors",
                    Some("disable" | "off") => "PowerOffMonitors",
                    _ => return Err(invalid("Invalid DPMS action")),
                };
                self.action(name, json!({}), timeout)
            }
            "logout" => self.action("Quit", json!({"skip_confirmation":true}), timeout),
            "reload" => self.action("LoadConfigFile", json!({"path":null}), timeout),
            "configure_monitors" => self.configure_monitors(&argument(0, "records"), timeout),
            "toggle_workspace_floating" => {
                let state = self.transport.snapshot(10.0, MAX_BYTES)?;
                let active = &state["activeWorkspace"];
                let rows = state["windows"]
                    .as_array()
                    .ok_or_else(|| invalid("Malformed niri records"))?
                    .iter()
                    .filter(|row| truthy(active) && row["workspaceId"] == active["id"])
                    .collect::<Vec<_>>();
                let floating = rows.iter().any(|row| !truthy(&row["floating"]));
                for row in rows {
                    self.execute(
                        "float_window",
                        &[row["windowId"].clone()],
                        &json!({"action":if floating{"enable"}else{"disable"}}),
                    )?;
                }
                Ok(Value::Null)
            }
            "pin_window" | "kill_window" | "fullscreen_state" | "set_cursor" | "notify"
            | "set_animations" | "set_performance" | "set_options" | "render_options"
            | "apply_config" => Err(failure(
                "Operation is not supported by the niri backend; check compositor capabilities",
            )),
            _ => Err(invalid(format!("Unknown compositor action: {method}"))),
        }
    }
    fn configure_monitors(&mut self, records: &Value, timeout: f64) -> Result<Value> {
        super::niri_config::monitors(records)?;
        let records = records
            .as_array()
            .ok_or_else(|| invalid("Invalid output geometry"))?;
        let previous = self.query("monitors", timeout)?;
        let previous = previous
            .as_array()
            .ok_or_else(|| invalid("Malformed niri records"))?;
        if records
            .iter()
            .any(|row| !previous.iter().any(|old| old["name"] == row["name"]))
        {
            return Err(invalid("Selected display is disconnected"));
        }
        let actions = |row: &Value| -> Result<Vec<Value>> {
            let transforms = [
                "Normal",
                "90",
                "180",
                "270",
                "Flipped",
                "Flipped90",
                "Flipped180",
                "Flipped270",
            ];
            let integer = |key: &str| -> Result<Value> {
                let value = &row[key];
                if value.as_i64().is_some() || value.as_u64().is_some() {
                    return Ok(value.clone());
                }
                let value = finite(value)?.trunc();
                if value < i64::MIN as f64 || value >= -(i64::MIN as f64) {
                    return Err(invalid("Output integer range exceeded"));
                }
                Ok(json!(value as i64))
            };
            let mut result = Vec::new();
            if row.get("refreshRate").is_some_and(truthy) {
                result.push(json!({"Mode":{"mode":{"Specific":{"width":integer("width")?,"height":integer("height")?,"refresh":row["refreshRate"]}}}}));
            }
            let transform = row.get("transform").and_then(Value::as_f64).unwrap_or(0.0) as usize;
            result.extend([
                json!({"Scale":{"scale":{"Specific":row["scale"]}}}),
                json!({"Transform":{"transform":transforms.get(transform).ok_or_else(||invalid("Invalid output geometry"))?}}),
                json!({"Position":{"position":{"Specific":{"x":integer("x")?,"y":integer("y")?}}}}),
            ]);
            Ok(result)
        };
        let applied = json!({"OutputConfigChanged":"Applied"});
        let operation = (|| {
            for row in records {
                for action in actions(row)? {
                    if self.transport.request(
                        json!({"Output":{"output":row["name"],"action":action}}),
                        timeout,
                        MAX_BYTES,
                    )? != applied
                    {
                        return Err(failure("Output configuration was not applied"));
                    }
                }
            }
            Ok(())
        })();
        if let Err(error) = operation {
            for row in records {
                let previous = previous
                    .iter()
                    .find(|old| old["name"] == row["name"])
                    .ok_or_else(|| invalid("Selected display is disconnected"))?;
                for action in actions(previous)? {
                    let response = self.transport.request(
                        json!({"Output":{"output":row["name"],"action":action}}),
                        timeout,
                        MAX_BYTES,
                    )?;
                    if response != applied
                        && response != json!({"OutputConfigChanged":"OutputWasMissing"})
                    {
                        return Err(failure(
                            "Output change failed and rollback was not accepted",
                        ));
                    }
                }
            }
            return Err(error);
        }
        Ok(json!({"code":0,"stdout":"ok\n","stderr":""}))
    }
    fn move_window(&mut self, window: &str, kwargs: &Value, timeout: f64) -> Result<Value> {
        if let Some(workspace) = kwargs.get("workspace").filter(|v| !v.is_null()) {
            if kwargs.get("x").is_some_and(|v| !v.is_null())
                || kwargs.get("y").is_some_and(|v| !v.is_null())
            {
                return Err(invalid("Specify a workspace or geometry, not both"));
            }
            // Overview keeps a layout selection but clears application keyboard focus.
            // Let niri resolve that selection when the caller did not specify a window.
            let id = if window.is_empty() {
                Value::Null
            } else {
                json!(self.window_id(window, timeout)?)
            };
            let reference = self.workspace_reference(workspace, timeout)?;
            return self.action("MoveWindowToWorkspace",json!({"window_id":id,"reference":reference,"focus":kwargs.get("follow").is_some_and(truthy)}),timeout);
        }
        let x = member(kwargs, "x")?;
        let y = member(kwargs, "y")?;
        finite(x)?;
        finite(y)?;
        let mut row = self.window(window, timeout)?;
        if !truthy(&row["floating"]) {
            return Err(invalid("Absolute placement requires a floating window"));
        }
        let relative = kwargs.get("relative").is_some_and(truthy);
        let (mut dx, mut dy) = (x.clone(), y.clone());
        if !relative {
            let outputs = self.query("monitors", timeout)?;
            let mut target = None;
            for out in outputs
                .as_array()
                .ok_or_else(|| invalid("Malformed niri records"))?
            {
                let left = finite(&out["x"])?;
                if left <= finite(x)? && finite(x)? < left + finite(&out["logicalWidth"])? {
                    let top = finite(&out["y"])?;
                    if top <= finite(y)? && finite(y)? < top + finite(&out["logicalHeight"])? {
                        target = Some(out);
                        break;
                    }
                }
            }
            if let Some(target) = target.filter(|out| out["name"] != row["outputId"]) {
                let id = native_id(
                    row["windowId"]
                        .as_str()
                        .ok_or_else(|| invalid("Invalid opaque window ID"))?,
                    "window",
                )?;
                self.action(
                    "MoveWindowToMonitor",
                    json!({"id":id,"output":target["name"]}),
                    timeout,
                )?;
                row = self.window(
                    row["windowId"]
                        .as_str()
                        .ok_or_else(|| invalid("Invalid opaque window ID"))?,
                    timeout,
                )?;
            }
            if !truthy(&row["geometryKnown"]) {
                return Err(failure("Floating window position is not yet available"));
            }
            dx = subtract(x, &row["geometry"]["x"])?;
            dy = subtract(y, &row["geometry"]["y"])?;
        }
        let id = native_id(
            row["windowId"]
                .as_str()
                .ok_or_else(|| invalid("Invalid opaque window ID"))?,
            "window",
        )?;
        self.action(
            "MoveFloatingWindow",
            json!({"id":id,"x":{"AdjustFixed":dx},"y":{"AdjustFixed":dy}}),
            timeout,
        )
    }
}
fn subtract(a: &Value, b: &Value) -> Result<Value> {
    if let (Some(a), Some(b)) = (a.as_i64(), b.as_i64()) {
        Ok(json!(a.checked_sub(b).ok_or_else(|| invalid(
            "Geometry integer range exceeded"
        ))?))
    } else {
        Ok(json!(finite(a)? - finite(b)?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_monitor_transaction_requests_and_rollback_failures() -> crate::Result<()> {
        struct Transaction {
            snapshot: Value,
            input: Value,
            requests: Vec<Value>,
            probes: Vec<Value>,
        }
        impl Transport for Transaction {
            fn snapshot(&mut self, timeout: f64, max_bytes: usize) -> Result<Value> {
                self.probes
                    .push(json!({"timeout":timeout as i64,"max_bytes":max_bytes}));
                Ok(self.snapshot.clone())
            }
            fn request(&mut self, request: Value, timeout: f64, max_bytes: usize) -> Result<Value> {
                self.requests.push(
                    json!({"request":request,"timeout":timeout as i64,"max_bytes":max_bytes}),
                );
                let index = self.requests.len() as u64;
                let at = self.input["failure_at"].as_u64();
                let response = if at == Some(index) {
                    self.input["failure"].as_str().unwrap_or("Applied")
                } else if at.is_some_and(|at| index > at) {
                    self.input["rollback"].as_str().unwrap_or("Applied")
                } else {
                    "Applied"
                };
                match response {
                    "error" => Err(failure("fixture IPC failure")),
                    "unexpected" => Ok(json!("unexpected")),
                    response => Ok(json!({"OutputConfigChanged":response})),
                }
            }
        }
        let states: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-state-reference.json"
        ))?;
        let snapshot = states["cases"]
            .as_array()
            .ok_or("Missing states")?
            .iter()
            .find(|case| case["request"]["name"] == "initial-u64-output-relative")
            .ok_or("Missing initial state")?["observations"]
            .as_array()
            .ok_or("Missing observations")?
            .last()
            .ok_or("Missing snapshot")?["snapshot"]
            .clone();
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-monitor-transaction-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let input = &case["request"];
            let mut backend = Backend {
                transport: Transaction {
                    snapshot: snapshot.clone(),
                    input: input.clone(),
                    requests: vec![],
                    probes: vec![],
                },
            };
            let result = backend.execute(
                "configure_monitors",
                input["args"].as_array().ok_or("Missing args")?,
                &input["kwargs"],
            );
            let (result, exit) = match result {
                Ok(value) => (value, 0),
                Err(error) => (json!({"error":error.message,"type":error.kind}), 1),
            };
            assert_eq!(
                crate::json::encode(&result)? + "\n",
                case["observed"]["stdout"],
                "{}",
                input["name"]
            );
            assert_eq!(json!(exit), case["observed"]["exit"], "{}", input["name"]);
            assert_eq!(
                json!(backend.transport.requests),
                case["observed"]["requests"],
                "{}",
                input["name"]
            );
            assert_eq!(
                json!(backend.transport.probes),
                case["observed"]["probes"],
                "{}",
                input["name"]
            );
        }
        Ok(())
    }
    struct Fixture {
        snapshot: Value,
        probes: Vec<Value>,
        requests: Vec<Value>,
        move_output: bool,
    }
    impl Transport for Fixture {
        fn snapshot(&mut self, timeout: f64, max_bytes: usize) -> Result<Value> {
            self.probes.push(json!({"timeout":if timeout.fract()==0.0{json!(timeout as i64)}else{json!(timeout)},"max_bytes":max_bytes}));
            Ok(self.snapshot.clone())
        }
        fn request(&mut self, value: Value, timeout: f64, max_bytes: usize) -> Result<Value> {
            self.requests.push(json!({"request":value,"timeout":if timeout.fract()==0.0{json!(timeout as i64)}else{json!(timeout)},"max_bytes":max_bytes}));
            if self.move_output && value["Action"].get("MoveWindowToMonitor").is_some() {
                if let Some(windows) = self.snapshot["windows"].as_array_mut() {
                    for window in windows {
                        if window["windowId"] == "niri-window:9007199254740995" {
                            window["outputId"] = json!("DP-1");
                            window["geometry"]["x"] = json!(-1892);
                            window["geometry"]["y"] = json!(44);
                        }
                    }
                }
                self.snapshot["activeWindow"]["outputId"] = json!("DP-1");
                self.snapshot["activeWindow"]["geometry"]["x"] = json!(-1892);
                self.snapshot["activeWindow"]["geometry"]["y"] = json!(44);
            }
            Ok(json!("Handled"))
        }
    }
    #[test]
    fn workspace_move_uses_niri_selection_without_application_focus() -> crate::Result<()> {
        for (workspace, reference) in [
            (json!(2), json!({"Id": 202})),
            (json!({"relative": 1}), json!({"Id": 202})),
        ] {
            let mut backend = Backend {
                transport: Fixture {
                    snapshot: json!({
                        "activeWindow": null,
                        "windows": [],
                        "focusedMonitor": {"name": "eDP-1"},
                        "activeWorkspace": {"outputId": "eDP-1", "index": 1},
                        "workspaces": [{"id": "niri-workspace:202", "outputId": "eDP-1", "index": 2}]
                    }),
                    probes: vec![],
                    requests: vec![],
                    move_output: false,
                },
            };
            backend.execute("move_window", &[], &json!({"workspace": workspace}))?;
            assert_eq!(
                backend.transport.requests[0]["request"],
                json!({"Action": {
                    "MoveWindowToWorkspace": {"window_id": null, "reference": reference, "focus": false}
                }})
            );
            assert_eq!(backend.transport.probes.len(), 1);
        }
        Ok(())
    }
    #[test]
    fn frozen_current_python_action_results_and_ipc_requests() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-actions-reference.json"
        ))?;
        let states: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-state-reference.json"
        ))?;
        let initial = states["cases"]
            .as_array()
            .ok_or("Missing states")?
            .iter()
            .find(|case| case["request"]["name"] == "initial-u64-output-relative")
            .ok_or("Missing initial snapshot")?["observations"]
            .as_array()
            .ok_or("Missing observations")?
            .last()
            .ok_or("Missing snapshot")?["snapshot"]
            .clone();
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let input = &case["request"];
            let mut backend = Backend {
                transport: Fixture {
                    snapshot: initial.clone(),
                    probes: vec![],
                    requests: vec![],
                    move_output: input.get("move_output").is_some(),
                },
            };
            let (value, exit) = match backend.execute(
                input["method"].as_str().ok_or("Missing method")?,
                input["args"].as_array().ok_or("Missing args")?,
                &input["kwargs"],
            ) {
                Ok(value) => (value, 0),
                Err(error) => (json!({"error":error.message,"type":error.kind}), 1),
            };
            let expected = &case["observed"];
            assert_eq!(
                crate::json::encode(&value)? + "\n",
                expected["stdout"].as_str().ok_or("Missing stdout")?,
                "{}",
                input["name"]
            );
            assert_eq!(json!(exit), expected["exit"], "{}", input["name"]);
            assert_eq!(
                backend.transport.probes,
                expected["probes"]
                    .as_array()
                    .ok_or("Missing probes")?
                    .clone(),
                "{} probes",
                input["name"]
            );
            assert_eq!(
                backend.transport.requests,
                expected["requests"]
                    .as_array()
                    .ok_or("Missing requests")?
                    .clone(),
                "{} requests",
                input["name"]
            );
        }
        assert_eq!(
            reference["cases"].as_array().ok_or("Missing cases")?.len(),
            78
        );
        Ok(())
    }
}
