//! Palette results matching the QML command palette's distinct ordering.
use crate::{
    context::{array, Context},
    model, search,
};
use serde_json::{json, Value};

fn matching(row: &Value, words: &[&str]) -> bool {
    let haystack = format!(
        "{} {}",
        row["name"].as_str().unwrap_or(""),
        row["description"].as_str().unwrap_or("")
    )
    .to_lowercase();
    words.iter().all(|word| haystack.contains(word))
}

fn raw_slice(value: &str, utf16_offset: usize) -> &str {
    let mut units = 0;
    for (index, character) in value.char_indices() {
        if units >= utf16_offset {
            return &value[index..];
        }
        units += character.len_utf16();
        if units >= utf16_offset {
            return &value[index + character.len_utf8()..];
        }
    }
    ""
}

/// Return rows in QML Palette.qml order, retaining opaque engine operations.
pub fn query(context: &Context, query: &str) -> Vec<Value> {
    let normalized = query.to_lowercase();
    let q = normalized
        .strip_prefix('>')
        .map_or(normalized.as_str(), str::trim_start)
        .trim();
    if q.starts_with("brain:") {
        let value = raw_slice(query, 6).trim();
        return vec![json!({
            "id":"nacre-palette:brain-query", "kind":"action",
            "name":context.tr("ui.palette.name.search-brain-for", "Search brain for {query}", json!({"query":value})),
            "description":context.tr("extras.palette.description.open-the-brain-drawer", "Open the brain drawer", json!({})),
            "icon":"neurology", "operation":"legacy-action", "action":"brain-query",
            "value":value, "disabled":false
        })];
    }

    let words: Vec<_> = q.split_whitespace().collect();
    let actions: Vec<_> = context
        .legacy_actions
        .iter()
        .filter(|row| row["launcher"] != false)
        .filter(|row| matching(row, &words))
        .cloned()
        .collect();
    if q.is_empty() {
        return actions;
    }

    let windows: Vec<_> = array(&context.palette_windows)
        .iter()
        .filter_map(|window| {
            let name = window["title"].as_str().unwrap_or("");
            let workspace = window["workspace"]["id"].as_str().unwrap_or("");
            let description = format!("Window · workspace {workspace}");
            let matched = json!({"name":name,"description":description});
            matching(&matched, &words).then(|| {
                json!({
                    "id":format!("nacre-palette:window:{}", window["windowId"].as_str().unwrap_or("")),
                    "kind":"window", "name":name, "label":name, "description":description,
                    "icon":"window", "operation":"focus", "value":window["windowId"],
                    "disabled":false
                })
            })
        })
        .collect();

    let records: Vec<_> = context
        .visible_apps
        .iter()
        .map(|app| search::Prepared::new((**app).clone()))
        .collect();
    let apps = search::query(&records, query, &context.ordering)
        .into_iter()
        .take(8)
        .map(|app| {
            let mut row = model::application(app);
            row["description"] = json!(context.tr(
                "extras.palette.description.application",
                "Application",
                json!({})
            ));
            row["icon"] = json!("apps");
            row
        });
    actions.into_iter().chain(windows).chain(apps).collect()
}
