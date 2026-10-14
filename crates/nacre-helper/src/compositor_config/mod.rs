//! Deterministic neutral configuration lowering; no compositor or QML side effects.
mod hyprland;
mod niri;

use serde_json::{json, Map, Value};
use std::{fmt, path::Path};

pub const HYPRLAND_OUTPUTS: &[&str] = &[
    "hypr/conf/cursor.lua",
    "hypr/conf/keybinding.lua",
    "hypr/conf/windowrule.lua",
    "hypr/conf/nacre.lua",
    "hypr/conf/brain.lua",
    "nacre/shell-tools/shortcuts.lua",
];
pub const NIRI_OUTPUTS: &[&str] = &[
    "niri/bindings.kdl",
    "niri/window-rules.kdl",
    "niri/unsupported.json",
];

#[derive(Debug)]
pub struct Error {
    pub kind: &'static str,
    pub message: String,
}
impl fmt::Display for Error {
    fn fmt(&self, output: &mut fmt::Formatter<'_>) -> fmt::Result {
        output.write_str(&self.message)
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
fn missing(key: &str) -> Error {
    Error {
        kind: "KeyError",
        message: format!("'{key}'"),
    }
}
fn fields<'a>(
    value: &'a Value,
    allowed: &[&str],
    required: &[&str],
) -> Result<&'a Map<String, Value>> {
    value
        .as_object()
        .filter(|object| {
            object.keys().all(|key| allowed.contains(&key.as_str()))
                && required.iter().all(|key| object.contains_key(*key))
        })
        .ok_or_else(|| invalid("Invalid compositor declaration fields"))
}
fn member<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value.get(key).ok_or_else(|| missing(key))
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
fn integer(value: &Value) -> bool {
    value
        .as_number()
        .is_some_and(|number| number.is_i64() || number.is_u64())
}
fn nonnegative(value: &Value) -> bool {
    integer(value) && (value.as_u64().is_some() || value.as_i64().is_some_and(|n| n >= 0))
}
fn python_text(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Null => "None".into(),
        _ => value.to_string(),
    }
}

pub fn supported_backends() -> Value {
    json!(["hyprland", "niri"])
}
pub fn backend_capabilities(backend: &str) -> Result<Value> {
    if !["hyprland", "niri"].contains(&backend) {
        return Err(invalid("Unknown compositor backend"));
    }
    let niri = backend == "niri";
    Ok(
        json!({"backend":backend,"backendReady":!niri,"setup_ready":!niri,"managedIncludes":niri,"minimumVersion":if niri {Some("26.04")} else {None},"includeMinimumVersion":if niri {Some("25.11")} else {None},"testedVersion":if niri {Some("26.04")} else {None},"generatedFiles":if niri {NIRI_OUTPUTS} else {HYPRLAND_OUTPUTS}}),
    )
}

pub fn lua_value(value: &Value) -> Result<String> {
    lua(value, true)
}
pub fn runtime_lua_value(value: &Value) -> Result<String> {
    lua(value, false)
}
fn lua(value: &Value, named_fields: bool) -> Result<String> {
    Ok(match value {
        Value::Null => "nil".into(),
        Value::Bool(v) => v.to_string(),
        Value::Number(v) => v.to_string(),
        Value::String(value) => {
            let mut text = String::from("\"");
            for c in value.chars() {
                match c {
                    '\\' | '"' => {
                        text.push('\\');
                        text.push(c)
                    }
                    c if (c as u32) < 32 => text.push_str(&format!("\\{:03}", c as u32)),
                    c => text.push(c),
                }
            }
            text.push('"');
            text
        }
        Value::Array(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|v| lua(v, named_fields))
                .collect::<Result<Vec<_>>>()?
                .join(",")
        ),
        Value::Object(values) => {
            const RESERVED: &[&str] = &[
                "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto",
                "if", "in", "local", "nil", "not", "or", "repeat", "return", "then", "true",
                "until", "while",
            ];
            let mut rows = Vec::new();
            for (key, value) in values {
                let identifier = key
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
                    && key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
                let field = if named_fields && identifier && !RESERVED.contains(&key.as_str()) {
                    key.clone()
                } else {
                    format!("[{}]", lua(&Value::String(key.clone()), named_fields)?)
                };
                rows.push(format!("{field}={}", lua(value, named_fields)?));
            }
            format!("{{{}}}", rows.join(","))
        }
    })
}
pub fn workspace_target(value: &Value) -> Result<Value> {
    let object = fields(
        value,
        &["kind", "number", "scope", "offset", "name"],
        &["kind"],
    )?;
    match value["kind"].as_str() {
        Some("number")
            if object.len() == 2
                && object.contains_key("number")
                && integer(&value["number"])
                && value["number"].as_u64().is_some_and(|n| n > 0) =>
        {
            return Ok(value["number"].clone())
        }
        Some("scratchpad") if object.len() == 2 && object.contains_key("name") => {
            if let Some(name) = value["name"].as_str() {
                return Ok(json!(if name.is_empty() {
                    "special".into()
                } else {
                    format!("special:{name}")
                }));
            }
        }
        Some("relative")
            if object.len() == 3
                && object.contains_key("scope")
                && object.contains_key("offset") =>
        {
            let prefix = match value["scope"].as_str() {
                Some("monitor") => Some("m"),
                Some("existing") => Some("e"),
                Some("global") => Some("r"),
                _ => None,
            };
            if let (Some(prefix), Some(offset)) = (prefix, value["offset"].as_i64()) {
                return Ok(json!(format!("{prefix}{offset:+}")));
            }
        }
        _ => {}
    }
    Err(invalid("Invalid workspace target"))
}
pub fn profile_rules(profile: &Value) -> Result<Value> {
    let routes = member(profile, "routes")?.as_array().ok_or_else(|| Error {
        kind: "TypeError",
        message: "'NoneType' object is not iterable".into(),
    })?;
    let mut rules = Vec::new();
    for route in routes {
        let name = member(route, "name")?
            .as_str()
            .ok_or_else(|| invalid("Invalid profile route name"))?;
        rules.push(json!({"kind":"window-rule","id":format!("profile-{name}"),"selector":{"app_id_regex":member(route,"match")?},"effects":{"workspace":{"target":{"kind":"number","number":member(route,"workspace")?},"activate":false}}}));
    }
    let brain = member(profile, "brainWorkspace")?;
    if !brain.is_null() {
        rules.push(json!({"kind":"window-rule","id":"profile-brain","selector":{"app_id_regex":"^(nacre-brain|chrome-127[.]0[.]0[.]1__-Default|[Oo]bsidian)$"},"effects":{"workspace":{"target":{"kind":"number","number":brain},"activate":false}}}));
    }
    Ok(Value::Array(rules))
}
pub fn render_hyprland(contract: &Value, directory: &Path) -> Result<Value> {
    hyprland::render(contract, directory)
}
pub fn rule_lua(rule: &Value) -> Result<String> {
    hyprland::rule(rule)
}
pub fn quoted_kdl(value: &Value) -> Result<String> {
    niri::quoted(value)
}
pub fn rule_kdl(rule: &Value) -> Result<String> {
    niri::rule(rule)
}
/// Module-operation boundary shared by caller migration and developer parity.
pub fn execute(
    module: &str,
    operation: &str,
    request: &Value,
    default_directory: &Path,
) -> Result<Value> {
    let empty_args = Vec::new();
    let empty_kwargs = Map::new();
    let args = request
        .get("args")
        .and_then(Value::as_array)
        .unwrap_or(&empty_args);
    let kwargs = request
        .get("kwargs")
        .and_then(Value::as_object)
        .unwrap_or(&empty_kwargs);
    let argument = |index: usize, key: &str| {
        args.get(index)
            .or_else(|| kwargs.get(key))
            .ok_or_else(|| missing(key))
    };
    let backend = kwargs
        .get("backend")
        .and_then(Value::as_str)
        .unwrap_or("hyprland");
    let directory_index = if operation == "load" { 0 } else { 1 };
    let directory = kwargs.get("directory").or_else(|| {
        if operation == "load" || operation == "render" {
            args.get(directory_index)
        } else {
            None
        }
    });
    let directory = match directory {
        None => default_directory.to_owned(),
        Some(value) => value
            .get("$path")
            .and_then(Value::as_str)
            .map(std::path::PathBuf::from)
            .ok_or_else(|| invalid("Expected a tagged directory Path"))?,
    };
    match (module, operation) {
        ("neutral", "supported_backends") => Ok(supported_backends()),
        ("neutral", "backend_capabilities") => backend_capabilities(
            argument(0, "name")?
                .as_str()
                .ok_or_else(|| invalid("Unknown compositor backend"))?,
        ),
        ("neutral" | "hyprland", "workspace_target") => workspace_target(argument(0, "value")?),
        ("hyprland", "lua_value") => lua_value(argument(0, "value")?).map(Value::String),
        ("hyprland", "action_lua") => hyprland::action(argument(0, "row")?).map(Value::String),
        ("neutral" | "hyprland", "rule_lua") => {
            if backend != "hyprland" {
                return Err(invalid(format!(
                    "Unsupported compositor backend: {backend}"
                )));
            }
            rule_lua(argument(0, "rule")?).map(Value::String)
        }
        ("neutral", "load") => {
            let text = std::fs::read_to_string(directory.join("compositor-contract.json"))
                .map_err(|e| Error {
                    kind: "OSError",
                    message: e.to_string(),
                })?;
            serde_json::from_str(&text).map_err(|e| Error {
                kind: "JSONDecodeError",
                message: e.to_string(),
            })
        }
        ("neutral", "render") => render(argument(0, "contract")?, &directory, backend),
        ("hyprland", "render") => render_hyprland(argument(0, "contract")?, &directory),
        ("niri", "render") => niri::render(argument(0, "contract")?),
        ("neutral", "profile_rules") => profile_rules(argument(0, "profile")?),
        ("neutral", "render_keyboard") => {
            render_keyboard(argument(0, "values")?, backend).map(Value::String)
        }
        ("neutral", "render_profile") => {
            let mut names = std::collections::HashMap::new();
            if let Some(entries) = kwargs
                .get("workspace_names")
                .and_then(|v| v.get("$integer_map"))
                .and_then(Value::as_array)
            {
                for entry in entries {
                    if let (Some(key), Some(value)) = (entry[0].as_i64(), entry[1].as_str()) {
                        names.insert(key, value.to_owned());
                    }
                }
            }
            let profile = argument(0, "profile")?;
            let contract = if backend == "hyprland"
                && profile.get("windowTrashBindings").is_some_and(truthy)
            {
                let text = std::fs::read_to_string(directory.join("compositor-contract.json"))
                    .map_err(|e| Error {
                        kind: "OSError",
                        message: e.to_string(),
                    })?;
                serde_json::from_str(&text).map_err(|e| Error {
                    kind: "JSONDecodeError",
                    message: e.to_string(),
                })?
            } else {
                Value::Null
            };
            render_profile(profile, backend, &names, &contract, &directory).map(Value::String)
        }
        ("neutral" | "niri", "rule_kdl") => rule_kdl(argument(0, "rule")?).map(Value::String),
        ("niri", "quoted") => quoted_kdl(argument(0, "value")?).map(Value::String),
        ("niri", "keys") => niri::keys(argument(0, "value")?).map(Value::String),
        ("niri", "binding_command") => niri::binding(argument(0, "row")?).map(Value::String),
        ("niri", "workspace_args") => {
            niri::workspace_args(argument(0, "value")?).map(|values| json!(values))
        }
        _ => Err(invalid(format!(
            "Unknown compiler operation: {module}.{operation}"
        ))),
    }
}
pub fn render(contract: &Value, directory: &Path, backend: &str) -> Result<Value> {
    match backend {
        "hyprland" => render_hyprland(contract, directory),
        "niri" => {
            render_hyprland(contract, directory)?;
            niri::render(contract)
        }
        other => Err(invalid(format!("Unsupported compositor backend: {other}"))),
    }
}
pub fn render_profile(
    profile: &Value,
    backend: &str,
    workspace_names: &std::collections::HashMap<i64, String>,
    contract: &Value,
    directory: &Path,
) -> Result<String> {
    let rules = profile_rules(profile)?;
    let rules = rules
        .as_array()
        .ok_or_else(|| invalid("Invalid profile rules"))?;
    if backend == "hyprland" {
        let mut text = rules
            .iter()
            .map(rule_lua)
            .collect::<Result<Vec<_>>>()?
            .join("\n");
        if !rules.is_empty() {
            text.push('\n');
        }
        if profile.get("windowTrashBindings").is_some_and(truthy) {
            let bindings =
                member(member(contract, "profileBindings")?, "windowTrashBindings")?.clone();
            let mut selected = contract.clone();
            selected
                .as_object_mut()
                .ok_or_else(|| invalid("Invalid compositor declaration fields"))?
                .shift_remove("profileBindings");
            for value in selected["outputs"]
                .as_object_mut()
                .ok_or_else(|| invalid("Invalid compositor declaration fields"))?
                .values_mut()
            {
                *value = json!([]);
            }
            selected["outputs"]["hypr/conf/keybinding.lua"] = bindings;
            let output = render_hyprland(&selected, directory)?;
            text.push_str(
                output["hypr/conf/keybinding.lua"]
                    .as_str()
                    .ok_or_else(|| invalid("Invalid profile output"))?,
            );
        }
        return Ok(text);
    }
    if backend != "niri" {
        return Err(invalid("Unknown compositor backend"));
    }
    let mut rows = Vec::new();
    for rule in rules {
        let mut rule = rule.clone();
        let target = &rule["effects"]["workspace"]["target"];
        let name = target["number"]
            .as_i64()
            .and_then(|number| workspace_names.get(&number))
            .ok_or_else(|| {
                invalid("Niri profile routes require an explicit workspace number-to-name map")
            })?;
        rule["effects"]["workspace"]["target"] = json!({"kind":"name","name":name});
        rows.push(rule_kdl(&rule)?);
    }
    Ok(rows.join("\n") + if rows.is_empty() { "" } else { "\n" })
}
pub fn render_keyboard(values: &Value, backend: &str) -> Result<String> {
    let values = values
        .as_object()
        .ok_or_else(|| invalid("Invalid keyboard values"))?;
    match backend {
        "hyprland" => {
            let mut lines = vec![
                "-- Your system keyboard choice. Edit here; Nacre updates preserve this file."
                    .into(),
                "hl.config({".into(),
            ];
            for (key, value) in values {
                lines.push(format!(
                    "    [\"input.kb_{key}\"] = {},",
                    runtime_lua_value(value)?
                ));
            }
            lines.push("})".into());
            Ok(lines.join("\n") + "\n")
        }
        "niri" => {
            if values.keys().any(|key| {
                !["layout", "model", "variant", "options", "rules"].contains(&key.as_str())
            }) {
                return Err(invalid("Unknown keyboard option"));
            }
            let mut text = "input {\n    keyboard {\n        xkb {\n".to_owned();
            for (key, value) in values {
                text.push_str(&format!(
                    "            {key} {}\n",
                    crate::json::encode(value).map_err(|e| invalid(e.to_string()))?
                ));
            }
            text.push_str("        }\n    }\n}\n");
            Ok(text)
        }
        _ => Err(invalid("Unknown compositor backend")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_frozen_python_compiler_outputs_and_failures() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/compositor-config-reference.json"
        ))?;
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools");
        let mut matched = 0;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let request = &case["request"];
            let args = request["args"].as_array().ok_or("Missing arguments")?;
            let operation = request["operation"].as_str().ok_or("Missing operation")?;
            let result = match operation {
                "supported_backends" => Ok(supported_backends()),
                "backend_capabilities" => {
                    backend_capabilities(args[0].as_str().ok_or("Missing backend")?)
                }
                "lua_value" => lua_value(&args[0]).map(Value::String),
                "workspace_target" => workspace_target(&args[0]),
                "profile_rules" => profile_rules(&args[0]),
                "render_keyboard" => render_keyboard(
                    &args[0],
                    request["kwargs"]["backend"]
                        .as_str()
                        .ok_or("Missing backend")?,
                )
                .map(Value::String),
                "render" => {
                    let mut contract = reference["contract"].clone();
                    match request["mutation"].as_str() {
                        Some("unknown-root") => contract["unknown"] = json!(true),
                        Some("bad-version") => contract["version"] = json!(0),
                        Some("unknown-action") => {
                            contract["outputs"]["hypr/conf/keybinding.lua"][0]["action"] =
                                json!("unknown")
                        }
                        Some("unknown-output") => contract["outputs"]["unknown.lua"] = json!([]),
                        Some("bad-selector") => {
                            contract["outputs"]["hypr/conf/windowrule.lua"][0]["selector"]
                                ["app_id_regex"] = json!(true)
                        }
                        Some("bad-float") => {
                            contract["outputs"]["hypr/conf/windowrule.lua"][0]["effects"]
                                ["floating"] = json!("false")
                        }
                        _ => {}
                    }
                    render(
                        &contract,
                        &directory,
                        request["kwargs"]["backend"].as_str().unwrap_or("hyprland"),
                    )
                }
                "quoted" => quoted_kdl(&args[0]).map(Value::String),
                "render_profile" => {
                    let mut names = std::collections::HashMap::new();
                    if let Some(entries) = request["workspace_names"].as_array() {
                        for entry in entries {
                            names.insert(
                                entry[0]
                                    .as_i64()
                                    .ok_or("Invalid reference workspace number")?,
                                entry[1]
                                    .as_str()
                                    .ok_or("Invalid reference workspace name")?
                                    .to_owned(),
                            );
                        }
                    }
                    render_profile(
                        &args[0],
                        request["kwargs"]["backend"]
                            .as_str()
                            .ok_or("Missing backend")?,
                        &names,
                        &reference["contract"],
                        &directory,
                    )
                    .map(Value::String)
                }
                _ => return Err(format!("Unexpected reference operation: {operation}").into()),
            };
            matched += 1;
            let (value, exit) = match result {
                Ok(value) => (value, 0),
                Err(error) => (json!({"error":error.message,"type":error.kind}), 1),
            };
            assert_eq!(
                crate::json::encode(&value)? + "\n",
                case["stdout"].as_str().ok_or("Missing stdout")?,
                "{}",
                request["name"]
            );
            assert_eq!(json!(exit), case["exit"], "{}", request["name"]);
        }
        assert_eq!(
            matched, 62,
            "Every frozen compiler case must run against the native implementation"
        );
        Ok(())
    }
}
