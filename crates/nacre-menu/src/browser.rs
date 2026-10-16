//! Existing action-provider matching and desktop category groups.
use crate::{model, text};
use serde_json::{json, Value};

pub fn groups() -> Vec<Value> {
    let mut values = vec![
        json!({"id":"favorites","label":"Favorites","icon":"favorite"}),
        json!({"id":"actions","label":"Actions","icon":"bolt"}),
        json!({"id":"all","label":"All apps","icon":"apps"}),
    ];
    for (id, label, icon, categories) in [
        ("development", "Development", "code", &["Development"][..]),
        (
            "media",
            "Media",
            "movie",
            &["AudioVideo", "Audio", "Video"][..],
        ),
        ("games", "Games", "sports_esports", &["Game"][..]),
        ("graphics", "Graphics", "palette", &["Graphics"][..]),
        ("internet", "Internet", "public", &["Network"][..]),
        ("office", "Office", "description", &["Office"][..]),
        (
            "education",
            "Education",
            "school",
            &["Education", "Science"][..],
        ),
        ("system", "System", "settings", &["System", "Settings"][..]),
        ("utilities", "Utilities", "build", &["Utility"][..]),
    ] {
        values.push(json!({"id":id,"label":label,"icon":icon,"categories":categories}));
    }
    values
}
pub fn available(apps: &[Value]) -> Vec<Value> {
    groups()
        .into_iter()
        .filter(|g| {
            g["categories"].as_array().is_none_or(|categories| {
                apps.iter().any(|app| {
                    app["categories"]
                        .as_array()
                        .is_some_and(|values| categories.iter().any(|c| values.contains(c)))
                })
            })
        })
        .collect()
}
pub fn actions(items: &[Value]) -> Vec<Value> {
    items
        .iter()
        .filter(|item| item["action"] != "power")
        .map(|item| {
            let mut result = item.clone();
            result["id"] = json!(format!(
                "nacre-action:{}:{}",
                text::field(item, "action"),
                text::uri_component(&text::field(item, "value"))
            ));
            result["kind"] = json!("action");
            result
        })
        .collect()
}
pub fn ranked_actions<'a>(items: &'a [Value], query: &str) -> Vec<&'a Value> {
    let term = model::normalized(query);
    let words: Vec<_> = term.split(' ').filter(|v| !v.is_empty()).collect();
    let mut result = Vec::new();
    for item in items {
        let aliases: Vec<String> = item["aliases"]
            .as_array()
            .map(|v| v.iter().map(text::string).collect())
            .unwrap_or_default();
        let haystack = model::normalized(
            &[
                text::field(item, "name"),
                text::field(item, "description"),
                text::field(item, "action"),
                text::field(item, "value"),
                aliases.join(" "),
            ]
            .join(" "),
        );
        if !words.iter().all(|word| haystack.contains(*word)) {
            continue;
        }
        let score = if model::normalized(&text::field(item, "action")) == term {
            100
        } else if aliases.iter().any(|alias| model::normalized(alias) == term) {
            80
        } else if model::normalized(&text::field(item, "name")).starts_with(&term) {
            60
        } else {
            40
        };
        result.push((item, score));
    }
    if !term.is_empty() {
        result.sort_by_key(|a| std::cmp::Reverse(a.1));
    }
    result.into_iter().map(|(item, _)| item).collect()
}

/// Materialize the old DesktopActions inputs without a UI singleton dependency.
pub fn default_actions(context: &crate::context::Context) -> Vec<Value> {
    let input: Vec<Value> = crate::context::array(&context.constants["legacyActions"])
        .iter()
        .map(|entry| {
            let mut entry = entry.clone();
            if context.settings[entry["value"].as_str().unwrap_or("")] == false
                && entry.get("disabledName").is_some()
            {
                entry["name"] = entry["disabledName"].clone();
                entry["nameId"] = entry["disabledNameId"].clone();
            }
            entry = context.localized(&entry);
            for field in ["nameId", "descriptionId", "disabledName", "disabledNameId"] {
                entry.as_object_mut().unwrap().remove(field);
            }
            entry
        })
        .collect();
    actions(&input)
        .into_iter()
        .map(|mut entry| {
            entry["operation"] = json!("legacy-action");
            entry["label"] = entry["name"].clone();
            entry["breadcrumb"] = json!("Actions");
            entry
        })
        .collect()
}
