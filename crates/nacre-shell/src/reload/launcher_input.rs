// SPDX-License-Identifier: GPL-3.0-only
//! Keyboard movement uses the rendered rail, grid/list and quick-control geometry.
use super::Runtime;
use bead::{core::Value, render::Scene};
use std::{collections::BTreeMap, error::Error};

fn rail(action: &str, arg: &Value) -> bool {
    action == "navigate" || (action == "launcher.app" && arg == &Value::Text("widgets".into()))
}

impl Runtime {
    pub(super) fn launcher_rail_focused(&self) -> bool {
        self.session.focused_action().is_some_and(|(_, action, arg)| rail(&action, &arg))
    }

    fn launcher_focus(&mut self, id: &str) -> Result<Option<Scene>, Box<dyn Error>> {
        let mut changes = BTreeMap::from([
            ("bead.focused".into(), Value::Text(id.into())),
            ("bead.focus_visible".into(), Value::Bool(true)),
        ]);
        if let Some(node) = self.session.view_nodes().iter().find(|node| node.id == id)
            && let Some(clip) = node.clip
            && node.action.as_deref().zip(node.action_arg.as_ref())
                .is_some_and(|(action, arg)| rail(action, arg))
        {
            let rect = bead::ui::rect(&node.draw);
            let delta = if rect.y < clip.y { rect.y - clip.y }
                else { (rect.y + rect.h - clip.y - clip.h).max(0.) };
            let scroll = match self.session.stores.get("launcher.rail.scroll") {
                Some(Value::Number(scroll)) => *scroll,
                _ => 0.,
            };
            changes.insert("launcher.rail.scroll".into(), Value::Number((scroll + f64::from(delta)).max(0.)));
        }
        let zone = match self.session.view_nodes().iter().find(|node| node.id == id).and_then(|node| node.action.as_deref()) {
            Some("select_app") => "content",
            Some("launcher.quick") => "quick",
            _ => "rail",
        };
        changes.insert("launcher.focus_zone".into(), Value::Text(zone.into()));
        Ok(self.session.update(changes)?)
    }

    pub(super) fn launcher_content_focus(&mut self, first: bool) -> Result<Option<Scene>, Box<dyn Error>> {
        if first {
            self.session.update(BTreeMap::from([("launcher.scroll.y".into(), Value::Number(0.))]))?;
        }
        let selected = self.session.stores.get("launcher.selection");
        let target = self.session.view_nodes().iter()
            .filter(|node| node.action.as_deref() == Some("select_app"))
            .find(|node| first || node.action_arg.as_ref() == selected)
            .map(|node| (node.id.clone(), node.action_arg.clone().unwrap()));
        if let Some((id, arg)) = target {
            self.session.update(BTreeMap::from([("launcher.selection".into(), arg)]))?;
            return self.launcher_focus(&id);
        }
        // Empty categories retain a keyboard path to both side zones.
        Ok(self.session.focus_id("launcher.search")?)
    }

    pub(super) fn launcher_key(&mut self, key: &str, shift: bool) -> Result<Option<Scene>, Box<dyn Error>> {
        if let Some((text, cursor, _)) = self.session.surrounding_text()
            && ((key == "left" && !text.is_empty() && cursor > 0)
                || (key == "right" && !text.is_empty()))
        {
            return Ok(self.session.edit_key(if key == "left" {
                bead::ui::input::EditKey::Left
            } else { bead::ui::input::EditKey::Right }, shift)?);
        }
        let focused = self.session.focused_action();
        let rails = self.session.view_nodes().iter()
            .filter(|node| node.action.as_deref().zip(node.action_arg.as_ref())
                .is_some_and(|(action, arg)| rail(action, arg)))
            .map(|node| (node.id.clone(), node.action_arg.clone().unwrap())).collect::<Vec<_>>();
        let quick = self.session.view_nodes().iter()
            .filter(|node| node.action.as_deref() == Some("launcher.quick"))
            .map(|node| node.id.clone()).collect::<Vec<_>>();
        if let Some(index) = rails.iter().position(|(id, _)| focused.as_ref().is_some_and(|(focus, _, _)| focus == id)) {
            if matches!(key, "right" | "enter") { return self.launcher_content_focus(true); }
            let next = match key { "up" => index.saturating_sub(1), "down" => (index + 1).min(rails.len() - 1), _ => index };
            if next != index {
                let changes = self.session.action_values("navigate", rails[next].1.clone())?;
                self.session.update(changes)?;
            }
            return self.launcher_focus(&rails[next].0);
        }
        if let Some(index) = quick.iter().position(|id| focused.as_ref().is_some_and(|(focus, _, _)| focus == id)) {
            if key == "left" { return self.launcher_content_focus(false); }
            let next = match key { "up" => index.saturating_sub(1), "down" => (index + 1).min(quick.len() - 1), _ => index };
            return self.launcher_focus(&quick[next]);
        }
        let buttons = self.session.view_nodes().iter()
            .filter(|node| node.action.as_deref() == Some("select_app"))
            .map(|node| (node.action_arg.clone(), bead::ui::rect(&node.draw), node.clip))
            .collect::<Vec<_>>();
        let first = buttons.first();
        let columns = first.map_or(1, |(_, rect, _)| buttons.iter().filter(|(_, r, _)| (r.y - rect.y).abs() < 0.5).count()).max(1);
        let index = buttons.iter().position(|(arg, _, _)| arg.as_ref() == self.session.stores.get("launcher.selection")).unwrap_or(0);
        if key == "left" && (self.session.surrounding_text().is_some() || index % columns == 0) {
            let navigation = self.session.resolve("launcher.navigation")?;
            let active = match &navigation {
                Value::List(rows) => rows.iter().find(|row| row["selected"] == true).and_then(|row| row["id"].as_str()),
                _ => None,
            };
            if let Some((id, _)) = rails.iter().find(|(_, arg)| active.is_some_and(|active| arg == &Value::Text(active.into()))).or(rails.first()) {
                return self.launcher_focus(id);
            }
        }
        if key == "right" {
            if self.session.surrounding_text().is_some() { return self.launcher_content_focus(true); }
            if (index % columns == columns - 1 || index + 1 == buttons.len()) && let Some(id) = quick.first() {
                return self.launcher_focus(id);
            }
        }
        if let Some((_, rect, clip)) = first {
            let pitch = buttons.iter().map(|(_, r, _)| f64::from(r.y - rect.y)).filter(|d| *d > 0.5).min_by(f64::total_cmp).unwrap_or(f64::from(rect.h));
            let geometry = crate::launcher::NavigationGeometry {
                columns, pitch, row_height: f64::from(rect.h),
                viewport_height: f64::from(clip.map_or(rect.h, |clip| clip.h)),
            };
            if let Some(changes) = self.launcher.as_ref().and_then(|menu| menu.navigate(&self.session.stores, key, geometry)) {
                self.session.update(changes)?;
                return self.launcher_content_focus(false);
            }
        }
        Ok(None)
    }
}
