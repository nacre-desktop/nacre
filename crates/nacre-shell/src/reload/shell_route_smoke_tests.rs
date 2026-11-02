// SPDX-License-Identifier: GPL-3.0-only
//! Exercise the actual Luau shell entries through the panel Runtime.
use super::*;

fn fake_source(data: serde_json::Value) -> std::sync::Arc<dyn bead::core::source::Source> {
    std::sync::Arc::new(bead_test::source::FakeSource::new(data).expect("fixture source"))
}

fn core_network() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},"wireless_enabled":true,
        "devices":{"items":{"wifi0":{"name":"Wi-Fi","kind":"wireless","connected":true}},"order":["wifi0"]},
        "access_points":{"items":{"ap0":{"device":"wifi0","name":"Home Wi-Fi","strength":86,"secured":true,"active":true,"connection":"wifi-connection"}},"order":["ap0"]},
        "connections":{"items":{"wired":{"name":"Ethernet","kind":"wired","active":true},"wifi-connection":{"name":"Wi-Fi profile","kind":"wireless","active":true}},"order":["wired","wifi-connection"]},
        "primary_connection":"wifi-connection"
    })
}

fn core_bluetooth() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},
        "adapters":{"items":{"adapter":{"name":"Fixture adapter","powered":true,"discovering":false}},"order":["adapter"]},
        "devices":{"items":{"headphones":{"adapter":"adapter","name":"Fixture headphones","connected":true,"paired":true,"trusted":true,"pairing":false}},"order":["headphones"]}
    })
}

fn core_battery() -> serde_json::Value {
    serde_json::json!({"health":{"available":true,"stale":false},"percentage":83.0,"charging":true,"state":1,"time_to_full_seconds":5400,"on_battery":false})
}

fn core_audio() -> serde_json::Value {
    serde_json::json!({
        "health":{"available":true,"stale":false},"default_output":"out","default_input":"mic",
        "nodes":{"items":{"out":{"name":"Fixture output","kind":"output","volume":0.5,"muted":false,"ready":true,"readback":{"linear_volume":0.5,"perceptual_volume":0.5,"muted":false}},"mic":{"name":"Fixture mic","kind":"input","volume":0.0,"muted":false,"ready":true}},"order":["mic","out"]}
    })
}

/// B0185: a mid-slide frame of a route's open scene, in both directions, composites one
/// flattened content layer instead of repainting the content per frame.
fn slide_layer_builds(open_scene: &bead::render::Scene, opening: bool) -> usize {
    use bead::render::{Damage, PixelFormat, Renderer, ShmTarget, tree::TreeCpuRenderer};
    let scale = 1.5;
    let mut base = open_scene.clone();
    base.reveal(1.);
    let mut worker = bead::render::tree::TreeCache::default();
    worker.prepare(&base, scale).expect("worker bake");
    base.prepared
        .insert(scale.to_bits(), worker.frame.clone().expect("baked frame"));
    let (w, h) = (
        (base.bounds.w * scale).ceil() as u32,
        (base.bounds.h * scale).ceil() as u32,
    );
    let mut bytes = vec![0u8; w as usize * h as usize * 4];
    let mut renderer = TreeCpuRenderer::default();
    for progress in [0.3, 0.6, 0.9] {
        let mut scene = base.clone();
        let (dx, dy) = scene.panel_offset(progress, opening);
        scene.offset = (scene.offset.0 + dx, scene.offset.1 + dy);
        renderer
            .prepare_on_ui(&scene, scale)
            .expect("prepare_on_ui");
        renderer
            .render(
                &scene,
                ShmTarget {
                    bytes: &mut bytes,
                    width: w,
                    height: h,
                    scale,
                    format: PixelFormat::Rgba,
                },
                Damage::full(w, h),
            )
            .expect("slide frame renders");
    }
    renderer.slide_layer_builds
}

/// Model Niri's single atlas plane at integer physical-pixel translations. This is a
/// deterministic planner-vs-legacy pixel check; the compositor's GL wrapper is tested in
/// nacre-niri, so this deliberately doesn't claim to exercise that renderer.
fn compose_motion_plane(
    plan: &bead::render::layer_motion::LayerMotionPlan,
    progress: f32,
) -> Vec<u8> {
    assert_eq!(plan.planes.len(), 1, "test compositor uses one plane");
    let plane = &plan.planes[0];
    let pose = plane
        .from
        .interpolate(plane.to, progress, plane.opacity_mode);
    let width = plan.atlas.width as i32;
    let height = plan.atlas.height as i32;
    let scale = plan.atlas.scale;
    let dest_x = (pose.destination.x * scale).round() as i32;
    let dest_y = (pose.destination.y * scale).round() as i32;
    let clip_left = (pose.clip.x * scale).round() as i32;
    let clip_top = (pose.clip.y * scale).round() as i32;
    let clip_right = ((pose.clip.x + pose.clip.w) * scale).round() as i32;
    let clip_bottom = ((pose.clip.y + pose.clip.h) * scale).round() as i32;
    assert_eq!(
        (pose.destination.w * scale).round() as i32,
        width,
        "this planner path preserves atlas width"
    );
    assert_eq!(
        (pose.destination.h * scale).round() as i32,
        height,
        "this planner path preserves atlas height"
    );
    let mut output = vec![0; plan.atlas.pixels.len()];
    for y in 0..height {
        if y < clip_top || y >= clip_bottom {
            continue;
        }
        let source_y = y - dest_y;
        if !(0..height).contains(&source_y) {
            continue;
        }
        for x in 0..width {
            if x < clip_left || x >= clip_right {
                continue;
            }
            let source_x = x - dest_x;
            if !(0..width).contains(&source_x) {
                continue;
            }
            let source = ((source_y * width + source_x) * 4) as usize;
            let destination = ((y * width + x) * 4) as usize;
            for channel in 0..4 {
                output[destination + channel] = (f32::from(plan.atlas.pixels[source + channel])
                    * pose.opacity)
                    .round()
                    .clamp(0., 255.) as u8;
            }
        }
    }
    output
}

fn render_legacy_close(
    scene: &bead::render::Scene,
    progress: f32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    use bead::render::{Damage, PixelFormat, Renderer, ShmTarget, tree::TreeCpuRenderer};
    let mut posed = scene.clone();
    let (dx, dy) = posed.panel_offset(progress, false);
    posed.offset = (posed.offset.0 + dx, posed.offset.1 + dy);
    posed.opacity = posed
        .motion
        .as_ref()
        .expect("route motion")
        .opacity(progress);
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let mut renderer = TreeCpuRenderer::default();
    renderer
        .prepare_on_ui(&posed, 1.5)
        .expect("legacy pose prepares");
    renderer
        .render(
            &posed,
            ShmTarget {
                bytes: &mut pixels,
                width,
                height,
                scale: 1.5,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )
        .expect("legacy close frame paints");
    pixels
}

fn worst_pixel_difference(left: &[u8], right: &[u8]) -> u8 {
    assert_eq!(left.len(), right.len());
    left.iter()
        .zip(right)
        .map(|(a, b)| a.abs_diff(*b))
        .max()
        .unwrap_or_default()
}

#[test]
fn real_shell_panel_routes_open_and_close_with_prepared_scenes() {
    let mut layered = Vec::new();
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette).expect("shell palette loads");
    let fallback = crate::sound::fallback(colors.get("surface").expect("surface token"));
    let routes = [
        ("dashboard", "dashboard/today/main.luau"),
        ("session", "session/main.luau"),
        ("launcher", "ui/main.luau"),
        ("network", "status/network.luau"),
        ("bluetooth", "status/bluetooth.luau"),
        ("battery", "status/battery.luau"),
        ("sound", "sound/main.luau"),
        ("sound-core", "sound/core.luau"),
        ("calendar", "popouts/main.luau"),
        ("chat", "chat/main.luau"),
        ("prompt", "prompt/main.luau"),
        ("control", "control/main.luau"),
        ("wallpaper", "extras/main.luau"),
    ];
    let compositor_closes = [
        "dashboard",
        "session",
        "launcher",
        "network",
        "bluetooth",
        "battery",
        "sound",
        "sound-core",
        "calendar",
        "chat",
        "prompt",
        "control",
    ];
    let mut compositor_close_routes = Vec::new();
    let mut reveal_open_fallback_routes = Vec::new();
    let mut flat_open_routes = Vec::new();
    for (name, entry) in routes {
        let session = bead::ui::Session::new(
            root.clone(),
            entry.into(),
            palette.clone(),
            colors.clone(),
            fallback.clone(),
        );
        let mut runtime = Runtime::new(session);
        runtime.state_file = None;
        let mut restored = BTreeMap::from([("panel.open".into(), bead::core::Value::Bool(false))]);
        if name == "wallpaper" {
            restored.insert(
                "extras.kind".into(),
                bead::core::Value::Text("wallpaper".into()),
            );
        }
        runtime.session.restore_on_start(restored);
        runtime = match name {
            "network" => runtime
                .with_core_network_status(fake_source(core_network()))
                .expect("network projector"),
            "bluetooth" => runtime
                .with_core_bluetooth_status(fake_source(core_bluetooth()))
                .expect("Bluetooth projector"),
            "battery" => runtime
                .with_core_battery_status(fake_source(core_battery()))
                .expect("battery projector"),
            "sound" => runtime
                .with_core_audio_status(fake_source(core_audio()), None)
                .expect("sound projector"),
            "sound-core" => runtime
                .with_view_edge("top")
                .expect("sound edge")
                .with_panel_geometry(382., 286., 0.23)
                .expect("sound geometry")
                .with_frame(crate::placement::Attachment::Popout, &colors)
                .expect("sound frame")
                .with_core_audio_status(fake_source(core_audio()), None)
                .expect("core sound projector"),
            "control" => runtime
                .with_control_center(crate::control_center::ControlCenter::new(
                    None,
                    Vec::new(),
                    None,
                    Vec::new(),
                ))
                .expect("control-center projector"),
            _ => runtime,
        };
        if name == "network" {
            runtime.session.stores = runtime
                .session
                .stores
                .reconcile(&BTreeMap::from([
                    ("network.wifi_enabled".into(), bead::core::Value::Bool(true)),
                    ("network.wifi_checked".into(), bead::core::Value::Bool(true)),
                    ("network.rows".into(), bead::core::Value::list(Vec::new())),
                    (
                        "network.view".into(),
                        bead::core::Value::Text("list".into()),
                    ),
                    (
                        "network.wifi_message".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "network.link_visible".into(),
                        bead::core::Value::Bool(false),
                    ),
                    (
                        "network.link_message".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    ("network.scroll".into(), bead::core::Value::Number(0.)),
                    ("network.can_hidden".into(), bead::core::Value::Bool(false)),
                    ("network.can_forget".into(), bead::core::Value::Bool(false)),
                    ("network.saved".into(), bead::core::Value::list(Vec::new())),
                    (
                        "network.scan_enabled".into(),
                        bead::core::Value::Bool(false),
                    ),
                    (
                        "network.view".into(),
                        bead::core::Value::Text("list".into()),
                    ),
                ]))
                .expect("network route state");
        }
        if name == "bluetooth" {
            runtime.session.stores = runtime
                .session
                .stores
                .reconcile(&BTreeMap::from([
                    (
                        "bluetooth.power_visible".into(),
                        bead::core::Value::Bool(false),
                    ),
                    (
                        "bluetooth.power_checked".into(),
                        bead::core::Value::Bool(true),
                    ),
                    (
                        "bluetooth.power_enabled".into(),
                        bead::core::Value::Bool(true),
                    ),
                    (
                        "bluetooth.power_busy".into(),
                        bead::core::Value::Bool(false),
                    ),
                    (
                        "bluetooth.power_message".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.devices".into(),
                        bead::core::Value::list(Vec::new()),
                    ),
                    (
                        "bluetooth.list_message".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.scan_label".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.scan_enabled".into(),
                        bead::core::Value::Bool(false),
                    ),
                    ("bluetooth.scanning".into(), bead::core::Value::Bool(false)),
                    (
                        "bluetooth.empty_text".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.view".into(),
                        bead::core::Value::Text("list".into()),
                    ),
                    (
                        "bluetooth.pair_title".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.pair_kind".into(),
                        bead::core::Value::Text("waiting".into()),
                    ),
                    (
                        "bluetooth.pair_text".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.pair_countdown".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    (
                        "bluetooth.pair_error".into(),
                        bead::core::Value::Text(String::new()),
                    ),
                    ("bluetooth.pair_busy".into(), bead::core::Value::Bool(false)),
                    (
                        "bluetooth.pair_retry".into(),
                        bead::core::Value::Bool(false),
                    ),
                ]))
                .expect("Bluetooth route state");
        }
        if name == "sound" {
            runtime.session.stores = runtime
                .session
                .stores
                .reconcile(&crate::desktop::values(
                    &bead::services::Snapshot::default(),
                    "",
                ))
                .expect("desktop audio view state");
        }
        runtime.viewport(1920, 1200);
        runtime.services(Box::new(|_| {}));

        for (event, open) in [
            (UiEvent::Reload, None),
            (UiEvent::PanelVisibility(true), Some(true)),
            (UiEvent::PanelVisibility(false), Some(false)),
        ] {
            let action = match open {
                None => "reload",
                Some(true) => "open",
                Some(false) => "close",
            };
            let mut scene = runtime
                .stage(event)
                .unwrap_or_else(|error| panic!("{name}/{action}: stage failed: {error}"))
                .unwrap_or_else(|| panic!("{name}/{action}: stage produced no scene"));
            assert!(
                !scene.primitives.is_empty(),
                "{name}: stage produced no prepared scene"
            );
            if action == "open" {
                if compositor_closes.contains(&name) {
                    use bead::render::{
                        PixelFormat, layer_motion::LayerMotionPlan, tree::TreeCpuRenderer,
                    };
                    let (width, height) =
                        (scene.bounds.w.ceil() as u32, scene.bounds.h.ceil() as u32);
                    // The real runtime can still be waiting for its async CPU cache
                    // request. Bake and attach the same worker resource the production
                    // planner requires before asking it to paint the one-shot atlas.
                    let mut prepared = scene.clone();
                    prepared.reveal(1.);
                    let mut worker = bead::render::tree::TreeCache::default();
                    worker
                        .prepare(&prepared, 1.5)
                        .unwrap_or_else(|error| panic!("{name}: worker bake failed: {error}"));
                    scene.prepared.insert(
                        1.5f32.to_bits(),
                        worker.frame.clone().expect("worker bake produces frame"),
                    );
                    let reveal = scene.motion.as_ref().is_some_and(|motion| motion.reveal);

                    let mut renderer = TreeCpuRenderer::default();
                    let opening = LayerMotionPlan::for_slide(
                        &scene,
                        true,
                        width,
                        height,
                        1.5,
                        PixelFormat::Rgba,
                        &mut renderer,
                    )
                    .unwrap_or_else(|error| panic!("{name}: open plan failed: {error}"));
                    if reveal {
                        assert!(
                            opening.is_none(),
                            "{name}: a growing-body reveal must stay on the current CPU path until its exact shape is representable"
                        );
                        reveal_open_fallback_routes.push(name);
                    } else {
                        let opening = opening.unwrap_or_else(|| {
                            panic!("{name}: flat opening should produce a compositor plan")
                        });
                        assert_eq!(opening.planes.len(), 1, "{name}: one motion plane");
                        flat_open_routes.push(name);
                    }

                    let mut renderer = TreeCpuRenderer::default();
                    let closing = LayerMotionPlan::for_slide(
                        &scene,
                        false,
                        width,
                        height,
                        1.5,
                        PixelFormat::Rgba,
                        &mut renderer,
                    )
                    .unwrap_or_else(|error| panic!("{name}: close plan failed: {error}"))
                    .unwrap_or_else(|| panic!("{name}: closing should produce a compositor plan"));
                    assert_eq!(closing.planes.len(), 1, "{name}: one close motion plane");
                    assert!(
                        !closing.atlas.pixels.is_empty(),
                        "{name}: closing atlas contains the complete surface"
                    );
                    for close_progress in [1., 0.75, 0.5, 0.25, 0.] {
                        let legacy = render_legacy_close(
                            &scene,
                            close_progress,
                            closing.atlas.width,
                            closing.atlas.height,
                        );
                        let compositor = compose_motion_plane(&closing, 1. - close_progress);
                        let worst = worst_pixel_difference(&legacy, &compositor);
                        assert!(
                            worst <= 2,
                            "{name}: close p={close_progress} atlas-vs-legacy channel diff {worst}"
                        );
                    }
                    compositor_close_routes.push(name);
                }
                if scene.motion.as_ref().is_some_and(|m| m.reveal) {
                    for opening in [true, false] {
                        let builds = slide_layer_builds(&scene, opening);
                        assert!(builds <= 1, "{name}: {builds} slide layers in one slide");
                        if builds == 1 {
                            layered.push((name, opening));
                        }
                    }
                }
                scene.reveal(0.);
            }
            let mut cache = bead::render::tree::TreeCache::default();
            cache
                .prepare(&scene, 1.5)
                .unwrap_or_else(|error| panic!("{name}: CPU scene preparation failed: {error}"));
            assert!(cache.frame.is_some(), "{name}: CPU frame was not prepared");
            runtime.commit();
            if let Some(open) = open {
                assert_eq!(
                    runtime.session.stores.get("panel.open"),
                    Some(&bead::core::Value::Bool(open)),
                    "{name}: panel.open reflects visibility"
                );
            }
        }
    }
    for route in ["dashboard", "launcher", "control", "calendar"] {
        for opening in [true, false] {
            assert!(
                layered.contains(&(route, opening)),
                "{route}: a slide (opening={opening}) must composite its content layer; got {layered:?}"
            );
        }
    }
    for route in [
        "dashboard",
        "launcher",
        "control",
        "calendar",
        "session",
        "chat",
    ] {
        assert!(
            compositor_close_routes.contains(&route),
            "{route}: real shell route produces a compositor-driven close plan"
        );
        assert!(
            reveal_open_fallback_routes.contains(&route),
            "{route}: reveal opening is explicitly known to use the CPU fallback until exact growing-body geometry is supported"
        );
    }
    assert!(
        flat_open_routes.contains(&"prompt"),
        "prompt: flat opening is compositor-eligible"
    );
}
