use nacre_menu::{context::Context, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::io::Read;

fn reference() -> Result<Value> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(include_bytes!("fixtures/providers.json.gz").as_slice())
        .read_to_end(&mut raw)?;
    assert_eq!(
        format!("{:x}", Sha256::digest(&raw)),
        "3f00ad038253c7ad0e3e0e235dae0d5285a18397071aaeb9aeabf5d722eb840e"
    );
    Ok(serde_json::from_slice(&raw)?)
}
fn context(reference: &Value, name: &str) -> Result<Context> {
    let seed = &reference["contexts"][name];
    let mut catalog: Value = serde_json::from_str(include_str!("fixtures/base-menu.json"))?;
    catalog["rows"] = catalog["entries"].clone();
    let constants = serde_json::from_str(include_str!("../data/defaults.json"))?;
    let mut c = Context::new(catalog, constants, "en-US", 1720000000.0)?;
    for (out, key) in [
        (&mut c.snapshot, "snapshot"),
        (&mut c.extras, "DesktopExtras"),
        (&mut c.audio, "NacreAudio"),
        (&mut c.network, "NacreNetwork"),
        (&mut c.bluetooth, "NacreBluetooth"),
        (&mut c.wallpapers, "NacreWallpapers"),
        (&mut c.compositor, "NacreCompositor"),
        (&mut c.power, "PowerProfiles"),
        (&mut c.updates, "Updates"),
        (&mut c.previews, "previews"),
        (&mut c.preferences, "LauncherPreferences"),
    ] {
        *out = seed[key].clone();
    }
    c.settings = seed["DesktopSettings"]["data"].clone();
    c.controls = seed["NacreControlTools"]["data"].clone();
    c.players = seed["NacrePlayers"].clone();
    c.workflows = seed["workflows"]["data"].clone();
    c.library = seed["library"]["data"].clone();
    c.personality = seed["personality"].as_str().unwrap_or("").into();
    c.recording_active = seed["recordingActive"] == true;
    c.can_ask = seed["canAsk"] == true;
    if let Some(options) = seed.get("sourceOptions") {
        c.source_options = options.clone();
    }
    if name != "empty" {
        c.all_apps = reference["rawApps"].as_array().unwrap().clone();
        c.visible_apps = c.all_apps[..2].to_vec();
    } else {
        c.emoji_entries.clear();
    }
    c.audio["sink_id"] = c.audio["outputs"][0]["id"].clone();
    c.utc_offset_seconds = Some(0);
    for (out, key) in [
        (&mut c.brain_query, "brainQuery"),
        (&mut c.file_query, "fileQuery"),
        (&mut c.package_query, "packageQuery"),
    ] {
        *out = seed[key].as_str().unwrap_or("").into();
    }
    for (out, key) in [
        (&mut c.notes, "notes"),
        (&mut c.files, "fileRows"),
        (&mut c.packages, "packageRows"),
    ] {
        *out = seed[key].as_array().cloned().unwrap_or_default();
    }
    Ok(c)
}
fn difference(a: &Value, b: &Value, path: &str) -> String {
    if a == b {
        return String::new();
    }
    if let (Some(a), Some(b)) = (a.as_array(), b.as_array()) {
        if a.len() != b.len() {
            return format!("{path}.length {} != {}", a.len(), b.len());
        }
        for (i, (a, b)) in a.iter().zip(b).enumerate() {
            if a != b {
                return difference(a, b, &format!("{path}[{i}]"));
            }
        }
    }
    if let (Some(a), Some(b)) = (a.as_object(), b.as_object()) {
        for key in a.keys().chain(b.keys()) {
            if a.get(key) != b.get(key) {
                return match (a.get(key), b.get(key)) {
                    (Some(a), Some(b)) => difference(a, b, &format!("{path}.{key}")),
                    _ => format!("{path}.{key} optional presence differs"),
                };
            }
        }
    }
    let short = |v: &Value| v.to_string().chars().take(140).collect::<String>();
    format!("{path} {} != {}", short(a), short(b))
}
#[test]
fn frozen_provider_functions_keep_full_rows_and_optional_fields() -> Result<()> {
    let r = reference()?;
    let mut errors = Vec::new();
    let mut checked = 0;
    for name in [
        "empty",
        "populated",
        "disabledSources",
        "busyAndUnavailable",
    ] {
        let c = context(&r, name)?;
        for case in r["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["context"] == name && case["method"] == "provider")
        {
            checked += 1;
            let params = case["params"].as_array().unwrap();
            let route = params[0].as_str().unwrap();
            let query = params[1].as_str().unwrap();
            match c.provider(route, query) {
                Ok(rows) => {
                    if json!(rows) != case["result"] {
                        errors.push(format!(
                            "{name} {route} {query:?}: {}",
                            difference(&json!(rows), &case["result"], "$")
                        ));
                    }
                }
                Err(e) => errors.push(format!("{name} {route} {query:?}: {e}")),
            }
        }
    }
    assert_eq!(checked, 316);
    assert!(
        errors.is_empty(),
        "{} differing provider cases; first{:?}",
        errors.len(),
        &errors[..errors.len().min(12)]
    );
    Ok(())
}

#[test]
fn frozen_content_composition_ranking_and_source_gates_match() -> Result<()> {
    let r = reference()?;
    let mut errors = Vec::new();
    let mut checked = 0;
    for name in [
        "empty",
        "populated",
        "disabledSources",
        "busyAndUnavailable",
    ] {
        let c = context(&r, name)?;
        for case in r["cases"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|case| case["context"] == name && case["method"] == "content")
        {
            checked += 1;
            let p = case["params"].as_array().unwrap();
            let route = p[0].as_str().unwrap();
            let query = p[1].as_str().unwrap();
            let filter = p.get(2).and_then(Value::as_str).unwrap_or("all");
            let category = p.get(3).and_then(Value::as_str).unwrap_or("all");
            let indexed = c.indexed_content(route, query, filter, category)?;
            if json!(indexed) != case["result"] {
                errors.push(format!(
                    "indexed {name} {route} {query:?} {filter}: {}",
                    difference(&json!(indexed), &case["result"], "$")
                ));
            }
            match c.content(route, query, filter, category) {
                Ok(rows) => {
                    if json!(rows) != case["result"] {
                        errors.push(format!(
                            "{name} {route} {query:?} {filter}: {}",
                            difference(&json!(rows), &case["result"], "$")
                        ));
                    }
                }
                Err(e) => errors.push(format!("{name} {route} {query:?}: {e}")),
            }
        }
    }
    assert_eq!(checked, 228);
    assert!(
        errors.is_empty(),
        "{} differing content cases; first{:?}",
        errors.len(),
        &errors[..errors.len().min(15)]
    );
    Ok(())
}

#[test]
fn cached_generations_update_desktop_inputs_and_keep_query_data_and_history_live() -> Result<()> {
    let r = reference()?;
    let mut c = context(&r, "populated")?;
    c.indexed_content("apps", "application", "all", "all")?;
    c.visible_apps[0]["name"] = json!("Unique changed application");
    c.inputs_changed();
    let after = c.indexed_content("apps", "unique changed", "all", "all")?;
    assert!(after
        .iter()
        .any(|v| v["name"] == "Unique changed application"));
    assert_eq!(after, c.content("apps", "unique changed", "all", "all")?);
    // These are per-query inputs, not part of the cached desktop generation.
    c.previews["emojiQuery"] = json!("heart");
    c.previews["emojiMatches"] =
        json!([{"value":"♥","name":"Fixture heart","aliases":["heart"],"category":"Fixture"}]);
    c.preferences["usage"] = json!({});
    c.preferences["recent"] = json!([]);
    assert_eq!(
        c.indexed_content("apps", "heart", "all", "all")?,
        c.content("apps", "heart", "all", "all")?
    );
    Ok(())
}
