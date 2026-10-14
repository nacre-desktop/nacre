//! Private notification snapshots; preserve feedback filtering and publication modes.
use crate::{i18n::tr, json, paths::Paths, Result};
use regex::Regex;
use serde_json::Value;
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

const FIELDS: [&str; 8] = [
    "key", "time", "summary", "body", "appName", "appIcon", "image", "urgency",
];
fn retained(row: &Value, policy: &Value) -> Result<bool> {
    let row = row.as_object().ok_or_else(|| {
        tr(
            "helper.notification-history.invalid-notification-history",
            "Invalid notification history",
        )
    })?;
    if row.get("transient").is_some_and(truthy) {
        return Ok(false);
    }
    let app = row
        .get("appName")
        .map(|v| v.as_str().ok_or("Invalid notification history"))
        .transpose()?
        .unwrap_or("")
        .to_lowercase();
    if !policy["feedbackApps"]
        .as_array()
        .ok_or("Invalid notification policy")?
        .iter()
        .any(|v| v.as_str() == Some(&app))
    {
        return Ok(true);
    }
    let summary = row
        .get("summary")
        .map(|v| v.as_str().ok_or("Invalid notification history"))
        .transpose()?
        .unwrap_or("");
    if policy["feedbackSummaries"]
        .as_array()
        .ok_or("Invalid notification policy")?
        .iter()
        .any(|v| v.as_str() == Some(summary))
    {
        return Ok(false);
    }
    for pattern in policy["feedbackPatterns"]
        .as_array()
        .ok_or("Invalid notification policy")?
    {
        let expression = Regex::new(pattern.as_str().ok_or("Invalid notification policy")?)?;
        if expression.is_match(summary)
            || summary
                .strip_suffix('\n')
                .is_some_and(|text| expression.is_match(text))
        {
            return Ok(false);
        }
    }
    Ok(true)
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
fn policy(paths: &Paths) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(
        paths.assets().join("notification-policy.json"),
    )?)?)
}
pub fn load(path: &Path, policy: &Value) -> Result<Value> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(Value::Array(Vec::new()))
        }
        Err(error) => return Err(error.into()),
    };
    let rows: Value = serde_json::from_slice(&bytes)?;
    let rows = rows.as_array().ok_or_else(|| {
        tr(
            "helper.notification-history.invalid-notification-history",
            "Invalid notification history",
        )
    })?;
    let mut result = Vec::new();
    for row in rows {
        if retained(row, policy)? {
            result.push(row.clone());
        }
    }
    Ok(Value::Array(result))
}
pub fn save(path: &Path, rows: &Value, policy: &Value) -> Result<()> {
    let rows = rows
        .as_array()
        .filter(|rows| rows.iter().all(Value::is_object))
        .ok_or_else(|| {
            tr(
                "helper.notification-history.invalid-notification-history",
                "Invalid notification history",
            )
        })?;
    let mut selected = Vec::new();
    for row in rows {
        if retained(row, policy)? {
            let mut display = serde_json::Map::new();
            for key in FIELDS {
                if let Some(value) = row.get(key) {
                    display.insert(key.into(), value.clone());
                }
            }
            selected.push(Value::Object(display));
        }
    }
    let parent = path.parent().ok_or("Invalid notification history path")?;
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    use std::io::Write;
    file.write_all(json::encode(&selected)?.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    Ok(())
}
pub fn execute(arguments: &[String], input: Option<Value>) -> Result<Option<Value>> {
    let paths = Paths::environment()?;
    let path = paths.state.join("shell/notifications/history.json");
    let policy = policy(&paths)?;
    match arguments {
        [action] if action == "load" && input.is_none() => Ok(Some(load(&path, &policy)?)),
        [action] if action == "save" => {
            save(
                &path,
                &input.ok_or_else(|| {
                    tr(
                        "helper.notification-history.invalid-notification-history",
                        "Invalid notification history",
                    )
                })?,
                &policy,
            )?;
            Ok(None)
        }
        _ => Err(tr(
            "helper.notification-history.unknown-history-action",
            "Unknown history action",
        )
        .into()),
    }
}
