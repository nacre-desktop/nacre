//! Existing menu overrides, validation and built-in fallback behavior.
use crate::{
    context::{array, yes},
    model, text,
};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
const OPERATIONS: &[&str] = &[
    "tool",
    "route",
    "calculator",
    "legacy",
    "scope",
    "toggle",
    "next-wallpaper",
    "personality",
    "background",
    "palette-mode",
    "setting-toggle",
    "settings",
    "config",
    "form",
    "maintenance",
    "info",
    "url",
    "power",
    "power-profile",
    "command",
    "media",
    "bring-visitors",
];
const PROVIDERS: &[&str] = &[
    "apps",
    "wallpapers",
    "accents",
    "fonts",
    "software",
    "packages",
    "installed",
    "windows",
    "wifi",
    "bluetooth",
    "clipboard",
    "keys",
    "emoji",
    "timers",
    "webapps",
    "audio",
    "software-themes",
    "software-extensions",
    "environments",
    "software-ai",
    "tui-apps",
    "health-fixits",
    "system-setup",
    "system-hardware",
    "system-speed",
];
const FIELDS: &[&str] = &[
    "labelId",
    "descriptionId",
    "confirmId",
    "id",
    "kind",
    "label",
    "description",
    "icon",
    "operation",
    "value",
    "provider",
    "capability",
    "state",
    "confirm",
    "brand",
    "aliases",
    "hidden",
    "arguments",
];
pub type Translator<'a> = dyn Fn(&str, &str, Value) -> String + 'a;
fn message(tr: &Translator<'_>, id: &str, fallback: &str) -> String {
    tr(id, fallback, json!({}))
}
fn plain(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|s| s.encode_utf16().count() <= 4096 && !s.contains('\0'))
}
fn id_valid(id: &str) -> bool {
    id.len() <= 120
        && id.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && id.split('.').enumerate().all(|(i, p)| {
            !p.is_empty()
                && p.bytes().enumerate().all(|(j, c)| {
                    c.is_ascii_lowercase()
                        || c.is_ascii_digit()
                        || c == b'-'
                        || (i == 0 && j == 0 && c.is_ascii_lowercase())
                })
        })
}
fn validate(row: &Value, partial: bool, tr: &Translator<'_>) -> std::result::Result<(), String> {
    let object = row
        .as_object()
        .filter(|_| row["id"].as_str().is_some_and(id_valid))
        .ok_or_else(|| {
            message(
                tr,
                "launcher.validation.menu-entries-need-a-lowercase-dotted-id",
                "Menu entries need a lowercase dotted ID",
            )
        })?;
    for (key, value) in object {
        if !FIELDS.contains(&key.as_str()) {
            return Err(tr(
                "launcher.validation.unknown-menu-field",
                "Unknown menu field: {key}",
                json!({"key":key}),
            ));
        }
        if key == "hidden" && !value.is_boolean() {
            return Err(message(
                tr,
                "launcher.validation.hidden-must-be-true-or-false",
                "Hidden must be true or false",
            ));
        }
        if !["hidden", "arguments", "aliases"].contains(&key.as_str()) && !plain(value) {
            return Err(tr(
                "launcher.validation.invalid-menu-text",
                "Invalid menu text: {key}",
                json!({"key":key}),
            ));
        }
    }
    if !partial
        && (!matches!(row["kind"].as_str(), Some("group" | "action"))
            || row["label"]
                .as_str()
                .is_none_or(|s| text::trim(s).is_empty()))
    {
        return Err(message(
            tr,
            "launcher.validation.every-menu-entry-needs-a-kind-and-label",
            "Every menu entry needs a kind and label",
        ));
    }
    if row.get("kind").is_some() && !matches!(row["kind"].as_str(), Some("group" | "action")) {
        return Err(message(
            tr,
            "launcher.validation.unknown-menu-kind",
            "Unknown menu kind",
        ));
    }
    if row.get("operation").is_some()
        && !OPERATIONS.contains(&row["operation"].as_str().unwrap_or(""))
    {
        return Err(message(
            tr,
            "launcher.validation.unknown-menu-operation",
            "Unknown menu operation",
        ));
    }
    if row.get("provider").is_some() && !PROVIDERS.contains(&row["provider"].as_str().unwrap_or(""))
    {
        return Err(message(
            tr,
            "launcher.validation.unknown-menu-provider",
            "Unknown menu provider",
        ));
    }
    for field in ["aliases", "arguments"] {
        if let Some(value) = row.get(field) {
            if value
                .as_array()
                .is_none_or(|v| v.len() > 32 || v.iter().any(|s| !plain(s) || !yes(s)))
            {
                return Err(tr(
                    "launcher.validation.invalid-field",
                    "Invalid {field}",
                    json!({"field":field}),
                ));
            }
        }
    }
    if !partial
        && row["kind"] == "action"
        && !OPERATIONS.contains(&row["operation"].as_str().unwrap_or(""))
    {
        return Err(message(
            tr,
            "launcher.validation.an-action-needs-a-supported-operation",
            "An action needs a supported operation",
        ));
    }
    if row["operation"] == "command"
        && (array(&row["arguments"]).is_empty()
            || row["arguments"][0]
                .as_str()
                .is_some_and(|s| s.starts_with('-')))
    {
        return Err(message(
            tr,
            "launcher.validation.commands-use-an-explicit-executable-and-argument-array",
            "Commands use an explicit executable and argument array",
        ));
    }
    Ok(())
}
fn document(text: &str, tr: &Translator<'_>) -> std::result::Result<Value, String> {
    if text.encode_utf16().count() > 2097152 {
        return Err(message(
            tr,
            "launcher.validation.menu-file-is-too-large",
            "Menu file is too large",
        ));
    }
    let value: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if !value.is_object()
        || value["version"] != 1
        || value["entries"].as_array().is_none_or(|v| v.len() > 1000)
    {
        return Err(message(
            tr,
            "launcher.validation.unsupported-menu-format",
            "Unsupported menu format",
        ));
    }
    Ok(value)
}
fn check_tree(rows: &[Value], tr: &Translator<'_>) -> std::result::Result<(), String> {
    let by_id: BTreeMap<_, _> = rows
        .iter()
        .map(|r| (r["id"].as_str().unwrap_or(""), r))
        .collect();
    if by_id
        .get("home")
        .is_none_or(|r| r["kind"] != "group" || yes(&r["hidden"]))
    {
        return Err(message(
            tr,
            "launcher.validation.home-must-remain-available",
            "Home must remain available",
        ));
    }
    let mut aliases = BTreeMap::new();
    for row in rows {
        validate(row, false, tr)?;
        let id = row["id"].as_str().unwrap();
        let parent = model::parent_of(id);
        if !parent.is_empty() && by_id.get(parent).is_none_or(|r| r["kind"] != "group") {
            return Err(format!(
                "{}{id}",
                message(
                    tr,
                    "launcher.validation.missing-group-for",
                    "Missing group for "
                )
            ));
        }
        for alias in array(&row["aliases"]) {
            let key = text::trim(alias.as_str().unwrap()).to_lowercase();
            if aliases.get(&key).is_some_and(|old| *old != id) {
                return Err(message(
                    tr,
                    "launcher.validation.ambiguous-menu-alias",
                    "Ambiguous menu alias",
                ));
            }
            let reserved = match key.as_str() {
                "favorites" => Some("home"),
                "all" => Some("apps"),
                "actions" => Some("actions"),
                "settings" => Some("setup"),
                "system" => Some("power"),
                "clipboard" => Some("actions.clipboard"),
                "keys" => Some("learn.shortcuts"),
                "overview" => Some("actions.windows"),
                "palette" => Some("style"),
                "wallpaper" => Some("style.wallpapers"),
                _ => None,
            };
            if reserved.is_some_and(|v| v != id) {
                return Err(message(
                    tr,
                    "launcher.validation.reserved-route-alias",
                    "Reserved route alias",
                ));
            }
            aliases.insert(key, id);
        }
    }
    Ok(())
}
fn visible(rows: &[Value]) -> Vec<Value> {
    rows.iter()
        .filter(|r| {
            !yes(&r["hidden"])
                && !rows.iter().any(|parent| {
                    yes(&parent["hidden"])
                        && text::field(r, "id")
                            .starts_with(&format!("{}.", text::field(parent, "id")))
                })
        })
        .cloned()
        .collect()
}
pub fn merge_with(
    base_text: &str,
    user_text: &str,
    tr: &Translator<'_>,
) -> std::result::Result<Value, String> {
    let base = document(base_text, tr)?;
    let rows = array(&base["entries"]);
    for row in rows {
        validate(row, false, tr)?;
    }
    let ids: BTreeMap<String, usize> = rows
        .iter()
        .enumerate()
        .map(|(i, r)| (text::field(r, "id"), i))
        .collect();
    if ids.len() != rows.len() {
        return Err(message(
            tr,
            "launcher.validation.duplicate-menu-id",
            "Duplicate menu ID",
        ));
    }
    let mut error = String::new();
    if !text::trim(user_text).is_empty() {
        let result = (|| -> std::result::Result<Vec<Value>, String> {
            let extension = document(user_text, tr)?;
            let mut candidate = rows.to_vec();
            let mut positions = ids.clone();
            let mut seen = BTreeSet::new();
            for row in array(&extension["entries"]) {
                let id = text::field(row, "id");
                validate(row, positions.contains_key(&id), tr)?;
                if !seen.insert(id.clone()) {
                    return Err(message(
                        tr,
                        "launcher.validation.duplicate-extension-id",
                        "Duplicate extension ID",
                    ));
                }
                if let Some(index) = positions.get(&id) {
                    let old = candidate[*index].as_object_mut().unwrap();
                    for field in ["label", "description", "confirm"] {
                        if row.get(field).is_some() && row.get(format!("{field}Id")).is_none() {
                            old.remove(&format!("{field}Id"));
                        }
                    }
                    for (key, value) in row.as_object().unwrap() {
                        old.insert(key.clone(), value.clone());
                    }
                } else {
                    positions.insert(id, candidate.len());
                    candidate.push(row.clone());
                }
            }
            check_tree(&candidate, tr)?;
            Ok(candidate)
        })();
        match result {
            Ok(candidate) => {
                return Ok(
                    json!({"rows":visible(&candidate),"software":base.get("software").cloned().unwrap_or(json!([])),"fonts":base.get("fonts").cloned().unwrap_or(json!([])),"error":""}),
                )
            }
            Err(message) => {
                error = tr(
                    "launcher.validation.extension",
                    "Menu extension: {error}. Built-in entries remain available.",
                    json!({"error":message}),
                );
            }
        }
    }
    check_tree(rows, tr)?;
    Ok(
        json!({"rows":visible(rows),"software":base.get("software").cloned().unwrap_or(json!([])),"fonts":base.get("fonts").cloned().unwrap_or(json!([])),"error":error}),
    )
}
pub fn merge(base_text: &str, user_text: &str) -> std::result::Result<Value, String> {
    merge_with(base_text, user_text, &|_, fallback, args| {
        let mut value = fallback.to_owned();
        if let Some(args) = args.as_object() {
            for (key, arg) in args {
                value = value.replace(&format!("{{{key}}}"), &text::literal(arg));
            }
        }
        value
    })
}
