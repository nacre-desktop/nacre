//! Pure state projection around Bead's notification `Center`.
//!
//! This module owns no bus connections, settings writers, or storage handles.
use bead_services::notifications::{Center, Notice};
use nacre_protocol::{
    core_services::{
        CoreCall, Entities, EntityId, Health, HistoryStorage, Notification, NotificationAction,
        NotificationsState,
    },
    ErrorCode, Failure, MAX_SAFE_INTEGER,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const ACTIVE_COUNT: usize = 128;
const HISTORY_ROWS: usize = 10_000;
const HISTORY_BYTES: usize = 16 * 1024 * 1024;
const HISTORY_WINDOW_ROWS: usize = 64;
const HISTORY_WINDOW_BYTES: usize = 256 * 1024;
const HISTORY_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

#[derive(Clone, Debug)]
pub struct Policy {
    pub expire: bool,
    pub default_timeout_ms: u32,
    pub feedback_apps: BTreeSet<String>,
    pub feedback_summaries: BTreeSet<String>,
    pub feedback_patterns: Vec<String>,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            expire: true,
            default_timeout_ms: 5_000,
            feedback_apps: BTreeSet::new(),
            feedback_summaries: BTreeSet::new(),
            feedback_patterns: Vec::new(),
        }
    }
}

/// Native input after image resolution. `sender` and action keys remain private.
#[derive(Clone)]
pub struct Input {
    pub sender: String,
    pub replaces: u32,
    pub notice: Notice,
    pub timeout: i32,
    pub desktop_entry: Option<String>,
    pub image_path: Option<String>,
}

#[derive(Clone, PartialEq, Eq)]
pub enum NativeSignal {
    ActionInvoked {
        destination: String,
        native_id: u32,
        key: String,
    },
    NotificationClosed {
        destination: String,
        native_id: u32,
        reason: u32,
    },
}

#[derive(Clone, Default)]
pub struct Outcome {
    /// Native ID/cookie for the bus adapter only; never place it in protocol state.
    pub native_id: Option<u32>,
    pub changed: bool,
    /// Whether retained display history must be flushed to storage.
    pub persist: bool,
    pub signals: Vec<NativeSignal>,
    /// Event name and text-free payload. The Hub assigns `state_seq`.
    pub events: Vec<(String, Value)>,
}

#[derive(Clone)]
struct Active {
    id: EntityId,
    revision: u64,
    arrival: u64,
    created_ms: u64,
    timeout_ms: u32,
    sender: String,
    desktop_entry: Option<String>,
    image_path: Option<String>,
    retained: bool,
    actions: Vec<(EntityId, String)>,
    hoverers: HashSet<u64>,
    popup_deadline: Option<Instant>,
    popup_remaining: Option<Duration>,
}

#[derive(Clone)]
struct Closed {
    id: EntityId,
    notification: Notification,
    arrival: u64,
}

pub struct Model {
    boot: String,
    generation: u64,
    next_id: u64,
    arrival: u64,
    center: Center,
    active: BTreeMap<u32, Active>,
    history: Vec<Closed>,
    policy: Policy,
    ui_hover: HashMap<u64, HashSet<u32>>,
    ui_inhibitors: HashSet<u64>,
    native_inhibitors: HashMap<u32, String>,
    last_wall_ms: u64,
    last_now: Instant,
}

impl Model {
    pub fn new(boot: &str, generation: u64, rows: Vec<Value>, dnd: bool, policy: Policy) -> Self {
        let now_ms = system_now_ms();
        let mut model = Self {
            boot: boot.to_owned(),
            generation,
            next_id: 0,
            arrival: 0,
            center: Center::default(),
            active: BTreeMap::new(),
            history: Vec::new(),
            policy,
            ui_hover: HashMap::new(),
            ui_inhibitors: HashSet::new(),
            native_inhibitors: HashMap::new(),
            last_wall_ms: now_ms,
            last_now: Instant::now(),
        };
        model.center.set_dnd(dnd);
        for row in rows {
            if let Some(closed) = model.restore_row(&row) {
                model.history.push(closed);
            }
        }
        model.history.sort_by_key(|row| row.arrival);
        model.prune_history(now_ms);
        model
    }

    pub fn notify(&mut self, input: Input, now: Instant, wall_ms: u64) -> Result<Outcome, Failure> {
        self.last_wall_ms = wall_ms;
        self.last_now = now;
        if input.sender.is_empty()
            || input.sender.len() > 255
            || input.sender.contains('\0')
            || input.timeout < -1
            || !valid_icon(&input.notice.icon)
            || input
                .desktop_entry
                .as_deref()
                .is_some_and(|value| value.len() > 1024 || value.contains('\0'))
            || input.image_path.as_deref().is_some_and(|value| {
                value.len() > 4096 || !value.starts_with('/') || value.contains('\0')
            })
        {
            return Err(failure(
                ErrorCode::Validation,
                "Invalid notification",
                false,
            ));
        }
        let replacing = (input.replaces != 0)
            .then_some(input.replaces)
            .filter(|native| {
                self.active
                    .get(native)
                    .is_some_and(|active| active.sender == input.sender)
            });
        if replacing.is_none() && self.active.len() >= ACTIVE_COUNT {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification limit reached",
                false,
            ));
        }
        let resolved_timeout = if input.timeout == -1 {
            if self.policy.default_timeout_ms > i32::MAX as u32 {
                return Err(failure(
                    ErrorCode::Validation,
                    "Invalid notification timeout policy",
                    false,
                ));
            }
            self.policy.default_timeout_ms
        } else {
            input.timeout as u32
        };
        let id = match replacing
            .and_then(|native| self.active.get(&native).map(|active| active.id.clone()))
        {
            Some(id) => id,
            None => self.peek_id(1)?,
        };
        let revision = replacing
            .and_then(|native| {
                self.active
                    .get(&native)
                    .map(|active| active.revision.saturating_add(1))
            })
            .unwrap_or(1);
        if revision == 0 || revision > MAX_SAFE_INTEGER {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification revision exhausted",
                false,
            ));
        }
        let retained = self.should_retain(&input.notice);
        let action_offset = if replacing.is_some() { 1 } else { 2 };
        let action_ids = self.action_ids(&input.notice, action_offset)?;
        let mut native_notice = input.notice.clone();
        native_notice.deadline = None;
        native_notice.remaining = None;
        native_notice.hovered = false;
        native_notice.popup = false;
        native_notice.expanded = false;
        native_notice.timeout_ms = resolved_timeout as u64;
        let candidate = active_notification(
            &native_notice,
            revision,
            wall_ms,
            wall_ms,
            now,
            &action_ids,
            input.image_path.clone(),
        );
        let mut candidate = candidate;
        candidate.timeout_ms = Some(resolved_timeout as u64);
        candidate.retained = Some(retained);
        candidate.desktop_entry = input.desktop_entry.clone();
        self.check_candidate(replacing, &id, candidate)?;

        // The underlying Center owns its deadline and native ID allocation.
        let center_timeout = if self.policy.expire {
            resolved_timeout as i32
        } else {
            0
        };
        let native_id = self
            .center
            .notify(native_notice, replacing.unwrap_or(0), center_timeout, now)
            .map_err(|_| {
                failure(
                    ErrorCode::TooLarge,
                    "Notification could not be accepted",
                    false,
                )
            })?;
        if self.suppressed() {
            self.center.hide_popups();
        }
        if self
            .center
            .active
            .get(&native_id)
            .is_some_and(|notice| notice.hovered)
        {
            let _ = self.center.hover(native_id, false, now);
        }
        let old = replacing.and_then(|native| self.active.remove(&native));
        let was_replacement = old.is_some();
        let should_persist = retained || old.as_ref().is_some_and(|entry| entry.retained);
        if !was_replacement {
            self.next_id += 1;
        }
        self.next_id += action_ids.len() as u64;
        let arrival = old.as_ref().map_or_else(
            || {
                self.arrival = self.arrival.saturating_add(1);
                self.arrival
            },
            |entry| entry.arrival,
        );
        let revision = old.as_ref().map_or(1, |entry| entry.revision + 1);
        let actions = action_ids;
        let popup_deadline = if self.policy.expire && !self.suppressed() {
            popup_duration(input.timeout, resolved_timeout).map(|duration| now + duration)
        } else {
            None
        };
        self.active.insert(
            native_id,
            Active {
                id: old.map_or(id.clone(), |entry| entry.id),
                revision,
                arrival,
                created_ms: wall_ms,
                timeout_ms: resolved_timeout,
                sender: input.sender,
                desktop_entry: input.desktop_entry,
                image_path: input.image_path,
                retained,
                actions,
                hoverers: HashSet::new(),
                popup_deadline,
                popup_remaining: None,
            },
        );
        self.ui_hover.retain(|_, ids| {
            ids.remove(&native_id);
            !ids.is_empty()
        });
        let history_pruned = self.prune_history(wall_ms);
        let (name, payload) = if was_replacement {
            ("updated", event_revision(id, revision))
        } else {
            ("shown", event_revision(id, revision))
        };
        Ok(Outcome {
            native_id: Some(native_id),
            changed: true,
            persist: should_persist || history_pruned,
            signals: Vec::new(),
            events: if history_pruned {
                vec![
                    (name.into(), payload),
                    ("history_changed".into(), json!({})),
                ]
            } else {
                vec![(name.into(), payload)]
            },
        })
    }

    /// Apply a settings-writer-acknowledged DND value.
    pub fn set_dnd(&mut self, enabled: bool, now: Instant, wall_ms: u64) -> Outcome {
        self.last_wall_ms = wall_ms;
        self.last_now = now;
        if self.center.dnd == enabled {
            return Outcome::default();
        }
        let before = self.suppressed();
        self.center.set_dnd(enabled);
        self.clear_hover_on_suppression(now);
        let after = self.suppressed();
        let mut outcome = Outcome {
            changed: true,
            ..Outcome::default()
        };
        outcome
            .events
            .push(("dnd_changed".into(), json!({"enabled":enabled})));
        if before != after {
            outcome
                .events
                .push(("presentation_changed".into(), json!({})));
        }
        outcome
    }

    pub fn control(
        &mut self,
        call: CoreCall,
        client: u64,
        now: Instant,
        wall_ms: u64,
    ) -> Result<Outcome, Failure> {
        self.last_wall_ms = wall_ms;
        self.last_now = now;
        match call {
            CoreCall::NotificationDnd(_) => Err(failure(
                ErrorCode::PermissionDenied,
                "DND is controlled by settings",
                false,
            )),
            CoreCall::NotificationDismiss(p) => {
                if p.forget {
                    if let Some(native) = self.native_id(&p.id) {
                        self.require_active(&p.id, p.revision)?;
                        Ok(self.close_active(native, 2, "dismissed", now, wall_ms, true, true))
                    } else if let Some(index) = self.history.iter().position(|row| {
                        row.id == p.id && row.notification.revision == Some(p.revision)
                    }) {
                        self.history.remove(index);
                        Ok(Outcome {
                            changed: true,
                            persist: true,
                            events: vec![("history_changed".into(), json!({}))],
                            ..Outcome::default()
                        })
                    } else {
                        Err(stale())
                    }
                } else {
                    let native = self.require_active(&p.id, p.revision)?;
                    Ok(self.close_active(native, 2, "dismissed", now, wall_ms, true, false))
                }
            }
            CoreCall::NotificationClear(p) => Ok(self.clear(p.scope, now, wall_ms)),
            CoreCall::NotificationAction(p) => {
                let native = self.require_active(&p.id, p.revision)?;
                let entry = self.active.get(&native).ok_or_else(stale)?;
                let destination = entry.sender.clone();
                let key = entry
                    .actions
                    .iter()
                    .find(|(id, _)| id == &p.action)
                    .map(|(_, key)| key.clone())
                    .ok_or_else(stale)?;
                let closes = self.center.action(native, &key).map_err(|_| stale())?;
                let mut outcome = Outcome {
                    changed: true,
                    signals: vec![NativeSignal::ActionInvoked {
                        destination,
                        native_id: native,
                        key,
                    }],
                    events: vec![(
                        "action".into(),
                        json!({"id":p.id,"revision":p.revision,"action":p.action}),
                    )],
                    ..Outcome::default()
                };
                if closes {
                    outcome.merge(self.close_active(
                        native,
                        2,
                        "dismissed",
                        now,
                        wall_ms,
                        true,
                        false,
                    ));
                }
                Ok(outcome)
            }
            CoreCall::NotificationHover(p) => {
                let native = self.require_active(&p.id, p.revision)?;
                Ok(self.set_hover(client, native, p.hovered, now, wall_ms))
            }
            CoreCall::NotificationInhibited(p) => {
                self.set_ui_inhibited(client, p.inhibited, now, wall_ms)
            }
            _ => Err(failure(
                ErrorCode::UnknownMethod,
                "Unknown notification method",
                false,
            )),
        }
    }

    pub fn close_native(
        &mut self,
        native_id: u32,
        native_reason: u32,
        now: Instant,
        wall_ms: u64,
    ) -> Outcome {
        self.last_wall_ms = wall_ms;
        self.last_now = now;
        let (reason, name) = match native_reason {
            1 => (1, "expired"),
            2 => (2, "dismissed"),
            3 => (3, "application"),
            _ => (4, "other"),
        };
        self.close_active(native_id, reason, name, now, wall_ms, true, false)
    }

    pub fn inhibit_native(
        &mut self,
        owner: String,
        reason: String,
        now: Instant,
        wall_ms: u64,
    ) -> Result<Outcome, Failure> {
        self.last_wall_ms = wall_ms;
        self.last_now = now;
        if owner.is_empty()
            || owner.len() > 255
            || owner.contains('\0')
            || reason.len() > 1024
            || reason.contains('\0')
        {
            return Err(failure(ErrorCode::Validation, "Invalid inhibition", false));
        }
        let before = self.suppressed();
        let cookie = self
            .center
            .inhibit(owner.clone(), reason)
            .map_err(|_| failure(ErrorCode::TooLarge, "Inhibition limit reached", false))?;
        self.native_inhibitors.insert(cookie, owner);
        self.clear_hover_on_suppression(now);
        let changed = !before && self.suppressed();
        Ok(Outcome {
            native_id: Some(cookie),
            changed: true,
            events: if changed {
                vec![("presentation_changed".into(), json!({}))]
            } else {
                Vec::new()
            },
            ..Outcome::default()
        })
    }

    pub fn uninhibit_native(
        &mut self,
        owner: &str,
        cookie: u32,
        now: Instant,
        wall_ms: u64,
    ) -> Result<Outcome, Failure> {
        self.last_wall_ms = wall_ms;
        self.last_now = now;
        if self.native_inhibitors.get(&cookie).map(String::as_str) != Some(owner) {
            return Err(failure(
                ErrorCode::PermissionDenied,
                "Unknown inhibition",
                false,
            ));
        }
        let before = self.suppressed();
        self.center
            .uninhibit(owner, cookie)
            .map_err(|_| failure(ErrorCode::PermissionDenied, "Unknown inhibition", false))?;
        self.native_inhibitors.remove(&cookie);
        let changed = before != self.suppressed();
        Ok(Outcome {
            changed: true,
            events: if changed {
                vec![("presentation_changed".into(), json!({}))]
            } else {
                Vec::new()
            },
            ..Outcome::default()
        })
    }

    pub fn owner_left(&mut self, owner: &str, now: Instant, wall_ms: u64) -> Outcome {
        self.last_wall_ms = wall_ms;
        self.last_now = now;
        let before = self.suppressed();
        self.center.owner_left(owner);
        self.native_inhibitors.retain(|_, source| source != owner);
        self.clear_hover_on_suppression(now);
        let changed = before != self.suppressed();
        Outcome {
            changed,
            events: if changed {
                vec![("presentation_changed".into(), json!({}))]
            } else {
                Vec::new()
            },
            ..Outcome::default()
        }
    }

    pub fn disconnect(&mut self, client: u64, now: Instant, wall_ms: u64) -> Outcome {
        self.last_wall_ms = wall_ms;
        self.last_now = now;
        let before = self.suppressed();
        let mut outcome = Outcome::default();
        if let Some(ids) = self.ui_hover.remove(&client) {
            for native in ids {
                if let Some(entry) = self.active.get_mut(&native) {
                    entry.hoverers.remove(&client);
                    if entry.hoverers.is_empty() {
                        if let Ok(()) = self.center.hover(native, false, now) {
                            entry.popup_deadline = entry.popup_remaining.take().map(|d| now + d);
                            outcome.changed = true;
                        }
                    }
                }
            }
        }
        let removed_inhibitor = self.ui_inhibitors.remove(&client);
        let effective_changed = before != self.suppressed();
        if effective_changed || outcome.changed {
            outcome.changed = true;
            outcome
                .events
                .push(("presentation_changed".into(), json!({})));
        }
        outcome.changed |= removed_inhibitor;
        outcome
    }

    pub fn tick(&mut self, now: Instant, wall_ms: u64) -> Outcome {
        self.last_wall_ms = wall_ms;
        self.last_now = now;
        let mut outcome = Outcome::default();
        let expired = self.center.expire(now);
        for native in expired {
            let notice = self
                .center
                .history
                .iter()
                .find(|notice| notice.id == native)
                .cloned();
            self.center.history.retain(|notice| notice.id != native);
            self.center.exiting.remove(&native);
            outcome.merge(
                self.close_already_removed(native, notice, 1, "expired", wall_ms, now, true, false),
            );
        }
        if self.prune_history(wall_ms) {
            outcome.changed = true;
            outcome.persist = true;
            outcome.events.push(("history_changed".into(), json!({})));
        }
        let hidden: Vec<u32> = self
            .active
            .iter()
            .filter_map(|(native, entry)| {
                entry
                    .popup_deadline
                    .is_some_and(|deadline| deadline <= now)
                    .then_some(*native)
            })
            .collect();
        let has_hidden = !hidden.is_empty();
        for native in hidden {
            if let Some(entry) = self.active.get_mut(&native) {
                entry.popup_deadline = None;
                if let Some(notice) = self.center.active.get_mut(&native) {
                    notice.popup = false;
                }
                outcome.changed = true;
            }
        }
        if !outcome.events.is_empty() || has_hidden {
            outcome
                .events
                .push(("presentation_changed".into(), json!({})));
        }
        outcome
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        self.center
            .deadline()
            .into_iter()
            .chain(
                self.active
                    .values()
                    .filter_map(|entry| entry.popup_deadline),
            )
            .chain(self.history.iter().filter_map(|row| {
                let ttl_ms = HISTORY_TTL.as_millis().min(MAX_SAFE_INTEGER as u128) as u64;
                let until = row
                    .notification
                    .created_ms
                    .saturating_add(ttl_ms)
                    .saturating_sub(self.last_wall_ms);
                (until > 0).then(|| self.last_now + Duration::from_millis(until))
            }))
            .min()
    }

    pub fn snapshot(&self, storage: HistoryStorage) -> NotificationsState {
        let history_ready = storage.ready;
        let mut active_rows: Vec<_> = self.active.iter().collect();
        active_rows.sort_by_key(|(_, entry)| entry.arrival);
        let mut active_items = BTreeMap::new();
        let mut active_order = Vec::new();
        let mut popup_order = Vec::new();
        for (native, entry) in active_rows {
            if let Some(notice) = self.center.active.get(native) {
                let notification = project_active(entry, notice, self.last_wall_ms, self.last_now);
                if notice.popup {
                    popup_order.push(entry.id.clone());
                }
                active_order.push(entry.id.clone());
                active_items.insert(entry.id.clone(), notification);
            }
        }
        let mut history_items = BTreeMap::new();
        let mut history_order = Vec::new();
        for row in self.history.iter().rev() {
            if history_order.len() >= HISTORY_WINDOW_ROWS {
                break;
            }
            let mut candidate_order = history_order.clone();
            candidate_order.push(row.id.clone());
            let mut candidate_items = history_items.clone();
            candidate_items.insert(row.id.clone(), row.notification.clone());
            let candidate = Entities {
                items: candidate_items,
                order: candidate_order,
            };
            let candidate_bytes =
                serde_json::to_vec(&candidate).map_or(usize::MAX, |bytes| bytes.len());
            if candidate_bytes > HISTORY_WINDOW_BYTES {
                break;
            }
            history_order = candidate.order;
            history_items = candidate.items;
        }
        let history_more = history_order.len() < self.history.len();
        NotificationsState {
            health: Health {
                available: true,
                stale: false,
                error: None,
            },
            do_not_disturb: self.center.dnd,
            active: Entities {
                items: active_items,
                order: active_order,
            },
            history: Entities {
                items: history_items,
                order: history_order,
            },
            popup_order: Some(popup_order),
            inhibited: Some(self.suppressed()),
            history_count: storage.ready.then_some(
                self.history.len() as u64
                    + self
                        .active
                        .iter()
                        .filter(|(native, _)| {
                            self.center
                                .active
                                .get(*native)
                                .is_some_and(|notice| !notice.transient)
                        })
                        .count() as u64,
            ),
            history_storage: Some(storage),
            history_more: history_ready.then_some(history_more),
        }
    }

    pub fn persisted_rows(&self) -> Result<Value, Failure> {
        let mut active_rows = Vec::new();
        // Reserve room for the existing store envelope and trailing newline.
        let mut bytes = 1024usize;
        for (native, entry) in &self.active {
            if !entry.retained
                || entry.created_ms < self.last_wall_ms.saturating_sub(30 * 24 * 60 * 60 * 1000)
            {
                continue;
            }
            let (Some(notice), Some(notification)) = (
                self.center.active.get(native),
                self.active_notification(*native),
            ) else {
                continue;
            };
            if notice.transient {
                continue;
            }
            let value = persisted_row(&entry.id, &notification);
            let size = persisted_size(&value)?;
            if active_rows.len() >= HISTORY_ROWS || bytes.saturating_add(size) > HISTORY_BYTES {
                return Err(failure(
                    ErrorCode::Io,
                    "History storage limit reached",
                    true,
                ));
            }
            bytes += size;
            active_rows.push((entry.arrival, value));
        }
        active_rows.sort_by_key(|(arrival, _)| *arrival);
        let mut closed = Vec::new();
        for row in self.history.iter().rev() {
            if !self.retained_history(&row.notification) {
                continue;
            }
            let value = persisted_row(&row.id, &row.notification);
            let size = persisted_size(&value)?;
            if active_rows.len() + closed.len() >= HISTORY_ROWS
                || bytes.saturating_add(size) > HISTORY_BYTES
            {
                break;
            }
            bytes += size;
            closed.push(value);
        }
        closed.reverse();
        let mut rows = closed;
        rows.extend(active_rows.into_iter().map(|(_, row)| row));
        Ok(Value::Array(rows))
    }

    pub fn closed_count(&self) -> usize {
        self.history.len()
    }

    pub fn image_references(&self) -> Vec<String> {
        let mut paths = BTreeSet::new();
        for entry in self.active.values() {
            if let Some(path) = &entry.image_path {
                paths.insert(path.clone());
            }
        }
        for notice in self.center.active.values() {
            if notice.icon.starts_with('/') {
                paths.insert(notice.icon.clone());
            }
        }
        for row in &self.history {
            if let Some(path) = &row.notification.image_path {
                paths.insert(path.clone());
            }
            if let Some(path) = row
                .notification
                .icon
                .as_deref()
                .filter(|icon| icon.starts_with('/'))
            {
                paths.insert(path.to_owned());
            }
        }
        paths.into_iter().collect()
    }

    fn clear(
        &mut self,
        scope: nacre_protocol::core_services::NotificationClearScope,
        now: Instant,
        wall_ms: u64,
    ) -> Outcome {
        use nacre_protocol::core_services::NotificationClearScope as Scope;
        match scope {
            Scope::History => {
                if self.history.is_empty() {
                    return Outcome::default();
                }
                self.history.clear();
                Outcome {
                    changed: true,
                    persist: true,
                    events: vec![("history_changed".into(), json!({}))],
                    ..Outcome::default()
                }
            }
            Scope::Popups => {
                let targets: Vec<u32> = self
                    .active
                    .keys()
                    .filter_map(|native| {
                        self.center
                            .active
                            .get(native)
                            .is_some_and(|n| n.popup)
                            .then_some(*native)
                    })
                    .collect();
                if targets.is_empty() {
                    return Outcome::default();
                }
                self.center.hide_popups();
                for native in targets {
                    self.clear_entry_hover(native, now);
                    if let Some(entry) = self.active.get_mut(&native) {
                        entry.popup_deadline = None;
                    }
                }
                Outcome {
                    changed: true,
                    events: vec![("presentation_changed".into(), json!({}))],
                    ..Outcome::default()
                }
            }
            Scope::Retained => {
                let targets: Vec<u32> = self
                    .active
                    .iter()
                    .filter_map(|(native, entry)| entry.retained.then_some(*native))
                    .collect();
                let had_history = !self.history.is_empty();
                let had_retained_active = !targets.is_empty();
                let mut result = Outcome::default();
                for native in targets {
                    result.merge(self.close_active(
                        native,
                        2,
                        "dismissed",
                        now,
                        wall_ms,
                        true,
                        true,
                    ));
                }
                if had_history {
                    self.history.clear();
                    result.changed = true;
                }
                result.persist |= had_history || had_retained_active;
                if result.changed {
                    result.events.push(("history_changed".into(), json!({})));
                }
                result
            }
        }
    }

    fn set_hover(
        &mut self,
        client: u64,
        native: u32,
        hovered: bool,
        now: Instant,
        _wall_ms: u64,
    ) -> Outcome {
        if hovered && self.suppressed() {
            return Outcome::default();
        }
        let Some(entry) = self.active.get_mut(&native) else {
            return Outcome::default();
        };
        let changed = if hovered {
            if !entry.hoverers.insert(client) {
                false
            } else {
                self.ui_hover.entry(client).or_default().insert(native);
                if entry.hoverers.len() == 1 {
                    entry.popup_remaining = entry
                        .popup_deadline
                        .take()
                        .map(|deadline| deadline.saturating_duration_since(now));
                    self.center.hover(native, true, now).is_ok()
                } else {
                    true
                }
            }
        } else if entry.hoverers.remove(&client) {
            if let Some(ids) = self.ui_hover.get_mut(&client) {
                ids.remove(&native);
            }
            if entry.hoverers.is_empty() {
                entry.popup_deadline = entry.popup_remaining.take().map(|d| now + d);
                self.center.hover(native, false, now).is_ok()
            } else {
                true
            }
        } else {
            false
        };
        Outcome {
            changed,
            events: if changed {
                vec![("presentation_changed".into(), json!({}))]
            } else {
                Vec::new()
            },
            ..Outcome::default()
        }
    }

    fn set_ui_inhibited(
        &mut self,
        client: u64,
        inhibited: bool,
        now: Instant,
        _wall_ms: u64,
    ) -> Result<Outcome, Failure> {
        let before = self.suppressed();
        if inhibited {
            if !self.ui_inhibitors.insert(client) {
                return Ok(Outcome::default());
            }
            self.center.hide_popups();
            self.clear_hover_on_suppression(now);
        } else if !self.ui_inhibitors.remove(&client) {
            return Ok(Outcome::default());
        }
        let suppression_changed = before != self.suppressed();
        Ok(Outcome {
            changed: true,
            events: if suppression_changed {
                vec![("presentation_changed".into(), json!({}))]
            } else {
                Vec::new()
            },
            ..Outcome::default()
        })
    }

    fn clear_hover_on_suppression(&mut self, now: Instant) {
        if !self.suppressed() {
            return;
        }
        for (native, entry) in &mut self.active {
            if !entry.hoverers.is_empty() {
                let _ = self.center.hover(*native, false, now);
                entry.hoverers.clear();
                entry.popup_remaining = None;
            }
            entry.popup_deadline = None;
        }
        self.ui_hover.clear();
    }

    fn clear_entry_hover(&mut self, native: u32, now: Instant) {
        let Some(entry) = self.active.get_mut(&native) else {
            return;
        };
        if entry.hoverers.is_empty() {
            return;
        }
        for client in entry.hoverers.drain() {
            if let Some(ids) = self.ui_hover.get_mut(&client) {
                ids.remove(&native);
            }
        }
        let _ = self.center.hover(native, false, now);
        entry.popup_remaining = None;
    }

    #[allow(clippy::too_many_arguments)] // Native/public close reasons and retention policy.
    fn close_active(
        &mut self,
        native: u32,
        signal_reason: u32,
        reason: &str,
        now: Instant,
        wall_ms: u64,
        send_signal: bool,
        forget: bool,
    ) -> Outcome {
        if !self.active.contains_key(&native) {
            return Outcome::default();
        }
        let notice = self.center.close(native).ok();
        self.center.history.retain(|notice| notice.id != native);
        self.center.exiting.remove(&native);
        self.close_already_removed(
            native,
            notice,
            signal_reason,
            reason,
            wall_ms,
            now,
            send_signal,
            forget,
        )
    }

    #[allow(clippy::too_many_arguments)] // Shares the explicit close policy above.
    fn close_already_removed(
        &mut self,
        native: u32,
        notice: Option<Notice>,
        reason_code: u32,
        reason: &str,
        wall_ms: u64,
        now: Instant,
        send_signal: bool,
        forget: bool,
    ) -> Outcome {
        let Some(entry) = self.active.remove(&native) else {
            return Outcome::default();
        };
        self.ui_hover.retain(|_, ids| {
            ids.remove(&native);
            !ids.is_empty()
        });
        let notification = notice
            .as_ref()
            .map(|notice| project_active(&entry, notice, wall_ms, now));
        let mut history_changed = false;
        if !forget && entry.retained && notice.as_ref().is_some_and(|n| !n.transient) {
            self.history.retain(|row| row.id != entry.id);
            self.arrival = self.arrival.saturating_add(1);
            if let Some(mut notification) = notification {
                notification.actions = Entities {
                    items: BTreeMap::new(),
                    order: Vec::new(),
                };
                notification.default_action = None;
                notification.remaining_ms = None;
                notification.expires_ms = None;
                notification.paused = Some(false);
                notification.image_path = None;
                self.history.push(Closed {
                    id: entry.id.clone(),
                    notification,
                    arrival: self.arrival,
                });
                history_changed = true;
            }
            if history_changed {
                self.prune_history(wall_ms);
            }
        }
        let mut outcome = Outcome {
            changed: true,
            events: vec![(
                "closed".into(),
                json!({"id":entry.id.clone(),"reason":reason}),
            )],
            ..Outcome::default()
        };
        if history_changed {
            outcome.events.push(("history_changed".into(), json!({})));
            outcome.persist = true;
        } else if forget && entry.retained {
            outcome.persist = true;
        }
        if send_signal {
            outcome.signals.push(NativeSignal::NotificationClosed {
                destination: entry.sender.clone(),
                native_id: native,
                reason: reason_code,
            });
        }
        outcome
    }

    fn check_candidate(
        &self,
        replacing: Option<u32>,
        id: &EntityId,
        candidate: Notification,
    ) -> Result<(), Failure> {
        let mut state = self.snapshot(HistoryStorage {
            ready: true,
            writable: true,
            error: None,
        });
        if replacing.is_none() {
            state.active.order.push(id.clone());
        }
        state.active.items.insert(id.clone(), candidate);
        state.popup_order = Some(Vec::new());
        state.history_count = Some(
            self.history.len() as u64
                + state
                    .active
                    .items
                    .values()
                    .filter(|notice| !notice.transient)
                    .count() as u64,
        );
        if state.producer_valid() {
            Ok(())
        } else {
            Err(failure(
                ErrorCode::TooLarge,
                "Notification exceeds state limits",
                false,
            ))
        }
    }

    fn require_active(&self, id: &EntityId, revision: u64) -> Result<u32, Failure> {
        let native = self.native_id(id).ok_or_else(stale)?;
        if self
            .active
            .get(&native)
            .is_some_and(|entry| entry.revision == revision)
        {
            Ok(native)
        } else {
            Err(stale())
        }
    }

    fn native_id(&self, id: &EntityId) -> Option<u32> {
        self.active
            .iter()
            .find_map(|(native, entry)| (&entry.id == id).then_some(*native))
    }

    fn action_ids(&self, notice: &Notice, offset: u64) -> Result<Vec<(EntityId, String)>, Failure> {
        notice
            .actions
            .iter()
            .enumerate()
            .map(|(index, (key, _))| {
                let id = self.peek_id(offset.saturating_add(index as u64))?;
                Ok((id, key.clone()))
            })
            .collect()
    }

    fn peek_id(&self, offset: u64) -> Result<EntityId, Failure> {
        let value = self
            .next_id
            .checked_add(offset)
            .ok_or_else(|| failure(ErrorCode::TooLarge, "Notification IDs exhausted", false))?;
        let id = EntityId(format!("ntf:{}:{}:{}", self.boot, self.generation, value));
        if id.valid() {
            Ok(id)
        } else {
            Err(failure(
                ErrorCode::TooLarge,
                "Notification IDs exhausted",
                false,
            ))
        }
    }

    fn should_retain(&self, notice: &Notice) -> bool {
        if notice.transient {
            return false;
        }
        let app = notice.app.to_lowercase();
        if !self
            .policy
            .feedback_apps
            .iter()
            .any(|candidate| candidate == &app)
        {
            return true;
        }
        if self.policy.feedback_summaries.contains(&notice.summary) {
            return false;
        }
        !self
            .policy
            .feedback_patterns
            .iter()
            .filter_map(|pattern| regex::Regex::new(pattern).ok())
            .any(|pattern| {
                pattern.is_match(&notice.summary)
                    || notice
                        .summary
                        .strip_suffix('\n')
                        .is_some_and(|value| pattern.is_match(value))
            })
    }

    fn active_notification(&self, native: u32) -> Option<Notification> {
        let entry = self.active.get(&native)?;
        let notice = self.center.active.get(&native)?;
        Some(project_active(
            entry,
            notice,
            self.last_wall_ms,
            self.last_now,
        ))
    }

    fn suppressed(&self) -> bool {
        self.center.suppressed() || !self.ui_inhibitors.is_empty()
    }

    fn restore_row(&mut self, row: &Value) -> Option<Closed> {
        let notification = restored_notification(row)?;
        let candidate = Notice {
            id: 0,
            app: notification.app_name.clone(),
            icon: String::new(),
            summary: notification.summary.clone(),
            body: notification.body.clone(),
            actions: Vec::new(),
            urgency: urgency_code(&notification.urgency),
            transient: false,
            resident: false,
            popup: false,
            expanded: false,
            timeout_ms: 0,
            time_unix_ms: 0,
            deadline: None,
            remaining: None,
            hovered: false,
        };
        if !self.should_retain(&candidate) {
            return None;
        }
        if self.last_wall_ms.saturating_sub(notification.created_ms)
            >= HISTORY_TTL.as_millis() as u64
        {
            return None;
        }
        let id = self.peek_id(1).ok()?;
        self.next_id = self.next_id.saturating_add(1);
        self.arrival = self.arrival.saturating_add(1);
        Some(Closed {
            id,
            notification,
            arrival: self.arrival,
        })
    }

    fn prune_history(&mut self, wall_ms: u64) -> bool {
        let before = self.history.len();
        let cutoff = wall_ms.saturating_sub(HISTORY_TTL.as_millis() as u64);
        self.history
            .retain(|row| row.notification.created_ms > cutoff);
        // Only rows written to storage consume its count and byte quota. The
        // public history_count intentionally also includes unretained feedback.
        let active_count = self
            .active
            .iter()
            .filter(|(native, entry)| {
                entry.retained
                    && entry.created_ms >= wall_ms.saturating_sub(HISTORY_TTL.as_millis() as u64)
                    && self
                        .center
                        .active
                        .get(*native)
                        .is_some_and(|notice| !notice.transient)
            })
            .count();
        let mut used_bytes = 1024usize;
        for (native, entry) in &self.active {
            if !entry.retained
                || entry.created_ms < wall_ms.saturating_sub(HISTORY_TTL.as_millis() as u64)
                || !self
                    .center
                    .active
                    .get(native)
                    .is_some_and(|notice| !notice.transient)
            {
                continue;
            }
            if let Some(notification) = self.active_notification(*native) {
                let row = persisted_row(&entry.id, &notification);
                used_bytes = used_bytes.saturating_add(persisted_size(&row).unwrap_or(usize::MAX));
            }
        }
        let mut closed_count = 0usize;
        let mut keep_from = self.history.len();
        for (index, row) in self.history.iter().enumerate().rev() {
            if !self.retained_history(&row.notification) {
                keep_from = index;
                continue;
            }
            let value = persisted_row(&row.id, &row.notification);
            let size = persisted_size(&value).unwrap_or(usize::MAX);
            if active_count.saturating_add(closed_count + 1) > HISTORY_ROWS
                || used_bytes.saturating_add(size) > HISTORY_BYTES
            {
                break;
            }
            used_bytes += size;
            closed_count += 1;
            keep_from = index;
        }
        // Retain the contiguous newest closed prefix that fits the envelope,
        // row-count and byte limits. Never evict a live notification.
        self.history.drain(..keep_from);
        self.history.len() < before
    }

    fn retained_history(&self, notification: &Notification) -> bool {
        notification.created_ms
            > self
                .last_wall_ms
                .saturating_sub(HISTORY_TTL.as_millis() as u64)
            && !notification.transient
            && self.should_retain(&Notice {
                id: 0,
                app: notification.app_name.clone(),
                icon: notification.icon.clone().unwrap_or_default(),
                summary: notification.summary.clone(),
                body: notification.body.clone(),
                actions: Vec::new(),
                urgency: urgency_code(&notification.urgency),
                transient: notification.transient,
                resident: notification.resident,
                popup: false,
                expanded: false,
                timeout_ms: 0,
                time_unix_ms: notification.created_ms,
                deadline: None,
                remaining: None,
                hovered: false,
            })
    }
}

pub(super) fn history_supported(rows: &[Value]) -> bool {
    rows.iter().all(|row| restored_notification(row).is_some())
}

fn restored_notification(row: &Value) -> Option<Notification> {
    let text = |name: &str| -> Option<String> {
        match row.get(name) {
            None | Some(Value::Null) => Some(String::new()),
            Some(Value::String(value)) => Some(value.clone()),
            _ => None,
        }
    };
    let app = text("appName")?;
    let summary = text("summary")?;
    let body = text("body")?;
    let icon = text("appIcon")?;
    if app.len() > 8192
        || summary.len() > 8192
        || body.len() > 8192
        || icon.len() > 8192
        || [app.as_str(), summary.as_str(), body.as_str(), icon.as_str()]
            .iter()
            .any(|value| value.contains('\0'))
    {
        return None;
    }
    if row
        .get("transient")
        .is_some_and(|value| !value.is_boolean())
        || row.get("transient").and_then(Value::as_bool) == Some(true)
    {
        return None;
    }
    let urgency = match row.get("urgency") {
        None => 1,
        Some(value) => value.as_u64()?,
    };
    if urgency > 2 {
        return None;
    }
    let created_ms = row
        .get("time")
        .and_then(Value::as_str)
        .and_then(|time| chrono::DateTime::parse_from_rfc3339(time).ok())?
        .timestamp_millis()
        .max(0) as u64;
    if created_ms > MAX_SAFE_INTEGER {
        return None;
    }
    let notification = Notification {
        app_name: app,
        summary,
        body,
        urgency: match urgency {
            0 => "low",
            2 => "critical",
            _ => "normal",
        }
        .to_owned(),
        resident: false,
        transient: false,
        created_ms,
        icon: (!icon.is_empty() && safe_named_icon(&icon)).then_some(icon),
        // A cache entry or native image URL never survives a daemon restart.
        image_path: None,
        actions: Entities {
            items: BTreeMap::new(),
            order: Vec::new(),
        },
        default_action: None,
        timeout_ms: None,
        remaining_ms: None,
        revision: Some(1),
        desktop_entry: None,
        retained: Some(true),
        expires_ms: None,
        paused: Some(false),
    };
    (serde_json::to_vec(&notification).ok()?.len() <= 128 * 1024).then_some(notification)
}

impl Outcome {
    fn merge(&mut self, mut other: Outcome) {
        self.changed |= other.changed;
        self.persist |= other.persist;
        self.signals.append(&mut other.signals);
        self.events.append(&mut other.events);
        self.native_id = self.native_id.or(other.native_id);
    }
}

fn active_notification(
    notice: &Notice,
    revision: u64,
    created_ms: u64,
    wall_ms: u64,
    now: Instant,
    actions: &[(EntityId, String)],
    image_path: Option<String>,
) -> Notification {
    Notification {
        app_name: notice.app.clone(),
        summary: notice.summary.clone(),
        body: notice.body.clone(),
        urgency: urgency_text(notice.urgency).into(),
        resident: notice.resident,
        transient: notice.transient,
        created_ms,
        icon: (!notice.icon.is_empty()).then(|| notice.icon.clone()),
        image_path,
        actions: Entities {
            items: actions
                .iter()
                .map(|(id, key)| {
                    (
                        id.clone(),
                        NotificationAction {
                            label: notice
                                .actions
                                .iter()
                                .find(|(candidate, _)| candidate == key)
                                .map(|(_, label)| label.clone())
                                .unwrap_or_default(),
                        },
                    )
                })
                .collect(),
            order: actions.iter().map(|(id, _)| id.clone()).collect(),
        },
        default_action: actions
            .iter()
            .find(|(_, key)| key == "default")
            .map(|(id, _)| id.clone()),
        timeout_ms: Some(notice.timeout_ms),
        remaining_ms: notice.remaining.map(duration_ms),
        revision: Some(revision),
        desktop_entry: None,
        retained: Some(false),
        expires_ms: notice.deadline.map(|deadline| {
            wall_ms.saturating_add(duration_ms(deadline.saturating_duration_since(now)))
        }),
        paused: Some(notice.hovered),
    }
}

fn project_active(entry: &Active, notice: &Notice, wall_ms: u64, now: Instant) -> Notification {
    let mut notification = active_notification(
        notice,
        entry.revision,
        entry.created_ms,
        wall_ms,
        now,
        &entry.actions,
        entry.image_path.clone(),
    );
    notification.timeout_ms = Some(entry.timeout_ms as u64);
    notification.retained = Some(entry.retained);
    notification.desktop_entry = entry.desktop_entry.clone();
    notification
}

fn persisted_row(id: &EntityId, notification: &Notification) -> Value {
    let time = chrono::DateTime::from_timestamp_millis(
        notification.created_ms.min(i64::MAX as u64) as i64,
    )
    .map(|date| date.to_rfc3339())
    .unwrap_or_else(|| "1970-01-01T00:00:00+00:00".into());
    json!({
        "key":id.0.clone(),
        "time":time,
        "summary":notification.summary.clone(),
        "body":notification.body.clone(),
        "appName":notification.app_name.clone(),
        "appIcon":notification.icon.clone().unwrap_or_default(),
        "image":notification.image_path.clone().unwrap_or_default(),
        "urgency":urgency_code(&notification.urgency),
        "transient":false
    })
}

fn persisted_size(row: &Value) -> Result<usize, Failure> {
    crate::json::encode(row)
        .map(|encoded| encoded.len().saturating_add(2))
        .map_err(|_| failure(ErrorCode::Internal, "History unavailable", false))
}

fn popup_duration(timeout: i32, resolved: u32) -> Option<Duration> {
    let milliseconds = if timeout == 0 {
        5_000
    } else if resolved > 10_000 {
        10_000
    } else {
        resolved
    };
    (milliseconds > 0).then(|| Duration::from_millis(milliseconds as u64))
}

fn valid_icon(icon: &str) -> bool {
    icon.len() <= 8192
        && !icon.contains('\0')
        && (icon.starts_with('/') || (!icon.contains('/') && !icon.contains(':')))
}

fn safe_named_icon(icon: &str) -> bool {
    !icon.contains('/') && !icon.contains(':') && !icon.contains('\0') && icon.len() <= 8192
}

fn event_revision(id: EntityId, revision: u64) -> Value {
    json!({"id":id,"revision":revision})
}
fn urgency_text(urgency: u8) -> &'static str {
    match urgency {
        0 => "low",
        2 => "critical",
        _ => "normal",
    }
}
fn urgency_code(urgency: &str) -> u8 {
    match urgency {
        "low" => 0,
        "critical" => 2,
        _ => 1,
    }
}
fn duration_ms(duration: Duration) -> u64 {
    duration.as_millis().min(MAX_SAFE_INTEGER as u128) as u64
}
fn system_now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or(0)
}
fn failure(code: ErrorCode, message: &'static str, retry: bool) -> Failure {
    Failure::new(code, message, retry)
}
fn stale() -> Failure {
    failure(
        ErrorCode::StaleRevision,
        "Notification changed; refresh state",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn notice(summary: &str) -> Notice {
        Notice {
            id: 0,
            app: "fixture.app".into(),
            icon: String::new(),
            summary: summary.into(),
            body: "Fixture body".into(),
            actions: vec![("private-key".into(), "Open".into())],
            urgency: 1,
            transient: false,
            resident: false,
            popup: false,
            expanded: false,
            timeout_ms: 0,
            time_unix_ms: 0,
            deadline: None,
            remaining: None,
            hovered: false,
        }
    }

    fn input(sender: &str, replaces: u32, notice: Notice, timeout: i32) -> Input {
        Input {
            sender: sender.into(),
            replaces,
            notice,
            timeout,
            desktop_entry: Some("fixture.desktop".into()),
            image_path: None,
        }
    }

    fn storage() -> HistoryStorage {
        HistoryStorage {
            ready: true,
            writable: true,
            error: None,
        }
    }

    fn call(method: &str, params: Value) -> CoreCall {
        serde_json::from_value(json!({"m":method,"p":params})).unwrap()
    }

    fn history_rows(count: usize, wall_ms: u64) -> Vec<Value> {
        let time = chrono::DateTime::from_timestamp_millis(wall_ms as i64)
            .unwrap()
            .to_rfc3339();
        (0..count)
            .map(|index| {
                json!({
                    "key":format!("old-{index}"),
                    "time":time,
                    "summary":format!("row {index}"),
                    "body":"Fixture body",
                    "appName":"fixture.app",
                    "appIcon":"",
                    "urgency":1
                })
            })
            .collect()
    }

    #[test]
    fn legacy_history_support_check_requires_encodable_timestamped_rows() {
        let wall = system_now_ms();
        let valid = history_rows(1, wall);
        assert!(history_supported(&valid));

        let mut undated = valid[0].clone();
        undated.as_object_mut().unwrap().remove("time");
        assert!(!history_supported(&[undated]));

        let mut overlong = valid[0].clone();
        overlong["summary"] = Value::String("x".repeat(8193));
        assert!(!history_supported(&[overlong]));
    }

    #[test]
    fn restored_history_keeps_the_25_hour_dst_day_boundary() {
        let row = |time| {
            json!({
                "key":"fixture",
                "time":time,
                "summary":"DST boundary",
                "body":"",
                "appName":"fixture.app",
                "appIcon":"",
                "urgency":1
            })
        };
        let before = restored_notification(&row("2026-10-25T00:30:00+02:00")).unwrap();
        let after = restored_notification(&row("2026-10-26T00:30:00+01:00")).unwrap();

        assert_eq!(after.created_ms - before.created_ms, 25 * 60 * 60 * 1000);
    }

    #[test]
    fn replacement_keeps_notice_id_increments_revision_and_rotates_private_action_ids() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 3, Vec::new(), false, Policy::default());
        let first = model
            .notify(input(":1.5", 0, notice("first"), -1), now, wall)
            .unwrap();
        let native = first.native_id.unwrap();
        let state = model.snapshot(storage());
        let id = state.active.order[0].clone();
        let old_action = state.active.items[&id].actions.order[0].clone();

        model
            .notify(
                input(":1.5", native, notice("replacement"), -1),
                now + Duration::from_secs(1),
                wall + 1000,
            )
            .unwrap();
        let state = model.snapshot(storage());
        assert_eq!(state.active.order, vec![id.clone()]);
        assert_eq!(state.active.items[&id].revision, Some(2));
        let new_action = state.active.items[&id].actions.order[0].clone();
        assert_ne!(old_action, new_action);
        assert!(!serde_json::to_string(&state)
            .unwrap()
            .contains("private-key"));
        assert!(model
            .control(
                call(
                    "notifications.invoke_action",
                    json!({"id":id.clone(),"revision":1,"action":old_action})
                ),
                5,
                now,
                wall
            )
            .is_err());
        let outcome = model
            .control(
                call(
                    "notifications.invoke_action",
                    json!({"id":id.clone(),"revision":2,"action":new_action}),
                ),
                5,
                now,
                wall,
            )
            .unwrap();
        assert!(outcome.signals.iter().any(|signal| matches!(signal, NativeSignal::ActionInvoked { destination, key, .. } if destination == ":1.5" && key == "private-key")));
        assert!(outcome
            .signals
            .iter()
            .any(|signal| matches!(signal, NativeSignal::NotificationClosed { reason: 2, .. })));
        assert!(model.snapshot(storage()).active.items.is_empty());
        assert_eq!(model.snapshot(storage()).history_count, Some(1));
    }

    #[test]
    fn default_action_is_published_as_opaque_id_and_invokable() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 3, Vec::new(), false, Policy::default());
        let mut with_default = notice("has default");
        with_default.actions = vec![
            ("default".into(), "Open".into()),
            ("snooze".into(), "Snooze".into()),
        ];
        model
            .notify(input(":1.5", 0, with_default, -1), now, wall)
            .unwrap();
        let state = model.snapshot(storage());
        let id = state.active.order[0].clone();
        let notification = &state.active.items[&id];
        let default_action = notification.default_action.clone().unwrap();
        assert_eq!(notification.actions.items[&default_action].label, "Open");
        assert_eq!(notification.actions.items.len(), 2);
        assert!(!serde_json::to_string(notification)
            .unwrap()
            .contains("\"default\""));

        let invoked = model
            .control(
                call(
                    "notifications.invoke_action",
                    json!({"id":id,"revision":1,"action":default_action}),
                ),
                5,
                now,
                wall,
            )
            .unwrap();
        assert!(invoked.signals.iter().any(|signal| matches!(signal,
            NativeSignal::ActionInvoked { destination, key, .. }
                if destination == ":1.5" && key == "default")));

        let mut without_default = Model::new("boot-a", 4, Vec::new(), false, Policy::default());
        without_default
            .notify(input(":1.6", 0, notice("no default"), -1), now, wall)
            .unwrap();
        let state = without_default.snapshot(storage());
        let notification = &state.active.items[&state.active.order[0]];
        assert_eq!(notification.default_action, None);
        assert_eq!(notification.actions.items.len(), 1);
    }

    #[test]
    fn deadlines_hover_pause_suppression_and_popup_hide_are_independent() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        let mut critical = notice("timed");
        critical.urgency = 2;
        let native = model
            .notify(input(":1.6", 0, critical, -1), now, wall)
            .unwrap()
            .native_id
            .unwrap();
        let id = model.snapshot(storage()).active.order[0].clone();
        let initial_state = model.snapshot(storage());
        let active = &initial_state.active.items[&id];
        assert_eq!(active.timeout_ms, Some(5000));
        assert_eq!(active.urgency, "critical");
        assert_eq!(active.expires_ms, Some(wall + 5000));

        let hover = || {
            call(
                "notifications.set_hover",
                json!({"id":id.clone(),"revision":1,"hovered":true}),
            )
        };
        model
            .control(hover(), 9, now + Duration::from_secs(1), wall + 1000)
            .unwrap();
        let paused_state = model.snapshot(storage());
        let paused = &paused_state.active.items[&id];
        assert_eq!(paused.paused, Some(true));
        assert_eq!(paused.remaining_ms, Some(4000));
        assert_eq!(paused.expires_ms, None);
        assert!(model
            .tick(now + Duration::from_secs(9), wall + 9000)
            .events
            .is_empty());
        model.set_dnd(true, now + Duration::from_secs(9), wall + 9000);
        let suppressed = model.snapshot(storage());
        assert_eq!(suppressed.inhibited, Some(true));
        assert!(suppressed.popup_order.unwrap().is_empty());
        assert_eq!(suppressed.active.items[&id].paused, Some(false));
        model.set_dnd(false, now + Duration::from_secs(9), wall + 9000);
        assert!(model.snapshot(storage()).popup_order.unwrap().is_empty());
        let expired = model.tick(now + Duration::from_secs(14), wall + 14_000);
        assert!(expired.signals.iter().any(|signal| matches!(signal, NativeSignal::NotificationClosed { native_id, reason: 1, .. } if *native_id == native)));
    }

    #[test]
    fn zero_timeout_hides_popup_without_closing_and_hover_pauses_popup_clock() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        let native = model
            .notify(input(":1.7", 0, notice("popup"), 0), now, wall)
            .unwrap()
            .native_id
            .unwrap();
        let id = model.snapshot(storage()).active.order[0].clone();
        model
            .control(
                call(
                    "notifications.set_hover",
                    json!({"id":id.clone(),"revision":1,"hovered":true}),
                ),
                1,
                now + Duration::from_secs(2),
                wall + 2000,
            )
            .unwrap();
        model.tick(now + Duration::from_secs(8), wall + 8000);
        assert!(model.snapshot(storage()).active.items.contains_key(&id));
        model
            .control(
                call(
                    "notifications.set_hover",
                    json!({"id":id.clone(),"revision":1,"hovered":false}),
                ),
                1,
                now + Duration::from_secs(8),
                wall + 8000,
            )
            .unwrap();
        model.tick(now + Duration::from_secs(11), wall + 11_000);
        assert!(model.snapshot(storage()).active.items.contains_key(&id));
        model.tick(now + Duration::from_secs(12), wall + 12_000);
        let state = model.snapshot(storage());
        assert!(state.active.items.contains_key(&id));
        assert!(state.popup_order.unwrap().is_empty());
        assert_ne!(native, 0);
    }

    #[test]
    fn long_timeout_uses_ten_second_popup_policy_and_expire_toggle_disables_both_clocks() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        let id = model
            .notify(input(":1.20", 0, notice("long"), 20_000), now, wall)
            .unwrap()
            .native_id
            .unwrap();
        model.tick(now + Duration::from_secs(10), wall + 10_000);
        let state = model.snapshot(storage());
        assert!(state
            .active
            .items
            .contains_key(&EntityId("ntf:boot-a:1:1".to_owned())));
        assert!(state.popup_order.unwrap().is_empty());
        model.tick(now + Duration::from_secs(19), wall + 19_000);
        assert!(model
            .snapshot(storage())
            .active
            .items
            .contains_key(&EntityId("ntf:boot-a:1:1".to_owned())));
        model.tick(now + Duration::from_secs(20), wall + 20_000);
        assert!(model.snapshot(storage()).active.items.is_empty());
        assert_ne!(id, 0);

        let policy = Policy {
            expire: false,
            ..Policy::default()
        };
        let mut model = Model::new("boot-b", 1, Vec::new(), false, policy);
        let id = model
            .notify(
                input(":1.21", 0, notice("persistent popup"), 1_000),
                now,
                wall,
            )
            .unwrap()
            .native_id
            .unwrap();
        model.tick(now + Duration::from_secs(30), wall + 30_000);
        let state = model.snapshot(storage());
        assert!(state
            .active
            .items
            .contains_key(&EntityId("ntf:boot-b:1:1".to_owned())));
        assert_eq!(state.popup_order.as_ref().unwrap().len(), 1);
        assert_ne!(id, 0);
    }

    #[test]
    fn storage_restore_uses_new_boot_ids_and_inert_history() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        model
            .notify(input(":1.8", 0, notice("persisted"), 0), now, wall)
            .unwrap();
        let old_id = model.snapshot(storage()).active.order[0].clone();
        let rows = model.persisted_rows().unwrap().as_array().unwrap().clone();
        let restored = Model::new("boot-b", 2, rows, false, Policy::default());
        let state = restored.snapshot(storage());
        assert!(state.active.items.is_empty());
        assert_eq!(state.history.order.len(), 1);
        let new_id = &state.history.order[0];
        assert_ne!(new_id, &old_id);
        let notice = &state.history.items[new_id];
        assert!(notice.actions.items.is_empty());
        assert_eq!(notice.paused, Some(false));
        assert_eq!(notice.expires_ms, None);
        assert_eq!(notice.remaining_ms, None);
        assert_eq!(notice.image_path, None);
    }

    #[test]
    fn multiple_active_rows_restore_as_inert_history_in_arrival_order() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        model
            .notify(input(":1.81", 0, notice("first"), 0), now, wall)
            .unwrap();
        model
            .notify(input(":1.82", 0, notice("second"), 0), now, wall)
            .unwrap();

        let rows = model.persisted_rows().unwrap();
        assert_eq!(rows[0]["summary"], "first");
        assert_eq!(rows[1]["summary"], "second");
        let restored = Model::new(
            "boot-b",
            2,
            rows.as_array().unwrap().clone(),
            false,
            Policy::default(),
        );
        let state = restored.snapshot(storage());
        let summaries: Vec<_> = state
            .history
            .order
            .iter()
            .map(|id| state.history.items[id].summary.as_str())
            .collect();
        assert_eq!(summaries, vec!["second", "first"]);
        assert!(state.active.items.is_empty());
        assert!(state.popup_order.unwrap().is_empty());
    }

    #[test]
    fn retained_history_expires_on_a_policy_deadline_without_polling() {
        let now_ms = system_now_ms();
        let created_ms = now_ms
            .saturating_sub(HISTORY_TTL.as_millis() as u64)
            .saturating_add(1000);
        let time = chrono::DateTime::from_timestamp_millis(created_ms as i64)
            .unwrap()
            .to_rfc3339();
        let rows = vec![
            json!({"key":"old-private-key","time":time,"summary":"old","body":"body","appName":"fixture.app","appIcon":"fixture-icon","urgency":1}),
        ];
        let mut model = Model::new("boot-a", 1, rows, false, Policy::default());
        assert_eq!(model.snapshot(storage()).history.order.len(), 1);
        let deadline = model.next_deadline().unwrap();
        let outcome = model.tick(deadline + Duration::from_millis(1), now_ms + 1002);
        assert!(outcome.persist);
        assert!(outcome
            .events
            .iter()
            .any(|(name, _)| name == "history_changed"));
        assert!(model.snapshot(storage()).history.order.is_empty());
    }

    #[test]
    fn resolved_icon_cache_references_survive_close_but_not_restore() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        let mut notice = notice("images");
        notice.icon = "/private/cache/icon.png".into();
        let mut request = input(":1.30", 0, notice, 0);
        request.image_path = Some("/private/cache/body.png".into());
        let native = model.notify(request, now, wall).unwrap().native_id.unwrap();
        assert_eq!(
            model.image_references(),
            vec!["/private/cache/body.png", "/private/cache/icon.png"]
        );
        let closed = model.close_native(native, 3, now, wall);
        assert!(closed
            .signals
            .iter()
            .any(|signal| matches!(signal, NativeSignal::NotificationClosed { reason: 3, .. })));
        assert_eq!(model.image_references(), vec!["/private/cache/icon.png"]);
        let rows = model.persisted_rows().unwrap().as_array().unwrap().clone();
        let restored = Model::new("boot-b", 2, rows, false, Policy::default());
        assert!(restored.image_references().is_empty());
        assert!(restored
            .snapshot(storage())
            .history
            .items
            .values()
            .next()
            .unwrap()
            .icon
            .is_none());
    }

    #[test]
    fn admission_rejects_oversize_before_mutating_center_or_replacement() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        let native = model
            .notify(input(":1.9", 0, notice("before"), -1), now, wall)
            .unwrap()
            .native_id
            .unwrap();
        let mut oversized = notice("too large");
        oversized.body = "é".repeat(4097);
        assert!(model
            .notify(input(":1.9", native, oversized, -1), now, wall)
            .is_err());
        let state = model.snapshot(storage());
        assert_eq!(state.active.items.len(), 1);
        let active = state.active.items.values().next().unwrap();
        assert_eq!(active.summary, "before");
        assert_eq!(active.revision, Some(1));
    }

    #[test]
    fn feedback_policy_native_owners_and_connection_controls_remain_private() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut policy = Policy::default();
        policy.feedback_apps.insert("fixture.app".into());
        policy.feedback_summaries.insert("feedback".into());
        let mut model = Model::new("boot-a", 1, Vec::new(), false, policy);
        model
            .notify(input(":1.10", 0, notice("feedback"), 0), now, wall)
            .unwrap();
        let id = model.snapshot(storage()).active.order[0].clone();
        assert_eq!(
            model.snapshot(storage()).active.items[&id].retained,
            Some(false)
        );
        assert_eq!(model.snapshot(storage()).history_count, Some(1));
        model
            .control(
                call("notifications.dismiss", json!({"id":id,"revision":1})),
                1,
                now,
                wall,
            )
            .unwrap();
        assert_eq!(model.snapshot(storage()).history_count, Some(0));

        let popup = model
            .notify(input(":1.11", 0, notice("popup"), 0), now, wall)
            .unwrap();
        model
            .control(
                call("notifications.clear", json!({"scope":"popups"})),
                1,
                now,
                wall,
            )
            .unwrap();
        assert!(model.snapshot(storage()).active.items.len() == 1);
        assert!(model.snapshot(storage()).popup_order.unwrap().is_empty());
        assert!(popup.native_id.is_some());

        let inhibited = model
            .inhibit_native(":1.12".into(), "private reason".into(), now, wall)
            .unwrap();
        let cookie = inhibited.native_id.unwrap();
        assert_eq!(model.snapshot(storage()).inhibited, Some(true));
        assert!(!serde_json::to_string(&model.snapshot(storage()))
            .unwrap()
            .contains("private reason"));
        assert!(model.uninhibit_native(":1.13", cookie, now, wall).is_err());
        model.owner_left(":1.12", now, wall);
        assert_eq!(model.snapshot(storage()).inhibited, Some(false));
        assert!(model.snapshot(storage()).popup_order.unwrap().is_empty());

        model
            .control(
                call("notifications.set_inhibited", json!({"inhibited":true})),
                44,
                now,
                wall,
            )
            .unwrap();
        assert_eq!(model.snapshot(storage()).inhibited, Some(true));
        model.disconnect(44, now, wall);
        assert_eq!(model.snapshot(storage()).inhibited, Some(false));
    }

    #[test]
    fn dismiss_retains_by_default_and_explicit_forget_removes_history() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        let native = model
            .notify(input(":1.40", 0, notice("retained"), 0), now, wall)
            .unwrap()
            .native_id
            .unwrap();
        let id = model.snapshot(storage()).active.order[0].clone();
        model
            .control(
                call(
                    "notifications.dismiss",
                    json!({"id":id.clone(),"revision":1}),
                ),
                1,
                now,
                wall,
            )
            .unwrap();
        assert_eq!(model.snapshot(storage()).history.order, vec![id.clone()]);
        assert_eq!(model.persisted_rows().unwrap().as_array().unwrap().len(), 1);
        let forgotten = model
            .control(
                call(
                    "notifications.dismiss",
                    json!({"id":id,"revision":1,"forget":true}),
                ),
                1,
                now,
                wall,
            )
            .unwrap();
        assert!(forgotten.persist);
        assert!(model.snapshot(storage()).history.items.is_empty());
        assert!(model
            .persisted_rows()
            .unwrap()
            .as_array()
            .unwrap()
            .is_empty());
        assert!(model.center.active.is_empty());
        assert!(model.center.history.is_empty());
        assert_ne!(native, 0);
    }

    #[test]
    fn history_clear_and_retained_clear_have_separate_targets() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        model
            .notify(input(":1.41", 0, notice("closed history"), 0), now, wall)
            .unwrap();
        let closed_id = model.snapshot(storage()).active.order[0].clone();
        model
            .control(
                call(
                    "notifications.dismiss",
                    json!({"id":closed_id,"revision":1}),
                ),
                1,
                now,
                wall,
            )
            .unwrap();
        let active_id = model
            .notify(input(":1.42", 0, notice("active retained"), 0), now, wall)
            .unwrap()
            .native_id
            .unwrap();
        let state = model.snapshot(storage());
        assert_eq!(state.history.order.len(), 1);
        assert_eq!(state.active.order.len(), 1);

        model
            .control(
                call("notifications.clear", json!({"scope":"history"})),
                1,
                now,
                wall,
            )
            .unwrap();
        let state = model.snapshot(storage());
        assert!(state.history.items.is_empty());
        assert_eq!(state.active.order.len(), 1);

        let cleared = model
            .control(
                call("notifications.clear", json!({"scope":"retained"})),
                1,
                now,
                wall,
            )
            .unwrap();
        assert!(cleared.signals.iter().any(|signal| matches!(signal, NativeSignal::NotificationClosed { native_id, reason: 2, .. } if *native_id == active_id)));
        assert!(model.snapshot(storage()).active.items.is_empty());
        assert!(model.snapshot(storage()).history.items.is_empty());
    }

    #[test]
    fn active_limit_is_atomic_and_a_valid_replacement_still_succeeds() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        let mut first_native = 0;
        for index in 0..ACTIVE_COUNT {
            let sender = format!(":1.{}", index + 50);
            let outcome = model
                .notify(input(&sender, 0, notice("active"), 0), now, wall)
                .unwrap();
            if index == 0 {
                first_native = outcome.native_id.unwrap();
            }
        }
        assert_eq!(model.snapshot(storage()).active.items.len(), 128);
        let first_id = model.active[&first_native].id.clone();
        assert!(model
            .notify(input(":1.999", 0, notice("over limit"), 0), now, wall)
            .is_err());
        assert_eq!(model.snapshot(storage()).active.items.len(), 128);
        model
            .notify(
                input(":1.50", first_native, notice("replacement"), 0),
                now,
                wall,
            )
            .unwrap();
        assert_eq!(model.snapshot(storage()).active.items.len(), 128);
        assert_eq!(model.active[&first_native].id, first_id);
        assert_eq!(model.active[&first_native].revision, 2);
    }

    #[test]
    fn native_inhibitors_are_bounded_separately_from_connection_inhibitors() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        for index in 0..64 {
            model
                .inhibit_native(":1.60".into(), format!("reason {index}"), now, wall)
                .unwrap();
        }
        assert!(model
            .inhibit_native(":1.60".into(), "one too many".into(), now, wall)
            .is_err());
        let ui = model
            .control(
                call("notifications.set_inhibited", json!({"inhibited":true})),
                3,
                now,
                wall,
            )
            .unwrap();
        assert!(ui.changed);
        assert_eq!(model.snapshot(storage()).inhibited, Some(true));
        model.disconnect(3, now, wall);
        assert_eq!(model.snapshot(storage()).inhibited, Some(true));
        model.owner_left(":1.60", now, wall);
        assert_eq!(model.snapshot(storage()).inhibited, Some(false));
    }

    #[test]
    fn history_snapshot_is_a_contiguous_newest_sixty_four_row_window() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        for index in 0..65 {
            let native = model
                .notify(
                    input(
                        &format!(":1.{}", index + 100),
                        0,
                        notice(&format!("row {index}")),
                        0,
                    ),
                    now,
                    wall,
                )
                .unwrap()
                .native_id
                .unwrap();
            model.close_native(native, 3, now, wall);
        }
        let state = model.snapshot(storage());
        assert!(state.producer_valid());
        assert_eq!(state.history.order.len(), 64);
        assert_eq!(state.history_count, Some(65));
        assert_eq!(state.history_more, Some(true));
        let newest = &state.history.items[&state.history.order[0]].summary;
        let oldest = &state.history.items[state.history.order.last().unwrap()].summary;
        assert_eq!(newest, "row 64");
        assert_eq!(oldest, "row 1");
    }

    #[test]
    fn new_retained_active_notice_prunes_oldest_closed_row_and_persists() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new(
            "boot-a",
            1,
            history_rows(HISTORY_ROWS, wall),
            false,
            Policy::default(),
        );
        let outcome = model
            .notify(input(":1.200", 0, notice("new active"), 0), now, wall)
            .unwrap();

        assert!(outcome.persist);
        assert!(outcome
            .events
            .iter()
            .any(|(name, _)| name == "history_changed"));
        let state = model.snapshot(storage());
        assert_eq!(state.history_count, Some(HISTORY_ROWS as u64));
        assert_eq!(state.history.order.len(), HISTORY_WINDOW_ROWS);
        assert_eq!(model.history.len(), HISTORY_ROWS - 1);
        assert_eq!(model.history.first().unwrap().notification.summary, "row 1");
        let rows = model.persisted_rows().unwrap();
        assert_eq!(rows.as_array().unwrap().len(), HISTORY_ROWS);
        assert_eq!(rows[0]["summary"], "row 1");
    }

    #[test]
    fn unretained_feedback_active_notice_does_not_consume_storage_quota() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut policy = Policy::default();
        policy.feedback_apps.insert("fixture.app".into());
        policy.feedback_summaries.insert("feedback".into());
        let mut model = Model::new("boot-a", 1, history_rows(HISTORY_ROWS, wall), false, policy);
        let outcome = model
            .notify(input(":1.201", 0, notice("feedback"), 0), now, wall)
            .unwrap();

        assert!(!outcome.persist);
        assert_eq!(
            model.snapshot(storage()).history_count,
            Some(HISTORY_ROWS as u64 + 1)
        );
        assert_eq!(model.history.len(), HISTORY_ROWS);
        let rows = model.persisted_rows().unwrap();
        assert_eq!(rows.as_array().unwrap().len(), HISTORY_ROWS);
        assert_eq!(rows[0]["summary"], "row 0");
    }

    #[test]
    fn history_byte_quota_evicts_oldest_closed_rows_before_live_rows() {
        let now = Instant::now();
        let wall = system_now_ms();
        let version = crate::state::by_id("notifications").unwrap().version;
        for (boot, row_count, large_field) in [
            ("ascii", 2_000, "x".repeat(8192)),
            ("unicode", 680, "😀".repeat(2048)),
        ] {
            let mut rows = history_rows(row_count, wall);
            for row in &mut rows {
                row["body"] = Value::String(large_field.clone());
            }
            let mut model = Model::new(boot, 1, rows, false, Policy::default());
            let initial_closed = model.closed_count();
            let mut history_changed = false;

            for index in 0..8 {
                let mut incoming = notice(&large_field);
                incoming.app = large_field.clone();
                incoming.body = large_field.clone();
                let outcome = model
                    .notify(
                        input(&format!(":1.{}", index + 300), 0, incoming, 0),
                        now,
                        wall,
                    )
                    .unwrap();
                history_changed |= outcome
                    .events
                    .iter()
                    .any(|(name, _)| name == "history_changed");
                let persisted_rows = model.persisted_rows().unwrap();
                assert_eq!(
                    model.snapshot(storage()).history_count,
                    Some(persisted_rows.as_array().unwrap().len() as u64)
                );
                let envelope = json!({"version":version,"items":persisted_rows});
                let encoded = crate::json::encode(&envelope).unwrap();
                assert!(encoded.len() <= HISTORY_BYTES);
            }

            assert!(history_changed);
            assert!(model.closed_count() + 8 < initial_closed + 8);
            assert_eq!(model.snapshot(storage()).active.items.len(), 8);
        }
    }
    #[test]
    fn native_replacement_is_sender_scoped_and_resident_action_stays_live() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        let mut resident = notice("resident");
        resident.resident = true;
        let native = model
            .notify(input(":1.1", 0, resident, 0), now, wall)
            .unwrap()
            .native_id
            .unwrap();
        let before = model.snapshot(storage());
        let id = before.active.order[0].clone();
        let action = before.active.items[&id].actions.order[0].clone();
        let outcome = model
            .control(
                call(
                    "notifications.invoke_action",
                    json!({"id":id,"revision":1,"action":action}),
                ),
                1,
                now,
                wall,
            )
            .unwrap();
        assert!(outcome.changed);
        assert_eq!(outcome.signals.len(), 1);
        assert!(matches!(
            outcome.signals[0],
            NativeSignal::ActionInvoked { .. }
        ));
        assert!(model.snapshot(storage()).active.items.contains_key(&id));
        let other = model
            .notify(input(":1.2", native, notice("other"), 0), now, wall)
            .unwrap()
            .native_id
            .unwrap();
        assert_ne!(native, other);
        let state = model.snapshot(storage());
        assert_eq!(state.active.items.len(), 2);
        assert_eq!(state.active.items[&id].summary, "resident");
        assert_eq!(state.active.items[&id].revision, Some(1));
    }

    #[test]
    fn ui_disconnect_releases_hover_and_inhibition_without_popup_replay() {
        let now = Instant::now();
        let wall = system_now_ms();
        let mut model = Model::new("boot-a", 1, Vec::new(), false, Policy::default());
        model
            .notify(input(":1.1", 0, notice("timed"), 1000), now, wall)
            .unwrap();
        let id = model.snapshot(storage()).active.order[0].clone();
        model
            .control(
                call(
                    "notifications.set_hover",
                    json!({"id":id,"revision":1,"hovered":true}),
                ),
                1,
                now,
                wall,
            )
            .unwrap();
        model.disconnect(1, now + Duration::from_secs(2), wall + 2000);
        model.tick(now + Duration::from_secs(3), wall + 3000);
        assert!(model.snapshot(storage()).active.items.is_empty());
        model
            .control(
                call("notifications.set_inhibited", json!({"inhibited":true})),
                2,
                now,
                wall,
            )
            .unwrap();
        model
            .notify(input(":1.1", 0, notice("suppressed"), 0), now, wall)
            .unwrap();
        model.disconnect(2, now, wall);
        let state = model.snapshot(storage());
        assert_eq!(state.inhibited, Some(false));
        assert!(state.popup_order.unwrap().is_empty());
    }
}
