// SPDX-License-Identifier: GPL-3.0-only
//! The transient notice stack and OSD as panel views of the main shell (B0030). They
//! share its renderer, fonts and palette and exist only while shown: the shell's
//! panel lifecycle releases them when idle. Both read one [`Notices`] backend.
use crate::notifications::Notices;
use bead::{
    core::Edge,
    render::{Palette, Rect},
    wayland::PanelSpec,
};
use std::{
    path::PathBuf,
    sync::{Arc, OnceLock},
};

/// The bar's exclusive zone, the QML stack inset, the scene's shadow margin and the OSD
/// pill's lift above the output's bottom edge (design review D10) (px).
const BAR_HEIGHT: i32 = 50;
const INSET: i32 = 12;
const SHADOW: i32 = 32;
const OSD_LIFT: i32 = 96;

/// The owner can arrive after the specs are built (private compatibility owner).
pub type Owner = Arc<OnceLock<Arc<dyn Notices>>>;

/// What the shell supplies to every notice view.
#[derive(Clone)]
pub struct Style {
    pub fill: [u8; 4],
    pub palette: PathBuf,
    pub colors: Palette,
    pub trace: bool,
}

struct Geometry {
    id: &'static str,
    edge: Edge,
    width: f32,
    bounds: Rect,
    entry: &'static str,
    position: Option<bead::wayland::PanelPositioner>,
}

fn spec(owner: Owner, style: Style, geometry: Geometry, interactive: bool) -> PanelSpec {
    let mut fallback = crate::sound::fallback(style.fill);
    fallback.width = geometry.width;
    fallback.bounds = geometry.bounds;
    fallback.primitives.clear();
    if let Some(motion) = &mut fallback.motion {
        motion.id = geometry.id.into();
        motion.edge = geometry.edge;
    }
    let cold = fallback.clone();
    let entry = geometry.entry;
    PanelSpec {
        position: geometry.position,
        engages_decorations: false,
        overlay: false,
        retain_when_closed: false,
        // Notices take pointer input (hover pauses the producer, cards click) but
        // never keyboard focus or an outside-dismiss layer. The private compatibility
        // owner's OSD stays click-through.
        passive: !interactive,
        keyboard_focus: false,
        text_focus_only: false,
        initial_open: false,
        fallback,
        fallback_factory: None,
        renderer: bead::render::RendererKind::Cpu,
        factory: Arc::new(move |open| {
            let session = bead::ui::Session::new(
                "shell".into(),
                entry.into(),
                style.palette.clone(),
                style.colors.clone(),
                cold.clone(),
            );
            let owner = owner.get().cloned().ok_or("notification owner not ready")?;
            crate::reload::Runtime::panel_session(session, open)
                .with_trace(style.trace)
                .with_notification_owner(owner, open)
                .map(|r| Box::new(r) as Box<dyn bead::wayland::SceneSource>)
                .map_err(|e| e.to_string())
        }),
    }
}

/// Bottom-centre level pill (QML `NacreOsd`), its lower edge [`OSD_LIFT`] above the output's.
pub fn osd(owner: Owner, style: Style, interactive: bool) -> PanelSpec {
    let geometry = Geometry {
        id: crate::notice_service::VIEW_OSD,
        edge: Edge::Bottom,
        width: 336.,
        bounds: Rect::new(-32., -32., 400., 124.),
        entry: "osd/main.luau",
        position: Some(Arc::new(|scene, _, (output_width, output_height)| {
            (
                (output_width - scene.bounds.w.ceil() as i32) / 2,
                output_height - scene.bounds.h.ceil() as i32 + SHADOW - OSD_LIFT,
            )
        })),
    };
    spec(owner, style, geometry, interactive)
}

/// Top-right popup stack (QML `NacreNotificationStack`), flush with the usable area.
pub fn notices(owner: Owner, style: Style) -> PanelSpec {
    let geometry = Geometry {
        id: "notifications",
        edge: Edge::Right,
        width: 380.,
        bounds: Rect::new(-32., -32., 444., 65.),
        entry: "notifications/main.luau",
        position: Some(Arc::new(|scene, _, (output_width, _)| {
            // Panels ignore the bar's exclusive zone, so clear it here. The scene's
            // 32 px shadow margin sits inside the 12 px QML inset.
            (
                output_width - scene.bounds.w.ceil() as i32 + SHADOW - INSET,
                BAR_HEIGHT + INSET - SHADOW,
            )
        })),
    };
    spec(owner, style, geometry, true)
}
