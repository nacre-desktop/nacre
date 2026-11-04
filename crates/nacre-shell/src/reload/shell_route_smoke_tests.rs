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
    use bead::render::{tree::TreeCpuRenderer, Damage, PixelFormat, Renderer, ShmTarget};
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
    use bead::render::{tree::TreeCpuRenderer, Damage, PixelFormat, Renderer, ShmTarget};
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

/// Render a real shell route through the existing per-frame CPU reveal path. These
/// frames are consumed by the compositor raster-parity test; the export path is set
/// only by the dev-1 fixture-generation command.
fn render_legacy_reveal(
    scene: &bead::render::Scene,
    eased_progress: f32,
    scale: f32,
) -> (u32, u32, Vec<u8>) {
    use bead::render::{tree::TreeCpuRenderer, Damage, PixelFormat, Renderer, ShmTarget};
    let mut open_scene = scene.clone();
    // Runtime::stage may attach an asynchronous, partially prepared frame to
    // the scene. Never let that frame seed the resting OPEN snapshot: the
    // worker must rasterize the final reveal=1 scene exactly once.
    open_scene.prepared.clear();
    open_scene.reveal(1.);
    let mut worker = bead::render::tree::TreeCache::default();
    worker
        .prepare(&open_scene, scale)
        .expect("resting open reveal worker resources prepare");
    let prepared_open_frame = worker.frame.clone().expect("prepared open CPU frame");
    let mut posed = scene.clone();
    posed.reveal(1.);
    posed.reveal(eased_progress);
    let (dx, dy) = posed.panel_offset(eased_progress, true);
    posed.offset = (posed.offset.0 + dx, posed.offset.1 + dy);
    posed.opacity = posed
        .motion
        .as_ref()
        .expect("reveal motion")
        .opacity(eased_progress);
    let (width, height) = (
        (posed.bounds.w * scale).ceil() as u32,
        (posed.bounds.h * scale).ceil() as u32,
    );
    posed.prepared.insert(scale.to_bits(), prepared_open_frame);
    let mut pixels = vec![0; width as usize * height as usize * 4];
    let mut renderer = TreeCpuRenderer::default();
    renderer
        .prepare_on_ui(&posed, scale)
        .expect("legacy reveal pose prepares");
    renderer
        .render(
            &posed,
            ShmTarget {
                bytes: &mut pixels,
                width,
                height,
                scale,
                format: PixelFormat::Rgba,
            },
            Damage::full(width, height),
        )
        .expect("legacy reveal frame paints");
    (width, height, pixels)
}

fn rect_json(rect: bead::render::Rect) -> serde_json::Value {
    serde_json::json!({"x":rect.x,"y":rect.y,"w":rect.w,"h":rect.h})
}

fn nacre_reveal_attachment(route: &str) -> Option<crate::placement::Attachment> {
    match route {
        "dashboard" => Some(crate::placement::Attachment::Attached(
            bead::core::Edge::Top,
        )),
        "launcher" => Some(crate::placement::Attachment::Attached(
            bead::core::Edge::Bottom,
        )),
        "control" => Some(crate::placement::Attachment::ControlCenter),
        _ => None,
    }
}

fn surface_global_origin(route: &str, scene: &bead::render::Scene) -> (i32, i32) {
    crate::placement::Frame::DEFAULT
        .positioner(nacre_reveal_attachment(route).expect("target reveal route"))(
        scene,
        None,
        (1920, 1200),
    )
}

fn motion_plan_json(
    route: &str,
    scene: &bead::render::Scene,
    plan: &bead::render::layer_motion::LayerMotionPlan,
) -> serde_json::Value {
    use bead::render::{layer_motion::OpacityMode, PixelFormat};
    let state = |state: bead::render::layer_motion::PlaneState| {
        serde_json::json!({
            "destination": rect_json(state.destination),
            "clip": rect_json(state.clip),
            "opacity": state.opacity
        })
    };
    let planes = plan
        .planes
        .iter()
        .map(|plane| {
            let opacity_mode = match plane.opacity_mode {
                OpacityMode::Linear => serde_json::json!({"kind":"linear"}),
                OpacityMode::OpaqueAbove(threshold) => {
                    serde_json::json!({"kind":"opaque_above","threshold":threshold})
                }
            };
            serde_json::json!({
                "id": plane.id,
                "source": rect_json(plane.source),
                "from": state(plane.from),
                "to": state(plane.to),
                "opacity_mode": opacity_mode
            })
        })
        .collect::<Vec<_>>();
    let reveal = plan.reveal.as_ref().map(|contour| {
        let rim = contour.rim.map(|rim| {
            serde_json::json!({
                "colors": rim.colors,
                "stops": rim.stops,
                "start": rim.start,
                "end": rim.end,
                "width": rim.width,
                "opacity": rim.opacity
            })
        });
        let shadow = contour.shadow.map(|shadow| {
            serde_json::json!({
                "color": shadow.color,
                "blur": shadow.blur,
                "spread": shadow.spread,
                "offset": shadow.offset
            })
        });
        serde_json::json!({
            "output_bounds": rect_json(contour.output_bounds),
            "crop": rect_json(contour.crop),
            "static_rects": contour.static_rects.iter().copied().map(rect_json).collect::<Vec<_>>(),
            "holes": contour.holes.iter().copied().map(rect_json).collect::<Vec<_>>(),
            "animated_from": rect_json(contour.animated_from),
            "animated_to": rect_json(contour.animated_to),
            "content_clip_from": rect_json(contour.content_clip_from),
            "content_clip_to": rect_json(contour.content_clip_to),
            "radii_from": contour.radii_from,
            "radii_to": contour.radii_to,
            "corner_mode": format!("{:?}", contour.corner_mode),
            "mode": format!("{:?}", contour.mode),
            "opacity_plane": contour.opacity_plane,
            "square_edges": contour.square_edges,
            "fill": contour.fill,
            "rim": rim,
            "shadow": shadow
        })
    });
    let global_origin = surface_global_origin(route, scene);
    serde_json::json!({
        "schema": "nacre-bead-layer-motion-reveal-fixture-v1",
        "duration_ms": plan.duration.as_secs_f64() * 1000.,
        "curve": format!("{:?}", plan.curve),
        "retarget": plan.retarget,
        "canvas": {
            "output_logical_size": [1920, 1200],
            "scene_bounds": rect_json(scene.bounds),
            "scene_offset": [scene.offset.0, scene.offset.1],
            "surface_global_origin": [global_origin.0, global_origin.1],
            "atlas_is_surface_local": true,
            "buffer_pixel_origin": [0, 0],
            "surface_position_is_applied_by_layer_surface": true
        },
        "atlas": {
            "width": plan.atlas.width,
            "height": plan.atlas.height,
            "stride": plan.atlas.stride,
            "scale": plan.atlas.scale,
            "format": match plan.atlas.format { PixelFormat::Rgba => "RGBA", PixelFormat::Bgra => "BGRA" },
            "filename": "ROUTE-SCALE-atlas.png"
        },
        "planes": planes,
        "reveal": reveal,
        "source": "Nacre Runtime::stage PanelVisibility(true), with_frame production attachment, Bead LayerMotionPlan::for_slide"
    })
}

fn tight_plan_atlas(plan: &bead::render::layer_motion::LayerMotionPlan) -> Vec<u8> {
    let row_bytes = plan.atlas.width as usize * 4;
    let stride = plan.atlas.stride as usize;
    assert!(stride >= row_bytes);
    let mut pixels = Vec::with_capacity(row_bytes * plan.atlas.height as usize);
    for row in 0..plan.atlas.height as usize {
        let start = row * stride;
        pixels.extend_from_slice(&plan.atlas.pixels[start..start + row_bytes]);
    }
    pixels
}

fn verify_or_export_motion_plan(
    route: &str,
    scale: f32,
    scene: &bead::render::Scene,
    plan: &bead::render::layer_motion::LayerMotionPlan,
) {
    use image::ImageEncoder;
    let fixture_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bead-layer-motion/reveal-goldens");
    let export_dir = std::env::var_os("NACRE_REVEAL_REFERENCE_DIR").map(std::path::PathBuf::from);
    let directory = export_dir.as_deref().unwrap_or(&fixture_dir);
    let stem = format!("{route}-s{scale:.1}");
    let atlas_name = format!("{stem}-atlas.png");
    let metadata_name = format!("{stem}-plan.json");
    let mut metadata = motion_plan_json(route, scene, plan);
    metadata["atlas"]["filename"] = serde_json::Value::String(atlas_name.clone());
    let atlas_bytes = tight_plan_atlas(plan);
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(
            &atlas_bytes,
            plan.atlas.width,
            plan.atlas.height,
            image::ExtendedColorType::Rgba8,
        )
        .expect("encode route compositor content atlas");
    if let Some(directory) = &export_dir {
        std::fs::create_dir_all(directory).expect("create motion plan reference directory");
        std::fs::write(directory.join(&atlas_name), &png).expect("write motion content atlas");
        std::fs::write(
            directory.join(&metadata_name),
            serde_json::to_vec_pretty(&metadata).expect("encode motion plan metadata"),
        )
        .expect("write motion plan metadata");
    } else {
        let expected_metadata: serde_json::Value = serde_json::from_slice(
            &std::fs::read(directory.join(&metadata_name))
                .unwrap_or_else(|error| panic!("{metadata_name}: missing fixture: {error}")),
        )
        .unwrap_or_else(|error| panic!("{metadata_name}: invalid fixture: {error}"));
        assert_eq!(metadata, expected_metadata, "{metadata_name}: plan changed");
        let expected_atlas = std::fs::read(directory.join(&atlas_name))
            .unwrap_or_else(|error| panic!("{atlas_name}: missing fixture: {error}"));
        let expected_atlas = image::load_from_memory(&expected_atlas)
            .unwrap_or_else(|error| panic!("{atlas_name}: invalid PNG: {error}"))
            .to_rgba8();
        assert_eq!(
            expected_atlas.dimensions(),
            (plan.atlas.width, plan.atlas.height)
        );
        assert_eq!(
            expected_atlas.as_raw(),
            &atlas_bytes,
            "{atlas_name}: atlas changed"
        );
    }
}

fn export_reveal_reference(
    directory: &std::path::Path,
    route: &str,
    timeline_t: f32,
    eased_progress: f32,
    scale: f32,
    duration_ms: f64,
    curve: bead::core::animation::Curve,
    plan_metadata: &str,
    logical_size: (f32, f32),
    scene_bounds: bead::render::Rect,
    scene_offset: (f32, f32),
    crop: bead::render::Rect,
    surface_origin: (i32, i32),
    width: u32,
    height: u32,
    pixels: &[u8],
) {
    use image::ImageEncoder;
    let stem = format!("{route}-s{scale:.1}-t{timeline_t:.2}");
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new(&mut png)
        .write_image(pixels, width, height, image::ExtendedColorType::Rgba8)
        .expect("encode Nacre CPU reference");
    std::fs::write(directory.join(format!("{stem}.png")), png).expect("write Nacre CPU reference");
    std::fs::write(
        directory.join(format!("{stem}.json")),
        serde_json::to_vec_pretty(&serde_json::json!({
            "route": route,
            "timeline_t": timeline_t,
            "eased_progress": eased_progress,
            "scale": scale,
            "duration_ms": duration_ms,
            "sample_time_ms": duration_ms * f64::from(timeline_t),
            "curve": format!("{curve:?}"),
            "plan_metadata": plan_metadata,
            "content_atlas": plan_metadata.replace("-plan.json", "-atlas.png"),
            "logical_width": logical_size.0,
            "logical_height": logical_size.1,
            "scene_bounds": rect_json(scene_bounds),
            "scene_offset": [scene_offset.0, scene_offset.1],
            "width": width,
            "height": height,
            "crop": { "x": crop.x, "y": crop.y, "width": crop.w, "height": crop.h },
            "surface_global_origin": [surface_origin.0, surface_origin.1],
            "format": "RGBA8",
            "source": "bead-render TreeCpuRenderer with Nacre RevealArt draw callback"
        }))
        .expect("encode Nacre reference metadata"),
    )
    .expect("write Nacre reference metadata");
}

fn assert_reveal_reference(
    route: &str,
    timeline_t: f32,
    eased_progress: f32,
    scale: f32,
    duration_ms: f64,
    curve: bead::core::animation::Curve,
    plan_metadata: &str,
    logical_size: (f32, f32),
    scene_bounds: bead::render::Rect,
    scene_offset: (f32, f32),
    crop: bead::render::Rect,
    surface_origin: (i32, i32),
    width: u32,
    height: u32,
    pixels: &[u8],
) {
    let directory = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/bead-layer-motion/reveal-goldens");
    let stem = format!("{route}-s{scale:.1}-t{timeline_t:.2}");
    let encoded = std::fs::read(directory.join(format!("{stem}.png")))
        .unwrap_or_else(|error| panic!("{stem}: missing CPU reveal reference: {error}"));
    let expected = image::load_from_memory(&encoded)
        .unwrap_or_else(|error| panic!("{stem}: decode CPU reveal reference: {error}"))
        .to_rgba8();
    assert_eq!(
        expected.dimensions(),
        (width, height),
        "{stem}: root dimensions"
    );
    assert_eq!(
        expected.as_raw(),
        pixels,
        "{stem}: Nacre CPU reveal changed from the committed reference"
    );
    let manifest: serde_json::Value = serde_json::from_slice(
        &std::fs::read(directory.join(format!("{stem}.json")))
            .unwrap_or_else(|error| panic!("{stem}: missing CPU reveal metadata: {error}")),
    )
    .unwrap_or_else(|error| panic!("{stem}: parse CPU reveal metadata: {error}"));
    assert_eq!(manifest["route"], route);
    assert_eq!(manifest["timeline_t"], timeline_t);
    assert_eq!(manifest["eased_progress"], eased_progress);
    assert_eq!(manifest["scale"], scale);
    assert_eq!(manifest["duration_ms"], duration_ms);
    let recorded_sample_ms = manifest["sample_time_ms"]
        .as_f64()
        .expect("sample_time_ms is numeric");
    assert!(
        (recorded_sample_ms - duration_ms * f64::from(timeline_t)).abs() < 1e-9,
        "{stem}: sample time agrees with duration and timeline progress"
    );
    assert_eq!(manifest["curve"], format!("{curve:?}"));
    assert_eq!(manifest["plan_metadata"], plan_metadata);
    assert_eq!(
        manifest["content_atlas"],
        plan_metadata.replace("-plan.json", "-atlas.png")
    );
    assert_eq!(manifest["logical_width"], logical_size.0);
    assert_eq!(manifest["logical_height"], logical_size.1);
    assert_eq!(manifest["scene_bounds"], rect_json(scene_bounds));
    assert_eq!(manifest["scene_offset"][0], scene_offset.0);
    assert_eq!(manifest["scene_offset"][1], scene_offset.1);
    assert_eq!(manifest["width"], width);
    assert_eq!(manifest["height"], height);
    assert_eq!(manifest["crop"]["x"], crop.x);
    assert_eq!(manifest["crop"]["y"], crop.y);
    assert_eq!(manifest["crop"]["width"], crop.w);
    assert_eq!(manifest["crop"]["height"], crop.h);
    assert_eq!(manifest["surface_global_origin"][0], surface_origin.0);
    assert_eq!(manifest["surface_global_origin"][1], surface_origin.1);
}

fn interpolate_rect(
    from: bead::render::Rect,
    to: bead::render::Rect,
    progress: f32,
) -> bead::render::Rect {
    let mix = |a: f32, b: f32| a + (b - a) * progress;
    bead::render::Rect::new(
        mix(from.x, to.x),
        mix(from.y, to.y),
        mix(from.w, to.w),
        mix(from.h, to.h),
    )
}

fn eased_progress(curve: bead::core::animation::Curve, t: f32) -> f32 {
    use bead::core::animation::Curve;
    match curve {
        Curve::EaseOut => 1. - (1. - t).powi(3),
        Curve::EaseInOut => {
            if t < 0.5 {
                4. * t.powi(3)
            } else {
                1. - (-2. * t + 2.).powi(3) / 2.
            }
        }
        Curve::EmphasizedAccelerate => t.powi(4),
        Curve::Spring => {
            let x = 8. * t;
            (1. - (1. + x) * (-x).exp()) / (1. - 9. * (-8_f32).exp())
        }
    }
}

fn opening_reveal_contour(
    scene: &bead::render::Scene,
) -> &bead::render::layer_motion::RevealContour {
    scene
        .motion
        .as_ref()
        .and_then(|motion| motion.art.as_ref())
        .and_then(|art| art.contour.as_ref())
        .expect("reveal scene has compositor contour")
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
    let mut reveal_open_routes = Vec::new();
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
        let nacre_reveal_attachment = nacre_reveal_attachment(name);
        if let Some(attachment) = nacre_reveal_attachment {
            runtime = runtime
                .with_frame(attachment, &colors)
                .unwrap_or_else(|error| panic!("{name}: Nacre frame setup failed: {error}"));
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
                if scene.motion.is_some() {
                    // Panel surfaces draw from the padded resting origin. The
                    // production presenter uses the same 32px inset before it
                    // applies the edge-relative motion offset.
                    scene.offset = (32., 32.);
                }
                let mut reveal_timing = None;
                if compositor_closes.contains(&name) {
                    use bead::render::{
                        layer_motion::LayerMotionPlan, tree::TreeCpuRenderer, PixelFormat,
                    };
                    // These padded bounds are the logical layer-surface viewport
                    // used by the route's production panel configuration.
                    let (width, height) =
                        (scene.bounds.w.ceil() as u32, scene.bounds.h.ceil() as u32);
                    // The real runtime can still be waiting for its async CPU cache
                    // request. Bake and attach the same worker resource the production
                    // planner requires before asking it to paint the one-shot atlas.
                    let mut prepared = scene.clone();
                    // A staged route may carry an incomplete worker snapshot.
                    // Reuse is only valid for the complete OPEN resting pose.
                    prepared.prepared.clear();
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
                        if let Some(opening) = opening {
                            if ["dashboard", "launcher", "control"].contains(&name) {
                                verify_or_export_motion_plan(name, 1.5, &scene, &opening);
                                reveal_timing =
                                    Some((opening.curve, opening.duration.as_secs_f64() * 1000.));
                            }
                            let contour = opening.reveal.as_ref().unwrap_or_else(|| {
                                panic!("{name}: reveal plan must carry compositor contour geometry")
                            });
                            assert!(contour.valid(), "{name}: reveal descriptor validates");
                            if ["dashboard", "launcher", "control"].contains(&name) {
                                assert_eq!(
                                    contour.corner_mode,
                                    bead::render::layer_motion::RevealCornerMode::QuadraticCut
                                );
                                assert_eq!(
                                    contour.mode,
                                    bead::render::layer_motion::RevealMode::Visible
                                );
                            }
                            assert!(contour.animated_from.w == 0. || contour.animated_from.h == 0.);
                            assert_eq!(contour.opacity_plane, 0);
                            assert!(contour.animated_to.w > 0. && contour.animated_to.h > 0.);
                            for progress in [0., 0.25, 0.5, 0.75, 1.] {
                                let rect = interpolate_rect(
                                    contour.animated_from,
                                    contour.animated_to,
                                    progress,
                                );
                                let clip = interpolate_rect(
                                    contour.content_clip_from,
                                    contour.content_clip_to,
                                    progress,
                                );
                                assert!(
                                    rect.w >= 0. && rect.h >= 0.,
                                    "{name}: geometry at {progress}"
                                );
                                assert!(clip.w >= 0. && clip.h >= 0., "{name}: clip at {progress}");
                            }
                            reveal_open_routes.push(name);
                        } else {
                            assert!(
                                !["dashboard", "launcher", "control"].contains(&name),
                                "{name}: required Nacre reveal should be compositor eligible"
                            );
                            reveal_open_fallback_routes.push(name);
                        }
                    } else {
                        let opening = opening.unwrap_or_else(|| {
                            panic!("{name}: flat opening should produce a compositor plan")
                        });
                        assert_eq!(opening.planes.len(), 1, "{name}: one motion plane");
                        flat_open_routes.push(name);
                    }

                    if reveal && reveal_open_routes.contains(&name) {
                        let mut scale_one = prepared.clone();
                        let mut scale_one_worker = bead::render::tree::TreeCache::default();
                        scale_one_worker
                            .prepare(&scale_one, 1.)
                            .unwrap_or_else(|error| {
                                panic!("{name}: scale-1 worker bake failed: {error}")
                            });
                        scale_one.prepared.insert(
                            1_f32.to_bits(),
                            scale_one_worker
                                .frame
                                .clone()
                                .expect("scale-1 worker bake produces frame"),
                        );
                        let mut scale_one_renderer = TreeCpuRenderer::default();
                        let scale_one_plan = LayerMotionPlan::for_slide(
                            &scale_one,
                            true,
                            width,
                            height,
                            1.,
                            PixelFormat::Rgba,
                            &mut scale_one_renderer,
                        )
                        .unwrap_or_else(|error| panic!("{name}: scale-1 plan failed: {error}"))
                        .unwrap_or_else(|| panic!("{name}: scale-1 reveal plan unavailable"));
                        assert!(scale_one_plan.reveal.as_ref().is_some_and(|c| c.valid()));
                        if ["dashboard", "launcher", "control"].contains(&name) {
                            verify_or_export_motion_plan(name, 1., &scene, &scale_one_plan);
                            assert_eq!(
                                scale_one_plan.reveal.as_ref().unwrap().animated_to,
                                opening_reveal_contour(&scene)
                                    .translated(scene.offset.0, scene.offset.1)
                                    .animated_to,
                                "{name}: logical contour geometry is scale invariant"
                            );
                            assert_eq!(
                                reveal_timing,
                                Some((
                                    scale_one_plan.curve,
                                    scale_one_plan.duration.as_secs_f64() * 1000.
                                )),
                                "{name}: logical timing is scale invariant"
                            );
                        }
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
                    .unwrap_or_else(|error| panic!("{name}: close plan failed: {error}"));
                    if let Some(closing) = closing {
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
                    } else {
                        assert!(
                            !["dashboard", "launcher", "control"].contains(&name),
                            "{name}: required Nacre close should retain compositor motion"
                        );
                    }
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
                if scene.motion.as_ref().is_some_and(|motion| motion.reveal)
                    && ["dashboard", "launcher", "control"].contains(&name)
                {
                    let contour = opening_reveal_contour(&scene);
                    let export_directory = std::env::var_os("NACRE_REVEAL_REFERENCE_DIR")
                        .map(std::path::PathBuf::from);
                    if let Some(directory) = &export_directory {
                        std::fs::create_dir_all(directory)
                            .expect("create reveal reference directory");
                    }
                    let (curve, duration_ms) = reveal_timing
                        .expect("target Nacre opening stores its actual compositor timing");
                    let root_origin = surface_global_origin(name, &scene);
                    for scale in [1., 1.5] {
                        for timeline_t in [0., 0.25, 0.5, 0.75, 1.] {
                            let progress = eased_progress(curve, timeline_t);
                            let (width, height, pixels) =
                                render_legacy_reveal(&scene, progress, scale);
                            let plan_metadata = format!("{name}-s{scale:.1}-plan.json");
                            if let Some(directory) = &export_directory {
                                export_reveal_reference(
                                    directory,
                                    name,
                                    timeline_t,
                                    progress,
                                    scale,
                                    duration_ms,
                                    curve,
                                    &plan_metadata,
                                    (scene.bounds.w, scene.bounds.h),
                                    scene.bounds,
                                    scene.offset,
                                    contour.crop,
                                    root_origin,
                                    width,
                                    height,
                                    &pixels,
                                );
                            } else {
                                assert_reveal_reference(
                                    name,
                                    timeline_t,
                                    progress,
                                    scale,
                                    duration_ms,
                                    curve,
                                    &plan_metadata,
                                    (scene.bounds.w, scene.bounds.h),
                                    scene.bounds,
                                    scene.offset,
                                    contour.crop,
                                    root_origin,
                                    width,
                                    height,
                                    &pixels,
                                );
                            }
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
    for route in ["dashboard", "launcher", "control"] {
        assert!(
            compositor_close_routes.contains(&route),
            "{route}: real shell route produces a compositor-driven close plan"
        );
        assert!(
            reveal_open_routes.contains(&route),
            "{route}: real shell route produces an exact compositor-driven reveal plan"
        );
    }
    assert!(
        reveal_open_fallback_routes
            .iter()
            .all(|route| !["dashboard", "launcher", "control"].contains(route)),
        "required Nacre reveals must not retain the stock compositor fallback"
    );
    assert!(
        flat_open_routes.contains(&"prompt"),
        "prompt: flat opening is compositor-eligible"
    );
}
