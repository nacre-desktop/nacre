//! Hyprland semantic encoding; transport is a socket boundary, never hyprctl.
use super::niri_state::Error;
use serde_json::{json, Map, Value};
type Result<T> = std::result::Result<T, Error>;
fn invalid(message: impl Into<String>) -> Error {
    Error {
        kind: "ValueError",
        message: message.into(),
    }
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}
fn python_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        _ => value.to_string(),
    }
}
fn lua(value: &Value) -> Result<String> {
    crate::compositor_config::runtime_lua_value(value).map_err(|error| invalid(error.message))
}
fn table(values: &Value) -> Result<String> {
    let values = values
        .as_object()
        .ok_or_else(|| invalid("Invalid compositor field"))?;
    let key = regex::Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$")
        .map_err(|error| invalid(error.to_string()))?;
    let mut fields = Vec::new();
    for (name, value) in values {
        fields.push(format!(
            "{}={}",
            if key.is_match(name) {
                name.clone()
            } else {
                format!("[{}]", lua(&json!(name))?)
            },
            lua(value)?
        ));
    }
    Ok(format!("{{{}}}", fields.join(",")))
}
pub fn window_selector(value: &Value) -> Result<String> {
    let mut value = if truthy(value) {
        python_text(value)
    } else {
        String::new()
    };
    if value.chars().count() > 1024 || value.chars().any(|char| (char as u32) < 32) {
        return Err(invalid("Invalid window ID"));
    }
    if value.starts_with("0x") {
        value = "address:".to_owned() + &value;
    }
    let pattern =
        regex::Regex::new(r"^(address:0x[0-9a-fA-F]+|class:.+|title:.+|pid:[0-9]+|active)$")
            .map_err(|error| invalid(error.to_string()))?;
    if !value.is_empty() && !pattern.is_match(&value) {
        return Err(invalid("Invalid Hyprland window ID"));
    }
    Ok(value)
}
pub fn workspace_reference(value: &Value) -> Result<Value> {
    let value = if let Some(object) = value.as_object() {
        if object.contains_key("output")
            && !object.contains_key("id")
            && !object.contains_key("name")
        {
            return Err(invalid(
                "Output-relative workspace indices are not supported by this backend",
            ));
        }
        object
            .get("id")
            .or_else(|| object.get("name"))
            .or_else(|| object.get("index"))
            .unwrap_or(&Value::Null)
    } else {
        value
    };
    if !(value.is_string() || value.as_i64().is_some() || value.as_u64().is_some())
        || python_text(value).is_empty()
        || python_text(value).chars().count() > 200
        || python_text(value).chars().any(|char| (char as u32) < 32)
    {
        return Err(invalid("Invalid workspace reference"));
    }
    Ok(value.clone())
}
pub trait Transport {
    fn run(&mut self, args: Vec<String>, timeout: f64, check: bool) -> Result<Value>;
    fn run_default(&mut self, args: Vec<String>) -> Result<Value> {
        self.run(args, 10.0, true)
    }
}
pub const LEGACY_POWER_BINDING:&str="-- Run after release/input processing so this key does not immediately wake DPMS.\nhl.bind(\"XF86PowerOff\", hl.dsp.exec_cmd(\"sleep 0.2; ~/.config/hypr/scripts/hyprctl-lua.sh dpms disable\"), {\n    release = true,\n    locked = true,\n})\n";
const OPTIONS: &[(&str, &str)] = &[
    ("animations", "animations.enabled"),
    ("blur", "decoration.blur.enabled"),
    ("shadow", "decoration.shadow.enabled"),
    ("followMouse", "input.follow_mouse"),
    ("naturalScroll", "input.touchpad.natural_scroll"),
    ("gapsIn", "general.gaps_in"),
    ("gapsOut", "general.gaps_out"),
    ("borderSize", "general.border_size"),
    ("rounding", "decoration.rounding"),
    ("keyRepeatDelay", "input.repeat_delay"),
    ("keyRepeatRate", "input.repeat_rate"),
    ("mouseSpeed", "input.sensitivity"),
    ("mouseAcceleration", "input.accel_profile"),
    ("mouseLeftHanded", "input.left_handed"),
    ("mouseNaturalScroll", "input.natural_scroll"),
    ("touchpadTap", "input.touchpad.tap-to-click"),
    ("touchpadTypingGuard", "input.touchpad.disable_while_typing"),
];
pub fn render_options(options: &Value) -> Result<String> {
    let mut fields = Vec::new();
    for (key, value) in options
        .as_object()
        .ok_or_else(|| invalid("Invalid compositor field"))?
    {
        fields.push(format!("[{}]={}", lua(&json!(key))?, lua(value)?));
    }
    Ok(format!("hl.config({{{}}})\n", fields.join(",")))
}
pub fn performance_options() -> Value {
    json!({"animations":{"enabled":false},"decoration":{"shadow":{"enabled":false},"blur":{"enabled":false},"active_opacity":1,"inactive_opacity":1,"fullscreen_opacity":1,"rounding":0},"general":{"gaps_in":0,"gaps_out":0,"border_size":1}})
}
pub fn render_desktop(preferences: &Value) -> Result<String> {
    let mut options = Map::new();
    for (name, key) in OPTIONS {
        if let Some(value) = preferences.get(*name) {
            options.insert(
                (*key).into(),
                if *name == "followMouse" {
                    json!(if truthy(value) { 1 } else { 0 })
                } else {
                    value.clone()
                },
            );
        }
    }
    let mut result = render_options(&Value::Object(options))?;
    if preferences.get("keyboardLayouts").is_some() || preferences.get("touchpadEnabled").is_some()
    {
        result.push_str("-- Nacre software input begin\n");
        if let Some(layout) = preferences.get("keyboardLayouts") {
            result.push_str(&render_keyboard(&json!({"layout":layout}))?);
        }
        if let Some(enabled) = preferences.get("touchpadEnabled").and_then(Value::as_bool) {
            if let Some(devices) = preferences.get("touchpadDevices").and_then(Value::as_array) {
                for device in devices {
                    result.push_str(&format!(
                        "hl.device({{name={},enabled={enabled}}})\n",
                        lua(device)?
                    ));
                }
            }
        }
        result.push_str("-- Nacre software input end\n");
    }
    Ok(result)
}
pub fn render_keyboard(values: &Value) -> Result<String> {
    let mut rows = vec![
        "-- Your system keyboard choice. Edit here; Nacre updates preserve this file.".into(),
        "hl.config({".into(),
    ];
    for (key, value) in values
        .as_object()
        .ok_or_else(|| invalid("Invalid compositor field"))?
    {
        rows.push(format!("    [\"input.kb_{key}\"] = {},", lua(value)?));
    }
    rows.push("})".into());
    Ok(rows.join("\n") + "\n")
}
pub fn render_monitors(monitors: &Value) -> Result<String> {
    let mut lines = Vec::new();
    for monitor in monitors
        .as_array()
        .ok_or_else(|| invalid("Invalid output geometry"))?
    {
        if monitor.get("output").is_some() {
            lines.push(format!("hl.monitor({})", table(monitor)?));
            continue;
        }
        let get = |name: &str| {
            monitor.get(name).ok_or_else(|| Error {
                kind: "KeyError",
                message: format!("'{name}'"),
            })
        };
        let number = |name: &str| -> Result<f64> {
            get(name)?
                .as_f64()
                .filter(|v| v.is_finite())
                .ok_or_else(|| invalid("Invalid output geometry"))
        };
        let integer = |name: &str| -> Result<String> {
            let value = get(name)?;
            if value.as_i64().is_some() || value.as_u64().is_some() {
                Ok(value.to_string())
            } else {
                Ok(format!("{:.0}", number(name)?.trunc()))
            }
        };
        let scale = number("scale")?;
        let scale = if scale.fract() == 0.0 {
            format!("{scale:.1}")
        } else {
            scale.to_string()
        };
        let mirror = monitor
            .get("mirrorOf")
            .filter(|value| truthy(value) && **value != "none")
            .cloned()
            .unwrap_or(json!(""));
        lines.push(format!(
            "hl.monitor({{output={},mode={},scale={scale},position={},transform={},mirror={}}})",
            lua(get("name")?)?,
            lua(&json!(format!(
                "{}x{}@{:.3}",
                integer("width")?,
                integer("height")?,
                number("refreshRate")?
            )))?,
            lua(&json!(format!("{}x{}", integer("x")?, integer("y")?)))?,
            monitor
                .get("transform")
                .filter(|value| truthy(value))
                .map(python_text)
                .unwrap_or("0".into()),
            lua(&mirror)?
        ));
    }
    Ok(lines.join("\n"))
}
pub fn parse_event(line: &str) -> Value {
    let (name, data) = line.split_once(">>").unwrap_or((line, ""));
    let kind = if name.starts_with("monitoradded") || name.starts_with("monitorremoved") {
        "outputs-changed"
    } else if ["workspace", "focusedmon", "moveworkspace", "activespecial"]
        .iter()
        .any(|prefix| name.starts_with(prefix))
    {
        "workspace-monitor-change"
    } else {
        match name {
            "activewindowv2" => "window-focus",
            "urgent" => "window-urgent",
            "openwindow" => "window-open",
            "closewindow" => "window-close",
            "movewindow" | "movewindowv2" => "window-workspace",
            "windowtitle" | "windowtitlev2" => "window-title",
            "changefloatingmode" => "window-floating",
            "fullscreen" => "window-fullscreen",
            "pin" => "window-pin",
            "configreloaded" => "configuration-reloaded",
            _ => "other",
        }
    };
    let mut event = Map::new();
    event.insert("type".into(), json!(kind));
    if kind.starts_with("window-") && name != "fullscreen" {
        let window = data.split(',').next().unwrap_or("");
        let candidate = if window.starts_with("0x") {
            window.to_owned()
        } else {
            format!("0x{window}")
        };
        if regex::Regex::new(r"^0x[0-9a-fA-F]+$").is_ok_and(|pattern| pattern.is_match(&candidate))
        {
            event.insert("windowId".into(), json!(candidate));
        }
    }
    Value::Object(event)
}
pub fn capabilities() -> Value {
    json!({"stableWorkspaceIndices":true,"globalWorkspaces":true,"perOutputWorkspaces":false,"scrollingLayout":false,"floatingPlacement":true,"pinWindow":true,"windowCapture":true,"windowHide":true,"windowTrash":true,"scratchpad":true,"windowGroups":true,"windowForceKill":true,"pseudoTiling":true,"layoutSplit":true,"keymaps":true,"nonConsumingShortcuts":true,"bindingIntrospection":true,"monitorMirroring":true,"fullscreenState":true})
}
pub fn valid_window_id(value: &Value) -> bool {
    value.as_str().is_some_and(|value| {
        !value.strip_prefix("0x").unwrap_or("").is_empty()
            && value.starts_with("0x")
            && value[2..].bytes().all(|byte| byte.is_ascii_hexdigit())
    })
}
pub fn ensure_desktop_loader(home: &std::path::Path) -> Result<()> {
    let text =
        std::fs::read_to_string(home.join(".config/hypr/hyprland.lua")).map_err(|error| Error {
            kind: "OSError",
            message: error.to_string(),
        })?;
    if !text.contains("-- Nacre desktop settings") || !text.contains("load_private(\"desktop\")") {
        return Err(Error {
            kind: "RuntimeError",
            message: "Managed desktop loader missing; run ./install.sh --apply from the repository"
                .into(),
        });
    }
    Ok(())
}
pub fn binding_source_files(
    home: &std::path::Path,
    nacre_config: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>> {
    let mut paths = Vec::new();
    match std::fs::read_dir(home.join(".config/hypr/conf")) {
        Ok(entries) => {
            for entry in entries {
                let path = entry
                    .map_err(|error| Error {
                        kind: "OSError",
                        message: error.to_string(),
                    })?
                    .path();
                if path.extension().is_some_and(|ext| ext == "lua") {
                    paths.push(path);
                }
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            return Err(Error {
                kind: "OSError",
                message: error.to_string(),
            })
        }
    }
    paths.sort();
    paths.push(nacre_config.join("shortcuts.lua"));
    let pattern =
        regex::Regex::new(r"\bhl\.bind\s*\(").map_err(|error| invalid(error.to_string()))?;
    let mut bindings = Vec::new();
    for path in paths {
        if path.exists() {
            let text = std::fs::read_to_string(&path).map_err(|error| Error {
                kind: "OSError",
                message: error.to_string(),
            })?;
            if pattern.is_match(&text) {
                bindings.push(path);
            }
        }
    }
    Ok(bindings)
}
pub fn capture_bindings(
    path: &std::path::Path,
    kind: &str,
    program: Option<&std::path::Path>,
    lua_capture: &std::path::Path,
) -> Result<Value> {
    if !["bindings", "rules", "declarations"].contains(&kind) {
        return Err(invalid("Unsupported declaration kind"));
    }
    let program = program.ok_or_else(|| Error {
        kind: "RuntimeError",
        message: "Lua required for declarative binding checks".into(),
    })?;
    let output = crate::process::run(
        program
            .to_str()
            .ok_or_else(|| invalid("Lua program path is not UTF-8"))?,
        &[
            lua_capture
                .to_str()
                .ok_or_else(|| invalid("Lua capture path is not UTF-8"))?
                .into(),
            path.to_str()
                .ok_or_else(|| invalid("Binding path is not UTF-8"))?
                .into(),
            kind.into(),
        ],
        None,
        std::time::Duration::from_secs(10),
    )
    .map_err(|error| Error {
        kind: "RuntimeError",
        message: error.to_string(),
    })?;
    if !output.status.success() {
        return Err(Error {
            kind: "RuntimeError",
            message: format!(
                "Cannot capture bindings in {}: {}",
                path.file_name().unwrap_or_default().to_string_lossy(),
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| Error {
        kind: "UnicodeDecodeError",
        message: error.to_string(),
    })?;
    let value = crate::json::parse_value(&stdout).map_err(|error| Error {
        kind: "JSONDecodeError",
        message: error.to_string(),
    })?;
    Ok(if value.is_array() { value } else { json!([]) })
}
pub fn window_job_token(value: &Value) -> Result<String> {
    if !valid_window_id(value) {
        return Err(invalid("Invalid window ID"));
    }
    Ok(value.as_str().ok_or_else(|| invalid("Invalid window ID"))?[2..].into())
}
pub fn night_light_command(enabled: bool) -> Vec<String> {
    let mut command = vec!["hyprctl".into(), "hyprsunset".into()];
    command.extend(if enabled {
        vec!["temperature".into(), "4500".into()]
    } else {
        vec!["identity".into()]
    });
    command
}
pub struct Backend<T> {
    pub transport: T,
}
fn normalize_window(value: Value) -> Result<Value> {
    if !truthy(&value) {
        return Ok(value);
    }
    let mut row = value
        .as_object()
        .cloned()
        .ok_or_else(|| invalid("Malformed compositor query record"))?;
    let value = Value::Object(row.clone());
    let geometry = |key: &str, index: usize| -> Result<Value> {
        match value.get(key) {
            None => Ok(json!(0)),
            Some(array) => array.get(index).cloned().ok_or_else(|| Error {
                kind: "IndexError",
                message: "list index out of range".into(),
            }),
        }
    };
    for (key, item) in [
        (
            "windowId",
            value.get("address").cloned().unwrap_or(json!("")),
        ),
        ("id", value.get("address").cloned().unwrap_or(json!(""))),
        ("appId", value.get("class").cloned().unwrap_or(json!(""))),
        (
            "workspaceId",
            value
                .get("workspace")
                .and_then(|workspace| workspace.get("id"))
                .cloned()
                .unwrap_or(Value::Null),
        ),
        ("hidden", json!(value.get("hidden").is_some_and(truthy))),
        ("pinned", json!(value.get("pinned").is_some_and(truthy))),
        (
            "fullscreenMode",
            value.get("fullscreen").cloned().unwrap_or(json!(0)),
        ),
        (
            "outputId",
            value.get("monitor").cloned().unwrap_or(Value::Null),
        ),
        (
            "geometry",
            json!({"x":geometry("at",0)?,"y":geometry("at",1)?,"width":geometry("size",0)?,"height":geometry("size",1)?}),
        ),
    ] {
        row.insert(key.into(), item);
    }
    Ok(Value::Object(row))
}
impl<T: Transport> Backend<T> {
    pub fn environment_updates(&mut self, environment: &Value) -> Result<Value> {
        let result = self
            .transport
            .run(vec!["instances".into(), "-j".into()], 10.0, false)?;
        let mut updates = Map::new();
        if result["code"] == 0 {
            let instances = crate::json::parse_value(
                result["stdout"]
                    .as_str()
                    .ok_or_else(|| invalid("Malformed compositor query response"))?,
            )
            .map_err(|error| Error {
                kind: "JSONDecodeError",
                message: error.to_string(),
            })?;
            if let Some(instances) = instances
                .as_array()
                .filter(|instances| instances.len() == 1)
            {
                updates.insert(
                    "HYPRLAND_INSTANCE_SIGNATURE".into(),
                    instances[0]["instance"].clone(),
                );
            }
        }
        if environment.get("XDG_CURRENT_DESKTOP").is_none() {
            updates.insert("XDG_CURRENT_DESKTOP".into(), json!("Hyprland"));
        }
        Ok(Value::Object(updates))
    }
    pub fn query_result(&mut self, name: &str, timeout: f64, check: bool) -> Result<Value> {
        let command = match name {
            "monitors" => vec!["monitors", "-j"],
            "all_monitors" => vec!["monitors", "all", "-j"],
            "workspaces" => vec!["workspaces", "-j"],
            "windows" => vec!["clients", "-j"],
            "active_window" => vec!["activewindow", "-j"],
            "active_workspace" => vec!["activeworkspace", "-j"],
            "cursor" => vec!["cursorpos", "-j"],
            "instances" => vec!["instances", "-j"],
            "bindings" => vec!["binds", "-j"],
            _ => return Err(invalid("Unknown compositor query")),
        };
        self.transport.run(
            command.into_iter().map(str::to_owned).collect(),
            timeout,
            check,
        )
    }
    pub fn query(&mut self, name: &str, timeout: f64, max_bytes: Option<usize>) -> Result<Value> {
        let response = self.query_result(name, timeout, true)?;
        let text = response["stdout"]
            .as_str()
            .ok_or_else(|| invalid("Malformed compositor query response"))?;
        if max_bytes.is_some_and(|limit| text.len() > limit) {
            return Err(invalid("Compositor response exceeded its size limit"));
        }
        let result = crate::json::parse_value(text).map_err(|error| Error {
            kind: "JSONDecodeError",
            message: error.to_string(),
        })?;
        let array = [
            "monitors",
            "all_monitors",
            "windows",
            "workspaces",
            "instances",
            "bindings",
        ]
        .contains(&name);
        if (array && !result.is_array()) || (!array && !result.is_object()) {
            return Err(invalid("Malformed compositor query response"));
        }
        if array
            && result
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| !row.is_object()))
        {
            return Err(invalid("Malformed compositor query record"));
        }
        match name {
            "windows" => Ok(Value::Array(
                result
                    .as_array()
                    .ok_or_else(|| invalid("Malformed compositor query response"))?
                    .iter()
                    .cloned()
                    .map(normalize_window)
                    .collect::<Result<Vec<_>>>()?,
            )),
            "active_window" => normalize_window(result),
            _ => Ok(result),
        }
    }
    fn action(&mut self, name: &str, values: Value, timeout: f64) -> Result<Value> {
        self.transport.run(
            vec![
                "eval".into(),
                format!("hl.dispatch(hl.dsp.{name}({}))", table(&values)?),
            ],
            timeout,
            true,
        )
    }
    pub fn execute(&mut self, method: &str, args: &[Value], kwargs: &Value) -> Result<Value> {
        let timeout = kwargs["timeout"].as_f64().unwrap_or(10.0);
        let argument = |index: usize, key: &str| {
            args.get(index)
                .or_else(|| kwargs.get(key))
                .cloned()
                .unwrap_or(Value::Null)
        };
        let window = argument(0, "window");
        match method {
            "valid_window_id"=>Ok(json!(valid_window_id(&argument(0,"value")))),
            "window_job_token"=>window_job_token(&argument(0,"value")).map(Value::from),
            "version_command"=>Ok(json!(["Hyprland","--version"])),
            "required_commands"=>Ok(json!(["Hyprland","hyprctl"])),
            "diagnostic_commands"=>Ok(json!({"runningVersion":["hyprctl","version"],"installedVersion":["Hyprland","--version"],"allMonitors":["hyprctl","-j","monitors","all"],"configErrors":["hyprctl","configerrors"]})),
            "screenshot_window"=>Err(Error{kind:"RuntimeError",message:"Use the geometry-based capture helper on Hyprland".into()}),
            "night_light_command"=>Ok(json!(night_light_command(truthy(&argument(0,"enabled"))))),
            "set_night_light_temperature"=>self.transport.run(night_light_command(truthy(&argument(0,"enabled")))[1..].to_vec(),kwargs["timeout"].as_f64().unwrap_or(1.0),true),
            "apply_config"=>self.transport.run(vec!["eval".into(),python_text(&argument(0,"text"))],timeout,kwargs.get("check").is_none_or(truthy)),
            "configure_monitors"=>self.transport.run(vec!["eval".into(),render_monitors(&argument(0,"records"))?],timeout,true),
            "config_errors"=>{
                let response=self.transport.run(vec!["configerrors".into()],timeout,true)?;
                Ok(json!(response["stdout"].as_str().ok_or_else(||invalid("Malformed compositor query response"))?.trim()))
            }
            "logout"=>self.transport.run(vec!["eval".into(),"hl.dispatch(hl.dsp.exit())".into()],timeout,true),
            "get_desktop_option"=>{
                let key=argument(0,"key");let key=key.as_str().ok_or_else(||invalid("Invalid compositor field"))?;
                let option=OPTIONS.iter().find(|(name,_)|*name==key).map(|(_,option)|*option).ok_or_else(||Error{kind:"KeyError",message:format!("'{key}'")})?;
                let response=self.transport.run(vec!["getoption".into(),option.into(),"-j".into()],kwargs["timeout"].as_f64().unwrap_or(8.0),true)?;
                let value=crate::json::parse_value(response["stdout"].as_str().ok_or_else(||invalid("Malformed compositor query response"))?).map_err(|error|Error{kind:"JSONDecodeError",message:error.to_string()})?;
                let result=if let Some(value)=value.get("bool").or_else(||value.get("int")){value.clone()}else{
                    let css=value.get("css").and_then(Value::as_str).ok_or_else(||Error{kind:"KeyError",message:"'css'".into()})?;
                    json!(css.split_whitespace().next().ok_or_else(||Error{kind:"IndexError",message:"list index out of range".into()})?.parse::<i64>().map_err(|error|invalid(error.to_string()))?)
                };
                Ok(if key=="followMouse"{json!(truthy(&result))}else{result})
            }
            "capabilities"=>Ok(capabilities()),
            "config_path"=>{
                let kind=argument(1,"kind");let kind=kind.as_str().ok_or_else(||invalid("Unknown compositor configuration"))?;
                if !["palette","desktop","monitor","profile"].contains(&kind){return Err(invalid("Unknown compositor configuration"));}
                let home=argument(0,"home");let paths=crate::paths::Paths::new(home.as_str().ok_or_else(||invalid("Invalid home path"))?);
                Ok(json!(paths.config.join(format!("{kind}.lua")).to_string_lossy()))
            }
            "toggle_workspace_floating"=>self.transport.run_default(vec!["eval".into(),"local ws = hl.get_active_workspace(); local windows = hl.get_workspace_windows(ws); local make_float = false; for _, window in ipairs(windows) do if not window.floating then make_float = true; break end end; for _, window in ipairs(windows) do hl.dispatch(hl.dsp.window.float({ action = make_float and \"set\" or \"unset\", window = window })) end".into()]),
            "render_keyboard"=>render_keyboard(&argument(0,"values")).map(Value::from),
            "render_options"=>render_options(&argument(0,"options")).map(Value::from),
            "render_desktop_options"=>render_desktop(&argument(0,"preferences")).map(Value::from),
            "render_monitors"=>render_monitors(&argument(0,"monitors")).map(Value::from),
            "set_options"=>self.transport.run(vec!["eval".into(),render_options(&argument(0,"options"))?],timeout,true),
            "set_desktop_options"=>self.transport.run(vec!["eval".into(),render_desktop(&argument(0,"preferences"))?],timeout,true),
            "set_animations"=>self.transport.run(vec!["eval".into(),render_options(&json!({"animations":{"enabled":argument(0,"enabled")}}))?],timeout,true),
            "set_performance"=>if !truthy(&argument(0,"enabled")){self.execute("reload",&[],kwargs)}else{self.execute("set_options",&[performance_options()],kwargs)},
            "parse_event"=>Ok(parse_event(argument(0,"line").as_str().ok_or_else(||invalid("Invalid compositor event"))?)),
            "switch_workspace"=>self.action("focus",json!({"workspace":workspace_reference(&argument(0,"workspace"))?,"on_current_monitor":kwargs.get("current_monitor").cloned().unwrap_or(json!(true))}),timeout),
            "focus_direction"|"move_direction"=>{
                let direction=argument(0,"direction");
                if !["left","right","up","down"].iter().any(|value|direction==*value){return Err(invalid(if method=="focus_direction"{"Invalid focus direction"}else{"Invalid move direction"}));}
                self.action(if method=="focus_direction"{"focus"}else{"window.move"},json!({"direction":direction}),timeout)
            }
            "cycle_window"=>{
                let direction=args.first().or_else(||kwargs.get("direction")).cloned().unwrap_or(json!("next"));
                if direction!="next" && direction!="previous"{return Err(invalid("Invalid cycle direction"));}
                self.action("window.cycle",json!({"next":direction=="next"}),timeout)
            }
            "focus_window"|"close_window"|"kill_window"=>self.action(match method{"focus_window"=>"focus","close_window"=>"window.close",_=>"window.kill"},json!({"window":window_selector(&window)?}),timeout),
            "move_window"=>{
                let mut values=Map::new();values.insert("window".into(),json!(window_selector(&window)?));
                if let Some(workspace)=kwargs.get("workspace").filter(|value|!value.is_null()){values.insert("workspace".into(),workspace_reference(workspace)?);values.insert("follow".into(),kwargs.get("follow").cloned().unwrap_or(json!(false)));}
                if kwargs.get("x").is_some_and(|value|!value.is_null()) || kwargs.get("y").is_some_and(|value|!value.is_null()){values.insert("x".into(),kwargs.get("x").cloned().unwrap_or(Value::Null));values.insert("y".into(),kwargs.get("y").cloned().unwrap_or(Value::Null));values.insert("relative".into(),kwargs.get("relative").cloned().unwrap_or(json!(false)));}
                self.action("window.move",Value::Object(values),timeout)
            }
            "move_workspace"=>self.action("workspace.move",json!({"workspace":workspace_reference(&argument(0,"workspace"))?,"monitor":argument(1,"monitor")}),timeout),
            "float_window"=>self.action("window.float",json!({"window":window_selector(&window)?,"action":kwargs.get("action").cloned().unwrap_or(json!("toggle"))}),timeout),
            "pin_window"=>{let mut values=Map::new();values.insert("window".into(),json!(window_selector(&window)?));if let Some(action)=kwargs.get("action").filter(|value|!value.is_null()){values.insert("action".into(),action.clone());}self.action("window.pin",Value::Object(values),timeout)}
            "fullscreen_window"=>self.action("window.fullscreen",json!({"mode":kwargs.get("mode").cloned().unwrap_or(json!("fullscreen")),"window":window_selector(&window)?}),timeout),
            "fullscreen_state"=>self.action("window.fullscreen_state",json!({"internal":kwargs.get("internal").cloned().unwrap_or(json!(0)),"client":kwargs.get("client").cloned().unwrap_or(json!(0)),"action":kwargs.get("action").cloned().unwrap_or(json!("set")),"window":window_selector(&window)?}),timeout),
            "resize_window"=>self.action("window.resize",json!({"x":argument(1,"width"),"y":argument(2,"height"),"relative":kwargs.get("relative").cloned().unwrap_or(json!(false)),"window":window_selector(&window)?}),timeout),
            "place_window"=>{
                self.execute("float_window",std::slice::from_ref(&window),&json!({"action":"enable","timeout":timeout}))?;
                if kwargs.get("width").is_some_and(|value|!value.is_null()) && kwargs.get("height").is_some_and(|value|!value.is_null()){self.execute("resize_window",&[window.clone(),kwargs["width"].clone(),kwargs["height"].clone()],&json!({"timeout":timeout}))?;}
                self.execute("move_window",&[window],&json!({"x":kwargs["x"],"y":kwargs["y"],"relative":false,"timeout":timeout}))
            }
            "dpms"=>{let mut values=Map::new();values.insert("action".into(),argument(0,"action"));let monitor=args.get(1).or_else(||kwargs.get("monitor"));if let Some(monitor)=monitor.filter(|value|!value.is_null()){values.insert("monitor".into(),monitor.clone());}self.action("dpms",Value::Object(values),timeout)}
            "reload"=>self.transport.run(vec!["reload".into()],timeout,true),
            "set_cursor"=>self.transport.run(vec!["setcursor".into(),python_text(&argument(0,"theme")),python_text(&argument(1,"size"))],timeout,true),
            "notify"=>self.transport.run(vec!["notify".into(),python_text(&argument(0,"icon")),python_text(&argument(1,"timeout_ms")),python_text(&argument(2,"color")),python_text(&argument(3,"message"))],timeout,true),
            _=>Err(invalid(format!("Unknown compositor operation: {method}"))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frozen_actual_lua_binding_capture_records_and_errors() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/hyprland-bindings-reference.json"
        ))?;
        let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../nacre/shell-tools/compositor_backends/hyprland/bindings.lua");
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing capture cases")?
        {
            let home = tempfile::tempdir()?;
            let path = home.path().join("bindings.lua");
            std::fs::write(
                &path,
                case["request"]["text"].as_str().ok_or("Missing Lua")?,
            )?;
            let result = capture_bindings(
                &path,
                case["request"]["kind"].as_str().ok_or("Missing kind")?,
                if case["request"]["missing"] == true {
                    None
                } else {
                    Some(std::path::Path::new("/usr/bin/lua"))
                },
                &source,
            );
            let (result, exit) = match result {
                Ok(value) => (value, 0),
                Err(error) => (json!({"error":error.message,"type":error.kind}), 1),
            };
            assert_eq!(
                crate::json::encode(&result)? + "\n",
                case["observed"]["stdout"],
                "{}",
                case["request"]["name"]
            );
            assert_eq!(json!(exit), case["observed"]["exit"]);
        }
        Ok(())
    }
    #[test]
    fn frozen_loader_binding_discovery_and_legacy_bytes() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/hyprland-files-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing file cases")? {
            let home = tempfile::tempdir()?;
            let paths = crate::paths::Paths::new(home.path());
            for (name, content) in case["request"]["files"]
                .as_object()
                .ok_or("Missing inputs")?
            {
                let path = home.path().join(name);
                std::fs::create_dir_all(path.parent().ok_or("Missing parent")?)?;
                std::fs::write(path, content.as_str().ok_or("Missing content")?)?;
            }
            let result = match case["request"]["method"].as_str() {
                Some("ensure_desktop_loader") => {
                    ensure_desktop_loader(home.path()).map(|()| Value::Null)
                }
                Some("binding_source_files") => binding_source_files(home.path(), &paths.config)
                    .map(|paths| {
                        json!(paths
                            .iter()
                            .map(|path| path
                                .strip_prefix(home.path())
                                .map(|path| path.to_string_lossy().into_owned()))
                            .collect::<std::result::Result<Vec<_>, _>>()
                            .expect("Paths remain inside disposable HOME"))
                    }),
                Some("night_light_configured_elsewhere") => Ok(json!(home
                    .path()
                    .join(".config/hypr/hyprsunset.conf")
                    .exists())),
                Some("legacy_power_binding") => Ok(json!(LEGACY_POWER_BINDING)),
                _ => return Err("Unknown file method".into()),
            };
            let (result, exit) = match result {
                Ok(value) => (value, 0),
                Err(error) => (json!({"error":error.message,"type":error.kind}), 1),
            };
            assert_eq!(
                crate::json::encode(&result)? + "\n",
                case["observed"]["stdout"],
                "{}",
                case["request"]["name"]
            );
            assert_eq!(json!(exit), case["observed"]["exit"]);
        }
        Ok(())
    }
    #[test]
    fn frozen_remaining_configuration_options_and_night_light() -> crate::Result<()> {
        struct Reply {
            reply: Value,
            calls: Vec<Value>,
            method: String,
        }
        impl Transport for Reply {
            fn run(&mut self, args: Vec<String>, timeout: f64, check: bool) -> Result<Value> {
                let mut kwargs = Map::new();
                kwargs.insert(
                    "timeout".into(),
                    if self.method == "environment_updates" {
                        Value::Null
                    } else {
                        json!(timeout as i64)
                    },
                );
                if self.method == "environment_updates" || self.method == "apply_config" {
                    kwargs.insert("check".into(), json!(check));
                }
                self.calls.push(json!({"args":args,"kwargs":kwargs}));
                Ok(json!({"code":0,"stdout":self.reply,"stderr":""}))
            }
        }
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/hyprland-remaining-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let input = &case["request"];
            let method = input["method"].as_str().ok_or("Missing method")?;
            let mut backend = Backend {
                transport: Reply {
                    reply: input["reply"].clone(),
                    calls: vec![],
                    method: method.into(),
                },
            };
            let result = match method {
                "session_id" => Ok(json!("fixture")),
                "environment_updates" => {
                    backend.environment_updates(&json!({"HYPRLAND_INSTANCE_SIGNATURE":"fixture"}))
                }
                method => backend.execute(
                    method,
                    input["args"].as_array().ok_or("Missing args")?,
                    &input["kwargs"],
                ),
            };
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
                json!(backend.transport.calls),
                case["observed"]["calls"],
                "{}",
                input["name"]
            );
        }
        Ok(())
    }
    #[test]
    fn frozen_query_shapes_unknown_fields_geometry_and_limits() -> crate::Result<()> {
        struct Query {
            reply: Value,
            calls: Vec<Value>,
        }
        impl Transport for Query {
            fn run(&mut self, args: Vec<String>, timeout: f64, _check: bool) -> Result<Value> {
                self.calls
                    .push(json!({"args":args,"kwargs":{"timeout":timeout as i64,"check":true}}));
                Ok(json!({"stdout":self.reply}))
            }
        }
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/hyprland-query-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let input = &case["request"];
            let mut backend = Backend {
                transport: Query {
                    reply: input["reply"].clone(),
                    calls: vec![],
                },
            };
            let result = backend.query(
                input["query"].as_str().ok_or("Missing query")?,
                3.0,
                input["max_bytes"].as_u64().map(|value| value as usize),
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
                json!(backend.transport.calls),
                case["observed"]["calls"],
                "{}",
                input["name"]
            );
        }
        Ok(())
    }
    struct Fixture {
        calls: Vec<Value>,
    }
    impl Transport for Fixture {
        fn run_default(&mut self, args: Vec<String>) -> Result<Value> {
            self.calls.push(json!({"args":args,"kwargs":{}}));
            Ok(json!({"code":0,"stdout":"ok\n","stderr":""}))
        }
        fn run(&mut self, args: Vec<String>, timeout: f64, _check: bool) -> Result<Value> {
            self.calls
                .push(json!({"args":args,"kwargs":{"timeout":timeout as i64}}));
            Ok(json!({"code":0,"stdout":"ok\n","stderr":""}))
        }
    }
    #[test]
    fn frozen_current_python_semantic_actions_and_lua_calls() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/hyprland-backend-reference.json"
        ))?;
        let mut count = 0;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let input = &case["request"];
            let method = input["method"].as_str().ok_or("Missing method")?;
            let mut backend = Backend {
                transport: Fixture { calls: vec![] },
            };
            let result = backend.execute(
                method,
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
                json!(backend.transport.calls),
                case["observed"]["calls"],
                "{}",
                input["name"]
            );
            count += 1;
        }
        assert_eq!(count, 115);
        Ok(())
    }
}
