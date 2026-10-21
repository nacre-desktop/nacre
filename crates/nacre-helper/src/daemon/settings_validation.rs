//! The Settings registry's value contract, shared by RPC and hand-edit validation.
use serde_json::Value;
use std::sync::OnceLock;

pub fn descriptors() -> &'static [Value] {
    static REGISTRY: OnceLock<Value> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../nacre/shell/modules/settings/registry/settings.json"
        ))
        .expect("checked-in Settings registry")
    })["settings"]
        .as_array()
        .expect("settings array")
}

fn js_space(c: char) -> bool {
    matches!(c, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}' |
        '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' |
        '\u{3000}' | '\u{feff}')
}

pub fn valid_value(setting: &Value, value: &Value) -> bool {
    let kind = setting["type"].as_str().unwrap_or("");
    let valid = match kind {
        "boolean" => value.is_boolean(),
        "string" => value.is_string(),
        "array" => value.is_array(),
        "integer" => value
            .as_f64()
            .is_some_and(|n| n.is_finite() && n.fract() == 0.0),
        "number" => value.as_f64().is_some_and(f64::is_finite),
        "enum" => setting["options"].as_array().is_some_and(|options| {
            options.iter().any(|option| {
                let expected = &option["value"];
                expected == value
                    || expected.is_number()
                        && value.is_number()
                        && expected.as_f64() == value.as_f64()
            })
        }),
        _ => false,
    };
    if !valid {
        return false;
    }
    if kind == "array" {
        let items = value.as_array().expect("checked array");
        let entity = setting["entityType"].as_str().unwrap_or("");
        if [
            "app",
            "command",
            "text-list",
            "navigation-order",
            "navigation-hidden",
        ]
        .contains(&entity)
            && (items.len() > 512
                || items.iter().any(|item| {
                    item.as_str()
                        .is_none_or(|s| s.encode_utf16().count() > 4096 || s.contains('\0'))
                }))
        {
            return false;
        }
        if entity == "command"
            && items
                .first()
                .and_then(Value::as_str)
                .is_none_or(|s| s.trim_matches(js_space).is_empty())
        {
            return false;
        }
    }
    if let Some(range) = setting.get("range") {
        // All registry ranges currently have numeric types; no JS coercion is needed.
        let Some(number) = value.as_f64() else {
            return false;
        };
        let (Some(min), Some(max), Some(step)) = (
            range["min"].as_f64(),
            range["max"].as_f64(),
            range["step"].as_f64(),
        ) else {
            return false;
        };
        let steps = (number - min) / step;
        if number < min || number > max || (steps - steps.round()).abs() > 0.00001 {
            return false;
        }
    }
    true
}

fn developer_mode() -> bool {
    std::env::var("NACRE_DEVELOPER_MODE").is_ok_and(|value| value == "1")
}

fn value_with_developer(setting: &Value, value: &Value, developer: bool) -> bool {
    valid_value(setting, value)
        || developer
            && value == "pseudo"
            && setting["store"]["id"] == "desktop"
            && setting["store"]["key"]
                .as_array()
                .is_some_and(|key| key.len() == 1 && key[0] == "language")
}

/// The controller's existing developer-only language is not a public enum option.
/// Keep the raw registry validator exact while sharing its explicit runtime gate.
pub fn configured_value(setting: &Value, value: &Value) -> bool {
    value_with_developer(setting, value, developer_mode())
}

pub fn descriptor(store: &str, key: &[String]) -> Option<&'static Value> {
    descriptors().iter().find(|setting| {
        setting["backend"] != "planned"
            && setting["store"]["id"] == store
            && setting["store"]["key"].as_array().is_some_and(|path| {
                path.iter()
                    .map(Value::as_str)
                    .eq(key.iter().map(|s| Some(s.as_str())))
            })
    })
}

pub fn document(store: &str, data: &Value) -> bool {
    document_with_developer(store, data, developer_mode())
}

fn document_with_developer(store: &str, data: &Value, developer: bool) -> bool {
    if store == "battery-alerts" {
        return data.is_object()
            && data
                .get("severity")
                .is_none_or(|v| v.as_u64().is_some_and(|n| n <= 2));
    }
    descriptors()
        .iter()
        .filter(|s| s["store"]["id"] == store && s["backend"] != "planned")
        .all(|s| {
            let Some(path) = s["store"]["key"].as_array() else {
                return false;
            };
            let mut value = data;
            for key in path {
                let Some(next) = key.as_str().and_then(|key| value.get(key)) else {
                    return true;
                };
                value = next;
            }
            value_with_developer(s, value, developer)
        })
}

/// The existing confirmed launcher-history action clears these two owned
/// fields. It does not grant callers a generic writer for unregistered keys.
pub fn history_reset(key: &[String], value: &Value) -> bool {
    key.len() == 1
        && match key[0].as_str() {
            "recent" => value.as_array().is_some_and(Vec::is_empty),
            "usage" => value.as_object().is_some_and(serde_json::Map::is_empty),
            _ => false,
        }
}

/// The fields the existing native providers already produce. No new store or
/// path permission is granted; unrelated fields may only survive unchanged.
pub fn provider_field(store: &str, key: &str, value: &Value) -> bool {
    if let Some(setting) = descriptor(store, &[key.into()]) {
        return valid_value(setting, value);
    }
    match (store, key) {
        ("colors", "favorites") => value.as_array().is_some_and(|v| v.len() <= 64),
        ("colors", "history") => value.as_array().is_some_and(|v| v.len() <= 20),
        ("colors", "wallpapers") => value.is_object(),
        ("colors", "appThemes") => value.as_object().is_some_and(|values| {
            values.iter().all(|(key, value)| {
                [
                    "gtk3", "gtk4", "qt", "kde", "kitty", "rofi", "lock", "login",
                    "chromium", "discord", "spotify",
                ]
                .contains(&key.as_str())
                    && value.is_boolean()
            })
        }),
        ("colors", "lastApplied") => value.as_str().is_some_and(|s| s.len() <= 128),
        ("apps", "emoji") => value.as_array().is_some_and(|items| {
            !items.is_empty()
                && items.len() <= 32
                && items.iter().all(|v| {
                    v.as_str()
                        .is_some_and(|s| !s.contains('\0') && s.len() <= 4096)
                })
        }),
        ("launcher-workflows", "entries") => value.as_array().is_some_and(|items| {
            items.len() <= 300
                && items
                    .iter()
                    .all(|v| crate::launcher_workflows::validate(v).is_ok())
        }),
        ("launcher-workflows", "roots") => value.as_array().is_some_and(|items| {
            items.len() <= 32
                && items.iter().all(|v| {
                    v.as_str().is_some_and(|s| {
                        std::path::Path::new(s).is_absolute()
                            && s != "/"
                            && s.len() <= 4096
                            && !s.contains('\0')
                    })
                })
        }),
        ("wallpaper-picker", "rotationAnchorMs") => value.as_i64().is_some_and(|v| v >= 0),
        ("wallpaper-picker", "kind") => value
            .as_str()
            .is_some_and(|s| ["static", "dynamic"].contains(&s)),
        ("wallpaper-picker", "rotationKind") => value
            .as_str()
            .is_some_and(|s| ["all", "static", "dynamic"].contains(&s)),
        ("wallpaper-picker", "layout") => value
            .as_str()
            .is_some_and(|s| ["carousel", "spotlight", "hexagons"].contains(&s)),
        ("wallpaper-picker", "motionMode") => value
            .as_str()
            .is_some_and(|s| ["full", "battery", "still"].contains(&s)),
        ("wallpaper-picker", "paletteMode") => value
            .as_str()
            .is_some_and(|s| ["dark", "light"].contains(&s)),
        ("wallpaper-picker", "palettePersonality") => value.as_str().is_some_and(|s| {
            [
                "natural", "pigment", "harmony", "pop", "mist", "vivid", "pearl", "tide",
            ]
            .contains(&s)
        }),
        ("wallpaper-picker", "palettePreset") => value
            .as_str()
            .is_some_and(|s| s.len() <= 4096 && !s.contains('\0')),
        (
            "wallpaper-picker",
            "paused"
            | "pauseCovered"
            | "rotationEnabled"
            | "rotationShuffle"
            | "paletteHarmony"
            | "paletteAutoMode"
            | "paletteBackgroundFromWallpaper",
        ) => value.is_boolean(),
        ("wallpaper-picker", "rotationMinutes") => {
            value.as_i64().is_some_and(|n| (5..=1440).contains(&n))
        }
        _ => false,
    }
}

pub fn provider_key(store: &str, key: &str) -> bool {
    descriptor(store, &[key.into()]).is_some()
        || match store {
            "apps" => key == "emoji",
            "launcher-workflows" => ["entries", "roots"].contains(&key),
            "colors" => [
                "favorites",
                "history",
                "wallpapers",
                "lastApplied",
                "appThemes",
            ]
            .contains(&key),
            "wallpaper-picker" => [
                "kind",
                "rotationKind",
                "layout",
                "motionMode",
                "paletteMode",
                "palettePersonality",
                "palettePreset",
                "paused",
                "pauseCovered",
                "rotationEnabled",
                "rotationShuffle",
                "paletteHarmony",
                "paletteAutoMode",
                "paletteBackgroundFromWallpaper",
                "rotationMinutes",
                "rotationAnchorMs",
            ]
            .contains(&key),
            _ => false,
        }
}

/// All direct top-level keys owned by an existing native provider. This is
/// used only to verify a whole-document replacement's deletion intent.
pub fn provider_keys(store: &str) -> Vec<String> {
    let mut keys = descriptors()
        .iter()
        .filter(|setting| setting["store"]["id"] == store && setting["backend"] != "planned")
        .filter_map(|setting| {
            let path = setting["store"]["key"].as_array()?;
            (path.len() == 1)
                .then(|| path[0].as_str())
                .flatten()
                .filter(|key| provider_key(store, key))
                .map(str::to_owned)
        })
        .collect::<std::collections::BTreeSet<_>>();
    let extra: &[&str] = match store {
        "apps" => &["emoji"],
        "launcher-workflows" => &["entries", "roots"],
        "colors" => &[
            "favorites",
            "history",
            "wallpapers",
            "lastApplied",
            "appThemes",
        ],
        "wallpaper-picker" => &[
            "kind",
            "rotationKind",
            "layout",
            "motionMode",
            "paletteMode",
            "palettePersonality",
            "palettePreset",
            "paused",
            "pauseCovered",
            "rotationEnabled",
            "rotationShuffle",
            "paletteHarmony",
            "paletteAutoMode",
            "paletteBackgroundFromWallpaper",
            "rotationMinutes",
            "rotationAnchorMs",
        ],
        _ => &[],
    };
    keys.extend(extra.iter().map(|key| (*key).to_owned()));
    keys.into_iter().collect()
}

pub fn provider_change(store: &str, key: &str, value: &Value, previous: Option<&Value>) -> bool {
    if store == "cli" && key == "orient" {
        let Some(incoming) = value.as_object() else {
            return false;
        };
        let mut incoming = incoming.clone();
        let Some(accents) = incoming.remove("accents") else {
            return false;
        };
        let Some(accents) = accents.as_object() else {
            return false;
        };
        if accents.iter().any(|(path, value)| {
            path.contains('\0')
                || value
                    .as_str()
                    .is_none_or(|s| s.len() > 4096 || s.contains('\0'))
        }) {
            return false;
        }
        let mut old = previous
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default();
        old.remove("accents");
        return incoming == old;
    }
    if store == "colors" && key == "appThemes" {
        return value.as_object().is_some_and(|values| {
            values.iter().all(|(key, value)| {
                ([
                    "gtk3", "gtk4", "qt", "kde", "kitty", "rofi", "lock", "login",
                    "chromium", "discord", "spotify",
                ]
                .contains(&key.as_str())
                    && value.is_boolean())
                    || previous.and_then(|p| p.get(key)) == Some(value)
            })
        });
    }
    provider_field(store, key, value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn pseudo_language_keeps_the_existing_explicit_developer_gate() {
        let language = descriptor("desktop", &["language".into()]).unwrap();
        assert!(!valid_value(language, &json!("pseudo")));
        assert!(!value_with_developer(language, &json!("pseudo"), false));
        assert!(value_with_developer(language, &json!("pseudo"), true));
        assert!(value_with_developer(language, &json!("en"), false));
        assert!(!value_with_developer(language, &json!("unknown"), true));
        let other = json!({"type":"enum","options":[],"store":{"id":"desktop","key":["other"]}});
        assert!(!value_with_developer(&other, &json!("pseudo"), true));
    }

    #[test]
    fn developer_documents_keep_all_other_registry_checks() {
        let pseudo = json!({"language":"pseudo","dnd":true});
        assert!(!document_with_developer("desktop", &pseudo, false));
        assert!(document_with_developer("desktop", &pseudo, true));
        assert!(!document_with_developer(
            "desktop",
            &json!({"language":"pseudo","dnd":"invalid"}),
            true
        ));
    }

    // Keep the frozen JavaScript corpus unchanged; new descriptors receive live coverage here.
    #[test]
    fn app_theming_targets_have_fresh_boolean_validation_coverage() {
        for target in ["chromium", "discord", "spotify"] {
            let setting = descriptor("colors", &["appThemes".into(), target.into()]).unwrap();
            let expected_id = format!("colors/app-theming.targets.{target}");
            assert_eq!(setting["id"].as_str(), Some(expected_id.as_str()));
            assert!(valid_value(setting, &json!(true)));
            assert!(valid_value(setting, &json!(false)));
            assert!(!valid_value(setting, &json!("true")));
            let mut values = serde_json::Map::new();
            values.insert(target.into(), json!(true));
            let value = Value::Object(values);
            assert!(provider_field("colors", "appThemes", &value));
            assert!(provider_change(
                "colors",
                "appThemes",
                &value,
                Some(&json!({}))
            ));
        }
    }

    #[test]
    fn frozen_javascript_values_match_exactly() {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/settings-values-reference.json"
        ))
        .unwrap();
        let cases = reference["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 2258);
        for (index, case) in cases.iter().enumerate() {
            assert_eq!(
                valid_value(&case["setting"], &case["value"]),
                case["valid"].as_bool().unwrap(),
                "case {index}: {}",
                case["setting"]["id"]
            );
        }
    }
}
