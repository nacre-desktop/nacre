// SPDX-License-Identifier: GPL-3.0-only
//! N0248 Nacre AI app: each rail page's model over the N0319 contract fixtures (never live
//! data), the host action's guards, and the window goldens for Chats and Usage at 1x dark.
use super::*;
use bead_test::{Config as HarnessConfig, Harness, source::FakeSource};
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
/// The data of the last frame for `topic` (snap or patch) or of the reply to call `id`.
fn fixture(file: &str, pick: impl Fn(&Json) -> bool) -> Json {
    file.lines()
        .map(|l| serde_json::from_str::<Json>(l).unwrap())
        .rev()
        .find(pick)
        .map(|frame| {
            if frame["t"] == "ret" {
                frame["ok"].clone()
            } else {
                frame["data"].clone()
            }
        })
        .unwrap()
}
const APP: &str = include_str!("../../../protocol/fixtures/ai-app.jsonl");
const USAGE: &str = include_str!("../../../protocol/fixtures/dashboard-ai-usage.jsonl");
fn topic(name: &str) -> Json {
    fixture(APP, |f| f["topic"] == name)
}
fn chats_reply() -> Json {
    fixture(APP, |f| f["t"] == "ret" && f["id"] == 3)
}
fn usage() -> Json {
    fixture(USAGE, |f| f["topic"] == "ai.usage" && f["seq"] == 1)
}
/// Two and a half minutes after the fixture's running chat was last active.
fn now() -> u64 {
    1_800_000_150_000
}
fn app() -> AiApp {
    let source = |data: Json| Arc::new(FakeSource::new(data).unwrap()) as Arc<dyn Source>;
    AiApp::new(None).with_fixture(
        vec![
            ("ai.providers", source(topic("ai.providers"))),
            ("ai.projects", source(topic("ai.projects"))),
            ("ai.accounts", source(topic("ai.accounts"))),
            ("ai.usage", source(usage())),
        ],
        Some(chats_reply()),
        now,
    )
}
fn stores(values: &[(&str, &str)]) -> Stores {
    let map: BTreeMap<String, Value> = [
        ("ai.page", "chats"),
        ("ai.provider", ""),
        ("ai.project", ""),
    ]
    .iter()
    .chain(values)
    .map(|(k, v)| (k.to_string(), text(v)))
    .collect();
    Stores::default().reconcile(&map).unwrap()
}
fn list(model: &BTreeMap<String, Value>, key: &str) -> Vec<Json> {
    match &model[key] {
        Value::List(items) => items.to_vec(),
        other => panic!("{key}: {other:?}"),
    }
}

#[test]
fn providers_come_from_the_registry_in_order_with_one_chosen() {
    let rows = providers(&topic("ai.providers"), "");
    let ids: Vec<_> = rows.iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert_eq!(ids, ["codex", "claude"]);
    assert_eq!(rows[0]["selected"], true);
    assert_eq!(rows[1]["label"], "Claude Code");
    let rows = providers(&topic("ai.providers"), "claude");
    assert_eq!(
        (rows[0]["selected"].clone(), rows[1]["selected"].clone()),
        (json!(false), json!(true))
    );
    // An unknown or invalid stored id falls back to the first; a bad registry shows nothing.
    assert_eq!(
        providers(&topic("ai.providers"), "nope")[0]["selected"],
        true
    );
    assert!(
        providers(
            &json!({"providers":[{"id":"Bad Id","label":"x","kind":"cli","capabilities":{}}]}),
            ""
        )
        .is_empty()
    );
}

#[test]
fn projects_rows_name_the_host_and_keep_the_choice() {
    let rows = projects(&topic("ai.projects"), "fixture");
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[1]["selected"], true);
    assert_eq!(rows[0]["selected"], false);
    assert_eq!(rows[2]["path"], "dev-1:/home/dev/remote");
    assert_eq!(rows[2]["remote"], true);
    assert_eq!(projects(&topic("ai.projects"), "")[0]["selected"], true);
}

#[test]
fn accounts_group_under_their_provider_with_a_status() {
    crate::i18n::init();
    let rows = accounts(
        &topic("ai.accounts"),
        &providers(&topic("ai.providers"), ""),
    );
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["group"], "Codex");
    assert_eq!(rows[0]["status"], "Signed in");
    assert_eq!(rows[0]["tone"], "success");
    assert_eq!(rows[1]["group"], "Claude Code");
    assert_eq!(rows[1]["status"], "Not installed");
    assert_eq!(rows[1]["active"], true);
}

#[test]
fn chats_group_running_first_and_name_the_provider() {
    crate::i18n::init();
    let rows = chat_rows(&chats_reply(), now());
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["group"], "Running");
    assert_eq!(rows[0]["running"], true);
    assert_eq!(rows[0]["provider"], "codex");
    assert_eq!(rows[0]["age"], "2m");
    assert_eq!(rows[1]["group"], "Today");
    assert_eq!(rows[1]["project"], "dev-1:remote");
    assert_eq!(rows[1]["resumable"], false);
    // A day later the saved chat moves to Earlier.
    let later = chat_rows(&chats_reply(), now() + DAY_MS);
    assert_eq!(later[1]["group"], "Earlier");
    assert_eq!(later[1]["provider"], "claude");
}

#[test]
fn an_open_chat_waiting_for_input_is_running_with_a_waiting_badge() {
    crate::i18n::init();
    let mut reply = chats_reply();
    let chats = reply["chats"].as_array_mut().unwrap();
    assert!(chats[1].get("session").is_none_or(Json::is_null));
    chats[1]["session"] = json!("ai-general-chat-claude-default-1");
    let rows = chat_rows(&reply, now() + DAY_MS);
    assert_eq!(rows.len(), 2);
    assert!(rows.iter().all(|row| row["running"] == true));
    assert_eq!(rows[0]["group"], "Running");
    assert_eq!(rows[1]["group"], "");
    assert_eq!(rows[0]["working"], true);
    assert_eq!(rows[1]["working"], false);
}

#[test]
fn usage_rows_are_rings_for_windows_and_lines_otherwise() {
    crate::i18n::init();
    let state: AiUsageState = serde_json::from_value(usage()).unwrap();
    let rows = usage_rows(&state, 1_800_000_000_000);
    assert_eq!(rows[0]["percent"], json!(12.0));
    assert_eq!(rows[0]["value"], "12%");
    assert_eq!(rows[0]["detail"], "Resets in 1h");
    assert_eq!(rows[1]["head"], true);
    assert_eq!(rows[2]["label"], "5 hour: 3% used · resets Mon 11:00");
    assert!(rows[2].get("percent").is_none());
}

#[test]
fn the_projection_fills_every_page_and_follows_the_stored_choice() {
    crate::i18n::init();
    let app = app();
    let model = app.project(&stores(&[]));
    for key in [
        "ai.providers",
        "ai.projects",
        "ai.accounts",
        "ai.usage",
        "ai.chats",
    ] {
        assert!(!list(&model, key).is_empty(), "{key}");
    }
    assert_eq!(model["ai.connected"], Value::Bool(true));
    let model = app.project(&stores(&[
        ("ai.provider", "claude"),
        ("ai.project", "remote"),
    ]));
    let selected = |key: &str, field: &str| {
        list(&model, key)
            .into_iter()
            .find(|r| r["selected"] == true)
            .map(|r| r[field].clone())
    };
    assert_eq!(selected("ai.providers", "id"), Some(json!("claude")));
    assert_eq!(
        selected("ai.projects", "path"),
        Some(json!("dev-1:/home/dev/remote"))
    );
    // Without a daemon the window says so instead of showing stale rows.
    let offline = AiApp::new(None).project(&stores(&[]));
    assert_eq!(offline["ai.connected"], Value::Bool(false));
    assert!(list(&offline, "ai.chats").is_empty());
}

#[test]
fn actions_without_a_daemon_change_nothing() {
    let app = app();
    for arg in [
        "start",
        "resume:chat0000000000000001",
        "resume:",
        "refresh:usage",
        "bogus",
    ] {
        app.act(arg, &stores(&[]));
    }
    assert!(app.calls.borrow().pending.is_empty());
}

#[test]
fn a_success_clears_the_note_and_a_failed_list_is_asked_again() {
    let mut calls = Calls::default();
    calls.settle(Err(source::Error::new("failed", "resume failed", false)));
    assert_eq!(calls.message, "resume failed");
    calls.settle(Ok(json!({})));
    assert!(calls.message.is_empty());
    // A failed first list keeps `asked`; the refresh button marks the list stale to ask again.
    calls.asked = true;
    calls.settle(Err(source::Error::new("failed", "list failed", false)));
    assert!(calls.listed.is_none() && !calls.stale);
    let app = app();
    app.act("refresh", &stores(&[]));
    assert!(app.calls.borrow().stale);
}

#[test]
fn a_new_daemon_boot_lists_the_chats_again() {
    let mut calls = Calls::default();
    calls.see_boot(Some("a".into()));
    assert!(!calls.stale, "the first boot seen is not a restart");
    calls.see_boot(Some("a".into()));
    assert!(!calls.stale);
    calls.see_boot(Some("b".into()));
    assert!(calls.stale);
}

// ---- goldens ---------------------------------------------------------------------------------

fn golden(name: &str, page: &str) {
    crate::i18n::init();
    let mut model = app().project(&stores(&[("ai.page", page)]));
    model.insert("ai.page".into(), text(page));
    let mut h = Harness::load_with_icons(
        &root(),
        Path::new("ai/main.luau"),
        &root().join("palette.json"),
        model,
        HarnessConfig {
            width: 1120,
            height: 760,
            scale: 1.,
        },
        BTreeMap::new(),
    )
    .unwrap();
    let frame = h.pose((0., 0.), 1.).unwrap();
    let path = root().join(format!("tests/goldens/{name}.png"));
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&path).unwrap();
    }
    frame
        .compare(
            &path,
            Default::default(),
            &std::env::temp_dir().join(format!("bead-{name}-diff")),
        )
        .unwrap_or_else(|error| panic!("{name}: {error}"));
}

#[test]
fn ai_app_window_goldens() {
    golden("ai-app-chats-1", "chats");
    golden("ai-app-usage-1", "usage");
}
