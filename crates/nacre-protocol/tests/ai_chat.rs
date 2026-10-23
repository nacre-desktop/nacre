use nacre_protocol::{
    ai_chat::{
        self, AiApprove, AiChannelRef, AiChat, AiSend, AiSendFailed, MAX_CHANNELS, MAX_MESSAGES,
        MAX_TEXT_JSON,
    },
    artifacts, framing, Message, Mirror, Update,
};
use serde_json::json;

#[test]
fn chat_transcript_replays_and_every_snapshot_validates() {
    let fixture = artifacts::generate()
        .remove("protocol/fixtures/ai-chat.jsonl")
        .unwrap();
    let mut mirror = Mirror::default();
    let mut frames = 0;
    for line in fixture.lines() {
        let message = framing::decode(line.as_bytes()).unwrap();
        frames += 1;
        let update = mirror.receive(&message);
        assert!(!matches!(update, Update::Resync(_)), "{line}");
        if let Message::Snap { topic, .. } | Message::Patch { topic, .. } = &message {
            let chat: AiChat = serde_json::from_value(mirror.topics[topic].1.clone()).unwrap();
            chat.validate().unwrap();
        }
        if let Message::Ev { data, .. } = &message {
            serde_json::from_value::<AiSendFailed>(data.clone())
                .unwrap()
                .validate()
                .unwrap();
        }
    }
    assert_eq!(frames, ai_chat::transcript().len());
    assert_eq!(mirror.topics[ai_chat::TOPIC].0, 7);
    assert!(mirror.topics[ai_chat::TOPIC].1["channels"]
        .as_object()
        .unwrap()
        .is_empty());
}

#[test]
fn methods_and_exports_are_published() {
    let methods = ai_chat::methods();
    for name in [
        "open",
        "send",
        "cancel",
        "approve",
        "new",
        "resume",
        "provider",
        "workspace",
        "saved",
    ] {
        let method = &methods[&format!("ai.chat.{name}")];
        assert!(method.timeout <= 45_000 && method.params.is_object());
    }
    let generated = artifacts::generate();
    for path in [
        "protocol/schema/ai-chat.json",
        "protocol/schema/ai-chat-send.json",
        "protocol/schema/ai-chat-send-failed.json",
        "protocol/schema/ai-chat-saved.json",
        "protocol/luau/ai-chat.d.luau",
        "protocol/fixtures/ai-chat.jsonl",
    ] {
        assert!(generated.contains_key(path), "missing {path}");
    }
    assert!(generated["protocol/luau/ai-chat.d.luau"].contains("export type AiChat ="));
}

#[test]
fn requests_are_bounded_and_channel_names_are_checked() {
    let send = |channel: &str, text: &str| AiSend {
        channel: channel.into(),
        text: text.into(),
        attachments: vec![],
    };
    send("", "hello").validate().unwrap();
    send("p-1", "hello").validate().unwrap();
    for bad in ["P1", "a/b", &"a".repeat(49), "x y"] {
        assert!(send(bad, "hello").validate().is_err(), "{bad}");
    }
    assert!(send("", "   ").validate().is_err());
    assert!(send("", &"x".repeat(64_001)).validate().is_err());
    send("", &"x".repeat(64_000)).validate().unwrap();
    let mut files = send("", "hello");
    files.attachments = vec![json!({"path":"/a"}); 8];
    files.validate().unwrap();
    files.attachments.push(json!({}));
    assert!(files.validate().is_err());
    files.attachments = vec![json!("not an object")];
    assert!(files.validate().is_err());
    assert!(AiChannelRef {
        channel: "Bad".into()
    }
    .validate()
    .is_err());
    let answer = |id: &str, answers| AiApprove {
        channel: "".into(),
        id: id.into(),
        answers,
    };
    answer("a", json!({})).validate().unwrap();
    assert!(answer("", json!({})).validate().is_err());
    assert!(answer("a", json!([])).validate().is_err());
    assert!(answer("a", json!({"x":"y".repeat(17_000)}))
        .validate()
        .is_err());
}

#[test]
fn snapshot_bounds_reject_oversized_channels_messages_and_questions() {
    let transcript = ai_chat::transcript();
    let mut chat: AiChat = serde_json::from_value(json!({
        "available":true,
        "channels":{"p1":{"state":transcript[2]["data"]["channels"]["p1"]["state"],"messages":{}}}
    }))
    .unwrap();
    chat.validate().unwrap();
    let channel = chat.channels["p1"].clone();
    for index in 0..=MAX_CHANNELS {
        chat.channels.insert(format!("c{index}"), channel.clone());
    }
    assert!(chat.validate().is_err());
    chat.channels.retain(|k, _| k == "p1");
    let message = |text: String| ai_chat::AiMessage {
        id: "m".into(),
        role: ai_chat::Role::Assistant,
        text,
        truncated: false,
    };
    let mut full = chat.channels["p1"].clone();
    for index in 0..=MAX_MESSAGES {
        full.messages
            .insert(format!("{index:010}"), message("x".into()));
    }
    assert!(full.validate().is_err());
    full.messages.clear();
    full.messages.insert("1".into(), message("x".into()));
    assert!(full.validate().is_err(), "ordinal keys are ten digits");
    full.messages.clear();
    full.messages
        .insert("0000000001".into(), message("x".repeat(MAX_TEXT_JSON)));
    assert!(full.validate().is_err(), "serialized quotes count");
    full.messages.clear();
    full.state.question = Some(json!({"q":"x".repeat(33 * 1024)}));
    assert!(full.validate().is_err());
}

#[test]
fn saved_chat_list_is_bounded_and_its_fixture_validates() {
    let saved = |n: usize| ai_chat::AiSavedList {
        chats: (0..n)
            .map(|i| ai_chat::AiSaved {
                key: format!("k{i}"),
                id: "t".into(),
                title: "title".into(),
                agent: ai_chat::Provider::Claude,
                account: "Default".into(),
                state: "saved".into(),
            })
            .collect(),
    };
    saved(ai_chat::MAX_SAVED).validate().unwrap();
    assert!(saved(ai_chat::MAX_SAVED + 1).validate().is_err());
    let mut list = saved(1);
    list.chats[0].key.clear();
    assert!(list.validate().is_err());
    let result = ai_chat::transcript()
        .into_iter()
        .find(|f| f["t"] == "ret" && f["id"] == 5)
        .unwrap();
    let fixture: ai_chat::AiSavedList = serde_json::from_value(result["ok"].clone()).unwrap();
    fixture.validate().unwrap();
}
