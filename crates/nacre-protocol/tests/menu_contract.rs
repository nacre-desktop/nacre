use nacre_protocol::{
    artifacts, framing,
    menu::{MenuActivate, MenuActivated, MenuQuery, MenuQueryResult},
    Message, Reply, MAX_SAFE_INTEGER,
};
use serde_json::{json, Value};

#[test]
fn launcher_goldens_use_typed_bounded_methods_and_stable_maps() {
    let traces = artifacts::menu_transcripts();
    assert_eq!(traces.len(), 6);
    let mut populated = false;
    for trace in traces.values() {
        for line in trace.lines() {
            match framing::decode(line.as_bytes()).unwrap() {
                Message::Call { m, p, .. } if m == "menu.query" => {
                    serde_json::from_value::<MenuQuery>(p)
                        .unwrap()
                        .validate()
                        .unwrap()
                }
                Message::Call { m, p, .. } if m == "menu.activate" => {
                    serde_json::from_value::<MenuActivate>(p)
                        .unwrap()
                        .validate()
                        .unwrap()
                }
                Message::Ret {
                    reply: Reply::Ok { ok },
                    ..
                } if ok.get("items").is_some() => {
                    let result: MenuQueryResult = serde_json::from_value(ok.clone()).unwrap();
                    result.validate().unwrap();
                    assert!(ok["items"].is_object());
                    assert!(ok["order"].is_array());
                    assert_eq!(serde_json::to_value(&result).unwrap(), ok);
                    if result.items.len() == 320 {
                        populated = true;
                        assert_eq!(result.order.first().unwrap(), "app-0000");
                        assert_eq!(result.order.last().unwrap(), "app-0319");
                    }
                }
                Message::Ret {
                    reply: Reply::Ok { ok },
                    ..
                } => serde_json::from_value::<MenuActivated>(ok)
                    .unwrap()
                    .validate()
                    .unwrap(),
                Message::Ret {
                    reply: Reply::Err { .. },
                    ..
                } => {}
                _ => panic!("unexpected launcher golden message"),
            }
        }
    }
    assert!(
        populated,
        "typing fixture must contain at least300 real result records"
    );
}

fn populated() -> MenuQueryResult {
    let trace = artifacts::menu_transcripts()
        .remove("protocol/fixtures/menu-apps.jsonl")
        .unwrap();
    let frame: Value = serde_json::from_str(trace.lines().nth(1).unwrap()).unwrap();
    serde_json::from_value(frame["ok"].clone()).unwrap()
}
#[test]
fn result_order_is_authoritative_and_invalid_members_do_not_validate() {
    let mut result = populated();
    result.order.reverse();
    result.validate().unwrap();
    let round: MenuQueryResult =
        serde_json::from_value(serde_json::to_value(&result).unwrap()).unwrap();
    assert_eq!(round.order, result.order);
    result.order[0] = result.order[1].clone();
    assert!(result.validate().is_err());
    let mut result = populated();
    result.items.remove("app-0000");
    assert!(result.validate().is_err());
    let mut result = populated();
    result.catalog_rev = MAX_SAFE_INTEGER + 1;
    assert!(result.validate().is_err());
    let mut result = populated();
    result.total = Some(1);
    assert!(result.validate().is_err());
}
#[test]
fn method_bounds_and_future_optional_fields_keep_v1_compatible() {
    let query: MenuQuery =
        serde_json::from_value(json!({"query":"# fixture","futureOptional":true})).unwrap();
    query.validate().unwrap();
    assert_eq!(query.mode, "apps");
    let mut query = query;
    query.limit = Some(0);
    assert!(query.validate().is_err());
    query.limit = Some(320);
    query.query = "x".repeat(65537);
    assert!(query.validate().is_err());
    let mut result = serde_json::to_value(populated()).unwrap();
    result["futureOptional"] = json!({"safe":true});
    result["items"]["app-0000"]["futureOptional"] = json!("ignored");
    result["items"]["app-0000"]["status"] = json!("active");
    result["items"]["app-0000"]["checked"] = json!(true);
    result["items"]["app-0000"]["reason"] = json!("Fixture action is active");
    let mut decoded = serde_json::from_value::<MenuQueryResult>(result).unwrap();
    decoded.validate().unwrap();
    let item = decoded.items.get_mut("app-0000").unwrap();
    assert_eq!(item.status.as_deref(), Some("active"));
    assert_eq!(item.checked, Some(true));
    assert_eq!(item.reason.as_deref(), Some("Fixture action is active"));
    item.status = Some("x".repeat(129));
    assert!(decoded.validate().is_err());
    let item = decoded.items.get_mut("app-0000").unwrap();
    item.status = None;
    item.reason = Some("x".repeat(4097));
    assert!(decoded.validate().is_err());
    let activate: MenuActivate = serde_json::from_value(
        json!({"id":"app-0000","activation":"fixture-app","catalog_rev":1,"futureOptional":null}),
    )
    .unwrap();
    activate.validate().unwrap();
    let mut activate = activate;
    activate.activation.clear();
    assert!(activate.validate().is_err());
    assert!(serde_json::from_value::<MenuQueryResult>(
        json!({"query":"","mode":"apps","route":"home","catalog_rev":1,"items":[],"order":[]})
    )
    .is_err());
}

#[test]
fn menu_groups_golden_decodes_into_the_published_types() {
    let files = artifacts::generate();
    for line in files["protocol/fixtures/menu-groups.jsonl"].lines() {
        match framing::decode(line.as_bytes()).unwrap() {
            Message::Call { m, p, .. } => {
                assert_eq!(m, "menu.groups");
                serde_json::from_value::<nacre_protocol::menu::MenuGroups>(p).unwrap();
            }
            Message::Ret {
                reply: Reply::Ok { ok },
                ..
            } => {
                let result: nacre_protocol::menu::MenuGroupsResult =
                    serde_json::from_value(ok).unwrap();
                result.validate().unwrap();
                assert_eq!(result.groups.len(), 2);
            }
            _ => panic!("unexpected menu groups golden message"),
        }
    }
}
