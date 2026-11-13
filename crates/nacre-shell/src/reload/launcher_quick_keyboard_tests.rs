// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::nacre_source::{Client, Config};
use bead::core::source::Source;
use nacre_protocol::{Capability, Message, Reply, framing};
use std::{collections::BTreeMap, path::Path, time::Duration};
use tokio::{io::BufReader, sync::mpsc};

#[tokio::test(flavor = "current_thread")]
async fn launcher_quick_enter_and_space_dispatch_the_focused_toggle() -> Result<(), Box<dyn Error>> {
    let lab = super::wifi_runtime_tests::Lab::new();
    let listener = lab.bind();
    let (calls_tx, mut calls_rx) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(reader.next().await.unwrap(), Some(Message::Hello { .. })));
        let mut methods = nacre_protocol::core_methods();
        methods.insert("notifications.set_dnd".into(), nacre_protocol::Method {
            v: 1, class: Capability::System, timeout: 10_000,
            params: serde_json::json!({"type":"object"}), result: serde_json::json!({}),
        });
        framing::write(&mut writer, &Message::Welcome {
            proto: 1, min: 1, boot: "launcher-quick-test".into(),
            topics: ["controls", "notifications", "dashboard.updates", "capabilities"]
                .into_iter().map(|topic| (topic.into(), 1)).collect(),
            methods, frame_ms: None,
        }).await.unwrap();
        loop {
            let Some(message) = reader.next().await.unwrap() else { break };
            match message {
                Message::Sub { topics } => for topic in topics {
                    let data = match topic.as_str() {
                        "notifications" => serde_json::json!({"health":{"available":true,"stale":false},"do_not_disturb":false}),
                        "controls" => serde_json::json!({"health":{"available":true,"stale":false},"night_light":{"enabled":false},"stay_awake":{"enabled":false}}),
                        _ => serde_json::json!({"health":{"available":true,"stale":false}}),
                    };
                    framing::write(&mut writer, &Message::Snap { topic, seq: 0, data }).await.unwrap();
                },
                Message::Call { id, m, p, .. } => {
                    calls_tx.send((m, p)).unwrap();
                    framing::write(&mut writer, &Message::Ret { id, reply: Reply::Ok { ok: serde_json::json!({"changed":true}) } }).await.unwrap();
                }
                Message::Ping { id } => framing::write(&mut writer, &Message::Pong { id }).await.unwrap(),
                _ => {}
            }
        }
    });
    let client = Client::start(Config::new(lab.0.join("daemon.sock"), Vec::new()))?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let h = bead_test::Harness::load(
        &root, Path::new("ui/main.luau"), &root.join("palette.json"), BTreeMap::new(),
        bead_test::Config { width: 1920, height: 1200, scale: 1. },
    )?;
    let mut runtime = Runtime::new(h.session).with_launcher(None)?
        .with_launcher_quick(crate::launcher_quick::Quick::lease(client.clone())?)?;
    runtime.state_file = None;
    for _ in 0..500 {
        runtime.stage(UiEvent::Model(BTreeMap::new()))?;
        runtime.commit();
        let count = runtime.session.view_nodes().iter()
            .filter(|node| node.action.as_deref() == Some("launcher.quick")).count();
        if count > 0 { break; }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let node = runtime.session.view_nodes().iter()
        .find(|node| node.action.as_deref() == Some("launcher.quick")
            && node.action_arg == Some(bead::core::Value::Text("actions.controls.dnd".into())))
        .expect("DND quick control is rendered").id.clone();
    runtime.session.focus_id(&node)?;
    runtime.stage(UiEvent::Command { command: "enter".into(), shift: false })?;
    runtime.commit();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), calls_rx.recv()).await?.ok_or("call channel closed")?,
        ("notifications.set_dnd".into(), serde_json::json!({"enabled":true})),
    );
    for _ in 0..500 {
        tokio::time::sleep(Duration::from_millis(10)).await;
        runtime.stage(UiEvent::Model(BTreeMap::new()))?;
        runtime.commit();
        if runtime.session.focused_action().is_some() { break; }
    }
    runtime.stage(UiEvent::Key { text: " ".into(), backspace: false })?;
    runtime.commit();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(5), calls_rx.recv()).await?.ok_or("call channel closed")?,
        ("notifications.set_dnd".into(), serde_json::json!({"enabled":true})),
    );
    drop(runtime);
    Source::stop(&client).await?;
    server.abort();
    Ok(())
}
