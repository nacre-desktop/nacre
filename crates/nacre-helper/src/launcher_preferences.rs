//! Atomic launcher favorites, visibility and bounded recency/usage state.
use crate::{files, i18n::tr, paths::Paths, Result};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    os::unix::fs::PermissionsExt,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
fn invalid() -> crate::Error {
    tr(
        "helper.launcher-preferences.invalid-launcher-preferences-original-file-preserved",
        "Invalid launcher preferences; original file preserved",
    )
    .into()
}
pub fn load(path: &Path) -> Result<Value> {
    let data = crate::state::read_as(path, Some(crate::state::by_id("launcher")?), json!({}))?;
    normalized(&data)
}

pub fn normalized(data: &Value) -> Result<Value> {
    let data = data.as_object().ok_or_else(invalid)?;
    let mut result = serde_json::Map::new();
    for key in ["favorites", "hidden", "recent"] {
        let mut seen = HashSet::new();
        let mut items = Vec::new();
        if let Some(values) = data.get(key) {
            for value in values.as_array().ok_or_else(invalid)? {
                if let Some(value) = value.as_str() {
                    if seen.insert(value.to_owned()) {
                        items.push(json!(value));
                    }
                }
            }
        }
        result.insert(key.into(), Value::Array(items));
    }
    let mut usage = serde_json::Map::new();
    if let Some(entries) = data.get("usage").and_then(Value::as_object) {
        let cutoff = now() + 60.0;
        for (id, entry) in entries {
            let count = entry.get("count").and_then(Value::as_u64);
            let last = entry.get("last").and_then(Value::as_f64);
            if id.chars().count() <= 256
                && count.is_some_and(|v| v > 0)
                && last.is_some_and(|v| v >= 0.0 && v <= cutoff)
            {
                usage.insert(
                    id.clone(),
                    json!({"count":count.unwrap().min(1_000_000),"last":entry["last"]}),
                );
            }
        }
    }
    result.insert("usage".into(), Value::Object(usage));
    Ok(Value::Object(result))
}
pub fn initialize(path: &Path) -> Result<Value> {
    if !path.exists() {
        fs::create_dir_all(path.parent().ok_or("Invalid preferences path")?)?;
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
        {
            Ok(mut file) => {
                file.write_all(
                    crate::json::encode(&json!({"version":1,"favorites":[],"hidden":[]}))?
                        .as_bytes(),
                )?;
                fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    load(path)
}
pub fn change(path: &Path, action: &str, value: &Value) -> Result<Value> {
    change_request(action, value)?;
    let _guard = files::lock(&path.with_extension("lock"))?;
    let data = changed(load(path)?, action, value)?;
    crate::state::write_as(path, Some(crate::state::by_id("launcher")?), &data)?;
    Ok(data)
}

fn change_request<'a>(action: &str, value: &'a Value) -> Result<(&'static str, &'a str, bool)> {
    let key = match action {
        "favorite" => "favorites",
        "hide" => "hidden",
        "record" => "recent",
        _ => return Err("Unsupported helper operation".into()),
    };
    let id = value.get("id").and_then(Value::as_str).filter(|id| {
        !id.is_empty() && id.chars().count() <= 256 && !id.chars().any(|v| (v as u32) < 32)
    });
    let enabled = value.get("enabled").and_then(Value::as_bool);
    if id.is_none() || enabled.is_none() {
        return Err(tr(
            "helper.launcher-preferences.invalid-launcher-preference",
            "Invalid launcher preference",
        )
        .into());
    }
    let id = id.unwrap();
    let enabled = enabled.unwrap();
    Ok((key, id, enabled))
}

/// The same preference mutation with persistence owned by the daemon service.
pub fn changed(mut data: Value, action: &str, value: &Value) -> Result<Value> {
    let (key, id, enabled) = change_request(action, value)?;
    let items = data[key].as_array_mut().unwrap();
    items.retain(|value| value.as_str() != Some(id));
    if enabled {
        if action == "record" {
            items.insert(0, json!(id));
            items.truncate(12);
            let usage = data["usage"].as_object_mut().unwrap();
            let count = usage
                .get(id)
                .and_then(|entry| entry["count"].as_u64())
                .unwrap_or(0);
            usage.insert(
                id.into(),
                json!({"count":(count+1).min(1_000_000),"last":now()}),
            );
            let mut sorted: Vec<_> = usage
                .iter()
                .map(|(id, value)| (id.clone(), value.clone()))
                .collect();
            sorted.sort_by(|a, b| {
                b.1["last"]
                    .as_f64()
                    .unwrap()
                    .partial_cmp(&a.1["last"].as_f64().unwrap())
                    .unwrap()
            });
            data["usage"] = Value::Object(sorted.into_iter().take(512).collect());
        } else {
            items.push(json!(id));
        }
    }
    Ok(data)
}
pub fn execute(arguments: &[String], input: Option<Value>) -> Result<Option<Value>> {
    if let Some(value) = crate::daemon::menu::preferences_client(arguments, input.clone())? {
        return Ok(Some(value));
    }
    execute_direct(arguments, input)
}
pub fn execute_direct(arguments: &[String], input: Option<Value>) -> Result<Option<Value>> {
    let path = Paths::environment()?.config.join("launcher.json");
    match arguments {
        [action] if action == "load" && input.is_none() => Ok(Some(initialize(&path)?)),
        [action] if ["favorite", "hide", "record"].contains(&action.as_str()) => Ok(Some(change(
            &path,
            action,
            &input.ok_or("Invalid launcher preference")?,
        )?)),
        _ => Err("Unsupported helper operation".into()),
    }
}
