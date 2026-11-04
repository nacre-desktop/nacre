// SPDX-License-Identifier: GPL-3.0-only
//! B0036: the owner's panel enable flags and frame sheen as the shell loop's live
//! switches. Nacre keys stay in `prefs`; this maps decoded values onto the generic
//! `ShellControl` answers (bar present, panel allowed, decoration variant).
use crate::{
    frame_config::{FrameConfig, LipConfig},
    placement::{Frame, FrameSettings},
    prefs::Prefs,
};
use bead::core::source::{Snapshot, Source, Subscription};
use bead::wayland::{
    EdgeChrome, ExternalEdgeCue, ExternalEdgeCueSender, ExternalEdgePhase, ShellControl,
};
use std::collections::BTreeMap;
use std::sync::{
    Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering::SeqCst},
};

/// The frame-related answers (B0047): decoded settings plus the lip configuration.
#[derive(Default)]
struct FrameState {
    settings: Option<FrameSettings>,
    ai: bool,
    delay: u32,
    click: bool,
    config: FrameConfig,
    /// The parent owns these lips; child app backdrops only preserve their input.
    parent_lips: Option<Vec<LipConfig>>,
    /// Panel views that exist; a lip whose view is missing is not shown.
    views: std::collections::BTreeSet<String>,
}

impl FrameState {
    fn settings(&self, bar: bool) -> FrameSettings {
        self.settings.unwrap_or(FrameSettings {
            width: 10.,
            rounding: 25.,
            left: true,
            right: true,
            bottom: true,
            top: true,
            bar,
        })
    }
}
pub(crate) fn configured_lip(config: &LipConfig, frame: Frame) -> Option<Lip> {
    let thickness = match config.edge {
        bead::core::Edge::Top => frame.top,
        bead::core::Edge::Bottom => frame.bottom,
        bead::core::Edge::Left => frame.left,
        bead::core::Edge::Right => frame.right,
    };
    (thickness > 0.).then(|| Lip {
        id: config.id.clone(),
        edge: config.edge,
        length: config.length,
        thickness,
        opens: config.opens.clone(),
    })
}

pub struct Gate {
    bar: AtomicBool,
    dashboard: AtomicBool,
    controls: AtomicBool,
    sheen: AtomicBool,
    ai: AtomicBool,
    pearls: AtomicBool,
    pearl_drag: AtomicBool,
    pearl_views: Mutex<crate::pearls::Policy>,
    variant: AtomicU64,
    native_edge_source: AtomicBool,
    frame: Mutex<FrameState>,
    watchers: Mutex<Vec<Box<dyn Fn() + Send + Sync>>>,
}
#[derive(Default)]
struct EdgeSnapshots {
    compositor_connected: bool,
    event_connected: bool,
    acknowledged: bool,
    cue_capable: bool,
    outputs: BTreeMap<String, (i32, i32)>,
}
pub(crate) struct EdgeEventState {
    gate: std::sync::Arc<Gate>,
    snapshots: Mutex<EdgeSnapshots>,
    cue_sender: ExternalEdgeCueSender,
    active_cues: Mutex<BTreeMap<(String, &'static str), ExternalEdgeCue>>,
}
impl EdgeEventState {
    pub(crate) fn new(
        gate: std::sync::Arc<Gate>,
        cue_sender: ExternalEdgeCueSender,
    ) -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            gate,
            snapshots: Mutex::new(EdgeSnapshots::default()),
            cue_sender,
            active_cues: Mutex::new(BTreeMap::new()),
        })
    }
    fn publish_active(&self, snapshots: &EdgeSnapshots) {
        let active = snapshots.compositor_connected
            && snapshots.event_connected
            && snapshots.acknowledged
            && snapshots.cue_capable
            && !self.gate.edge_chrome().click_to_open;
        self.gate.set_native_edge_source(active);
        if !active {
            self.clear_native_hovers();
        }
    }
    fn update_compositor(&self, snapshot: &Snapshot) {
        let mut state = self.snapshots.lock().unwrap_or_else(|p| p.into_inner());
        state.compositor_connected = snapshot.connected;
        state.cue_capable = snapshot.data["capabilities"]["edge_trigger_cues"].as_bool()
            == Some(true);
        state.outputs = if snapshot.connected {
            snapshot.data["outputs"]
                .as_object()
                .into_iter()
                .flat_map(|outputs| outputs.iter())
                .filter_map(|(name, row)| {
                    let width = row["logicalWidth"].as_f64()?;
                    let height = row["logicalHeight"].as_f64()?;
                    (width.is_finite()
                        && height.is_finite()
                        && (1.0..=32768.0).contains(&width)
                        && (1.0..=32768.0).contains(&height))
                    .then(|| (name.clone(), (width.round() as i32, height.round() as i32)))
                })
                .collect()
        } else {
            BTreeMap::new()
        };
        self.clear_removed_outputs(&state.outputs);
        self.publish_active(&state);
    }
    fn update_event_source(&self, snapshot: &Snapshot) {
        let mut state = self.snapshots.lock().unwrap_or_else(|p| p.into_inner());
        state.event_connected = snapshot.connected;
        state.acknowledged =
            snapshot.connected && snapshot.data["edgeTriggersActive"].as_bool() == Some(true);
        self.publish_active(&state);
    }
    pub(crate) fn route_event(&self, message: &nacre_protocol::Message) {
        let nacre_protocol::Message::Ev {
            topic, name, data, ..
        } = message
        else {
            return;
        };
        if topic != "compositor.events" || name != "niri" {
            return;
        }
        let event_type = data["type"].as_str();
        let output = match data["output"].as_str().filter(|name| !name.is_empty()) {
            Some(output) => output,
            None => return,
        };
        let position = match data["position"].as_array().filter(|pair| pair.len() == 2) {
            Some(pair) => {
                let (Some(x), Some(y)) = (pair[0].as_f64(), pair[1].as_f64()) else { return; };
                let position = (x as f32, y as f32);
                if !(x.is_finite() && y.is_finite() && position.0.is_finite() && position.1.is_finite()) {
                    return;
                }
                position
            }
            None => return,
        };
        let (edge, phase, progress) = match event_type {
            Some("edge-approach") => (parse_edge(data["edge"].as_str()), Some(ExternalEdgePhase::Approach), cue_progress(data)),
            Some("edge-hover") => (parse_edge(data["edge"].as_str()), Some(ExternalEdgePhase::Hover), cue_progress(data)),
            Some("edge-triggered") => (parse_edge(data["edge"].as_str()), Some(ExternalEdgePhase::Triggered), Some(1.0)),
            Some("edge-left") => (parse_edge(data["edge"].as_str()), Some(ExternalEdgePhase::Left), Some(0.0)),
            _ => return,
        };
        let (Some(edge), Some(phase), Some(progress)) = (edge, phase, progress) else {
            return;
        };
        {
            let state = self.snapshots.lock().unwrap_or_else(|p| p.into_inner());
            if !(state.compositor_connected
                && state.event_connected
                && state.acknowledged
                && state.cue_capable
                && state.outputs.contains_key(output))
                || !self.gate.edge_chrome().external_source
            {
                return;
            }
        }
        {
            let mut cues = self.active_cues.lock().unwrap_or_else(|p| p.into_inner());
            let key = (output.to_owned(), edge_name(&edge));
            if phase == ExternalEdgePhase::Left {
                cues.remove(&key);
            } else {
                cues.insert(key, ExternalEdgeCue {
                    output: output.to_owned(),
                    edge,
                    position,
                    progress,
                    phase,
                });
            }
        }
        let _ = self.cue_sender.send(ExternalEdgeCue {
            output: output.to_owned(),
            edge,
            position,
            progress,
            phase,
        });
    }
    fn clear_native_hovers(&self) {
        let active = std::mem::take(&mut *self.active_cues.lock().unwrap_or_else(|p| p.into_inner()));
        self.send_left_cues(active.into_values());
    }
    fn clear_removed_outputs(&self, outputs: &BTreeMap<String, (i32, i32)>) {
        let removed = {
            let mut active = self.active_cues.lock().unwrap_or_else(|p| p.into_inner());
            let removed_keys: Vec<_> = active
                .iter()
                .filter_map(|(key, cue)| (!outputs.contains_key(&cue.output)).then(|| key.clone()))
                .collect();
            removed_keys.into_iter().filter_map(|key| active.remove(&key)).collect::<Vec<_>>()
        };
        self.send_left_cues(removed.into_iter());
    }
    fn send_left_cues(&self, cues: impl IntoIterator<Item = ExternalEdgeCue>) {
        for cue in cues {
            let _ = self.cue_sender.send(ExternalEdgeCue {
                output: cue.output,
                edge: cue.edge,
                position: cue.position,
                progress: 0.,
                phase: ExternalEdgePhase::Left,
            });
        }
    }
    #[cfg(test)]
    pub(crate) fn active_cues(&self) -> Vec<ExternalEdgeCue> {
        self.active_cues.lock().unwrap_or_else(|p| p.into_inner()).values().cloned().collect()
    }
    pub(crate) fn refresh_source(&self) {
        let snapshots = self.snapshots.lock().unwrap_or_else(|p| p.into_inner());
        self.publish_active(&snapshots);
    }
}

fn parse_edge(edge: Option<&str>) -> Option<bead::core::Edge> {
    match edge? {
        "left" => Some(bead::core::Edge::Left),
        "right" => Some(bead::core::Edge::Right),
        "top" => Some(bead::core::Edge::Top),
        "bottom" => Some(bead::core::Edge::Bottom),
        _ => None,
    }
}
fn edge_name(edge: &bead::core::Edge) -> &'static str {
    match edge {
        bead::core::Edge::Left => "left",
        bead::core::Edge::Right => "right",
        bead::core::Edge::Top => "top",
        bead::core::Edge::Bottom => "bottom",
    }
}
fn cue_progress(data: &serde_json::Value) -> Option<f32> {
    let progress = data["progress"].as_f64()?;
    (progress.is_finite() && (0.0..=1.0).contains(&progress)).then_some(progress as f32)
}
pub(crate) struct EdgeSourceSubscriptions {
    _subscriptions: [Subscription; 2],
}
impl EdgeSourceSubscriptions {
    pub(crate) fn attach(
        state: std::sync::Arc<EdgeEventState>,
        compositor: std::sync::Arc<dyn Source>,
        events: std::sync::Arc<dyn Source>,
    ) -> Result<Self, String> {
        let compositor_state = state.clone();
        let compositor_subscription = compositor
            .subscribe(std::sync::Arc::new(move |snapshot| {
                compositor_state.update_compositor(&snapshot);
            }))
            .map_err(|error| error.to_string())?;
        let event_state = state.clone();
        let event_subscription = events
            .subscribe(std::sync::Arc::new(move |snapshot| {
                event_state.update_event_source(&snapshot);
            }))
            .map_err(|error| error.to_string())?;
        state.update_compositor(&compositor.snapshot());
        state.update_event_source(&events.snapshot());
        Ok(Self {
            _subscriptions: [compositor_subscription, event_subscription],
        })
    }
}
impl Gate {
    /// Run the watchers (something the tiles depend on changed outside the settings).
    pub fn notify(&self) {
        for watcher in self
            .watchers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .iter()
        {
            watcher();
        }
    }
    pub fn new() -> std::sync::Arc<Self> {
        std::sync::Arc::new(Self {
            bar: AtomicBool::new(true),
            dashboard: AtomicBool::new(true),
            controls: AtomicBool::new(true),
            sheen: AtomicBool::new(true),
            ai: AtomicBool::new(true),
            pearls: AtomicBool::new(true),
            pearl_drag: AtomicBool::new(true),
            pearl_views: Mutex::new(Default::default()),
            variant: AtomicU64::new(0),
            native_edge_source: AtomicBool::new(false),
            frame: Mutex::new(FrameState {
                ai: true,
                delay: 350,
                ..Default::default()
            }),
            watchers: Mutex::new(Vec::new()),
        })
    }
    pub fn set_native_edge_source(&self, active: bool) {
        if self.native_edge_source.swap(active, SeqCst) != active {
            self.notify();
        }
    }
    /// The lips the frame declares and the panel views that exist (call once at start).
    pub fn configure_frame(&self, config: FrameConfig, views: impl IntoIterator<Item = String>) {
        let mut frame = self.frame.lock().unwrap_or_else(|p| p.into_inner());
        frame.config = config;
        frame.views = views.into_iter().collect();
        drop(frame);
        crate::placement::set_frame_lips(&self.visible_frame_lips());
    }
    pub fn parent_lips(&self, config: FrameConfig) {
        self.frame_lock().parent_lips = Some(config.lips);
        crate::placement::set_frame_lips(&self.visible_frame_lips());
    }
    fn visible_frame_lips(&self) -> Vec<LipConfig> {
        let frame = self.frame_lock();
        frame
            .parent_lips
            .as_ref()
            .unwrap_or(&frame.config.lips)
            .iter()
            .filter(|l| {
                self.panel_allowed(&l.opens)
                    && (frame.parent_lips.is_some()
                        || frame.views.contains(&l.opens)
                        || (l.opens == "assistant" && crate::app::configured()))
            })
            .cloned()
            .collect()
    }
    fn frame_lock(&self) -> std::sync::MutexGuard<'_, FrameState> {
        self.frame.lock().unwrap_or_else(|p| p.into_inner())
    }
    /// Take the decoded values; watchers run only when an answer changed.
    pub fn apply(&self, p: &Prefs) {
        let mut changed = false;
        for (cell, value) in [
            (&self.bar, p.top_bar_enabled),
            (&self.dashboard, p.dashboard_enabled),
            (&self.controls, p.control_center_enabled),
        ] {
            changed |= cell.swap(value, SeqCst) != value;
        }
        changed |= self.ai.swap(p.ai_sidebar_enabled, SeqCst) != p.ai_sidebar_enabled;
        let lips_changed = changed;
        self.pearls.store(p.detachable_panels, SeqCst);
        self.pearl_drag.store(p.drag_panels, SeqCst);
        *self.pearl_views.lock().unwrap_or_else(|p| p.into_inner()) = crate::pearls::Policy {
            enabled: p.detachable_panels,
            drag: p.drag_panels,
            views: p.pearl_views.clone(),
        };
        let settings = p.frame_settings();
        let frame_changed = {
            let mut frame = self.frame_lock();
            let moved = frame.settings != Some(settings);
            frame.settings = Some(settings);
            frame.ai = p.ai_sidebar_enabled;
            let lips = (frame.delay, frame.click) != (p.lip_hover_delay, p.click_edge_menus);
            frame.delay = p.lip_hover_delay;
            frame.click = p.click_edge_menus;
            changed |= lips;
            moved
        };
        crate::placement::set_frame_lips(&self.visible_frame_lips());
        if self.sheen.swap(p.frame_sheen, SeqCst) != p.frame_sheen || frame_changed || lips_changed
        {
            self.variant.fetch_add(1, SeqCst);
            // Open panels carry their own frame art: rebuild them too.
            bead::ui::text_style::refresh();
            changed = true;
        }
        if changed {
            for watcher in self
                .watchers
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
            {
                watcher();
            }
        }
    }
}
impl ShellControl for Gate {
    fn watch(&self, changed: Box<dyn Fn() + Send + Sync>) {
        self.watchers
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .push(changed);
    }
    fn bar_enabled(&self) -> bool {
        self.bar.load(SeqCst)
    }
    /// Dashboard pages follow `dashboardEnabled`; the audio, network, Bluetooth and
    /// battery panels (and their hover peeks) follow `controlCenterEnabled`.
    fn panel_allowed(&self, view: &str) -> bool {
        let view = view.strip_prefix("peek-").unwrap_or(view);
        if view.starts_with("dashboard-") {
            self.dashboard.load(SeqCst)
        } else if matches!(
            view,
            "sound" | "audio" | "network" | "bluetooth" | "battery"
        ) {
            self.controls.load(SeqCst)
        } else if view == "assistant" {
            // The on-demand app owns the sidebar; its lip follows aiSidebarEnabled.
            self.ai.load(SeqCst)
        } else {
            true
        }
    }
    fn decoration_variant(&self) -> u64 {
        self.variant.load(SeqCst) + crate::placement::external_rev()
    }
    fn palette_changed(&self, palette: &std::collections::BTreeMap<String, [u8; 4]>) {
        crate::app::set_live_palette(palette);
        if crate::placement::set_live_palette(palette) {
            self.variant.fetch_add(1, SeqCst);
            for watcher in self
                .watchers
                .lock()
                .unwrap_or_else(|p| p.into_inner())
                .iter()
            {
                watcher();
            }
        }
    }
    /// The header (bar plus top frame edge) is the bar surface's thickness and zone.
    fn bar_thickness(&self) -> Option<u32> {
        let settings = self.frame_lock().settings?;
        Some(Frame::from_settings(&settings).top as u32)
    }
    fn external_panel_open(&self, view: &str, _output: &str) -> Option<bool> {
        (view == "assistant" && crate::app::configured())
            .then(|| crate::placement::external_open("chat"))
    }
    fn external_panel_request(&self, view: &str, output: &str, toggle: bool) {
        if view == "assistant" {
            crate::app::panel_request("chat", if toggle { "toggle" } else { "hover-open" }, output);
        }
    }
    fn external_panel_hover(&self, view: &str, output: &str, inside: bool) {
        if view == "assistant" {
            crate::app::panel_request(
                "chat",
                if inside { "hover-enter" } else { "hover-leave" },
                output,
            );
        }
    }
    fn edge_chrome(&self) -> EdgeChrome {
        crate::placement::set_frame_lips(&self.visible_frame_lips());
        let frame = self.frame_lock();
        let settings = frame.settings(self.bar.load(SeqCst));
        let geometry = Frame::from_settings(&settings);
        // With the bar its surface holds the header's zone; without it the top edge needs
        // its own strip.
        let top = if settings.bar { 0. } else { geometry.top };
        let lips = frame
            .config
            .lips
            .iter()
            .filter(|l| {
                (frame.views.contains(&l.opens)
                    || (l.opens == "assistant" && crate::app::configured()))
                    && self.panel_allowed(&l.opens)
            })
            .filter_map(|l| configured_lip(l, geometry))
            .collect();
        EdgeChrome {
            reserved: [
                top as u32,
                geometry.right as u32,
                geometry.bottom as u32,
                geometry.left as u32,
            ],
            lips,
            hover_delay_ms: frame.delay,
            click_to_open: frame.click,
            external_source: self.native_edge_source.load(SeqCst) && !frame.click,
        }
    }
    fn edge_input_regions(&self, output: (i32, i32)) -> Vec<bead::render::Rect> {
        let frame = self.frame_lock();
        if let Some(lips) = &frame.parent_lips {
            let geometry = Frame::from_settings(&frame.settings(self.bar.load(SeqCst)));
            EdgeChrome {
                lips: lips
                    .iter()
                    .filter(|l| self.panel_allowed(&l.opens))
                    .filter_map(|l| configured_lip(l, geometry))
                    .collect(),
                external_source: self.native_edge_source.load(SeqCst) && !frame.click,
                ..Default::default()
            }
            .input_regions(output)
        } else {
            drop(frame);
            self.edge_chrome().input_regions(output)
        }
    }
    fn pearls_enabled(&self) -> bool {
        self.pearls.load(SeqCst)
    }
    fn pearl_drag(&self) -> bool {
        self.pearl_drag.load(SeqCst)
    }
    fn pearl_allowed(&self, view: &str) -> bool {
        self.pearl_views
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .allows(view)
    }
    fn edge_hover_allowed(&self, open_panel: Option<&str>) -> bool {
        open_panel.is_none_or(|open| {
            !self
                .frame_lock()
                .config
                .blocked_by
                .iter()
                .any(|b| b == open)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Arc, atomic::AtomicU32};

    #[test]
    fn flags_gate_their_panels_and_wake_watchers_only_on_change() {
        let gate = Gate::new();
        let woke = Arc::new(AtomicU32::new(0));
        let counter = woke.clone();
        gate.watch(Box::new(move || {
            counter.fetch_add(1, SeqCst);
        }));
        assert!(gate.bar_enabled() && gate.panel_allowed("dashboard-today"));
        let off = Prefs::decode(
            &json!({"dashboardEnabled": false, "controlCenterEnabled": false, "topBarEnabled": false}),
            &json!(null),
        );
        gate.apply(&off);
        gate.apply(&off);
        assert_eq!(woke.load(SeqCst), 1);
        assert!(!gate.bar_enabled());
        for view in [
            "dashboard-today",
            "dashboard-weather",
            "sound",
            "peek-network",
            "battery",
        ] {
            assert!(!gate.panel_allowed(view), "{view}");
        }
        for view in ["launcher", "calendar", "session", "tray"] {
            assert!(gate.panel_allowed(view), "{view}");
        }
        // Wrong types and missing keys mean enabled, like QML's `!== false`.
        gate.apply(&Prefs::decode(
            &json!({"dashboardEnabled": 0, "topBarEnabled": "no"}),
            &json!(null),
        ));
        assert!(gate.bar_enabled() && gate.panel_allowed("dashboard-today"));
        // Count this gate's settings rebuilds independently of external app
        // visibility revisions changed by other tests in the same process.
        assert_eq!(gate.variant.load(SeqCst), 2);
        gate.apply(&Prefs::decode(&json!({"frameSheen": false}), &json!(null)));
        assert_eq!(gate.variant.load(SeqCst), 3);
    }

    fn lips() -> FrameConfig {
        crate::frame_config::parse(
            br#"return {stores={["frame.lips"]={
                {id="dashboard",edge="top",length=208,opens="dashboard-today"},
                {id="assistant",edge="left",length=144,opens="assistant"},
                {id="controls",edge="right",length=144,opens="sound"}},
                ["frame.lips.blocked_by"]={{id="launcher"}}}}"#,
            std::path::Path::new("t.luau"),
        )
        .unwrap()
    }
    fn gate_with(desktop: serde_json::Value) -> std::sync::Arc<Gate> {
        let gate = Gate::new();
        gate.configure_frame(
            lips(),
            ["dashboard-today", "sound", "launcher"].map(String::from),
        );
        gate.apply(&Prefs::decode(&desktop, &json!(null)));
        gate
    }
    fn ids(chrome: &EdgeChrome) -> Vec<(&str, f32)> {
        chrome
            .lips
            .iter()
            .map(|l| (l.id.as_str(), l.thickness))
            .collect()
    }

    #[test]
    fn default_frame_reserves_three_edges_and_the_bar_holds_the_top() {
        let gate = gate_with(json!({}));
        let chrome = gate.edge_chrome();
        // top, right, bottom, left; the bar's own zone is the 50 px header.
        assert_eq!(chrome.reserved, [0, 10, 10, 10]);
        assert_eq!(gate.bar_thickness(), Some(50));
        // The assistant lip is hidden: no such view exists yet.
        assert_eq!(ids(&chrome), [("dashboard", 50.), ("controls", 10.)]);
        assert_eq!((chrome.hover_delay_ms, chrome.click_to_open), (350, false));
        assert!(chrome.lips.iter().all(|l| l.length > 0.));
    }

    #[test]
    fn frame_settings_resize_reserve_and_hide_lips_live() {
        let gate = gate_with(json!({
            "frameWidth": 14, "frameLeftEnabled": false, "frameRightEnabled": false,
            "frameBottomEnabled": false, "lipHoverDelay": 120, "clickEdgeMenus": true,
        }));
        let chrome = gate.edge_chrome();
        assert_eq!(chrome.reserved, [0, 0, 0, 0]);
        assert_eq!(gate.bar_thickness(), Some(54), "40 bar + 14 top edge");
        // The right frame edge is off, so the lip that sat on it has no room.
        assert_eq!(ids(&chrome), [("dashboard", 54.)]);
        assert_eq!((chrome.hover_delay_ms, chrome.click_to_open), (120, true));
        // Bar and top edge both off: nothing reserved at the top and no top lip.
        let bare = gate_with(json!({"topBarEnabled": false, "frameTopEnabled": false}));
        assert_eq!(bare.edge_chrome().reserved, [0, 10, 10, 10]);
        assert!(bare.edge_chrome().lips.iter().all(|l| l.id != "dashboard"));
        // Bar off but the top edge on: the edge needs its own strip and hosts the lip.
        let strip = gate_with(json!({"topBarEnabled": false}));
        assert_eq!(strip.edge_chrome().reserved, [10, 10, 10, 10]);
        assert_eq!(
            ids(&strip.edge_chrome()),
            [("dashboard", 10.), ("controls", 10.)]
        );
    }

    #[test]
    fn lips_follow_their_panels_enable_flags_and_blockers() {
        let gate = gate_with(json!({"dashboardEnabled": false, "controlCenterEnabled": false}));
        assert!(gate.edge_chrome().lips.is_empty());
        let gate = gate_with(json!({}));
        assert!(gate.edge_hover_allowed(None));
        assert!(gate.edge_hover_allowed(Some("dashboard-today")));
        assert!(!gate.edge_hover_allowed(Some("launcher")));
        // Changing only the delay wakes watchers without rebuilding frame tiles.
        let woke = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let counter = woke.clone();
        gate.watch(Box::new(move || {
            counter.fetch_add(1, SeqCst);
        }));
        let variant = gate.decoration_variant();
        gate.apply(&Prefs::decode(&json!({"lipHoverDelay": 500}), &json!(null)));
        assert_eq!(woke.load(SeqCst), 1);
        assert_eq!(gate.decoration_variant(), variant);
        assert_eq!(gate.edge_chrome().hover_delay_ms, 500);
    }
    #[test]
    fn child_backdrops_preserve_parent_lips_without_creating_duplicate_lips() {
        let gate = Gate::new();
        gate.parent_lips(
            crate::frame_config::parse(
                include_bytes!("../../../shell/frame/behavior.luau"),
                std::path::Path::new("frame/behavior.luau"),
            )
            .unwrap(),
        );
        assert!(gate.edge_chrome().lips.is_empty());
        let regions = gate.edge_input_regions((1920, 1200));
        let covers = |x: f32, y: f32| {
            regions
                .iter()
                .any(|r| x >= r.x && x < r.x + r.w && y >= r.y && y < r.y + r.h)
        };
        assert!(covers(3., 600.) && covers(1917., 600.) && covers(960., 3.));
        assert!(!covers(200., 600.));
        gate.apply(&Prefs::decode(
            &serde_json::json!({"aiSidebarEnabled":false}),
            &serde_json::Value::Null,
        ));
        assert!(
            gate.visible_frame_lips()
                .iter()
                .all(|l| l.edge != bead::core::Edge::Left)
        );
        assert!(
            !gate
                .edge_input_regions((1920, 1200))
                .iter()
                .any(|r| r.x == 0.)
        );
    }
    #[test]
    fn native_cue_takeover_keeps_visual_lips_inputless_and_restores_click_input() {
        let gate = Gate::new();
        gate.configure_frame(
            FrameConfig {
                lips: vec![
                    LipConfig {
                        id: "assistant".into(),
                        edge: bead::core::Edge::Left,
                        length: 100.,
                        opens: "assistant".into(),
                    },
                    LipConfig {
                        id: "status".into(),
                        edge: bead::core::Edge::Left,
                        length: 200.,
                        opens: "status".into(),
                    },
                ],
                ..Default::default()
            },
            ["assistant".into(), "status".into()],
        );
        gate.set_native_edge_source(true);
        assert!(gate.edge_chrome().external_source);
        assert!(gate.edge_input_regions((1000, 800)).is_empty());
        gate.apply(&Prefs::decode(
            &json!({"clickEdgeMenus":true}),
            &json!(null),
        ));
        assert!(!gate.edge_chrome().external_source);
        assert!(!gate.edge_input_regions((1000, 800)).is_empty());
    }
}
