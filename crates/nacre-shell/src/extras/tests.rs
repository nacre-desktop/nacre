// SPDX-License-Identifier: GPL-3.0-only
use super::*;
use crate::nacre_source::Config;
use nacre_protocol::{Message, Reply, framing};
use tokio::{
    io::BufReader,
    net::UnixListener,
    sync::{mpsc, oneshot},
};
fn fixture() -> Json {
    json!({"available":true,"capabilities":{"perOutputWorkspaces":true},"focus":{"outputId":"OUT"},
        "workspaces":{"ws1":{"index":1,"name":"Work","outputId":"OUT"},"ws2":{"index":2,"outputId":"OTHER"}},"workspaceOrder":["ws1","ws2"],
        "windows":{"win1":{"title":"Editor","wmClass":"editor","workspaceId":"ws1","outputId":"OUT"},"win2":{"title":"Other","workspaceId":"ws2","outputId":"OTHER"},"hidden":{"hidden":true}},"windowOrder":["win1","win2","hidden"]})
}

#[test]
fn raw_catalogue_palette_extension_survives_typed_entry_projection() {
    let catalogue = json!({"entries":[
        {"id":"/wall/a.png","kind":"image","palette":{"mode":"dark","colours":{"background":"#123456","onSurface":"#ffffff","primary":"#abcdef"}}},
        {"id":"/wall/b.png","kind":"image"},
        {"id":"/wall/c.png","kind":"image","palette":{"mode":"dark","colours":{"background":"red","onSurface":"#ffffff","primary":"#abcdef"}}}
    ]});
    let expected = json!({"mode":"dark","colours":{"background":"#123456","onSurface":"#ffffff","primary":"#abcdef"}});
    assert_eq!(
        raw_catalogue_palette(&catalogue, "/wall/a.png"),
        Some(&expected)
    );
    assert!(raw_catalogue_palette(&catalogue, "/wall/b.png").is_none());
    assert!(raw_catalogue_palette(&catalogue, "/wall/c.png").is_none());
    assert!(raw_catalogue_palette(&catalogue, "/wall/missing.png").is_none());
    assert!(valid_catalogue_palette(&json!({
        "mode":"dark",
        "colours":{"background":"#123456","onSurface":"#ffffff","primary":"#abcdef"}
    })));
    for invalid in [
        json!({"mode":"dark","colours":{"background":"#123456","onSurface":"#ffffff"}}),
        json!({"mode":"dark","colours":{"background":"red","onSurface":"#ffffff","primary":"#abcdef"}}),
        json!({"mode":"sepia","colours":{"background":"#123456","onSurface":"#ffffff","primary":"#abcdef"}}),
    ] {
        assert!(!valid_catalogue_palette(&invalid));
    }
}
#[test]
fn targeted_palette_hydration_keeps_the_retained_cache_bounded() {
    let mut entries = (0..80)
        .map(|index| {
            json!({
                "id":format!("/wall/{index}.png"),
                "active":index == 0,
                "palette":{"mode":"dark","colours":{
                    "background":"#123456","onSurface":"#ffffff","primary":"#abcdef"
                }}
            })
        })
        .collect::<Vec<_>>();
    let requested = (16..80)
        .map(|index| format!("/wall/{index}.png"))
        .collect::<Vec<_>>();
    trim_catalogue_palette_cache(&mut entries, &requested);
    let retained = entries
        .iter()
        .filter(|entry| entry.get("palette").is_some())
        .count();
    assert_eq!(retained, CATALOGUE_PALETTE_CACHE_LIMIT);
    assert!(
        entries
            .iter()
            .any(|entry| { entry["id"] == "/wall/0.png" && entry.get("palette").is_some() })
    );
    assert!(
        entries
            .iter()
            .any(|entry| { entry["id"] == "/wall/16.png" && entry.get("palette").is_some() })
    );
    assert!(
        entries
            .iter()
            .any(|entry| { entry["id"] == "/wall/79.png" && entry.get("palette").is_none() })
    );
}

#[test]
fn overview_follows_focused_output_and_stable_ids() {
    let (windows, workspaces) = compositor_rows(&fixture());
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0]["id"], "win1");
    assert_eq!(windows[0]["workspace"], "ws1");
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0]["index"], 1);
}
struct Lab {
    dir: std::path::PathBuf,
    client: Client,
    calls: mpsc::UnboundedReceiver<(String, Json)>,
    activation_release: Option<oneshot::Sender<()>>,
}
impl Drop for Lab {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
fn start_with_empty_wallpaper_catalogue(extras: &Extras) {
    let mut state = extras.state.borrow_mut();
    state.catalogue = None;
    state.catalogue_boot = None;
    state.catalogue_request_boot = None;
    state.catalogue_request_ids.clear();
    state.catalogue_request_handles.clear();
    state.catalogue_result.clear();
    state.catalogue_preloaded = false;
}
async fn lab_with_topics(requested_topics: Vec<String>) -> Lab {
    lab_with_activation_behavior(requested_topics, false, false, 0).await
}
async fn lab_with_activation_gate(
    requested_topics: Vec<String>,
    gate_first_activation: bool,
) -> Lab {
    lab_with_activation_behavior(requested_topics, gate_first_activation, false, 0).await
}
async fn lab_with_activation_behavior(
    requested_topics: Vec<String>,
    gate_first_activation: bool,
    fail_first_activation: bool,
    stale_menu_activations: usize,
) -> Lab {
    let lab = unconnected_lab(
        requested_topics,
        gate_first_activation,
        fail_first_activation,
        stale_menu_activations,
    );
    await_connected(&lab.client).await;
    lab
}
async fn await_connected(client: &Client) {
    for _ in 0..200 {
        if client.snapshot().connected {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(client.snapshot().connected);
}
/// The fake daemon is listening but the client has not connected yet: the daemon boot is not known.
fn unconnected_lab(
    requested_topics: Vec<String>,
    gate_first_activation: bool,
    fail_first_activation: bool,
    stale_menu_activations: usize,
) -> Lab {
    use std::os::unix::fs::PermissionsExt;
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "extras-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700)).unwrap();
    let socket = dir.join("daemon.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (activation_release, activation_wait) = oneshot::channel();
    let activation_wait = Arc::new(std::sync::Mutex::new(
        gate_first_activation.then_some(activation_wait),
    ));
    let fixture_image = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../shell/sample.png")
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600)).unwrap();
    let (tx, calls) = mpsc::unbounded_channel();
    let mut offered_topics = BTreeMap::from([
        ("compositor".into(), 1),
        ("shortcuts".into(), 1),
        ("wallpaper.active".into(), 1),
    ]);
    for topic in &requested_topics {
        offered_topics.insert(topic.clone(), 1);
    }
    let fail_activation = Arc::new(std::sync::atomic::AtomicBool::new(fail_first_activation));
    let fail_activation_for_server = fail_activation.clone();
    let stale_menu_for_server =
        Arc::new(std::sync::atomic::AtomicUsize::new(stale_menu_activations));
    let activation_wait_for_server = activation_wait.clone();
    tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let (read, mut writer) = stream.into_split();
        let mut reader = framing::Reader::new(BufReader::new(read));
        assert!(matches!(
            reader.next().await.unwrap(),
            Some(Message::Hello { .. })
        ));
        let mut methods = nacre_protocol::core_methods();
        for name in [
            "window.focus",
            "workspace.activate",
            "shortcuts.refresh",
            "menu.query",
            "menu.activate",
            "wallpaper.catalogue",
            "wallpaper.activate",
            "settings.set",
            "settings.provider",
        ] {
            methods.insert(
                name.into(),
                nacre_protocol::Method {
                    v: 1,
                    class: nacre_protocol::Capability::Desktop,
                    timeout: 1000,
                    params: json!({}),
                    result: json!({}),
                },
            );
        }
        methods.remove("daemon.status");
        let registry_methods = methods.clone();
        let registry_topics = offered_topics.keys().cloned().collect::<Vec<_>>();
        framing::write(
            &mut writer,
            &Message::Welcome {
                proto: 1,
                min: 1,
                boot: "extras-boot".into(),
                topics: offered_topics,
                methods,
                frame_ms: None,
            },
        )
        .await
        .unwrap();
        while let Ok(Some(msg)) = reader.next().await {
            match msg {
                Message::Sub { topics } => {
                    for topic in topics {
                        let data = match topic.as_str() {
                            "compositor" => fixture(),
                            "wallpaper.active" => {
                                json!({"available":true,"stale":false,"revision":17,
                                "current":{"revision":17,"palette_revision":17,"path":"/fixture/current.jpg",
                                    "file_identity":{"device":"1","inode":"2","size":4,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},
                                    "presentation":"crop","palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}}},
                                "outputs":{}})
                            }
                            "settings.store.wallpaper-picker" => {
                                json!({"store":"wallpaper-picker","rev":3,"data":{"kind":"static","layout":"carousel","paused":true,"future":"preserve"}})
                            }
                            _ => {
                                json!({"health":{"available":true,"stale":false},"items":{"order":["k1"],"items":{"k1":{"key":"Super+K","description":"Shortcuts"}}}})
                            }
                        };
                        if framing::write(
                            &mut writer,
                            &Message::Snap {
                                topic,
                                seq: 0,
                                data,
                            },
                        )
                        .await
                        .is_err()
                        {
                            return;
                        }
                    }
                }
                Message::Call { id, m, p, .. } => {
                    if m != "daemon.status" {
                        let _ = tx.send((m.clone(), p.clone()));
                    }
                    let ok = if m == "menu.query" {
                        // Order is deliberately not alphabetical: `order` alone is authoritative (N0058).
                        let q = p["query"].as_str().unwrap_or_default();
                        let order = if q.is_empty() {
                            json!(["action", "action-b"])
                        } else if q.trim_start_matches('>').trim().starts_with("brain:") {
                            json!(["brain"])
                        } else {
                            json!(["action", "window", "app"])
                        };
                        let all = json!({
                            "action":{"name":"Open Settings","kind":"action","activation":"opaque-ticket","disabled":false},
                            "action-b":{"name":"Calculator","kind":"action","activation":"ticket-b","disabled":false},
                            "window":{"name":"Term","kind":"window","description":"Window · workspace 1","icon":"window","activation":"ticket-w","disabled":false},
                            "app":{"name":"Zed","kind":"app","description":"Application","icon":"apps","activation":"ticket-a","disabled":false},
                            "brain":{"name":"Search brain for x","kind":"action","description":"Open the brain drawer","icon":"neurology","activation":"ticket-br","disabled":false}});
                        let items: serde_json::Map<String, Json> = order
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|id| {
                                (
                                    id.as_str().unwrap().to_owned(),
                                    all[id.as_str().unwrap()].clone(),
                                )
                            })
                            .collect();
                        json!({"query":p["query"],"mode":p["mode"],"route":p["route"],"catalog_rev":7,"order":order,"items":items})
                    } else if m == "menu.activate" {
                        json!({"keep_open":true,"view":{"route":"actions.windows","query":"edited"}})
                    } else if m == "wallpaper.catalogue" {
                        let entries = json!([
                            {"id":"/fixture/current.jpg","kind":"image","animated":false,"title":"Current","thumbnail":"","preview":"","active":true},
                            {"id":"/fixture/next.jpg","kind":"image","animated":false,"title":"Next","thumbnail":"","preview":"","active":false},
                            {"id":"/fixture/next.mp4","kind":"video","animated":false,"title":"Moving","thumbnail":"","preview":"","active":false},
                            {"id":"/fixture/animated.gif","kind":"image","animated":true,"title":"Animated","thumbnail":"","preview":"","active":false}
                        ]);
                        let ids = p["ids"].as_array().cloned().unwrap_or_default();
                        let entries: Vec<Json> = entries
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|entry| {
                                let mut entry = entry.clone();
                                if !ids.is_empty()
                                    && ids.iter().any(|id| id.as_str() == entry["id"].as_str())
                                {
                                    entry["thumbnail"] = fixture_image.clone().into();
                                    entry["palette"] = json!({"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}});
                                    if entry["id"] != "/fixture/next.jpg" && entry["id"] != "/fixture/next.mp4" {
                                        entry["preview"] = fixture_image.clone().into();
                                    }
                                    if entry["id"] == "/fixture/next.mp4" {
                                        entry["video_predecode"] = json!({
                                            "poster_path":fixture_image,
                                            "poster_file_identity":{"device":"1","inode":"2","size":4,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},
                                            "source_file_identity":{"device":"1","inode":"3","size":32,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0}
                                        });
                                    }
                                }
                                entry
                            })
                            .collect();
                        json!({"entries":entries})
                    } else if m == "wallpaper.activate" {
                        let wait = activation_wait_for_server
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())
                            .take();
                        if let Some(wait) = wait {
                            let _ = wait.await;
                        }
                        let revision = p["revision"].as_u64().unwrap_or(17) + 1;
                        json!({"revision":revision,"palette_revision":revision,"path":p["id"],
                            "file_identity":{"device":"1","inode":"3","size":4,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},
                            "presentation":"crop","palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}}})
                    } else if m == "daemon.status" {
                        json!({"boot":"extras-boot","topics":registry_topics,"methods":registry_methods})
                    } else {
                        json!({"changed":true})
                    };
                    let reply = if m == "wallpaper.activate"
                        && fail_activation_for_server
                            .swap(false, std::sync::atomic::Ordering::AcqRel)
                    {
                        Reply::Err {
                            err: nacre_protocol::Failure::new(
                                nacre_protocol::ErrorCode::Unavailable,
                                "fixture activation failure",
                                false,
                            ),
                        }
                    } else if m == "menu.activate"
                        && stale_menu_for_server
                            .try_update(
                                std::sync::atomic::Ordering::AcqRel,
                                std::sync::atomic::Ordering::Acquire,
                                |n| n.checked_sub(1),
                            )
                            .is_ok()
                    {
                        Reply::Err {
                            err: nacre_protocol::Failure::new(
                                nacre_protocol::ErrorCode::StaleRevision,
                                "Catalog changed; query again",
                                true,
                            ),
                        }
                    } else {
                        Reply::Ok { ok }
                    };
                    if framing::write(&mut writer, &Message::Ret { id, reply })
                        .await
                        .is_err()
                    {
                        return;
                    }
                }
                _ => {}
            }
        }
    });
    let client = Client::start(Config::new(socket, requested_topics)).unwrap();
    Lab {
        dir,
        client,
        calls,
        activation_release: gate_first_activation.then_some(activation_release),
    }
}
async fn lab() -> Lab {
    lab_with_topics(vec![]).await
}
fn stores(query: &str) -> Stores {
    Stores::default()
        .reconcile(&BTreeMap::from([(
            "extras.query".into(),
            Value::Text(query.into()),
        )]))
        .unwrap()
}
async fn ready(extras: &Extras, stores: &Stores) -> BTreeMap<String, Value> {
    for _ in 0..200 {
        let model = extras.project(stores, Some(Arc::new(|| {})));
        if model["extras.available"] == Value::Bool(true) {
            return model;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("extras data unavailable");
}
#[tokio::test(flavor = "current_thread")]
async fn published_topics_focus_and_shortcut_refresh() {
    let mut lab = lab().await;
    let overview = Extras::new(Kind::Overview, lab.client.clone()).unwrap();
    let model = ready(&overview, &stores("")).await;
    assert!(matches!(&model["extras.windows"],Value::List(rows) if rows.len()==1));
    overview.act("focus:missing", &stores(""), Arc::new(|| {}));
    assert!(lab.calls.try_recv().is_err());
    overview.act("focus:win1", &stores(""), Arc::new(|| {}));
    let call = tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(call, ("window.focus".into(), json!({"window":"win1"})));
    drop(overview);
    let keys = Extras::new(Kind::Keybindings, lab.client.clone()).unwrap();
    let model = ready(&keys, &stores("")).await;
    assert!(matches!(&model["extras.rows"],Value::List(rows) if rows[0]["key"]=="Super+K"));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        ("shortcuts.refresh".into(), json!({}))
    );
}
#[tokio::test(flavor = "current_thread")]
async fn palette_uses_opaque_engine_activation_and_rejects_obsolete_query() {
    let mut lab = lab().await;
    let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
    let initial = stores("");
    let model = ready(&palette, &initial).await;
    assert!(
        matches!(&model["extras.rows"],Value::List(rows) if rows[0].get("activation").is_none())
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "menu.query".into(),
            json!({"query":"","mode":"palette","route":"palette"})
        )
    );
    palette.act("activate:action", &stores("changed"), Arc::new(|| {}));
    assert!(lab.calls.try_recv().is_err());
    palette.act("activate:action", &initial, Arc::new(|| {}));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "menu.activate".into(),
            json!({"id":"action","activation":"opaque-ticket","catalog_rev":7})
        )
    );
}
fn names(model: &BTreeMap<String, Value>) -> Vec<String> {
    match &model["extras.rows"] {
        Value::List(rows) => rows
            .iter()
            .map(|r| r["name"].as_str().unwrap_or_default().to_owned())
            .collect(),
        _ => panic!("rows missing"),
    }
}
#[tokio::test(flavor = "current_thread")]
async fn palette_mode_keeps_the_daemon_order_for_empty_typed_and_prefixed_queries() {
    let mut lab = lab().await;
    let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
    let model = ready(&palette, &stores("")).await;
    assert_eq!(names(&model), ["Open Settings", "Calculator"]);
    for (query, expected) in [
        ("set", vec!["Open Settings", "Term", "Zed"]),
        ("> set", vec!["Open Settings", "Term", "Zed"]),
        ("brain: x", vec!["Search brain for x"]),
        (">brain:x", vec!["Search brain for x"]),
    ] {
        let model = ready(&palette, &stores(query)).await;
        assert_eq!(names(&model), expected, "query {query:?}");
        let call = loop {
            let call = lab.calls.recv().await.unwrap();
            if call.1["query"] == query {
                break call;
            }
        };
        assert_eq!(call.0, "menu.query");
        assert_eq!(call.1["mode"], "palette");
        assert_eq!(call.1["route"], "palette");
    }
}
#[tokio::test(flavor = "current_thread")]
async fn a_closed_panel_forgets_the_typed_query() {
    let lab = lab().await;
    let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
    let closed = Stores::default()
        .reconcile(&BTreeMap::from([
            ("extras.query".into(), Value::Text("set".into())),
            ("panel.open".into(), Value::Bool(false)),
        ]))
        .unwrap();
    let model = palette.project(&closed, None);
    assert_eq!(model["extras.query"], Value::Text(String::new()));
}
#[tokio::test(flavor = "current_thread")]
async fn palette_requeries_and_retries_a_stale_activation_once_without_a_message() {
    // One stale reply is retried silently; a second one is reported instead of looping (N0309).
    for stale in [1, 2] {
        let mut lab = lab_with_activation_behavior(vec![], false, false, stale).await;
        let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
        let initial = stores("");
        ready(&palette, &initial).await;
        while lab.calls.try_recv().is_ok() {}
        palette.act("activate:action", &initial, Arc::new(|| {}));
        let mut calls = Vec::new();
        let mut model = BTreeMap::new();
        for _ in 0..400 {
            model = palette.project(&initial, Some(Arc::new(|| {})));
            while let Ok(call) = lab.calls.try_recv() {
                calls.push(call);
            }
            if calls.len() == 3 && palette.state.borrow().action.is_none() {
                break;
            }
            assert_eq!(model["extras.message"], Value::Text(String::new()));
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        let activation = json!({"id":"action","activation":"opaque-ticket","catalog_rev":7});
        assert_eq!(
            calls,
            [
                ("menu.activate".into(), activation.clone()),
                (
                    "menu.query".into(),
                    json!({"query":"","mode":"palette","route":"palette"})
                ),
                ("menu.activate".into(), activation),
            ]
        );
        let state = palette.state.borrow();
        assert!(state.retry.is_none() && state.action_id.is_none());
        if stale == 1 {
            assert_eq!(model["extras.message"], Value::Text(String::new()));
            assert_eq!(state.route.as_deref(), Some("actions.windows"));
        } else {
            assert_eq!(
                model["extras.message"],
                Value::Text("Action unavailable; refresh and try again".into())
            );
        }
    }
}
#[tokio::test(flavor = "current_thread")]
async fn palette_activates_a_typed_row_with_its_own_ticket() {
    let mut lab = lab().await;
    let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
    let typed = stores("set");
    ready(&palette, &typed).await;
    while lab.calls.try_recv().is_ok() {}
    palette.act("activate:window", &typed, Arc::new(|| {}));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "menu.activate".into(),
            json!({"id":"window","activation":"ticket-w","catalog_rev":7})
        )
    );
}
#[tokio::test(flavor = "current_thread")]
async fn wallpaper_catalogue_projects_and_selection_calls_revision_fenced_activate() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&wallpaper);
    let stores = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.query".into(), Value::Text(String::new())),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    let first = wallpaper.project(&stores, Some(wake.clone()));
    assert_eq!(first["extras.wallpaper.loading"], Value::Bool(true));
    assert_eq!(
        first["extras.available"],
        Value::Bool(true),
        "an admitted catalogue request renders the loading state instead of unavailable"
    );
    assert!(
        wallpaper.content_ready(),
        "metadata loading cannot block the first frame"
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    let mut rows = Value::list(Vec::new());
    for _ in 0..200 {
        let model = wallpaper.project(&stores, Some(wake.clone()));
        rows = model["extras.rows"].clone();
        if matches!(&rows,Value::List(entries) if entries.len()==4) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(matches!(&rows,Value::List(entries) if entries.len()==4
            && entries.iter().any(|entry| entry["id"]=="/fixture/animated.gif" && entry["animated"]==true)
            && entries.iter().any(|entry| entry["id"]=="/fixture/next.mp4")));
    // An unchanged projection reuses the rows instead of rebuilding them (this runs on every Strand frame).
    let again = wallpaper.project(&stores, Some(wake.clone()));
    assert!(
        matches!((&rows, &again["extras.rows"]), (Value::List(a), Value::List(b)) if Arc::ptr_eq(a, b))
    );
    assert!(
        wallpaper
            .state
            .borrow()
            .catalogue_result
            .iter()
            .all(|entry| entry["thumbnail"] == "" && entry["preview"] == ""),
        "ids:[] provides all metadata without materializing cache paths"
    );
    wallpaper.act(
        "wallpaper:activate:/fixture/next.jpg",
        &stores,
        wake.clone(),
    );
    let (method, params) = tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(method, "wallpaper.activate");
    assert_eq!(params["id"], "/fixture/next.jpg");
    // The first reply confirms revision 18; selecting before it lands would race it.
    let confirmed = || {
        let queue = wallpaper.wallpaper_actions.as_ref().unwrap();
        let queue = queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.confirmed_revision("extras-boot")
    };
    for _ in 0..400 {
        if confirmed() == Some(18) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(confirmed(), Some(18));
    wallpaper.act("wallpaper:activate:/fixture/next.mp4", &stores, wake);
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        (
            "wallpaper.activate".into(),
            json!({"revision":18,"id":"/fixture/next.mp4"})
        )
    );

    let closed = stores
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(false))]))
        .unwrap();
    wallpaper.project(&closed, Some(Arc::new(|| {})));
    let reopened = closed
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(true))]))
        .unwrap();
    let reopened_model = wallpaper.project(&reopened, Some(Arc::new(|| {})));
    assert!(
        matches!(
            &reopened_model["extras.rows"],
            Value::List(entries) if entries.len() == 4
        ),
        "a valid catalogue remains available when the panel reopens"
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "a reopen with cached catalogue rows must not respawn the full catalogue scan"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_hydrates_only_visible_image_ids_and_suppresses_missing_preview_retries() {
    use bead::render::{Primitive, Rect, Scene};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&wallpaper);
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    wallpaper.project(&open, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }

    let mut scene = Scene {
        layers: Vec::new(),
        font_files: Vec::new(),
        primitives: vec![Primitive::image(
            Rect::new(0., 0., 8., 6.),
            std::path::PathBuf::from("@icon:wallpaper:preview:/fixture/next.mp4"),
        )],
        ..Scene::empty(8., 6.)
    };
    wallpaper.attach_images(&mut scene, Some(wake.clone()));
    assert!(
        wallpaper.content_ready(),
        "visible thumbnails hydrate progressively"
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/next.mp4"]})
        )
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    {
        let state = wallpaper.state.borrow();
        assert_eq!(
            state.catalogue_result.len(),
            4,
            "hydration retains the full catalogue"
        );
        let next = state
            .catalogue_result
            .iter()
            .find(|entry| entry["id"] == "/fixture/next.mp4")
            .unwrap();
        let expected = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../shell/sample.png")
            .canonicalize()
            .unwrap();
        let expected = expected.to_string_lossy();
        assert_eq!(next["thumbnail"].as_str(), Some(expected.as_ref()));
        assert_eq!(
            next["preview"], "",
            "the moving poster comes from the explicit predecode descriptor"
        );
        assert_eq!(next["video_predecode"]["poster_path"], expected.as_ref());
        assert_eq!(
            next["video_predecode"]["source_file_identity"]["inode"],
            "3"
        );
        assert!(
            state
                .catalogue_result
                .iter()
                .filter(|entry| entry["id"] != "/fixture/next.mp4")
                .all(|entry| entry["thumbnail"] == "" && entry["preview"] == "")
        );
    }
    let mut repeated = Scene {
        layers: Vec::new(),
        font_files: Vec::new(),
        primitives: vec![Primitive::image(
            Rect::new(0., 0., 8., 6.),
            std::path::PathBuf::from("@icon:wallpaper:preview:/fixture/next.mp4"),
        )],
        ..Scene::empty(8., 6.)
    };
    wallpaper.attach_images(&mut repeated, Some(wake.clone()));
    assert!(
        wallpaper.content_ready(),
        "settled missing previews use their hydrated thumbnail on the first frame"
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "a missing preview must not cause an automatic retry loop"
    );
    assert!(
        repeated.asset_data.values().any(|bytes| !bytes.is_empty()),
        "the advertised thumbnail remains a fallback"
    );
    // A cached thumbnail is ready content and does not restart a catalogue child.
    let mut cached = repeated.clone();
    if let Primitive::Image { path, .. } = &mut cached.primitives[0] {
        *path = std::path::PathBuf::from("@icon:wallpaper:thumbnail:/fixture/next.mp4");
    }
    let template = cached.clone();
    let ready_revision = wallpaper.state.borrow().image_revision;
    wallpaper.attach_images(&mut cached, Some(wake.clone()));
    assert!(
        wallpaper.content_ready(),
        "cached thumbnails must not delay ready images"
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "a cached thumbnail must not respawn palette/catalogue preparation"
    );
    assert_eq!(
        wallpaper.state.borrow().image_revision,
        ready_revision,
        "a cached thumbnail must not discard prepared render content"
    );
    wallpaper.attach_images(&mut template.clone(), Some(wake.clone()));
    assert!(
        lab.calls.try_recv().is_err(),
        "a settled visible set is prepared once"
    );
    let mut other_visible = template.clone();
    other_visible.asset_data.clear();
    if let Primitive::Image { path, .. } = &mut other_visible.primitives[0] {
        *path = std::path::PathBuf::from("@icon:wallpaper:preview:/fixture/next.jpg");
    }
    let mut hydrated_other = other_visible.clone();
    wallpaper.attach_images(&mut other_visible, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/next.jpg"]})
        )
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        wallpaper
            .state
            .borrow()
            .catalogue_result
            .iter()
            .any(|entry| {
                entry["id"] == "/fixture/next.mp4"
                    && entry["video_predecode"]["source_file_identity"]["inode"] == "3"
            }),
        "metadata for another requested card preserves the hydrated video's descriptor"
    );
    wallpaper.attach_images(&mut hydrated_other, Some(wake.clone()));
    assert!(wallpaper.content_ready());

    // A real refresh ticket may be pending while displayed paths remain hydrated.
    wallpaper.state.borrow_mut().catalogue =
        wallpaper.begin("wallpaper.catalogue", json!({"ids":[]}), Some(wake));
    assert!(wallpaper.state.borrow().catalogue.is_some());
    assert!(
        wallpaper.content_ready(),
        "same-boot hydrated content remains ready during refresh"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_cached_thumbnails_without_pick_data_fetch_it_once() {
    use bead::render::{Primitive, Rect, Scene};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&wallpaper);
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    let settle = |wallpaper: &Extras| wallpaper.state.borrow().catalogue.is_none();
    wallpaper.project(&open, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if settle(&wallpaper) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let scene = |handle: &str| Scene {
        layers: Vec::new(),
        font_files: Vec::new(),
        primitives: vec![Primitive::image(
            Rect::new(0., 0., 8., 6.),
            std::path::PathBuf::from(handle),
        )],
        ..Scene::empty(8., 6.)
    };
    // First sight hydrates the thumbnail, palette and the video's poster descriptor.
    wallpaper.attach_images(
        &mut scene("@icon:wallpaper:thumbnail:/fixture/next.mp4"),
        Some(wake.clone()),
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/next.mp4"]})
        )
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if settle(&wallpaper) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // A persisted catalogue keeps the thumbnails but not the pick data: the next open asks again.
    {
        let mut state = wallpaper.state.borrow_mut();
        state.catalogue_settled_handles.clear();
        let entry = state
            .catalogue_result
            .iter_mut()
            .find(|entry| entry["id"] == "/fixture/next.mp4")
            .unwrap()
            .as_object_mut()
            .unwrap();
        entry.remove("palette");
        entry.remove("video_predecode");
    }
    wallpaper.attach_images(
        &mut scene("@icon:wallpaper:thumbnail:/fixture/next.mp4"),
        Some(wake.clone()),
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/next.mp4"]})
        )
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if settle(&wallpaper) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    {
        let state = wallpaper.state.borrow();
        let entry = state
            .catalogue_result
            .iter()
            .find(|entry| entry["id"] == "/fixture/next.mp4")
            .unwrap();
        assert!(!entry["palette"].is_null() && !entry["video_predecode"].is_null());
    }
    wallpaper.attach_images(
        &mut scene("@icon:wallpaper:thumbnail:/fixture/next.mp4"),
        Some(wake),
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "an entry with its pick data starts no helper child"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_close_discards_an_inflight_hydration_ticket_and_cache() {
    use bead::render::{Primitive, Rect, Scene};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&wallpaper);
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    wallpaper.project(&open, Some(wake.clone()));
    let _ = lab.calls.recv().await.unwrap();
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut scene = Scene {
        layers: Vec::new(),
        font_files: Vec::new(),
        primitives: vec![Primitive::image(
            Rect::new(0., 0., 8., 6.),
            std::path::PathBuf::from("@icon:wallpaper:thumbnail:/fixture/current.jpg"),
        )],
        ..Scene::empty(8., 6.)
    };
    wallpaper.attach_images(&mut scene, Some(wake.clone()));
    let call = lab.calls.recv().await.unwrap();
    assert_eq!(call.0, "wallpaper.catalogue");
    assert!(!wallpaper.state.borrow().catalogue_request_ids.is_empty());
    let closed = Stores::default()
        .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(false))]))
        .unwrap();
    wallpaper.project(&closed, Some(wake));
    let state = wallpaper.state.borrow();
    assert!(state.catalogue.is_none());
    assert!(state.catalogue_request_ids.is_empty());
    assert!(state.catalogue_pending_handles.is_empty());
    assert_eq!(state.image_bytes, 0);
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_refreshes_deleted_cache_paths_and_settles_still_broken_paths() {
    use bead::render::{Primitive, Rect, Scene};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    let wake: CallWake = Arc::new(|| {});
    let stale = lab.dir.join("deleted-cache.png");
    image::RgbaImage::from_pixel(8, 6, image::Rgba([40, 70, 90, 255]))
        .save(&stale)
        .unwrap();
    std::fs::remove_file(&stale).unwrap();
    {
        let mut state = wallpaper.state.borrow_mut();
        state.wallpaper_open = true;
        state.wallpaper_boot = Some("extras-boot".into());
        state.catalogue_boot = Some("extras-boot".into());
        state.catalogue_result = vec![json!({
            "id":"/fixture/current.jpg","kind":"image","title":"Current",
            "thumbnail":stale.to_string_lossy(),"preview":"","active":true
        })];
    }
    let scene_for = |id: &str| Scene {
        layers: Vec::new(),
        font_files: Vec::new(),
        primitives: vec![Primitive::image(
            Rect::new(0., 0., 8., 6.),
            std::path::PathBuf::from(format!("@icon:wallpaper:thumbnail:0:{id}")),
        )],
        ..Scene::empty(8., 6.)
    };

    let cached = lab.dir.join("cached.png");
    image::RgbaImage::from_pixel(8, 6, image::Rgba([40, 70, 90, 255]))
        .save(&cached)
        .unwrap();
    wallpaper.state.borrow_mut().catalogue_result.push(json!({
        "id":"/fixture/cached.jpg","kind":"image","title":"Cached",
        "thumbnail":cached.to_string_lossy(),"preview":"","active":false,
        "palette":{"mode":"dark","colours":{"background":"#102030"}}
    }));
    let mut cached_scene = scene_for("/fixture/cached.jpg");
    wallpaper.attach_images(&mut cached_scene, Some(wake.clone()));
    assert!(
        cached_scene
            .asset_data
            .contains_key(&cached.canonicalize().unwrap())
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "a valid persisted thumbnail is ready without a catalogue/prewarm request"
    );

    let mut scene = scene_for("/fixture/current.jpg");
    wallpaper.attach_images(&mut scene, Some(wake.clone()));
    assert!(
        scene.primitives.is_empty(),
        "deleted cache bytes are deferred while hydration runs"
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/current.jpg"]})
        )
    );
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let expected = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../shell/sample.png")
        .canonicalize()
        .unwrap();
    let mut refreshed = scene_for("/fixture/current.jpg");
    wallpaper.attach_images(&mut refreshed, Some(wake.clone()));
    assert!(
        refreshed.asset_data.contains_key(&expected),
        "hydration restores an evicted/deleted thumbnail path"
    );

    let broken = lab.dir.join("still-broken.png");
    {
        let mut state = wallpaper.state.borrow_mut();
        state.catalogue_result.push(json!({
            "id":"/fixture/broken.jpg","kind":"image","title":"Broken",
            "thumbnail":broken.to_string_lossy(),"preview":"","active":false
        }));
    }
    let mut broken_scene = scene_for("/fixture/broken.jpg");
    wallpaper.attach_images(&mut broken_scene, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/broken.jpg"]})
        )
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut repeated = scene_for("/fixture/broken.jpg");
    wallpaper.attach_images(&mut repeated, Some(wake.clone()));
    assert!(repeated.primitives.is_empty());
    assert!(
        lab.calls.try_recv().is_err(),
        "a broken path returned by hydration is settled for the current visible set"
    );

    let mut changed_set = scene_for("/fixture/broken.jpg");
    changed_set.primitives.push(Primitive::image(
        Rect::new(0., 0., 8., 6.),
        std::path::PathBuf::from("@icon:wallpaper:thumbnail:0:/fixture/current.jpg"),
    ));
    wallpaper.attach_images(&mut changed_set, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.catalogue".into(),
            json!({"ids":["/fixture/broken.jpg"]})
        ),
        "a changed visible set requests only the image missing from the local cache"
    );
    for _ in 0..200 {
        wallpaper.project(&open, Some(wake.clone()));
        if wallpaper.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let mut stable_changed = scene_for("/fixture/broken.jpg");
    stable_changed.primitives.push(Primitive::image(
        Rect::new(0., 0., 8., 6.),
        std::path::PathBuf::from("@icon:wallpaper:thumbnail:0:/fixture/current.jpg"),
    ));
    wallpaper.attach_images(&mut stable_changed, Some(wake));
    assert!(
        lab.calls.try_recv().is_err(),
        "the unchanged new visible set remains settled"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_latest_activation_survives_view_retirement() {
    use bead::render::Scene;
    let mut lab = lab_with_activation_gate(vec!["wallpaper.active".into()], true).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&extras);
    let queue = extras.wallpaper_actions.as_ref().unwrap().clone();
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    extras.project(&open, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    for _ in 0..200 {
        extras.project(&open, Some(wake.clone()));
        if extras.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        extras.state.borrow().catalogue_result.len(),
        4,
        "activation fixture catalogue is ready"
    );
    extras.act(
        "wallpaper:activate:/fixture/current.jpg",
        &open,
        wake.clone(),
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.activate".into(),
            json!({"revision":17,"id":"/fixture/current.jpg"})
        )
    );
    extras.act("wallpaper:activate:/fixture/next.jpg", &open, wake.clone());
    assert_eq!(
        queue
            .lock()
            .unwrap()
            .inflight
            .as_ref()
            .map(|inflight| inflight.selection.id.as_str()),
        Some("/fixture/next.jpg"),
        "the latest click replaces the in-flight selection immediately"
    );

    let file = std::path::PathBuf::from("/cache/view-only.png");
    let bytes: Arc<[u8]> = Arc::from(vec![1, 2, 3, 4]);
    let weak = Arc::downgrade(&bytes);
    extras
        .state
        .borrow_mut()
        .images
        .insert(file.clone(), bytes.clone());
    let scene = Scene {
        asset_data: BTreeMap::from([(file, bytes.clone())]),
        layers: Vec::new(),
        font_files: Vec::new(),
        primitives: Vec::new(),
        ..Scene::empty(8., 8.)
    };
    drop(bytes);
    drop(scene);
    drop(extras);
    drop(wake);
    assert!(
        weak.upgrade().is_none(),
        "the activation queue must not retain view image data"
    );

    lab.activation_release.take().unwrap().send(()).unwrap();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        (
            "wallpaper.activate".into(),
            json!({"revision":17,"id":"/fixture/next.jpg"})
        ),
        "the new request proceeds after the serial fixture finishes the old one"
    );
    for _ in 0..200 {
        if queue.lock().unwrap().inflight.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let state = queue.lock().unwrap();
    assert!(state.inflight.is_none());
    assert_eq!(state.confirmed, Some(("extras-boot".into(), 18)));
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_activation_error_is_reported_without_retry_and_boot_change_clears_state() {
    let mut lab =
        lab_with_activation_behavior(vec!["wallpaper.active".into()], true, true, 0).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&extras);
    let queue = extras.wallpaper_actions.as_ref().unwrap().clone();
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    extras.project(&open, Some(wake.clone()));
    let _ = lab.calls.recv().await.unwrap();
    for _ in 0..200 {
        extras.project(&open, Some(wake.clone()));
        if extras.state.borrow().catalogue.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    extras.act(
        "wallpaper:activate:/fixture/current.jpg",
        &open,
        wake.clone(),
    );
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        (
            "wallpaper.activate".into(),
            json!({"revision":17,"id":"/fixture/current.jpg"})
        )
    );
    lab.activation_release.take().unwrap().send(()).unwrap();
    for _ in 0..200 {
        if queue.lock().unwrap().inflight.is_none() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let model = extras.project(&open, Some(wake));
    assert_eq!(
        model["extras.message"],
        Value::Text("Wallpaper selection failed".into())
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "failed activation does not retry without a newer user selection"
    );
    {
        let mut state = queue.lock().unwrap();
        assert!(state.inflight.is_none());
        state.confirmed = Some(("extras-boot".into(), 18));
        state.invalidate_boot(Some("new-boot"));
        assert!(state.confirmed.is_none());
        assert!(state.error.is_none());
    }
}

// Keep the fixture daemon schedulable while the UI performs synchronous staging.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wallpaper_runtime_search_and_next_button_dispatch_real_activation() {
    use bead::wayland::{SceneSource, UiEvent};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&extras);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let colors = bead::render::Palette::read(&root.join("palette.json")).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(
        root.clone(),
        "extras/main.luau".into(),
        root.join("palette.json"),
        colors,
        cold,
    );
    let mut runtime = crate::reload::Runtime::panel_session(session, true)
        .with_extras(extras)
        .unwrap();
    assert_eq!(
        runtime.session.stores.get("bead.focused"),
        Some(&Value::Text(String::new())),
        "wallpaper picker opens without assigning focus to search"
    );
    runtime.services(Box::new(|_| {}));
    assert!(runtime.viewport(1920, 1200));
    let loading_scene = runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .expect("cold catalogue loading must publish its loading scene immediately");
    assert!(
        loading_scene.primitives.iter().any(|primitive| matches!(
            primitive,
            bead::render::Primitive::Text { text, .. } if text == "Loading wallpapers…"
        )),
        "the first cold frame must show the loading message"
    );
    assert!(runtime.commit());
    let mut first_content = None;
    for _ in 0..200 {
        if let Some(scene) = runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap() {
            assert!(
                scene.asset_data.values().any(|bytes| !bytes.is_empty()),
                "first published content includes hydrated wallpaper bytes"
            );
            first_content = Some(scene);
            assert!(runtime.commit());
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    let initial_scene =
        first_content.expect("hydration eventually publishes the first content frame");
    let frame = crate::placement::Frame::current();
    let usable_width = (1920. - frame.left - frame.right).max(1.);
    let usable_height = (1200. - frame.top - frame.bottom).max(1.);
    assert_eq!(
        runtime.session.stores.get("extras.viewport.width"),
        Some(&Value::Number(usable_width as f64))
    );
    assert_eq!(
        runtime.session.stores.get("extras.viewport.height"),
        Some(&Value::Number(usable_height as f64))
    );
    let panel_root = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.action.as_deref() == Some("close"))
        .expect("picker root has the generic close action");
    let panel_rect = bead::ui::rect(&panel_root.draw);
    assert_eq!(
        (panel_rect.w, panel_rect.h),
        (usable_width, usable_height),
        "Strand fills the area inside the shell frame"
    );
    assert!(
        runtime.scroll_regions().is_empty(),
        "wheel ticks move the strand once instead of expanding into smooth scrolling"
    );
    let motion = initial_scene
        .motion
        .as_ref()
        .expect("picker uses the shared panel motion");
    assert!(
        !motion.reveal,
        "the strand runs its own entry, not the shared reveal"
    );
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    let mut loaded = false;
    for _ in 0..200 {
        runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
        runtime.commit();
        loaded = matches!(runtime.session.stores.get("extras.rows"),Some(Value::List(rows)) if rows.len()==4);
        if loaded {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        loaded,
        "the real picker must receive the asynchronous catalogue"
    );
    let first_card = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| {
            matches!(&node.action_arg,
            Some(Value::Text(argument)) if argument.starts_with("wallpaper:select:"))
        })
        .expect("wallpaper card remains focusable");
    let first_card_rect = bead::ui::rect(&first_card.draw);
    assert!(
        !runtime.dismiss_at(
            first_card_rect.x + first_card_rect.w / 2.,
            first_card_rect.y + first_card_rect.h / 2.,
        ),
        "clicking a card must not dismiss the picker"
    );
    let (method, params) = tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(method, "wallpaper.catalogue");
    assert!(!params["ids"].as_array().unwrap().is_empty());
    assert!(params["ids"].as_array().unwrap().len() <= 128);
    let fixture_image = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../shell/sample.png")
        .canonicalize()
        .unwrap();
    let mut hydrated_scene =
        Some(initial_scene).filter(|scene| scene.asset_data.contains_key(&fixture_image));
    for _ in 0..200 {
        if hydrated_scene.is_some() {
            break;
        }
        let scene = runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
        runtime.commit();
        if scene
            .as_ref()
            .is_some_and(|scene| scene.asset_data.contains_key(&fixture_image))
        {
            hydrated_scene = scene;
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        hydrated_scene.is_some(),
        "hydration must invalidate the Luau scene and attach bytes to a fresh runtime scene"
    );
    while let Ok((method, _)) = lab.calls.try_recv() {
        assert_eq!(
            method, "wallpaper.catalogue",
            "all catalogue hydration calls are settled before activation assertions"
        );
    }
    let search = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.id.ends_with(".search") && node.input.is_some())
        .expect("wallpaper search input is visible");
    let search_rect = bead::ui::rect(&search.draw);
    runtime
        .stage(UiEvent::Click {
            x: search_rect.x + search_rect.w / 2.,
            y: search_rect.y + search_rect.h / 2.,
        })
        .unwrap();
    runtime.commit();
    runtime.stage(UiEvent::Release).unwrap();
    runtime.commit();
    runtime
        .stage(UiEvent::Key {
            text: "Next".into(),
            backspace: false,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("extras.query"),
        Some(&Value::Text("Next".into()))
    );
    runtime
        .stage(UiEvent::Command {
            command: "enter".into(),
            shift: false,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(
        next_wallpaper_runtime_call(&mut runtime, &mut lab.calls).await,
        (
            "wallpaper.activate".into(),
            json!({"revision":17,"id":"/fixture/next.jpg"})
        ),
        "Enter keeps the chosen wallpaper at once"
    );
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&Value::Bool(false)),
        "and closes the picker"
    );
    runtime
        .session
        .stores
        .commit(BTreeMap::from([("panel.open".into(), Value::Bool(true))]))
        .unwrap();

    for _ in 0..4 {
        runtime
            .stage(UiEvent::Key {
                text: String::new(),
                backspace: true,
            })
            .unwrap();
        runtime.commit();
    }
    assert_eq!(
        runtime.session.stores.get("extras.query"),
        Some(&Value::Text(String::new()))
    );
    let navigation = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.action.as_deref() == Some("extras.next"))
        .expect("picker exposes an actionable next button");
    let rect = bead::ui::rect(&navigation.draw);
    runtime
        .stage(UiEvent::Click {
            x: rect.x + rect.w / 2.,
            y: rect.y + rect.h / 2.,
        })
        .unwrap();
    runtime.commit();
    runtime.stage(UiEvent::Release).unwrap();
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("extras.wallpaper.selection_id"),
        Some(&Value::Text("/fixture/next.jpg".into()))
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "a move only slides the strand; the wallpaper follows once it settles"
    );
    assert_eq!(
        next_wallpaper_runtime_call(&mut runtime, &mut lab.calls).await,
        (
            "wallpaper.activate".into(),
            json!({"revision":18,"id":"/fixture/next.jpg"})
        )
    );
    let card = runtime.session.view_nodes().iter().find(|node| {
        let rect = bead::ui::rect(&node.draw);
        matches!(&node.action_arg, Some(Value::Text(argument)) if argument.starts_with("wallpaper:select:"))
            && node.contains(rect.x + rect.w / 2., rect.y + rect.h / 2.)
    }).expect("a visible wallpaper card during selection travel");
    let card = bead::ui::rect(&card.draw);
    runtime
        .stage(UiEvent::Scroll {
            x: card.x + card.w / 2.,
            y: card.y + card.h / 2.,
            delta: 1.,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(
        next_wallpaper_runtime_call(&mut runtime, &mut lab.calls).await,
        (
            "wallpaper.activate".into(),
            json!({"revision":19,"id":"/fixture/next.mp4"})
        ),
        "positive Wayland wheel distance moves to the next wallpaper (videos share the strand)"
    );
    let root_node = runtime
        .session
        .view_nodes()
        .iter()
        .find(|node| node.action.as_deref() == Some("close"))
        .expect("wallpaper root uses the generic host close action");
    let root_rect = bead::ui::rect(&root_node.draw);
    let point = [(0.05, 0.15), (0.95, 0.15), (0.05, 0.85), (0.95, 0.85)]
        .into_iter()
        .map(|(x, y)| (root_rect.x + root_rect.w * x, root_rect.y + root_rect.h * y))
        .find(|(x, y)| {
            runtime
                .session
                .view_nodes()
                .iter()
                .filter(|node| node.action.is_some() && node.action.as_deref() != Some("close"))
                .all(|node| {
                    let rect = bead::ui::rect(&node.draw);
                    *x < rect.x || *x > rect.x + rect.w || *y < rect.y || *y > rect.y + rect.h
                })
        })
        .expect("picker has a background point outside child actions");
    assert!(
        runtime.dismiss_at(point.0, point.1),
        "the empty background still dismisses the picker"
    );
    runtime
        .stage(UiEvent::Click {
            x: point.0,
            y: point.1,
        })
        .unwrap();
    runtime.commit();
    runtime.stage(UiEvent::Release).unwrap();
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&Value::Bool(false))
    );
}

// Real wheel events reach Strand as whole +1 steps and must take the coalesced wheel path, not the arrow-key one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_wheel_burst_over_strand_steps_live_and_starts_one_switch_for_its_last_pick() {
    use bead::wayland::{SceneSource, UiEvent};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&extras);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let colors = bead::render::Palette::read(&root.join("palette.json")).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(
        root.clone(),
        "extras/main.luau".into(),
        root.join("palette.json"),
        colors,
        cold,
    );
    let mut runtime = crate::reload::Runtime::panel_session(session, true)
        .with_extras(extras)
        .unwrap();
    runtime.services(Box::new(|_| {}));
    assert!(runtime.viewport(1920, 1200));
    runtime.stage(UiEvent::Reload).unwrap();
    assert!(runtime.commit());
    for _ in 0..200 {
        if runtime
            .stage(UiEvent::Model(BTreeMap::new()))
            .unwrap()
            .is_some()
        {
            assert!(runtime.commit());
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    while let Ok((method, _)) = lab.calls.try_recv() {
        assert_eq!(method, "wallpaper.catalogue");
    }
    let card = runtime.session.view_nodes().iter().find(|node| {
        let rect = bead::ui::rect(&node.draw);
        matches!(&node.action_arg, Some(Value::Text(argument)) if argument.starts_with("wallpaper:select:"))
            && node.contains(rect.x + rect.w / 2., rect.y + rect.h / 2.)
    }).expect("a visible wallpaper card");
    let card = bead::ui::rect(&card.draw);
    let selection = |runtime: &crate::reload::Runtime| {
        runtime
            .session
            .stores
            .get("extras.wallpaper.selection_id")
            .cloned()
    };
    let mut picks = vec![selection(&runtime)];
    for _ in 0..3 {
        runtime
            .stage(UiEvent::Scroll {
                x: card.x + card.w / 2.,
                y: card.y + card.h / 2.,
                delta: 1.,
            })
            .unwrap();
        runtime.commit();
        picks.push(selection(&runtime));
        tokio::time::sleep(Duration::from_millis(30)).await;
    }
    assert!(
        picks.windows(2).all(|pair| pair[0] != pair[1]),
        "each wheel step advances the selection by one: {picks:?}"
    );
    assert!(lab.calls.try_recv().is_err(), "nothing switches mid-burst");
    tokio::time::sleep(WALLPAPER_WHEEL_RELEASE).await;
    let (method, params) = next_wallpaper_runtime_call(&mut runtime, &mut lab.calls).await;
    assert_eq!(method, "wallpaper.activate");
    assert_eq!(
        Some(Value::Text(params["id"].as_str().unwrap().into())),
        picks.pop().unwrap(),
        "one switch, for the last pick: {params} after {picks:?}"
    );
    assert!(lab.calls.try_recv().is_err(), "and only one");
}
// A wallpaper palette tween restages Strand at every step, with the bar: no Strand scene opts out of the steps
// (with the last step only, its `primary` thread snapped ~500 ms after the bar had eased: a second colour change),
// and each step only re-colours the scene; it never reprojects the strand (rows, images, caption).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn strand_takes_every_palette_step_without_reprojecting() {
    use bead::wayland::{SceneSource, UiEvent};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    start_with_empty_wallpaper_catalogue(&extras);
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let colors = bead::render::Palette::read(&root.join("palette.json")).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(
        root.clone(),
        "extras/main.luau".into(),
        root.join("palette.json"),
        colors.clone(),
        cold,
    );
    let mut runtime = crate::reload::Runtime::panel_session(session, true)
        .with_extras(extras)
        .unwrap();
    runtime.services(Box::new(|_| {}));
    assert!(runtime.viewport(1920, 1200));
    let mut scenes: Vec<_> = runtime
        .stage(UiEvent::Reload)
        .unwrap()
        .into_iter()
        .collect();
    runtime.commit();
    // The open storyboard and the catalogue's arrival, then a pick and its glide.
    let mut press = None;
    for frame in 0..400 {
        let loaded = matches!(runtime.session.stores.get("extras.rows"), Some(Value::List(rows)) if rows.len() == 4);
        if loaded && press.is_none() {
            press = Some(frame);
        }
        let event = if press == Some(frame) {
            UiEvent::Command {
                command: "right".into(),
                shift: false,
            }
        } else {
            UiEvent::Model(BTreeMap::new())
        };
        scenes.extend(runtime.stage(event).unwrap());
        runtime.commit();
        while lab.calls.try_recv().is_ok() {}
        if press.is_some_and(|press| frame > press + 40) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(8)).await;
    }
    assert!(press.is_some(), "the catalogue arrives");
    assert!(
        scenes.len() > 10 && scenes.iter().all(|scene| !scene.palette_at_rest),
        "every Strand scene takes each palette step ({} scenes)",
        scenes.len()
    );
    let rows = runtime.session.stores.get("extras.rows").cloned();
    let revision = runtime.session.stores.revision();
    for red in [40, 120, 200] {
        let step: BTreeMap<_, _> = colors
            .0
            .iter()
            .map(|(role, c)| (role.clone(), [red, c[1], c[2], c[3]]))
            .collect();
        let scene = runtime
            .stage(UiEvent::WallpaperPalette(step.clone()))
            .unwrap()
            .expect("a step re-colours Strand");
        runtime.commit();
        assert_eq!(
            strand_thread(&scene),
            Some(step["primary"]),
            "the thread takes it"
        );
    }
    assert_eq!(
        runtime.session.stores.revision(),
        revision,
        "without a projection"
    );
    assert_eq!(runtime.session.stores.get("extras.rows").cloned(), rows);
}
/// The colour the Strand thread is stroked in.
fn strand_thread(scene: &bead::render::Scene) -> Option<[u8; 4]> {
    let layer = scene.layers.iter().find(|l| l.id.ends_with(".thread"))?;
    scene.primitives[layer.own.clone()]
        .iter()
        .find_map(|p| match p {
            bead::render::Primitive::Polyline { color, .. } => Some(*color),
            _ => None,
        })
}
async fn next_wallpaper_runtime_call(
    runtime: &mut crate::reload::Runtime,
    calls: &mut mpsc::UnboundedReceiver<(String, Json)>,
) -> (String, Json) {
    use bead::wayland::{SceneSource, UiEvent};
    for _ in 0..200 {
        runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap();
        runtime.commit();
        if let Ok(call) = calls.try_recv() {
            if call.0 == "wallpaper.catalogue" {
                let ids = call.1["ids"]
                    .as_array()
                    .expect("hydration call carries ids");
                assert!(!ids.is_empty(), "metadata loading uses ids:[] only once");
                assert!(
                    ids.len() <= 128,
                    "hydration request stays within protocol bounds"
                );
                continue;
            }
            return call;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("wallpaper runtime did not issue its action")
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_thumbnail_cache_is_advertised_bounded_and_released_on_close() {
    use bead::render::{Primitive, Rect, Scene};
    let lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    let thumbnail = lab.dir.join("thumbnail.png");
    let unadvertised = lab.dir.join("unadvertised.png");
    let oversized = lab.dir.join("oversized.png");
    image::RgbaImage::from_pixel(8, 6, image::Rgba([12, 120, 200, 255]))
        .save(&thumbnail)
        .unwrap();
    image::RgbaImage::from_pixel(8, 6, image::Rgba([200, 60, 30, 255]))
        .save(&unadvertised)
        .unwrap();
    image::RgbaImage::from_pixel(1601, 1, image::Rgba([20, 30, 40, 255]))
        .save(&oversized)
        .unwrap();
    extras.state.borrow_mut().catalogue_result = vec![
        json!({
            "id":"/fixture/advertised.jpg","kind":"image","title":"Advertised",
            "thumbnail":thumbnail.to_string_lossy(),"preview":"","active":false
        }),
        json!({
            "id":"/fixture/oversized.jpg","kind":"image","title":"Oversized",
            "thumbnail":oversized.to_string_lossy(),"preview":"","active":false
        }),
    ];
    {
        let mut state = extras.state.borrow_mut();
        state.wallpaper_open = true;
        state.wallpaper_boot = Some("extras-boot".into());
        state.catalogue_boot = Some("extras-boot".into());
        state.catalogue_request_boot = Some("extras-boot".into());
        state.image_revision = 9;
        // A previously visible file must not remain warm after leaving the scene.
        assert!(
            cached_wallpaper_image(&mut state, &unadvertised, 16 * bead::core::cache::MIB)
                .is_some()
        );
    }

    let logical =
        |field: &str, id: &str| std::path::PathBuf::from(format!("@icon:wallpaper:{field}:{id}"));
    let mut scene = Scene {
        layers: Vec::new(),
        font_files: Vec::new(),
        primitives: vec![
            Primitive::image(
                Rect::new(0., 0., 8., 6.),
                logical("thumbnail", "/fixture/advertised.jpg"),
            ),
            Primitive::image(
                Rect::new(10., 0., 8., 6.),
                logical("thumbnail", "/fixture/unadvertised.jpg"),
            ),
            Primitive::image(
                Rect::new(20., 0., 8., 6.),
                logical("thumbnail", "/fixture/oversized.jpg"),
            ),
        ],
        ..Scene::empty(32., 8.)
    };
    extras.attach_images(&mut scene, None);
    assert!(scene.asset_data.contains_key(&thumbnail));
    assert_eq!(extras.state.borrow().images.len(), 1);
    assert!(extras.image_bytes() > 0 && extras.image_bytes() <= 16 * bead::core::cache::MIB);
    let bead::render::Primitive::Image { path: loaded, .. } = &scene.primitives[0] else {
        panic!("image primitive")
    };
    assert_eq!(loaded, &thumbnail);
    assert_eq!(
        scene.primitives.len(),
        1,
        "unadvertised and invalid images are deferred out of the scene"
    );

    let opened = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.query".into(), Value::Text("Advertised".into())),
            (
                "extras.wallpaper.selection_id".into(),
                Value::Text("/fixture/advertised.jpg".into()),
            ),
        ]))
        .unwrap();
    let opened_model = extras.project(&opened, None);
    let generation = opened_model["extras.wallpaper.resource_revision"].clone();
    let closed = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(false)),
            ("extras.query".into(), Value::Text("Advertised".into())),
            (
                "extras.wallpaper.selection_id".into(),
                Value::Text("/fixture/advertised.jpg".into()),
            ),
        ]))
        .unwrap();
    let closing_model = extras.project(&closed, None);
    assert!(matches!(&closing_model["extras.rows"], Value::List(rows) if rows.len()==2));
    assert_eq!(closing_model["extras.available"], Value::Bool(true));
    assert_eq!(
        closing_model["extras.wallpaper.resource_revision"],
        generation
    );
    assert!(
        !closing_model.contains_key("extras.query"),
        "query state remains unchanged during the slide-out"
    );
    assert!(!closing_model.contains_key("extras.wallpaper.selection_id"));
    assert!(
        extras.image_bytes() > 0,
        "encoded thumbnail bytes remain leased through the closing frame"
    );
    let weak = Arc::downgrade(scene.asset_data.values().next().unwrap());
    drop(scene);
    drop(extras);
    assert!(
        weak.upgrade().is_none(),
        "retiring Extras releases cached thumbnail bytes"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn wallpaper_scene_attachment_bytes_stay_within_the_cache_budget() {
    use bead::render::{Primitive, Rect, Scene};
    let lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    let shared: Arc<[u8]> = Arc::from(vec![0u8; 9 * bead::core::cache::MIB]);
    let entries: Vec<Json> = (0..3)
        .map(|index| {
            json!({
                "id":format!("/fixture/large-{index}.png"),"kind":"image",
                "title":format!("Large {index}"),"thumbnail":format!("/cache/thumb-{index}.png"),
                "preview":"","active":false
            })
        })
        .collect();
    {
        let mut state = extras.state.borrow_mut();
        state.wallpaper_open = true;
        state.catalogue_result = entries;
        for index in 0..3 {
            state.images.insert(
                std::path::PathBuf::from(format!("/cache/thumb-{index}.png")),
                shared.clone(),
            );
        }
    }
    let mut scene = Scene {
        layers: Vec::new(),
        font_files: Vec::new(),
        primitives: (0..3)
            .map(|index| {
                Primitive::image(
                    Rect::new(index as f32 * 8., 0., 8., 6.),
                    std::path::PathBuf::from(format!(
                        "@icon:wallpaper:thumbnail:/fixture/large-{index}.png"
                    )),
                )
            })
            .collect(),
        ..Scene::empty(24., 8.)
    };
    extras.attach_images(&mut scene, None);
    let attached_bytes: usize = scene.asset_data.values().map(|bytes| bytes.len()).sum();
    assert!(attached_bytes <= 16 * bead::core::cache::MIB);
    assert_eq!(
        scene.asset_data.len(),
        1,
        "the second shared large image would exceed the per-scene byte budget"
    );
}
#[test]
fn extras_goldens() -> bead_test::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    for kind in [Kind::Overview, Kind::Palette, Kind::Keybindings] {
        for scale in [1., 1.5] {
            let (windows, workspaces) = compositor_rows(&fixture());
            let rows = if kind == Kind::Keybindings {
                vec![
                    json!({"id":"k1","key":"Super + K","description":"Keyboard shortcuts"}),
                    json!({"id":"k2","key":"Super + Tab","description":"Windows"}),
                ]
            } else {
                // The first rows QML's empty palette shows (DesktopActions order, N0058).
                [
                    ("wallpaper", "Open wallpaper picker", "wallpaper"),
                    ("calculator", "Calculator", "calculate"),
                    ("colors", "Open Nacre Colors", "palette"),
                    ("settings", "Open Nacre Settings", "settings"),
                    ("ai", "Open Nacre AI", "smart_toy"),
                    ("brain", "Open Nacre Brain", "neurology"),
                    ("files", "Open files…", "folder_open"),
                ]
                .map(|(id, name, icon)| json!({"id":id,"name":name,"description":"Action","icon":icon,"kind":"action"}))
                .to_vec()
            };
            let mut h = bead_test::Harness::load_with_icons(
                &root,
                std::path::Path::new("extras/main.luau"),
                &root.join("palette.json"),
                BTreeMap::from([
                    ("panel.open".into(), Value::Bool(true)),
                    ("extras.kind".into(), Value::Text(kind.name().into())),
                    ("extras.windows".into(), Value::list(windows)),
                    ("extras.workspaces".into(), Value::list(workspaces)),
                    ("extras.rows".into(), Value::list(rows)),
                ]),
                bead_test::Config {
                    width: 1280,
                    height: 1000,
                    scale,
                },
                BTreeMap::from([
                    ("editor".into(), root.join("tests/icon.svg")),
                    ("preferences-system".into(), root.join("tests/icon.svg")),
                ]),
            )?;
            let frame = h.render()?;
            let path = root.join(format!("tests/goldens/extras-{}-{scale}.png", kind.name()));
            if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
                frame.save(&path)?;
            }
            frame.compare(
                &path,
                Default::default(),
                &std::env::temp_dir().join(format!("extras-{}-{scale}-diff", kind.name())),
            )?;
        }
    }
    Ok(())
}
/// Twelve distinct 320x200 gradients stand in for the catalogue's thumbnails.
fn strand_thumbnails(dir: &std::path::Path, count: usize) -> BTreeMap<String, std::path::PathBuf> {
    std::fs::create_dir_all(dir).unwrap();
    (0..count)
        .map(|index| {
            let [r, g, b] = strand_colour(index, 1.);
            let image = image::RgbaImage::from_fn(320, 200, |x, y| {
                let light = 1.15 - y as f32 / 200. * 0.6 + x as f32 / 320. * 0.15;
                let channel = |value: u8| (f32::from(value) * light).clamp(0., 255.) as u8;
                image::Rgba([channel(r), channel(g), channel(b), 255])
            });
            let path = dir.join(format!("thumb-{index:02}.png"));
            image.save(&path).unwrap();
            (
                format!("wallpaper:thumbnail:0:/fixture/wallpaper-{index:02}"),
                path,
            )
        })
        .collect()
}
fn strand_colour(index: usize, light: f32) -> [u8; 3] {
    // HSV with hue stepping 37 degrees per wallpaper, saturation .65 and value .9.
    let hue = (index * 37 % 360) as f32 / 60.;
    let channel = |n: f32| {
        let k = (n + hue) % 6.;
        let value = 0.9 - 0.9 * 0.65 * k.min(4. - k).clamp(0., 1.);
        (value * light * 255.).clamp(0., 255.) as u8
    };
    [channel(5.), channel(3.), channel(1.)]
}
fn strand_rows(count: usize, kind: &str) -> Value {
    const NAMES: [&str; 12] = [
        "Aurora",
        "Coast",
        "Crimson Wood",
        "Blue Layers",
        "Blossom Lake",
        "Canyon",
        "Forest",
        "Galaxy",
        "Glacier",
        "Lagoon",
        "Moss",
        "Orchid",
    ];
    Value::list(
        (0..count)
            .map(|index| {
                let hex = |light: f32| {
                    let [r, g, b] = strand_colour(index, light);
                    format!("#{r:02x}{g:02x}{b:02x}")
                };
                json!({
                    "id":format!("/fixture/wallpaper-{index:02}"),
                    "kind":if kind == "mixed" && index % 3 == 1 {"video"} else {"image"},
                    "title":if count == 47 && index == 11 {"Silksong - Battle Flames - 4K".to_owned()} else if index < NAMES.len() {NAMES[index].to_owned()} else {format!("Wallpaper {index:02}")},
                    "thumbnail":format!("wallpaper:thumbnail:0:/fixture/wallpaper-{index:02}"),
                    "preview":"",
                    "active":index == 5,
                    "palette":{"mode":"dark","colours":{
                        "background":"#0b0c10","onSurface":"#f0f0f0","primary":hex(1.),
                        "secondary":hex(0.7),"tertiary":hex(1.2),"primaryContainer":hex(0.5)}}
                })
            })
            .collect(),
    )
}
/// The picker over `count` wallpapers; `position` (the strand's unwrapped slot) may sit between two.
fn strand_harness(
    count: usize,
    kind: &str,
    query: &str,
    position: f64,
    logical: (u32, u32),
    scale: f32,
    thumbnails: &std::path::Path,
) -> bead_test::Result<bead_test::Harness> {
    strand_harness_with(
        "palette.json",
        count,
        kind,
        query,
        position,
        logical,
        scale,
        thumbnails,
    )
}
/// `strand_harness` under another shell palette (a file of shell/).
#[allow(clippy::too_many_arguments)]
fn strand_harness_with(
    palette: &str,
    count: usize,
    kind: &str,
    query: &str,
    position: f64,
    logical: (u32, u32),
    scale: f32,
    thumbnails: &std::path::Path,
) -> bead_test::Result<bead_test::Harness> {
    crate::i18n::init(); // the caption's strings
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let frame = crate::placement::Frame::DEFAULT;
    let selected = format!("/fixture/wallpaper-{:02}", position.floor() as usize - 1);
    let stores = BTreeMap::from([
        ("panel.open".into(), Value::Bool(true)),
        ("extras.kind".into(), Value::Text("wallpaper".into())),
        ("extras.available".into(), Value::Bool(true)),
        ("extras.message".into(), Value::Text(String::new())),
        ("extras.query".into(), Value::Text(query.into())),
        ("extras.rows".into(), strand_rows(count, kind)),
        ("extras.wallpaper.loading".into(), Value::Bool(false)),
        (
            "extras.wallpaper.selection_id".into(),
            Value::Text(selected.clone()),
        ),
        ("extras.wallpaper.position".into(), Value::Number(position)),
        ("extras.wallpaper.glide".into(), Value::Bool(false)),
        ("extras.wallpaper.visit".into(), Value::Number(1.)),
        // Pinned at the end of the entrance: the goldens show the strand open (the markers are not stepped here).
        ("extras.wallpaper.entry_ms".into(), Value::Number(250.)),
        (
            "extras.wallpaper.resolution_id".into(),
            Value::Text(selected),
        ),
        (
            "extras.wallpaper.resolution".into(),
            Value::Text("3840 × 2160".into()),
        ),
        (
            "extras.viewport.width".into(),
            Value::Number(f64::from(logical.0) - f64::from(frame.left + frame.right)),
        ),
        (
            "extras.viewport.height".into(),
            Value::Number(f64::from(logical.1) - f64::from(frame.top + frame.bottom)),
        ),
    ]);
    bead_test::Harness::load_with_icons(
        &root,
        std::path::Path::new("extras/main.luau"),
        &root.join(palette),
        stores,
        bead_test::Config {
            width: logical.0 - 20 + 64,
            height: logical.1 - 60 + 64,
            scale,
        },
        strand_thumbnails(thumbnails, count),
    )
}
fn pearl_images(harness: &bead_test::Harness) -> usize {
    harness
        .scene()
        .primitives
        .iter()
        .filter(|primitive| {
            matches!(primitive, bead::render::Primitive::Image { path, .. }
            if path.file_name().is_some_and(|name| name.to_string_lossy().starts_with("thumb-")))
        })
        .count()
}
#[test]
fn wallpaper_strand_goldens() -> bead_test::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let thumbnails = std::env::temp_dir().join(format!("strand-goldens-{}", std::process::id()));
    // (name, kind, query, strand position, logical size, scale, entries)
    for (name, kind, query, position, logical, scale, count) in [
        ("rest", "mixed", "", 6., (1440, 900), 1., 12),
        ("rest", "mixed", "", 6., (1920, 1200), 1.5, 12),
        ("wide", "mixed", "", 6., (1920, 1080), 1., 12),
        ("mid", "mixed", "", 6.5, (1440, 900), 1., 12),
        ("empty", "mixed", "zzz", 1., (1440, 900), 1., 12),
        ("search", "mixed", "o", 3., (1440, 900), 1., 12),
        // The owner's library: 47 entries and a long name that must stop at the arrows.
        ("long", "mixed", "", 12., (1920, 1200), 1.5, 47),
        // The light scheme (sky wallpaper's palette): dark thread and halo, pill and caption readable.
        ("light", "mixed", "", 6., (1920, 1200), 1.5, 12),
    ] {
        let palette = if name == "light" {
            "tests/palette-light.json"
        } else {
            "palette.json"
        };
        let mut harness = strand_harness_with(
            palette,
            count,
            kind,
            query,
            position,
            logical,
            scale,
            &thumbnails,
        )?;
        let pearls = pearl_images(&harness);
        if name == "empty" {
            assert_eq!(pearls, 0, "no result, no pearls");
        } else {
            assert!(
                pearls >= 9,
                "the strand shows its pearls before a golden is approved"
            );
        }
        assert!(
            harness
                .scene()
                .primitives
                .iter()
                .any(|primitive| matches!(primitive,
                bead::render::Primitive::Text { text, .. } if text == "shuffle")),
            "the pill must render before approving a golden"
        );
        let frame = harness.render()?;
        let file = format!("wallpaper-strand-{name}-{scale}.png");
        let path = root.join("tests/goldens").join(&file);
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            frame.save(&path)?;
        }
        frame.compare(
            &path,
            Default::default(),
            &std::env::temp_dir().join(format!("{file}-diff")),
        )?;
    }
    let _ = std::fs::remove_dir_all(&thumbnails);
    Ok(())
}
/// The entrance pinned mid-way (`extras.wallpaper.entry_ms`): everything together, part faded and part dropped.
#[test]
fn wallpaper_strand_entry_goldens() -> bead_test::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let thumbnails = std::env::temp_dir().join(format!("strand-entry-{}", std::process::id()));
    let mut harness = strand_harness(12, "mixed", "", 6., (1440, 900), 1., &thumbnails)?;
    harness.update(BTreeMap::from([(
        "extras.wallpaper.entry_ms".into(),
        Value::Number(100.),
    )]))?;
    let frame = harness.render()?;
    let file = "wallpaper-strand-open-100-1.png";
    let path = root.join("tests/goldens").join(file);
    if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
        frame.save(&path)?;
    }
    frame.compare(
        &path,
        Default::default(),
        &std::env::temp_dir().join(format!("{file}-diff")),
    )?;
    let _ = std::fs::remove_dir_all(&thumbnails);
    Ok(())
}
#[tokio::test(flavor = "current_thread")]
async fn the_strands_wrapped_left_side_is_requested_with_the_rest_of_the_window() {
    use bead::render::{Primitive, Rect, Scene};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    {
        let mut state = extras.state.borrow_mut();
        state.catalogue_result = (0..36)
            .map(|index| json!({"id":format!("/fixture/w-{index:02}"),"kind":"image","active":index == 0}))
            .collect();
    }
    // Open at index 0 of 36: the left pearls are the catalogue's tail, 31..=35.
    let window: Vec<usize> = [31, 32, 33, 34, 35, 0, 1, 2, 3, 4, 5].into();
    let mut scene = Scene {
        layers: Vec::new(),
        font_files: Vec::new(),
        primitives: window
            .iter()
            .map(|index| {
                Primitive::image(
                    Rect::new(0., 0., 8., 6.),
                    std::path::PathBuf::from(format!(
                        "@icon:wallpaper:thumbnail:/fixture/w-{index:02}"
                    )),
                )
            })
            .collect(),
        ..Scene::empty(8., 6.)
    };
    extras.attach_images(&mut scene, Some(wake));
    let (method, params) = lab.calls.recv().await.unwrap();
    assert_eq!(method, "wallpaper.catalogue");
    let ids: Vec<&str> = params["ids"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Json::as_str)
        .collect();
    for index in &window {
        assert!(
            ids.contains(&format!("/fixture/w-{index:02}").as_str()),
            "pearl {index} must be requested: {ids:?}"
        );
    }
    let _ = open;
}
#[test]
fn the_first_pearl_requests_its_left_neighbours_from_the_catalogue_tail() -> bead_test::Result<()> {
    let thumbnails = std::env::temp_dir().join(format!("strand-wrap-{}", std::process::id()));
    // On open at index 0 of 36 the strand's left side is the tail of the catalogue: 31..=35.
    let harness = strand_harness(36, "mixed", "", 1., (1440, 900), 1., &thumbnails)?;
    let names: Vec<String> = harness
        .scene()
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            bead::render::Primitive::Image { path, .. } => path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .filter(|name| name.starts_with("thumb-")),
            _ => None,
        })
        .collect();
    for tail in 31..36 {
        assert!(
            names.contains(&format!("thumb-{tail}.png")),
            "pearl {tail} must be requested on open at index 0: {names:?}"
        );
    }
    assert_eq!(names.len(), 11);
    let _ = std::fs::remove_dir_all(&thumbnails);
    Ok(())
}
#[test]
fn a_big_catalogue_renders_at_most_eleven_pearls() -> bead_test::Result<()> {
    let thumbnails = std::env::temp_dir().join(format!("strand-count-{}", std::process::id()));
    for position in [1., 20., 46.5, 47.] {
        let harness = strand_harness(47, "mixed", "", position, (1440, 900), 1., &thumbnails)?;
        assert!(
            pearl_images(&harness) <= 11,
            "47 wallpapers at position {position} must render at most 11 pearl images"
        );
    }
    let at_rest = strand_harness(47, "mixed", "", 20., (1440, 900), 1., &thumbnails)?;
    assert_eq!(pearl_images(&at_rest), 11);
    let _ = std::fs::remove_dir_all(&thumbnails);
    Ok(())
}
/// The per-frame Strand geometry main.luau computed before N2 (its point_at, reach and size_at, numbers from
/// tokens.luau), the reference its keyframe tracks must follow.
struct OldStrand {
    s: f64,
    centre: (f64, f64),
    a: f64,
}
impl OldStrand {
    const SIZES: [f64; 7] = [280., 150., 108., 84., 68., 56., 48.];
    fn new(width: f64, height: f64) -> Self {
        let s = (width / 1440.).min(height / 864.);
        let centre = (width / 2., height / 2.);
        let (half, sag) = (centre.0 + 140. * s, centre.1 + 30. * s);
        let (mut lo, mut hi) = (50. * s, 5000. * s);
        for _ in 0..60 {
            let mid = (lo + hi) / 2.;
            if mid * ((half / mid).cosh() - 1.) > sag {
                lo = mid
            } else {
                hi = mid
            }
        }
        Self {
            s,
            centre,
            a: (lo + hi) / 2.,
        }
    }
    fn size_at(&self, d: f64) -> f64 {
        let i = (d.floor() as usize).min(Self::SIZES.len() - 2);
        (Self::SIZES[i] + (Self::SIZES[i + 1] - Self::SIZES[i]) * (d - i as f64)) * self.s
    }
    fn arc(&self, offset: f64) -> f64 {
        let step = |k: f64| (self.size_at(k) + self.size_at(k + 1.)) / 2. + 22. * self.s;
        let (d, mut length, mut k) = (offset.abs(), 0., 0.);
        while d - k > 1. {
            length += step(k);
            k += 1.;
        }
        offset.signum() * (length + (d - k) * step(k))
    }
    fn point_at(&self, arc: f64) -> (f64, f64) {
        let a = self.a;
        (
            self.centre.0 + a * (arc / a).asinh(),
            self.centre.1 + a - (a * a + arc * arc).sqrt(),
        )
    }
    /// The pearls the old loop placed at driver value `d`, by offset: (unwrapped slot, offset).
    fn pearls(count: usize, d: f64) -> Vec<(i64, f64)> {
        let (n, wrap) = (count as f64, |i: i64| (i - 1).rem_euclid(count as i64));
        let mut placed: Vec<(i64, f64)> = vec![];
        for index in (d - 5.).floor() as i64..=(d + 5.).floor() as i64 + 1 {
            let offset = ((wrap(index) + 1) as f64 - d + n / 2.).rem_euclid(n) - n / 2.;
            if offset.abs() <= 5. && !placed.iter().any(|(i, _)| wrap(*i) == wrap(index)) {
                placed.push(((d + offset).round() as i64, offset));
            }
        }
        placed.sort_by(|l, r| l.1.total_cmp(&r.1));
        placed
    }
}
fn strand_layer<'a>(
    scene: &'a bead::render::Scene,
    id: &str,
) -> Option<&'a bead::render::SceneLayer> {
    scene
        .layers
        .iter()
        .find(|l| l.id.ends_with(&format!(".{id}")))
}
fn strand_image(scene: &bead::render::Scene, id: &str) -> Option<(bead::render::Rect, f32)> {
    let layer = strand_layer(scene, id)?;
    scene.primitives[layer.own.clone()]
        .iter()
        .find_map(|p| match p {
            bead::render::Primitive::Image { rect, opacity, .. } => Some((*rect, *opacity)),
            _ => None,
        })
}
/// Whether everything layer `id` draws is transparent.
fn strand_unseen(scene: &bead::render::Scene, id: &str) -> bool {
    use bead::render::Primitive;
    strand_layer(scene, id).is_none_or(|layer| {
        scene.primitives[layer.own.start..layer.subtree_end]
            .iter()
            .all(|p| match p {
                Primitive::Image { opacity, .. } => *opacity < 0.01,
                Primitive::Rounded { fill, border, .. } => {
                    fill[3] <= 2 && border.is_none_or(|(c, _)| c[3] <= 2)
                }
                Primitive::Arc { color, .. } | Primitive::Text { color, .. } => color[3] <= 2,
                _ => true,
            })
    })
}
/// The old geometry at the driver value the patched `scene` shows (read off pearl `to`'s place): pearl centres and
/// sizes, knots and the ring within 0.5 px, image opacities within 0.01; pearls and knots the old loop did not draw are
/// transparent. The layout rounds each part's rest rect (at `to`) to whole pixels and the tracks move and scale that
/// rect, so the reference carries the same rounding. The reading is good to ~1e-3 slot: a part that steps (appears,
/// goes, drops its shadow or sheen) that close to it is not judged. Returns the driver value.
fn check_strand_frame(
    scene: &bead::render::Scene,
    old: &OldStrand,
    count: usize,
    to: f64,
) -> Result<f64, String> {
    // The strand box sits at the panel's origin.
    let origin = strand_layer(scene, "strand").ok_or("no strand")?.bounds;
    let centre = |r: bead::render::Rect| {
        (
            f64::from(r.x - origin.x + r.w / 2.),
            f64::from(r.y - origin.y + r.h / 2.),
        )
    };
    let edge = 5_f64.min(count as f64 / 2.);
    let snap = |left: f64, size: f64| {
        let (a, b) = (left.round(), (left + size).round());
        ((a + b) / 2., b - a)
    };
    let pearl_at = |o: f64| {
        let (x, y) = old.point_at(old.arc(o));
        (x, y, old.size_at(o.abs()))
    };
    // Pearl i at driver d: centre x, centre y, width, height.
    let pearl = |i: i64, d: f64| {
        let (rx, ry, rs) = pearl_at((i as f64 - to).clamp(-edge, edge));
        let (x, y, size) = pearl_at(i as f64 - d);
        let ((cx, w), (cy, h)) = (snap(rx - rs / 2., rs), snap(ry - rs / 2., rs));
        (cx + x - rx, cy + y - ry, w * size / rs, h * size / rs)
    };
    // Pearl `to`'s x falls as the driver grows.
    let target = to as i64;
    let seen = centre(
        strand_layer(scene, &format!("pearl-{target}"))
            .ok_or("no target")?
            .bounds,
    )
    .0;
    let (mut lo, mut hi) = (to - 4., to + 4.);
    for _ in 0..60 {
        let mid = (lo + hi) / 2.;
        if pearl(target, mid).0 > seen {
            lo = mid
        } else {
            hi = mid
        }
    }
    let d = (lo + hi) / 2.;
    let mut wrong = vec![];
    let near = |what: String, got: f64, want: f64, tolerance: f64| {
        ((got - want).abs() > tolerance).then(|| format!("{what}: {got:.3} vs {want:.3}"))
    };
    let pearls = OldStrand::pearls(count, d);
    let unsure = |o: f64| [edge, 2.5, 0.7].iter().any(|c| (o.abs() - c).abs() < 2e-3);
    for &(i, o) in pearls.iter().filter(|p| !unsure(p.1)) {
        let (x, y, w, h) = pearl(i, d);
        let size = old.size_at(o.abs());
        let alpha = 1. - 0.5 * (o.abs() - 4.).clamp(0., 1.);
        let rect = strand_layer(scene, &format!("pearl-{i}"))
            .ok_or(format!("d={d}: no pearl {i}"))?
            .bounds;
        let (_, opacity) =
            strand_image(scene, &format!("pearl-{i}.thumbnail")).ok_or("no thumbnail")?;
        for (what, got, want) in [
            ("x", centre(rect).0, x),
            ("y", centre(rect).1, y),
            ("width", f64::from(rect.w), w),
            ("height", f64::from(rect.h), h),
        ] {
            wrong.extend(near(format!("pearl {i} {what}"), got, want, 0.5));
        }
        wrong.extend(near(
            format!("pearl {i} alpha"),
            f64::from(opacity),
            alpha,
            0.01,
        ));
        let inner = o.abs() <= 2.5;
        let glow = alpha * (size / (280. * old.s) + 0.35).min(1.);
        let sheen = (1. - o.abs()).max(0.);
        for (part, want) in [
            ("shadow", if inner { glow } else { 0. }),
            ("shade", if inner { alpha } else { 0. }),
            ("sheen", if sheen > 0.3 { sheen * alpha } else { 0. }),
        ] {
            let got =
                strand_image(scene, &format!("pearl-{i}.{part}")).map_or(0., |(_, o)| f64::from(o));
            wrong.extend(near(format!("pearl {i} {part}"), got, want, 0.01));
        }
    }
    let knot_drawn = |i: i64| {
        pearls
            .windows(2)
            .any(|w| w[0].0 == i && w[1].1 - w[0].1 < 1.5)
    };
    for layer in &scene.layers {
        let Some((_, id)) = layer.id.rsplit_once('.') else {
            continue;
        };
        let Some((kind, slot)) = id
            .split_once('-')
            .filter(|(k, _)| ["pearl", "knot"].contains(k))
        else {
            continue;
        };
        let i: i64 = slot.parse().map_err(|_| format!("odd id {id}"))?;
        let o = i as f64 - d;
        let (drawn, unsure) = if kind == "knot" {
            (knot_drawn(i), unsure(o) || unsure(o + 1.))
        } else {
            (pearls.iter().any(|p| p.0 == i), unsure(o))
        };
        if !drawn && !unsure && !strand_unseen(scene, id) {
            wrong.push(format!("{id} is drawn"));
        }
    }
    let knot = (5. * old.s).max(3.);
    let knot_at = |o: f64| {
        let (l, r) = (o.clamp(-edge, edge), (o + 1.).clamp(-edge, edge));
        old.point_at(
            ((old.arc(l) + old.size_at(l.abs()) / 2.) + (old.arc(r) - old.size_at(r.abs()) / 2.))
                / 2.,
        )
    };
    for &(i, o) in pearls.iter().filter(|p| knot_drawn(p.0)) {
        let (rx, ry) = knot_at(i as f64 - to);
        let (x, y) = knot_at(o);
        let got = centre(
            strand_layer(scene, &format!("knot-{i}"))
                .ok_or(format!("no knot {i}"))?
                .bounds,
        );
        wrong.extend(near(
            format!("knot {i} x"),
            got.0,
            snap(rx - knot / 2., knot).0 + x - rx,
            0.5,
        ));
        wrong.extend(near(
            format!("knot {i} y"),
            got.1,
            snap(ry - knot / 2., knot).0 + y - ry,
            0.5,
        ));
    }
    let stroke = (2. * old.s).max(1.);
    let ring_at = |nearest: f64| old.size_at(nearest) + 14. * old.s - stroke;
    let rest = ring_at((to - to.round()).abs());
    let nearest = pearls.iter().fold(5_f64, |n, p| n.min(p.1.abs()));
    let width = snap(old.centre.0 - rest / 2., rest).1 * ring_at(nearest) / rest;
    let alpha = (255. * (1. - nearest * 1.6).clamp(0., 1.)).floor();
    let ring = strand_layer(scene, "ring").ok_or("no ring")?;
    for p in &scene.primitives[ring.own.start..ring.subtree_end] {
        if let bead::render::Primitive::Arc { rect, color, .. } = p {
            wrong.extend(near("ring width".into(), f64::from(rect.w), width, 0.5));
            wrong.extend(near("ring alpha".into(), f64::from(color[3]), alpha, 2.6));
        }
    }
    if wrong.is_empty() {
        Ok(d)
    } else {
        Err(format!("d={d}: {}", wrong.join("; ")))
    }
}
/// A Strand move to `to` (a tap, or a glide), a frame sampled every 40 ms (glide: 15 ms) for `sample` from the move's
/// build, so the probe stays light next to other tests: (frames, layout passes during the frames, time per frame, the
/// move's build time: behaviour, Luau, view_ir and layout).
fn strand_move(
    harness: &mut bead_test::Harness,
    count: usize,
    to: f64,
    glide: bool,
    sample: Duration,
    mut frame: impl FnMut(&bead::render::Scene),
) -> bead_test::Result<(usize, usize, Duration, Duration)> {
    let started = Instant::now();
    let built = harness.session.update(BTreeMap::from([
        ("extras.wallpaper.position".into(), Value::Number(to)),
        (
            "extras.wallpaper.selection_id".into(),
            Value::Text(format!(
                "/fixture/wallpaper-{:02}",
                (to as i64 - 1).rem_euclid(count as i64)
            )),
        ),
        ("extras.wallpaper.glide".into(), Value::Bool(glide)),
    ]))?;
    let build = started.elapsed();
    harness.session.commit(built.expect("a move rebuilds"));
    let layouts = harness.counters().layouts;
    let (mut frames, mut spent) = (0, Duration::ZERO);
    while frames == 0 || started.elapsed() < sample {
        std::thread::sleep(Duration::from_millis(if glide { 15 } else { 40 }));
        let at = Instant::now();
        let scene = harness.session.relayout()?.expect("a frame");
        harness.session.commit(scene.clone());
        spent += at.elapsed();
        frames += 1;
        frame(&scene);
    }
    Ok((
        frames,
        harness.counters().layouts - layouts,
        spent / frames.max(1) as u32,
        build,
    ))
}
/// N2: a Strand move is built once, then every frame takes Bead's patch path (no Luau, no layout), and each frame's
/// pearls, knots, ring and image opacities equal the old per-frame geometry at that driver value: taps and glides
/// both ways, a move across a whole slot (the formulas' kinks), a retarget mid-move, the wrap at the list's end and a
/// short list where a pearl jumps ends.
#[test]
fn strand_moves_patch_every_frame_and_follow_the_old_geometry() -> bead_test::Result<()> {
    let thumbnails = std::env::temp_dir().join(format!("strand-tracks-{}", std::process::id()));
    let frame = crate::placement::Frame::DEFAULT;
    let old = OldStrand::new(
        1440. - f64::from(frame.left + frame.right),
        900. - f64::from(frame.top + frame.bottom),
    );
    let mut failures = vec![];
    let (mut moves, mut frames, mut layouts, mut inside, mut spent, mut builds) =
        (0, 0, 0, 0, Duration::ZERO, Duration::ZERO);
    // (wallpapers, rest, [(to, glide, sampled ms)])
    for (count, rest, steps) in [
        (
            12,
            6.,
            vec![
                (7., false, 600),
                (6., false, 600),
                (7., true, 150),
                (6., true, 150),
                (8., false, 600),
                (9., false, 120),
                (10., false, 600),
                (12., false, 600),
                (13., false, 600),
            ],
        ),
        (4, 1., vec![(2., false, 600), (1., false, 600)]),
    ] {
        let mut harness = strand_harness(count, "mixed", "", rest, (1440, 900), 1., &thumbnails)?;
        for (to, glide, ms) in steps {
            let mut interior = 0;
            let stats = strand_move(
                &mut harness,
                count,
                to,
                glide,
                Duration::from_millis(ms),
                |scene| match check_strand_frame(scene, &old, count, to) {
                    Ok(d) if (d - to).abs() > 1e-3 => interior += 1,
                    Ok(_) => {}
                    Err(error) => failures.push(format!("{count} -> {to}: {error}")),
                },
            )?;
            eprintln!(
                "N2 move {count}: -> {to} glide={glide}: build {:?}, {} frames",
                stats.3, stats.0
            );
            moves += 1;
            frames += stats.0;
            layouts += stats.1;
            spent += stats.2;
            builds += stats.3;
            inside += interior;
        }
    }
    eprintln!(
        "N2 strand: {moves} moves, {frames} frames ({inside} mid-move), {layouts} layout passes during frames, \
         {:.2} ms per frame, {:.1} ms per move build (this build profile)",
        spent.as_secs_f64() * 1e3 / moves as f64,
        builds.as_secs_f64() * 1e3 / moves as f64
    );
    assert_eq!(layouts, 0, "a move's frames must patch: no Luau, no layout");
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    assert!(inside >= 10, "too few mid-move frames sampled: {inside}");
    let _ = std::fs::remove_dir_all(&thumbnails);
    Ok(())
}
/// B0372: a move's patched frames reuse every pearl image's raster. A scale track keeps a bake_size image's raster key,
/// so no image is baked again from one patched frame to the next (live N2 re-baked every pearl image per frame).
#[test]
fn a_strand_move_reuses_its_image_rasters_on_patched_frames() -> bead_test::Result<()> {
    use bead::render::{
        Primitive,
        tree::{ImagePolicy, TreeCache},
    };
    let thumbnails = std::env::temp_dir().join(format!("strand-bakes-{}", std::process::id()));
    let mut harness = strand_harness(12, "mixed", "", 6., (1440, 900), 1., &thumbnails)?;
    let icons = strand_thumbnails(&thumbnails, 12);
    // The frames of a tap, prepared afterwards: the cache sees each patched scene in order.
    let mut scenes = Vec::new();
    strand_move(
        &mut harness,
        12,
        7.,
        false,
        Duration::from_millis(450),
        |scene| scenes.push(scene.clone()),
    )?;
    let mut cache = TreeCache::default().with_image_policy(ImagePolicy::UploadOnly);
    let mut previous: Vec<Option<std::sync::Arc<bead::render::tree::ImagePayload>>> = Vec::new();
    let (mut baked, mut moved, mut rects) = (Vec::new(), 0, Vec::new());
    for mut images in scenes {
        // Only the images, with their bytes, as the host prepares them.
        images
            .primitives
            .retain(|p| matches!(p, Primitive::Image { .. }));
        images.layers.clear();
        images.clips.clear();
        for primitive in &mut images.primitives {
            if let Primitive::Image { path, .. } = primitive {
                if let Some(icon) = path.to_str().and_then(|p| p.strip_prefix("@icon:")) {
                    *path = icons[icon].clone();
                }
                if !images.asset_data.contains_key(&*path) {
                    let bytes = std::fs::read(&*path)?;
                    images.asset_data.insert(path.clone(), bytes.into());
                }
            }
        }
        // Mid-move frames: the pearls' image rects scale and slide between them.
        let now_rects: Vec<_> = images
            .primitives
            .iter()
            .map(|p| match p {
                Primitive::Image { rect, .. } => (rect.x, rect.y, rect.w),
                _ => (0., 0., 0.),
            })
            .collect();
        moved += usize::from(!rects.is_empty() && now_rects != rects);
        rects = now_rects;
        cache.prepare(&images, 1.)?;
        let frame = cache.frame.as_ref().ok_or("no frame")?;
        let now: Vec<_> = frame.items.iter().map(|item| item.image.clone()).collect();
        if !previous.is_empty() {
            // B0377: a zero-opacity image not hydrated yet has no item (it prefetches behind the frame) and
            // gets one when the resource worker finishes it, between any two frames: that is its first bake.
            // A re-bake is new or lost pixels for an image that already had some.
            let rebaked = |(now, before): &(&Option<_>, &Option<_>)| match (now, before) {
                (Some(a), Some(b)) => !std::sync::Arc::ptr_eq(a, b),
                (None, Some(_)) => true,
                _ => false,
            };
            baked.push(now.iter().zip(&previous).filter(rebaked).count());
        }
        previous = now;
    }
    eprintln!(
        "N2 image bakes per patched frame: {baked:?} ({} images)",
        previous.len()
    );
    assert!(
        moved >= 3,
        "too few mid-move frames: {moved} of {}",
        baked.len()
    );
    assert!(
        baked.iter().all(|n| *n == 0),
        "images baked again on patched frames: {baked:?}"
    );
    let _ = std::fs::remove_dir_all(&thumbnails);
    Ok(())
}
/// The longest shuffle of a 48-wallpaper strand (24 slots, the far-part cap) builds within Bead's track and markup
/// budgets and still patches its frames.
#[test]
fn a_long_strand_jump_stays_within_the_track_budget() -> bead_test::Result<()> {
    let thumbnails = std::env::temp_dir().join(format!("strand-jump-{}", std::process::id()));
    let mut harness = strand_harness(48, "mixed", "", 1., (1440, 900), 1., &thumbnails)?;
    let (frames, layouts, frame, build) = strand_move(
        &mut harness,
        48,
        25.,
        false,
        Duration::from_millis(100),
        |_| {},
    )?;
    eprintln!("N2 long jump: build {build:?}, {frames} frames at {frame:?}");
    assert_eq!(layouts, 0);
    let _ = std::fs::remove_dir_all(&thumbnails);
    Ok(())
}
/// Shuffles and glides at the owner's 1920x1200 @1.5 keep their images' textures well within Bead's 64 MiB
/// native-gl-images cap: live N2 baked every pearl a 24-slot shuffle carried past the centre at full size (~29 x 1.2 MB
/// in its first frame) and the cap refused uploads. Every move still lands on rasters at least the size they are drawn.
#[test]
fn strand_shuffles_and_glides_stay_within_the_image_texture_budget() -> bead_test::Result<()> {
    use bead::render::Primitive;
    let thumbnails = std::env::temp_dir().join(format!("strand-textures-{}", std::process::id()));
    let scale = 1.5;
    let mut harness = strand_harness(48, "mixed", "", 1., (1920, 1200), scale, &thumbnails)?;
    let images = |scene: &bead::render::Scene| {
        let mut out = Vec::new();
        for primitive in &scene.primitives {
            if let Primitive::Image {
                path,
                rect,
                bake_size,
                tint,
                ..
            } = primitive
            {
                out.push((
                    path.clone(),
                    *rect,
                    bake_size.unwrap_or((rect.w, rect.h)),
                    *tint,
                ));
            }
        }
        out
    };
    // Physical px at 4 bytes: one texture per distinct raster (B0375) and, as the GL kept them before, per image.
    let need = |scene: &bead::render::Scene| {
        let rasters: Vec<_> = images(scene)
            .into_iter()
            .map(|(path, _, (w, h), tint)| {
                (
                    path,
                    (w * scale).ceil() as usize,
                    (h * scale).ceil() as usize,
                    tint,
                )
            })
            .collect();
        let bytes = |(_, w, h, _): &(_, usize, usize, _)| w * h * 4;
        let distinct: std::collections::BTreeSet<_> = rasters.iter().collect();
        (
            distinct.into_iter().map(bytes).sum::<usize>(),
            rasters.iter().map(bytes).sum::<usize>(),
        )
    };
    // The most a frame's image is drawn up from its raster (1 = sharp).
    let softest = |scene: &bead::render::Scene| {
        images(scene)
            .iter()
            .map(|(_, rect, (w, _), _)| rect.w / w)
            .fold(0f32, f32::max)
    };
    let (mut peak, mut landed) = (need(harness.scene()), Vec::new());
    let mut track = |scene: &bead::render::Scene| {
        let (distinct, items) = need(scene);
        peak = (peak.0.max(distinct), peak.1.max(items));
    };
    // Shuffles both ways and again, a held key retargeting mid-glide, then a tap; the long samples reach the rest.
    for (to, glide, sample) in [
        (25., false, 700),
        (1., false, 700),
        (25., false, 700),
        (26., true, 40),
        (27., true, 40),
        (28., true, 300),
        (30., false, 700),
    ] {
        let mut last = None;
        strand_move(
            &mut harness,
            48,
            to,
            glide,
            Duration::from_millis(sample),
            |scene| {
                track(scene);
                last = Some(softest(scene));
            },
        )?;
        if sample >= 300 {
            landed.push(last.ok_or("no frame")?);
        }
    }
    let mib = |bytes: usize| bytes as f64 / 1048576.;
    eprintln!(
        "Strand image textures per frame: peak {:.1} MiB distinct, {:.1} MiB one per image; landed draw/raster {landed:?}",
        mib(peak.0),
        mib(peak.1)
    );
    assert!(
        peak.0 < 24 << 20,
        "a frame's images need {} bytes of textures",
        peak.0
    );
    // At rest the centre pearl draws ~0.15 % over its raster (layout rounding, as before N2); a small raster under the
    // centre pearl would draw 3.3x.
    assert!(
        landed.iter().all(|soft| *soft <= 1.01),
        "a move landed on an upscaled image: {landed:?}"
    );
    let _ = std::fs::remove_dir_all(&thumbnails);
    Ok(())
}
async fn open_picker(lab: &mut Lab) -> (Extras, Stores, CallWake) {
    open_picker_on(lab, None).await
}
async fn open_picker_on(
    lab: &mut Lab,
    niri_socket: Option<std::path::PathBuf>,
) -> (Extras, Stores, CallWake) {
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    extras.state.borrow_mut().niri_socket = niri_socket;
    start_with_empty_wallpaper_catalogue(&extras);
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    extras.project(&open, Some(wake.clone()));
    assert_eq!(
        lab.calls.recv().await.unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    for _ in 0..200 {
        extras.project(&open, Some(wake.clone()));
        let ready = {
            let state = extras.state.borrow();
            state.catalogue.is_none() && state.wallpaper_origin.is_some()
        };
        if ready {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(extras.state.borrow().catalogue_result.len(), 4);
    assert_eq!(
        extras.state.borrow().wallpaper_origin.as_deref(),
        Some("/fixture/current.jpg"),
        "the picker remembers the wallpaper it opened over"
    );
    (extras, open, wake)
}
fn closed(_open: &Stores) -> Stores {
    Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(false)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap()
}
async fn activated(lab: &mut Lab) -> Json {
    let (method, params) = tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
        .await
        .expect("an activation")
        .unwrap();
    assert_eq!(method, "wallpaper.activate");
    params["id"].clone()
}
fn thumbnail_scene(handles: &[&str]) -> bead::render::Scene {
    use bead::render::{Primitive, Rect, Scene};
    Scene {
        layers: Vec::new(),
        font_files: Vec::new(),
        primitives: handles
            .iter()
            .map(|id| {
                Primitive::image(
                    Rect::new(0., 0., 8., 6.),
                    std::path::PathBuf::from(format!("@icon:wallpaper:thumbnail:{id}")),
                )
            })
            .collect(),
        ..Scene::empty(8., 6.)
    }
}
#[tokio::test(flavor = "current_thread")]
async fn a_thumbnail_that_settles_empty_is_retried_twice_then_given_up() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    let blank = "/fixture/next.jpg";
    let handle = format!("thumbnail:{blank}");
    // Its file is not there (yet) and the reply does not change that.
    let unready = |extras: &Extras| {
        extras.state.borrow_mut().catalogue_result.iter_mut().for_each(|entry| {
            entry["thumbnail"] = "/fixture/not-written-yet.jpg".into();
            entry["palette"] = json!({"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}});
        });
    };
    unready(&extras);
    let settle = |extras: &Extras| extras.state.borrow().catalogue.is_none();
    let mut requests = 0;
    // First sight, then each due retry: one request apiece; a not-yet-due look asks nothing.
    for (wait, asks) in [
        (0, true),
        (0, false),
        (60, true),
        (100, true),
        (100, false),
        (100, false),
    ] {
        tokio::time::sleep(Duration::from_millis(wait)).await;
        extras.project(&open, Some(wake.clone()));
        extras.attach_images(&mut thumbnail_scene(&[blank]), Some(wake.clone()));
        let got = tokio::time::timeout(Duration::from_millis(150), lab.calls.recv())
            .await
            .ok()
            .flatten();
        {
            let state = extras.state.borrow();
            assert_eq!(
                got.is_some(),
                asks,
                "step after {wait} ms: {got:?} settled={:?} retries={:?} pending={} open={}",
                state.catalogue_settled_handles,
                state.thumb_retries,
                state.catalogue.is_some(),
                state.wallpaper_open
            );
        }
        if let Some((method, params)) = got {
            assert_eq!(method, "wallpaper.catalogue");
            assert_eq!(params["ids"], json!([blank]));
            requests += 1;
            for _ in 0..200 {
                extras.project(&open, Some(wake.clone()));
                if settle(&extras) {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            unready(&extras);
        }
    }
    assert_eq!(requests, 3, "the first ask and exactly two retries");
    assert_eq!(
        extras.state.borrow().thumb_retries[&handle].0,
        THUMB_RETRIES + 1
    );
    assert!(
        extras.thumb_retry_at().is_none(),
        "nothing left to wake for"
    );
    extras.project(&closed(&open), Some(wake));
    assert!(
        extras.state.borrow().thumb_retries.is_empty(),
        "the count resets on close"
    );
}
#[tokio::test(flavor = "current_thread")]
async fn a_thumbnail_that_loads_is_never_retried() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    let file = std::env::temp_dir().join(format!("loaded-thumb-{}.png", std::process::id()));
    image::RgbaImage::from_pixel(4, 4, image::Rgba([9, 9, 9, 255]))
        .save(&file)
        .unwrap();
    {
        let mut state = extras.state.borrow_mut();
        let entry = state
            .catalogue_result
            .iter_mut()
            .find(|entry| entry["id"] == "/fixture/next.jpg")
            .unwrap();
        entry["thumbnail"] = file.to_string_lossy().into_owned().into();
        entry["palette"] = json!({"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}});
    }
    for _ in 0..4 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        extras.project(&open, Some(wake.clone()));
        extras.attach_images(
            &mut thumbnail_scene(&["/fixture/next.jpg"]),
            Some(wake.clone()),
        );
        assert!(
            lab.calls.try_recv().is_err(),
            "a loaded thumbnail asks for nothing"
        );
    }
    assert!(extras.state.borrow().thumb_retries.is_empty());
    let _ = std::fs::remove_file(file);
}
/// A fake nacre-niri: answers the first request with `reply`, then reports whether the connection
/// stayed open (`true` once the client is gone, i.e. EOF).
fn fake_niri(
    name: &str,
    reply: &'static str,
) -> (
    std::path::PathBuf,
    std::sync::mpsc::Receiver<String>,
    std::sync::mpsc::Receiver<bool>,
) {
    use std::io::{BufRead, Read, Write};
    let path = std::env::temp_dir().join(format!("extras-peek-{name}-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
    let (request_tx, request_rx) = std::sync::mpsc::channel();
    let (eof_tx, eof_rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let Ok((mut stream, _)) = listener.accept() else {
            return;
        };
        let mut line = String::new();
        std::io::BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        request_tx.send(line).unwrap();
        writeln!(stream, "{reply}").unwrap();
        let mut byte = [0u8; 1];
        let _ = eof_tx.send(stream.read(&mut byte).map(|n| n == 0).unwrap_or(false));
    });
    (path, request_rx, eof_rx)
}
#[tokio::test(flavor = "current_thread")]
async fn the_peek_is_held_while_the_picker_is_open_and_dropped_on_every_close() {
    for close in ["esc", "commit"] {
        let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
        let (socket, request, eof) = fake_niri(close, r#"{"Ok":"Handled"}"#);
        let (extras, open, wake) = open_picker_on(&mut lab, Some(socket.clone())).await;
        assert_eq!(
            request.recv_timeout(Duration::from_secs(2)).unwrap().trim(),
            r#"{"NacrePeekDesktop":{"output":null}}"#,
            "one peek for the focused output, asked when the picker opens"
        );
        extras.project(&open, Some(wake.clone()));
        assert!(
            eof.recv_timeout(Duration::from_millis(200)).is_err(),
            "still held while open"
        );
        if close == "commit" {
            extras.act("wallpaper:commit:/fixture/current.jpg", &open, wake.clone());
            extras.project(&open, Some(wake.clone()));
        }
        extras.project(&closed(&open), Some(wake));
        assert_eq!(
            eof.recv_timeout(Duration::from_secs(2)),
            Ok(true),
            "closing by {close} releases the hold"
        );
        let _ = std::fs::remove_file(socket);
    }
}
#[test]
fn the_peek_asks_every_output_or_else_the_focused_one() {
    assert_eq!(
        peek_outputs(&json!({"outputs":{"DP-1":{},"eDP-1":{}}})),
        vec![Some("DP-1".to_owned()), Some("eDP-1".to_owned())]
    );
    assert_eq!(peek_outputs(&json!({"outputs":{}})), vec![None]);
}
/// A busy compositor answers the peek late: opening must not wait for it (it blocked the main thread up to 100 ms).
#[tokio::test(flavor = "current_thread")]
async fn a_slow_peek_answer_never_blocks_the_open() {
    use std::io::{BufRead, Write};
    let lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let socket = std::env::temp_dir().join(format!("extras-peek-slow-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut line = String::new();
        std::io::BufReader::new(stream.try_clone().unwrap())
            .read_line(&mut line)
            .unwrap();
        std::thread::sleep(Duration::from_millis(90));
        writeln!(stream, r#"{{"Ok":"Handled"}}"#).unwrap();
        std::thread::sleep(Duration::from_millis(500)); // keep the hold until the test is done with it
    });
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    extras.state.borrow_mut().niri_socket = Some(socket.clone());
    start_with_empty_wallpaper_catalogue(&extras);
    let open = Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap();
    let wake: CallWake = Arc::new(|| {});
    let started = Instant::now();
    extras.project(&open, Some(wake.clone()));
    assert!(
        started.elapsed() < Duration::from_millis(60),
        "the open does not wait for the compositor's answer"
    );
    for _ in 0..100 {
        extras.project(&open, Some(wake.clone()));
        if extras.state.borrow().peek.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        extras.state.borrow().peek.is_some(),
        "the late answer is collected"
    );
    server.join().unwrap();
    let _ = std::fs::remove_file(socket);
}
#[tokio::test(flavor = "current_thread")]
async fn the_wallpaper_fade_is_warm_exactly_while_the_picker_is_open() {
    use bead::wayland::ShellControl;
    for close in ["esc", "commit"] {
        let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
        let (extras, open, wake) = open_picker(&mut lab).await;
        let gate = crate::shell_control::Gate::new();
        let woken = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = woken.clone();
        gate.watch(Box::new(move || {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));
        extras.project(&closed(&open), Some(wake.clone()));
        extras.state.borrow_mut().warm_gate = Some(gate.clone());
        assert!(!gate.wallpaper_warm());
        extras.project(&open, Some(wake.clone()));
        assert!(
            !gate.wallpaper_warm(),
            "the hold waits for the open storyboard"
        );
        extras.state.borrow_mut().warm_due = Some(Instant::now());
        extras.project(&open, Some(wake.clone()));
        assert!(
            gate.wallpaper_warm(),
            "the settled storyboard takes the hold"
        );
        assert_eq!(woken.load(std::sync::atomic::Ordering::SeqCst), 1);
        if close == "commit" {
            extras.act("wallpaper:commit:/fixture/current.jpg", &open, wake.clone());
        }
        extras.project(&closed(&open), Some(wake));
        assert!(!gate.wallpaper_warm(), "closing by {close} drops it");
        assert_eq!(woken.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
}
#[tokio::test(flavor = "current_thread")]
async fn an_error_reply_holds_nothing_and_an_empty_desktop_switches_no_workspace() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (socket, request, _eof) = fake_niri("err", r#"{"Err":"unknown request"}"#);
    let (extras, open, wake) = open_picker_on(&mut lab, Some(socket.clone())).await;
    assert!(request.recv_timeout(Duration::from_secs(2)).is_ok());
    assert!(extras.state.borrow().peek.is_none(), "stock niri: no hold");
    // The fixture's focused workspace has no active flag, so there is nothing to switch away from.
    extras.project(&closed(&open), Some(wake));
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(lab.calls.try_recv().is_err(), "no workspace was switched");
    let _ = std::fs::remove_file(socket);
}
/// A held arrow while frames are slow: the repeats (every 62 ms for 3 s) reach the strand in bursts at each 60 ms
/// frame, then the key is lost. Backlogged moves collapse, nothing runs on after the last real event, a tap moves once.
#[test]
fn a_held_arrow_never_runs_away_when_frames_are_slow() {
    let t0 = Instant::now();
    let at = |ms: u64| t0 + Duration::from_millis(ms);
    // Real repeats are due every 62 ms; a busy loop only processes events at multiples of 120 ms (two frames).
    let mut gate = StepGate::default();
    let (mut accepted, mut last_due) = (Vec::new(), 0);
    let mut due = 0;
    while due <= 3000 {
        last_due = due;
        due += 62;
    }
    let mut pending = 0;
    let mut next_due = 0;
    for frame in (0..=3000 + 600).step_by(120) {
        while next_due <= last_due && next_due <= frame {
            pending += 1;
            next_due += 62;
        }
        for _ in 0..pending {
            if gate.admit(at(frame)) {
                accepted.push(frame);
            }
        }
        pending = 0;
    }
    // At most one step per processed burst: no catch-up after the last real event.
    assert!(accepted.len() <= 3600 / 120 + 1, "{accepted:?}");
    assert!(
        accepted.iter().all(|f| *f <= last_due + 120),
        "no step after the release: {accepted:?}"
    );
    // A tap long after is not blocked, and two moves inside the window collapse to one.
    assert!(gate.admit(at(10_000)));
    assert!(!gate.admit(at(10_000 + 30)));
    assert!(gate.admit(at(10_000 + 62)));
}
#[test]
fn the_fallback_shows_an_empty_workspace_and_leaves_an_already_empty_one_alone() {
    let busy = json!({"focus":{"outputId":"OUT"},
        "workspaces":{"a":{"index":1,"outputId":"OUT","active":true},"b":{"index":2,"outputId":"OUT"},"c":{"index":1,"outputId":"OTHER","active":true}},
        "windows":{"w":{"workspaceId":"a"},"x":{"workspaceId":"c"}}});
    assert_eq!(
        peek_workspace_plan(&busy),
        Some((json!("OUT"), 1, 2, "a".into()))
    );
    let empty_now = json!({"focus":{"outputId":"OUT"},
        "workspaces":{"a":{"index":1,"outputId":"OUT","active":true},"b":{"index":2,"outputId":"OUT"}},
        "windows":{}});
    assert_eq!(
        peek_workspace_plan(&empty_now),
        None,
        "already empty: do nothing"
    );
    let no_spare = json!({"focus":{"outputId":"OUT"},
        "workspaces":{"a":{"index":1,"outputId":"OUT","active":true}},
        "windows":{"w":{"workspaceId":"a"}}});
    assert_eq!(peek_workspace_plan(&no_spare), None);
}
#[tokio::test(flavor = "current_thread")]
async fn the_fallback_returns_to_the_previous_workspace_on_close() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    extras.state.borrow_mut().peek = Some(Peek::Workspace {
        output: json!("OUT"),
        previous: 3,
    });
    extras.project(&closed(&open), Some(wake));
    let (method, params) = tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(method, "workspace.activate");
    assert_eq!(params, json!({"output":"OUT","workspace":3}));
}
#[tokio::test(flavor = "current_thread")]
async fn a_move_activates_at_once_with_a_500_ms_out_quint_fade() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    extras.act("wallpaper:move:/fixture/next.jpg", &open, wake);
    assert_eq!(
        activated(&mut lab).await,
        "/fixture/next.jpg",
        "no settle: the step activates when it is made"
    );
    if let Some(preview) = extras.wallpaper_preview() {
        let fade = preview.take_fade().expect("the strand asked for a fade");
        assert_eq!(fade.kind, bead::wayland::WallpaperFadeKind::Fade);
        assert_eq!(fade.duration_ms, 500);
        assert_eq!(fade.curve, bead::core::animation::Curve::OutQuint);
        assert_eq!(
            preview.take_fade(),
            Some(fade),
            "a retarget gets the same fade"
        );
    }
}
#[tokio::test(flavor = "current_thread")]
async fn five_quick_moves_end_on_the_last_id() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    for id in [
        "next.jpg",
        "next.mp4",
        "animated.gif",
        "next.jpg",
        "animated.gif",
    ] {
        extras.act(
            &format!("wallpaper:move:/fixture/{id}"),
            &open,
            wake.clone(),
        );
    }
    // The first move fired at once; its fade is the one in flight, the rest wait for its end.
    tokio::time::sleep(Duration::from_millis(WALLPAPER_MOVE_MS.into())).await;
    extras.fire_pending_move();
    let (mut last, mut calls) = (None, 0);
    while let Ok(Some((method, params))) =
        tokio::time::timeout(Duration::from_millis(300), lab.calls.recv()).await
    {
        assert_eq!(method, "wallpaper.activate");
        last = Some(params["id"].clone());
        calls += 1;
    }
    assert_eq!(
        last.as_ref().and_then(Json::as_str),
        Some("/fixture/animated.gif")
    );
    assert_eq!(calls, 2, "the first step, then one follow to the latest");
}
#[tokio::test(flavor = "current_thread")]
async fn a_step_mid_fade_waits_for_its_end_and_one_after_it_does_not() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    extras.act("wallpaper:move:/fixture/next.jpg", &open, wake.clone());
    extras.state.borrow_mut().wallpaper_fade = Duration::from_millis(120);
    tokio::time::sleep(Duration::from_millis(40)).await;
    // Mid-fade: held back, not a second transition; the latest of several wins.
    extras.act("wallpaper:move:/fixture/next.mp4", &open, wake.clone());
    extras.act("wallpaper:move:/fixture/animated.gif", &open, wake.clone());
    extras.fire_pending_move();
    assert_eq!(activated(&mut lab).await, "/fixture/next.jpg");
    assert!(
        lab.calls.try_recv().is_err(),
        "nothing else starts while the fade runs"
    );
    let ends = extras.thumb_retry_at().expect("the follow is scheduled");
    assert!(ends > Instant::now(), "its deadline is the fade's end");
    tokio::time::sleep(Duration::from_millis(100)).await;
    extras.fire_pending_move();
    assert_eq!(activated(&mut lab).await, "/fixture/animated.gif");
    // A tap after the fade ended starts the next one at once, as before.
    extras.state.borrow_mut().wallpaper_fade = Duration::from_millis(30);
    tokio::time::sleep(Duration::from_millis(60)).await;
    extras.act("wallpaper:move:/fixture/next.jpg", &open, wake);
    assert_eq!(activated(&mut lab).await, "/fixture/next.jpg");
}
#[tokio::test(flavor = "current_thread")]
async fn a_wheel_burst_starts_one_switch_on_the_last_pick_after_its_quiet_window() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    for id in ["next.jpg", "animated.gif", "next.mp4"] {
        extras.act(
            &format!("wallpaper:wheel:/fixture/{id}"),
            &open,
            wake.clone(),
        );
        tokio::time::sleep(Duration::from_millis(30)).await;
        extras.fire_pending_move();
    }
    assert!(lab.calls.try_recv().is_err(), "nothing starts mid-burst");
    tokio::time::sleep(WALLPAPER_WHEEL_RELEASE).await;
    extras.fire_pending_move();
    assert_eq!(activated(&mut lab).await, "/fixture/next.mp4");
    assert!(lab.calls.try_recv().is_err(), "one switch for the burst");
    assert!(extras.state.borrow().wallpaper_pending.is_none());
}
#[tokio::test(flavor = "current_thread")]
async fn a_three_second_hold_retargets_the_wallpaper_at_the_cap_and_ends_on_the_last_id() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    let ids = ["next.jpg", "animated.gif", "next.mp4"];
    let (mut fires, mut seen) = (0, None);
    for step in 0..90 {
        extras.act(
            &format!("wallpaper:move:/fixture/{}", ids[step % 3]),
            &open,
            wake.clone(),
        );
        tokio::time::sleep(Duration::from_millis(33)).await;
        extras.fire_pending_move(); // the runtime's deadline wake
        let fired = extras.state.borrow().wallpaper_fired_at;
        if fired != seen {
            (fires, seen) = (fires + 1, fired);
        }
    }
    tokio::time::sleep(WALLPAPER_RELEASE).await;
    extras.fire_pending_move(); // the landing
    // 3 s: the 500 ms leading fade, then one follow per 180 ms fade (about 14), and the landing.
    assert!((10..=19).contains(&fires), "a 3 s hold: {fires} retargets");
    assert_eq!(
        extras.state.borrow().wallpaper_shown.as_deref(),
        Some("/fixture/next.mp4"),
        "the hold ends on the last pearl"
    );
    assert!(extras.state.borrow().wallpaper_pending.is_none());
}
#[tokio::test(flavor = "current_thread")]
async fn the_predecode_window_is_replanned_only_where_a_move_lands() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    extras.act("wallpaper:move:/fixture/next.jpg", &open, wake.clone());
    assert!(
        !extras.state.borrow().wallpaper_prefetched,
        "the first move fired"
    );
    extras.state.borrow_mut().wallpaper_prefetched = true;
    extras.act("wallpaper:move:/fixture/animated.gif", &open, wake.clone());
    assert!(
        extras.state.borrow().wallpaper_prefetched,
        "a held-back move leaves the plan alone"
    );
    extras.state.borrow_mut().wallpaper_fade = Duration::from_millis(60);
    tokio::time::sleep(Duration::from_millis(80)).await;
    let fired = extras.state.borrow().wallpaper_fired_at;
    extras.act("wallpaper:move:/fixture/next.mp4", &open, wake);
    assert_ne!(
        extras.state.borrow().wallpaper_fired_at,
        fired,
        "a glide retarget fired"
    );
    assert!(
        extras.state.borrow().wallpaper_prefetched,
        "and left the plan alone too"
    );
    tokio::time::sleep(Duration::from_millis(200)).await;
    extras.fire_pending_move();
    assert!(
        !extras.state.borrow().wallpaper_prefetched,
        "the landing re-planned it"
    );
}
#[tokio::test(flavor = "current_thread")]
async fn a_pending_pick_lands_when_esc_closes_and_nothing_restores_the_origin() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    extras.act("wallpaper:move:/fixture/next.jpg", &open, wake.clone());
    extras.act("wallpaper:move:/fixture/animated.gif", &open, wake.clone());
    assert_eq!(activated(&mut lab).await, "/fixture/next.jpg");
    extras.project(&closed(&open), Some(wake));
    assert_eq!(
        activated(&mut lab).await,
        "/fixture/animated.gif",
        "the held-back pick lands"
    );
    tokio::time::sleep(WALLPAPER_RETARGET_CAP).await;
    extras.fire_pending_move();
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(
        lab.calls.try_recv().is_err(),
        "once, and the origin never returns"
    );
}
#[tokio::test(flavor = "current_thread")]
async fn a_landed_pick_stays_on_esc_and_enter_adds_no_second_activation() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    // Open and close without moving: nothing is activated.
    extras.project(&closed(&open), Some(wake.clone()));
    assert!(
        lab.calls.try_recv().is_err(),
        "an untouched visit activates nothing"
    );
    extras.project(&open, Some(wake.clone()));
    extras.act("wallpaper:move:/fixture/next.jpg", &open, wake.clone());
    assert_eq!(activated(&mut lab).await, "/fixture/next.jpg");
    extras.project(&closed(&open), Some(wake.clone()));
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(lab.calls.try_recv().is_err(), "Esc leaves the pick");
    extras.project(&open, Some(wake.clone()));
    extras.act("wallpaper:move:/fixture/animated.gif", &open, wake.clone());
    assert_eq!(activated(&mut lab).await, "/fixture/animated.gif");
    extras.act(
        "wallpaper:commit:/fixture/animated.gif",
        &open,
        wake.clone(),
    );
    let values = extras.project(&open, Some(wake.clone()));
    assert_eq!(values["panel.open"], Value::Bool(false), "Enter closes");
    extras.project(&closed(&open), Some(wake));
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(
        lab.calls.try_recv().is_err(),
        "the pick had landed: no second activation"
    );
}
#[tokio::test(flavor = "current_thread")]
async fn a_key_burst_holds_the_palette_until_its_landing() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    if let Some(queue) = &extras.wallpaper_actions {
        queue.lock().unwrap().preview = Some(Arc::new(crate::wallpaper::Preview::default()));
    }
    extras.act("wallpaper:move:/fixture/next.jpg", &open, wake.clone());
    assert!(
        extras.state.borrow().palette_visit.is_none(),
        "a single press lands at once"
    );
    extras.act("wallpaper:move:/fixture/animated.gif", &open, wake.clone());
    extras.state.borrow_mut().wallpaper_fade = Duration::from_millis(1);
    tokio::time::sleep(Duration::from_millis(5)).await;
    extras.act("wallpaper:move:/fixture/next.mp4", &open, wake.clone());
    assert!(
        extras.state.borrow().palette_visit.is_some(),
        "a glide fire holds the palette"
    );
    tokio::time::sleep(WALLPAPER_RETARGET_CAP + WALLPAPER_RELEASE).await;
    extras.fire_pending_move();
    assert!(
        extras.state.borrow().palette_visit.is_none(),
        "the landing ends the hold"
    );
}
/// A close asks for at most one palette change: after a burst's landing (on its last glide pick: the one re-ask for
/// the real palette) it asks for nothing more, and a close that cuts a burst short lands it with that one re-ask.
#[tokio::test(flavor = "current_thread")]
async fn a_strand_close_asks_for_at_most_one_palette_change() {
    use std::sync::atomic::{AtomicU32, Ordering::SeqCst};
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let (extras, open, wake) = open_picker(&mut lab).await;
    let preview = Arc::new(crate::wallpaper::Preview::default());
    let asked = Arc::new(AtomicU32::new(0));
    let counter = asked.clone();
    preview.set_changed(Arc::new(move || {
        counter.fetch_add(1, SeqCst);
    }));
    extras
        .wallpaper_actions
        .as_ref()
        .unwrap()
        .lock()
        .unwrap()
        .preview = Some(preview.clone());
    let primary = |red: u8| {
        bead::render::Palette(Arc::new(std::collections::HashMap::from([(
            "primary".to_owned(),
            [red, 0, 0, 255],
        )])))
    };
    // A tap that lands, then a burst whose glide pick shows with the held palette.
    let burst = async |lab: &mut Lab| {
        extras.act("wallpaper:move:/fixture/animated.gif", &open, wake.clone());
        assert_eq!(activated(lab).await, "/fixture/animated.gif");
        extras.act("wallpaper:move:/fixture/next.jpg", &open, wake.clone());
        extras.state.borrow_mut().wallpaper_fade = Duration::from_millis(1);
        tokio::time::sleep(Duration::from_millis(5)).await;
        extras.act("wallpaper:move:/fixture/next.mp4", &open, wake.clone());
        assert_eq!(activated(lab).await, "/fixture/next.mp4");
        assert!(
            extras.state.borrow().palette_visit.is_some(),
            "the glide holds"
        );
        preview.frame_palette("DP-1", primary(9), Some(primary(1)));
    };
    // Every ask is a frame request (a selection's or a re-ask): a possible palette change.
    let asks = || asked.load(SeqCst);
    burst(&mut lab).await;
    tokio::time::sleep(WALLPAPER_RETARGET_CAP + WALLPAPER_RELEASE).await;
    let before = asks();
    extras.fire_pending_move();
    assert_eq!(
        asks() - before,
        1,
        "the landing on the glide pick re-asks once"
    );
    extras.project(&closed(&open), Some(wake.clone()));
    assert_eq!(
        asks() - before,
        1,
        "a landed visit's close asks for nothing"
    );
    tokio::time::sleep(WALLPAPER_RETARGET_CAP + WALLPAPER_RELEASE).await;
    extras.project(&open, Some(wake.clone()));
    burst(&mut lab).await;
    let before = asks();
    extras.project(&closed(&open), Some(wake.clone()));
    assert_eq!(
        asks() - before,
        1,
        "a close mid-burst lands it with one re-ask"
    );
    tokio::time::sleep(Duration::from_millis(60)).await;
    assert!(
        lab.calls.try_recv().is_err(),
        "and no activation after close"
    );
}
#[test]
fn wallpaper_picker_uses_active_middle_then_visible_query_and_pointer_selection()
-> bead_test::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = bead::render::Palette::read(&root.join("palette.json"))?;
    let fallback = crate::sound::fallback(palette.get("surface")?);
    let mut session = bead::ui::Session::new(
        root.clone(),
        "extras/main.luau".into(),
        root.join("palette.json"),
        palette,
        fallback,
    );
    let initial = session.reload()?;
    session.check_sources()?;
    session.commit(initial);
    let rows = Value::list(vec![
        json!({"id":"/wallpapers/first.jpg","kind":"image","title":"First","thumbnail":"","preview":"","active":false}),
        json!({"id":"/wallpapers/current.jpg","kind":"image","title":"Current middle","thumbnail":"","preview":"","active":true}),
        json!({"id":"/wallpapers/garden.jpg","kind":"image","title":"Garden path","thumbnail":"","preview":"","active":false}),
        json!({"id":"/wallpapers/last.jpg","kind":"image","title":"Last","thumbnail":"","preview":"","active":false}),
        json!({"id":"/wallpapers/animated.gif","kind":"image","animated":true,"title":"Animated image","thumbnail":"","preview":"","active":false}),
        json!({"id":"/wallpapers/movie.mp4","kind":"video","animated":false,"title":"Movie","thumbnail":"","preview":"","active":false}),
    ]);
    let view = session
        .update(BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
            ("extras.available".into(), Value::Bool(true)),
            ("extras.message".into(), Value::Text(String::new())),
            ("extras.rows".into(), rows),
            (
                "extras.wallpaper.selection_id".into(),
                Value::Text(String::new()),
            ),
            ("extras.viewport.width".into(), Value::Number(1280.)),
            ("extras.viewport.height".into(), Value::Number(900.)),
        ]))?
        .expect("wallpaper view update");
    session.commit(view);
    let chosen = session.action_values("extras.choose", Value::Bool(true))?;
    assert_eq!(
        chosen["extras.request"],
        Value::Text("wallpaper:commit:/wallpapers/current.jpg".into())
    );
    let query = session
        .update(BTreeMap::from([(
            "extras.query".into(),
            Value::Text("garden".into()),
        )]))?
        .expect("search update");
    session.commit(query);
    let chosen = session.action_values("extras.choose", Value::Bool(true))?;
    assert_eq!(
        chosen["extras.request"],
        Value::Text("wallpaper:commit:/wallpapers/garden.jpg".into())
    );
    let query = session
        .update(BTreeMap::from([(
            "extras.query".into(),
            Value::Text(String::new()),
        )]))?
        .expect("clear search");
    session.commit(query);
    let select =
        session.action_values("extras.select", Value::Text("/wallpapers/last.jpg".into()))?;
    let selected = session.update(select)?.expect("pointer selection update");
    session.commit(selected);
    let chosen = session.action_values("extras.choose", Value::Bool(true))?;
    assert_eq!(
        chosen["extras.request"],
        Value::Text("wallpaper:commit:/wallpapers/last.jpg".into()),
        "pointer selection controls the next keyboard activation"
    );
    // One strand holds stills, animated images and videos together.
    let all = session
        .update(BTreeMap::from([(
            "extras.query".into(),
            Value::Text(String::new()),
        )]))?
        .expect("clearing the search");
    session.commit(all);
    let behavior_path = root.join("extras/behavior.luau");
    let behavior =
        bead::ui::script::Script::load(&std::fs::read(&behavior_path)?, &behavior_path).unwrap();
    let Value::List(all_rows) = behavior
        .resolve("extras.filtered", &session.stores)
        .unwrap()
    else {
        panic!("the wallpaper filter returns rows")
    };
    assert_eq!(all_rows.len(), 6, "no Static/Dynamic split");
    // "video" and "still" match the media type as well as the text.
    for (word, expected) in [
        ("video", vec!["/wallpapers/movie.mp4"]),
        (
            "still",
            vec![
                "/wallpapers/first.jpg",
                "/wallpapers/current.jpg",
                "/wallpapers/garden.jpg",
                "/wallpapers/last.jpg",
            ],
        ),
    ] {
        let typed = session
            .update(BTreeMap::from([(
                "extras.query".into(),
                Value::Text(word.into()),
            )]))?
            .expect("typing a media word");
        session.commit(typed);
        let Value::List(found) = behavior
            .resolve("extras.filtered", &session.stores)
            .unwrap()
        else {
            panic!("the wallpaper filter returns rows")
        };
        let ids: Vec<&str> = found.iter().filter_map(|row| row["id"].as_str()).collect();
        assert_eq!(ids, expected, "typing {word}");
    }
    let cleared = session
        .update(BTreeMap::from([(
            "extras.query".into(),
            Value::Text(String::new()),
        )]))?
        .expect("clearing the search");
    session.commit(cleared);
    // Opening on the final item must take one forward step across the seam.
    let reset = session
        .update(BTreeMap::from([
            (
                "extras.wallpaper.selection_id".into(),
                Value::Text("/wallpapers/last.jpg".into()),
            ),
            ("extras.wallpaper.position".into(), Value::Number(1.)),
        ]))?
        .unwrap();
    session.commit(reset);
    let next = session.action_values("extras.next", Value::Bool(true))?;
    assert_eq!(next["extras.wallpaper.position"], Value::Number(5.));
    let next = session.update(next)?.unwrap();
    session.commit(next);
    let reset = session
        .update(BTreeMap::from([
            (
                "extras.wallpaper.selection_id".into(),
                Value::Text("/wallpapers/first.jpg".into()),
            ),
            ("extras.wallpaper.position".into(), Value::Number(1.)),
        ]))?
        .unwrap();
    session.commit(reset);
    let previous = session.action_values("extras.prev", Value::Bool(true))?;
    assert_eq!(
        previous["extras.wallpaper.position"],
        Value::Number(0.),
        "zero is a valid unwrapped position when moving left across the seam"
    );
    Ok(())
}

#[cfg(feature = "trace")]
pub(crate) async fn retirement() {
    // Exercise page-owned leases. A permanently requested wallpaper topic belongs
    // to the shared renderer/client and correctly survives any picker closing.
    let lab = lab().await;
    assert!(lab.client.topic("wallpaper.active").is_err());
    for kind in [Kind::Overview, Kind::Keybindings, Kind::Wallpaper] {
        let extras = Extras::new(kind, lab.client.clone()).unwrap();
        if kind == Kind::Wallpaper {
            let open = Stores::default()
                .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(true))]))
                .unwrap();
            extras.project(&open, Some(Arc::new(|| {})));
            let topic = extras.topic.as_ref().unwrap();
            let mut ready = false;
            for _ in 0..200 {
                let snapshot = topic.snapshot();
                ready = snapshot.connected && snapshot.data["revision"] == 17;
                if ready {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            assert!(
                ready,
                "{} fixture publishes the active wallpaper topic",
                kind.name()
            );
        } else {
            ready(&extras, &stores("")).await;
        }
        let subscriptions = extras.subscribe(Arc::new(|| {})).unwrap();
        let snapshot = extras.topic.as_ref().unwrap().snapshot();
        let payload = Arc::downgrade(&snapshot.data);
        drop(snapshot);
        drop(subscriptions);
        drop(extras);
        for _ in 0..100 {
            if payload.upgrade().is_none() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert!(
            payload.upgrade().is_none(),
            "closed {} extras must release topic data",
            kind.name()
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn keyboard_search_and_focus_use_the_actual_luau_view() {
    use bead::wayland::{SceneSource, UiEvent};
    let mut lab = lab().await;
    let extras = Extras::new(Kind::Overview, lab.client.clone()).unwrap();
    ready(&extras, &stores("")).await;
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let colors = bead::render::Palette::read(&root.join("palette.json")).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(
        root.clone(),
        "extras/main.luau".into(),
        root.join("palette.json"),
        colors,
        cold,
    );
    let mut runtime = crate::reload::Runtime::panel_session(session, true)
        .with_extras(extras)
        .unwrap();
    runtime.services(Box::new(|_| {}));
    runtime.stage(UiEvent::Reload).unwrap().expect("view loads");
    runtime.commit();
    assert!(
        runtime
            .session
            .view_nodes()
            .iter()
            .any(|n| n.label == "Editor")
    );
    runtime
        .stage(UiEvent::Model(BTreeMap::from([(
            "extras.query".into(),
            Value::Text("no match".into()),
        )])))
        .unwrap();
    runtime.commit();
    assert!(
        !runtime
            .session
            .view_nodes()
            .iter()
            .any(|n| n.label == "Editor")
    );
    runtime
        .stage(UiEvent::Command {
            command: "enter".into(),
            shift: false,
        })
        .unwrap();
    runtime.commit();
    assert!(lab.calls.try_recv().is_err());
    runtime
        .stage(UiEvent::Model(BTreeMap::from([(
            "extras.query".into(),
            Value::Text(String::new()),
        )])))
        .unwrap();
    runtime.commit();
    runtime
        .stage(UiEvent::Command {
            command: "enter".into(),
            shift: false,
        })
        .unwrap();
    runtime.commit();
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        ("window.focus".into(), json!({"window":"win1"}))
    );
    runtime
        .stage(UiEvent::Model(BTreeMap::from([(
            "panel.open".into(),
            Value::Bool(true),
        )])))
        .unwrap();
    runtime.commit();
    assert_eq!(
        runtime.session.stores.get("panel.open"),
        Some(&Value::Bool(true)),
        "a quick reopen must supersede the earlier close request"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_actions_require_current_available_output_and_membership() {
    let mut lab = lab().await;
    let mut overview = Extras::new(Kind::Overview, lab.client.clone()).unwrap();
    let mut data = fixture();
    data["available"] = json!(false);
    let source = Arc::new(bead_test::source::FakeSource::new(data.clone()).unwrap());
    overview.topic = Some(source.clone());
    overview.act("workspace:1", &stores(""), Arc::new(|| {}));
    assert!(lab.calls.try_recv().is_err());
    data["available"] = json!(true);
    source.publish(data).unwrap();
    overview.act("workspace:2", &stores(""), Arc::new(|| {}));
    assert!(
        lab.calls.try_recv().is_err(),
        "other output's workspace is not admitted"
    );
    overview.act("workspace:1", &stores(""), Arc::new(|| {}));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        (
            "workspace.activate".into(),
            json!({"workspace":1,"output":"OUT"})
        )
    );
}

#[tokio::test(flavor = "current_thread")]
async fn shared_bar_compositor_and_nullable_menu_fields_load_real_views() {
    use bead::wayland::{SceneSource, UiEvent};
    let mut lab = lab_with_topics(vec!["compositor".into()]).await;
    for kind in [Kind::Overview, Kind::Palette, Kind::Keybindings] {
        let extras = Extras::new(kind, lab.client.clone()).unwrap();
        let model = ready(&extras, &stores("")).await;
        if kind == Kind::Palette {
            assert!(
                matches!(&model["extras.rows"], Value::List(rows) if rows[0]["description"] == "" && rows[0]["icon"] == "apps")
            );
        }
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
        let colors = bead::render::Palette::read(&root.join("palette.json")).unwrap();
        let cold = crate::sound::fallback(colors.get("surface").unwrap());
        let session = bead::ui::Session::new(
            root.clone(),
            "extras/main.luau".into(),
            root.join("palette.json"),
            colors,
            cold,
        );
        let mut runtime = crate::reload::Runtime::panel_session(session, true)
            .with_extras(extras)
            .unwrap();
        runtime.services(Box::new(|_| {}));
        let scene = runtime
            .stage(UiEvent::Reload)
            .unwrap()
            .expect("real topic-backed view loads");
        // Exercise decoding too: a staged icon is not proof its bytes render.
        use bead::render::Renderer;
        bead::render::tree::TreeCpuRenderer::default()
            .prepare(&scene, 1.)
            .unwrap();
        runtime.commit();
        let label = match kind {
            Kind::Overview => "Editor",
            Kind::Palette => "Open Settings",
            Kind::Keybindings => "Super+K",
            Kind::Wallpaper => unreachable!("wallpaper uses the catalogue fixture"),
        };
        assert!(
            runtime
                .session
                .view_nodes()
                .iter()
                .any(|node| node.label.trim() == label),
            "{} must render its topic/engine rows",
            kind.name()
        );
        if kind == Kind::Keybindings {
            let close = runtime
                .session
                .view_nodes()
                .iter()
                .find(|node| node.label == "Close" && node.action.is_some())
                .unwrap()
                .id
                .clone();
            runtime
                .stage(UiEvent::Model(BTreeMap::from([(
                    "bead.focused".into(),
                    Value::Text(close),
                )])))
                .unwrap();
            runtime.commit();
            runtime
                .stage(UiEvent::Command {
                    command: "enter".into(),
                    shift: false,
                })
                .unwrap();
            runtime.commit();
            assert_eq!(
                runtime.session.stores.get("panel.open"),
                Some(&Value::Bool(false)),
                "Enter activates the focused Close button"
            );
        }
        if kind == Kind::Overview {
            let node = runtime
                .session
                .view_nodes()
                .iter()
                .find(|node| node.label == "Editor" && node.action.is_some())
                .unwrap();
            let rect = bead::ui::rect(&node.draw);
            runtime
                .stage(UiEvent::Click {
                    x: rect.x + rect.w / 2.,
                    y: rect.y + rect.h / 2.,
                })
                .unwrap();
            runtime.commit();
            assert!(
                lab.calls.try_recv().is_err(),
                "press must not focus before release"
            );
            runtime.stage(UiEvent::Release).unwrap();
            runtime.commit();
            assert_eq!(
                tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
                    .await
                    .unwrap()
                    .unwrap(),
                ("window.focus".into(), json!({"window":"win1"}))
            );
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn palette_navigation_applies_engine_query_and_schedules_its_route() {
    let lab = lab().await;
    let palette = Extras::new(Kind::Palette, lab.client.clone()).unwrap();
    ready(&palette, &stores("")).await;
    let woke = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let signal = woke.clone();
    let wake: CallWake = Arc::new(move || {
        signal.store(true, std::sync::atomic::Ordering::SeqCst);
    });
    palette.act("activate:action", &stores(""), Arc::new(|| {}));
    let mut query = None;
    for _ in 0..200 {
        let model = palette.project(&stores(""), Some(wake.clone()));
        if let Some(Value::Text(text)) = model.get("extras.query") {
            query = Some(text.clone());
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(query.as_deref(), Some("edited"));
    assert!(woke.load(std::sync::atomic::Ordering::SeqCst));
    let model = ready(&palette, &stores("edited")).await;
    assert!(!model.contains_key("panel.open"));
    assert!(
        matches!(&palette.state.borrow().result,Some((_,result)) if result.route=="actions.windows" && result.query=="edited")
    );
}

#[test]
fn long_shortcut_lists_materialize_only_the_viewport() -> bead_test::Result<()> {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let rows = (0..1000).map(|index| json!({"id":format!("key{index}"),"key":format!("Mod+{index}"),"description":format!("Action {index}")})).collect();
    let mut h = bead_test::Harness::load(
        &root,
        std::path::Path::new("extras/main.luau"),
        &root.join("palette.json"),
        BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("keybindings".into())),
            ("extras.rows".into(), Value::list(rows)),
        ]),
        bead_test::Config {
            width: 1280,
            height: 1000,
            scale: 1.,
        },
    )?;
    h.render()?;
    assert!(
        h.scene().primitives.len() < 200,
        "off-screen rows must stay unmaterialized"
    );
    assert!(
        matches!(h.session.stores.get("extras.rows"),Some(Value::List(rows)) if rows.len()==1000)
    );
    h.update(BTreeMap::from([(
        "extras.scroll".into(),
        Value::Number(4000.),
    )]))?;
    h.render()?;
    assert!(h.scene().primitives.len() < 200);
    assert!(
        h.scene()
            .primitives
            .iter()
            .any(|p| matches!(p,bead::render::Primitive::Text{text,..} if text=="Action 86"))
    );
    Ok(())
}

#[test]
fn live_shortcuts_topic_shape_renders_in_keybindings_view() -> bead_test::Result<()> {
    let mut items = serde_json::Map::new();
    let mut order = Vec::new();
    for index in 0..91 {
        let id = format!("k{index}");
        order.push(id.clone());
        items.insert(
            id,
            json!({"key":format!("SUPER+{index}"),"description":format!("Shortcut action {index}")}),
        );
    }
    let topic =
        json!({"health":{"available":true,"stale":false},"items":{"items":items,"order":order}});
    let rows = shortcut_rows(&topic);
    assert_eq!(rows.len(), 91);
    assert_eq!(rows[0]["key"], "SUPER+0");
    assert_eq!(rows[90]["description"], "Shortcut action 90");

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let mut h = bead_test::Harness::load(
        &root,
        std::path::Path::new("extras/main.luau"),
        &root.join("palette.json"),
        BTreeMap::from([
            ("panel.open".into(), Value::Bool(true)),
            ("extras.kind".into(), Value::Text("keybindings".into())),
            ("extras.rows".into(), Value::list(rows)),
        ]),
        bead_test::Config {
            width: 1280,
            height: 1000,
            scale: 1.,
        },
    )?;
    h.render()?;
    assert!(h.scene().primitives.iter().any(|primitive|
        matches!(primitive, bead::render::Primitive::Text { text, .. } if text == "SUPER+0")));
    assert!(h.scene().primitives.iter().any(|primitive|
        matches!(primitive, bead::render::Primitive::Text { text, .. } if text == "Shortcut action 0")));
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn previews_lease_only_while_open_and_with_the_setting_and_attach_the_newest_thumbnail() {
    use bead::wayland::{CaptureHub, CapturePreview};
    let lab = lab().await;
    let dir = lab.dir.join("capture");
    let hub = CaptureHub::test_hub(dir.clone());
    let mut extras = Extras::new(Kind::Overview, lab.client.clone())
        .unwrap()
        .with_capture(hub.clone());
    let with_open = |open: bool| {
        Stores::default()
            .reconcile(&BTreeMap::from([("panel.open".into(), Value::Bool(open))]))
            .unwrap()
    };
    let wake: CallWake = Arc::new(|| {});
    let rows = |model: &BTreeMap<String, Value>| match &model["extras.windows"] {
        Value::List(rows) => rows.clone(),
        other => panic!("{other:?}"),
    };
    // N0234: windowCapture=false (Hyprland v0.1): open shows icon tiles and leases nothing.
    let compositor = Arc::new(bead_test::source::FakeSource::new(fixture()).unwrap());
    extras.topic = Some(compositor.clone() as Arc<dyn Source>);
    let model = extras.project(&with_open(true), Some(wake.clone()));
    assert_eq!(model["extras.available"], Value::Bool(true));
    assert_eq!(hub.lease_count(), 0, "no windowCapture: nothing leased");
    assert!(rows(&model).iter().all(|row| row.get("preview").is_none()));
    let mut capable = fixture();
    capable["capabilities"]["windowCapture"] = json!(true);
    compositor.publish(capable).unwrap();
    for _ in 0..200 {
        if extras.project(&with_open(false), None)["extras.available"] == Value::Bool(true) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(hub.lease_count(), 0, "closed: nothing leased");
    let model = extras.project(&with_open(true), Some(wake.clone()));
    assert_eq!(
        hub.lease_count(),
        1,
        "open: one lease for the listed windows"
    );
    assert!(
        rows(&model)[0].get("preview").is_none(),
        "no frame yet: icon fallback"
    );
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("w1-1.png");
    std::fs::write(&path, b"x").unwrap();
    hub.publish_for_test(
        "win1",
        CapturePreview {
            path: path.clone(),
            width: 160,
            height: 100,
        },
    );
    let model = extras.project(&with_open(true), Some(wake));
    let row = &rows(&model)[0];
    assert_eq!(row["preview"], path.to_string_lossy().as_ref());
    assert_eq!(
        (row["preview_width"].clone(), row["preview_height"].clone()),
        (160.into(), 100.into())
    );
    assert_eq!(hub.lease_count(), 1, "re-projecting reuses the lease");
    extras.project(&with_open(false), None);
    assert_eq!(hub.lease_count(), 0, "closing releases the lease");
    assert!(
        hub.preview("win1").is_none() && !path.exists(),
        "thumbnails go with it"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn picker_preferences_use_provider_lane_and_preserve_the_document() {
    let mut lab = lab_with_topics(vec![crate::prefs::WALLPAPER_PICKER.into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    let wake: CallWake = Arc::new(|| {});
    for _ in 0..200 {
        if wallpaper
            .wallpaper_preference("kind", |_| json!("dynamic"))
            .is_some()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // The retired `layout` request does nothing; a saved `layout` key survives every write untouched.
    wallpaper.act(
        "wallpaper:layout:spotlight",
        &Stores::default(),
        wake.clone(),
    );
    assert!(
        lab.calls.try_recv().is_err(),
        "layout is no longer a preference"
    );
    // The retired Static/Dynamic `kind` request does nothing too; the saved key survives every write.
    wallpaper.act("wallpaper:kind:dynamic", &Stores::default(), wake.clone());
    assert!(
        lab.calls.try_recv().is_err(),
        "kind is no longer a preference"
    );
    wallpaper.act("wallpaper:pause", &Stores::default(), wake);
    let (method, params) = lab.calls.recv().await.unwrap();
    assert_eq!(method, "settings.provider");
    assert_eq!(params["store"], "wallpaper-picker");
    assert_eq!(params["rev"], 3);
    assert_eq!(params["value"]["paused"], false);
    assert_eq!(params["value"]["future"], "preserve");
    assert_eq!(
        params["value"]["kind"], "static",
        "a stale saved kind is tolerated"
    );
    assert_eq!(params["value"]["layout"], "carousel");
}

#[tokio::test(flavor = "current_thread")]
async fn failed_activation_retires_its_early_preview_outside_the_view() {
    let mut lab =
        lab_with_activation_behavior(vec!["wallpaper.active".into()], false, true, 0).await;
    let preview = Arc::new(crate::wallpaper::Preview::default());
    let generation = preview.select(
        "extras-boot".into(),
        "/fixture/next.jpg".into(),
        "image".into(),
        None,
    );
    let mut admission = WallpaperActivationQueue::new(lab.client.clone());
    admission.preview = Some(preview.clone());
    let queue = Arc::new(std::sync::Mutex::new(admission));
    let wake: CallWake = Arc::new(|| {});
    assert!(WallpaperActivationQueue::submit(
        &queue,
        WallpaperSelection {
            id: "/fixture/next.jpg".into(),
            boot: "extras-boot".into(),
            revision: 17,
            selected_at: std::time::Instant::now(),
            file_bytes: 0,
            source_width: 0,
            source_height: 0,
        },
        &wake
    ));
    assert_eq!(
        preview.generation_for("extras-boot", "/fixture/next.jpg"),
        Some(generation)
    );
    let (method, _) = lab.calls.recv().await.unwrap();
    assert_eq!(method, "wallpaper.activate");
    for _ in 0..200 {
        if preview
            .generation_for("extras-boot", "/fixture/next.jpg")
            .is_none()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert_eq!(
        preview.generation_for("extras-boot", "/fixture/next.jpg"),
        None
    );
    assert_eq!(
        queue.lock().unwrap().error(),
        Some("Wallpaper selection failed".into())
    );
}
/// STRAND_FRAMES: one line per burst of motion, closed by the first frame at rest.
#[test]
fn strand_frames_reports_one_line_per_burst() {
    let t0 = Instant::now();
    let at = |ms: u64| t0 + Duration::from_millis(ms);
    let mut log = FrameLog::default();
    assert_eq!(
        log.frame(at(0), false),
        None,
        "rest without motion is silent"
    );
    assert_eq!(log.frame(at(10), true), None);
    log.stages.extend([4., 6.]);
    for (ms, moving) in [(18, true), (26, true), (60, true)] {
        assert_eq!(log.frame(at(ms), moving), None);
    }
    let line = log
        .frame(at(68), false)
        .expect("the settle frame closes the burst");
    assert_eq!(
        line,
        "STRAND_FRAMES frames=4 duration_ms=58 interval_mean=14.5 p95=34.0 max=34.0 stages=2 stage_mean_ms=5.0 stage_max_ms=6.0"
    );
    assert!(log.last.is_none() && log.intervals.is_empty() && log.stages.is_empty());
    assert_eq!(
        log.frame(at(90), false),
        None,
        "and the next rest frame is silent again"
    );
}

fn snapshot_entry(id: &str, kind: &str, thumbnail: &str) -> Json {
    json!({"id":id,"kind":kind,"animated":id.ends_with(".gif"),"title":"Old","thumbnail":thumbnail,"preview":"","active":false})
}

/// A catalogue loaded from the snapshot, as a restarted shell has it, before any picker visit.
fn snapshot_loaded(extras: &Extras, stale_thumbnail: &str) {
    let mut state = extras.state.borrow_mut();
    state.wallpaper_boot = Some("extras-boot".into());
    state.catalogue_boot = Some("extras-boot".into());
    state.catalogue_result = vec![
        snapshot_entry("/fixture/current.jpg", "image", ""),
        snapshot_entry("/fixture/next.jpg", "image", stale_thumbnail),
        snapshot_entry("/fixture/next.mp4", "video", ""),
        snapshot_entry("/fixture/animated.gif", "image", ""),
    ];
}

async fn settle_refresh(extras: &Extras, stores: &Stores) {
    for _ in 0..200 {
        extras.project(stores, Some(Arc::new(|| {})));
        if extras.state.borrow().catalogue.is_none() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    panic!("the catalogue refresh never answered");
}

fn panel(open: bool) -> Stores {
    Stores::default()
        .reconcile(&BTreeMap::from([
            ("panel.open".into(), Value::Bool(open)),
            ("extras.kind".into(), Value::Text("wallpaper".into())),
        ]))
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn snapshot_loaded_catalogue_refreshes_once_per_boot_while_closed_without_rebaking() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    snapshot_loaded(&wallpaper, "");
    let revision = wallpaper.state.borrow().image_revision;
    let closed = panel(false);
    wallpaper.project(&closed, Some(Arc::new(|| {})));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    settle_refresh(&wallpaper, &closed).await;
    {
        let state = wallpaper.state.borrow();
        assert_eq!(
            state.catalogue_result[0]["title"], "Current",
            "stale rows are replaced"
        );
        assert_eq!(
            state.image_revision, revision,
            "identical image paths keep their pearls"
        );
    }
    // Reopening and closing again in the same boot asks for nothing more.
    let open = panel(true);
    wallpaper.project(&open, Some(Arc::new(|| {})));
    wallpaper.project(&closed, Some(Arc::new(|| {})));
    wallpaper.project(&closed, Some(Arc::new(|| {})));
    assert!(
        lab.calls.try_recv().is_err(),
        "one full refresh per boot, not per open"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn catalogue_refresh_drops_stale_predecode_and_keeps_warm_thumbnails() {
    let mut lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    snapshot_loaded(&wallpaper, "/warm/next-thumbnail.jpg");
    wallpaper.state.borrow_mut().catalogue_result[2]["video_predecode"] =
        json!({"poster_path":"/old/poster.png"});
    let revision = wallpaper.state.borrow().image_revision;
    let closed = panel(false);
    wallpaper.project(&closed, Some(Arc::new(|| {})));
    assert_eq!(lab.calls.recv().await.unwrap().0, "wallpaper.catalogue");
    settle_refresh(&wallpaper, &closed).await;
    let state = wallpaper.state.borrow();
    assert!(
        state.catalogue_result[2]["video_predecode"].is_null(),
        "the stale predecode is replaced, so the pick fetches the current one"
    );
    assert_eq!(
        state.catalogue_result[1]["thumbnail"],
        "/warm/next-thumbnail.jpg"
    );
    assert_eq!(state.image_revision, revision);
}

#[tokio::test(flavor = "current_thread")]
async fn closed_strand_wakes_for_its_refresh_until_it_is_sent() {
    let lab = lab_with_topics(vec!["wallpaper.active".into()]).await;
    let wallpaper = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    snapshot_loaded(&wallpaper, "");
    assert!(
        wallpaper.thumb_retry_at().is_some(),
        "a closed Strand asks the runtime to wake it so the refresh goes out at shell start"
    );
    let closed = panel(false);
    wallpaper.project(&closed, Some(Arc::new(|| {})));
    settle_refresh(&wallpaper, &closed).await;
    assert!(
        wallpaper.thumb_retry_at().is_none(),
        "no wakes once it was sent"
    );
}

#[tokio::test(flavor = "current_thread")]
async fn closed_strand_runtime_sends_its_refresh_on_a_deadline_wake_then_stops_waking() {
    use bead::wayland::{SceneSource, UiEvent};
    // The shell's startup build runs before the daemon boot is known.
    let mut lab = unconnected_lab(vec!["wallpaper.active".into()], false, false, 0);
    // The previous boot's snapshot, which project() loads once the boot is known.
    crate::wallpaper_catalogue_cache::store(&[
        snapshot_entry("/fixture/current.jpg", "image", ""),
        snapshot_entry("/fixture/next.jpg", "image", ""),
    ]);
    let extras = Extras::new(Kind::Wallpaper, lab.client.clone()).unwrap();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let colors = bead::render::Palette::read(&root.join("palette.json")).unwrap();
    let cold = crate::sound::fallback(colors.get("surface").unwrap());
    let session = bead::ui::Session::new(
        root.clone(),
        "extras/main.luau".into(),
        root.join("palette.json"),
        colors,
        cold,
    );
    let mut runtime = crate::reload::Runtime::panel_session(session, false)
        .with_extras(extras)
        .unwrap();
    runtime.services(Box::new(|_| {}));
    if runtime.stage(UiEvent::Reload).unwrap().is_some() {
        runtime.commit();
    }
    assert!(
        lab.calls.try_recv().is_err(),
        "the startup build sends nothing while closed"
    );
    await_connected(&lab.client).await;
    // Bead's worker sleeps until wakeup_at() and then stages a model wake.
    let due = runtime
        .wakeup_at()
        .expect("a closed Strand with a pending refresh asks to be woken");
    assert!(due <= std::time::Instant::now() + Duration::from_millis(300));
    let (mut staged, mut pearls) = (0, 0);
    let wallpaper_images = |scene: &bead::render::Scene| {
        scene
            .primitives
            .iter()
            .filter(|p| matches!(p, bead::render::Primitive::Image { path, .. } if path.to_string_lossy().starts_with("@icon:wallpaper:")))
            .count()
    };
    if let Some(scene) = runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap() {
        (staged, pearls) = (staged + 1, pearls + wallpaper_images(&scene));
        runtime.commit();
    }
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), lab.calls.recv())
            .await
            .unwrap()
            .unwrap(),
        ("wallpaper.catalogue".into(), json!({"ids":[]}))
    );
    assert!(
        runtime.wakeup_at().is_none(),
        "no refresh deadline remains once the request is sent"
    );
    for _ in 0..20 {
        if let Some(scene) = runtime.stage(UiEvent::Model(BTreeMap::new())).unwrap() {
            (staged, pearls) = (staged + 1, pearls + wallpaper_images(&scene));
            runtime.commit();
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert!(
        lab.calls.try_recv().is_err(),
        "exactly one request while closed"
    );
    // A closed Strand may restage its (hidden) rows when the catalogue content changes, but it carries no pearl images to bake.
    assert_eq!(
        pearls, 0,
        "no wallpaper image reaches a closed stage ({staged} scenes staged)"
    );
}
