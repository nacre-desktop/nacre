//! Existing helper English catalog and pseudo-language behavior.
use crate::paths::Paths;
use serde_json::Value;
use std::{env, fs, sync::OnceLock};

fn english() -> &'static Value {
    static CATALOG: OnceLock<Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        if let Ok(paths) = Paths::environment() {
            let assets = paths.assets();
            if let Some(parent) = assets.parent() {
                for name in ["source/i18n/en.json", "shell/i18n/en.json"] {
                    if let Ok(bytes) = fs::read(parent.join(name)) {
                        if let Ok(value) = serde_json::from_slice::<Value>(&bytes) {
                            if value.is_object() {
                                return value;
                            }
                        }
                    }
                }
            }
        }
        Value::Null
    })
}
fn language() -> String {
    if let Ok(value) = env::var("NACRE_LANGUAGE") {
        if !value.is_empty() {
            return value;
        }
    }
    Paths::environment()
        .ok()
        .and_then(|paths| fs::read(paths.config.join("desktop.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|value| value["language"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "system".into())
}
pub use nacre_i18n::pseudo;
pub fn tr(identifier: &str, fallback: &str) -> String {
    nacre_i18n::translate(english(), &language(), identifier, fallback, &Value::Null)
}
/// The catalog text of an identifier from data (a manifest), or `None` when it has none.
pub fn catalog_text(identifier: &str) -> Option<String> {
    let text = nacre_i18n::translate(english(), &language(), identifier, "", &Value::Null);
    (!text.is_empty()).then_some(text)
}
