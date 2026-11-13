// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::nacre_source::{Client, Config};
use bead::core::source::Source;
use nacre_protocol::{Capability, Message, Reply, framing};
use std::{collections::BTreeMap, path::Path, time::Duration};
use tokio::{io::BufReader, sync::mpsc};

#[test]
fn launcher_quick_enter_and_space_dispatch_the_focused_toggle() -> Result<(), Box<dyn Error>> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let _guard = rt.enter();
    let lab = super::wifi_runtime_tests::Lab::new();
    let listener = lab.bind();
    let (calls_tx, mut calls_rx) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(
            reader.next().await.unwrap(),
            Some(Message::Hello { .. })
        ));
        let mut methods = nacre_protocol::core_methods();
        methods.insert(
            "notifications.set_dnd".into(),
            nacre_protocol::Method {
                v: 1,
                class: Capability::System,
                timeout: 10_000,
                params: serde_json::json!({"type":"object"}),
                result: serde_json::json!({}),
            },
        );
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "launcher-quick-test".into(),
                topics: [
                    "controls",
                    "notifications",
                    "dashboard.updates",
                    "capabilities",
                ]
                .into_iter()
                .map(|topic| (topic.into(), 1))
                .collect(),
                methods,
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        let mut sequence = 0;
        loop {
            let Some(message) = reader.next().await.unwrap() else {
                break;
            };
            match message {
                Message::Sub { topics } => {
                    for topic in topics {
                        let data = match topic.as_str() {
                            "notifications" => {
                                serde_json::json!({"health":{"available":true,"stale":false},"do_not_disturb":false})
                            }
                            "controls" => {
                                serde_json::json!({"health":{"available":true,"stale":false},"night_light":{"enabled":false},"stay_awake":{"enabled":false}})
                            }
                            _ => serde_json::json!({"health":{"available":true,"stale":false}}),
                        };
                        framing::write(
                            &mut writer,
                            &Message::Snap {
                                topic,
                                seq: 0,
                                data,
                            },
                        )
                        .await
                        .unwrap();
                    }
                }
                Message::Call { id, m, p, .. } => {
                    let enabled = p["enabled"].clone();
                    calls_tx.send((m, p)).unwrap();
                    framing::write(
                        &mut writer,
                        &Message::Ret {
                            id,
                            reply: Reply::Ok {
                                ok: serde_json::json!({"changed":true}),
                            },
                        },
                    )
                    .await
                    .unwrap();
                    sequence += 1;
                    framing::write(&mut writer, &Message::Snap { topic: "notifications".into(), seq: sequence,
                        data: serde_json::json!({"health":{"available":true,"stale":false},"do_not_disturb":enabled}) }).await.unwrap();
                }
                Message::Ping { id } => framing::write(&mut writer, &Message::Pong { id })
                    .await
                    .unwrap(),
                _ => {}
            }
        }
    });
    let client = Client::start(Config::new(lab.0.join("daemon.sock"), Vec::new()))?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let h = bead_test::Harness::load(
        &root,
        Path::new("ui/main.luau"),
        &root.join("palette.json"),
        BTreeMap::new(),
        bead_test::Config {
            width: 1920,
            height: 1200,
            scale: 1.,
        },
    )?;
    let mut runtime = Runtime::new(h.session)
        .with_launcher(None)?
        .with_launcher_quick(crate::launcher_quick::Quick::lease(client.clone())?)?;
    runtime.isolate_state(&lab.0.join("ui-health.json"));
    for _ in 0..500 {
        runtime.stage(UiEvent::Model(BTreeMap::new()))?;
        runtime.commit();
        let count = runtime
            .session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("launcher.quick"))
            .count();
        if count > 0 {
            break;
        }
        rt.block_on(tokio::time::sleep(Duration::from_millis(10)));
    }
    let node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| {
            node.action.as_deref() == Some("launcher.quick")
                && node.action_arg == Some(bead::core::Value::Text("actions.controls.dnd".into()))
        })
        .expect("DND quick control is rendered")
        .id
        .clone();
    if let Some(scene) = runtime.session.focus_id(&node)? {
        runtime.session.commit(scene);
    }
    runtime.stage(UiEvent::Command {
        command: "enter".into(),
        shift: false,
    })?;
    runtime.commit();
    assert_eq!(
        runtime.session.focused_action().unwrap().1,
        "launcher.quick",
        "pending toggle retains focus"
    );
    assert_eq!(
        rt.block_on(tokio::time::timeout(
            Duration::from_secs(5),
            calls_rx.recv()
        ))?
        .ok_or("call channel closed")?,
        (
            "notifications.set_dnd".into(),
            serde_json::json!({"enabled":true})
        ),
    );
    for _ in 0..500 {
        rt.block_on(tokio::time::sleep(Duration::from_millis(10)));
        runtime.stage(UiEvent::Model(BTreeMap::new()))?;
        runtime.commit();
        if runtime.session.focused_action().is_some()
            && matches!(runtime.session.stores.get("launcher.quick"), Some(bead::core::Value::List(rows)) if rows[0]["on"] == true)
        {
            break;
        }
    }
    runtime.stage(UiEvent::Key {
        text: " ".into(),
        backspace: false,
    })?;
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("launcher.search.text"),
        Some(&bead::core::Value::Text(String::new()))
    );
    assert_eq!(
        rt.block_on(tokio::time::timeout(
            Duration::from_secs(5),
            calls_rx.recv()
        ))?
        .ok_or("call channel closed")?,
        (
            "notifications.set_dnd".into(),
            serde_json::json!({"enabled":false})
        ),
    );
    drop(runtime);
    rt.block_on(Source::stop(&client))?;
    server.abort();
    Ok(())
}
