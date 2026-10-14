//! Restore application constraints faithfully; only new constrained tiled windows resize.
use crate::Result;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use unicode_casefold::UnicodeCaseFold;
pub mod cli;
pub mod runtime;
pub mod x11;

pub const MIN_SIZE: u64 = 1 << 4;
pub const MAX_SIZE: u64 = 1 << 5;
pub const MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct Pair {
    pub x: i32,
    pub y: i32,
}
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct SizeHints {
    pub flags: u64,
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
    pub min_width: i32,
    pub min_height: i32,
    pub max_width: i32,
    pub max_height: i32,
    pub width_inc: i32,
    pub height_inc: i32,
    pub min_aspect: Pair,
    pub max_aspect: Pair,
    pub base_width: i32,
    pub base_height: i32,
    pub win_gravity: i32,
}
pub type Identity = (BTreeSet<String>, String);
pub trait X11 {
    fn windows(&mut self) -> Result<Vec<u32>>;
    fn identity(&mut self, window: u32) -> Result<Option<Identity>>;
    fn ordinary(&mut self, window: u32) -> Result<bool>;
    fn hints(&mut self, window: u32) -> Result<Option<SizeHints>>;
    fn set_hints(&mut self, window: u32, hints: &SizeHints) -> Result<()>;
}
#[derive(Default, Serialize)]
pub struct Policy {
    pub windows: BTreeMap<u64, Value>,
    pub saved: BTreeMap<u32, i32>,
    pub mapping: BTreeMap<u32, BTreeSet<u64>>,
    pub classes: BTreeSet<String>,
    pub initialized: bool,
    pub new: BTreeSet<u64>,
    pub opening: BTreeMap<u64, f64>,
    pub eligible: BTreeSet<u64>,
}
fn identifier(window: &Value) -> Result<u64> {
    window["id"]
        .as_u64()
        .ok_or_else(|| "Invalid Niri window identifier".into())
}
fn class(window: &Value) -> String {
    window["app_id"]
        .as_str()
        .unwrap_or("")
        .case_fold()
        .collect()
}
impl Policy {
    pub fn event(&mut self, event: &Value) -> Result<bool> {
        if let Some(change) = event.get("WindowsChanged") {
            let mut windows = BTreeMap::new();
            for window in change["windows"]
                .as_array()
                .ok_or("Malformed Niri windows event")?
            {
                windows.insert(identifier(window)?, window.clone());
            }
            if self.initialized {
                self.new
                    .extend(windows.keys().filter(|id| !self.windows.contains_key(id)));
            }
            self.windows = windows;
            self.initialized = true;
            return Ok(true);
        }
        if let Some(change) = event.get("WindowOpenedOrChanged") {
            let window = &change["window"];
            let id = identifier(window)?;
            let previous = self.windows.get(&id);
            let changed = previous.is_some_and(|value| self.classes.contains(&class(value)))
                || self.classes.contains(&class(window));
            if self.initialized && previous.is_none() {
                self.new.insert(id);
            }
            self.windows.insert(id, window.clone());
            return Ok(changed);
        }
        if let Some(change) = event.get("WindowClosed") {
            let id = identifier(change)?;
            let previous = self.windows.remove(&id);
            self.new.remove(&id);
            self.opening.remove(&id);
            return Ok(previous.is_some_and(|value| self.classes.contains(&class(&value))));
        }
        Ok(false)
    }
    pub fn tiled(&mut self, identity: Option<&Identity>, window: u32) -> bool {
        let Some((classes, title)) = identity else {
            return false;
        };
        let mut matches: BTreeSet<u64> = self
            .windows
            .iter()
            .filter(|(_, value)| {
                classes.contains(&class(value)) && value["title"].as_str() == Some(title)
            })
            .map(|(id, _)| *id)
            .collect();
        if let Some(previous) = self.mapping.get(&window) {
            matches.extend(previous.iter().filter(|id| {
                self.windows
                    .get(id)
                    .is_some_and(|value| classes.contains(&class(value)))
            }));
        }
        if !matches.is_empty() {
            self.mapping.insert(window, matches.clone());
        }
        !matches.is_empty()
            && matches
                .iter()
                .all(|id| self.windows[id]["is_floating"] == false)
    }
    pub fn restore(&mut self, window: u32, x11: &mut impl X11) -> Result<()> {
        let previous = self.saved.remove(&window);
        let hints = x11.hints(window)?;
        if let (Some(previous), Some(mut hints)) = (previous, hints) {
            if hints.flags & MIN_SIZE != 0 && hints.min_width == 1 {
                hints.min_width = if hints.flags & MAX_SIZE != 0 && hints.max_width > 0 {
                    previous.min(hints.max_width)
                } else {
                    previous
                };
                x11.set_hints(window, &hints)?;
            }
        }
        Ok(())
    }
    pub fn reconcile(&mut self, x11: &mut impl X11, now: f64) -> Result<()> {
        let present: BTreeSet<u32> = x11.windows()?.into_iter().collect();
        let mut identities = BTreeMap::new();
        for window in &present {
            identities.insert(*window, x11.identity(*window)?);
        }
        self.classes = identities
            .values()
            .flatten()
            .flat_map(|(classes, _)| classes.iter().cloned())
            .collect();
        self.eligible.clear();
        self.saved.retain(|window, _| present.contains(window));
        self.mapping.retain(|window, _| present.contains(window));
        for window in present {
            if !x11.ordinary(window)? || !self.tiled(identities[&window].as_ref(), window) {
                if self.saved.contains_key(&window) {
                    self.restore(window, x11)?;
                }
                continue;
            }
            let Some(mut hints) = x11
                .hints(window)?
                .filter(|hints| hints.flags & MIN_SIZE != 0)
            else {
                self.saved.remove(&window);
                continue;
            };
            if hints.flags & MAX_SIZE != 0 && hints.min_width == hints.max_width {
                if self.saved.contains_key(&window) {
                    self.restore(window, x11)?;
                }
                continue;
            }
            if let Some(mapping) = self.mapping.get(&window) {
                self.eligible.extend(mapping);
            }
            if hints.min_width > 1 {
                self.saved.insert(window, hints.min_width);
                hints.min_width = 1;
                x11.set_hints(window, &hints)?;
                if let Some(mapping) = self.mapping.get(&window) {
                    for id in mapping.intersection(&self.new) {
                        self.opening.insert(*id, now + 0.15);
                    }
                    for id in mapping {
                        self.new.remove(id);
                    }
                }
            }
        }
        Ok(())
    }
    pub fn apply_openings(
        &mut self,
        now: f64,
        mut resize: impl FnMut(u64) -> Result<()>,
    ) -> Result<()> {
        let ready: Vec<u64> = self
            .opening
            .iter()
            .filter(|(_, deadline)| **deadline <= now)
            .map(|(id, _)| *id)
            .collect();
        for id in ready {
            self.opening.remove(&id);
            if self.eligible.contains(&id)
                && self
                    .windows
                    .get(&id)
                    .is_some_and(|window| window["is_floating"] == false)
            {
                resize(id)?;
            }
        }
        Ok(())
    }
    pub fn close(&mut self, x11: &mut impl X11) -> Result<()> {
        let windows: Vec<u32> = self.saved.keys().copied().collect();
        for window in windows {
            self.restore(window, x11)?;
        }
        Ok(())
    }
    pub fn snapshot(&self) -> Result<Value> {
        Ok(serde_json::to_value(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct Fixture {
        value: SizeHints,
        present: Vec<u32>,
        identity: Option<Identity>,
        ordinary: bool,
        available: bool,
        calls: Vec<Value>,
    }
    impl X11 for Fixture {
        fn windows(&mut self) -> Result<Vec<u32>> {
            self.calls.push(json!(["windows"]));
            Ok(self.present.clone())
        }
        fn identity(&mut self, window: u32) -> Result<Option<Identity>> {
            self.calls.push(json!(["identity", window]));
            Ok(self.identity.clone())
        }
        fn ordinary(&mut self, window: u32) -> Result<bool> {
            self.calls.push(json!(["ordinary", window]));
            Ok(self.ordinary)
        }
        fn hints(&mut self, window: u32) -> Result<Option<SizeHints>> {
            self.calls.push(json!(["hints", window]));
            Ok(self.available.then(|| self.value.clone()))
        }
        fn set_hints(&mut self, window: u32, hints: &SizeHints) -> Result<()> {
            self.value = hints.clone();
            self.calls.push(json!(["set_hints", window, hints]));
            Ok(())
        }
    }
    #[test]
    fn frozen_current_policy_all_intermediate_outputs_and_constraints() -> Result<()> {
        let reference: Value =
            serde_json::from_str(include_str!("../tests/fixtures/niri-widths-reference.json"))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let mut fixture = Fixture {
                value: SizeHints {
                    flags: 48,
                    min_width: 1600,
                    min_height: 600,
                    max_width: 4000,
                    max_height: 2000,
                    width_inc: 10,
                    height_inc: 20,
                    base_width: 30,
                    base_height: 40,
                    min_aspect: Pair { x: 4, y: 3 },
                    max_aspect: Pair { x: 16, y: 9 },
                    win_gravity: 1,
                    ..SizeHints::default()
                },
                present: vec![10],
                identity: Some((BTreeSet::from(["fixture".into()]), "Window".into())),
                ordinary: true,
                available: true,
                calls: vec![],
            };
            let mut policy = Policy::default();
            let mut resizes = vec![];
            for (index, step) in case["request"]["steps"]
                .as_array()
                .ok_or("Missing steps")?
                .iter()
                .enumerate()
            {
                if let Some(values) = step["hints"].as_object() {
                    let mut hints = serde_json::to_value(&fixture.value)?;
                    for (key, value) in values {
                        hints[key] = value.clone();
                    }
                    fixture.value = serde_json::from_value(hints)?;
                }
                if let Some(value) = step["ordinary"].as_bool() {
                    fixture.ordinary = value;
                }
                if let Some(value) = step["hints_available"].as_bool() {
                    fixture.available = value;
                }
                if let Some(value) = step.get("present") {
                    fixture.present = serde_json::from_value(value.clone())?;
                }
                if let Some(value) = step.get("identity") {
                    fixture.identity = serde_json::from_value(value.clone())?;
                }
                let mut event_result = Value::Null;
                if let Some(event) = step.get("event") {
                    event_result = json!(policy.event(event)?);
                }
                if step["reconcile"] == true {
                    policy.reconcile(&mut fixture, 100.0)?;
                }
                if let Some(now) = step["apply"].as_f64() {
                    policy.apply_openings(now, |id| {
                        resizes.push(id);
                        Ok(())
                    })?;
                }
                if step["close"] == true {
                    policy.close(&mut fixture)?;
                }
                let mut observed = policy.snapshot()?;
                observed["event_result"] = event_result;
                observed["hints"] = serde_json::to_value(&fixture.value)?;
                observed["calls"] = json!(fixture.calls);
                observed["resizes"] = json!(resizes);
                assert_eq!(
                    observed, case["observed"]["steps"][index],
                    "{} step {index}",
                    case["request"]["name"]
                );
            }
        }
        Ok(())
    }
}
