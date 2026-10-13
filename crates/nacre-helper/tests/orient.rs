//! Read the existing frozen Python references; never regenerate palettes here.
use nacre_helper::orient;
use serde_json::{json, Value};
use std::{fs::File, io::Read, path::Path};

fn dereference<'a>(mut value: &'a Value, pool: &'a Value) -> &'a Value {
    while let Some(index) = value.get("$ref") {
        value = &pool[index.as_u64().unwrap() as usize];
    }
    value
}
fn expanded(value: &Value, pool: &Value) -> Value {
    let value = dereference(value, pool);
    match value {
        Value::Object(items) => Value::Object(
            items
                .iter()
                .map(|(key, v)| (key.clone(), expanded(v, pool)))
                .collect(),
        ),
        Value::Array(items) => Value::Array(items.iter().map(|v| expanded(v, pool)).collect()),
        _ => value.clone(),
    }
}
fn scrub(value: &mut Value) {
    match value {
        Value::Object(items) => {
            items.remove("engine");
            items.remove("path");
            for item in items.values_mut() {
                scrub(item);
            }
        }
        Value::Array(items) => {
            for item in items {
                scrub(item);
            }
        }
        _ => {}
    }
}
#[test]
fn direct_libraries_match_frozen_wallpaper_palettes() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../deps/orient");
    let gallery = root.join("tests/orient-gallery/images");
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(
        File::open(root.join("tests/parity/golden/outputs.json.gz")).unwrap(),
    )
    .read_to_end(&mut bytes)
    .unwrap();
    let reference: Value = serde_json::from_slice(&bytes).unwrap();
    let pool = &reference["pool"];
    let cases = dereference(&dereference(&reference["root"], pool)["images"], pool)
        .as_array()
        .unwrap();
    let mut matched = 0;
    for case in cases {
        let case = dereference(case, pool);
        let name = case["name"].as_str().unwrap();
        let path = gallery.join(format!(
            "{name}.{}",
            if name == "moving-lantern" {
                "gif"
            } else {
                "png"
            }
        ));
        if !path.is_file() {
            continue;
        }
        for record in dereference(&case["palettes"], pool).as_array().unwrap() {
            let record = expanded(record, pool);
            let result = orient::from_image(&path, &record["options"]);
            let mut actual = match result {
                Ok(value) => json!({"value":value}),
                Err(error) => {
                    let kind = match error.downcast_ref::<orient_core::Error>() {
                        Some(orient_core::Error::Key(_)) => "KeyError",
                        _ => "ValueError",
                    };
                    json!({"error":kind,"message":error.to_string()})
                }
            };
            scrub(&mut actual);
            assert!(
                actual == record["expected"],
                "Direct library mismatch: {name}, {}",
                record["options"]
            );
            matched += 1;
        }
    }
    assert_eq!(matched, 210);
}
