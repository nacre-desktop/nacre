use super::*;
use std::{collections::HashSet, fs};
const EXTENSIONS: &[(&str, &str, &str)] = &[
    (
        "cursor.session",
        "compositor_backends/hyprland/session-cursor.lua",
        "apply_session_cursor",
    ),
    (
        "panel.drag",
        "compositor_backends/hyprland/panel-drag.lua",
        "nacre_panel_drag",
    ),
];
fn callback(action: &str) -> Option<(&'static str, &'static str)> {
    match action {
        "cursor.session" => Some(("cursor.session", "apply_session_cursor")),
        "panel.drag" => Some(("panel.drag", "nacre_panel_drag")),
        "panel.drag.finish" => Some(("panel.drag", "nacre_panel_drag_finish")),
        _ => None,
    }
}
fn parameters(action: &str) -> &'static [&'static str] {
    match action {
        "application.launch" => &["command"],
        "workspace.switch" => &["workspace", "current_monitor"],
        "window.focus-direction" => &["direction"],
        "window.move" => &["workspace", "direction"],
        "window.fullscreen" => &["mode"],
        "window.floating" => &["action"],
        "window.resize" => &["x", "y", "relative"],
        "window.cycle" => &["next"],
        "workspace.scratchpad" | "keymap.switch" | "shell.shortcut" => &["name"],
        _ => &[],
    }
}
fn method(action: &str) -> Option<&'static str> {
    Some(match action {
        "application.launch" | "compositor.reload" => "exec_cmd",
        "workspace.switch" | "window.focus-direction" => "focus",
        "window.move" => "window.move",
        "window.close" => "window.close",
        "window.fullscreen" => "window.fullscreen",
        "window.floating" => "window.float",
        "window.pin" => "window.pin",
        "window.pseudo" => "window.pseudo",
        "window.resize" => "window.resize",
        "window.cycle" => "window.cycle_next",
        "group.toggle" => "group.toggle",
        "group.previous" => "group.prev",
        "group.next" => "group.next",
        "layout.toggle-split" => "layout",
        "workspace.scratchpad" => "workspace.toggle_special",
        "keymap.switch" => "submap",
        "shell.shortcut" => "global",
        _ => return None,
    })
}
pub(super) fn action(row: &Value) -> Result<String> {
    let action = member(row, "action")?
        .as_str()
        .ok_or_else(|| invalid("Invalid compositor action"))?;
    let params = fields(member(row, "parameters")?, parameters(action), &[])?;
    let required = match action {
        "workspace.switch" => Some("workspace"),
        "window.focus-direction" => Some("direction"),
        "window.fullscreen" => Some("mode"),
        "window.floating" => Some("action"),
        "window.cycle" => Some("next"),
        _ => None,
    };
    if required.is_some_and(|key| !params.contains_key(key)) {
        return Err(invalid("Missing compositor action parameters"));
    }
    if action == "window.move"
        && !(params.len() == 1
            && (params.contains_key("workspace") || params.contains_key("direction")))
    {
        return Err(invalid("Window move requires one workspace or direction"));
    }
    if params.get("direction").is_some_and(|v| {
        !v.as_str()
            .is_some_and(|v| ["left", "right", "up", "down"].contains(&v))
    }) {
        return Err(invalid("Invalid window direction"));
    }
    if params.get("mode").is_some_and(|v| {
        !v.as_str()
            .is_some_and(|v| ["fullscreen", "maximized"].contains(&v))
    }) {
        return Err(invalid("Invalid fullscreen mode"));
    }
    for key in ["current_monitor", "relative", "next"] {
        if params.get(key).is_some_and(|v| !v.is_boolean()) {
            return Err(invalid("Invalid action boolean"));
        }
    }
    if action == "window.resize"
        && !params.is_empty()
        && (params.len() != 3
            || !["x", "y", "relative"]
                .iter()
                .all(|key| params.contains_key(*key))
            || !integer(&params["x"])
            || !integer(&params["y"]))
    {
        return Err(invalid("Invalid window resize dimensions"));
    }
    if params.get("action").is_some_and(|v| !v.is_string()) {
        return Err(invalid("Invalid floating action"));
    }
    if let Some((_, function)) = callback(action) {
        if !params.is_empty() {
            return Err(invalid(
                "Backend callback accepts no declarative parameters",
            ));
        }
        return Ok(function.into());
    }
    let method =
        method(action).ok_or_else(|| invalid(format!("Unknown compositor action: {action}")))?;
    let scalar = match action {
        "application.launch" => Some("command"),
        "workspace.scratchpad" | "keymap.switch" | "shell.shortcut" => Some("name"),
        _ => None,
    };
    let argument = if let Some(key) = scalar {
        if params.len() != 1 || !params.get(key).is_some_and(Value::is_string) {
            return Err(invalid(format!("Missing action parameter: {key}")));
        }
        lua_value(&params[key])?
    } else if action == "compositor.reload" {
        lua_value(&json!("hyprctl reload"))?
    } else if action == "layout.toggle-split" {
        lua_value(&json!("togglesplit"))?
    } else {
        let mut values = params.clone();
        if let Some(value) = values.get("workspace") {
            let target = workspace_target(value)?;
            values.insert("workspace".into(), target);
        }
        if let Some(value) = values.shift_remove("current_monitor") {
            values.insert("on_current_monitor".into(), value);
        }
        if values.is_empty() {
            String::new()
        } else {
            lua_value(&Value::Object(values))?
        }
    };
    Ok(format!("hl.dsp.{method}({argument})"))
}
pub(super) fn rule(row: &Value) -> Result<String> {
    fields(
        row,
        &["kind", "id", "selector", "effects", "backend"],
        &["kind", "selector", "effects"],
    )?;
    let selectors = fields(
        &row["selector"],
        &["app_id_regex", "title_regex", "namespace_regex"],
        &[],
    )?;
    let effects = fields(
        &row["effects"],
        &[
            "floating",
            "centered",
            "pinned",
            "size",
            "workspace",
            "corner_radius",
            "client_border",
        ],
        &[],
    )?;
    if selectors.values().any(|v| !v.is_string()) {
        return Err(invalid("Rule selectors must be regex strings"));
    }
    if row.get("id").is_some_and(|v| !v.is_string()) {
        return Err(invalid("Rule identity must be a string"));
    }
    for key in ["floating", "centered", "pinned", "client_border"] {
        if effects.get(key).is_some_and(|v| !v.is_boolean()) {
            return Err(invalid("Invalid rule boolean"));
        }
    }
    if effects
        .get("corner_radius")
        .is_some_and(|v| !nonnegative(v))
    {
        return Err(invalid("Corner radius must be a nonnegative integer"));
    }
    let mut lowered = Map::new();
    if let Some(id) = row.get("id") {
        lowered.insert("name".into(), id.clone());
    }
    let mut matches = Map::new();
    for (key, value) in selectors {
        matches.insert(
            match key.as_str() {
                "app_id_regex" => "class",
                "title_regex" => "title",
                _ => "namespace",
            }
            .into(),
            value.clone(),
        );
    }
    lowered.insert("match".into(), Value::Object(matches));
    for (key, value) in effects {
        if key == "client_border" {
            if value == true {
                lowered.insert("border_size".into(), json!(0));
            }
            continue;
        }
        let value = if key == "workspace" {
            fields(value, &["target", "activate"], &["target", "activate"])?;
            if !value["activate"].is_boolean() {
                return Err(invalid("Invalid workspace rule activation"));
            }
            json!(
                python_text(&workspace_target(&value["target"])?)
                    + if value["activate"] == true {
                        ""
                    } else {
                        " silent"
                    }
            )
        } else {
            value.clone()
        };
        let name = match key.as_str() {
            "floating" => "float",
            "centered" => "center",
            "pinned" => "pin",
            "size" => "size",
            "workspace" => "workspace",
            "corner_radius" => "rounding",
            "client_border" => "border_size",
            _ => return Err(invalid("Unknown window rule effect")),
        };
        lowered.insert(name.into(), value);
    }
    let empty = json!({});
    let backend = row.get("backend").unwrap_or(&empty);
    let backend = fields(backend, &["hyprland", "niri"], &[])?;
    if let Some(niri) = backend.get("niri") {
        fields(niri, &["blur", "namespace_regex"], &[])?;
        if !niri.get("blur").is_some_and(Value::is_boolean) {
            return Err(invalid("Invalid niri blur effect"));
        }
        if niri.get("namespace_regex").is_some_and(|v| !v.is_string()) {
            return Err(invalid("Invalid niri layer selector"));
        }
    }
    if let Some(hyprland) = backend.get("hyprland") {
        let values = fields(
            hyprland,
            &[
                "blur",
                "ignore_alpha",
                "no_anim",
                "fullscreen_state",
                "suppress_event",
                "no_initial_focus",
            ],
            &[],
        )?;
        for (key, value) in values {
            lowered.insert(key.clone(), value.clone());
        }
    }
    let method = match row["kind"].as_str() {
        Some("window-rule") => "window_rule",
        Some("layer-rule") => "layer_rule",
        _ => return Err(invalid("Unknown rule kind")),
    };
    Ok(format!(
        "hl.{method}({})",
        lua_value(&Value::Object(lowered))?
    ))
}
pub(super) fn render(contract: &Value, directory: &Path) -> Result<Value> {
    fields(
        contract,
        &["version", "extensions", "outputs", "profileBindings"],
        &["version", "extensions", "outputs"],
    )?;
    if !(contract["version"] == 1
        || contract["version"] == true
        || contract["version"].as_f64() == Some(1.0))
    {
        return Err(invalid("Unsupported compositor contract version"));
    }
    if let Some(profiles) = contract.get("profileBindings") {
        fields(profiles, &["windowTrashBindings"], &[])?;
        let empty = json!([]);
        let optional = profiles
            .get("windowTrashBindings")
            .unwrap_or(&empty)
            .as_array()
            .ok_or_else(|| invalid("Profile bindings must be binding declarations"))?;
        if optional
            .iter()
            .any(|row| !row.is_object() || row["kind"] != "binding")
        {
            return Err(invalid("Profile bindings must be binding declarations"));
        }
        let mut selected = contract.clone();
        selected
            .as_object_mut()
            .ok_or_else(|| invalid("Invalid compositor declaration fields"))?
            .shift_remove("profileBindings");
        let bindings = selected["outputs"]["hypr/conf/keybinding.lua"]
            .as_array_mut()
            .ok_or_else(|| invalid("Expected ordered compositor declarations"))?;
        bindings.extend(optional.iter().cloned());
        render(&selected, directory)?;
    }
    let outputs = fields(&contract["outputs"], HYPRLAND_OUTPUTS, HYPRLAND_OUTPUTS)?;
    fields(
        &contract["extensions"],
        &["cursor.session", "panel.drag"],
        &["cursor.session", "panel.drag"],
    )?;
    for (key, path, function) in EXTENSIONS {
        let extension = &contract["extensions"][key];
        fields(
            extension,
            &["backend", "source", "function", "purpose"],
            &["backend", "source", "function", "purpose"],
        )?;
        if extension["backend"] != "hyprland"
            || extension["source"] != *path
            || extension["function"] != *function
        {
            return Err(invalid("Unexpected backend extension"));
        }
    }
    let mut result = Map::new();
    for (path, entries) in outputs {
        let entries = entries
            .as_array()
            .ok_or_else(|| invalid("Expected ordered compositor declarations"))?;
        let mut lines = vec![
            "-- Generated by tools/generate_compositor_config.py; edit compositor-contract.json."
                .to_owned(),
        ];
        let mut keymap = json!("");
        let mut extensions = HashSet::new();
        for row in entries {
            if !row.is_object() || row.get("kind").is_none() {
                return Err(invalid("Invalid compositor declaration"));
            }
            let next = row.get("keymap").cloned().unwrap_or(json!(""));
            if next != keymap {
                if truthy(&keymap) {
                    lines.push("end)".into());
                }
                if truthy(&next) {
                    if !next.is_string() {
                        return Err(invalid("Invalid keymap name"));
                    }
                    lines.push(format!(
                        "hl.define_submap({}, function()",
                        lua_value(&next)?
                    ));
                }
                keymap = next;
            }
            match row["kind"].as_str() {
                Some("binding") => {
                    fields(
                        row,
                        &["kind", "keys", "action", "parameters", "options", "keymap"],
                        &["keys", "action", "parameters", "options"],
                    )?;
                    let flags = fields(
                        &row["options"],
                        &[
                            "description",
                            "locked",
                            "repeating",
                            "mouse",
                            "non_consuming",
                            "release",
                            "ignore_mods",
                        ],
                        &[],
                    )?;
                    if flags.iter().any(|(key, value)| {
                        if key == "description" {
                            !value.is_string()
                        } else {
                            !value.is_boolean()
                        }
                    }) {
                        return Err(invalid("Invalid binding option type"));
                    }
                    if !row["keys"].as_str().is_some_and(|s| !s.is_empty()) {
                        return Err(invalid("Invalid binding keys"));
                    }
                    if row["action"]
                        .as_str()
                        .and_then(callback)
                        .is_some_and(|(id, _)| !extensions.contains(id))
                    {
                        return Err(invalid(
                            "Callback binding requires its backend extension first",
                        ));
                    }
                    lines.push(format!(
                        "hl.bind({}, {}, {})",
                        lua_value(&row["keys"])?,
                        action(row)?,
                        lua_value(&row["options"])?
                    ));
                }
                Some("window-rule" | "layer-rule") => lines.push(rule(row)?),
                Some("event") => {
                    fields(row, &["kind", "event", "callback"], &["event", "callback"])?;
                    if row["event"] != "session.ready"
                        || row["callback"] != "cursor.session"
                        || !extensions.contains("cursor.session")
                    {
                        return Err(invalid("Unsupported compositor event/callback"));
                    }
                    lines.push("hl.on(\"hyprland.start\", apply_session_cursor)".into());
                }
                Some("unbind") => {
                    fields(row, &["kind", "keys"], &["keys"])?;
                    lines.push(format!("hl.unbind({})", lua_value(&row["keys"])?));
                }
                Some("backend-config") => {
                    fields(row, &["kind", "backend", "value"], &["backend", "value"])?;
                    if row["backend"] != "hyprland" {
                        return Err(invalid("Unsupported configuration backend"));
                    }
                    lines.push(format!("hl.config({})", lua_value(&row["value"])?));
                }
                Some("extension") => {
                    fields(row, &["kind", "id"], &["id"])?;
                    let id = row["id"]
                        .as_str()
                        .ok_or_else(|| invalid("Invalid/repeated backend extension"))?;
                    let path = EXTENSIONS
                        .iter()
                        .find(|(key, _, _)| *key == id)
                        .map(|(_, path, _)| path)
                        .ok_or_else(|| invalid("Invalid/repeated backend extension"))?;
                    if !extensions.insert(id) {
                        return Err(invalid("Invalid/repeated backend extension"));
                    }
                    let text = fs::read_to_string(directory.join(path)).map_err(|error| Error {
                        kind: "OSError",
                        message: error.to_string(),
                    })?;
                    lines.push(text.trim_end().into());
                }
                _ => return Err(invalid("Unknown compositor declaration kind")),
            }
        }
        if truthy(&keymap) {
            lines.push("end)".into());
        }
        result.insert(path.clone(), json!(lines.join("\n") + "\n"));
    }
    Ok(Value::Object(result))
}
