// SPDX-License-Identifier: GPL-3.0-only
//! B0045: Settings window rows, store read/write over a fake daemon, goldens, lifecycle.
use super::settings::*;
use crate::nacre_source::{CallWake, Client, Config};
use bead::core::{Stores, Value, source::Source};
use bead_test::{Config as HarnessConfig, Harness, source::FakeSource};
use serde_json::{Value as Json, json};
use std::{collections::BTreeMap, path::Path, sync::Arc, time::Duration};

fn root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn stores(page: &str, search: &str) -> Stores {
    Stores::default()
        .reconcile(&BTreeMap::from([
            ("settings.page".into(), Value::Text(page.into())),
            ("settings.search".into(), Value::Text(search.into())),
        ]))
        .unwrap()
}
fn doc(store: &str, rev: u64, data: Json) -> Json {
    json!({"store": store, "rev": rev, "exists": true, "data": data, "dirty": false})
}
fn fixture(desktop: Json) -> (Settings, Arc<FakeSource>) {
    fixture_with(desktop, &[])
}
/// The desktop store plus other fixed topics (by topic name).
fn fixture_with(desktop: Json, topics: &[(&str, Json)]) -> (Settings, Arc<FakeSource>) {
    let source = Arc::new(FakeSource::new(doc("desktop", 1, desktop)).unwrap());
    let mut sources: BTreeMap<String, Arc<dyn Source>> = BTreeMap::from([(
        "settings.store.desktop".to_string(),
        source.clone() as Arc<dyn Source>,
    )]);
    for (name, data) in topics {
        sources.insert(
            (*name).into(),
            Arc::new(FakeSource::new(data.clone()).unwrap()),
        );
    }
    (
        Settings::new(None, &root()).unwrap().with_sources(sources),
        source,
    )
}
/// The projected rows as a flat list: each section is followed by the rows it carries (B0067 nests them).
fn items(model: &BTreeMap<String, Value>) -> Vec<Json> {
    let mut out = Vec::new();
    for row in nested_items(model) {
        let inner = row["rows"].as_array().cloned().unwrap_or_default();
        out.push(row);
        out.extend(inner);
    }
    out
}
fn nested_items(model: &BTreeMap<String, Value>) -> Vec<Json> {
    match &model["settings.items"] {
        Value::List(rows) => rows.to_vec(),
        other => panic!("{other:?}"),
    }
}
fn row<'a>(rows: &'a [Json], id: &str) -> &'a Json {
    rows.iter()
        .find(|r| r["id"] == id)
        .unwrap_or_else(|| panic!("{id} missing"))
}
fn wake() -> CallWake {
    Arc::new(|| {})
}

#[test]
fn registry_loads_and_every_navigation_page_has_a_label_and_a_group() {
    let (settings, _) = fixture(json!({}));
    let model = settings.project(&stores("", ""));
    let Value::List(nav) = &model["settings.nav"] else {
        panic!()
    };
    let pages = nav.iter().filter(|n| n["kind"] == "page").count();
    let groups = nav.iter().filter(|n| n["kind"] == "group").count();
    assert_eq!((groups, pages), (5, 21), "five groups, 21 top-level pages");
    assert!(nav.iter().all(|n| {
        !n["label"]
            .as_str()
            .unwrap()
            .starts_with("settings.registry.")
    }));
    // The default page is the first Bead can edit.
    assert_eq!(model["settings.page"], Value::Text("appearance".into()));
    assert_eq!(model["settings.title"], Value::Text("Appearance".into()));
}

#[test]
fn every_settings_page_renders_without_visible_translation_keys() {
    let untranslated = |text: &str| {
        ["ui.", "bead.", "helper.", "settings."]
            .iter()
            .any(|prefix| {
                text.strip_prefix(prefix).is_some_and(|tail| {
                    !tail.is_empty()
                        && tail.bytes().all(|byte| {
                            byte.is_ascii_lowercase()
                                || byte.is_ascii_digit()
                                || b".-".contains(&byte)
                        })
                })
            })
    };
    let (navigator, _) = fixture(json!({}));
    let nav = navigator.project(&stores("", ""));
    let Value::List(pages) = &nav["settings.nav"] else {
        panic!()
    };
    for page in pages.iter().filter(|page| page["kind"] == "page") {
        let id = page["id"].as_str().unwrap();
        let (settings, _) = fixture(json!({}));
        let mut model = settings.project(&stores(id, ""));
        model.insert("settings.writable".into(), Value::Bool(true));
        let mut view = harness(model, 1280, 1000, 1.);
        view.render().unwrap();
        let visible: Vec<String> = view
            .scene()
            .primitives
            .iter()
            .filter_map(|primitive| {
                if let bead::render::Primitive::Text { text, .. } = primitive {
                    Some(text.to_string())
                } else {
                    None
                }
            })
            .collect();
        assert!(
            visible.iter().all(|text| !untranslated(text)),
            "{id}: {visible:?}"
        );
    }
}

#[test]
fn capability_gates_collection_wallpapers_and_niri_workspace_preferences() {
    for (flag, present) in [(false, false), (true, true)] {
        let (settings, _) = fixture_with(
            json!({}),
            &[(
                "capabilities",
                json!({"flags": {
                    "workspaceCollections": flag,
                    "niriWorkspaces": flag
                }}),
            )],
        );
        let wallpaper = items(&settings.project(&stores("wallpaper", "")));
        for id in [
            "section:wallpaper.collection",
            "wallpaper.collection.collectionWallpapers",
        ] {
            assert_eq!(wallpaper.iter().any(|row| row["id"] == id), present, "{id}");
        }
        // The wallpaper folder is not a desk collection: every compositor keeps it.
        assert!(
            wallpaper
                .iter()
                .any(|row| row["id"] == "wallpaper.collection.folder")
        );
        let workspaces = items(&settings.project(&stores("windows-workspaces", "")));
        assert_eq!(
            workspaces
                .iter()
                .any(|row| row["id"] == "windows-workspaces.workspaces.workspaceCount"),
            present
        );
    }
}

#[test]
fn power_controls_follow_their_backend_capabilities() {
    for (capable, expected) in [(false, false), (true, true)] {
        let (settings, _) = fixture_with(
            json!({}),
            &[(
                "capabilities",
                json!({"flags": {
                    "battery": true,
                    "batterySaver": capable,
                    "powerIdle": capable,
                    "powerHardware": capable
                }}),
            )],
        );
        let rows = items(&settings.project(&stores("power", "")));
        for id in [
            "power.saver.batterySaver",
            "power.idle.dim",
            "power.idle.sleep",
            "power.hardware.lid",
            "power.hardware.button",
        ] {
            assert_eq!(rows.iter().any(|row| row["id"] == id), expected, "{id}");
        }
    }
    // Desktop sessions still need idle and hardware controls without a battery.
    let (settings, _) = fixture_with(
        json!({}),
        &[(
            "capabilities",
            json!({"flags": {
                "battery": false, "powerIdle": true, "powerHardware": true
            }}),
        )],
    );
    let rows = items(&settings.project(&stores("power", "")));
    assert!(rows.iter().any(|row| row["id"] == "power.idle.dim"));
    assert!(rows.iter().any(|row| row["id"] == "power.hardware.button"));
    assert!(
        !rows
            .iter()
            .any(|row| row["id"] == "power.saver.batterySaver")
    );
}

#[test]
fn appearance_rows_follow_the_registry_types_and_the_store_with_nacre_defaults() {
    let (settings, _) =
        fixture(json!({"textSize": 14, "frameSheen": false, "monoFont": "Fira Code"}));
    let model = settings.project(&stores("appearance", ""));
    let rows = items(&model);
    assert_eq!(row(&rows, "appearance.fonts.textSize")["kind"], "number");
    assert_eq!(row(&rows, "appearance.fonts.textSize")["number"], 14);
    assert_eq!(row(&rows, "appearance.fonts.textSize")["unit"], "pt");
    assert_eq!(row(&rows, "appearance.frame.frameSheen")["checked"], false);
    // Unset keys read the registry default, like QML.
    assert_eq!(row(&rows, "appearance.frame.frameWidth")["number"], 10);
    assert_eq!(row(&rows, "appearance.motion.animations")["checked"], true);
    assert_eq!(row(&rows, "appearance.fonts.monoFont")["text"], "Fira Code");
    assert_eq!(
        row(&rows, "appearance.fonts.interfaceFont")["text"],
        "Default"
    );
    assert_eq!(rows.iter().filter(|r| r["kind"] == "section").count(), 3);
    // No daemon: visible but not writable.
    assert_eq!(model["settings.writable"], Value::Bool(false));
}

#[test]
fn shared_scroll_setting_defaults_and_preserves_a_saved_value() {
    for (data, expected) in [(json!({}), 0.35), (json!({"scrollSpeed": 1.25}), 1.25)] {
        let (settings, _) = fixture(data);
        let rows = items(&settings.project(&stores("keyboard-mouse", "")));
        let speed = row(&rows, "keyboard-mouse.scrolling.scrollSpeed");
        assert_eq!(speed["kind"], "number");
        assert_eq!(speed["number"], expected);
        assert_eq!(speed["label"], "Touchpad scroll speed");
    }
    for (data, expected) in [(json!({}), 1.5), (json!({"mouseScrollSpeed": 2.25}), 2.25)] {
        let (settings, _) = fixture(data);
        let rows = items(&settings.project(&stores("keyboard-mouse", "")));
        let speed = row(&rows, "keyboard-mouse.scrolling.mouseScrollSpeed");
        assert_eq!(speed["kind"], "number");
        assert_eq!(speed["number"], expected);
        assert_eq!(speed["label"], "Mouse wheel scroll speed");
    }
}

#[test]
fn mouse_scroll_control_is_visible_without_a_touchpad() {
    let (settings, _) = fixture_with(
        json!({}),
        &[(
            "capabilities",
            json!({"flags":{"input.controls":true,"input.touchpadSpeed":true}}),
        )],
    );
    let rows = items(&settings.project(&stores("keyboard-mouse", "")));
    assert!(
        rows.iter()
            .any(|row| row["id"] == "section:keyboard-mouse.scrolling")
    );
    assert!(
        rows.iter()
            .any(|row| row["id"] == "keyboard-mouse.scrolling.mouseScrollSpeed")
    );
    assert!(
        !rows
            .iter()
            .any(|row| row["id"] == "keyboard-mouse.scrolling.scrollSpeed")
    );
}

#[test]
fn notifications_page_and_search_use_the_same_registry_data() {
    let (settings, _) = fixture(json!({"dnd": true}));
    let rows = items(&settings.project(&stores("notifications", "")));
    assert_eq!(row(&rows, "notifications.delivery.dnd")["checked"], true);
    let found = items(&settings.project(&stores("appearance", "kitty")));
    assert!(
        found
            .iter()
            .any(|r| r["id"] == "setting:appearance.fonts.monoFont")
    );
    assert!(found.iter().all(|r| r["kind"] == "link"));
    let none = settings.project(&stores("appearance", "zzzzqqq"));
    assert_eq!(none["settings.empty"], Value::Bool(true));
    // Every registry page is editable now (Displays since B0089); without a daemon none is writable.
    let displays = settings.project(&stores("displays", ""));
    assert_eq!(displays["settings.readonly"], Value::Bool(false));
    assert_eq!(displays["settings.writable"], Value::Bool(false));
}

const DESKTOP: &str = "settings.store.desktop";
const APPS: &str = "settings.store.apps";
const PROFILE: &str = "settings.store.profile";
const WALLPAPER_PICKER: &str = "settings.store.wallpaper-picker";

/// A real client against a fake daemon: reads the store, writes with the revision
/// fence, and follows an outside edit.
#[tokio::test(flavor = "current_thread")]
async fn store_read_write_round_trip_over_the_wire_and_live_updates() {
    use nacre_protocol::{Capability, Message, Reply, framing};
    use std::os::unix::fs::PermissionsExt;
    use tokio::{io::BufReader, net::UnixListener, sync::mpsc};
    let dir = std::env::temp_dir().join(format!("settings-wire-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (calls_tx, mut calls) = mpsc::unbounded_channel();
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(
            reader.next().await.unwrap().unwrap(),
            Message::Hello { .. }
        ));
        let mut methods = nacre_protocol::core_methods();
        methods.insert(
            METHOD.into(),
            nacre_protocol::Method {
                v: 1,
                class: Capability::System,
                timeout: 10_000,
                params: json!({"type":"object"}),
                result: json!({}),
            },
        );
        let status_methods = methods.clone();
        let welcome = Message::Welcome {
            proto: 1,
            min: 1,
            boot: "settings-boot".into(),
            topics: [
                (DESKTOP.to_string(), 1),
                (APPS.to_string(), 1),
                (PROFILE.to_string(), 1),
                (WALLPAPER_PICKER.to_string(), 1),
            ]
            .into(),
            methods,
            frame_ms: None,
        };
        framing::write(&mut writer, &welcome).await.unwrap();
        let mut seq = 0;
        let mut docs = BTreeMap::from([
            (
                "desktop".to_owned(),
                doc("desktop", 3, json!({"dnd":false})),
            ),
            (
                "apps".to_owned(),
                doc("apps", 3, json!({"browser":["a","b","c"]})),
            ),
            (
                "profile".to_owned(),
                doc("profile", 3, json!({"startup":[]})),
            ),
            (
                "wallpaper-picker".to_owned(),
                doc("wallpaper-picker", 3, json!({"lockWallpaper":""})),
            ),
        ]);
        while let Ok(Some(message)) = reader.next().await {
            match message {
                Message::Sub { topics } => {
                    for topic in topics {
                        let store = topic.strip_prefix("settings.store.").unwrap();
                        let data = docs.get(store).unwrap().clone();
                        let snap = Message::Snap { topic, seq, data };
                        framing::write(&mut writer, &snap).await.unwrap();
                    }
                }
                Message::Call { id, m, p, .. } => {
                    if m == "daemon.status" {
                        let reply = Reply::Ok {
                            ok: json!({
                                "boot":"settings-boot",
                                "topics":[DESKTOP, APPS, PROFILE, WALLPAPER_PICKER],
                                "methods":status_methods,
                            }),
                        };
                        framing::write(&mut writer, &Message::Ret { id, reply })
                            .await
                            .unwrap();
                        continue;
                    }
                    assert_eq!(m, METHOD);
                    calls_tx.send(p.clone()).unwrap();
                    let store = p["store"].as_str().unwrap();
                    let key = p["key"][0].as_str().unwrap();
                    let document = docs.get_mut(store).unwrap();
                    document["data"][key] = p["value"].clone();
                    document["rev"] = json!(4);
                    let reply = Reply::Ok {
                        ok: json!({"rev": 4}),
                    };
                    framing::write(&mut writer, &Message::Ret { id, reply })
                        .await
                        .unwrap();
                    seq += 1;
                    let snap = Message::Snap {
                        topic: format!("settings.store.{store}"),
                        seq,
                        data: document.clone(),
                    };
                    framing::write(&mut writer, &snap).await.unwrap();
                }
                _ => {}
            }
        }
    });
    let client = Client::start(Config::new(
        socket,
        vec![
            DESKTOP.into(),
            APPS.into(),
            PROFILE.into(),
            WALLPAPER_PICKER.into(),
        ],
    ))
    .unwrap();
    for _ in 0..500 {
        if client.snapshot().connected && client.status().boot.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let settings = Settings::new(Some(client.clone()), &root())
        .unwrap()
        .with_startup_choices(&[
            (
                "Browser",
                json!({"label":"Browser","workspace":1,"argv":["browser"],"classes":["Browser"]}),
            ),
            (
                "Files",
                json!({"label":"Files","workspace":1,"argv":["files"],"classes":["Files"]}),
            ),
        ])
        .with_lock_wallpapers(&[("/wallpapers/lock.png", "Lock scene")]);
    let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = woken.clone();
    settings.subscribe(Arc::new(move || {
        counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    }));
    let at = stores("notifications", "");
    let mut model = settings.project(&at);
    for _ in 0..500 {
        model = settings.project(&at);
        if model["settings.writable"] == Value::Bool(true) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(model["settings.writable"], Value::Bool(true));
    assert_eq!(
        row(&items(&model), "notifications.delivery.dnd")["checked"],
        false
    );
    // Write: exactly the published call shape, fenced by the revision read from the topic.
    let before = woken.load(std::sync::atomic::Ordering::SeqCst);
    settings.act("set:notifications.delivery.dnd", "", wake());
    let params = calls.recv().await.unwrap();
    assert_eq!(
        params,
        json!({"store":"desktop","key":["dnd"],"value":true,"rev":3})
    );
    // The daemon accepts it and republishes; the row follows live.
    for _ in 0..500 {
        model = settings.project(&at);
        if row(&items(&model), "notifications.delivery.dnd")["checked"] == true {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(
        row(&items(&model), "notifications.delivery.dnd")["checked"],
        true
    );
    assert!(
        woken.load(std::sync::atomic::Ordering::SeqCst) > before,
        "store change woke the window"
    );
    assert_eq!(model["settings.message"], Value::Text(String::new()));
    // Rows the daemon has no contract for (a helper adapter) never send a call.
    settings.act("set:keyboard-mouse.keyboard.keyboardLayouts", "x", wake());
    assert!(calls.try_recv().is_err());
    // B0058: a reorderable list sends the whole swapped array; the first entry cannot move up.
    let apps = stores("apps", "");
    for _ in 0..500 {
        let model = settings.project(&apps);
        if row(&items(&model), "apps.defaults.browser")["items"]
            .as_array()
            .is_some_and(|i| i.len() == 3)
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    let browser = row(&items(&settings.project(&apps)), "apps.defaults.browser").clone();
    assert_eq!(browser["reorder"], true);
    settings.act("set:apps.defaults.browser|up:0", "", wake());
    assert!(
        calls.try_recv().is_err(),
        "the first entry has nowhere to go"
    );
    settings.act("set:apps.defaults.browser|down:0", "", wake());
    assert_eq!(
        calls.recv().await.unwrap(),
        json!({"store":"apps","key":["browser"],"value":["b","a","c"],"rev":3})
    );
    // The lock picker writes only a catalogue ID to wallpaper-picker and can restore the
    // empty value that means “follow the current desktop wallpaper”.
    let wallpaper_page = stores("wallpaper", "");
    for _ in 0..500 {
        let model = settings.project(&wallpaper_page);
        if model["settings.writable"] == Value::Bool(true) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    settings.act("do:lock-wallpaper|/wallpapers/lock.png", "", wake());
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), calls.recv())
            .await
            .unwrap()
            .unwrap(),
        json!({"store":"wallpaper-picker","key":["lockWallpaper"],"value":"/wallpapers/lock.png","rev":3})
    );
    for _ in 0..10 {
        settings.project(&wallpaper_page);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    settings.act("do:lock-wallpaper|", "", wake());
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), calls.recv())
            .await
            .unwrap()
            .unwrap(),
        json!({"store":"wallpaper-picker","key":["lockWallpaper"],"value":"","rev":4})
    );
    for _ in 0..10 {
        settings.project(&wallpaper_page);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    settings.act(
        "do:list-add|panels/top-bar.panel.modules|launcher",
        "",
        wake(),
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), calls.recv())
            .await
            .unwrap()
            .unwrap(),
        json!({"store":"desktop","key":["modules"],"value":["launcher"],"rev":4})
    );
    for _ in 0..10 {
        settings.project(&stores("panels/top-bar", ""));
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let startup_page = stores("apps", "");
    settings.project(&startup_page);
    settings.act("do:list-add|apps.startup.startup|g0i0", "", wake());
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), calls.recv())
            .await
            .unwrap()
            .unwrap(),
        json!({"store":"profile","key":["startup"],"value":[{"label":"Browser","workspace":1,"argv":["browser"],"classes":["Browser"]}],"rev":3})
    );
    for _ in 0..10 {
        settings.project(&startup_page);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    settings.act("do:list-add|apps.startup.startup|g0i1", "", wake());
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), calls.recv())
            .await
            .unwrap()
            .unwrap(),
        json!({"store":"profile","key":["startup"],"value":[
            {"label":"Browser","workspace":1,"argv":["browser"],"classes":["Browser"]},
            {"label":"Files","workspace":1,"argv":["files"],"classes":["Files"]}
        ],"rev":4})
    );
    for _ in 0..10 {
        settings.project(&startup_page);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    settings.act("set:apps.startup.startup|down:0", "", wake());
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), calls.recv())
            .await
            .unwrap()
            .unwrap(),
        json!({"store":"profile","key":["startup"],"value":[
            {"label":"Files","workspace":1,"argv":["files"],"classes":["Files"]},
            {"label":"Browser","workspace":1,"argv":["browser"],"classes":["Browser"]}
        ],"rev":4})
    );
    for _ in 0..10 {
        settings.project(&startup_page);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    settings.act("set:apps.startup.startup|rm:0", "", wake());
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), calls.recv())
            .await
            .unwrap()
            .unwrap(),
        json!({"store":"profile","key":["startup"],"value":[{"label":"Browser","workspace":1,"argv":["browser"],"classes":["Browser"]}],"rev":4})
    );
    // The window is closed: leases and subscriptions go with it.
    drop(settings);
    client.stop().await.unwrap();
    server.abort();
    std::fs::remove_dir_all(dir).ok();
}

#[test]
fn values_are_typed_clamped_and_staged_before_they_reach_the_wire() {
    let (settings, _) = fixture(json!({"textSize": 14}));
    // Without a daemon nothing is ever submitted, and the message says why.
    let at = stores("appearance", "");
    settings.project(&at);
    settings.act("set:appearance.fonts.textSize|99", "", wake());
    let model = settings.project(&at);
    assert_eq!(
        model["settings.message"],
        Value::Text("Settings cannot be changed right now.".into())
    );
}

/// B0058 slice 1: pages whose controls only write the settings store are editable and typed
/// from the registry; controls the daemon cannot honour yet stay read-only text.
#[test]
fn panels_windows_date_and_lock_pages_follow_the_registry() {
    let (settings, _) =
        fixture(json!({"topBarEnabled": false, "frameWidth": 12, "language": "nb"}));
    let panels = items(&settings.project(&stores("panels", "")));
    let lip_size = row(&panels, "panels.lips.lipSize");
    assert_eq!(lip_size["kind"], "number");
    assert_eq!(lip_size["number"], 48);
    assert_eq!(lip_size["unit"], "px");
    assert_eq!(
        row(&panels, "panels.lips.detachablePanels")["kind"],
        "toggle"
    );
    assert_eq!(row(&panels, "panels.lips.hoverDelay")["kind"], "number");
    // `backend: planned` settings are hidden, like QML outside development.
    assert!(
        panels
            .iter()
            .any(|r| r["id"] == "link:panels/top-bar" && r["kind"] == "link")
    );
    let top = items(&settings.project(&stores("panels/top-bar", "")));
    assert_eq!(row(&top, "panels/top-bar.panel.modules")["kind"], "list");
    assert_eq!(
        row(&top, "panels/top-bar.panel.modules")["choice_editor"],
        true
    );
    assert_eq!(row(&top, "panels/top-bar.panel.enabled")["checked"], false);
    assert_eq!(row(&top, "panels/top-bar.panel.enabled")["enabled"], true);
    assert!(
        top.iter()
            .any(|r| r["id"] == "panels/top-bar.panel.clockFormat")
    );
    assert!(top.iter().any(|r| r["id"] == "panels/top-bar.panel.tray"));
    let win = items(&settings.project(&stores("windows-workspaces", "")));
    assert!(
        win.iter()
            .any(|r| r["id"] == "windows-workspaces.workspaces.workspaceCount")
    );
    assert!(
        win.iter()
            .any(|r| r["id"] == "link:windows-workspaces/workflows")
    );
    let dt = items(&settings.project(&stores("date-time-language", "")));
    assert_eq!(
        row(&dt, "date-time-language.locale.language")["kind"],
        "choice"
    );
    // Lock and login (B0063): the four lock options are plain desktop store rows.
    let lock = items(&settings.project(&stores("lock-login", "")));
    assert_eq!(row(&lock, "lock-login.widgets.lockMedia")["kind"], "toggle");
    assert_eq!(row(&lock, "lock-login.widgets.lockMedia")["enabled"], true);
}

/// B0094: QML (`SettingsRow`) draws an enum of up to four options as a segmented row and more as a drop-down; the
/// options of both carry the `set:` argument `settings.do` takes (a segmented click used to send a bare `id|n`).
#[test]
fn enum_rows_are_segments_to_four_options_and_a_drop_down_beyond() {
    let (settings, _) = fixture(json!({"language": "nb"}));
    let dt = items(&settings.project(&stores("date-time-language", "")));
    let language = row(&dt, "date-time-language.locale.language");
    assert_eq!(language["kind"], "choice");
    assert_eq!(
        language["options"][1]["id"],
        "set:date-time-language.locale.language|1"
    );
    let colors = items(&settings.project(&stores("colors", "")));
    assert_eq!(row(&colors, "colors.palette.automatic")["kind"], "toggle");
    assert_eq!(row(&colors, "colors.palette.automatic")["checked"], false);
    let source = row(&colors, "colors.palette.source");
    assert_eq!(source["kind"], "select");
    assert_eq!(source["options"].as_array().unwrap().len(), 31);
    assert!(
        source["options"][3]["id"]
            .as_str()
            .unwrap()
            .starts_with("set:colors.palette.source|")
    );
}

/// B0058 slice 2: launcher, apps and the dashboard sub-pages.
#[test]
fn launcher_apps_and_dashboard_pages_follow_the_registry() {
    let (settings, _) = fixture(json!({}));
    let launcher = items(&settings.project(&stores("launcher", "")));
    assert_eq!(row(&launcher, "launcher.sidebar.order")["reorder"], true);
    assert_eq!(row(&launcher, "launcher.sidebar.hidden")["reorder"], false);
    assert!(
        launcher
            .iter()
            .any(|r| r["id"] == "link:launcher/favorites")
    );
    let favorites = items(&settings.project(&stores("launcher/favorites", "")));
    assert_eq!(
        row(&favorites, "launcher/favorites.list.favorites")["kind"],
        "list"
    );
    // Object lists need a launcher entry editor Bead does not have: text, never a control.
    let entries = items(&settings.project(&stores("launcher/entries", "")));
    assert_eq!(
        row(&entries, "launcher/entries.list.entries")["kind"],
        "help"
    );
    let apps = items(&settings.project(&stores("apps", "")));
    let browser = row(&apps, "apps.defaults.browser");
    assert_eq!(browser["kind"], "list");
    assert_eq!(browser["items"][0]["label"], "google-chrome-stable");
    assert_eq!(browser["items"][0]["down"], "apps.defaults.browser|down:0");
    // Startup apps use the same ordered list editor; choices are fetched from the installed app catalog.
    assert_eq!(row(&apps, "apps.startup.startup")["kind"], "list");
    assert_eq!(row(&apps, "apps.startup.startup")["reorder"], true);
    let wallpaper = items(&settings.project(&stores("wallpaper", "")));
    assert_eq!(
        row(&wallpaper, "wallpaper.motion.lockWallpaper")["kind"],
        "picker"
    );
    let pages = items(&settings.project(&stores("panels/dashboard/pages", "")));
    assert_eq!(
        row(&pages, "panels/dashboard/pages.editor.pages")["reorder"],
        true
    );
    let weather = items(&settings.project(&stores("panels/dashboard/weather", "")));
    assert_eq!(
        row(&weather, "panels/dashboard/weather.editor.weatherPhotos")["kind"],
        "toggle"
    );
    // The control-center tile list stays with its own editor (B0049).
    let cc = items(&settings.project(&stores("panels/control-center", "")));
    assert!(
        cc.iter()
            .all(|r| r["id"] != "panels/control-center.editor.tiles" || r["kind"] == "help")
    );
}

#[test]
fn closing_releases_every_store_source() {
    let (settings, source) = fixture(json!({}));
    let weak = Arc::downgrade(&source);
    settings.project(&stores("appearance", ""));
    drop(settings);
    // Only the test's own handle is left.
    assert_eq!(Arc::strong_count(&source), 1);
    drop(source);
    assert!(weak.upgrade().is_none());
}

pub(crate) fn harness(
    model: BTreeMap<String, Value>,
    width: u32,
    height: u32,
    scale: f32,
) -> Harness {
    // The built-in English table is installed once per process; do not race another test to it.
    crate::i18n::init();
    Harness::load(
        &root(),
        Path::new("settings/main.luau"),
        &root().join("palette.json"),
        model,
        HarnessConfig {
            width,
            height,
            scale,
        },
    )
    .unwrap()
}

#[test]
fn settings_items_scroll_preserves_touchpad_pixels_and_shared_wheel_distance() {
    let items = (0..40)
        .map(|index| {
            json!({
                "id": format!("fixture-{index}"),
                "kind": "link",
                "label": format!("Fixture setting {index}"),
                "description": "A tall fixture row for scroll distance checks",
                "page": "appearance",
            })
        })
        .collect();
    let mut h = harness(
        BTreeMap::from([
            ("settings.items".into(), Value::list(items)),
            (
                "settings.title".into(),
                Value::Text("Fixture settings".into()),
            ),
            ("settings.description".into(), Value::Text("".into())),
            ("settings.searching".into(), Value::Bool(false)),
        ]),
        960,
        640,
        1.,
    );
    let region = h
        .session
        .view_nodes()
        .iter()
        .find(|node| node.scroll_store.as_deref() == Some("settings.scroll"))
        .map(|node| bead::ui::rect(&node.draw))
        .expect("settings item viewport");
    for (delta, expected) in [(23.5, 23.5), (100., 123.5)] {
        h.script([bead_test::Input::Scroll {
            x: region.x + region.w / 2.,
            y: region.y + region.h / 2.,
            delta,
        }])
        .unwrap();
        assert_eq!(
            h.session.stores.get("settings.scroll"),
            Some(&Value::Number(expected))
        );
    }
}

pub(crate) fn golden(name: &str, h: &mut Harness) {
    // The window fills its surface: no panel shadow inset.
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

/// Keep the real section and row together when capturing an editor below the page fold.
fn focus_setting(model: &mut BTreeMap<String, Value>, id: &str) {
    let selected = row(&items(model), id).clone();
    let mut section = nested_items(model)
        .into_iter()
        .find(|section| {
            section["rows"]
                .as_array()
                .is_some_and(|rows| rows.iter().any(|row| row["id"] == id))
        })
        .expect("setting section");
    section["rows"] = json!([selected]);
    model.insert("settings.items".into(), Value::list(vec![section]));
}

#[test]
fn settings_window_goldens() {
    let desktop =
        json!({"textSize": 14, "frameSheen": false, "monoFont": "Fira Code", "dnd": true});
    for (name, page, search, scale) in [
        ("settings-appearance-1", "appearance", "", 1.),
        ("settings-colors-1", "colors", "", 1.),
        ("settings-appearance-1.5", "appearance", "", 1.5),
        ("settings-notifications-1", "notifications", "", 1.),
        ("settings-search-1", "appearance", "font", 1.),
        ("settings-panels-1", "panels", "", 1.),
        ("settings-panels-top-bar-1", "panels/top-bar", "", 1.),
        (
            "settings-windows-workspaces-1",
            "windows-workspaces",
            "",
            1.,
        ),
        (
            "settings-date-time-language-1",
            "date-time-language",
            "",
            1.,
        ),
        ("settings-lock-login-1", "lock-login", "", 1.),
        ("settings-launcher-1", "launcher", "", 1.),
        (
            "settings-launcher-favorites-1",
            "launcher/favorites",
            "",
            1.,
        ),
        ("settings-apps-1", "apps", "", 1.),
        (
            "settings-dashboard-pages-1",
            "panels/dashboard/pages",
            "",
            1.,
        ),
        (
            "settings-dashboard-weather-1",
            "panels/dashboard/weather",
            "",
            1.,
        ),
        (
            "settings-dashboard-media-1",
            "panels/dashboard/media",
            "",
            1.,
        ),
        (
            "settings-dashboard-home-time-1",
            "panels/dashboard/home-time",
            "",
            1.,
        ),
        (
            "settings-dashboard-calendar-focus-1",
            "panels/dashboard/calendar-focus",
            "",
            1.,
        ),
    ] {
        let (settings, _) = fixture(desktop.clone());
        let mut model = settings.project(&stores(page, search));
        if page == "colors" {
            focus_setting(&mut model, "colors.palette.automatic");
        }
        // Goldens show the enabled look a connected daemon gives.
        model.insert("settings.writable".into(), Value::Bool(true));
        model.insert("settings.search".into(), Value::Text(search.into()));
        let mut h = harness(model, 960, 640, scale);
        golden(name, &mut h);
    }
    let (settings, _) = fixture_with(
        json!({}),
        &[(
            "settings.store.profile",
            doc(
                "profile",
                1,
                json!({"startup":[
                    {"label":"Browser","workspace":1,"argv":["browser"],"classes":["Browser"]},
                    {"label":"Files","workspace":1,"argv":["files"],"classes":["Files"]}
                ]}),
            ),
        )],
    );
    let settings = settings.with_startup_choices(&[
        (
            "Browser",
            json!({"label":"Browser","workspace":1,"argv":["browser"],"classes":["Browser"]}),
        ),
        (
            "Files",
            json!({"label":"Files","workspace":1,"argv":["files"],"classes":["Files"]}),
        ),
    ]);
    let mut model = settings.project(&stores("apps", ""));
    focus_setting(&mut model, "apps.startup.startup");
    model.insert("settings.writable".into(), Value::Bool(true));
    let mut h = harness(model, 960, 640, 1.);
    golden("settings-apps-startup-chooser-1", &mut h);

    let (settings, _) = fixture(json!({}));
    let settings = settings.with_lock_wallpapers(&[
        ("/wallpapers/night.jpg", "Night sky"),
        ("/wallpapers/forest.mp4", "Forest motion"),
    ]);
    let mut model = settings.project(&stores("wallpaper", ""));
    focus_setting(&mut model, "wallpaper.motion.lockWallpaper");
    model.insert("settings.writable".into(), Value::Bool(true));
    let mut h = harness(model, 960, 640, 1.);
    golden("settings-wallpaper-lock-chooser-1", &mut h);
}

/// Every row kind of the form kit, including the states a real page rarely shows
/// together: staged apply, editing text, a swatch, a list and disabled controls.
#[test]
fn form_kit_gallery_golden() {
    let body = vec![
        json!({"id":"t1","kind":"toggle","label":"Toggle on","description":"A switch","enabled":true,"checked":true}),
        json!({"id":"t2","kind":"toggle","label":"Needs apply","description":"Pending value","enabled":true,"checked":true,"staged":true,"apply":true}),
        json!({"id":"c1","kind":"choice","label":"Segmented choice","description":"","enabled":true,
            "options":[{"id":"c1|0","label":"One","selected":false},{"id":"c1|1","label":"Two","selected":true},{"id":"c1|2","label":"Three","selected":false}]}),
        json!({"id":"d2","kind":"select","label":"Drop-down","description":"A select, closed","enabled":true,"text":"Two",
            "options":[{"id":"d2|0","label":"One","selected":false},{"id":"d2|1","label":"Two","selected":true}]}),
        json!({"id":"n1","kind":"number","label":"Number","description":"Stepper with a unit","enabled":true,"number":12,"text":"12","unit":"px","step":1,"min":0,"max":30,"at_min":false,"at_max":false}),
        json!({"id":"x1","kind":"text","label":"Text","description":"","enabled":true,"text":"Fira Code","editing":false}),
        json!({"id":"x2","kind":"text","label":"Colour","description":"Swatch","enabled":true,"text":"#7aa2f7","swatch":[122,162,247,255],"editing":false}),
        json!({"id":"x3","kind":"text","label":"Editing","description":"","enabled":true,"text":"x","editing":true}),
        json!({"id":"l1","kind":"list","label":"List","enabled":true,"editing":false,"items":[{"id":"l1|rm:0","label":"first"},{"id":"l1|rm:1","label":"second"}]}),
        json!({"id":"d1","kind":"toggle","label":"Disabled toggle","description":"","enabled":false,"checked":false}),
        json!({"id":"h1","kind":"help","label":"Help text under the controls."}),
        json!({"id":"h2","kind":"help","label":"Quiet: unavailable","title":"Quiet setting","description":"What it does","note":"Not available here"}),
    ];
    let rows = vec![
        json!({"id":"s1","kind":"section","label":"Controls","description":"One of every row kind","rows":body}),
    ];
    let model = BTreeMap::from([
        (
            "settings.nav".into(),
            Value::list(vec![
                json!({"id":"group:x","kind":"group","label":"Group","icon":"","selected":false}),
                json!({"id":"a","kind":"page","label":"Page","icon":"palette","selected":true}),
            ]),
        ),
        ("settings.items".into(), Value::list(rows)),
        ("settings.title".into(), Value::Text("Kit gallery".into())),
        (
            "settings.description".into(),
            Value::Text("Every form kit row".into()),
        ),
        ("settings.writable".into(), Value::Bool(true)),
    ]);
    let mut h = harness(model, 960, 900, 1.);
    golden("settings-kit-1", &mut h);
}

/// B0094: the drop-down open (the list under its field, the chosen option filled) and the searchable one with its
/// filter line, no match and a cut-off tail.
#[test]
fn select_rows_open_their_list_under_the_field() {
    let option = |id: &str, label: &str, selected: bool| json!({"id": id, "label": label, "selected": selected, "enabled": true});
    let long: Vec<Json> = (0..12)
        .map(|i| {
            option(
                &format!("m|{i}"),
                &format!("{}x{}@60.000Hz", 640 + i * 160, 360 + i * 90),
                i == 2,
            )
        })
        .collect();
    let body = vec![
        json!({"id":"a1","kind":"select","label":"Scale","description":"","enabled":true,"text":"125%",
            "options":[option("a1|0","100%",false),option("a1|1","125%",true),option("a1|2","150%",false)]}),
        json!({"id":"a2","kind":"select","label":"Resolution and refresh rate","description":"","enabled":true,"wide":true,
            "text":"960x540@60.000Hz","options":long,"filter":"Filter modes","more":"4 more, type to filter"}),
    ];
    let rows =
        vec![json!({"id":"s1","kind":"section","label":"Display","description":"","rows":body})];
    let model = |open: &str| {
        BTreeMap::from([
            (
                "settings.nav".into(),
                Value::list(vec![
                    json!({"id":"a","kind":"page","label":"Page","icon":"palette","selected":true}),
                ]),
            ),
            ("settings.items".into(), Value::list(rows.clone())),
            ("settings.title".into(), Value::Text("Displays".into())),
            ("settings.writable".into(), Value::Bool(true)),
            ("settings.select".into(), Value::Text(open.into())),
        ])
    };
    let mut h = harness(model("a1"), 960, 640, 1.);
    golden("settings-select-open-1", &mut h);
    let mut h = harness(model("a2"), 960, 760, 1.);
    golden("settings-select-search-1", &mut h);
}

/// Typing in the search field rebuilds the filtered list each key. Budget 8 ms per key
/// (release); a debug build is several times slower, so it only guards against a blow-up.
#[test]
fn typing_in_the_search_field_is_fast() {
    use bead::wayland::{SceneSource, UiEvent};
    crate::i18n::init();
    let palette = root().join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session =
        bead::ui::Session::new(root(), "settings/main.luau".into(), palette, colors, cold);
    let mut runtime = crate::reload::Runtime::new(session)
        .with_settings(None, &root())
        .unwrap();
    runtime.services(Box::new(|_| {}));
    runtime.viewport(960, 640);
    runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("settings view loads");
    runtime.commit();
    let node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|n| n.id.ends_with("search"))
        .expect("search field")
        .draw
        .clone();
    let rect = bead::ui::rect(&node);
    runtime
        .stage(UiEvent::Click {
            x: rect.x + 4.,
            y: rect.y + rect.h / 2.,
        })
        .unwrap();
    runtime.commit();
    let mut times = Vec::new();
    for ch in "keyboard layout".chars() {
        let started = std::time::Instant::now();
        runtime
            .stage(UiEvent::Key {
                text: ch.to_string(),
                backspace: false,
            })
            .unwrap();
        runtime.commit();
        times.push(started.elapsed().as_secs_f64() * 1000.);
    }
    times.sort_by(f64::total_cmp);
    let (p50, max) = (times[times.len() / 2], times[times.len() - 1]);
    println!(
        "SETTINGS_TYPING p50_ms={p50:.2} max_ms={max:.2} debug={}",
        cfg!(debug_assertions)
    );
    let budget = if cfg!(debug_assertions) { 80. } else { 8. };
    assert!(
        p50 <= budget && max <= budget * 2.,
        "p50 {p50:.2} max {max:.2} ms"
    );
    assert!(matches!(
        runtime.session.stores.get("settings.search"),
        Some(Value::Text(t)) if t == "keyboard layout"
    ));
}
