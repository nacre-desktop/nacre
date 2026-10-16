use nacre_protocol::{
    artifacts, framing,
    menu::{MenuActivate, MenuActivated, MenuQuery, MenuQueryResult, MenuUiActivate, UI_ACTIVATE},
    ErrorCode, Message, Reply, MAX_SAFE_INTEGER,
};
use serde_json::{json, Value};

#[test]
fn provider_goldens_keep_copy_confirmation_and_literal_arguments_distinct() {
    let traces = artifacts::menu_action_transcripts();
    let trace = &traces["protocol/fixtures/menu-ui-actions.jsonl"];
    let mut operations = Vec::new();
    let mut direct_refused = false;
    for line in trace.lines() {
        match framing::decode(line.as_bytes()).unwrap() {
            Message::Register { methods, .. } => assert_eq!(methods.get(UI_ACTIVATE), Some(&1)),
            Message::Call { m, p, .. } if m == "menu.query" => {
                serde_json::from_value::<MenuQuery>(p)
                    .unwrap()
                    .validate()
                    .unwrap();
            }
            Message::Call { m, p, .. } if m == "menu.activate" => {
                let request = serde_json::from_value::<MenuActivate>(p).unwrap();
                request.validate().unwrap();
                let canonical = serde_json::to_value(request).unwrap();
                assert!(canonical.get("confirmed").is_none());
                assert!(canonical.get("row").is_none());
            }
            Message::Call { m, p, .. } if m == UI_ACTIVATE => {
                let selected: MenuUiActivate = serde_json::from_value(p.clone()).unwrap();
                selected.validate().unwrap();
                assert_eq!(serde_json::to_value(&selected).unwrap(), p);
                if selected.row.operation == "power" {
                    assert_eq!(
                        selected.row.confirm.as_deref(),
                        Some("Restart the computer?")
                    );
                }
                if selected.row.operation == "command" {
                    assert_eq!(selected.row.arguments.as_ref().unwrap()[2], "$(literal)");
                }
                operations.push(selected.row.operation);
            }
            Message::Ret {
                reply: Reply::Ok { ok },
                ..
            } if ok.get("items").is_some() => {
                let result: MenuQueryResult = serde_json::from_value(ok).unwrap();
                result.validate().unwrap();
                assert_eq!(result.order, ["nacre-menu:calculation"]);
                assert!(result.items[&result.order[0]].activation.is_some());
            }
            Message::Ret {
                reply: Reply::Ok { ok },
                ..
            } => {
                serde_json::from_value::<MenuActivated>(ok)
                    .unwrap()
                    .validate()
                    .unwrap();
            }
            Message::Ret {
                reply: Reply::Err { err },
                ..
            } => {
                if err.code == ErrorCode::PermissionDenied {
                    assert!(!err.retry);
                    assert_eq!(err.dispatched, Some(false));
                    direct_refused = true;
                } else {
                    assert_eq!(err.code, ErrorCode::Unavailable);
                    assert!(!err.retry);
                }
            }
            _ => panic!("Unexpected action transcript frame"),
        }
    }
    assert!(direct_refused);
    assert_eq!(
        operations,
        [
            "copy",
            "command",
            "power",
            "copy-clip",
            "command",
            "future.fixture"
        ]
    );
}

fn selected() -> Value {
    json!({"catalog_rev":7,"row":{"id":"calculation","kind":"calculation","name":"2","operation":"copy","value":"2","disabled":false}})
}

#[test]
fn selected_action_bounds_refuse_bad_operands_and_argv_without_rewriting_them() {
    let valid = selected();
    let check = |value| {
        serde_json::from_value::<MenuUiActivate>(value)
            .is_ok_and(|selected| selected.validate().is_ok())
    };
    assert!(check(valid.clone()));
    for operand in [
        json!(null),
        json!(7),
        json!(true),
        json!({"command":"fixture"}),
        json!(["fixture"]),
    ] {
        let mut value = valid.clone();
        value["row"]["value"] = operand;
        assert!(!check(value));
    }
    for (field, invalid) in [
        ("id", json!("")),
        ("kind", json!("x".repeat(129))),
        ("operation", json!("")),
        ("value", json!("x".repeat(65537))),
        ("value", json!("fixture\0secret")),
        ("confirm", json!("x".repeat(4097))),
        ("arguments", json!(["fixture", ""])),
        ("arguments", json!(vec!["fixture"; 33])),
        ("terminalCommand", json!(vec!["fixture"; 65])),
    ] {
        let mut value = valid.clone();
        value["row"][field] = invalid;
        assert!(!check(value), "{field}");
    }
    let mut value = valid.clone();
    value["catalog_rev"] = json!(MAX_SAFE_INTEGER + 1);
    assert!(!check(value));
    let mut value = valid.clone();
    value["row"]["operation"] = json!("command");
    assert!(!check(value.clone()));
    value["row"]["arguments"] = json!(["fixture-app", "space & literal", "$(literal)"]);
    assert!(check(value));
    let mut value = valid;
    value["row"]["value"] = json!("literal\n$(fixture)\t😀");
    value["futureOptional"] = json!(true);
    value["row"]["confirmed"] = json!(true);
    let parsed: MenuUiActivate = serde_json::from_value(value.clone()).unwrap();
    parsed.validate().unwrap();
    let encoded = serde_json::to_value(parsed).unwrap();
    assert_eq!(encoded["row"]["value"], value["row"]["value"]);
    assert!(encoded.get("futureOptional").is_none());
    assert!(encoded["row"].get("confirmed").is_none());
}

#[test]
fn numeric_window_operands_stay_safe_and_future_operations_are_data_only() {
    let mut value = selected();
    value["row"]["operation"] = json!("focus");
    for operand in [-(MAX_SAFE_INTEGER as i64), 0, MAX_SAFE_INTEGER as i64] {
        value["row"]["value"] = json!(operand);
        serde_json::from_value::<MenuUiActivate>(value.clone())
            .unwrap()
            .validate()
            .unwrap();
    }
    value["row"]["value"] = json!(MAX_SAFE_INTEGER + 1);
    assert!(serde_json::from_value::<MenuUiActivate>(value.clone())
        .unwrap()
        .validate()
        .is_err());
    value["row"]["value"] = json!(1.5);
    assert!(serde_json::from_value::<MenuUiActivate>(value.clone()).is_err());
    value["row"]["operation"] = json!("future.fixture");
    value["row"].as_object_mut().unwrap().remove("value");
    serde_json::from_value::<MenuUiActivate>(value)
        .unwrap()
        .validate()
        .unwrap();
}
