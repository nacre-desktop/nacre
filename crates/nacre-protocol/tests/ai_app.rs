use nacre_protocol::{
    ai_app::{self, Accounts, Chats, Projects, Providers, Started, TOPICS},
    artifacts, framing, Message, Mirror, Reply, Update,
};
use serde_json::Value;

#[test]
fn transcript_replays_and_every_snapshot_and_reply_validates() {
    let fixture = artifacts::generate()
        .remove("protocol/fixtures/ai-app.jsonl")
        .unwrap();
    let mut mirror = Mirror::default();
    let mut frames = 0;
    for line in fixture.lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        frames += 1;
        assert!(
            !matches!(mirror.receive(&message), Update::Resync(_)),
            "{line}"
        );
        match &message {
            Message::Snap { topic, .. } | Message::Patch { topic, .. } => {
                let data = mirror.topics[topic].1.clone();
                match topic.as_str() {
                    ai_app::PROVIDERS_TOPIC => serde_json::from_value::<Providers>(data)
                        .unwrap()
                        .validate()
                        .unwrap(),
                    ai_app::PROJECTS_TOPIC => serde_json::from_value::<Projects>(data)
                        .unwrap()
                        .validate()
                        .unwrap(),
                    ai_app::ACCOUNTS_TOPIC => serde_json::from_value::<Accounts>(data)
                        .unwrap()
                        .validate()
                        .unwrap(),
                    other => panic!("unexpected topic {other}"),
                }
            }
            Message::Ret {
                reply: Reply::Ok { ok },
                ..
            } if ok.get("chats").is_some() => {
                serde_json::from_value::<Chats>(ok.clone())
                    .unwrap()
                    .validate()
                    .unwrap();
            }
            Message::Ret {
                reply: Reply::Ok { ok },
                ..
            } if ok.get("started").is_some() => {
                let started: Started = serde_json::from_value(ok.clone()).unwrap();
                assert!(started.id.as_deref().is_none_or(ai_app::valid_chat_id));
            }
            _ => {}
        }
    }
    assert_eq!(frames, ai_app::transcript().len());
    assert_eq!(mirror.topics.len(), TOPICS.len());
}

#[test]
fn methods_schemas_and_luau_types_are_exported() {
    let methods = ai_app::methods();
    for (name, ..) in ai_app::METHODS {
        assert!(ai_app::handles(name));
        assert!(methods[name].timeout <= 60_000 && methods[name].params.is_object());
    }
    assert!(
        !ai_app::handles("ai.chats.stop"),
        "stop is deliberately absent"
    );
    let generated = artifacts::generate();
    for name in ai_app::schemas().keys() {
        assert!(generated.contains_key(&format!("protocol/schema/{name}.json")));
    }
    let luau = &generated["protocol/luau/ai-app.d.luau"];
    for type_name in ["AiProviders", "AiChats", "AiChatStart", "BrainNote"] {
        assert!(luau.contains(&format!("export type {type_name} =")));
    }
}

/// Everything in these schemas is readable by the shell: no property may be
/// named like a credential, and nothing is nested under a free-form object.
#[test]
fn no_schema_property_is_named_like_a_credential() {
    fn walk(value: &Value, path: &str, bad: &mut Vec<String>) {
        if let Some(properties) = value.get("properties").and_then(Value::as_object) {
            for (name, schema) in properties {
                let lower = name.to_ascii_lowercase();
                if [
                    "token",
                    "secret",
                    "password",
                    "credential",
                    "key",
                    "auth",
                    "cookie",
                ]
                .iter()
                .any(|needle| lower.contains(needle))
                {
                    bad.push(format!("{path}.{name}"));
                }
                walk(schema, &format!("{path}.{name}"), bad);
            }
        }
        for key in ["$defs", "items", "additionalProperties"] {
            if let Some(nested) = value.get(key) {
                if let Some(map) = nested.as_object().filter(|_| key == "$defs") {
                    for (name, schema) in map {
                        walk(schema, &format!("{path}#{name}"), bad);
                    }
                } else {
                    walk(nested, path, bad);
                }
            }
        }
    }
    let mut bad = Vec::new();
    for (name, schema) in ai_app::schemas() {
        walk(&schema, &name, &mut bad);
    }
    let usage = artifacts::generate()["protocol/schema/dashboard-extras-state.json"].clone();
    walk(
        &serde_json::from_str(&usage).unwrap(),
        "dashboard-extras-state",
        &mut bad,
    );
    assert!(bad.is_empty(), "credential-like fields: {bad:?}");
    // The walker finds a planted field, so an empty list means something.
    let mut planted = Vec::new();
    walk(
        &serde_json::json!({"properties":{"apiToken":{"type":"string"}}}),
        "planted",
        &mut planted,
    );
    assert_eq!(planted, ["planted.apiToken"]);
}
