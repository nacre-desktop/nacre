// SPDX-License-Identifier: GPL-3.0-only
//! B0047: artwork for the frame's edge lips, ported from `NacreEdgeTab.qml`. The framework
//! blits the SVG; this is Nacre's look (accent cue that fills while the pointer dwells, a
//! chevron that points into or out of the panel).
use bead::wayland::{EdgeArt, LipState, intent::Lip};
use bead::{core::Edge, render::Palette};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering::SeqCst};

/// D6 (spec 0237): `desktop.edgeHandleStyle` `"arrows"` draws the chevron tab; the default is the
/// pearl grabber. Set by the shell gate when the preference changes.
pub static ARROWS: AtomicBool = AtomicBool::new(false);

/// The handle for a side lip in the configured style; other edges draw nothing.
pub fn handle(
    lip: &Lip,
    size: (f32, f32),
    state: LipState,
    accent: [u8; 4],
    secondary: [u8; 4],
    arrows: bool,
) -> Option<String> {
    if arrows {
        edge_tab(lip, size, state, accent, secondary)
    } else {
        grabber(lip, size, state, accent, secondary)
    }
}
/// The pearl grabber (design review D6): a rounded bar in primary centred on the lip, 6 x 56 at
/// rest, 8 x 72 while the dwell charges; the charge fills it top-down in the secondary role.
pub fn grabber(
    lip: &Lip,
    (width, height): (f32, f32),
    state: LipState,
    accent: [u8; 4],
    secondary: [u8; 4],
) -> Option<String> {
    if !matches!(lip.edge, Edge::Left | Edge::Right) {
        return None;
    }
    let charging = !state.active && state.progress > 0. && state.progress < 1.;
    let (thick, long): (f32, f32) = if charging { (8., 72.) } else { (6., 56.) };
    let thick = thick.min((width - 2.).max(2.));
    let (x, y) = ((width - thick) / 2., (height - long) / 2.);
    let r = thick / 2.;
    let mut body = format!(
        r#"<rect x="{x}" y="{y}" width="{thick}" height="{long}" rx="{r}" fill="{}" opacity="{}"/>"#,
        rgb(accent),
        if charging { 1. } else { 0.9 }
    );
    if charging {
        body.push_str(&format!(
            r#"<rect x="{x}" y="{y}" width="{thick}" height="{}" rx="{r}" fill="{}" clip-path="url(#bar)"/>"#,
            long * state.progress.clamp(0., 1.),
            rgb(secondary)
        ));
    }
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><defs><clipPath id="bar"><rect x="{x}" y="{y}" width="{thick}" height="{long}" rx="{r}"/></clipPath></defs>{body}</svg>"#
    ))
}

fn rgb(c: [u8; 4]) -> String {
    format!("rgb({},{},{})", c[0], c[1], c[2])
}
/// The tab drawn inside a left or right lip of `width` x `height` (frame width x lip
/// length). Other edges draw nothing, as in the reference (the top lip is invisible).
pub fn edge_tab(
    lip: &Lip,
    (width, height): (f32, f32),
    state: LipState,
    accent: [u8; 4],
    secondary: [u8; 4],
) -> Option<String> {
    let left = match lip.edge {
        Edge::Left => true,
        Edge::Right => false,
        _ => return None,
    };
    let amount = if state.active {
        1.
    } else {
        state.progress.clamp(0., 1.)
    };
    let mut body = String::new();
    // The cue: a 1 px line on the outer edge, 36 px tall, filling top-down as the dwell
    // advances; gone once full or while the panel is open.
    if !state.active && amount < 1. && amount > 0. {
        body.push_str(&format!(
            r#"<rect x="{}" y="{}" width="1" height="{}" fill="url(#cue)"/>"#,
            if left { 0. } else { width - 1. },
            (height - 36.) / 2.,
            36. * amount
        ));
    }
    let arrow_w = (width - 2.).min(8.);
    if arrow_w >= 4. {
        // Left: points right while closed, left while open. Right: the opposite.
        let right = if left { !state.active } else { state.active };
        let (x, y) = ((width - arrow_w) / 2., (height - 18.) / 2.);
        let (a, b) = if right {
            (1., arrow_w - 1.)
        } else {
            (arrow_w - 1., 1.)
        };
        body.push_str(&format!(
            r#"<path d="M{} {} L{} {} L{} {}" fill="none" stroke="{}" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round" opacity="{}"/>"#,
            x + a,
            y + 1.,
            x + b,
            y + 9.,
            x + a,
            y + 17.,
            rgb(accent),
            0.75 + 0.25 * amount
        ));
    }
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}"><defs><linearGradient id="cue" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{}"/><stop offset="1" stop-color="{}"/></linearGradient></defs>{body}</svg>"#,
        rgb(accent),
        rgb(secondary)
    ))
}
pub fn art(colors: &Palette) -> Result<EdgeArt, Box<dyn std::error::Error>> {
    let (accent, secondary) = (colors.get("primary")?, colors.get("secondary")?);
    let surface = rgb(colors.get("surface")?);
    Ok(Arc::new(move |lip, size, state| {
        handle(lip, size, state, accent, secondary, ARROWS.load(SeqCst)).map(|svg| {
            // Popup films leave this stable surface uncovered, including its material.
            Arc::from(format!(
                r#"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}"><rect width="100%" height="100%" fill="{surface}"/>{svg}</svg>"#,
                size.0, size.1
            ).into_bytes())
        })
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    fn lip(edge: Edge) -> Lip {
        Lip {
            id: "l".into(),
            edge,
            length: 144.,
            thickness: 10.,
            opens: "p".into(),
        }
    }
    const A: [u8; 4] = [10, 20, 30, 255];
    const S: [u8; 4] = [40, 50, 60, 255];
    fn idle() -> LipState {
        LipState {
            progress: 0.,
            active: false,
        }
    }
    #[test]
    fn lip_material_remains_opaque_when_the_popup_film_is_cut_out() {
        use bead::render::{PixelFormat, ShmTarget, svg_into};
        let surface = [21, 18, 23, 255];
        let colors = Palette(
            std::collections::HashMap::from([
                ("surface".into(), surface),
                ("primary".into(), A),
                ("secondary".into(), S),
            ])
            .into(),
        );
        let svg = art(&colors).unwrap()(&lip(Edge::Left), (10., 144.), idle()).unwrap();
        let mut bytes = vec![0; 10 * 144 * 4];
        svg_into(
            &svg,
            ShmTarget {
                bytes: &mut bytes,
                width: 10,
                height: 144,
                scale: 1.,
                format: PixelFormat::Rgba,
            },
        )
        .unwrap();
        assert_eq!(&bytes[..4], &surface);
    }
    #[test]
    fn the_grabber_is_a_centred_bar_that_grows_and_charges_with_the_dwell() {
        assert!(grabber(&lip(Edge::Top), (208., 50.), idle(), A, S).is_none());
        let rest = grabber(&lip(Edge::Left), (10., 144.), idle(), A, S).unwrap();
        assert!(
            rest.contains(r#"<rect x="2" y="44" width="6" height="56" rx="3""#),
            "{rest}"
        );
        assert!(
            !rest.contains("clip-path=\"url(#bar)\""),
            "no charge at rest"
        );
        let half = LipState {
            progress: 0.5,
            active: false,
        };
        let charging = grabber(&lip(Edge::Right), (10., 144.), half, A, S).unwrap();
        assert!(
            charging.contains(r#"<rect x="1" y="36" width="8" height="72" rx="4""#),
            "{charging}"
        );
        assert!(
            charging.contains(r#"height="36" rx="4" fill="rgb(40,50,60)" clip-path="url(#bar)""#),
            "{charging}"
        );
        assert!(
            handle(&lip(Edge::Left), (10., 144.), idle(), A, S, true)
                .unwrap()
                .contains("<path")
        );
        assert!(
            !handle(&lip(Edge::Left), (10., 144.), idle(), A, S, false)
                .unwrap()
                .contains("<path")
        );
    }
    #[test]
    fn only_side_lips_have_a_tab() {
        assert!(edge_tab(&lip(Edge::Top), (208., 50.), idle(), A, S).is_none());
        assert!(edge_tab(&lip(Edge::Bottom), (208., 10.), idle(), A, S).is_none());
        assert!(edge_tab(&lip(Edge::Left), (10., 144.), idle(), A, S).is_some());
        assert!(edge_tab(&lip(Edge::Right), (10., 144.), idle(), A, S).is_some());
    }
    #[test]
    fn the_chevron_points_into_the_panel_side_and_flips_when_open() {
        // Left, closed: points right (x grows to 8). Open: points left.
        let closed = edge_tab(&lip(Edge::Left), (10., 144.), idle(), A, S).unwrap();
        assert!(closed.contains(r#"d="M2 64 L8 72 L2 80""#), "{closed}");
        let open = LipState {
            progress: 0.,
            active: true,
        };
        let opened = edge_tab(&lip(Edge::Left), (10., 144.), open, A, S).unwrap();
        assert!(opened.contains(r#"d="M8 64 L2 72 L8 80""#), "{opened}");
        // Right is the mirror image.
        let right = edge_tab(&lip(Edge::Right), (10., 144.), idle(), A, S).unwrap();
        assert!(right.contains(r#"d="M8 64 L2 72 L8 80""#), "{right}");
    }
    #[test]
    fn the_cue_fills_with_the_dwell_and_is_gone_when_full_or_open() {
        let at = |progress, active| {
            edge_tab(
                &lip(Edge::Left),
                (10., 144.),
                LipState { progress, active },
                A,
                S,
            )
            .unwrap()
        };
        assert!(!at(0., false).contains("<rect"), "no cue before the dwell");
        let half = at(0.5, false);
        assert!(
            half.contains(r#"x="0" y="54" width="1" height="18""#),
            "{half}"
        );
        assert!(at(0.5, false).contains(r#"opacity="0.875""#));
        assert!(!at(1., false).contains("<rect"), "full: the cue fades out");
        assert!(!at(0.5, true).contains("<rect"), "open: no cue");
        let right = edge_tab(
            &lip(Edge::Right),
            (10., 144.),
            LipState {
                progress: 0.5,
                active: false,
            },
            A,
            S,
        )
        .unwrap();
        assert!(right.contains(r#"x="9" y="54""#), "{right}");
    }
    #[test]
    fn edge_arrow_remains_visible_and_native_progress_rasterizes_at_fractional_scales() {
        use bead::render::{PixelFormat, ShmTarget, svg_into};
        for edge in [Edge::Left, Edge::Right] {
            let lip = lip(edge);
            for scale in [1.0, 1.5] {
                let (width, height) = ((10.0 * scale) as u32, (144.0 * scale) as u32);
                let raster = |state| {
                    let svg = edge_tab(&lip, (10., 144.), state, A, S).unwrap();
                    let mut bytes = vec![0; (width * height * 4) as usize];
                    svg_into(
                        svg.as_bytes(),
                        ShmTarget {
                            bytes: &mut bytes,
                            width,
                            height,
                            scale,
                            format: PixelFormat::Rgba,
                        },
                    )
                    .unwrap();
                    bytes
                };
                let idle_pixels = raster(idle());
                let cue_pixels = raster(LipState {
                    progress: 0.5,
                    active: false,
                });
                let alpha_at =
                    |pixels: &[u8], x: u32, y: u32| pixels[((y * width + x) * 4 + 3) as usize];
                let arrow_pixels = (64..=80).any(|y| {
                    (2..=8).any(|x| {
                        alpha_at(
                            &idle_pixels,
                            (x as f32 * scale) as u32,
                            (y as f32 * scale) as u32,
                        ) > 0
                    })
                });
                assert!(arrow_pixels, "idle arrow must rasterize at scale {scale}");
                let hovered_arrow_pixels = (64..=80).any(|y| {
                    (2..=8).any(|x| {
                        alpha_at(
                            &cue_pixels,
                            (x as f32 * scale) as u32,
                            (y as f32 * scale) as u32,
                        ) > 0
                    })
                });
                assert!(
                    hovered_arrow_pixels,
                    "arrow must remain visible during native hover at scale {scale}"
                );
                let cue_x = if edge == Edge::Left { 0 } else { width - 1 };
                let cue_pixels_visible =
                    (54..72).any(|y| alpha_at(&cue_pixels, cue_x, (y as f32 * scale) as u32) > 0);
                assert!(
                    cue_pixels_visible,
                    "native hover progress must rasterize at scale {scale}"
                );
                assert!(
                    idle_pixels
                        .iter()
                        .zip(&cue_pixels)
                        .any(|(before, after)| before != after),
                    "cue pixels should change from idle at scale {scale}"
                );
            }
        }
    }
    #[test]
    fn a_thin_frame_drops_the_chevron_like_the_reference() {
        // width < 4 after the 2 px margin: QML hides the arrow.
        let thin = edge_tab(&lip(Edge::Left), (5., 144.), idle(), A, S).unwrap();
        assert!(!thin.contains("<path"));
        assert!(
            edge_tab(&lip(Edge::Left), (6., 144.), idle(), A, S)
                .unwrap()
                .contains("<path")
        );
    }
}
