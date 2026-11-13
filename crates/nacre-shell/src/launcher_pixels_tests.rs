// SPDX-License-Identifier: GPL-3.0-only
//! Real launcher widgets against accepted-source goldens, not a DSL imitation.
use bead::core::{Value, source::Source};
use bead_test::{Config, Harness, Result};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell")
}
fn harness(query: &str, scroll: f64) -> Result<Harness> {
    harness_at_scale(query, scroll, 1.5)
}
fn harness_at_scale(query: &str, scroll: f64, scale: f32) -> Result<Harness> {
    crate::i18n::init();
    let update = std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1");
    let selected = std::env::var_os("BEAD_ROW_REFERENCE_BUNDLE");
    if selected.is_some() && !update {
        return Err("reference bundle requires explicit golden capture".into());
    }
    let root = selected.map(PathBuf::from).unwrap_or_else(root);
    let rows=(0..320).map(|i|serde_json::json!({"id":format!("app-{i}"),"name":format!("Example application {i}"),"description":format!("Description {i}"),"icon":"fixture","selected":i==0,"disabled":i%3==2,"favorite":i%7==0,"favoriteId":format!("app-{i}"),"emoji":if i%4==0 {"A"}else{""}})).collect();
    Harness::load_with_icons(
        &root,
        Path::new("ui/main.luau"),
        &root.join("palette.json"),
        BTreeMap::from([
            ("launcher.apps".into(), Value::list(rows)),
            ("launcher.groups".into(), crate::launcher::fixture_groups()),
            ("launcher.result_count".into(), Value::Number(320.)),
            ("launcher.search.text".into(), Value::Text(query.into())),
            ("launcher.scroll.y".into(), Value::Number(scroll)),
            (
                "launcher.quick".into(),
                Value::list(vec![
                    serde_json::json!({"id":"actions.controls.dnd","label":"Do not disturb","reason":"Silence notification popups","status":"Off","icon":"notifications","on":false,"enabled":true}),
                    serde_json::json!({"id":"actions.controls.night-light","label":"Night light","reason":"Make the screen warmer","status":"On","icon":"nightlight","on":true,"enabled":true}),
                    serde_json::json!({"id":"actions.controls.stay-awake","label":"Stay awake","reason":"Prevent automatic sleep","status":"Off","icon":"coffee","on":false,"enabled":true}),
                ]),
            ),
            ("launcher.updates.count".into(), Value::Number(7.)),
        ]),
        Config {
            width: 1920,
            height: 1200,
            scale,
        },
        BTreeMap::from([("fixture".into(), root.join("tests/icon.svg"))]),
    )
}

#[test]
fn files_hint_keeps_first_result_at_the_same_pixel_row() -> Result<()> {
    for scale in [1., 1.5] {
        let mut h = harness_at_scale("", 0., scale)?;
        let mut first_y = None;
        for (query, message) in [
            ("", ""),
            (":", "Search these folders; type two or more characters to find files"),
            (":ab", ""),
        ] {
            h.update(BTreeMap::from([
                ("launcher.search.text".into(), Value::Text(query.into())),
                ("launcher.message".into(), Value::Text(message.into())),
            ]))?;
            h.render()?;
            let row = h.session.view_nodes().iter()
                .find(|node| node.action.as_deref() == Some("select_app"))
                .expect("first rendered result");
            let y = (bead::ui::rect(&row.draw).y * scale).round() as i32;
            assert_eq!(*first_y.get_or_insert(y), y, "{query:?} at scale {scale}");
            if !message.is_empty() {
                assert!(h.scene().primitives.iter().any(|draw| matches!(draw,
                    bead::render::Primitive::Text { text, .. } if text == message)));
            }
        }
    }
    Ok(())
}

struct KeyboardFixture(crate::launcher::Fixture);
impl bead::core::source::Source for KeyboardFixture {
    fn snapshot(&self) -> std::sync::Arc<bead::core::source::Snapshot> { self.0.snapshot() }
    fn subscribe(&self, callback: bead::core::source::Callback) -> bead::core::source::Result<bead::core::source::Subscription> { self.0.subscribe(callback) }
    fn invoke(&self, method: String, params: serde_json::Value) -> bead::core::source::Reply<'_> {
        Box::pin(async move {
            let mut reply = self.0.invoke(method.clone(), params.clone()).await?;
            if method == "menu.query" {
                reply["mode"] = params["mode"].clone();
                reply["route"] = params["route"].as_str().unwrap_or(if params["mode"] == "search" { "home" } else { "apps" }).into();
            }
            Ok(reply)
        })
    }
}

fn keyboard_runtime(query: &str) -> Result<crate::reload::Runtime> {
    use bead::wayland::{SceneSource, UiEvent};
    let rows = (0..32).map(|index| serde_json::json!({"id":format!("key-{index}"),"name":format!("Key {index}"),"icon":"fixture"})).collect::<Vec<_>>();
    let queries = ["", "a", "aa", "aaa", "ab", "> ", ">plugin "].into_iter()
        .map(|query| (query.to_owned(), crate::launcher::fixture_reply(query, rows.clone())))
        .collect::<serde_json::Map<_, _>>();
    let Value::List(groups) = crate::launcher::fixture_groups() else { unreachable!() };
    let groups = groups.iter().map(|row| serde_json::json!({"id":row["id"],"label":row["name"],"icon":row["icon"]})).collect::<Vec<_>>();
    let source = std::sync::Arc::new(KeyboardFixture(crate::launcher::Fixture::new(serde_json::json!({"queries":queries,"groups":groups}))?));
    let mut runtime = crate::reload::Runtime::new(harness(query, 0.)?.session).with_launcher(Some(source))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        runtime.stage(UiEvent::Model(BTreeMap::new()))?;
        runtime.commit();
        if runtime.session.stores.get("launcher.pending") == Some(&Value::Bool(false)) { break; }
        assert!(std::time::Instant::now() < deadline, "keyboard fixture did not settle");
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
    Ok(runtime)
}

fn keyboard_command(runtime: &mut crate::reload::Runtime, command: &str) -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    if let Some(scene) = runtime.stage(UiEvent::Command { command: command.into(), shift: false })? {
        if command != "escape" && let Some(Value::Text(id)) = runtime.session.stores.get("bead.focused") {
            let node = runtime.session.view_nodes().iter().find(|node| &node.id == id).expect("visible focus target");
            assert!(node.focus_ring, "focus ring enabled for {id}");
            let bounds = bead::ui::rect(&node.draw);
            assert!(scene.primitives.iter().any(|draw| matches!(draw,
                bead::render::Primitive::Rounded { rect, fill: [0, 0, 0, 0], border: Some(_), .. } if *rect == bounds)),
                "visible focus ring in {command} scene for {id}");
        }
    }
    runtime.commit();
    Ok(())
}

#[test]
fn launcher_keyboard_moves_between_rail_grid_list_and_quick_controls() -> Result<()> {
    for query in ["", "a"] {
        let mut runtime = keyboard_runtime(query)?;
        if !query.is_empty() {
            keyboard_command(&mut runtime, "down")?;
            assert_eq!(runtime.session.focused_action().unwrap().1, "select_app");
            keyboard_command(&mut runtime, "right")?;
            assert_eq!(runtime.session.focused_action().unwrap().1, "launcher.quick");
            keyboard_command(&mut runtime, "left")?;
            assert_eq!(runtime.session.focused_action().unwrap().1, "select_app");
        }
        runtime.session.focus_id("launcher.search")?;
        if !query.is_empty() { runtime.session.edit_key(bead::ui::input::EditKey::Home, false)?; }
        keyboard_command(&mut runtime, "left")?;
        assert_eq!(runtime.session.focused_action().unwrap().1, "navigate");
        keyboard_command(&mut runtime, "up")?;
        assert_eq!(runtime.session.stores.get("launcher.engine_route"), Some(&Value::Text("home".into())));
        keyboard_command(&mut runtime, "down")?;
        assert_eq!(runtime.session.stores.get("launcher.engine_route"), Some(&Value::Text(String::new())));
        keyboard_command(&mut runtime, "right")?;
        assert_eq!(runtime.session.focused_action().unwrap().1, "select_app");
        assert_eq!(runtime.session.stores.get("launcher.selection"), Some(&Value::Text("key-0".into())));
        keyboard_command(&mut runtime, "left")?;
        assert_eq!(runtime.session.focused_action().unwrap().1, "navigate");
        keyboard_command(&mut runtime, "enter")?;
        assert_eq!(runtime.session.focused_action().unwrap().1, "select_app");
        // Horizontal moves stay in content until its rightmost rendered column.
        for _ in 0..12 {
            keyboard_command(&mut runtime, "right")?;
            if runtime.session.focused_action().unwrap().1 == "launcher.quick" { break; }
        }
        assert_eq!(runtime.session.focused_action().unwrap().2, Value::Text("actions.controls.dnd".into()));
        for (key, id) in [("down", "night-light"), ("down", "stay-awake"), ("up", "night-light")] {
            keyboard_command(&mut runtime, key)?;
            assert_eq!(runtime.session.focused_action().unwrap().2, Value::Text(format!("actions.controls.{id}")));
        }
        let selected = runtime.session.stores.get("launcher.selection").cloned();
        keyboard_command(&mut runtime, "left")?;
        assert_eq!(runtime.session.focused_action().unwrap().1, "select_app");
        assert_eq!(runtime.session.stores.get("launcher.selection"), selected.as_ref());
        keyboard_command(&mut runtime, "down")?;
        assert_eq!(runtime.session.focused_action().unwrap().1, "select_app");
        keyboard_command(&mut runtime, "up")?;
    }
    Ok(())
}

#[test]
fn launcher_typing_reaches_search_from_each_zone_and_arrows_respect_the_caret() -> Result<()> {
    use bead::wayland::{SceneSource, UiEvent};
    let mut runtime = keyboard_runtime("")?;
    for action in ["navigate", "select_app", "launcher.quick"] {
        let id = runtime.session.view_nodes().iter().find(|node| node.action.as_deref() == Some(action)).unwrap().id.clone();
        runtime.session.focus_id(&id)?;
        runtime.stage(UiEvent::Key { text: "a".into(), backspace: false })?;
        runtime.commit();
        assert_eq!(runtime.session.stores.get("bead.focused"), Some(&Value::Text("launcher.search".into())));
        assert!(runtime.session.surrounding_text().unwrap().0.ends_with('a'));
    }
    runtime.session.update(BTreeMap::from([("launcher.search.text".into(), Value::Text("ab".into()))]))?;
    runtime.session.edit_key(bead::ui::input::EditKey::End, false)?;
    keyboard_command(&mut runtime, "left")?;
    assert_eq!(runtime.session.surrounding_text().unwrap().1, 1);
    keyboard_command(&mut runtime, "left")?;
    assert_eq!(runtime.session.surrounding_text().unwrap().1, 0);
    keyboard_command(&mut runtime, "right")?;
    assert_eq!(runtime.session.surrounding_text().unwrap().1, 1);
    runtime.session.edit_key(bead::ui::input::EditKey::End, false)?;
    keyboard_command(&mut runtime, "right")?;
    assert_eq!(runtime.session.surrounding_text().unwrap().1, 2);
    runtime.session.edit_key(bead::ui::input::EditKey::Home, false)?;
    keyboard_command(&mut runtime, "left")?;
    assert_eq!(runtime.session.focused_action().unwrap().1, "navigate");
    keyboard_command(&mut runtime, "escape")?;
    assert_eq!(runtime.session.stores.get("panel.open"), Some(&Value::Bool(false)));
    Ok(())
}

#[test]
fn quick_buttons_keep_their_control_ids_and_unknown_updates_stay_hidden() -> Result<()> {
    let mut h = harness("", 0.)?;
    let ids = h
        .session
        .view_nodes()
        .iter()
        .filter(|node| node.action.as_deref() == Some("launcher.quick"))
        .map(|node| node.action_arg.clone().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "actions.controls.dnd",
            "actions.controls.night-light",
            "actions.controls.stay-awake"
        ]
        .map(|id| Value::Text(id.into()))
    );
    assert!(
        h.session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("launcher.quick"))
            .all(|node| bead::ui::rect(&node.draw).h == 50.),
        "the QML default descriptions fit single-line, 50-pixel rows"
    );
    h.update(BTreeMap::from([(
        "launcher.updates.count".into(),
        Value::Number(-1.),
    )]))?;
    assert!(!h.session.view_nodes().iter().any(|node| {
        matches!(&node.draw, bead::render::Primitive::Text { text, .. } if text == "7 updates" || text == "Up to date")
    }));
    Ok(())
}

#[test]
fn launcher_prefix_caret_stays_inside_the_search_input() -> Result<()> {
    for text in [">", "> app search"] {
        let mut h = harness("", 0.)?;
        let input = h
            .session
            .view_nodes()
            .iter()
            .find(|node| node.input.as_deref() == Some("launcher.search.text"))
            .expect("launcher search input");
        let rect = bead::ui::rect(&input.draw);
        let clicked = h
            .session
            .click(rect.x + 30., rect.y + rect.h / 2.)?
            .expect("click updates the launcher");
        h.session.commit(clicked);
        let mut scene = h
            .session
            .key(text, false)?
            .expect("typing updates the launcher");
        h.session.decorate_inputs(&mut scene)?;
        let caret = scene
            .primitives
            .iter()
            .rev()
            .find_map(|primitive| match primitive {
                bead::render::Primitive::Rounded { rect, .. }
                    if rect.w <= 1.5 && (10.0..50.0).contains(&rect.h) =>
                {
                    Some(*rect)
                }
                _ => None,
            })
            .expect("focused text input caret");
        assert!(
            caret.x >= rect.x && caret.x <= rect.x + rect.w,
            "{text:?}: {caret:?} outside {rect:?}"
        );
        assert!(
            caret.y >= rect.y && caret.y + caret.h <= rect.y + rect.h,
            "{text:?}: caret must remain inside input bounds: {caret:?} vs {rect:?}"
        );
    }
    Ok(())
}

#[test]
fn real_launcher_grid_results_and_scrolled_pixels_match_accepted_markup() -> Result<()> {
    for (name, query, scroll) in [
        ("grid", "", 0.),
        ("results", "a", 0.),
        ("scrolled", "a", 500.),
    ] {
        let mut h = harness(query, scroll)?;
        assert_eq!(
            h.scene().motion.as_ref().map(|motion| motion.id.as_str()),
            Some("launcher")
        );
        assert_eq!(
            h.session
                .view_nodes()
                .iter()
                .filter(|n| n.action.as_deref() == Some("launcher.quick"))
                .count(),
            3,
            "Quick remains beside ordinary search results: {query:?}; viewport={:?}",
            h.session.stores.get("bead.viewport.width")
        );
        let source = h.render()?;
        let width = (h.scene().bounds.w * 1.5).ceil() as u32;
        let height = (h.scene().bounds.h * 1.5).ceil() as u32;
        assert!(width <= source.width && height <= source.height);
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in source
            .pixels
            .chunks_exact(source.width as usize * 4)
            .take(height as usize)
        {
            pixels.extend_from_slice(&row[..width as usize * 4]);
        }
        let frame = bead_test::Frame {
            width,
            height,
            pixels,
        };
        let golden = root()
            .join("tests/goldens")
            .join(format!("launcher-rows-{name}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            frame.save(&golden)?;
        }
        frame.compare(
            &golden,
            bead_test::Tolerance {
                channel: 0,
                changed_fraction: 0.,
            },
            &std::env::temp_dir().join("bead-launcher-rows"),
        )?;
        let actions = h
            .session
            .view_nodes()
            .iter()
            .filter(|n| n.action.as_deref() == Some("select_app"))
            .collect::<Vec<_>>();
        assert!(
            actions.len() >= 7,
            "populated fixture must exercise actual rows"
        );
        let ids = actions
            .iter()
            .map(|n| match &n.action_arg {
                Some(Value::Text(id)) => id.clone(),
                _ => panic!("row action lost its item ID"),
            })
            .collect::<BTreeSet<_>>();
        assert_eq!(
            actions.len(),
            ids.len(),
            "each visible row must retain its own action ID"
        );
        let favorites = h
            .session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("toggle_favorite"))
            .collect::<Vec<_>>();
        let row_ids = actions
            .iter()
            .filter_map(|node| match &node.action_arg {
                Some(Value::Text(id)) => Some(id.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        let favorite_ids = favorites
            .iter()
            .filter_map(|node| match &node.action_arg {
                Some(Value::Text(id)) => Some(id.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        assert!(
            favorite_ids.is_subset(&row_ids),
            "favorite actions must point at displayed rows"
        );
        let fully_visible_rows = actions
            .iter()
            .filter(|node| {
                let rect = bead::ui::rect(&node.draw);
                node.clip.is_none_or(|clip| {
                    rect.x >= clip.x
                        && rect.y >= clip.y
                        && rect.x + rect.w <= clip.x + clip.w
                        && rect.y + rect.h <= clip.y + clip.h
                })
            })
            .filter_map(|node| match &node.action_arg {
                Some(Value::Text(id)) => Some(id.clone()),
                _ => None,
            })
            .collect::<BTreeSet<_>>();
        for id in fully_visible_rows {
            let node = h
                .session
                .view_nodes()
                .iter()
                .find(|n| {
                    n.action.as_deref() == Some("select_app")
                        && n.action_arg == Some(Value::Text(id.clone()))
                })
                .unwrap();
            let rect = bead::ui::rect(&node.draw);
            h.script([bead_test::Input::Pointer {
                x: rect.x + rect.w / 2.,
                y: rect.y + rect.h / 2.,
            }])?;
            assert!(
                h.session.view_nodes().iter().any(|n| {
                    n.action.as_deref() == Some("toggle_favorite")
                        && n.action_arg == Some(Value::Text(id.clone()))
                }),
                "hover reveals the favorite action for {id}"
            );
        }
    }
    Ok(())
}

#[test]
fn restored_launcher_route_reopens_with_matching_mode_and_results_layout() -> Result<()> {
    for route in ["search", "actions"] {
        let mut h = harness("", 0.)?;
        // Only page/route strings are durable; mode and query are transient.
        h.update(BTreeMap::from([(
            "launcher.route".into(),
            Value::Text(route.into()),
        )]))?;
        assert_eq!(
            h.session.stores.get("launcher.mode"),
            Some(&Value::Text("apps".into()))
        );
        let mut reply = crate::launcher::fixture_reply(
            "",
            vec![serde_json::json!({"id":"restored","name":"Restored result","icon":"fixture"})],
        );
        reply["mode"] = serde_json::json!(route);
        reply["route"] = serde_json::json!(route);
        let source = std::sync::Arc::new(crate::launcher::Fixture::new(
            serde_json::json!({"queries":{"":reply}}),
        )?);
        let menu = crate::launcher::Menu::new(Some(source))?;
        h.session
            .set_projector(Box::new(move |stores| menu.project(stores)));
        h.update(BTreeMap::new())?;
        assert_eq!(
            h.session.stores.get("launcher.mode"),
            Some(&Value::Text(route.into()))
        );
        assert_eq!(
            h.session.stores.get("launcher.search.text"),
            Some(&Value::Text(String::new()))
        );
        assert!(
            h.scene()
                .layers
                .iter()
                .any(|layer| layer.id.contains("results"))
        );
        assert!(
            !h.scene()
                .layers
                .iter()
                .any(|layer| layer.id.contains("app-grid"))
        );
    }
    Ok(())
}

#[test]
fn ranked_prefix_keeps_actual_visible_pixels_and_actions_exact() -> Result<()> {
    for scroll in [0., 500.] {
        let mut h = harness("a", scroll)?;
        let full = h.render()?;
        let action_ids = |h: &Harness| {
            h.session
                .view_nodes()
                .iter()
                .filter(|node| node.action.as_deref() == Some("select_app"))
                .map(|node| format!("{:?}", node.action_arg))
                .collect::<Vec<_>>()
        };
        let expected = action_ids(&h);
        let Some(Value::List(rows)) = h.session.stores.get("launcher.apps") else {
            panic!("fixture rows")
        };
        let prefix = rows[..64].to_vec();
        h.update(BTreeMap::from([
            ("launcher.apps".into(), Value::list(prefix)),
            ("launcher.result_count".into(), Value::Number(64.)),
            ("launcher.total_count".into(), Value::Number(320.)),
        ]))?;
        assert_eq!(h.render()?.pixels, full.pixels);
        assert_eq!(action_ids(&h), expected);
    }
    Ok(())
}

#[test]
fn launcher_grid_and_results_scroll_from_their_actual_hit_regions() -> Result<()> {
    for query in ["", "a"] {
        let mut h = harness(query, 0.)?;
        let Some(Value::List(rows)) = h.session.stores.get("launcher.apps") else {
            panic!("launcher fixture rows")
        };
        h.update(BTreeMap::from([
            ("launcher.apps".into(), Value::list(rows[..81].to_vec())),
            ("launcher.result_count".into(), Value::Number(81.)),
            ("launcher.total_count".into(), Value::Number(81.)),
        ]))?;
        let viewport = h
            .session
            .view_nodes()
            .iter()
            .find(|node| node.scroll_store.as_deref() == Some("launcher.scroll.y"))
            .expect("launcher has a scroll viewport");
        let rect = bead::ui::rect(&viewport.draw);
        eprintln!(
            "launcher {query:?}: viewport={rect:?}, limit={:?}",
            viewport.scroll_limit
        );
        assert!(viewport.scroll_limit.is_some_and(|limit| limit > 400.));
        for (delta, expected) in [(23.5, 23.5), (100., 123.5), (-123.5, 0.)] {
            h.script([bead_test::Input::Scroll {
                x: rect.x + rect.w / 2.,
                y: rect.y + rect.h / 2.,
                delta,
            }])?;
            assert_eq!(
                h.session.stores.get("launcher.scroll.y"),
                Some(&Value::Number(expected)),
                "query {query:?}: viewport {rect:?} must follow input"
            );
        }
    }
    Ok(())
}

/// B0037: `launcher-layout` order and hidden ids drive the category rail live; Home is
/// never hidden and rows outside the order keep their place at the end.
#[test]
fn launcher_rail_follows_layout_order_and_hidden_ids() -> Result<()> {
    let rail = |order: &[&str], hidden: &[&str]| -> Result<Vec<String>> {
        let list = |ids: &[&str]| {
            Value::list(ids.iter().map(|i| serde_json::json!({ "id": i })).collect())
        };
        let root = root();
        let mut h = Harness::load(
            &root,
            Path::new("ui/main.luau"),
            &root.join("palette.json"),
            BTreeMap::from([
                ("launcher.groups".into(), crate::launcher::fixture_groups()),
                ("launcher.rail.order".into(), list(order)),
                ("launcher.rail.hidden".into(), list(hidden)),
            ]),
            Config {
                width: 1920,
                height: 1200,
                scale: 1.,
            },
        )?;
        h.render()?;
        Ok(h.session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("navigate"))
            .filter_map(|node| match &node.action_arg {
                Some(Value::Text(id)) => Some(id.clone()),
                _ => None,
            })
            .collect())
    };
    let all = [
        "home", "apps", "actions", "plugins", "style", "setup", "software", "health", "learn",
        "power",
    ];
    assert_eq!(rail(&[], &[])?, all);
    assert_eq!(
        rail(&[], &["actions", "home", "power"])?,
        [
            "home", "apps", "plugins", "style", "setup", "software", "health", "learn"
        ]
    );
    assert_eq!(
        rail(&["power", "style"], &[])?,
        [
            "home", "power", "style", "apps", "actions", "plugins", "setup", "software", "health",
            "learn"
        ]
    );
    assert_eq!(
        rail(&["apps", "style"], &["actions"])?,
        [
            "home", "apps", "style", "plugins", "setup", "software", "health", "learn", "power"
        ]
    );
    // Without `menu.groups` (older daemon) the rail keeps the groups every engine has.
    let bare = {
        let root = root();
        let mut h = Harness::load(
            &root,
            Path::new("ui/main.luau"),
            &root.join("palette.json"),
            BTreeMap::new(),
            Config {
                width: 1920,
                height: 1200,
                scale: 1.,
            },
        )?;
        h.render()?;
        h.session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("navigate"))
            .filter_map(|node| match &node.action_arg {
                Some(Value::Text(id)) => Some(id.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(bare, ["home", "apps", "actions", "plugins"]);
    Ok(())
}

#[test]
fn pending_query_keeps_the_accepted_model_geometry_until_replacement() -> Result<()> {
    let mut h = harness("", 0.)?;
    let grid = |h: &Harness| {
        h.session
            .view_nodes()
            .iter()
            .any(|n| n.id.ends_with("app-grid"))
    };
    assert!(grid(&h));
    h.update(BTreeMap::from([
        ("launcher.search.text".into(), Value::Text("a".into())),
        ("launcher.pending".into(), Value::Bool(true)),
    ]))?;
    assert!(
        grid(&h),
        "old grid rows must not be reshaped as pending search rows"
    );
    assert_eq!(
        h.session.stores.get("launcher.search.text"),
        Some(&Value::Text("a".into()))
    );
    h.update(BTreeMap::from([
        ("launcher.result_query".into(), Value::Text("a".into())),
        ("launcher.pending".into(), Value::Bool(false)),
    ]))?;
    assert!(!grid(&h), "accepted search results must use row geometry");
    Ok(())
}

#[test]
fn launcher_two_line_copy_is_inside_its_row_and_clip() -> Result<()> {
    for scale in [1., 1.5] {
        let mut h = harness_at_scale("a", 0., scale)?;
        let descriptions = [
            "First readable line\nSecond readable line".to_owned(),
            "A long readable description with words and emoji 🌊 ".repeat(30),
            "first\nsecond\nthird".to_owned(),
        ];
        let rows = descriptions.iter().enumerate().map(|(index, description)| serde_json::json!({
            "id":format!("wrapped-{index}"), "name":(["Explicit newline", "Wrapped description", "Bounded overflow"][index]),
            "description":description, "icon":"fixture", "selected":false, "favorite":false,
            "favoriteId":format!("wrapped-{index}")
        })).collect();
        h.update(BTreeMap::from([
            ("launcher.apps".into(), Value::list(rows)),
            ("launcher.total_count".into(), Value::Number(3.)),
            ("launcher.result_count".into(), Value::Number(3.)),
        ]))?;
        let nodes = h.session.view_nodes();
        for (index, description) in descriptions.iter().enumerate() {
            let id = Value::Text(format!("wrapped-{index}"));
            let row = nodes
                .iter()
                .find(|n| {
                    n.action.as_deref() == Some("select_app") && n.action_arg.as_ref() == Some(&id)
                })
                .unwrap();
            let row_rect = bead::ui::rect(&row.draw);
            // Passive text is in scene primitives, not the interactive hit-node list.
            let scene = h.scene();
            let lines = scene
                .layers
                .iter()
                .filter(|layer| layer.id.contains("description.line-"))
                .flat_map(|layer| layer.own.clone())
                .filter(|&index| {
                    let rect = bead::ui::rect(&scene.primitives[index]);
                    rect.y >= row_rect.y && rect.y < row_rect.y + row_rect.h
                })
                .collect::<Vec<_>>();
            assert_eq!(
                lines.len(),
                2,
                "scale {scale}, row {index}; layers: {:?}",
                scene.layers.iter().map(|l| &l.id).collect::<Vec<_>>()
            );
            for &index in &lines {
                let primitive = &scene.primitives[index];
                let rect = bead::ui::rect(primitive);
                assert!(rect.y >= row_rect.y && rect.y + rect.h <= row_rect.y + row_rect.h);
                assert!(
                    scene
                        .clips
                        .get(&index)
                        .is_none_or(|c| rect.y >= c.y && rect.y + rect.h <= c.y + c.h)
                );
                if let bead::render::Primitive::Text {
                    text,
                    family,
                    size,
                    color,
                    ..
                } = primitive
                {
                    assert!(bead::render::text_width(scene, text, family, *size)? <= rect.w);
                    assert!(rect.h >= size * 1.4);
                    assert_eq!(color[3], 255, "no dark text mask");
                } else {
                    panic!("description must be text");
                }
            }
            if description.contains("third") || description.len() > 100 {
                assert!(
                    matches!(&scene.primitives[lines[1]], bead::render::Primitive::Text { text, .. } if text.ends_with('…'))
                );
            }
        }
        let frame = h.render()?;
        let golden = root()
            .join("tests/goldens")
            .join(format!("launcher-two-line-{scale}.png"));
        if std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1") {
            frame.save(&golden)?;
        }
        frame.compare(
            &golden,
            bead_test::Tolerance {
                channel: 0,
                changed_fraction: 0.,
            },
            &std::env::temp_dir().join("bead-launcher-two-line"),
        )?;
    }
    Ok(())
}

#[test]
fn plugins_are_discoverable_and_navigate_to_the_existing_prefix() -> Result<()> {
    let mut h = harness("", 0.)?;
    assert!(
        h.session
            .view_nodes()
            .iter()
            .any(|n| n.action.as_deref() == Some("navigate")
                && n.action_arg == Some(Value::Text("plugins".into())))
    );
    assert!(
        h.session
            .view_nodes()
            .iter()
            .any(|n| n.action.as_deref() == Some("launcher.app")
                && n.action_arg == Some(Value::Text("widgets".into())))
    );
    assert!(crate::app::APPS.contains(&"widgets"));
    let changes = h
        .session
        .action_values("navigate", Value::Text("plugins".into()))?;
    h.update(changes)?;
    assert_eq!(
        h.session.stores.get("launcher.search.text"),
        Some(&Value::Text(">plugin ".into()))
    );
    assert_eq!(
        h.session.stores.get("launcher.engine_route"),
        Some(&Value::Text(String::new()))
    );
    Ok(())
}

#[test]
fn launcher_entry_table_icons_resolve_to_real_symbols() -> Result<()> {
    let table: serde_json::Value =
        serde_json::from_str(include_str!("../tests/launcher-actions.json"))?;
    // This is the shell's actual rendering table: values are codepoints from the pinned
    // upstream Symbols font, rather than desktop-theme lookups or placeholder images.
    let registry = include_str!("../../../shell/ui/components/glyphs.luau");
    let glyphs = registry
        .lines()
        .filter_map(|line| {
            let (name, point) = line.trim().strip_prefix("[\"")?.split_once("\"]=0x")?;
            let codepoint = u32::from_str_radix(point.trim_end_matches(','), 16).ok()?;
            Some((name, codepoint))
        })
        .collect::<BTreeMap<_, _>>();
    for row in ["catalogue", "providers", "navigation", "blocked"]
        .iter()
        .flat_map(|key| table[*key].as_array().unwrap())
    {
        let icon = row["icon"].as_str().expect("every audited row has an icon");
        let codepoint = *glyphs
            .get(icon)
            .unwrap_or_else(|| panic!("{}: missing {icon}", row["id"]));
        assert!(char::from_u32(codepoint).is_some() && (0xe000..=0xf8ff).contains(&codepoint));
    }
    assert!(glyphs.contains_key("extension") && glyphs.contains_key("widgets"));
    Ok(())
}

#[test]
fn optional_hint_chips_follow_the_adapter_flags() -> Result<()> {
    // Hint labels are anonymous text draws; inspect the rendered scene, not named nodes.
    let mut h = harness_at_scale("", 0., 1.)?;
    let labels = |h: &Harness| {
        h.scene()
            .primitives
            .iter()
            .filter_map(|draw| {
                if let bead::render::Primitive::Text { text, .. } = draw {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
    };
    h.render()?;
    assert!(
        !labels(&h)
            .iter()
            .any(|text| text.starts_with("# ") || text.starts_with("? "))
    );
    h.update(BTreeMap::from([
        ("launcher.hint_brain".into(), Value::Bool(true)),
        ("launcher.hint_ai".into(), Value::Bool(true)),
    ]))?;
    h.render()?;
    assert!(
        labels(&h).iter().any(|text| text.starts_with("# ")),
        "visible labels: {:?}; brain={:?}",
        labels(&h),
        h.session.resolve("launcher.hint_brain")?
    );
    assert!(labels(&h).iter().any(|text| text.starts_with("? ")));
    h.update(BTreeMap::from([(
        "launcher.hint_brain".into(),
        Value::Bool(false),
    )]))?;
    h.render()?;
    assert!(!labels(&h).iter().any(|text| text.starts_with("# ")));
    assert!(labels(&h).iter().any(|text| text.starts_with("? ")));
    Ok(())
}
