// SPDX-License-Identifier: GPL-3.0-only
//! Placement-only masks. Luau supplies actual body sizes; Qt supplies the
//! independent contour/rim reference. Existing populated content goldens remain.
use super::*;
use bead::render::{Palette, PixelFormat, ShmTarget};
use bead_test::{Config, Frame as Pixels, Harness, Result, Tolerance};
use std::{collections::BTreeMap, path::Path};

/// `clip` (output space) is the primitive's scene clip, as Bead paints it (B0430).
fn tile(canvas: &mut Pixels, rect: Rect, svg: &[u8], scale: f32, clip: Option<Rect>) -> Result<()> {
    let width = (rect.w * scale).ceil() as u32;
    let height = (rect.h * scale).ceil() as u32;
    let mut bytes = vec![0; width as usize * height as usize * 4];
    bead::render::svg_into(
        svg,
        ShmTarget {
            bytes: &mut bytes,
            width,
            height,
            scale,
            format: PixelFormat::Rgba,
        },
    )?;
    let (left, top) = (
        (rect.x * scale).round() as i32,
        (rect.y * scale).round() as i32,
    );
    for y in 0..height as i32 {
        for x in 0..width as i32 {
            let dx = x + left;
            let dy = y + top;
            if dx < 0 || dy < 0 || dx >= canvas.width as i32 || dy >= canvas.height as i32 {
                continue;
            }
            let (cx, cy) = ((dx as f32 + 0.5) / scale, (dy as f32 + 0.5) / scale);
            if clip.is_some_and(|c| cx < c.x || cy < c.y || cx >= c.x + c.w || cy >= c.y + c.h) {
                continue;
            }
            let source = ((y as u32 * width + x as u32) * 4) as usize;
            let target = ((dy as u32 * canvas.width + dx as u32) * 4) as usize;
            let a = bytes[source + 3] as u16;
            for channel in 0..3 {
                canvas.pixels[target + channel] = (bytes[source + channel] as u16
                    + (canvas.pixels[target + channel] as u16 * (255 - a) + 127) / 255)
                    .min(255) as u8;
            }
            canvas.pixels[target + 3] = 255;
        }
    }
    Ok(())
}
fn assert_interior(qml: &Pixels, bead: &Pixels, fill: [u8; 4], name: &str) -> Result<()> {
    let desktop = [0x34, 0x37, 0x3f, 255];
    let mut wrong = 0usize;
    for (i, (a, b)) in qml
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .zip(bead.pixels.as_chunks::<4>().0.iter())
        .enumerate()
    {
        if a == b || (*a != fill && *a != desktop) {
            continue;
        }
        let x = i % qml.width as usize;
        let y = i / qml.width as usize;
        if x < 2 || y < 2 || x + 2 >= qml.width as usize || y + 2 >= qml.height as usize {
            continue;
        }
        // A 5x5 solid neighbourhood excludes antialiasing/stroke boundaries.
        // No amount of whole-output tolerance may hide a stale corner inside it.
        if (-2..=2).all(|dy| {
            (-2..=2).all(|dx| {
                let n = (i as isize + dy * qml.width as isize + dx) as usize;
                qml.pixels[n * 4..n * 4 + 4] == a[..]
            })
        }) {
            wrong += 1;
        }
    }
    if wrong > 0 {
        return Err(format!("{name}: {wrong} stable-interior pixels differ from QML").into());
    }
    Ok(())
}
#[test]
fn all_popout_and_launcher_placement_masks_match_qml() -> Result<()> {
    let _live = crate::placement::LIVE_TEST
        .lock()
        .unwrap_or_else(|p| p.into_inner());
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = Palette::read(&root.join("tests/fixtures/palette-qml.json"))?;
    let cases: serde_json::Value = serde_json::from_str(include_str!(
        "../../../shell/tests/fixtures/placement-qt.json"
    ))?;
    let update = std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1");
    let golden = root.join("tests/goldens/placement");
    let artifacts = std::env::temp_dir().join("bead-placement-pixel-diffs");
    for scale in [1.5] {
        for case in cases.as_array().ok_or("Invalid Qt reference")? {
            let name = case["name"].as_str().ok_or("Missing case name")?;
            let edge = case["edge"].as_str().ok_or("Missing edge")?;
            let entry = match name {
                "calendar" => "popouts/main.luau",
                "network" | "bluetooth" | "battery" => "status/main.luau",
                "sound" | "sound-left" => "sound/main.luau",
                _ => "ui/main.luau",
            };
            let fixture = if entry == "sound/main.luau" {
                crate::desktop::values(&bead::services::Snapshot::default(), "")
            } else {
                BTreeMap::new()
            };
            let mut h = Harness::load(
                &root,
                Path::new(entry),
                &root.join("tests/fixtures/palette-qml.json"),
                fixture,
                Config {
                    width: 1920,
                    height: 1200,
                    scale,
                },
            )?;
            h.render()?;
            let actual = body(h.scene());
            // Boundary probes reuse the status body, not a launcher-sized mask.
            let size = if name.starts_with("corner-") || name == "left-clamp" {
                (310., 222.)
            } else {
                (actual.w, actual.h)
            };
            assert_eq!(
                size,
                (
                    case["width"].as_f64().ok_or("width")? as f32,
                    case["height"].as_f64().ok_or("height")? as f32
                ),
                "Actual Luau root size changed: {name}"
            );
            let attachment = if edge == "popout" {
                Attachment::Popout
            } else {
                Attachment::Attached(match edge {
                    "right" => Edge::Right,
                    "left" => Edge::Left,
                    "bottom" => Edge::Bottom,
                    _ => Edge::Top,
                })
            };
            let mut scene = crate::sound::fallback(palette.get("surface")?);
            scene.primitives = vec![bead::render::Primitive::Rounded {
                radial: None,
                rect: Rect::new(0., 0., size.0, size.1),
                radius: 16.,
                fill: palette.get("surface")?,
                border: None,
            }];
            let mut chrome = Chrome::new(attachment, &palette)?;
            chrome.center = case["center"].as_f64().map(|n| n as f32);
            chrome.decorate(&mut scene)?;
            let position = Frame::default().positioner(attachment);
            let (x, y) = position(&scene, chrome.center, (1920, 1200));
            assert_eq!(
                x as f32 + scene.offset.0,
                case["x"].as_f64().ok_or("x")? as f32
            );
            assert_eq!(
                y as f32 + scene.offset.1,
                case["y"].as_f64().ok_or("y")? as f32
            );
            let width = (1920. * scale) as u32;
            let height = (1200. * scale) as u32;
            let mut pixels = Pixels {
                width,
                height,
                pixels: [0x34, 0x37, 0x3f, 255].repeat(width as usize * height as usize),
            };
            let fill = palette.get("surface")?;
            for pixel in pixels.pixels[..(width as f32 * 50. * scale) as usize * 4]
                .as_chunks_mut::<4>()
                .0
            {
                *pixel = fill;
            }
            let factory = Chrome::new(Attachment::Attached(Edge::Top), &palette)?.decorations();
            for decoration in factory(
                (1920, 1200),
                chrome.engaged,
                Some(bead::wayland::DecorationPanel {
                    scene: &scene,
                    origin: (x, y),
                }),
            )
            .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?
            {
                tile(&mut pixels, decoration.rect, &decoration.svg, scale, None)?;
            }
            let place =
                |r: Rect| r.translated(x as f32 + scene.offset.0, y as f32 + scene.offset.1);
            for (index, primitive) in scene.primitives.iter().enumerate() {
                if let bead::render::Primitive::Image { rect, path, .. } = primitive {
                    tile(
                        &mut pixels,
                        place(*rect),
                        scene.asset_data.get(path).ok_or("Missing contour")?,
                        scale,
                        scene.clips.get(&index).copied().map(place),
                    )?;
                }
            }
            let filename = format!("bead-{name}-{scale}.png");
            if update {
                pixels.save(&golden.join(&filename))?;
            }
            pixels.compare(
                &golden.join(&filename),
                Tolerance::default(),
                &artifacts.join(format!("self-{name}")),
            )?;
            pixels.compare(
                &golden.join(format!("qml-{name}-{scale}.png")),
                Tolerance {
                    channel: 4,
                    changed_fraction: 0.001,
                },
                &artifacts.join(format!("qml-{name}")),
            )?;
            let qml = Pixels::load(&golden.join(format!("qml-{name}-{scale}.png")))?;
            assert_interior(&qml, &pixels, fill, name)?;
            if update {
                let mut pair = Pixels {
                    width: width * 2,
                    height,
                    pixels: vec![0; width as usize * 2 * height as usize * 4],
                };
                for row in 0..height as usize {
                    let start = row * width as usize * 4;
                    let out = row * width as usize * 8;
                    pair.pixels[out..out + width as usize * 4]
                        .copy_from_slice(&qml.pixels[start..start + width as usize * 4]);
                    pair.pixels[out + width as usize * 4..out + width as usize * 8]
                        .copy_from_slice(&pixels.pixels[start..start + width as usize * 4]);
                }
                pair.save(&artifacts.join(format!("pair-{name}-{scale}.png")))?;
            }
        }
    }
    Ok(())
}

// B0047: the configurable frame. One small output so every variant is a cheap golden; the
// default variant's geometry is already pinned against Qt by the cases above.
const OUTPUT: (u32, u32) = (800, 500);
const SCALE: f32 = 1.5;
fn frame_with(change: impl FnOnce(&mut crate::placement::FrameSettings)) -> Frame {
    let mut settings = crate::placement::FrameSettings {
        width: 10.,
        rounding: 25.,
        left: true,
        right: true,
        bottom: true,
        top: true,
        bar: true,
        border: true,
    };
    change(&mut settings);
    Frame::from_settings(&settings)
}
/// The frame as the shell composes it: the bar's fill in the header, the static tiles,
/// an optional joined panel and the lips' artwork.
fn compose(
    palette: &Palette,
    frame: Frame,
    open_panel: bool,
    lips: &[(bead::wayland::intent::Lip, bead::wayland::LipState)],
) -> Result<Pixels> {
    let (w, h) = (OUTPUT.0 as f32, OUTPUT.1 as f32);
    let width = (w * SCALE) as u32;
    let height = (h * SCALE) as u32;
    let mut pixels = Pixels {
        width,
        height,
        pixels: [0x34, 0x37, 0x3f, 255].repeat(width as usize * height as usize),
    };
    let fill = palette.get("surface")?;
    for pixel in pixels.pixels[..(width as f32 * frame.top * SCALE) as usize * 4]
        .as_chunks_mut::<4>()
        .0
    {
        *pixel = fill;
    }
    let mut chrome = Chrome::new(Attachment::Popout, palette)?;
    chrome.output = OUTPUT;
    chrome.center = Some(w / 2.);
    let mut scene = crate::sound::fallback(fill);
    scene.primitives = vec![bead::render::Primitive::Rounded {
        radial: None,
        rect: Rect::new(0., 0., 310., 222.),
        radius: 16.,
        fill,
        border: None,
    }];
    let mut panel = None;
    if open_panel {
        chrome.set_engaged(true);
        chrome.decorate_with(frame, &mut scene)?;
        let body = body(&scene);
        let at = frame.panel(Attachment::Popout, (w, h), (body.w, body.h), chrome.center);
        let origin = (
            (at.x - body.x + scene.bounds.x).round() as i32,
            (at.y - body.y + scene.bounds.y).round() as i32,
        );
        panel = Some(origin);
    }
    let tiles = chrome
        .tiles(
            frame,
            OUTPUT,
            open_panel,
            panel.map(|origin| bead::wayland::DecorationPanel {
                scene: &scene,
                origin,
            }),
        )
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    for decoration in tiles {
        tile(&mut pixels, decoration.rect, &decoration.svg, SCALE, None)?;
    }
    if let Some((x, y)) = panel {
        let place = |r: Rect| r.translated(x as f32 + scene.offset.0, y as f32 + scene.offset.1);
        for (index, primitive) in scene.primitives.iter().enumerate() {
            if let bead::render::Primitive::Image { rect, path, .. } = primitive {
                tile(
                    &mut pixels,
                    place(*rect),
                    scene.asset_data.get(path).ok_or("Missing contour")?,
                    SCALE,
                    scene.clips.get(&index).copied().map(place),
                )?;
            }
        }
    }
    let (accent, secondary, surface) = (
        palette.get("primary")?,
        palette.get("secondary")?,
        palette.get("surface")?,
    );
    for (lip, state) in lips {
        let rect = lip.rect((w, h));
        if let Some(svg) = crate::frame_art::handle(
            lip,
            (rect.w, rect.h),
            *state,
            accent,
            secondary,
            surface,
            (false, lip.edge == Edge::Top, false),
        ) {
            tile(&mut pixels, rect, svg.as_bytes(), SCALE, None)?;
        }
    }
    Ok(pixels)
}
fn lip_at(edge: Edge, thickness: f32) -> bead::wayland::intent::Lip {
    bead::wayland::intent::Lip {
        id: "l".into(),
        edge,
        length: 144.,
        thickness,
        opens: "p".into(),
    }
}
#[test]
fn configurable_frame_variants_match_their_goldens() -> Result<()> {
    use bead::wayland::LipState;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = Palette::read(&root.join("palette.json"))?;
    let update = std::env::var("BEAD_UPDATE_GOLDENS").as_deref() == Ok("1");
    let golden = root.join("tests/goldens/frame");
    let artifacts = std::env::temp_dir().join("bead-frame-pixel-diffs");
    let idle = LipState {
        progress: 0.,
        active: false,
        ..Default::default()
    };
    let left = lip_at(Edge::Left, 10.);
    let right = lip_at(Edge::Right, 10.);
    // The top handle spans the bar and the frame strip (208 x 50), as the shell configures it.
    let top = bead::wayland::intent::Lip {
        length: 208.,
        ..lip_at(Edge::Top, 50.)
    };
    let cases: Vec<(&str, Frame, bool, Vec<(_, LipState)>)> = vec![
        ("default", frame_with(|_| {}), false, vec![]),
        // The frame off: no bar, no top edge, no sides.
        (
            "frame-off",
            frame_with(|s| {
                s.bar = false;
                s.top = false;
                s.left = false;
                s.right = false;
                s.bottom = false;
            }),
            false,
            vec![],
        ),
        (
            "bar-only",
            frame_with(|s| {
                s.left = false;
                s.right = false;
                s.bottom = false;
            }),
            false,
            vec![],
        ),
        ("rounding-0", frame_with(|s| s.rounding = 0.), false, vec![]),
        (
            "rounding-40",
            frame_with(|s| s.rounding = 40.),
            false,
            vec![],
        ),
        ("no-left", frame_with(|s| s.left = false), false, vec![]),
        ("no-right", frame_with(|s| s.right = false), false, vec![]),
        ("no-bottom", frame_with(|s| s.bottom = false), false, vec![]),
        (
            "top-edge-without-bar",
            frame_with(|s| s.bar = false),
            false,
            vec![],
        ),
        ("wide-14", frame_with(|s| s.width = 14.), false, vec![]),
        // Joined contours with an open panel.
        ("panel-default", frame_with(|_| {}), true, vec![]),
        (
            "panel-wide-round",
            frame_with(|s| {
                s.width = 14.;
                s.rounding = 40.;
            }),
            true,
            vec![],
        ),
        (
            "panel-no-sides",
            frame_with(|s| {
                s.left = false;
                s.right = false;
            }),
            true,
            vec![],
        ),
        // Lip cues: idle chevrons, a half-filled dwell, and the open (flipped) state.
        (
            "lips-idle",
            frame_with(|_| {}),
            false,
            vec![(left.clone(), idle), (right.clone(), idle)],
        ),
        (
            "lips-dwell",
            frame_with(|_| {}),
            false,
            vec![
                (
                    left.clone(),
                    LipState {
                        progress: 0.5,
                        active: false,
                        ..Default::default()
                    },
                ),
                (
                    right.clone(),
                    LipState {
                        progress: 0.8,
                        active: false,
                        ..Default::default()
                    },
                ),
            ],
        ),
        (
            "lips-open",
            frame_with(|_| {}),
            true,
            vec![
                (
                    left.clone(),
                    LipState {
                        progress: 0.,
                        active: true,
                        ..Default::default()
                    },
                ),
                (
                    right.clone(),
                    LipState {
                        progress: 0.,
                        active: true,
                        ..Default::default()
                    },
                ),
            ],
        ),
        // B0279: the right lip docked on its panel as a lit tab (drawn on the screen edge here).
        (
            "lips-docked",
            frame_with(|_| {}),
            false,
            vec![
                (left.clone(), idle),
                (
                    right.clone(),
                    LipState {
                        active: true,
                        attach: 1.,
                        hovered: true,
                        ..Default::default()
                    },
                ),
            ],
        ),
        // The opt-in top handle: the pill under the bar title at rest, then a tab docked on the
        // dashboard's bottom edge (drawn at the screen edge here, like the side tabs above).
        (
            "lips-top",
            frame_with(|_| {}),
            false,
            vec![(top.clone(), idle)],
        ),
        (
            "lips-top-docked",
            frame_with(|_| {}),
            false,
            vec![(
                top.clone(),
                LipState {
                    active: true,
                    attach: 1.,
                    hovered: true,
                    ..Default::default()
                },
            )],
        ),
    ];
    for (name, frame, panel, lips) in cases {
        let pixels = compose(&palette, frame, panel, &lips)?;
        let filename = format!("frame-{name}.png");
        if update {
            std::fs::create_dir_all(&golden)?;
            pixels.save(&golden.join(&filename))?;
        }
        pixels.compare(
            &golden.join(&filename),
            Tolerance::default(),
            &artifacts.join(name),
        )?;
    }
    Ok(())
}

/// B0430 (owner: rim seam at the control centre's join): the frame strip and the joined art draw the
/// same translucent rim on two surfaces; two coats left a brighter row from the art's left edge. The
/// art is clipped out of the frame strips, so inside every frame tile outside the panel the tiles
/// composed with the art equal the tiles alone, at both scales. Sub-fillet (Bead report): a corner tile
/// inside the panel's final rect is drawn from the frame alone (from the final join it showed a
/// straight block while the revealed body was narrower than the fillet); at rest the art covers it.
#[test]
fn the_joined_art_leaves_the_frame_strip_rows_to_the_strip() -> Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = Palette::read(&root.join("palette.json"))?;
    let frame = frame_with(|_| {});
    let (w, h) = (OUTPUT.0 as f32, OUTPUT.1 as f32);
    let fill = palette.get("surface")?;
    let mut chrome = Chrome::new(Attachment::ControlCenter, &palette)?;
    chrome.output = OUTPUT;
    let mut scene = crate::sound::fallback(fill);
    scene.primitives = vec![bead::render::Primitive::Rounded {
        radial: None,
        rect: Rect::new(0., 0., 310., 222.),
        radius: 16.,
        fill,
        border: None,
    }];
    chrome.decorate_with(frame, &mut scene)?;
    let body = body(&scene);
    let at = frame.panel(Attachment::ControlCenter, (w, h), (body.w, body.h), None);
    let origin = (
        (at.x - body.x + scene.bounds.x).round() as i32,
        (at.y - body.y + scene.bounds.y).round() as i32,
    );
    let tiles = chrome
        .tiles(
            frame,
            OUTPUT,
            true,
            Some(bead::wayland::DecorationPanel {
                scene: &scene,
                origin,
            }),
        )
        .map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
    let place = |r: Rect| {
        r.translated(
            origin.0 as f32 + scene.offset.0,
            origin.1 as f32 + scene.offset.1,
        )
    };
    let (index, art, svg) = scene
        .primitives
        .iter()
        .enumerate()
        .find_map(|(index, p)| match p {
            bead::render::Primitive::Image { rect, path, .. }
                if path.to_string_lossy().starts_with("@joined-frame-") =>
            {
                Some((index, place(*rect), scene.asset_data[path].clone()))
            }
            _ => None,
        })
        .ok_or("joined art")?;
    let clip = scene.clips.get(&index).copied().map(place);
    let joined = body.translated(
        origin.0 as f32 - scene.bounds.x,
        origin.1 as f32 - scene.bounds.y,
    );
    let inside = |r: Rect| {
        joined.x <= r.x
            && joined.y <= r.y
            && joined.x + joined.w >= r.x + r.w
            && joined.y + joined.h >= r.y + r.h
    };
    let corners: Vec<_> = tiles.iter().filter(|d| inside(d.rect)).collect();
    assert!(
        !corners.is_empty(),
        "the docked control centre covers a corner tile"
    );
    for decoration in &corners {
        let frame_only = chrome.svg(
            frame,
            (w, h),
            &frame.material((w, h)),
            decoration.rect,
            true,
        )?;
        assert!(
            *decoration.svg == *frame_only.as_bytes(),
            "the corner tile {:?} is drawn from the final join",
            decoration.rect
        );
    }
    for scale in [1., 1.5] {
        let blank = || Pixels {
            width: (w * scale) as u32,
            height: (h * scale) as u32,
            pixels: vec![0; (w * scale) as usize * (h * scale) as usize * 4],
        };
        let (mut strip, mut composed, mut alone) = (blank(), blank(), blank());
        for decoration in &tiles {
            tile(&mut strip, decoration.rect, &decoration.svg, scale, None)?;
            tile(&mut composed, decoration.rect, &decoration.svg, scale, None)?;
        }
        tile(&mut composed, art, &svg, scale, clip)?;
        tile(&mut alone, art, &svg, scale, clip)?;
        let at = |p: &Pixels, x: u32, y: u32| {
            let i = ((y * p.width + x) * 4) as usize;
            [p.pixels[i], p.pixels[i + 1], p.pixels[i + 2]]
        };
        for decoration in &tiles {
            let r = decoration.rect;
            // At rest the art covers a corner tile inside the panel: there it is what shows.
            let reference = if inside(r) { &alone } else { &strip };
            let (x0, y0) = ((r.x * scale) as u32, (r.y * scale) as u32);
            let x1 = (((r.x + r.w) * scale) as u32).min(strip.width);
            let y1 = (((r.y + r.h) * scale) as u32).min(strip.height);
            for y in y0..y1 {
                for x in x0..x1 {
                    let (s, c) = (at(reference, x, y), at(&composed, x, y));
                    assert!(
                        s.iter().zip(c).all(|(s, c)| s.abs_diff(c) <= 2),
                        "scale {scale}: ({x},{y}) in tile {r:?}: composed {c:?} against the strip's {s:?}"
                    );
                }
            }
        }
    }
    Ok(())
}
