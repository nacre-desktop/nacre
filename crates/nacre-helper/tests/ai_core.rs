use nacre_helper::ai_core::{number, resolve_context, usage_snapshot};
use serde_json::json;

#[test]
fn context_matches_aliases_as_case_insensitive_whole_words_and_reports_ambiguity() {
    let registry = json!({"projects":[
        {"id":"nacre","label":"Nacre Desktop","path":"/work/nacre","build_host":"fixture-host","build_path":"/build","aliases":["shell"],"brain":{"aliases":["pearl"]}},
        {"id":"bead","aliases":["shell"]}
    ]});
    let result = resolve_context("Please inspect NACRE and pearl", &registry);
    assert_eq!(result["matched"].as_array().unwrap().len(), 1);
    assert_eq!(result["matched"][0]["matches"], json!(["nacre", "pearl"]));
    assert_eq!(result["ambiguous"], false);
    assert_eq!(result["matched"][0]["build_host"], "fixture-host");
    assert_eq!(result["matched"][0]["build_path"], "/build");
    assert!(resolve_context("nacrex", &registry)["matched"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(resolve_context("shell", &registry)["ambiguous"], true);
}

#[test]
fn usage_snapshot_filters_unknown_and_non_finite_values() {
    let result = usage_snapshot(&json!({
        "rate_limits":{"five_hour":{"used_percentage":25.5,"resets_at":123,"secret":"fixture"},"seven_day":{"used_percentage":null}},
        "cost":{"total_cost_usd":0.125,"total_duration_ms":true,"other":8}
    }));
    assert_eq!(
        result["five_hour"],
        json!({"used_percentage":25.5,"resets_at":123})
    );
    assert_eq!(result["seven_day"], json!({}));
    assert_eq!(result["session"], json!({"total_cost_usd":0.125}));
    assert!(result["observed_at"].as_str().unwrap().ends_with("+00:00"));
}

#[test]
fn numeric_filter_rejects_booleans_and_non_numbers() {
    assert!(number(&json!(12.5)));
    assert!(!number(&json!(true)));
    assert!(!number(&json!("12")));
}

#[test]
fn memory_slug_and_normalization_keep_historical_limits() {
    assert_eq!(
        nacre_helper::ai_core::slug("A Public World!"),
        "a-public-world"
    );
    assert_eq!(
        nacre_helper::ai_core::one("  several\n words  "),
        "several words"
    );
    assert_eq!(nacre_helper::ai_core::slug(&"x".repeat(100)).len(), 70);
    assert_eq!(nacre_helper::ai_core::slug("Straße"), "strasse");
}

#[test]
fn memory_vault_writer_rejects_secret_like_content_before_writing() {
    let result = nacre_helper::brain_data::write_note(
        "inbox",
        "fixture",
        "test fixture",
        "reported",
        "sk-proj-abcdefghijklmnopqrstuvwxyz123456",
    );
    assert!(result.is_err());
}

#[test]
fn environment_delta_never_serializes_inherited_values() {
    let delta = nacre_helper::ai_core::environment_delta("codex", None).unwrap();
    assert_eq!(delta, json!({"set":{},"unset":[]}));
}
