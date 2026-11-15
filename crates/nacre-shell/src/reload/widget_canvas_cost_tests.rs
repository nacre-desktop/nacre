// SPDX-License-Identifier: GPL-3.0-only
//! B0211: the desktop widget canvas repaint cost. A digital clock's minute tick re-rasters its
//! glowing face (text + shadow blur); the blur must not dominate that redraw.
use bead::core::Value;
use bead::render::{Damage, PixelFormat, Primitive, RenderTarget, Scene, ShmTarget};
use bead_test::{Config, Harness};
use serde_json::json;
use std::{collections::BTreeMap, path::Path, time::Duration};

fn clock_canvas() -> Harness {
    clock_canvas_at("digital", 320, 300, "12:00", "12:00:00")
}

fn clock_canvas_at(style: &str, w: u32, h: u32, time: &str, seconds: &str) -> Harness {
    let shell = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let doc = json!({"version":1,"preset":"Nacre default","hosts":{
        "top_bar":{"tiles":60,"mode":"content","regions":{"left":{"start":0,"span":60},
            "centre":{"start":0,"span":60},"right":{"start":0,"span":60}},"instances":[]},
        "canvas":{"width":w+80,"height":h+100,"instances":[{"iid":"clock-2","widget":"clock","group":"clock-2",
            "host":"canvas","style":style,"settings":{},"slot":{"x":40,"y":50,"w":w,"h":h}}]}},
        "widgets":{"clock-2":{"widget":"clock","settings":{},"forms":{"desktop":{"enabled":true,"style":style,"placement":{}}}}}});
    let mut stores = crate::desktop::values(&Default::default(), "");
    stores.insert("widgets.layout".into(), Value::list(vec![doc]));
    stores.insert("bead.desktop.clock".into(), Value::Text(time.into()));
    stores.insert(
        "bead.desktop.clock.seconds".into(),
        Value::Text(seconds.into()),
    );
    stores.insert(
        "bead.desktop.clock.date".into(),
        Value::Text("2026-10-30".into()),
    );
    Harness::load(
        &shell,
        Path::new("widgets/editor/canvas.luau"),
        &shell.join("palette.json"),
        stores,
        Config {
            width: w + 80,
            height: h + 100,
            scale: 1.5,
        },
    )
    .unwrap()
}

/// The desktop clock's face is drawn whole: no time, in any style or slot, is wider than its box
/// (a clipped last digit was seen at 00:24 to 00:30).
#[test]
fn the_desktop_clock_face_fits_its_box_at_every_size() {
    for (style, w, h) in [("digital", 420, 220), ("time", 220, 92), ("seconds", 240, 92)] {
        for time in ["00:24", "00:30", "08:08", "23:59"] {
            let harness = clock_canvas_at(style, w, h, time, &format!("{time}:30"));
            let mut faces = 0;
            for primitive in &harness.scene().primitives {
                let Primitive::Text {
                    rect,
                    text,
                    family,
                    size,
                    ..
                } = primitive
                else {
                    continue;
                };
                if family != "Outfit" {
                    continue;
                }
                faces += 1;
                let drawn = bead::render::text_width(harness.scene(), text, family, *size).unwrap();
                assert!(
                    drawn <= rect.w + 0.5,
                    "{style} {w}x{h} {time}: {text:?} is {drawn}px in a {}px box",
                    rect.w
                );
            }
            assert!(faces >= 1, "{style} {time}: the face is drawn");
        }
    }
}

/// Median redraw time of the canvas over minute ticks, one renderer as on the live surface.
fn tick_cost(keep_shadow: bool) -> Duration {
    let mut harness = clock_canvas();
    let mut renderer = bead::render::renderer(bead::render::RendererKind::Cpu).unwrap();
    let mut pixels = vec![0u8; 600 * 600 * 4];
    let mut samples = Vec::new();
    for minute in 1..=7 {
        harness
            .update(BTreeMap::from([(
                "bead.desktop.clock".into(),
                Value::Text(format!("12:{minute:02}")),
            )]))
            .unwrap();
        let mut scene: Scene = harness.scene().clone();
        let shadows = scene
            .primitives
            .iter_mut()
            .filter_map(|primitive| match primitive {
                Primitive::Text { shadow, .. } => Some(shadow),
                _ => None,
            })
            .filter(|shadow| shadow.is_some())
            .map(|shadow| {
                if !keep_shadow {
                    *shadow = None;
                }
            })
            .count();
        assert!(shadows >= 1, "the digital clock face glows");
        let started = std::time::Instant::now();
        renderer.prepare(&scene, 1.5).unwrap();
        let target = ShmTarget {
            bytes: &mut pixels,
            width: 600,
            height: 600,
            scale: 1.5,
            format: PixelFormat::Rgba,
        };
        renderer
            .draw(&scene, RenderTarget::Shm(target), Damage::full(600, 600))
            .unwrap();
        samples.push(started.elapsed());
    }
    samples.sort();
    samples[samples.len() / 2]
}

#[test]
fn a_clock_tick_redraws_the_desktop_canvas_at_a_bounded_cost() {
    let (plain, glowing) = (tick_cost(false), tick_cost(true));
    eprintln!("CANVAS_TICK plain={plain:?} glowing={glowing:?}");
    assert!(
        glowing < plain * 4 + Duration::from_millis(1),
        "the glow costs {glowing:?} against {plain:?} without it"
    );
}
