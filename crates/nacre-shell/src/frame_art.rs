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

/// `desktop.edgeHandleTopVisible` (opt-in): the top lip draws its handle under the bar title.
pub static TOP_TAB: AtomicBool = AtomicBool::new(false);
/// The docked top tab's input depth inside its 50 px lip (Bead's `docked_input_depth`): the frame
/// strip's depth, like a side tab's thickness, so the tab never covers windows under the dashboard.
pub const TOP_TAB_DEPTH: f32 = 10.;
/// Owner experiment 2026-11-16: a completed dwell keeps the swollen pill until the panel takes it as
/// its tab, so the pill grows once and flings open at that size (it used to drop back to rest size
/// for the frames between the dwell's end and the open). `false` restores the old look.
const HOLD_COMPLETED_DWELL: bool = true;

/// The handle for a side lip (and the top lip with `top_tab`) in the configured style.
pub fn handle(
    lip: &Lip,
    size: (f32, f32),
    state: LipState,
    accent: [u8; 4],
    secondary: [u8; 4],
    surface: [u8; 4],
    (arrows, top_tab): (bool, bool),
) -> Option<String> {
    if arrows {
        edge_tab(lip, size, state, accent, secondary)
    } else {
        grabber(lip, size, state, accent, surface, top_tab)
    }
}
/// The pearl grabber (design review D6): a rounded bar in primary centred on the lip, 6 x 56 at
/// rest. While the dwell charges it swells from its centre to 8 x 72 and a soft glow the width of
/// the lip grows around it, in the same accent so the handle only ever brightens (owner 2026-11-14:
/// no top-down fill, no dark halo). It stays swollen while its panel is open, so the hand-over from
/// the dwell to the open panel changes nothing but the glow. With reduced motion the charging look
/// is static.
///
/// B0279: the lip's own `surface` plate is part of the art. While a side lip's panel opens
/// (`attach` 0 free, 1 docked) the plate fades out and the pill rides on the panel's inner edge as
/// its tab, where the glow also answers `hovered`. The left (chat) lip's tab is drawn by the chat
/// app on its own panel (B0304 `tab_lips`).
///
/// With `top_tab` the top lip draws the same picture rotated (join side up, `TOP_TAB`): at rest only
/// the pill and its glow, in the frame strip under the bar title (the lip spans the bar, so no plate
/// may cover it); with `attach` the pill slides to the join, flush under the dashboard like a side
/// tab (owner 2026-11-16: a plate band there read as its own square).
pub fn grabber(
    lip: &Lip,
    (width, height): (f32, f32),
    state: LipState,
    accent: [u8; 4],
    surface: [u8; 4],
    top_tab: bool,
) -> Option<String> {
    let top = lip.edge == Edge::Top;
    if !(top && top_tab || matches!(lip.edge, Edge::Left | Edge::Right)) {
        return None;
    }
    // Side art as drawn (join side x = w); the top art is that picture rotated, w = thickness.
    let (w, h) = if top {
        (height, width)
    } else {
        (width, height)
    };
    let attach = state.attach.clamp(0., 1.);
    let charging = !state.active && state.progress > 0. && state.progress < 1.;
    // The tab keeps its swollen pill for the whole ride, even when `active` lags the close.
    let charge = if state.active || attach > 0. {
        1.
    } else if HOLD_COMPLETED_DWELL && state.progress >= 1. {
        1.
    } else if !charging {
        0.
    } else if bead::core::animation::reduced_motion() {
        1.
    } else {
        state.progress
    };
    let (thick, long) = (6. + 2. * charge, 56. + 16. * charge);
    let thick = thick.min((w - 2.).max(2.));
    // At rest the pill sits mid-strip: the lip's thickness past the bar (all of it without a bar).
    // Docked it slides onto the join so it meets the panel with no gap (owner 2026-11-16): x = w
    // for the right and top lips; x = 0 for the left lip, whose panel opens from the left.
    let bar = crate::placement::BAR_HEIGHT;
    let strip = if w > bar { (w - bar) / 2. } else { w / 2. };
    let rest = if top { strip } else { w / 2. };
    let docked = if lip.edge == Edge::Left {
        thick / 2.
    } else {
        w - thick / 2.
    };
    let cx = rest + (docked - rest) * attach;
    let (x, y) = (cx - thick / 2., (h - long) / 2.);
    // Owner 2026-11-16: beside the panel's sheen rim a docked side plate read as a black block, so it
    // fades out as the panel opens and the docked side tab is the pill alone.
    let mut body = if !top {
        if attach <= 0. {
            plate(w, h, 0., surface)
        } else if attach < 1. {
            format!(
                r#"<g opacity="{}">{}</g>"#,
                1. - attach,
                plate(w, h, 0., surface)
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };
    // Hover lights the docked tab only: on the free bar the dwell fill stays the visible cue.
    let glow = if charging {
        charge
    } else if state.hovered && attach > 0. {
        1.
    } else {
        0.
    };
    // While its panel is open the lip at its rest box keeps only the plate, filling the frame's
    // hole there: the pill rides the docked tab (owner 2026-11-16).
    let pill = !(state.active && attach <= 0.);
    if pill && glow > 0. {
        let gw = if top { 10. } else { w };
        body.push_str(&format!(
            r#"<rect x="{}" y="{}" width="{gw}" height="{}" rx="{}" fill="{}" opacity="{}"/>"#,
            cx - gw / 2.,
            y - 4.,
            long + 8.,
            gw / 2.,
            rgb(accent),
            0.3 * glow
        ));
    }
    if pill {
        body.push_str(&format!(
            r#"<rect x="{x}" y="{y}" width="{thick}" height="{long}" rx="{}" fill="{}" opacity="{}"/>"#,
            thick / 2.,
            rgb(accent),
            0.9 + 0.1 * charge
        ));
    }
    if top {
        body = format!(r#"<g transform="translate(0,{w}) rotate(-90)">{body}</g>"#);
    }
    Some(format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}">{body}</svg>"#
    ))
}

/// The opaque lip material (popup films leave it uncovered): the whole lip at `attach` 0, shrinking
/// to the 88 px tab (join side at x = `width`, free side rounded) with a concave fillet above and
/// below where it meets the panel.
fn plate(width: f32, height: f32, attach: f32, surface: [u8; 4]) -> String {
    let fill = rgb(surface);
    if attach <= 0. {
        return format!(r#"<rect width="{width}" height="{height}" fill="{fill}"/>"#);
    }
    let (top, tall) = (
        attach * (height - 88.) / 2.,
        height + attach * (88. - height),
    );
    let r = (width / 2.).min(6.) * attach;
    let (bottom, inner) = (top + tall, width - r);
    format!(
        r#"<path d="M{width} {top}H{r}A{r} {r} 0 0 0 0 {}V{}A{r} {r} 0 0 0 {r} {bottom}H{width}Z" fill="{fill}"/><path d="M{inner} {top}H{width}V{}A{r} {r} 0 0 1 {inner} {top}ZM{inner} {bottom}H{width}V{}A{r} {r} 0 0 0 {inner} {bottom}Z" fill="{fill}"/>"#,
        top + r,
        bottom - r,
        top - r,
        bottom + r,
    )
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
    let startup = [
        colors.get("surface")?,
        colors.get("primary")?,
        colors.get("secondary")?,
    ];
    Ok(Arc::new(move |lip, size, state| {
        // Owner 2026-11-14: the handle follows the wallpaper palette (the live primary), not the
        // palette the shell started with.
        let [surface, accent, secondary] = crate::placement::live_colors().unwrap_or(startup);
        let arrows = ARROWS.load(SeqCst);
        handle(lip, size, state, accent, secondary, surface, (arrows, TOP_TAB.load(SeqCst))).map(|svg| {
            if !arrows {
                return Arc::from(svg.into_bytes());
            }
            // Popup films leave this stable surface uncovered, including its material.
            let surface = rgb(surface);
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
            ..Default::default()
        }
    }
    fn docked(attach: f32, hovered: bool) -> LipState {
        LipState {
            active: true,
            attach,
            hovered,
            ..Default::default()
        }
    }
    #[test]
    fn side_lips_dock_as_their_pill_on_their_panel() {
        let at = |attach, hovered| {
            grabber(
                &lip(Edge::Right),
                (10., 144.),
                docked(attach, hovered),
                A,
                S,
                false,
            )
            .unwrap()
        };
        // Free: the plate is the whole lip, exactly as before.
        let free = grabber(&lip(Edge::Right), (10., 144.), idle(), A, S, false).unwrap();
        assert!(free.contains(r#"<rect width="10" height="144" fill="rgb(40,50,60)"/>"#));
        // Half way: the whole plate at half opacity.
        let half = at(0.5, false);
        assert!(
            half.contains(
                r#"<g opacity="0.5"><rect width="10" height="144" fill="rgb(40,50,60)"/></g>"#
            ),
            "{half}"
        );
        // Docked: the swollen pill alone, no plate; hover adds the glow around it.
        let tab = at(1., false);
        assert!(
            !tab.contains(r#"<rect width="10" height="144""#) && !tab.contains("<path"),
            "{tab}"
        );
        assert!(
            tab.contains(r#"width="8" height="72" rx="4" fill="rgb(10,20,30)" opacity="1""#),
            "{tab}"
        );
        assert!(
            tab.contains(r#"<rect x="2" y="36" width="8""#),
            "meets the join: {tab}"
        );
        assert!(!tab.contains(r#"opacity="0.3""#), "{tab}");
        let lit = at(1., true);
        assert!(
            lit.contains(r#"width="10" height="80" rx="5" fill="rgb(10,20,30)" opacity="0.3""#),
            "{lit}"
        );
        // The free bar ignores hover so the dwell fill stays the cue.
        let free_hover = LipState {
            hovered: true,
            ..idle()
        };
        let hovered = grabber(&lip(Edge::Right), (10., 144.), free_hover, A, S, false).unwrap();
        assert_eq!(hovered, free);
        // While its panel is open a lip's rest box keeps only its plate: its pill rides the tab.
        let rest = grabber(
            &lip(Edge::Right),
            (10., 144.),
            docked(0., false),
            A,
            S,
            false,
        )
        .unwrap();
        assert!(
            rest.contains(r#"<rect width="10" height="144" fill="rgb(40,50,60)"/>"#)
                && !rest.contains("fill=\"rgb(10,20,30)\""),
            "{rest}"
        );
        // The left lip docks like the right one (its tab is the chat app's): pill alone, lit by hover.
        let left = grabber(&lip(Edge::Left), (10., 144.), docked(1., true), A, S, false).unwrap();
        assert!(
            !left.contains(r#"<rect width="10" height="144""#) && !left.contains("<path"),
            "{left}"
        );
        assert!(
            left.contains(r#"<rect x="0" y="36" width="8" height="72""#),
            "{left}"
        );
        let left_rest = grabber(
            &lip(Edge::Left),
            (10., 144.),
            docked(0., false),
            A,
            S,
            false,
        )
        .unwrap();
        assert!(!left_rest.contains("fill=\"rgb(10,20,30)\""), "{left_rest}");
        let left_idle = grabber(&lip(Edge::Left), (10., 144.), idle(), A, S, false).unwrap();
        assert!(
            left_idle.contains(r#"width="6" height="56""#),
            "{left_idle}"
        );
    }
    #[test]
    fn a_docked_tab_leaves_the_corner_pixel_clear_for_the_wallpaper() {
        use bead::render::{PixelFormat, ShmTarget, svg_into};
        let colors = Palette(
            std::collections::HashMap::from([
                ("surface".into(), [21, 18, 23, 255]),
                ("primary".into(), A),
                ("secondary".into(), S),
            ])
            .into(),
        );
        let svg = art(&colors).unwrap()(&lip(Edge::Right), (10., 144.), docked(1., false)).unwrap();
        let mut bytes = vec![0; 10 * 144 * 4];
        let target = ShmTarget {
            bytes: &mut bytes,
            width: 10,
            height: 144,
            scale: 1.,
            format: PixelFormat::Rgba,
        };
        svg_into(&svg, target).unwrap();
        let alpha = |x: usize, y: usize| bytes[(y * 10 + x) * 4 + 3];
        assert_eq!(
            alpha(0, 0),
            0,
            "top-left corner is transparent while docked"
        );
        assert_eq!(alpha(5, 72), 255, "the pill is opaque");
        assert_eq!(alpha(9, 72), 255, "the pill meets the join");
        assert_eq!(alpha(0, 72), 0, "the free side stays clear");
        assert_eq!(alpha(5, 30), 0, "no plate above the pill");
        // At 1.5 the join column is still fully covered: no gap to the panel (owner 2026-11-16).
        let mut bytes = vec![0; 15 * 216 * 4];
        let target = ShmTarget {
            bytes: &mut bytes,
            width: 15,
            height: 216,
            scale: 1.5,
            format: PixelFormat::Rgba,
        };
        svg_into(&svg, target).unwrap();
        assert_eq!(bytes[(108 * 15 + 14) * 4 + 3], 255, "flush at scale 1.5");
        assert_eq!(bytes[(108 * 15) * 4 + 3], 0);
    }
    #[test]
    fn the_top_handle_is_a_pill_under_the_bar_title_that_becomes_a_tab_when_docked() {
        use bead::render::{PixelFormat, ShmTarget, svg_into};
        let top = |state| grabber(&lip(Edge::Top), (208., 50.), state, A, S, true).unwrap();
        let raster = |svg: &str| {
            let mut bytes = vec![0; 208 * 50 * 4];
            let target = ShmTarget {
                bytes: &mut bytes,
                width: 208,
                height: 50,
                scale: 1.,
                format: PixelFormat::Rgba,
            };
            svg_into(svg.as_bytes(), target).unwrap();
            bytes
        };
        let alpha = |bytes: &[u8], x: usize, y: usize| bytes[(y * 208 + x) * 4 + 3];
        // Rest: only the pill (56 x 6 at y 42, in the frame strip); the bar area is untouched.
        let rest = raster(&top(idle()));
        for y in 0..50 {
            for x in 0..208 {
                let pill = (76..132).contains(&x) && (42..48).contains(&y);
                assert!(pill || alpha(&rest, x, y) == 0, "stray pixel {x},{y}");
            }
        }
        assert!(alpha(&rest, 104, 45) > 200);
        // Dwell: the glow stays inside the 10 px strip, never over the title.
        let half = LipState {
            progress: 0.5,
            ..idle()
        };
        let dwell = raster(&top(half));
        assert!(alpha(&dwell, 104, 40) > 0 && alpha(&dwell, 104, 38) == 0);
        assert!((0..40).all(|y| (0..208).all(|x| alpha(&dwell, x, y) == 0)));
        // Docked: the pill alone rides up to the join (y 0-8, touching the dashboard), no plate.
        let tab = top(docked(1., false));
        assert!(!tab.contains("<path"), "{tab}");
        let docked_px = raster(&tab);
        assert!(
            alpha(&docked_px, 104, 5) > 200,
            "the pill hugs the panel edge"
        );
        assert_eq!(alpha(&docked_px, 104, 20), 0, "no plate band under it");
        assert!(
            alpha(&docked_px, 104, 0) > 200,
            "touching the dashboard edge"
        );
        assert_eq!(alpha(&docked_px, 104, 45), 0, "it left the frame strip");
        assert_eq!(alpha(&docked_px, 20, 5), 0, "only the 72 px pill");
        let half_open = top(docked(0.5, false));
        assert!(!half_open.contains("<path"), "{half_open}");
        // Hover lights the docked tab: a glow rect around the pill.
        assert!(
            top(docked(1., true))
                .contains(r#"width="10" height="80" rx="5" fill="rgb(10,20,30)" opacity="0.3""#)
        );
        assert!(!tab.contains(r#"opacity="0.3""#));
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
    fn the_grabber_is_a_centred_bar_that_swells_and_glows_with_the_dwell() {
        assert!(grabber(&lip(Edge::Top), (208., 50.), idle(), A, S, false).is_none());
        assert!(grabber(&lip(Edge::Bottom), (208., 10.), idle(), A, S, true).is_none());
        let rest = grabber(&lip(Edge::Left), (10., 144.), idle(), A, S, false).unwrap();
        assert!(
            rest.contains(r#"<rect x="2" y="44" width="6" height="56" rx="3""#),
            "{rest}"
        );
        assert_eq!(
            rest.matches("<rect").count(),
            2,
            "plate + bar, no glow at rest: {rest}"
        );
        let half = LipState {
            progress: 0.5,
            active: false,
            ..Default::default()
        };
        let charging = grabber(&lip(Edge::Right), (10., 144.), half, A, S, false).unwrap();
        // Half way: the bar is 7 x 64 from the centre and the lip-wide glow sits behind it.
        assert!(
            charging.contains(r#"<rect x="1.5" y="40" width="7" height="64" rx="3.5""#),
            "{charging}"
        );
        assert!(
            charging.contains(r#"<rect x="0" y="36" width="10" height="72" rx="5" fill="rgb(10,20,30)" opacity="0.15""#),
            "{charging}"
        );
        assert!(!charging.contains("clip-path"), "{charging}");
        // Open: the tab (Bead floors its attach above 0) keeps the swollen bar and only the glow
        // goes, so the hand-over never draws a dark frame; the rest box (attach 0) is plate only.
        let open = LipState {
            progress: 0.,
            active: true,
            attach: 0.001,
            ..Default::default()
        };
        let opened = grabber(&lip(Edge::Right), (10., 144.), open, A, S, false).unwrap();
        // (It starts sliding onto the join: x 1.001.)
        assert!(
            opened.contains(
                r#"y="36" width="8" height="72" rx="4" fill="rgb(10,20,30)" opacity="1""#
            ),
            "{opened}"
        );
        assert_eq!(opened.matches("<rect").count(), 2, "{opened}");
        let rest_box = LipState { attach: 0., ..open };
        assert_eq!(
            grabber(&lip(Edge::Right), (10., 144.), rest_box, A, S, false)
                .unwrap()
                .matches("<rect")
                .count(),
            1,
            "the rest box of an open lip is its plate alone"
        );
        let done = LipState {
            progress: 1.,
            active: false,
            ..Default::default()
        };
        assert_eq!(
            grabber(&lip(Edge::Right), (10., 144.), done, A, S, false)
                .unwrap()
                .matches("<rect")
                .count(),
            2,
            "a completed dwell draws no glow"
        );
        // The pill only grows from the dwell to the docked tab: no dip back to rest size between
        // the dwell's end (open pending) and the panel taking it (HOLD_COMPLETED_DWELL).
        let long = |state: LipState| {
            let svg = grabber(&lip(Edge::Right), (10., 144.), state, A, S, false).unwrap();
            [56, 60, 64, 68, 72]
                .into_iter()
                .find(|l| svg.contains(&format!(r#"height="{l}" rx="#)))
                .unwrap_or(0)
        };
        let steps = [
            LipState {
                progress: 0.25,
                ..idle()
            },
            LipState {
                progress: 0.5,
                ..idle()
            },
            LipState {
                progress: 1.,
                ..idle()
            },
            LipState {
                active: true,
                attach: 0.001,
                ..Default::default()
            },
        ];
        let sizes: Vec<_> = steps.into_iter().map(long).collect();
        assert!(
            sizes.windows(2).all(|w| w[0] <= w[1]) && sizes.last() == Some(&72),
            "{sizes:?}"
        );
        assert!(
            handle(
                &lip(Edge::Left),
                (10., 144.),
                idle(),
                A,
                S,
                S,
                (true, false)
            )
            .unwrap()
            .contains("<path")
        );
        assert!(
            !handle(
                &lip(Edge::Left),
                (10., 144.),
                idle(),
                A,
                S,
                S,
                (false, false)
            )
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
            ..Default::default()
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
                LipState {
                    progress,
                    active,
                    ..Default::default()
                },
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
                ..Default::default()
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
                    ..Default::default()
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
