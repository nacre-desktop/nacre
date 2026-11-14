// SPDX-License-Identifier: GPL-3.0-only
//! B0043: the sidebar model against fixtures and a fake daemon, plus goldens.
// One test at a time owns the process-wide `send_failed` queue, across awaits.
#![allow(clippy::await_holding_lock)]
use super::*;
use crate::nacre_source::Config;
use bead_test::{Harness, Input, source::FakeSource};
use nacre_protocol::{Capability, ErrorCode, Failure, Message, Reply, framing};
use std::path::Path;
use tokio::{io::BufReader, net::UnixListener, sync::mpsc};

/// `route_event` feeds one process-wide queue, so tests that project run one by one.
pub(crate) static SERIAL: Mutex<()> = Mutex::new(());
const FRAME_WIDTH: u32 = 534;
const FRAME_HEIGHT: u32 = 714;
pub(crate) fn serial() -> std::sync::MutexGuard<'static, ()> {
    SERIAL.lock().unwrap_or_else(|p| p.into_inner())
}
#[test]
fn running_indicator_ticker_starts_stops_and_uses_fast_period_only_while_working() {
    let _guard = serial();
    let (_, chat) = offline(json!({"available":true,"channels":{}}));
    *chat.notify.borrow_mut() = Some(Arc::new(|_| {}));
    chat.st.borrow_mut().launcher = Some(json!({"chats":[{"state":"running"}]}));
    chat.project(&Stores::default());
    assert_eq!(chat.ticker.borrow().as_ref().unwrap().period, PULSE_TICK);
    chat.st.borrow_mut().launcher = Some(json!({"chats":[{"state":"saved"}]}));
    let idle = chat.project(&Stores::default());
    assert_eq!(chat.ticker.borrow().as_ref().unwrap().period, HEARTBEAT);
    assert_eq!(idle["launcher.pulse"], number(0.));
    chat.project(&stores(&[("panel.open", Value::Bool(false))]));
    assert!(chat.ticker.borrow().is_none());
}
pub(crate) fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
pub(crate) fn meter() -> Meter {
    let font =
        Path::new(env!("BEAD_FRAMEWORK_ROOT")).join("crates/bead-test/fonts/IBMPlexSans.ttf");
    // A missing font silently falls back to another face and shifts every golden.
    assert!(font.is_file(), "test font missing: {}", font.display());
    Meter::fixed(vec![font])
}
pub(crate) fn state(busy: bool, status: &str, question: Option<Json>) -> Json {
    let mut s = json!({"defaultProvider":"claude","inWorkspace":false,"provider":"claude","account":"Default","title":"Fix the build","threadId":"t1","model":"","features":[],"composerKey":"k","busy":busy,"status":status,"error":""});
    if let Some(q) = question {
        s["question"] = q;
    }
    s
}
pub(crate) fn topic(messages: &[(&str, &str, &str)], st: Json) -> Json {
    let msgs: serde_json::Map<String, Json> = messages
        .iter()
        .enumerate()
        .map(|(i, (id, role, text))| {
            (
                format!("{:010}", i + 1),
                json!({"id": id, "role": role, "text": text}),
            )
        })
        .collect();
    json!({"available": true, "channels": {"": {"state": st, "messages": msgs}}})
}
fn approval() -> Json {
    json!({"id":"approval-1","kind":"approval","questions":[{"id":"approval","question":"Run the build command?","options":[{"label":"Allow once"},{"label":"Decline"}]}]})
}
fn stores(pairs: &[(&str, Value)]) -> Stores {
    let mut values: BTreeMap<String, Value> = pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.clone()))
        .collect();
    values
        .entry("chat.section".into())
        .or_insert(Value::Text("chat".into()));
    Stores::default().reconcile(&values).unwrap()
}
fn launcher_stores(pairs: &[(&str, Value)]) -> Stores {
    let mut values: BTreeMap<String, Value> = pairs
        .iter()
        .map(|(key, value)| (key.to_string(), value.clone()))
        .collect();
    values.insert("chat.section".into(), Value::Text("launcher".into()));
    Stores::default().reconcile(&values).unwrap()
}
pub(crate) fn offline(data: Json) -> (Arc<FakeSource>, Chat) {
    let source = Arc::new(FakeSource::new(data).unwrap());
    (source.clone(), Chat::new(None, source, meter))
}
fn cells(out: &BTreeMap<String, Value>) -> Vec<Json> {
    match &out["chat.cells"] {
        Value::List(items) => items.as_ref().clone(),
        other => panic!("{other:?}"),
    }
}
fn num(out: &BTreeMap<String, Value>, key: &str) -> f64 {
    match &out[key] {
        Value::Number(n) => *n,
        other => panic!("{key}: {other:?}"),
    }
}
fn string(out: &BTreeMap<String, Value>, key: &str) -> String {
    match &out[key] {
        Value::Text(t) => t.clone(),
        other => panic!("{key}: {other:?}"),
    }
}

#[test]
fn streaming_text_keeps_earlier_cells_and_follows_the_bottom() {
    let _guard = serial();
    let long = "word ".repeat(400);
    let (source, chat) = offline(topic(
        &[("u1", "user", "hello")],
        state(true, "Working", None),
    ));
    let first = chat.project(&stores(&[]));
    assert!(num(&first, "chat.extent") > 0.);
    let mut previous = cells(&first);
    let mut text = String::new();
    for step in 0..4 {
        text.push_str(&long);
        source
            .publish(topic(
                &[("u1", "user", "hello"), ("a1", "assistant", &text)],
                state(true, "Working", None),
            ))
            .unwrap();
        let st = stores(&[("chat.scroll", Value::Number(num(&first, "chat.scroll")))]);
        let out = chat.project(&st);
        let now = cells(&out);
        // Everything laid out before the streaming message is unchanged between tokens.
        for cell in now
            .iter()
            .filter(|c| !c["id"].as_str().unwrap().starts_with("0000000002:"))
        {
            if let Some(old) = previous.iter().find(|c| c["id"] == cell["id"]) {
                assert_eq!(old, cell);
            }
        }
        assert!(
            now.iter()
                .any(|c| c["id"].as_str().unwrap().starts_with("0000000002:")),
            "step {step}"
        );
        previous = now;
    }
}

#[test]
fn reading_away_stops_following_until_the_bottom_is_reached_again() {
    let _guard = serial();
    let msgs: Vec<(String, String)> = (0..500)
        .map(|i| {
            let code = if i % 10 == 0 {
                format!("\n\n```rust\nfn message_{i}() {{}}\n```\n")
            } else {
                String::new()
            };
            (
                format!("m{i}"),
                format!("message {i} {}{code}", "x ".repeat(60)),
            )
        })
        .collect();
    let rows: Vec<(&str, &str, &str)> = msgs
        .iter()
        .map(|(a, b)| (a.as_str(), "assistant", b.as_str()))
        .collect();
    let data = topic(&rows, state(false, "", None));
    let channel = serde_json::from_value(data["channels"][""].clone()).unwrap();
    let (source, chat) = offline(data);
    chat.set_test_channel(channel);
    let out = chat.project(&stores(&[]));
    let bottom = num(&out, "chat.scroll");
    assert!(bottom > 500.);
    // The reader scrolls to the top: the next update keeps that position.
    let out = chat.project(&stores(&[("chat.scroll", Value::Number(0.))]));
    assert_eq!(num(&out, "chat.scroll"), 0.);
    source
        .publish(topic(&rows, state(false, "still", None)))
        .unwrap();
    let out = chat.project(&stores(&[("chat.scroll", Value::Number(0.))]));
    assert_eq!(num(&out, "chat.scroll"), 0.);
    // The synthetic fixture exceeds the published snapshot bound, so the test helper
    // installs it directly; production sources still go through AiChannel::validate.
    let visible = cells(&out);
    assert!(visible.len() < 100, "{} cells materialized", visible.len());
    assert!(visible.iter().all(|c| c["y"].as_f64().unwrap() < 1500.));
    // At the bottom: following resumes.
    let current_bottom = f64::from(
        (num(&out, "chat.extent") as f32 - num(&out, "chat.transcript_height") as f32).max(0.),
    );
    let out = chat.project(&stores(&[("chat.scroll", Value::Number(current_bottom))]));
    assert!(
        (num(&out, "chat.scroll")
            - (num(&out, "chat.extent") - num(&out, "chat.transcript_height")))
        .abs()
            < 0.01
    );
    let away_from_bottom = current_bottom - 10.;
    let out = chat.project(&stores(&[("chat.scroll", Value::Number(away_from_bottom))]));
    assert_eq!(num(&out, "chat.scroll"), away_from_bottom);
}

#[test]
fn small_reader_scroll_stops_following_until_actual_bottom() {
    let _guard = serial();
    let mut msgs: Vec<(String, String)> = (0..30)
        .map(|i| (format!("m{i}"), format!("message {i} {}", "x ".repeat(60))))
        .collect();
    let rows: Vec<(&str, &str, &str)> = msgs
        .iter()
        .map(|(id, text)| (id.as_str(), "assistant", text.as_str()))
        .collect();
    let (source, chat) = offline(topic(&rows, state(true, "Working", None)));
    let initial = chat.project(&stores(&[]));
    let bottom = num(&initial, "chat.scroll");
    assert!(bottom > 500.);

    let first_reader_offset = bottom - 7.;
    let first = chat.project(&stores(&[(
        "chat.scroll",
        Value::Number(first_reader_offset),
    )]));
    assert_eq!(num(&first, "chat.scroll"), first_reader_offset);

    let fractional_offset = first_reader_offset - 0.25;
    let fractional = chat.project(&stores(&[(
        "chat.scroll",
        Value::Number(fractional_offset),
    )]));
    assert_eq!(num(&fractional, "chat.scroll"), fractional_offset);

    msgs.push((
        "m30".into(),
        format!("new streamed message {}", "x ".repeat(60)),
    ));
    let rows: Vec<(&str, &str, &str)> = msgs
        .iter()
        .map(|(id, text)| (id.as_str(), "assistant", text.as_str()))
        .collect();
    source
        .publish(topic(&rows, state(true, "Working", None)))
        .unwrap();
    let after_append = chat.project(&stores(&[(
        "chat.scroll",
        Value::Number(fractional_offset),
    )]));
    assert_eq!(num(&after_append, "chat.scroll"), fractional_offset);

    let new_bottom = f64::from(
        (num(&after_append, "chat.extent") as f32
            - num(&after_append, "chat.transcript_height") as f32)
            .max(0.),
    );
    let at_bottom = chat.project(&stores(&[("chat.scroll", Value::Number(new_bottom))]));
    assert_eq!(num(&at_bottom, "chat.scroll"), new_bottom);

    msgs.push(("m31".into(), format!("followed update {}", "x ".repeat(60))));
    let rows: Vec<(&str, &str, &str)> = msgs
        .iter()
        .map(|(id, text)| (id.as_str(), "assistant", text.as_str()))
        .collect();
    source
        .publish(topic(&rows, state(true, "Working", None)))
        .unwrap();
    let after_followed_append =
        chat.project(&stores(&[("chat.scroll", Value::Number(new_bottom))]));
    assert!(num(&after_followed_append, "chat.scroll") > new_bottom);
}

#[test]
fn assistant_markdown_blocks_and_copy_actions() {
    let _guard = serial();
    let md = "Intro **bold**.\n\n- one\n- two\n\n```sh\ncargo build\n```\n\n```\nls\n```";
    let (_s, chat) = offline(topic(
        &[("u", "user", "**not** markdown"), ("a", "assistant", md)],
        state(false, "", None),
    ));
    let out = chat.project(&stores(&[]));
    let all = cells(&out);
    let texts: Vec<String> = all
        .iter()
        .map(|c| c["t"].as_str().unwrap().to_owned())
        .collect();
    assert!(
        texts.contains(&"**not** markdown".to_string()),
        "user text stays plain"
    );
    assert!(texts.contains(&"Intro bold.".to_string()));
    assert!(texts.contains(&"•".to_string()) && texts.contains(&"cargo build".to_string()));
    let buttons: Vec<_> = all
        .iter()
        .filter(|c| c["k"] == "button")
        .map(|c| c["a"].as_str().unwrap().to_owned())
        .collect();
    let mut buttons = buttons;
    buttons.sort();
    assert_eq!(
        buttons,
        [
            "code:0000000002:0",
            "code:0000000002:1",
            "copy:0000000001",
            "copy:0000000002"
        ]
    );
    assert_eq!(
        chat.act("copy:0000000002", &stores(&[])).copy.as_deref(),
        Some(md)
    );
    assert_eq!(
        chat.act("code:0000000002:1", &stores(&[])).copy.as_deref(),
        Some("ls")
    );
    assert_eq!(chat.act("code:0000000002:9", &stores(&[])).copy, None);
    // Code blocks are monospace text over a filled box.
    assert!(
        all.iter()
            .any(|c| c["f"] == MONO && c["t"] == "cargo build")
    );
}

#[test]
fn approval_card_needs_every_answer_and_option_fills_it() {
    let _guard = serial();
    let (_s, chat) = offline(topic(
        &[("u", "user", "run it")],
        state(true, "Waiting", Some(approval())),
    ));
    let out = chat.project(&stores(&[]));
    assert_eq!(out["chat.has_question"], Value::Bool(true));
    assert_eq!(out["chat.can_reply"], Value::Bool(false));
    let Value::List(rows) = &out["chat.question_rows"] else {
        panic!()
    };
    let kinds: Vec<_> = rows.iter().map(|r| r["k"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["text", "option", "option", "input", "reply"]);
    let pick = chat.act("option:0:1", &stores(&[]));
    assert_eq!(pick.stores["chat.answer.0"], Value::Text("Decline".into()));
    let out = chat.project(&stores(&[("chat.answer.0", Value::Text("Decline".into()))]));
    assert_eq!(
        out["chat.can_reply"],
        Value::Bool(false),
        "no client: calls unavailable"
    );
    // Out-of-range choices change nothing.
    assert!(chat.act("option:0:7", &stores(&[])).stores.is_empty());
    assert!(chat.act("option:5:0", &stores(&[])).stores.is_empty());
}

#[test]
fn composer_grows_with_its_text_and_the_footer_shifts_the_transcript() {
    let _guard = serial();
    let (_s, chat) = offline(topic(&[], state(false, "", None)));
    let one = chat.project(&stores(&[("chat.draft", Value::Text("hi".into()))]));
    let many = chat.project(&stores(&[("chat.draft", Value::Text("line\n".repeat(7)))]));
    assert!(num(&many, "chat.composer_height") > num(&one, "chat.composer_height"));
    assert!(num(&many, "chat.transcript_height") < num(&one, "chat.transcript_height"));
    assert_eq!(one["chat.empty"], Value::Bool(true));
    assert_eq!(
        one["chat.can_send"],
        Value::Bool(false),
        "no daemon methods"
    );
}

#[test]
fn error_status_and_hidden_panel() {
    let _guard = serial();
    let mut st = state(false, "", None);
    st["error"] = json!("Provider failed");
    let (_s, chat) = offline(topic(&[("u", "user", "x")], st));
    let out = chat.project(&stores(&[]));
    assert_eq!(string(&out, "chat.status"), "Provider failed");
    assert_eq!(out["chat.status_error"], Value::Bool(true));
    // A hidden panel projects nothing and holds no layout.
    assert!(
        chat.project(&stores(&[("panel.open", Value::Bool(false))]))
            .is_empty()
    );
}

// ---- fake daemon -----------------------------------------------------------

pub(crate) enum Frame {
    Msg(Message),
    Drop(tokio::sync::oneshot::Receiver<()>),
}
pub(crate) struct Lab {
    dir: std::path::PathBuf,
    pub client: Client,
    pub calls: mpsc::UnboundedReceiver<(String, Json)>,
    pub push: mpsc::UnboundedSender<Frame>,
    pub fail: Arc<Mutex<Vec<String>>>,
    pub warning: Arc<Mutex<String>>,
    list_delay_ms: Arc<std::sync::atomic::AtomicU64>,
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn welcome(boot: u32) -> Message {
    let mut methods = nacre_protocol::core_methods();
    methods.extend(nacre_protocol::ai_chat::methods());
    for name in [
        "ai.launcher.list",
        "ai.launcher.open",
        "ai.launcher.new",
        "ai.launcher.action",
    ] {
        methods.insert(
            name.into(),
            nacre_protocol::Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 3000,
                params: json!({"type":"object"}),
                result: json!({}),
            },
        );
    }
    Message::Welcome {
        proto: 1,
        min: 1,
        boot: format!("chat-boot-{boot}"),
        topics: BTreeMap::from([("ai.chat".into(), 1)]),
        methods,
        frame_ms: None,
    }
}
pub(crate) async fn lab(initial: Json) -> Lab {
    use std::os::unix::fs::PermissionsExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "chat-lab-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (calls_tx, calls) = mpsc::unbounded_channel();
    let (push, mut frames) = mpsc::unbounded_channel::<Frame>();
    let fail: Arc<Mutex<Vec<String>>> = Default::default();
    let failing = fail.clone();
    let warning = Arc::new(Mutex::new(String::new()));
    let reported_warning = warning.clone();
    let list_delay_ms = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let delayed = list_delay_ms.clone();
    tokio::spawn(async move {
        let mut boot = 0;
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                return;
            };
            boot += 1;
            let (read, mut writer) = stream.into_split();
            let (inbox, mut incoming) = mpsc::unbounded_channel();
            tokio::spawn(async move {
                let mut reader = framing::Reader::new(BufReader::new(read));
                while let Ok(Some(message)) = reader.next().await {
                    if inbox.send(message).is_err() {
                        break;
                    }
                }
            });
            let Some(Message::Hello { .. }) = incoming.recv().await else {
                continue;
            };
            framing::write(&mut writer, &welcome(boot)).await.unwrap();
            let mut seq = 0;
            loop {
                tokio::select! {
                    message = incoming.recv() => match message {
                        Some(Message::Sub { .. }) => {
                            framing::write(&mut writer, &Message::Snap { topic: "ai.chat".into(), seq, data: initial.clone() }).await.unwrap();
                        }
                        Some(Message::Call { id, m, p, .. }) => {
                            let _ = calls_tx.send((m.clone(), p));
                            if m == "ai.launcher.list" {
                                tokio::time::sleep(Duration::from_millis(delayed.load(std::sync::atomic::Ordering::Relaxed))).await;
                            }
                            let chats: Vec<Json> = (1..=12).map(|index| {
                                let key = format!("k{index}");
                                let (title, project, provider, state) = if index == 2 {
                                    ("Brain notes", "brain", "claude", "idle")
                                } else {
                                    ("Build fixes", "nacre", "codex", if index == 1 { "running" } else { "idle" })
                                };
                                json!({"key":key,"title":format!("{title} {index}"),"project":project,"provider":provider,"state":state,"lastActivity":1794000000.0-(index as f64*600.0)})
                            }).collect();
                            let reply = if failing.lock().unwrap().contains(&m) {
                                Reply::Err { err: Failure::new(ErrorCode::Unavailable, "no worker", false) }
                            } else if m == "ai.launcher.list" {
                                let warning = reported_warning.lock().unwrap().clone();
                                Reply::Ok { ok: json!({"chats":chats,"projects":[{"id":"nacre","label":"Nacre"}],"defaultProvider":"codex","warning":warning}) }
                            } else if m == "ai.launcher.open" {
                                Reply::Ok { ok: json!({"ok":true,"focused":true}) }
                            } else {
                                Reply::Ok { ok: json!({"channel":""}) }
                            };
                            framing::write(&mut writer, &Message::Ret { id, reply }).await.unwrap();
                        }
                        Some(_) => {}
                        None => break,
                    },
                    frame = frames.recv() => match frame {
                        Some(Frame::Msg(Message::Snap { topic, data, .. })) => {
                            seq += 1;
                            framing::write(&mut writer, &Message::Snap { topic, seq, data }).await.unwrap();
                        }
                        Some(Frame::Msg(message)) => framing::write(&mut writer, &message).await.unwrap(),
                        Some(Frame::Drop(resume)) => {
                            drop(writer);
                            let _ = resume.await;
                            break;
                        },
                        None => return,
                    },
                }
            }
        }
    });
    let mut config = Config::new(socket, vec![]);
    config.event = Some(Arc::new(route_event));
    let client = Client::start(config).unwrap();
    Lab {
        dir,
        client,
        calls,
        push,
        fail,
        warning,
        list_delay_ms,
    }
}
pub(crate) async fn call_with_timeout(lab: &mut Lab) -> (String, Json) {
    tokio::time::timeout(Duration::from_secs(5), lab.calls.recv())
        .await
        .expect("fake service call timed out")
        .expect("fake service call channel closed")
}
async fn expect_launcher_call(lab: &mut Lab, method: &str, params: Json) {
    loop {
        let call = call_with_timeout(lab).await;
        if call.0 == method {
            assert_eq!(call.1, params);
            return;
        }
        assert_eq!(
            call.0, "ai.launcher.list",
            "unexpected call before {method}"
        );
    }
}
async fn finish_launcher_action(chat: &Chat, stores: &Stores) {
    for _ in 0..100 {
        chat.project(stores);
        if !chat
            .st
            .borrow()
            .pending
            .iter()
            .any(|pending| pending.call == Call::LauncherAction)
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("launcher action acknowledgement timed out");
}
fn snap(data: Json) -> Frame {
    Frame::Msg(Message::Snap {
        topic: "ai.chat".into(),
        seq: 0,
        data,
    })
}
pub(crate) async fn chat_for(lab: &Lab) -> Chat {
    let path = lab.dir.join("cache/chats.json");
    chat_for_cache(lab, path).await
}
async fn lab_source(lab: &Lab) -> Arc<dyn bead::core::source::Source> {
    for _ in 0..500 {
        if lab.client.snapshot().connected && lab.client.status().boot.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let topic = lab.client.lease_topic("ai.chat").unwrap();
    Arc::new(topic)
}
async fn chat_for_cache(lab: &Lab, cache: std::path::PathBuf) -> Chat {
    Chat::new(Some(lab.client.clone()), lab_source(lab).await, meter)
        .with_launcher_cache_path(cache)
}
async fn until(
    chat: &Chat,
    st: &Stores,
    what: &str,
    done: impl Fn(&BTreeMap<String, Value>) -> bool,
) -> BTreeMap<String, Value> {
    for _ in 0..800 {
        let out = chat.project(st);
        if done(&out) {
            return out;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}
fn opened(chat: &Chat) -> bool {
    chat.st.borrow().opened_at.is_some()
}

#[tokio::test(flavor = "current_thread")]
async fn open_heartbeat_send_stop_and_approve_use_the_published_calls() {
    let _guard = serial();
    let mut lab = lab(topic(&[("u1", "user", "hi")], state(false, "", None))).await;
    let chat = chat_for(&lab)
        .await
        .with_heartbeat(Duration::from_millis(80));
    let none = stores(&[]);
    let out = until(&chat, &none, "history", |o| num(o, "chat.extent") > 0.).await;
    assert_eq!(out["chat.can_send"], Value::Bool(false), "empty draft");
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        ("ai.chat.open".into(), json!({"channel":""}))
    );
    // Heartbeat: the worker is refreshed again after the period, not before.
    for _ in 0..300 {
        chat.project(&none);
        if let Ok(call) = lab.calls.try_recv() {
            assert_eq!(call.0, "ai.chat.open");
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    // Send: exact text, then the composer clears and the view follows.
    let draft = stores(&[("chat.draft", Value::Text("build it\nplease".into()))]);
    let out = until(&chat, &draft, "can_send", |o| {
        o["chat.can_send"] == Value::Bool(true)
    })
    .await;
    assert_eq!(out["chat.can_new"], Value::Bool(true));
    let sent = chat.act("send", &draft);
    assert_eq!(sent.stores["chat.draft"], Value::Text(String::new()));
    loop {
        let call = lab.calls.recv().await.unwrap();
        if call.0 == "ai.chat.send" {
            assert_eq!(
                call.1,
                json!({"channel":"","text":"build it\nplease","attachments":[]})
            );
            break;
        }
    }
    // Whitespace-only text is never sent.
    assert!(
        chat.act(
            "send",
            &stores(&[("chat.draft", Value::Text("  \n".into()))])
        )
        .stores
        .is_empty()
    );
    // Stop while busy, approval answers.
    lab.push
        .send(snap(topic(
            &[("u1", "user", "hi"), ("a1", "assistant", "run?")],
            state(true, "Waiting", Some(approval())),
        )))
        .unwrap();
    let out = until(&chat, &none, "question", |o| {
        o["chat.has_question"] == Value::Bool(true)
    })
    .await;
    assert_eq!(out["chat.can_stop"], Value::Bool(true));
    chat.act("stop", &none);
    let pick = chat.act("option:0:0", &none);
    let answered = stores(&[("chat.answer.0", pick.stores["chat.answer.0"].clone())]);
    chat.act("reply", &answered);
    let mut seen = Vec::new();
    while seen.len() < 2 {
        let call = lab.calls.recv().await.unwrap();
        if call.0 != "ai.chat.open" {
            seen.push(call);
        }
    }
    assert_eq!(seen[0], ("ai.chat.cancel".into(), json!({"channel":""})));
    assert_eq!(
        seen[1],
        (
            "ai.chat.approve".into(),
            json!({"channel":"","id":"approval-1","answers":{"approval":{"answers":["Allow once"]}}})
        )
    );
    // New chat and workspace are refused while busy, and a provider switch is explicit.
    chat.act("new", &none);
    chat.act("provider:codex", &none);
    chat.act("provider:bogus", &none);
    let call = loop {
        let call = lab.calls.recv().await.unwrap();
        if call.0 != "ai.chat.open" {
            break call;
        }
    };
    assert_eq!(
        call,
        (
            "ai.chat.provider".into(),
            json!({"channel":"","provider":"codex"})
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn a_failed_call_shows_an_error_and_a_refused_send_restores_the_text() {
    let _guard = serial();
    let mut lab = lab(topic(&[], state(false, "", None))).await;
    let chat = chat_for(&lab).await;
    let none = stores(&[]);
    until(&chat, &none, "state", |o| {
        string(o, "chat.subtitle").starts_with("Claude")
    })
    .await;
    lab.fail.lock().unwrap().push("ai.chat.send".into());
    let draft = stores(&[("chat.draft", Value::Text("lost".into()))]);
    chat.act("send", &draft);
    let out = until(&chat, &draft, "error", |o| {
        o["chat.status_error"] == Value::Bool(true)
    })
    .await;
    assert_eq!(string(&out, "chat.status"), "Request failed");
    assert!(lab.calls.try_recv().is_ok());
    // The worker later refuses a dispatched submission: the text returns in front of
    // what was typed since.
    lab.push
        .send(Frame::Msg(Message::Ev {
            topic: "ai.chat".into(),
            name: "send_failed".into(),
            seq: 1,
            data: json!({"channel":"","text":"unsent","attachments":[]}),
        }))
        .unwrap();
    let typed = stores(&[("chat.draft", Value::Text("typed".into()))]);
    let out = until(&chat, &typed, "restore", |o| o.contains_key("chat.draft")).await;
    assert_eq!(string(&out, "chat.draft"), "unsent\n\ntyped");
    // Other channels' events are ignored.
    route_event(&Message::Ev {
        topic: "ai.chat".into(),
        name: "send_failed".into(),
        seq: 2,
        data: json!({"channel":"p1","text":"other","attachments":[]}),
    });
    assert!(!chat.project(&typed).contains_key("chat.draft"));
}

#[tokio::test(flavor = "current_thread")]
async fn a_dropped_daemon_link_reopens_the_worker_and_resyncs() {
    let _guard = serial();
    let mut lab = lab(topic(&[("u1", "user", "before")], state(false, "", None))).await;
    let chat = chat_for(&lab).await;
    let none = stores(&[]);
    until(&chat, &none, "history", |o| num(o, "chat.extent") > 0.).await;
    assert_eq!(lab.calls.recv().await.unwrap().0, "ai.chat.open");
    let (resume, disconnected) = tokio::sync::oneshot::channel();
    lab.push.send(Frame::Drop(disconnected)).unwrap();
    until(&chat, &none, "disconnect", |o| {
        o["chat.can_new"] == Value::Bool(false) && !opened(&chat)
    })
    .await;
    assert!(!opened(&chat));
    // Keep reconnection parked until the disconnected projection was observed.
    resume.send(()).unwrap();
    // The client reconnects with a new boot id; the next projection reopens the channel.
    let out = until(&chat, &none, "reconnect", |o| {
        o["chat.can_new"] == Value::Bool(true)
    })
    .await;
    assert!(
        num(&out, "chat.extent") > 0.,
        "transcript survives the reconnect"
    );
    assert_eq!(lab.calls.recv().await.unwrap().0, "ai.chat.open");
}

#[tokio::test(flavor = "current_thread")]
async fn launcher_lists_searches_opens_focuses_and_starts_quick_chat_on_demand() {
    let _guard = serial();
    let mut lab = lab(topic(&[], state(false, "", None))).await;
    let chat = chat_for(&lab).await;
    let launcher = Stores::default();
    let first = chat.project(&launcher);
    assert_eq!(first["launcher.can_new"], Value::Bool(true));
    assert_eq!(call_with_timeout(&mut lab).await.0, "ai.launcher.list");
    let listed = until(
        &chat,
        &launcher,
        "launcher list",
        |out| matches!(&out["launcher.chats"], Value::List(rows) if rows.len() == 12),
    )
    .await;
    assert_eq!(listed["launcher.can_new"], Value::Bool(true));
    let populated = chat.st.borrow().launcher.clone();
    chat.st.borrow_mut().launcher.as_mut().unwrap()["chats"] = json!([]);
    let empty = chat.project(&launcher);
    assert!(!string(&empty, "launcher.empty_state").is_empty());
    chat.st.borrow_mut().launcher = populated;
    let Value::List(rows) = &listed["launcher.chats"] else {
        panic!("launcher rows")
    };
    assert_eq!(rows[0]["provider"], "Codex");
    assert_eq!(rows[0]["state"], "running");
    assert!(
        rows[0].get("project").is_none(),
        "project labels stay out of rows"
    );
    assert_eq!(
        rows[0]["group"], "Running",
        "the first running row heads its group"
    );
    assert!(!rows[0]["lastActivityLabel"].as_str().unwrap().is_empty());

    let search = launcher_stores(&[("launcher.search", Value::Text("brain".into()))]);
    let filtered = chat.project(&search);
    let Value::List(rows) = &filtered["launcher.chats"] else {
        panic!("filtered rows")
    };
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["key"], "k2");
    let move_down = chat.act("launcher:move:1", &search);
    assert_eq!(
        move_down.stores["launcher.selected"],
        Value::Text("k2".into())
    );

    let selected = launcher_stores(&[("launcher.selected", Value::Text("k2".into()))]);
    chat.act("launcher:open-selected", &selected);
    expect_launcher_call(&mut lab, "ai.launcher.open", json!({"key":"k2"})).await;
    finish_launcher_action(&chat, &launcher).await;
    // The shared service acknowledges a focused or newly opened chat window.
    chat.act("launcher:open:k1", &launcher);
    expect_launcher_call(&mut lab, "ai.launcher.open", json!({"key":"k1"})).await;
    finish_launcher_action(&chat, &launcher).await;

    let configured = launcher_stores(&[("launcher.provider", Value::Text("claude".into()))]);
    chat.act("launcher:new", &configured);
    expect_launcher_call(
        &mut lab,
        "ai.launcher.new",
        json!({"provider":"claude","project":"general-chat"}),
    )
    .await;
    finish_launcher_action(&chat, &launcher).await;
    let quick = chat.act("launcher:quick-chat", &launcher);
    assert_eq!(quick.stores["chat.section"], Value::Text("chat".into()));
    assert_eq!(quick.stores["launcher.quick_chat"], Value::Bool(true));
    let back = chat.act("launcher:back", &launcher);
    assert_eq!(back.stores["chat.section"], Value::Text("launcher".into()));
    assert_eq!(back.stores["launcher.quick_chat"], Value::Bool(false));
    // D7: Enter in the launcher's composer sends the draft with the launcher's provider.
    let ask = launcher_stores(&[
        ("launcher.provider", Value::Text("codex".into())),
        ("chat.draft", Value::Text("hello".into())),
    ]);
    let sent = chat.act("launcher:quick-chat", &ask);
    assert_eq!(sent.stores["chat.draft"], Value::Text(String::new()));
    let mut seen = Vec::new();
    while seen.len() < 2 {
        let call = call_with_timeout(&mut lab).await;
        if call.0 != "ai.launcher.list" {
            seen.push(call);
        }
    }
    seen.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        seen,
        [
            (
                "ai.chat.provider".to_owned(),
                json!({"channel":"","provider":"codex"})
            ),
            (
                "ai.chat.send".to_owned(),
                json!({"channel":"","text":"hello","attachments":[]})
            ),
        ]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn launcher_cache_is_first_frame_and_refreshes_while_actions_stay_available() {
    let _guard = serial();
    let mut lab = lab(topic(&[], state(false, "", None))).await;
    lab.list_delay_ms
        .store(150, std::sync::atomic::Ordering::Relaxed);
    let cache = lab.dir.join("cache/chats.json");
    let launcher = launcher_stores(&[]);
    let cold_source = lab_source(&lab).await;
    let cold_started = std::time::Instant::now();
    let cold_chat = Chat::new(Some(lab.client.clone()), cold_source, meter)
        .with_launcher_cache_path(cache.clone());
    cold_chat.project(&launcher);
    assert_eq!(call_with_timeout(&mut lab).await.0, "ai.launcher.list");
    let cold_deadline = std::time::Instant::now() + Duration::from_secs(2);
    let cold_ms = loop {
        let out = cold_chat.project(&launcher);
        if matches!(&out["launcher.chats"], Value::List(rows) if rows.len() == 12) {
            break cold_started.elapsed().as_millis();
        }
        assert!(
            std::time::Instant::now() < cold_deadline,
            "cold list did not arrive within 2 seconds"
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    };
    assert!(
        cold_ms >= 100,
        "delayed cold fetch returned implausibly early: {cold_ms} ms"
    );

    let mut cached = Chat::load_launcher_cache(&cache).expect("cold fetch wrote cache");
    cached["chats"][0]["title"] = json!("Saved cache title");
    cached["chats"][0]["state"] = json!("running");
    Chat::save_launcher_cache(&cache, &cached);
    let warm_source = lab_source(&lab).await;
    let warm_started = std::time::Instant::now();
    let warm_chat =
        Chat::new(Some(lab.client.clone()), warm_source, meter).with_launcher_cache_path(cache);
    let first = warm_chat.project(&launcher);
    let first_frame_ms = warm_started.elapsed().as_millis();
    let Value::List(rows) = &first["launcher.chats"] else {
        panic!("cached chats")
    };
    assert_eq!(rows.len(), 12);
    assert_eq!(rows[0]["title"], "Saved cache title");
    assert_eq!(rows[0]["state"], "idle");
    assert_eq!(first["launcher.pulse"], number(0.));
    if std::env::var("NACRE_TEST_PERF").as_deref() == Ok("1") {
        assert!(
            first_frame_ms < 30,
            "cached first-frame latency gate exceeded 30 ms: {first_frame_ms} ms"
        );
    }
    assert_eq!(first["launcher.can_new"], Value::Bool(true));
    warm_chat.act("launcher:new", &launcher);
    let mut saw_list = false;
    let mut saw_new = false;
    for _ in 0..2 {
        let call = call_with_timeout(&mut lab).await;
        saw_list |= call.0 == "ai.launcher.list";
        if call.0 == "ai.launcher.new" {
            assert_eq!(call.1, json!({"provider":"codex","project":"general-chat"}));
            saw_new = true;
        }
    }
    assert!(
        saw_list && saw_new,
        "new action is sent while the refresh is pending"
    );
    let fresh_deadline = warm_started + Duration::from_secs(2);
    loop {
        let out = warm_chat.project(&launcher);
        if matches!(&out["launcher.chats"], Value::List(rows) if rows.first().is_some_and(|row| row["title"] == "Build fixes 1"))
        {
            break;
        }
        assert!(
            std::time::Instant::now() < fresh_deadline,
            "fresh list did not arrive within 2 seconds"
        );
        tokio::time::sleep(Duration::from_millis(2)).await;
    }
    let fresh_ms = warm_started.elapsed().as_millis();
    if std::env::var("NACRE_TEST_PERF").as_deref() == Ok("1") {
        assert!(
            fresh_ms < 300,
            "fresh local fake service exceeded 300 ms: {fresh_ms} ms"
        );
    }
    eprintln!(
        "launcher timing: cold open-to-list={cold_ms} ms; cached first-frame={first_frame_ms} ms; fresh refresh={fresh_ms} ms"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn launcher_partial_warning_keeps_valid_rows_and_hides_raw_stderr() {
    let _guard = serial();
    let mut lab = lab(topic(&[], state(false, "", None))).await;
    *lab.warning.lock().unwrap() = "bash: /secret/path: permission denied".into();
    let chat = chat_for(&lab).await;
    let launcher = launcher_stores(&[]);
    chat.project(&launcher);
    assert_eq!(call_with_timeout(&mut lab).await.0, "ai.launcher.list");
    let out = until(
        &chat,
        &launcher,
        "partial launcher list",
        |out| matches!(&out["launcher.chats"], Value::List(rows) if !rows.is_empty()),
    )
    .await;
    assert_eq!(
        string(&out, "launcher.warning"),
        tr("bead.common.request-failed", &[])
    );
    assert!(!string(&out, "launcher.warning").contains("/secret/path"));
    let cached = std::fs::read_to_string(lab.dir.join("cache/chats.json")).unwrap();
    assert!(!cached.contains("permission denied"));
    assert!(cached.contains("Brain notes"));
}

#[test]
fn launcher_cache_rejects_corrupt_and_oversized_files() {
    let _guard = serial();
    let dir = std::env::temp_dir().join(format!("launcher-cache-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("chats.json");
    std::fs::write(&path, b"not json").unwrap();
    assert!(Chat::load_launcher_cache(&path).is_none());
    std::fs::write(&path, vec![b'x'; LAUNCHER_CACHE_LIMIT + 1]).unwrap();
    assert!(Chat::load_launcher_cache(&path).is_none());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unavailable_launcher_does_not_claim_empty_history() {
    let _guard = serial();
    let (_, chat) = offline(json!({"available": false, "channels": {}}));
    let projected = chat.project(&Stores::default());
    assert!(!string(&projected, "launcher.warning").is_empty());
    assert_eq!(string(&projected, "launcher.empty_state"), "");
}

#[test]
fn session_without_provider_timestamp_does_not_claim_old_activity() {
    let _guard = serial();
    let (_, chat) = offline(json!({"available":true,"channels":{}}));
    chat.st.borrow_mut().launcher = Some(
        json!({"chats":[{"key":"session","title":"Running chat","provider":"codex","project":"Demo","state":"running","lastActivity":0}],"projects":[],"warning":""}),
    );
    let projected = chat.project(&Stores::default());
    let Value::List(rows) = &projected["launcher.chats"] else {
        panic!("chat list")
    };
    assert!(
        rows[0].get("project").is_none(),
        "legacy project metadata is hidden"
    );
    assert_eq!(
        rows[0]["lastActivityLabel"],
        tr("bead.panel.launcher-activity-unknown", &[])
    );
}

#[tokio::test(flavor = "current_thread")]
async fn launching_a_window_releases_attached_keyboard_but_keeps_pinned_and_detached_launchers() {
    let _guard = serial();
    for (pinned, detached) in [(false, false), (true, false), (false, true)] {
        let mut lab = lab(topic(&[], state(false, "", None))).await;
        let chat = chat_for(&lab).await;
        let stores = launcher_stores(&[
            ("chat.pinned", Value::Bool(pinned)),
            ("view.detached", Value::Bool(detached)),
        ]);
        chat.project(&stores);
        assert_eq!(call_with_timeout(&mut lab).await.0, "ai.launcher.list");
        until(
            &chat,
            &stores,
            "history",
            |out| matches!(&out["launcher.chats"], Value::List(rows) if !rows.is_empty()),
        )
        .await;
        chat.act("launcher:open:k1", &stores);
        assert_eq!(call_with_timeout(&mut lab).await.0, "ai.launcher.open");
        let out = until(&chat, &stores, "window handoff", |out| {
            out.get("bead.focused") == Some(&Value::Text(String::new()))
        })
        .await;
        assert_eq!(
            out.get("panel.open") == Some(&Value::Bool(false)),
            !pinned && !detached
        );
        assert_eq!(out["chat.section"], Value::Text("launcher".into()));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn launcher_load_error_is_visible_and_does_not_retry_each_projection() {
    let _guard = serial();
    let mut lab = lab(topic(&[], state(false, "", None))).await;
    lab.fail.lock().unwrap().push("ai.launcher.list".into());
    let chat = chat_for(&lab).await;
    let launcher = Stores::default();
    let initial = chat.project(&launcher);
    assert!(!string(&initial, "launcher.empty_state").is_empty());
    assert_eq!(call_with_timeout(&mut lab).await.0, "ai.launcher.list");
    let mut failed = BTreeMap::new();
    for _ in 0..100 {
        failed = chat.project(&launcher);
        if !string(&failed, "launcher.warning").is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(!string(&failed, "launcher.warning").is_empty());
    assert_eq!(string(&failed, "launcher.empty_state"), "");
    for _ in 0..20 {
        chat.project(&launcher);
    }
    assert!(
        lab.calls.try_recv().is_err(),
        "a failed list is throttled instead of retried every frame"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn launcher_matches_scale_and_detached_goldens() -> bead_test::Result<()> {
    let _guard = serial();
    let mut lab = lab(topic(&[], state(false, "", None))).await;
    let chat = chat_for(&lab).await;
    let launcher = Stores::default();
    chat.project(&launcher);
    let _ = call_with_timeout(&mut lab).await;
    let mut projected = until(
        &chat,
        &launcher,
        "launcher golden data",
        |out| matches!(&out["launcher.chats"], Value::List(rows) if rows.len() == 12),
    )
    .await;
    let Value::List(rows) = &projected["launcher.chats"] else {
        panic!("launcher rows")
    };
    let mut rows = rows.as_ref().clone();
    for row in &mut rows {
        row["lastActivityLabel"] = json!("2 hr ago");
    }
    projected.insert("launcher.chats".into(), Value::list(rows));
    projected.insert("panel.open".into(), Value::Bool(true));
    for scale in [1., 1.5] {
        let mut regular = harness(projected.clone(), scale)?;
        let nodes = regular.session.view_nodes();
        let layers = &regular.scene().layers;
        for id in [
            "chat.header.logo",
            "chat.header.title",
            "chat.header.pin",
            "chat.compose",
            "chat.composer",
            "chat.actions.send",
            "chat.secondary.new-chat",
            "chat.header.provider.claude",
            "chat.header.provider.codex",
            "chat.search",
            "chat.refresh",
            "chat.recent-chats",
            "chat.secondary.brain",
            "chat.secondary.settings",
        ] {
            assert!(
                layers.iter().any(|layer| layer.id == id),
                "missing launcher node {id}"
            );
        }
        assert!(nodes.iter().any(
            |node| matches!(&node.action_arg, Some(Value::Text(arg)) if arg == "launcher:new")
        ));
        assert!(nodes.iter().any(
            |node| matches!(&node.action_arg, Some(Value::Text(arg)) if arg == "launcher:quick-chat")
        ));
        assert!(
            !nodes
                .iter()
                .any(|node| node.id.to_lowercase().contains("project"))
        );
        assert!(!projected.contains_key("launcher.project"));
        assert!(!projected.contains_key("launcher.projects"));
        let row = nodes
            .iter()
            .find(|node| node.id.contains("recent-chats") && node.id.ends_with("chat-row"))
            .expect("chat row");
        let title = layers
            .iter()
            .find(|layer| layer.id.starts_with(&row.id) && layer.id.ends_with(".title"))
            .expect("row title");
        let meta = layers
            .iter()
            .find(|layer| layer.id.starts_with(&row.id) && layer.id.ends_with(".meta"))
            .expect("row metadata");
        let row_rect = bead::ui::rect(&row.draw);
        for child in [title, meta] {
            let rect = child.bounds;
            assert!(
                rect.x >= row_rect.x
                    && rect.y >= row_rect.y
                    && rect.x + rect.w <= row_rect.x + row_rect.w
                    && rect.y + rect.h <= row_rect.y + row_rect.h,
                "row content is clipped: row={row_rect:?}, child={rect:?}"
            );
        }
        for layer in layers.iter().filter(|layer| {
            layer.id.starts_with(&row.id)
                && (layer.id.ends_with(".title") || layer.id.ends_with(".details"))
        }) {
            for primitive in &regular.scene().primitives[layer.own.clone()] {
                if let bead::render::Primitive::Text { rect, size, .. } = primitive {
                    assert!(
                        rect.h + 0.1 >= size * 1.4,
                        "chat text line is clipped: {rect:?}, font={size}"
                    );
                }
            }
        }
        golden("launcher", &mut regular, scale)?;
        if scale == 1. {
            // D7: the same launcher on the Orient light fixture.
            golden(
                "launcher-light",
                &mut harness_in(projected.clone(), 1., LIGHT)?,
                1.,
            )?;
        }
        let mut detached_projection = projected.clone();
        detached_projection.insert("view.detached".into(), Value::Bool(true));
        let mut detached = harness(detached_projection, scale)?;
        assert_eq!(detached.scene().opacity, 1., "detached panel stays visible");
        assert!(
            !detached
                .session
                .view_nodes()
                .iter()
                .any(|node| node.id.ends_with(".pin"))
        );
        golden("launcher-detached", &mut detached, scale)?;
        regular.presented()?;
        let mut utility = regular;
        let mut utility_projection = projected.clone();
        utility_projection.insert(
            "launcher.warning".into(),
            text(tr("bead.common.request-failed", &[])),
        );
        utility_projection.insert("bead.hovered".into(), text("chat.secondary.brain"));
        utility.update(utility_projection)?;
        assert_eq!(
            utility.scene().opacity,
            1.,
            "open launcher stays visible on hover"
        );
        let tooltip = utility
            .scene()
            .layers
            .iter()
            .find(|layer| layer.id == "chat.footer-tooltip")
            .expect("footer tooltip");
        let footer = utility
            .scene()
            .layers
            .iter()
            .find(|layer| layer.id == "chat.secondary")
            .expect("footer");
        assert!(tooltip.bounds.y > 0. && tooltip.bounds.y + tooltip.bounds.h < footer.bounds.y);
        assert!(
            tooltip.bounds.x >= footer.bounds.x
                && tooltip.bounds.x + tooltip.bounds.w <= footer.bounds.x + footer.bounds.w
        );
        golden("launcher-warning-tooltip", &mut utility, scale)?;
    }
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn missing_ai_shows_the_not_installed_state_and_calls_nothing() -> bead_test::Result<()> {
    // N0234: `ai: false` in the capability flags (package absent) replaces a failing sidebar.
    let _guard = serial();
    let mut lab = lab(topic(&[], state(false, "", None))).await;
    let flags = Arc::new(FakeSource::new(json!({"flags": {"ai": false, "brain": false}})).unwrap());
    let chat = chat_for(&lab).await.with_capabilities(Some(flags));
    let quick = stores(&[("launcher.quick_chat", Value::Bool(true))]);
    for at in [&quick, &launcher_stores(&[])] {
        assert_eq!(chat.project(at)["chat.missing"], Value::Bool(true));
        chat.act("launcher:new", at);
    }
    assert!(
        tokio::time::timeout(Duration::from_millis(200), lab.calls.recv())
            .await
            .is_err(),
        "no ai.* call without Nacre AI"
    );
    let mut projected = chat.project(&launcher_stores(&[]));
    projected.insert("panel.open".into(), Value::Bool(true));
    for scale in [1., 1.5] {
        let mut h = harness(projected.clone(), scale)?;
        let layers = &h.scene().layers;
        let shown = |id: &str| layers.iter().any(|layer| layer.id.starts_with(id));
        for id in [
            "chat.header.logo",
            "chat.header.pin",
            "chat.missing-title",
            "chat.missing-hint",
        ] {
            assert!(shown(id), "missing {id}");
        }
        for id in [
            "chat.header.provider",
            "chat.search",
            "chat.compose",
            "chat.secondary",
        ] {
            assert!(!shown(id), "{id} needs Nacre AI");
        }
        golden("missing", &mut h, scale)?;
    }
    Ok(())
}

#[test]
fn compact_conversation_menu_and_hover_copy_fit_without_reflow() -> bead_test::Result<()> {
    let _guard = serial();
    let (_, chat) = offline(topic(
        &[
            ("u1", "user", "Please inspect this"),
            ("a1", "assistant", "The result is ready."),
        ],
        state(false, "", None),
    ));
    let projected = chat.project(&stores(&[]));
    for scale in [1., 1.5] {
        let mut regular = harness(projected.clone(), scale)?;
        assert!(
            regular
                .session
                .view_nodes()
                .iter()
                .all(|node| !node.id.contains(".copy-"))
        );
        let extent_before = regular.session.stores.get("chat.extent").cloned();
        let bubble = regular
            .scene()
            .layers
            .iter()
            .find(|layer| layer.id.starts_with("chat.transcript") && layer.id.contains(".message-"))
            .expect("message bubble");
        let bubble_id = bubble.id.clone();
        let rect = bubble.bounds;
        let card = regular
            .scene()
            .layers
            .iter()
            .find(|layer| layer.id == "chat.compose")
            .expect("composer card")
            .bounds;
        let composer = regular
            .scene()
            .layers
            .iter()
            .find(|layer| layer.id == "chat.composer")
            .expect("composer")
            .bounds;
        let send = regular
            .scene()
            .layers
            .iter()
            .find(|layer| layer.id == "chat.actions.send")
            .expect("send button")
            .bounds;
        let actions = regular
            .scene()
            .layers
            .iter()
            .find(|layer| layer.id == "chat.actions")
            .expect("composer actions")
            .bounds;
        assert!(
            send.x >= card.x
                && send.y >= card.y
                && send.x + send.w <= card.x + card.w
                && send.y + send.h <= card.y + card.h
        );
        assert!(actions.y >= composer.y + composer.h && actions.y + actions.h <= card.y + card.h);
        regular.script([Input::Pointer {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        }])?;
        let copy_count = regular
            .session
            .view_nodes()
            .iter()
            .filter(|node| node.id.contains(".copy-"))
            .count();
        assert_eq!(
            copy_count, 1,
            "hover reveals one message Copy at scale {scale}"
        );
        assert_eq!(
            regular.session.stores.get("chat.extent"),
            extent_before.as_ref(),
            "hover does not reflow transcript"
        );
        let after = regular
            .scene()
            .layers
            .iter()
            .find(|layer| layer.id == bubble_id)
            .expect("same message bubble")
            .bounds;
        assert_eq!(after, rect, "hover keeps bubble geometry fixed");
        let message_key = cells(&projected)
            .iter()
            .find(|cell| cell["k"] == "fill")
            .and_then(|cell| cell["message"].as_str())
            .expect("message key")
            .to_owned();
        let message_node = regular
            .session
            .view_nodes()
            .iter()
            .find(|node| node.id.contains(&format!(".message-{message_key}")))
            .expect("focusable message")
            .id
            .clone();
        regular.script([Input::Pointer { x: 0., y: 0. }])?;
        let mut keyboard_projection = projected.clone();
        keyboard_projection.insert("bead.focused".into(), text(message_node));
        regular.update(keyboard_projection)?;
        regular.script([Input::Tab { backwards: false }])?;
        let focused = match regular.session.stores.get("bead.focused") {
            Some(Value::Text(value)) => value.clone(),
            _ => String::new(),
        };
        assert!(
            focused.contains(&format!(".copy-{message_key}")),
            "Tab from a message reaches its Copy control"
        );

        let mut menu_projection = projected.clone();
        menu_projection.insert("chat.menu.open".into(), Value::Bool(true));
        let menu = harness(menu_projection, scale)?;
        for action in ["workspace", "new-chat"] {
            assert!(
                menu.scene()
                    .layers
                    .iter()
                    .any(|layer| layer.id.contains("overflow-menu") && layer.id.ends_with(action)),
                "missing menu item {action}"
            );
        }
        let transcript_order = menu
            .scene()
            .layers
            .iter()
            .position(|layer| layer.id == "chat.transcript")
            .expect("transcript");
        let menu_order = menu
            .scene()
            .layers
            .iter()
            .position(|layer| {
                layer.id.contains("overflow-menu") && layer.id.ends_with(".workspace")
            })
            .expect("workspace menu layer");
        assert!(
            menu_order > transcript_order,
            "overflow menu paints above the transcript"
        );
    }
    Ok(())
}

// ---- goldens and the streaming frame budget ----------------------------------

pub(crate) fn harness(out: BTreeMap<String, Value>, scale: f32) -> bead_test::Result<Harness> {
    harness_in(out, scale, "palette.json")
}
const LIGHT: &str = "tests/fixtures/palette-light.json";
fn harness_in(
    out: BTreeMap<String, Value>,
    scale: f32,
    palette: &str,
) -> bead_test::Result<Harness> {
    Harness::load(
        &root(),
        Path::new("chat/main.luau"),
        &root().join(palette),
        out,
        bead_test::Config {
            width: FRAME_WIDTH,
            height: FRAME_HEIGHT,
            scale,
        },
    )
}

#[test]
fn transcript_scroll_preserves_touchpad_pixels_and_shared_wheel_distance() {
    let text = "scroll fixture ".repeat(80);
    let messages: Vec<_> = (0..30)
        .map(|index| {
            (
                format!("message-{index}"),
                if index % 2 == 0 { "user" } else { "assistant" }.to_owned(),
                text.clone(),
            )
        })
        .collect();
    let borrowed: Vec<_> = messages
        .iter()
        .map(|(id, role, body)| (id.as_str(), role.as_str(), body.as_str()))
        .collect();
    let (_source, chat) = offline(topic(&borrowed, state(false, "", None)));
    let mut projected = chat.project(&stores(&[]));
    projected.insert("chat.scroll".into(), Value::Number(0.));
    let mut h = harness(projected, 1.).unwrap();
    let region = h
        .session
        .view_nodes()
        .iter()
        .find(|node| node.scroll_store.as_deref() == Some("chat.scroll"))
        .map(|node| bead::ui::rect(&node.draw))
        .expect("chat transcript viewport");
    assert!(
        h.session
            .view_nodes()
            .iter()
            .find(|node| node.scroll_store.as_deref() == Some("chat.scroll"))
            .and_then(|node| node.scroll_limit)
            .is_some_and(|limit| limit > 130.)
    );
    for (delta, expected) in [(23.5, 23.5), (100., 123.5)] {
        h.script([bead_test::Input::Scroll {
            x: region.x + region.w / 2.,
            y: region.y + region.h / 2.,
            delta,
        }])
        .unwrap();
        assert_eq!(
            h.session.stores.get("chat.scroll"),
            Some(&Value::Number(expected))
        );
    }
}

fn golden(name: &str, h: &mut Harness, scale: f32) -> bead_test::Result<()> {
    // Stills use the flat CPU oracle: every primitive is painted into a fresh
    // target, independent of retained-tree repair/preparation between captures.
    // The harness still supplies the real view, layout, fonts and themed assets.
    use bead::render::{CpuRenderer, Damage, PixelFormat, Renderer, ShmTarget};
    let mut scene = h.scene().clone();
    scene.motion = None;
    scene.slide = None;
    scene.prepared.clear();
    scene.opacity = 1.;
    let (width, height) = (
        (FRAME_WIDTH as f32 * scale).ceil() as u32,
        (FRAME_HEIGHT as f32 * scale).ceil() as u32,
    );
    let mut frame = bead_test::Frame {
        width,
        height,
        pixels: vec![0; (width * height * 4) as usize],
    };
    let mut renderer = CpuRenderer::default();
    renderer.prepare(&scene, scale)?;
    renderer.render(
        &scene,
        ShmTarget {
            bytes: &mut frame.pixels,
            width,
            height,
            scale,
            format: PixelFormat::Rgba,
        },
        Damage::full(width, height),
    )?;
    for pixel in frame.pixels.as_chunks_mut::<4>().0 {
        let alpha = u32::from(pixel[3]);
        if alpha > 0 && alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = (u32::from(*channel) * 255 / alpha).min(255) as u8;
            }
        }
    }
    let path = root().join(format!("tests/goldens/chat-{name}-{scale}.png"));
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&path)?;
    }
    frame.compare(
        &path,
        Default::default(),
        &std::env::temp_dir().join(format!("bead-chat-{name}-{scale}-diff")),
    )
}

#[test]
fn chat_states_match_goldens() -> bead_test::Result<()> {
    let _guard = serial();
    let code = "Here is the fix:\n\n1. Update the manifest\n2. Run the build\n\n```sh\ncargo build --release\n```\n\nSee [the docs](https://example.org) for `flags`.";
    let states: [(&str, Json, &str); 4] = [
        ("empty", topic(&[], state(false, "", None)), ""),
        (
            "conversation",
            topic(
                &[
                    ("u1", "user", "Why does the release build fail on CI?"),
                    (
                        "a1",
                        "assistant",
                        "The linker runs out of memory because the job builds with too many parallel jobs. Limit it to two and the build fits comfortably.",
                    ),
                    ("u2", "user", "Thanks. Anything else?"),
                ],
                state(true, "Thinking", None),
            ),
            "Can you also\ncheck the cache?",
        ),
        (
            "code",
            topic(
                &[("u1", "user", "Show me"), ("a1", "assistant", code)],
                state(false, "", None),
            ),
            "",
        ),
        (
            "approval",
            topic(
                &[
                    ("u1", "user", "Build it"),
                    ("a1", "assistant", "Ready to run the build."),
                ],
                state(true, "Waiting", Some(approval())),
            ),
            "",
        ),
    ];
    for scale in [1., 1.5] {
        for (name, data, draft) in &states {
            let (_s, chat) = offline(data.clone());
            let mut out = chat.project(&stores(&[("chat.draft", Value::Text((*draft).into()))]));
            out.insert("chat.draft".into(), Value::Text((*draft).into()));
            let mut h = harness(out.clone(), scale)?;
            // A composer, transcript and (for approval) question rows exist as nodes.
            let ids: Vec<String> = h
                .session
                .view_nodes()
                .iter()
                .map(|n| n.id.clone())
                .collect();
            assert!(ids.iter().any(|i| i == "chat.composer"), "{ids:?}");
            assert_eq!(
                ids.iter().any(|i| i.starts_with("chat.questions")),
                *name == "approval"
            );
            golden(name, &mut h, scale)?;
            if scale == 1. && *name == "conversation" {
                let mut light = harness_in(out, 1., LIGHT)?;
                golden("conversation-light", &mut light, 1.)?;
            }
        }
    }
    Ok(())
}

#[test]
fn streaming_at_fifty_tokens_per_second_stays_inside_the_frame_budget() -> bead_test::Result<()> {
    let _guard = serial();
    let (source, chat) = offline(topic(&[("u1", "user", "Explain")], state(true, "", None)));
    let out = chat.project(&stores(&[]));
    let mut h = harness(out, 1.)?;
    let mut text = String::from("Streaming answer. ");
    let mut times = Vec::new();
    let tokens = [
        "alpha ",
        "beta ",
        "gamma ",
        "delta\n\n",
        "```\ncode line ",
        "more ",
        "\n```\n\n",
        "epsilon ",
    ];
    for i in 0..150 {
        text.push_str(tokens[i % tokens.len()]);
        source
            .publish(topic(
                &[("u1", "user", "Explain"), ("a1", "assistant", &text)],
                state(true, "", None),
            ))
            .unwrap();
        let started = std::time::Instant::now();
        let values = chat.project(&h.session.stores);
        h.update(values)?;
        h.render()?;
        times.push(started.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    let p99 = times[times.len() * 99 / 100];
    eprintln!(
        "CHAT_STREAM_FRAME_MS p50={:.2} p99={:.2} max={:.2} (CPU renderer, project+layout+render)",
        times[times.len() / 2],
        p99,
        times[times.len() - 1]
    );
    // Budget on a quiet host is 16.7 ms; the assertion leaves room for a shared CI runner.
    if !cfg!(debug_assertions) {
        assert!(p99 < 50., "p99 {p99:.2} ms");
    }
    Ok(())
}
