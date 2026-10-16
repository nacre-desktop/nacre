//! Existing source gates and literal web-search URLs.
use crate::text;
use serde_json::{json, Value};
pub fn defaults() -> Value {
    json!({"version":1,"brain":true,"files":true,"calculator":true,"actions":true,"ai":true,"webEngine":"duckduckgo"})
}
pub fn decode(text: &str) -> Value {
    let mut result = defaults();
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return result;
    };
    if !value.is_object() || value["version"] != 1 {
        return result;
    }
    for key in ["brain", "files", "calculator", "actions", "ai"] {
        if value.get(key).is_some_and(|v| !v.is_boolean()) {
            return result;
        }
    }
    if value
        .get("webEngine")
        .is_some_and(|v| !matches!(v.as_str(), Some("duckduckgo" | "google" | "brave")))
    {
        return result;
    }
    for key in ["brain", "files", "calculator", "actions", "ai", "webEngine"] {
        if let Some(v) = value.get(key) {
            result[key] = v.clone();
        }
    }
    result
}
pub fn web_url(engine: &str, query: &str) -> String {
    format!(
        "{}{}",
        match engine {
            "google" => "https://www.google.com/search?q=",
            "brave" => "https://search.brave.com/search?q=",
            _ => "https://duckduckgo.com/?q=",
        },
        text::uri_component(query)
    )
}
