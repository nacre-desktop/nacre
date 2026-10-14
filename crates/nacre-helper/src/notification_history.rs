//! Private notification snapshots; preserve feedback filtering and publication modes.
use crate::{i18n::tr, paths::Paths, Result};
use regex::Regex;
use serde_json::Value;
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

const RETENTION_SECONDS: i64 = 30 * 24 * 60 * 60;

fn within_retention(row: &Value, now: i64) -> bool {
    // Old snapshots without a valid date cannot be aged reliably.
    row["time"]
        .as_str()
        .and_then(|stamp| chrono::DateTime::parse_from_rfc3339(stamp).ok())
        .is_none_or(|stamp| stamp.timestamp() > now - RETENTION_SECONDS)
}

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
    let rows = crate::state::read_as(
        path,
        Some(crate::state::by_id("notifications")?),
        Value::Array(Vec::new()),
    )?;
    let rows = rows.as_array().ok_or_else(|| {
        tr(
            "helper.notification-history.invalid-notification-history",
            "Invalid notification history",
        )
    })?;
    let mut result = Vec::new();
    for row in rows {
        if retained(row, policy)? && within_retention(row, chrono::Utc::now().timestamp()) {
            result.push(row.clone());
        }
    }
    if rows
        .iter()
        .any(|row| !within_retention(row, chrono::Utc::now().timestamp()))
    {
        // Also remove expired snapshots from disk when history is opened.
        save(path, &Value::Array(result.clone()), policy)?;
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
        if retained(row, policy)? && within_retention(row, chrono::Utc::now().timestamp()) {
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
    crate::state::write_as(
        path,
        Some(crate::state::by_id("notifications")?),
        &Value::Array(selected),
    )?;
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

#[cfg(test)]
mod retention_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn thirty_days_boundary_and_unknown_dates() {
        let now = 1_800_000_000;
        let row = |seconds| json!({"time": chrono::DateTime::from_timestamp(seconds, 0).unwrap().to_rfc3339()});
        assert!(within_retention(&row(now - RETENTION_SECONDS + 1), now));
        assert!(!within_retention(&row(now - RETENTION_SECONDS), now));
        assert!(!within_retention(&row(now - RETENTION_SECONDS - 1), now));
        assert!(within_retention(&json!({}), now));
        assert!(within_retention(&json!({"time":"invalid"}), now));
    }

    #[test]
    fn expired_snapshots_are_removed_on_load_and_save() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("history.json");
        let policy = serde_json::from_str(include_str!(
            "../../../nacre/shell-tools/notification-policy.json"
        ))?;
        let rows = json!([
            {"key":"old", "time":"2000-01-01T00:00:00Z", "appName":"Demo"},
            {"key":"recent", "time":chrono::Utc::now().to_rfc3339(), "appName":"Demo"}
        ]);
        fs::write(&path, serde_json::to_vec(&rows)?)?;
        assert_eq!(load(&path, &policy)?[0]["key"], "recent");
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&path)?)?["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        save(&path, &rows, &policy)?;
        assert_eq!(load(&path, &policy)?.as_array().unwrap().len(), 1);
        Ok(())
    }
}
