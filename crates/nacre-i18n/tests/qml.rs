use serde_json::Value;
use sha2::{Digest, Sha256};

#[test]
fn frozen_current_qml_translation_and_pseudo_outputs_are_exact() {
    let bytes = include_bytes!("fixtures/qml-reference.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(bytes)),
        "224ea17d96c13cff1e5b6470de852d6991489f847046cd2680516eae44a1e1aa"
    );
    let reference: Value = serde_json::from_slice(bytes).unwrap();
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 16);
    for (index, case) in cases.iter().enumerate() {
        let language = case["language"].as_str().unwrap();
        let value = if case["op"] == "localize" {
            nacre_i18n::localize(&case["value"], &case["messages"], language)
        } else {
            Value::String(nacre_i18n::translate(
                &case["messages"],
                language,
                case["id"].as_str().unwrap(),
                case["fallback"].as_str().unwrap(),
                &case["args"],
            ))
        };
        assert_eq!(
            value, case["expected"],
            "frozen Qt translation case {index}"
        );
    }
}
