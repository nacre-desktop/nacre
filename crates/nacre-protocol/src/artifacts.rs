//! Deterministic exports. Schemas and Luau derive from the same Rust message types.
use crate::{Message, MIN_VERSION, VERSION};
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn luau(schema: &Value) -> String {
    if let Some(reference) = schema.get("$ref").and_then(Value::as_str) {
        return reference.rsplit('/').next().unwrap_or("any").into();
    }
    for keyword in ["oneOf", "anyOf"] {
        if let Some(items) = schema.get(keyword).and_then(Value::as_array) {
            let union = items
                .iter()
                .map(|s| format!("({})", luau(s)))
                .collect::<Vec<_>>()
                .join(" | ");
            if schema.get("properties").is_some() {
                let mut base = schema.clone();
                base.as_object_mut().expect("schema object").remove(keyword);
                return format!("({}) & ({union})", luau(&base));
            }
            return union;
        }
    }
    if let Some(items) = schema.get("allOf").and_then(Value::as_array) {
        return items
            .iter()
            .map(|s| format!("({})", luau(s)))
            .collect::<Vec<_>>()
            .join(" & ");
    }
    if let Some(value) = schema.get("const") {
        return value.to_string();
    }
    if let Some(values) = schema.get("enum").and_then(Value::as_array) {
        return values
            .iter()
            .map(Value::to_string)
            .collect::<Vec<_>>()
            .join(" | ");
    }
    if let Some(types) = schema.get("type").and_then(Value::as_array) {
        return types
            .iter()
            .map(|t| luau(&json!({"type":t})))
            .collect::<Vec<_>>()
            .join(" | ");
    }
    match schema.get("type").and_then(Value::as_str) {
        Some("null") => "nil".into(),
        Some("boolean") => "boolean".into(),
        Some("number" | "integer") => "number".into(),
        Some("string") => "string".into(),
        Some("array") => format!(
            "{{{}}}",
            schema
                .get("items")
                .map(luau)
                .unwrap_or_else(|| "any".into())
        ),
        Some("object") => {
            let required = schema.get("required").and_then(Value::as_array);
            let mut fields = Vec::new();
            if let Some(properties) = schema.get("properties").and_then(Value::as_object) {
                for (name, property) in properties {
                    let optional =
                        required.is_none_or(|keys| !keys.contains(&Value::String(name.clone())));
                    let value = luau(property);
                    fields.push(format!(
                        "[{}]: {}",
                        json!(name),
                        if optional {
                            format!("({value})?")
                        } else {
                            value
                        }
                    ));
                }
            }
            if let Some(additional) = schema.get("additionalProperties").filter(|v| v.is_object()) {
                fields.push(format!("[string]: {}", luau(additional)));
            }
            if fields.is_empty() {
                "{[string]: any}".into()
            } else {
                format!("{{ {} }}", fields.join(", "))
            }
        }
        _ => "any".into(),
    }
}
pub fn transcripts() -> BTreeMap<String, String> {
    let boot = "00000000-0000-4000-8000-000000000001";
    let welcome = json!({"t":"welcome","proto":VERSION,"min":MIN_VERSION,"boot":boot,"topics":{"compositor":1},"methods":crate::core_methods(),"frame_ms":16});
    let mut out = BTreeMap::new();
    let cases = vec![
        (
            "handshake",
            vec![
                json!({"t":"hello","proto":1,"min":1,"client":"fixture","futureOptional":true}),
                welcome.clone(),
            ],
        ),
        (
            "state",
            vec![
                welcome.clone(),
                json!({"t":"snap","topic":"compositor","seq":0,"data":{"windows":{},"order":[]}}),
                json!({"t":"patch","topic":"compositor","base":0,"seq":3,"data":{"windows":{"w1":{"id":"w1","focused":true}},"order":["w1"]}}),
                json!({"t":"patch","topic":"compositor","base":1,"seq":4,"data":{"windows":{"w1":{"focused":false}}}}),
                json!({"t":"resync","topics":["compositor"]}),
                json!({"t":"snap","topic":"compositor","seq":4,"data":{"windows":{"w1":{"id":"w1","focused":false}},"order":["w1"]}}),
            ],
        ),
        (
            "null-snapshot",
            vec![
                json!({"t":"snap","topic":"settings.desktop","seq":0,"data":{}}),
                json!({"t":"snap","topic":"settings.desktop","seq":1,"data":{"optional":null}}),
            ],
        ),
        (
            "calls",
            vec![
                json!({"t":"call","id":7,"m":"daemon.ping","p":{},"timeout":1000}),
                json!({"t":"ret","id":7,"ok":null}),
                json!({"t":"call","id":"missing","m":"example.missing","p":{}}),
                json!({"t":"ret","id":"missing","err":{"code":"unknown_method","message":"Unknown method","retry":false}}),
                json!({"t":"call","id":"deadline","m":"ui.dispatch","p":{},"timeout":1}),
                json!({"t":"ret","id":"deadline","err":{"code":"timeout","message":"Call deadline reached","retry":true,"dispatched":true}}),
            ],
        ),
        (
            "events",
            vec![
                json!({"t":"ev","topic":"notifications","name":"shown","seq":1,"data":{"id":"n1"}}),
                json!({"t":"lost","topic":"notifications","from":2,"to":5}),
                json!({"t":"ev","topic":"notifications","name":"shown","seq":6,"data":{"id":"n6"}}),
                json!({"t":"ping","id":"health"}),
                json!({"t":"pong","id":"health"}),
            ],
        ),
        (
            "restart",
            vec![
                welcome,
                json!({"t":"snap","topic":"compositor","seq":20,"data":{"windows":{},"order":[]}}),
                json!({"t":"welcome","proto":1,"min":1,"boot":"00000000-0000-4000-8000-000000000002","topics":{"compositor":1},"methods":{}}),
                json!({"t":"sub","topics":["compositor"]}),
                json!({"t":"snap","topic":"compositor","seq":0,"data":{"windows":{},"order":[]}}),
            ],
        ),
        (
            "ui",
            vec![
                json!({"t":"register","provider":"ui","methods":{"ui.dispatch":1}}),
                json!({"t":"call","id":1,"m":"ui.dispatch","p":{"target":"nacre","method":"launcher","args":[""]}}),
                json!({"t":"call","id":"provider-1","m":"ui.dispatch","p":{"target":"nacre","method":"launcher","args":[""]}}),
                json!({"t":"ret","id":"provider-1","ok":null}),
                json!({"t":"ret","id":1,"ok":null}),
            ],
        ),
    ];
    for (name, steps) in cases {
        let lines = steps
            .into_iter()
            .map(|step| {
                crate::framing::decode(&serde_json::to_vec(&step).expect("finite golden"))
                    .expect("valid golden");
                serde_json::to_string(&step).expect("finite golden") + "\n"
            })
            .collect();
        out.insert(format!("protocol/fixtures/{name}.jsonl"), lines);
    }
    out
}
pub fn generate() -> BTreeMap<String, String> {
    let schema = serde_json::to_value(schemars::schema_for!(Message)).expect("finite schema");
    let mut luau_types =
        String::from("-- Generated by nacre-protocol; do not edit. GPL-3.0-only.\n");
    if let Some(definitions) = schema.get("$defs").and_then(Value::as_object) {
        for (name, value) in definitions {
            luau_types.push_str(&format!("export type {name} = {}\n", luau(value)));
        }
    }
    luau_types.push_str(&format!("export type Message = {}\n", luau(&schema)));
    luau_types.push_str("return {}\n");
    let mut out = transcripts();
    out.insert(
        "protocol/schema/message.json".into(),
        serde_json::to_string_pretty(&schema).expect("finite schema") + "\n",
    );
    out.insert("protocol/luau/nacre.d.luau".into(), luau_types);
    out
}
