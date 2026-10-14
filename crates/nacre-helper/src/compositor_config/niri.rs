use super::*;
use std::collections::HashSet;

fn unsupported(message: impl Into<String>) -> Error {
    Error {
        kind: "NotImplementedError",
        message: message.into(),
    }
}
pub(super) fn quoted(value: &Value) -> Result<String> {
    let value = value
        .as_str()
        .filter(|s| !s.contains('\0'))
        .ok_or_else(|| invalid("Invalid KDL string"))?;
    let mut text = String::from("\"");
    for c in value.chars() {
        match c {
            '\\' | '"' => {
                text.push('\\');
                text.push(c)
            }
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            c if (c as u32) < 32 => text.push_str(&format!("\\u{{{:x}}}", c as u32)),
            c => text.push(c),
        }
    }
    text.push('"');
    Ok(text)
}
pub(super) fn keys(value: &Value) -> Result<String> {
    let value = value.as_str().ok_or_else(|| invalid("Invalid binding"))?;
    let mut pieces = Vec::new();
    for piece in value.split('+') {
        let piece = piece.trim();
        if piece.is_empty() || !piece.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Err(invalid("Invalid binding key"));
        }
        pieces.push(match piece {
            "SUPER" => "Super",
            "CTRL" => "Ctrl",
            "ALT" => "Alt",
            "SHIFT" => "Shift",
            "mouse_down" => "WheelScrollDown",
            "mouse_up" => "WheelScrollUp",
            "left" => "Left",
            "right" => "Right",
            "up" => "Up",
            "down" => "Down",
            "return" => "Return",
            "escape" => "Escape",
            other => other,
        });
    }
    Ok(pieces.join("+"))
}
fn shell_quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_@%+=:,./-".contains(c))
    {
        text.to_owned()
    } else {
        format!("'{}'", text.replace('\'', "'\"'\"'"))
    }
}
fn action_command(name: &str, arguments: &[String]) -> String {
    let args = std::iter::once(name.to_owned())
        .chain(arguments.iter().map(|s| shell_quote(s)))
        .collect::<Vec<_>>()
        .join(" ");
    format!("exec env NACRE_COMPOSITOR=niri python3 \"${{NACRE_COMPOSITOR_TOOLS:-$HOME/.local/share/nacre/shell/tools}}/compositor.py\" action {args}")
}
pub(super) fn workspace_args(value: &Value) -> Result<Vec<String>> {
    match value["kind"].as_str() {
        Some("number") => {
            let number = member(value, "number")?;
            if !number.as_u64().is_some_and(|n| (1..=255).contains(&n)) {
                return Err(invalid("Invalid workspace index"));
            }
            Ok(vec!["--workspace".into(), python_text(number)])
        }
        Some("relative") => {
            let offset = member(value, "offset")?;
            if !offset.as_i64().is_some_and(|n| n == 1 || n == -1) {
                return Err(invalid("Invalid relative workspace step"));
            }
            Ok(vec![
                "--workspace".into(),
                crate::json::encode(&json!({"relative":offset}))
                    .map_err(|e| invalid(e.to_string()))?,
            ])
        }
        Some("name") => Ok(vec![
            "--workspace".into(),
            python_text(member(value, "name")?),
        ]),
        _ => Err(unsupported(
            "Niri has no Hyprland special/scratchpad workspaces",
        )),
    }
}
pub(super) fn binding(row: &Value) -> Result<String> {
    let action = member(row, "action")?
        .as_str()
        .ok_or_else(|| invalid("Invalid compositor action"))?;
    let empty = json!({});
    let params = row.get("parameters").unwrap_or(&empty);
    let options = row.get("options").unwrap_or(&empty);
    if options.get("mouse").is_some_and(truthy) {
        return Err(unsupported(
            "Keep niri's native move/resize gestures; no pointer-hook substitution",
        ));
    }
    if [
        "window.pin",
        "window.pseudo",
        "layout.toggle-split",
        "workspace.scratchpad",
        "keymap.switch",
        "panel.drag.begin",
        "panel.drag.finish",
    ]
    .contains(&action)
        || row.get("keymap").is_some_and(truthy)
    {
        return Err(unsupported(format!(
            "No equivalent niri capability for {action}"
        )));
    }
    if options.get("non_consuming").is_some_and(truthy) {
        return Err(unsupported(
            "Niri bindings consume input; non-consuming hooks are unsupported",
        ));
    }
    if action == "application.launch" {
        let command = member(params, "command")?;
        let description = options
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or("");
        match description{
            "Nacre:close"=>return Ok(action_command("close_window",&[])),"Nacre:overview"=>return Ok("niri msg action toggle-overview".into()),"Nacre:screenshot"=>return Ok("niri msg action screenshot".into()),
            "Nacre:save-screenshot"=>return Ok("mkdir -p \"$HOME/Pictures/Screenshots\" && niri msg action screenshot-screen --show-pointer false --path \"$HOME/Pictures/Screenshots/screenshot_$(date +%Y%m%d_%H%M%S_%N).png\"".into()),
            _=>{}
        }
        if [
            "Nacre:put-away",
            "Nacre:restore",
            "Nacre:restore-all",
            "Nacre:force-close",
            "Nacre:hide",
            "Nacre:unhide",
        ]
        .contains(&description)
            || command
                .as_str()
                .is_some_and(|s| s.contains("/window-minimize.sh"))
        {
            return Err(unsupported(
                "Hyprland hide/trash/force-kill operation is not available",
            ));
        }
        return command
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| invalid("Invalid launch command"));
    }
    if action.starts_with("group.") {
        let name = match action {
            "group.toggle" => "toggle-column-tabbed-display",
            "group.previous" => "focus-window-up-or-bottom",
            "group.next" => "focus-window-down-or-top",
            _ => return Err(missing(action)),
        };
        return Ok(format!("niri msg action {name}"));
    }
    if action == "window.cycle" {
        return Ok(action_command(
            "cycle_window",
            &[
                "--direction".into(),
                if truthy(member(params, "next")?) {
                    "next"
                } else {
                    "previous"
                }
                .into(),
            ],
        ));
    }
    if action == "shell.shortcut" {
        let name = member(params, "name")?
            .as_str()
            .ok_or_else(|| invalid("Invalid shortcut name"))?;
        let name = shell_quote(name);
        return Ok(format!("if [ -n \"${{NACRE_QS_CONFIG:-}}\" ]; then exec \"${{NACRE_QS_BIN:-$HOME/.local/share/nacre/shell/bin/qs}}\" -p \"$NACRE_QS_CONFIG\" ipc call nacreCompositor shortcut {name}; else exec \"${{NACRE_QS_BIN:-$HOME/.local/share/nacre/shell/bin/qs}}\" -c nacre ipc call nacreCompositor shortcut {name}; fi"));
    }
    let (operation, args) = match action {
        "compositor.reload" => ("reload", vec![]),
        "window.close" => ("close_window", vec![]),
        "workspace.switch" => (
            "switch_workspace",
            workspace_args(member(params, "workspace")?)?,
        ),
        "window.move" if params.get("workspace").is_some() => {
            ("move_window", workspace_args(member(params, "workspace")?)?)
        }
        "window.move" => (
            "move_direction",
            vec![
                "--direction".into(),
                python_text(member(params, "direction")?),
            ],
        ),
        "window.focus-direction" => (
            "focus_direction",
            vec![
                "--direction".into(),
                python_text(member(params, "direction")?),
            ],
        ),
        "window.fullscreen" => (
            "fullscreen_window",
            vec!["--mode".into(), python_text(member(params, "mode")?)],
        ),
        "window.floating" => (
            "float_window",
            vec!["--action".into(), python_text(member(params, "action")?)],
        ),
        "window.resize" => (
            "resize_window",
            vec![
                "--width".into(),
                python_text(member(params, "x")?),
                "--height".into(),
                python_text(member(params, "y")?),
                "--relative".into(),
                params.get("relative").is_some_and(truthy).to_string(),
            ],
        ),
        _ => {
            return Err(unsupported(format!(
                "No equivalent niri action for {action}"
            )))
        }
    };
    Ok(action_command(operation, &args))
}
pub(super) fn rule(row: &Value) -> Result<String> {
    let empty = json!({});
    let selectors = row
        .get("selector")
        .unwrap_or(&empty)
        .as_object()
        .ok_or_else(|| invalid("Invalid window selector"))?;
    if selectors
        .keys()
        .any(|key| !["app_id_regex", "title_regex"].contains(&key.as_str()))
    {
        return Err(unsupported("No matching window selector on niri"));
    }
    let effects = row
        .get("effects")
        .unwrap_or(&empty)
        .as_object()
        .ok_or_else(|| invalid("Unknown window rule effect"))?;
    if effects.keys().any(|key| {
        ![
            "floating",
            "centered",
            "pinned",
            "size",
            "workspace",
            "corner_radius",
            "client_border",
        ]
        .contains(&key.as_str())
    }) {
        return Err(invalid("Unknown window rule effect"));
    }
    let mut parts = vec!["window-rule {".to_owned()];
    if !selectors.is_empty() {
        let values = selectors
            .iter()
            .map(|(key, value)| {
                Ok(format!(
                    "{}={}",
                    if key == "app_id_regex" {
                        "app-id"
                    } else {
                        "title"
                    },
                    quoted(value)?
                ))
            })
            .collect::<Result<Vec<_>>>()?;
        parts.push("    match ".to_owned() + &values.join(" "));
    }
    if let Some(floating) = effects.get("floating") {
        if !floating.is_boolean() {
            return Err(invalid("Invalid floating rule"));
        }
        parts.push(format!("    open-floating {floating}"));
    }
    if let Some(radius) = effects.get("corner_radius") {
        if !nonnegative(radius) {
            return Err(invalid("Corner radius must be a nonnegative integer"));
        }
        parts.push(format!("    geometry-corner-radius {radius}"));
        parts.push("    clip-to-geometry true".into());
    }
    if let Some(border) = effects.get("client_border") {
        if !border.is_boolean() {
            return Err(invalid("Client border must be a boolean"));
        }
        if border == true {
            parts.extend(
                [
                    "    border { off; }",
                    "    focus-ring { off; }",
                    "    shadow { off; }",
                ]
                .map(str::to_owned),
            );
        }
    }
    if let Some(workspace) = effects.get("workspace") {
        let target = member(workspace, "target")?;
        if target["kind"] != "name" {
            return Err(unsupported(
                "Niri initial window routes require a named workspace, not a changing index",
            ));
        }
        parts.push(format!(
            "    open-on-workspace {}",
            quoted(member(target, "name")?)?
        ));
    }
    if let Some(size) = effects.get("size") {
        let dimensions = if let Some(text) = size.as_str() {
            text.split_whitespace()
                .map(|s| json!(s))
                .collect::<Vec<_>>()
        } else {
            size.as_array()
                .cloned()
                .ok_or_else(|| invalid("Invalid window size rule"))?
        };
        if dimensions.len() != 2 {
            return Err(invalid("Invalid window size rule"));
        }
        for (key, dimension, axis) in [
            ("default-column-width", &dimensions[0], "w"),
            ("default-window-height", &dimensions[1], "h"),
        ] {
            let dimension = python_text(dimension);
            let regex = |pattern: &str| {
                regex::Regex::new(pattern)
                    .map(|r| r.is_match(&dimension))
                    .map_err(|e| invalid(e.to_string()))
            };
            let value = if regex(r"\A[1-9]\d*\z")? {
                format!("fixed {dimension}")
            } else if regex(r"\A(?:\d+(?:\.\d+)?)%\z")? {
                let number = dimension[..dimension.len() - 1]
                    .parse::<f64>()
                    .map_err(|_| invalid("Invalid size proportion"))?
                    / 100.0;
                format!("proportion {}", json!(number))
            } else if regex(&format!(r"\Amonitor_{axis}\*(?:\d+(?:\.\d+)?)\z"))? {
                format!(
                    "proportion {}",
                    dimension
                        .split_once('*')
                        .ok_or_else(|| invalid("Invalid size expression"))?
                        .1
                )
            } else {
                return Err(unsupported(format!(
                    "Unmapped size expression: {dimension}"
                )));
            };
            parts.push(format!("    {key} {{ {value}; }}"));
        }
    }
    parts.push("}".into());
    Ok(parts.join("\n"))
}
pub(super) fn render(contract: &Value) -> Result<Value> {
    if !contract.is_object()
        || !(contract["version"] == 1
            || contract["version"] == true
            || contract["version"].as_f64() == Some(1.0))
        || !contract["outputs"].is_object()
    {
        return Err(invalid("Invalid compositor contract"));
    }
    let mut binds = Vec::new();
    let mut rules = Vec::new();
    let mut unsupported_rows = Vec::new();
    let mut seen = HashSet::new();
    for (source, rows) in contract["outputs"]
        .as_object()
        .ok_or_else(|| invalid("Invalid compositor contract"))?
    {
        for row in rows
            .as_array()
            .ok_or_else(|| invalid("Invalid compositor declarations"))?
        {
            if !row.is_object() || row.get("kind").is_none() {
                return Err(invalid("Invalid compositor declaration"));
            }
            let kind = row["kind"]
                .as_str()
                .ok_or_else(|| invalid("Unknown compositor declaration kind"))?;
            let result: Result<()> = (|| {
                match kind {
                    "binding" => {
                        let command = binding(row)?;
                        let key = keys(member(row, "keys")?)?;
                        if !seen.insert(key.clone()) {
                            return Err(invalid(format!("Repeated niri binding: {key}")));
                        }
                        let empty = json!({});
                        let options = row.get("options").unwrap_or(&empty);
                        let mut flags = vec![format!(
                            "repeat={}",
                            options.get("repeating").is_some_and(truthy)
                        )];
                        let mut body = format!("spawn-sh {}", quoted(&json!(command))?);
                        if row["action"] == "window.focus-direction"
                            && row["parameters"]["direction"]
                                .as_str()
                                .is_some_and(|v| ["left", "right"].contains(&v))
                        {
                            body = format!(
                                "focus-column-{}",
                                row["parameters"]["direction"]
                                    .as_str()
                                    .ok_or_else(|| invalid("Invalid direction"))?
                            );
                            flags.push("cooldown-ms=120".into());
                        }
                        if options.get("locked").is_some_and(truthy) {
                            flags.push("allow-when-locked=true".into());
                        }
                        if let Some(description) = options.get("description").filter(|v| truthy(v))
                        {
                            flags.push(format!("hotkey-overlay-title={}", quoted(description)?));
                        }
                        binds.push(format!("    {key} {} {{ {body}; }}", flags.join(" ")));
                    }
                    "window-rule" => {
                        rules.push(rule(row)?);
                        let mut differing: Vec<String> = Vec::new();
                        let effects = &row["effects"];
                        if effects.get("pinned").is_some() {
                            differing.push("pinned".into());
                        }
                        if effects.get("centered") == Some(&json!(false)) {
                            differing
                                .push("centered=false (niri centers new floating windows)".into());
                        }
                        if let Some(size) = effects.get("size") {
                            let text = python_text(size);
                            if text.contains('%') || text.contains("monitor_") {
                                differing.push("size proportions use niri's working area".into());
                            }
                        }
                        if !differing.is_empty() {
                            unsupported_rows.push(json!({"source":source,"kind":kind,"id":row.get("id").cloned().unwrap_or(json!("")),"reason":format!("Unmapped window effects: {}",differing.join(", "))}));
                        }
                    }
                    "layer-rule" if row["backend"].get("niri").is_some_and(truthy) => {
                        let effect = &row["backend"]["niri"];
                        if fields(effect, &["blur", "namespace_regex"], &[]).is_err()
                            || !effect.get("blur").is_some_and(Value::is_boolean)
                        {
                            return Err(invalid("Invalid niri layer effect"));
                        }
                        let namespace = effect
                            .get("namespace_regex")
                            .unwrap_or(member(&row["selector"], "namespace_regex")?);
                        rules.push(format!("layer-rule {{\n    match namespace={}\n    background-effect {{ blur {}; }}\n}}",quoted(namespace)?,effect["blur"]));
                    }
                    "extension" | "layer-rule" | "backend-config" | "unbind" | "event" => {
                        return Err(unsupported(format!(
                            "Backend-specific {kind} is not emitted for niri"
                        )))
                    }
                    _ => return Err(invalid("Unknown compositor declaration kind")),
                }
                Ok(())
            })();
            if let Err(error) = result {
                if error.kind != "NotImplementedError" {
                    return Err(error);
                }
                unsupported_rows.push(json!({"source":source,"kind":kind,"id":row.get("id").cloned().unwrap_or_else(||row.get("keys").cloned().unwrap_or(json!(""))),"reason":error.message}));
            }
        }
    }
    let header="// Generated from compositor-contract.json; edit the neutral source.\n// Include before user bindings/rules so user-owned choices take precedence.\n";
    let unsupported =
        serde_json::to_string_pretty(&json!({"backend":"niri","unsupported":unsupported_rows}))
            .map_err(|e| invalid(e.to_string()))?
            + "\n";
    let mut outputs = Map::new();
    outputs.insert(
        NIRI_OUTPUTS[0].into(),
        json!(format!("{header}binds {{\n{}\n}}\n", binds.join("\n"))),
    );
    outputs.insert(
        NIRI_OUTPUTS[1].into(),
        json!(format!("{header}{}\n", rules.join("\n\n"))),
    );
    outputs.insert(NIRI_OUTPUTS[2].into(), json!(unsupported));
    Ok(Value::Object(outputs))
}
