// SPDX-License-Identifier: GPL-3.0-only
//! The existing bounded panel lifecycle hosts the launcher without warm rasters.
use bead::{
    core::Edge,
    render::{Palette, Primitive, Rect},
    wayland::PanelSpec,
};
use std::path::PathBuf;
/// No border (A): the launcher rises 24 px from below with a fade (owner's pick, "rise"), never growing
/// from an edge or the logo (180 ms out-quint; close 120 ms ease-in).
pub fn island_motion(motion: &mut bead::render::Motion) {
    motion.edge = Edge::Bottom;
    motion.duration = 0.18;
    motion.auto_open = false;
    motion.spring = false;
    motion.curve = Some(bead::core::animation::Curve::OutQuint);
    motion.return_duration = Some(0.12);
    motion.return_curve = Some(bead::core::animation::Curve::InCubic);
    motion.fade = true;
    motion.reveal = false;
    motion.slide_distance = Some(24.);
}
/// The card's surface origin: body centred on the output, 13 px above its bottom edge.
fn island_origin(scene: &bead::render::Scene, output: (i32, i32)) -> (i32, i32) {
    let body = crate::placement::body(scene);
    (
        ((output.0 as f32 - body.w) / 2. - body.x + scene.bounds.x).round() as i32,
        (output.1 as f32 - 13. - body.h - body.y + scene.bounds.y).round() as i32,
    )
}
/// Frame (B): today's bottom drawer. No border (A): the card above.
fn position(drawer: bead::wayland::PanelPositioner) -> bead::wayland::PanelPositioner {
    std::sync::Arc::new(move |scene, center, output| {
        if crate::placement::Frame::current().bordered {
            drawer(scene, center, output)
        } else {
            island_origin(scene, output)
        }
    })
}
pub fn spec(
    fill: [u8; 4],
    palette: PathBuf,
    colors: Palette,
    source: crate::launcher::SourceChoice,
    trace: bool,
    memory: crate::launcher::Memory,
    prefs: Option<std::sync::Arc<dyn bead::core::source::Source>>,
) -> PanelSpec {
    let mut fallback = crate::sound::fallback(fill);
    fallback.width = 1100.;
    // Match the final content geometry; layout uses output logical dimensions.
    fallback.bounds = Rect::new(-32., -32., 1164., 654.);
    fallback.primitives = vec![Primitive::Rounded {
        radial: None,
        rect: Rect::new(0., 0., 1100., 590.),
        radius: 12.,
        fill,
        border: None,
    }];
    if let Some(motion) = &mut fallback.motion {
        motion.id = "launcher".into();
        motion.edge = Edge::Bottom;
        motion.duration = bead::core::animation::panel_open_duration(590.);
        motion.fade = false;
        motion.reveal = true;
        motion.slide_distance = Some(590.);
    }
    let cold = fallback.clone();
    PanelSpec {
        position: Some(position(
            crate::placement::Frame::default()
                .positioner(crate::placement::Attachment::Attached(Edge::Bottom)),
        )),
        engages_decorations: false,
        overlay: false,
        retain_when_closed: false,
        passive: false,
        keyboard_focus: true,
        text_focus_only: false,
        initial_open: false,
        fallback,
        fallback_factory: None,
        renderer: bead::render::RendererKind::Cpu,
        factory: std::sync::Arc::new(move |open| {
            let mut cold = cold.clone();
            if !crate::placement::Frame::current().bordered
                && let Some(motion) = &mut cold.motion
            {
                island_motion(motion);
            }
            let session = bead::ui::Session::new(
                "shell".into(),
                "ui/main.luau".into(),
                palette.clone(),
                colors.clone(),
                cold,
            );
            let quick = source.quick().map_err(|e| e.to_string())?;
            let source = source.connect().map_err(|e| e.to_string())?;
            let mut runtime = crate::reload::Runtime::panel_session(session, open);
            if let Some(amount) = crate::theme::launcher_icon_tint(&colors) {
                runtime.session.stores = runtime
                    .session
                    .stores
                    .reconcile(&std::collections::BTreeMap::from([(
                        "launcher.icon_tint".into(),
                        bead::core::Value::Number(amount),
                    )]))
                    .map_err(|e| e.to_string())?;
            }
            runtime
                .with_trace(trace)
                .with_frame(
                    crate::placement::Attachment::Attached(Edge::Bottom),
                    &colors,
                )
                .and_then(|r| r.with_launcher(source))
                .and_then(|r| r.with_launcher_quick(quick))
                .and_then(|r| r.with_launcher_memory(memory.clone()))
                .and_then(|r| match &prefs {
                    Some(source) => r.with_prefs(source.clone()),
                    None => Ok(r),
                })
                .map(|r| Box::new(r) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_launcher_rises_at_the_bottom_centre_and_frame_keeps_the_drawer() {
        let mut scene = bead::render::Scene::empty(1164., 654.);
        scene.bounds = Rect::new(-32., -32., 1164., 654.);
        scene.primitives = vec![Primitive::Rounded {
            radial: None,
            rect: Rect::new(0., 0., 1100., 590.),
            radius: 12.,
            fill: [0; 4],
            border: None,
        }];
        // Body left (1920 - 1100) / 2 = 410, top 1200 - 13 - 590 = 597, less the 32 px shadow margin.
        assert_eq!(island_origin(&scene, (1920, 1200)), (378, 565));
        // The live frame is bordered by default: the drawer positioner answers unchanged.
        let drawer = crate::placement::Frame::default()
            .positioner(crate::placement::Attachment::Attached(Edge::Bottom));
        assert_eq!(
            position(drawer.clone())(&scene, None, (1920, 1200)),
            drawer(&scene, None, (1920, 1200))
        );
    }
}
