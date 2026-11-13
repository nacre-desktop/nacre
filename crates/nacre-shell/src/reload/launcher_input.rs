// SPDX-License-Identifier: GPL-3.0-only
//! Keyboard movement uses the rendered rail, grid/list and quick-control geometry.
use super::Runtime;
use bead::{
    core::{Stores, Value},
    render::Scene,
};
use std::{collections::BTreeMap, error::Error};

fn rail(action: &str, arg: &Value) -> bool {
    action == "navigate" || (action == "launcher.app" && arg == &Value::Text("widgets".into()))
}

fn key_id(key: &str) -> String {
    key.bytes().map(|byte| format!("{byte:02x}")).collect()
}

impl Runtime {
    pub(super) fn launcher_chooser_focus(&mut self, step: i32, close: bool) -> Result<Option<Scene>, Box<dyn Error>> {
        let group = match self.session.stores.get("launcher.chooser") {
            Some(Value::Text(id)) => id.clone(),
            _ => return Ok(None),
        };
        let choices = match self.session.stores.get("launcher.apps") {
            Some(Value::List(rows)) => rows.iter().find(|row| row["id"] == group)
                .and_then(|row| row["choices"].as_array()).cloned().unwrap_or_default(),
            _ => Vec::new(),
        };
        let focused = self.session.focused_action().map(|(_, _, arg)| arg);
        let index = choices.iter().position(|choice| focused.as_ref() == Some(&Value::Text(choice["id"].as_str().unwrap_or_default().into())))
            .map(|index| (index as i32 + step).rem_euclid(choices.len() as i32) as usize)
            .or_else(|| choices.iter().position(|choice| choice["checked"] == true)).unwrap_or(0);
        let target = if close { group.as_str() } else { choices.get(index).and_then(|choice| choice["id"].as_str()).unwrap_or("") };
        let node = self.session.view_nodes().iter().find(|node| node.action.as_deref() == Some("select_app")
            && node.action_arg == Some(Value::Text(target.into()))).map(|node| node.id.clone());
        let mut changes = BTreeMap::from([
            ("bead.focused".into(), Value::Text(node.unwrap_or_else(|| "launcher.search".into()))),
            ("bead.focus_visible".into(), Value::Bool(true)),
            ("launcher.focus_zone".into(), Value::Text(if close { "content" } else { "chooser" }.into())),
        ]);
        if close { changes.insert("launcher.chooser".into(), Value::Text(String::new())); }
        Ok(self.session.update(changes)?)
    }
}

// Selection, virtualization scroll and focus must enter the same staged frame.
// Repeat's keyed path is already used by the launcher's hover/favorite controls.
pub(super) fn project_focus(stores: &Stores, changes: &mut BTreeMap<String, Value>) {
    let text = |key| match stores.get(key) {
        Some(Value::Text(value)) => value.as_str(),
        _ => "",
    };
    let template = text("launcher.focus_template");
    if text("launcher.focus_zone") == "content-pending" {
        if !text("launcher.form.kind").is_empty()
            || stores.get("panel.open") == Some(&Value::Bool(false))
            || text("bead.focused") != template
        {
            changes.insert("launcher.focus_zone".into(), Value::Text("search".into()));
        }
        return;
    }
    if text("launcher.focus_zone") != "content"
        || template.is_empty()
        || !text("launcher.form.kind").is_empty()
        || stores.get("panel.open") == Some(&Value::Bool(false))
        || text("bead.focused") != template.replace("{}", &key_id(text("launcher.selection")))
    {
        return;
    }
    let selected = changes
        .get("launcher.selection")
        .or_else(|| stores.get("launcher.selection"));
    if let Some(Value::Text(selected)) = selected {
        let target = if selected.is_empty() {
            "launcher.search".into()
        } else {
            template.replace("{}", &key_id(selected))
        };
        changes.insert("bead.focused".into(), Value::Text(target));
    }
}

impl Runtime {
    pub(super) fn launcher_rail_focused(&self) -> bool {
        self.session
            .focused_action()
            .is_some_and(|(_, action, _)| action == "navigate")
    }

    fn launcher_focus(&mut self, id: &str) -> Result<Option<Scene>, Box<dyn Error>> {
        let mut changes = BTreeMap::from([
            ("bead.focused".into(), Value::Text(id.into())),
            ("bead.focus_visible".into(), Value::Bool(true)),
        ]);
        if let Some(node) = self.session.view_nodes().iter().find(|node| node.id == id)
            && let Some(clip) = node.clip
            && node
                .action
                .as_deref()
                .zip(node.action_arg.as_ref())
                .is_some_and(|(action, arg)| rail(action, arg))
        {
            let rect = bead::ui::rect(&node.draw);
            let delta = if rect.y < clip.y {
                rect.y - clip.y
            } else {
                (rect.y + rect.h - clip.y - clip.h).max(0.)
            };
            let scroll = match self.session.stores.get("launcher.rail.scroll") {
                Some(Value::Number(scroll)) => *scroll,
                _ => 0.,
            };
            changes.insert(
                "launcher.rail.scroll".into(),
                Value::Number((scroll + f64::from(delta)).max(0.)),
            );
        }
        let zone = match self
            .session
            .view_nodes()
            .iter()
            .find(|node| node.id == id)
            .and_then(|node| node.action.as_deref())
        {
            Some("select_app") => "content",
            Some("launcher.quick") => "quick",
            _ => "rail",
        };
        changes.insert("launcher.focus_zone".into(), Value::Text(zone.into()));
        Ok(self.session.update(changes)?)
    }

    pub(super) fn launcher_content_focus(
        &mut self,
        first: bool,
        mut changes: BTreeMap<String, Value>,
    ) -> Result<Option<Scene>, Box<dyn Error>> {
        if (first || changes.is_empty())
            && self.session.stores.get("launcher.pending") == Some(&Value::Bool(true))
        {
            changes.insert(
                "launcher.focus_zone".into(),
                Value::Text("content-pending".into()),
            );
            changes.insert(
                "launcher.focus_template".into(),
                self.session
                    .stores
                    .get("bead.focused")
                    .cloned()
                    .unwrap_or(Value::Text("launcher.search".into())),
            );
            return Ok(self.session.update(changes)?);
        }
        let selected = if first {
            changes.insert("launcher.scroll.y".into(), Value::Number(0.));
            match self.session.stores.get("launcher.apps") {
                Some(Value::List(rows)) => rows
                    .iter()
                    .find(|row| row["disabled"] != true)
                    .and_then(|row| row["id"].as_str())
                    .unwrap_or("")
                    .to_owned(),
                _ => String::new(),
            }
        } else {
            match changes
                .get("launcher.selection")
                .or_else(|| self.session.stores.get("launcher.selection"))
            {
                Some(Value::Text(id)) => id.clone(),
                _ => String::new(),
            }
        };
        if let Some(node) = self
            .session
            .view_nodes()
            .iter()
            .find(|node| node.action.as_deref() == Some("select_app"))
            && let Some(Value::Text(key)) = &node.action_arg
            && !selected.is_empty()
        {
            let template = node.id.replace(&format!(".k{}.", key_id(key)), ".k{}.");
            changes.extend(BTreeMap::from([
                (
                    "bead.focused".into(),
                    Value::Text(template.replace("{}", &key_id(&selected))),
                ),
                ("bead.focus_visible".into(), Value::Bool(true)),
                ("launcher.focus_zone".into(), Value::Text("content".into())),
                ("launcher.focus_template".into(), Value::Text(template)),
                ("launcher.selection".into(), Value::Text(selected)),
            ]));
        } else {
            changes.insert("bead.focused".into(), Value::Text("launcher.search".into()));
            changes.insert("launcher.focus_zone".into(), Value::Text("search".into()));
        }
        Ok(self.session.update(changes)?)
    }

    pub(super) fn launcher_key(
        &mut self,
        key: &str,
        shift: bool,
    ) -> Result<Option<Scene>, Box<dyn Error>> {
        if let Some((text, cursor, _)) = self.session.surrounding_text()
            && ((key == "left" && !text.is_empty() && cursor > 0)
                || (key == "right" && !text.is_empty()))
        {
            let scene = self.session.edit_key(
                if key == "left" {
                    bead::ui::input::EditKey::Left
                } else {
                    bead::ui::input::EditKey::Right
                },
                shift,
            )?;
            return Ok(self.session.focus_id("launcher.search")?.or(scene));
        }
        let focused = self.session.focused_action();
        let rails = self
            .session
            .view_nodes()
            .iter()
            .filter(|node| {
                node.action
                    .as_deref()
                    .zip(node.action_arg.as_ref())
                    .is_some_and(|(action, arg)| rail(action, arg))
            })
            .map(|node| (node.id.clone(), node.action_arg.clone().unwrap()))
            .collect::<Vec<_>>();
        let quick = self
            .session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("launcher.quick"))
            .map(|node| node.id.clone())
            .collect::<Vec<_>>();
        if let Some(index) = rails
            .iter()
            .position(|(id, _)| focused.as_ref().is_some_and(|(focus, _, _)| focus == id))
        {
            if matches!(key, "right" | "enter") {
                return self.launcher_content_focus(true, BTreeMap::new());
            }
            let next = match key {
                "up" => index.saturating_sub(1),
                "down" => (index + 1).min(rails.len() - 1),
                _ => index,
            };
            if next != index && rails[next].1 != Value::Text("widgets".into()) {
                let changes = self
                    .session
                    .action_values("navigate", rails[next].1.clone())?;
                self.session.update(changes)?;
            }
            return self.launcher_focus(&rails[next].0);
        }
        if let Some(index) = quick
            .iter()
            .position(|id| focused.as_ref().is_some_and(|(focus, _, _)| focus == id))
        {
            if key == "left" {
                return self.launcher_content_focus(false, BTreeMap::new());
            }
            let next = match key {
                "up" => index.saturating_sub(1),
                "down" => (index + 1).min(quick.len() - 1),
                _ => index,
            };
            return self.launcher_focus(&quick[next]);
        }
        let buttons = self
            .session
            .view_nodes()
            .iter()
            .filter(|node| node.action.as_deref() == Some("select_app"))
            .map(|node| {
                (
                    node.action_arg.clone(),
                    bead::ui::rect(&node.draw),
                    node.clip,
                )
            })
            .collect::<Vec<_>>();
        let first = buttons.first();
        let columns = first
            .map_or(1, |(_, rect, _)| {
                buttons
                    .iter()
                    .filter(|(_, r, _)| (r.y - rect.y).abs() < 0.5)
                    .count()
            })
            .max(1);
        let index = buttons
            .iter()
            .position(|(arg, _, _)| arg.as_ref() == self.session.stores.get("launcher.selection"))
            .unwrap_or(0);
        if key == "left"
            && (self.session.surrounding_text().is_some() || index.is_multiple_of(columns))
        {
            let navigation = self.session.resolve("launcher.navigation")?;
            let active = match &navigation {
                Value::List(rows) => rows
                    .iter()
                    .find(|row| row["selected"] == true)
                    .and_then(|row| row["id"].as_str()),
                _ => None,
            };
            if let Some((id, _)) = rails
                .iter()
                .find(|(_, arg)| active.is_some_and(|active| arg == &Value::Text(active.into())))
                .or(rails.first())
            {
                return self.launcher_focus(id);
            }
        }
        if key == "right" {
            if self.session.surrounding_text().is_some() {
                return self.launcher_content_focus(true, BTreeMap::new());
            }
            if (index % columns == columns - 1 || index + 1 == buttons.len())
                && let Some(id) = quick.first()
            {
                return self.launcher_focus(id);
            }
        }
        if let Some((_, rect, clip)) = first {
            let pitch = buttons
                .iter()
                .map(|(_, r, _)| f64::from(r.y - rect.y))
                .filter(|d| *d > 0.5)
                .min_by(f64::total_cmp)
                .unwrap_or(f64::from(rect.h));
            let geometry = crate::launcher::NavigationGeometry {
                columns,
                pitch,
                row_height: f64::from(rect.h),
                viewport_height: f64::from(clip.map_or(rect.h, |clip| clip.h)),
            };
            if let Some(changes) = self
                .launcher
                .as_ref()
                .and_then(|menu| menu.navigate(&self.session.stores, key, geometry))
            {
                return self.launcher_content_focus(false, changes);
            }
        }
        Ok(None)
    }
}
