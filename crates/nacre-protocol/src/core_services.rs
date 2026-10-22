//! Reserved service-v1 contracts, not a runtime capability registry.
//! Implementations advertise these only after their backend and policy gates pass.
use crate::{Failure, MAX_SAFE_INTEGER};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Opaque daemon-issued identifier. Never interpret as a path, address or command.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema)]
pub struct EntityId(
    #[schemars(
        length(min = 1, max = 128),
        regex(pattern = "^[A-Za-z0-9_.:-]{1,128}$(?![\\s\\S])")
    )]
    pub String,
);

impl EntityId {
    pub fn valid(&self) -> bool {
        !self.0.is_empty()
            && self.0.len() <= 128
            && self
                .0
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"_.:-".contains(&c))
    }
}

/// Collections use stable IDs and explicit ordering; never arrays of entities.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[schemars(rename = "Entities_{T}")]
pub struct Entities<T> {
    #[schemars(extend("maxProperties" = 2048))]
    pub items: BTreeMap<EntityId, T>,
    #[schemars(extend("maxItems" = 2048, "uniqueItems" = true))]
    pub order: Vec<EntityId>,
}

impl<T> Entities<T> {
    pub fn valid(&self) -> bool {
        self.items.len() <= 2048
            && self.items.len() == self.order.len()
            && self.items.keys().all(EntityId::valid)
            && self.order.iter().all(|id| self.items.contains_key(id))
            && self
                .order
                .iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.order.len()
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Health {
    pub available: bool,
    /// Retained last-good data may be displayed but is visibly stale.
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Failure>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Device {
    pub name: String,
    pub kind: String,
    pub connected: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AccessPoint {
    pub device: EntityId,
    /// Display text only. SSID bytes and credentials stay with NetworkManager.
    pub name: String,
    #[schemars(range(min = 0, max = 100))]
    pub strength: u8,
    pub secured: bool,
    pub active: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub connection: Option<EntityId>,
    /// Radio frequency in MHz; omitted when the backend does not report it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 100000))]
    pub frequency_mhz: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Connection {
    pub name: String,
    pub kind: String,
    pub active: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NetworkState {
    pub health: Health,
    pub wireless_enabled: bool,
    pub devices: Entities<Device>,
    pub access_points: Entities<AccessPoint>,
    pub connections: Entities<Connection>,
    /// The backend's primary active connection; never infer it from map order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub primary_connection: Option<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BluetoothAdapter {
    pub name: String,
    pub powered: bool,
    pub discovering: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BluetoothDevice {
    pub adapter: EntityId,
    pub name: String,
    pub connected: bool,
    pub paired: bool,
    pub trusted: bool,
    pub pairing: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 100))]
    pub battery: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BluetoothState {
    pub health: Health,
    pub adapters: Entities<BluetoothAdapter>,
    pub devices: Entities<BluetoothDevice>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PowerProfile {
    pub name: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PowerProfilesState {
    pub health: Health,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<EntityId>,
    pub profiles: Entities<PowerProfile>,
}

/// Battery readback is independent of profile selection. Missing is unknown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BatteryState {
    pub health: Health,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 100.0))]
    pub percentage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charging: Option<bool>,
}

/// Independently available backend readings. Unknown members are omitted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioReadback {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0))]
    pub linear_volume: Option<f64>,
    /// Cube root of linear volume, matching the existing Bead slider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0))]
    pub perceptual_volume: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AudioKind {
    Output,
    Input,
    Playback,
    Capture,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioNode {
    pub name: String,
    pub kind: AudioKind,
    /// Actual linear backend volume, 0=silent, 1=100%; may exceed 1 externally.
    /// The Nacre setter separately retains its existing 0..1 policy.
    #[schemars(range(min = 0.0))]
    pub volume: f64,
    pub muted: bool,
    pub ready: bool,
    /// Authoritative when present. Legacy volume/muted are readings only when
    /// ready=true; unavailable readback must never be displayed as zero/false.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub readback: Option<AudioReadback>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Current target for a playback/capture stream, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<EntityId>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioState {
    pub health: Health,
    pub nodes: Entities<AudioNode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_output: Option<EntityId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_input: Option<EntityId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NotificationAction {
    pub label: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Notification {
    pub app_name: String,
    pub summary: String,
    /// Display content, never evaluated as code; existing markup policy applies.
    pub body: String,
    #[schemars(schema_with = "notification_urgency_schema")]
    pub urgency: String,
    pub resident: bool,
    pub transient: bool,
    /// Unix milliseconds; no clock formatting in the service.
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub created_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Local cached file only, no remote URL or inline image payload.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image_path: Option<String>,
    pub actions: Entities<NotificationAction>,
    /// Opaque ID of the freedesktop default action, when present. Null means
    /// card activation has no default action; the native key stays private.
    #[serde(default)]
    pub default_action: Option<EntityId>,
    /// Resolved policy timeout; zero means no automatic expiry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub timeout_ms: Option<u64>,
    /// Remaining lifetime at the last service transition, including hover pause.
    /// Eligibility/expiry belongs to the service, not a client countdown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub remaining_ms: Option<u64>,
    /// Producer revision; absent in legacy snapshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub revision: Option<u64>,
    /// Untrusted display identity hint.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub desktop_entry: Option<String>,
    /// Effective closed-history retention policy.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retained: Option<bool>,
    /// Projected Unix-ms deadline; producer owns monotonic expiry.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub expires_ms: Option<u64>,
    /// Hover pause state.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub paused: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HistoryStorage {
    pub ready: bool,
    pub writable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Failure>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NotificationsState {
    pub health: Health,
    pub do_not_disturb: bool,
    pub active: Entities<Notification>,
    /// A bounded window onto the existing history store; no new retention policy.
    pub history: Entities<Notification>,
    /// Service-selected popup eligibility. Missing is unknown, not all active.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(extend("maxItems" = 128, "uniqueItems" = true))]
    pub popup_order: Option<Vec<EntityId>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inhibited: Option<bool>,
    /// Total retained history including nontransient active notices, not window size.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub history_count: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_storage: Option<HistoryStorage>,
    /// Whether older closed records exist beyond this bounded history window.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub history_more: Option<bool>,
}

impl NotificationsState {
    /// Validate a complete producer snapshot; legacy snapshots use `valid`.
    /// Native admission and actual patch budgets must also be checked by the host.
    pub fn producer_valid(&self) -> bool {
        if !CoreSnapshot::Notifications(self.clone()).valid()
            || self
                .active
                .items
                .keys()
                .any(|id| self.history.items.contains_key(id))
            || self.active.items.len() > 128
            || self.history.items.len() > 64
            || self.popup_order.is_none()
            || self.inhibited.is_none()
            || (self.do_not_disturb && self.inhibited != Some(true))
            || self.history_storage.as_ref().is_none_or(|storage| {
                (storage.writable && !storage.ready)
                    || (storage.ready
                        && (self.history_count.is_none() || self.history_more.is_none()))
                    || (!storage.ready
                        && (self.history_count.is_some() || self.history_more.is_some()))
            })
            || ((self.do_not_disturb || self.inhibited == Some(true))
                && self
                    .popup_order
                    .as_ref()
                    .is_some_and(|order| !order.is_empty()))
        {
            return false;
        }
        let bounded = self
            .active
            .items
            .values()
            .chain(self.history.items.values())
            .all(|n| {
                let text_ok = |value: &str, max: usize| value.len() <= max && !value.contains('\0');
                text_ok(&n.app_name, 8192)
                    && text_ok(&n.summary, 8192)
                    && text_ok(&n.body, 8192)
                    && text_ok(n.icon.as_deref().unwrap_or_default(), 8192)
                    && n.desktop_entry.as_deref().is_none_or(|v| text_ok(v, 1024))
                    && n.image_path.as_deref().is_none_or(|v| text_ok(v, 4096))
                    && n.actions.items.len() <= 16
                    && n.actions.items.values().all(|a| text_ok(&a.label, 1024))
                    && n.default_action
                        .as_ref()
                        .is_none_or(|id| n.actions.items.contains_key(id))
                    && matches!(n.urgency.as_str(), "low" | "normal" | "critical")
                    && n.revision
                        .is_some_and(|revision| revision > 0 && timestamp_valid(revision))
                    && n.retained.is_some()
                    && n.paused.is_some()
                    && !(n.transient && n.retained == Some(true))
                    && !(n.paused == Some(true) && n.expires_ms.is_some())
                    && serde_json::to_vec(n).is_ok_and(|encoded| encoded.len() <= 128 * 1024)
            });
        if !bounded
            || serde_json::to_vec(&self.active).map_or(true, |encoded| encoded.len() > 512 * 1024)
            || serde_json::to_vec(&self.history).map_or(true, |encoded| encoded.len() > 256 * 1024)
        {
            return false;
        }
        serde_json::to_vec(&serde_json::json!({"t":"snap","topic":"notifications","seq":MAX_SAFE_INTEGER,"data":self}))
            .is_ok_and(|encoded| encoded.len() < 1_048_576)
    }
}

fn notification_urgency_schema(_: &mut schemars::SchemaGenerator) -> schemars::Schema {
    schemars::json_schema!({"type":"string","enum":["low","normal","critical"]})
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OsdChannel {
    Volume,
    Brightness,
    Microphone,
    Capslock,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OsdUnit {
    Linear,
    Perceptual,
    Fraction,
    Boolean,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OsdItem {
    /// Changes for repeated equal-valued requests; scoped to the daemon boot.
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub serial: u64,
    pub channel: OsdChannel,
    pub unit: OsdUnit,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0))]
    pub value: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    /// Absolute same-host Unix milliseconds. Expired snapshots never replay an OSD.
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub expires_ms: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct OsdState {
    pub health: Health,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<OsdItem>,
}

impl AudioReadback {
    fn valid(&self) -> bool {
        [self.linear_volume, self.perceptual_volume]
            .into_iter()
            .flatten()
            .all(|value| value.is_finite() && value >= 0.0)
            && self
                .linear_volume
                .zip(self.perceptual_volume)
                .is_none_or(|(linear, perceptual)| {
                    (perceptual.powi(3) - linear).abs() <= 0.00001 * linear.max(1.0)
                })
    }
}

impl OsdItem {
    fn valid(&self) -> bool {
        self.serial > 0
            && timestamp_valid(self.serial)
            && timestamp_valid(self.expires_ms)
            && self
                .value
                .is_none_or(|value| value.is_finite() && value >= 0.0)
            && match (&self.channel, &self.unit) {
                (
                    OsdChannel::Volume | OsdChannel::Microphone,
                    OsdUnit::Linear | OsdUnit::Perceptual,
                ) => true,
                (OsdChannel::Brightness, OsdUnit::Fraction) => {
                    self.muted.is_none() && self.value.is_none_or(|value| value <= 1.0)
                }
                (OsdChannel::Capslock, OsdUnit::Boolean) => {
                    self.muted.is_none()
                        && self.value.is_none_or(|value| value == 0.0 || value == 1.0)
                }
                _ => false,
            }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrayMenuEntry {
    pub label: String,
    pub enabled: bool,
    pub visible: bool,
    pub separator: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub checked: Option<bool>,
    /// Child IDs into the same menu map, not nested entity arrays.
    pub children: Vec<EntityId>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrayItem {
    pub title: String,
    pub visible: bool,
    pub item_is_menu: bool,
    pub attention: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
    /// Decoded pixmaps stay in a worker/cache. Never publish pixel arrays.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_path: Option<String>,
    pub menu: Entities<TrayMenuEntry>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct TrayState {
    pub health: Health,
    pub items: Entities<TrayItem>,
}

impl TrayItem {
    fn valid_menu(&self) -> bool {
        if !self.menu.valid() {
            return false;
        }
        let mut children = std::collections::BTreeSet::new();
        if !self
            .menu
            .items
            .values()
            .all(|entry| entry.children.iter().all(|id| children.insert(id)))
        {
            return false;
        }
        for start in self.menu.items.keys() {
            let mut pending = vec![(start, Vec::new())];
            while let Some((id, mut ancestors)) = pending.pop() {
                if ancestors.len() >= 16 || ancestors.contains(&id) {
                    return false;
                }
                let Some(entry) = self.menu.items.get(id) else {
                    return false;
                };
                if entry
                    .children
                    .iter()
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    != entry.children.len()
                {
                    return false;
                }
                ancestors.push(id);
                pending.extend(
                    entry
                        .children
                        .iter()
                        .map(|child| (child, ancestors.clone())),
                );
            }
        }
        true
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WeatherHour {
    pub time: String,
    pub temperature: Option<f64>,
    pub wmo: Option<u16>,
    #[schemars(range(min = 0.0, max = 100.0))]
    pub rain: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WeatherDay {
    pub date: String,
    pub high: Option<f64>,
    pub low: Option<f64>,
    pub wmo: Option<u16>,
    #[schemars(range(min = 0.0, max = 100.0))]
    pub rain: Option<f64>,
    pub sunrise: Option<String>,
    pub sunset: Option<String>,
    pub uv: Option<f64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WeatherCurrent {
    pub location: String,
    pub timezone: String,
    pub local_time: String,
    pub wmo: Option<u16>,
    pub description: String,
    /// Temperatures in degrees Celsius, wind in km/h, pressure in hPa.
    pub temperature: Option<f64>,
    pub feels_like: Option<f64>,
    pub high: Option<f64>,
    pub low: Option<f64>,
    #[schemars(range(min = 0.0, max = 100.0))]
    pub humidity: Option<f64>,
    pub wind: Option<f64>,
    pub direction: Option<f64>,
    pub pressure: Option<f64>,
    pub aqi: Option<f64>,
    pub is_day: bool,
    pub hours: Entities<WeatherHour>,
    pub days: Entities<WeatherDay>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WeatherState {
    pub health: Health,
    /// Unix seconds, matching the existing helper's checked field.
    #[schemars(range(min = 0.0, max = 9007199254740991.0))]
    pub checked: Option<f64>,
    pub provider: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current: Option<WeatherCurrent>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct UpdatesState {
    pub health: Health,
    #[schemars(range(min = 0.0, max = 9007199254740991.0))]
    pub checked: Option<f64>,
    /// None means unknown, including a failed refresh; never treat as zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CalendarEvent {
    pub title: String,
    /// Unix seconds; day labels are interpreted in the configured local timezone.
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub start: u64,
    pub time: String,
    pub date: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CalendarState {
    pub health: Health,
    pub configured: bool,
    pub events: Entities<CalendarEvent>,
}

/// Existing dashboard-media enrichment result; not a new playback-control API.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct LyricLine {
    pub time: f64,
    pub text: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DashboardMediaState {
    pub health: Health,
    pub key: String,
    pub lines: Entities<LyricLine>,
    pub plain: String,
    pub palette: BTreeMap<String, String>,
    pub lyrics_message: String,
    pub cache_seconds: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SystemProcess {
    pub pid: u32,
    pub name: String,
    pub start: String,
    pub uid: u32,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub memory: u64,
    pub cpu_seconds: f64,
    pub parent: u32,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SystemDisk {
    pub path: String,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub total: u64,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub used: u64,
}
/// Physical memory in bytes; `used` excludes reclaimable cache (MemAvailable).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SystemMemory {
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub used: u64,
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub total: u64,
}
/// Busiest GPU. Each member is independently optional; omitted means unknown.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SystemGpu {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 100.0))]
    pub percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub memory_used: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub memory_total: Option<u64>,
}
/// One interval rate in **bytes per second** (the `_bps` names are fixed by Bead B0024).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NetworkSample {
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub down_bps: u64,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub up_bps: u64,
}
/// What the host card shows. Every member is independently optional; omitted means
/// unknown. Static for a subscription: clients count uptime from `boot_ms`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SystemHost {
    /// Login name of the daemon's user.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 64))]
    pub user: Option<String>,
    /// `PRETTY_NAME` (else `NAME`) from os-release.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub os: Option<String>,
    /// Kernel release, for example `6.12.1-arch1-1`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 128))]
    pub kernel: Option<String>,
    /// Seconds since boot when the subscription started.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub uptime_s: Option<u64>,
    /// Boot time in Unix milliseconds; uptime now is `now - boot_ms`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub boot_ms: Option<u64>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DashboardSystemState {
    pub health: Health,
    pub processes: Entities<SystemProcess>,
    pub disks: Entities<SystemDisk>,
    /// Whole-machine busy share in [0,100] over the last sample interval.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0.0, max = 100.0))]
    pub cpu_percent: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<SystemMemory>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gpu: Option<SystemGpu>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<SystemHost>,
    /// Oldest first, at most 32 samples; absent or empty means no sample yet.
    /// A time series, not entities: order is time, items have no identity.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schemars(extend("maxItems" = 32))]
    pub network: Vec<NetworkSample>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PlayerStatus {
    Playing,
    Paused,
    Stopped,
}
/// One MPRIS player. Text is display-only and bounded; unknown values are omitted.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Player {
    #[schemars(length(max = 256))]
    pub identity: String,
    pub status: PlayerStatus,
    #[schemars(length(max = 512))]
    pub title: String,
    #[schemars(length(max = 512))]
    pub artist: String,
    #[schemars(length(max = 512))]
    pub album: String,
    /// Absolute path of a private cached copy; remote URLs never cross the protocol.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(length(min = 1, max = 4096))]
    pub art_path: Option<String>,
    /// Position at `position_updated_ms`. While `playing`, clients extrapolate
    /// locally; the daemon republishes on seek, status and track changes only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub position_ms: Option<u64>,
    /// Unix milliseconds of the position reading; present exactly with `position_ms`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub position_updated_ms: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub length_ms: Option<u64>,
    pub can_control: bool,
    pub can_play_pause: bool,
    pub can_next: bool,
    pub can_previous: bool,
    pub can_seek: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlayerState {
    pub health: Health,
    /// The player the daemon considers current; clients may let the user override.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<EntityId>,
    pub players: Entities<Player>,
}

/// Payload alternatives embedded in the existing snap message, not a new frame.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "topic", content = "data")]
pub enum CoreSnapshot {
    #[serde(rename = "network")]
    Network(NetworkState),
    #[serde(rename = "bluetooth")]
    Bluetooth(BluetoothState),
    #[serde(rename = "power.profiles")]
    PowerProfiles(PowerProfilesState),
    #[serde(rename = "battery")]
    Battery(BatteryState),
    #[serde(rename = "audio")]
    Audio(AudioState),
    #[serde(rename = "notifications")]
    Notifications(NotificationsState),
    #[serde(rename = "osd")]
    Osd(OsdState),
    #[serde(rename = "tray")]
    Tray(TrayState),
    #[serde(rename = "dashboard.weather")]
    Weather(Box<WeatherState>),
    #[serde(rename = "dashboard.updates")]
    Updates(UpdatesState),
    #[serde(rename = "dashboard.calendar")]
    Calendar(CalendarState),
    #[serde(rename = "dashboard.media")]
    Media(DashboardMediaState),
    #[serde(rename = "dashboard.system")]
    System(DashboardSystemState),
    #[serde(rename = "dashboard.player")]
    Player(PlayerState),
}

impl CoreSnapshot {
    pub fn valid(&self) -> bool {
        match self {
            Self::Network(s) => {
                s.devices.valid()
                    && s.access_points.valid()
                    && s.connections.valid()
                    && s.primary_connection.as_ref().is_none_or(|id| {
                        s.connections
                            .items
                            .get(id)
                            .is_some_and(|connection| connection.active)
                    })
                    && s.access_points.items.values().all(|ap| {
                        ap.strength <= 100
                            && s.devices.items.contains_key(&ap.device)
                            && ap
                                .connection
                                .as_ref()
                                .is_none_or(|id| s.connections.items.contains_key(id))
                    })
            }
            Self::Bluetooth(s) => {
                s.adapters.valid()
                    && s.devices.valid()
                    && s.devices.items.values().all(|d| {
                        s.adapters.items.contains_key(&d.adapter)
                            && d.battery.is_none_or(|b| b <= 100)
                    })
            }
            Self::PowerProfiles(s) => {
                s.profiles.valid()
                    && s.active
                        .as_ref()
                        .is_none_or(|id| s.profiles.items.contains_key(id))
            }
            Self::Battery(s) => s.percentage.is_none_or(percent_valid),
            Self::Audio(s) => {
                s.nodes.valid()
                    && s.default_output.as_ref().is_none_or(|id| {
                        s.nodes
                            .items
                            .get(id)
                            .is_some_and(|n| n.kind == AudioKind::Output)
                    })
                    && s.default_input.as_ref().is_none_or(|id| {
                        s.nodes
                            .items
                            .get(id)
                            .is_some_and(|n| n.kind == AudioKind::Input)
                    })
                    && s.nodes.items.values().all(|n| {
                        n.volume.is_finite()
                            && n.volume >= 0.0
                            && n.readback.as_ref().is_none_or(|readback| {
                                readback.valid()
                                    && (!n.ready
                                        || (readback
                                            .linear_volume
                                            .is_none_or(|value| value == n.volume)
                                            && readback.muted.is_none_or(|value| value == n.muted)))
                            })
                            && n.target.as_ref().is_none_or(|id| {
                                s.nodes.items.get(id).is_some_and(|target| {
                                    matches!(
                                        (&n.kind, &target.kind),
                                        (AudioKind::Playback, AudioKind::Output)
                                            | (AudioKind::Capture, AudioKind::Input)
                                    )
                                })
                            })
                    })
            }
            Self::Notifications(s) => {
                s.active.valid()
                    && s.history.valid()
                    && s.popup_order.as_ref().is_none_or(|order| {
                        order.len() <= 128
                            && order.iter().all(|id| s.active.items.contains_key(id))
                            && order
                                .iter()
                                .collect::<std::collections::BTreeSet<_>>()
                                .len()
                                == order.len()
                    })
                    && s.history_count.is_none_or(|count| {
                        timestamp_valid(count)
                            && count
                                >= (s.history.items.len()
                                    + s.active
                                        .items
                                        .values()
                                        .filter(|notice| !notice.transient)
                                        .count()) as u64
                    })
                    && s.active
                        .items
                        .values()
                        .chain(s.history.items.values())
                        .all(|n| {
                            timestamp_valid(n.created_ms)
                                && n.timeout_ms.is_none_or(timestamp_valid)
                                && n.remaining_ms.is_none_or(timestamp_valid)
                                && n.revision.is_none_or(|revision| {
                                    revision > 0 && timestamp_valid(revision)
                                })
                                && n.expires_ms.is_none_or(timestamp_valid)
                                && n.actions.valid()
                                && n.image_path
                                    .as_ref()
                                    .is_none_or(|p| p.starts_with('/') && !p.contains('\0'))
                        })
            }
            Self::Osd(s) => s.current.as_ref().is_none_or(OsdItem::valid),
            Self::Tray(s) => {
                s.items.valid()
                    && s.items.items.values().all(|item| {
                        item.valid_menu()
                            && item
                                .icon_path
                                .as_ref()
                                .is_none_or(|p| p.starts_with('/') && !p.contains('\0'))
                            && item.menu.items.values().all(|e| {
                                e.children.iter().all(|id| item.menu.items.contains_key(id))
                            })
                    })
            }
            Self::Weather(s) => {
                s.checked.is_none_or(timestamp_seconds_valid)
                    && s.current.as_ref().is_none_or(|c| {
                        c.hours.valid()
                            && c.days.valid()
                            && c.hours.items.len() <= 24
                            && c.days.items.len() <= 10
                            && [
                                c.temperature,
                                c.feels_like,
                                c.high,
                                c.low,
                                c.humidity,
                                c.wind,
                                c.direction,
                                c.pressure,
                                c.aqi,
                            ]
                            .into_iter()
                            .flatten()
                            .all(f64::is_finite)
                            && c.humidity.is_none_or(percent_valid)
                            && c.hours.items.values().all(|h| {
                                h.temperature.is_none_or(f64::is_finite)
                                    && h.rain.is_none_or(percent_valid)
                            })
                            && c.days.items.values().all(|d| {
                                d.high.is_none_or(f64::is_finite)
                                    && d.low.is_none_or(f64::is_finite)
                                    && d.rain.is_none_or(percent_valid)
                                    && d.uv.is_none_or(f64::is_finite)
                            })
                    })
            }
            Self::Updates(s) => s.checked.is_none_or(timestamp_seconds_valid),
            Self::Calendar(s) => {
                s.events.valid() && s.events.items.values().all(|e| timestamp_valid(e.start))
            }
            Self::Media(s) => {
                s.lines.valid()
                    && s.lines
                        .items
                        .values()
                        .all(|l| l.time.is_finite() && l.time >= 0.0)
            }
            Self::System(s) => {
                s.processes.valid()
                    && s.disks.valid()
                    && s.processes.items.values().all(|p| {
                        p.cpu_seconds.is_finite()
                            && p.cpu_seconds >= 0.0
                            && timestamp_valid(p.memory)
                    })
                    && s.disks
                        .items
                        .values()
                        .all(|d| timestamp_valid(d.total) && d.used <= d.total)
                    && s.cpu_percent.is_none_or(percent_valid)
                    && s.memory.as_ref().is_none_or(|m| {
                        m.total > 0 && timestamp_valid(m.total) && m.used <= m.total
                    })
                    && s.gpu.as_ref().is_none_or(|g| {
                        g.percent.is_none_or(percent_valid)
                            && g.memory_total.is_none_or(|t| t > 0 && timestamp_valid(t))
                            && g.memory_used.is_none_or(|u| {
                                timestamp_valid(u) && g.memory_total.is_none_or(|t| u <= t)
                            })
                    })
                    && s.host.as_ref().is_none_or(|h| {
                        let text = |v: &Option<String>, max: usize| {
                            v.as_deref().is_none_or(|t| {
                                (1..=max).contains(&t.chars().count())
                                    && !t.chars().any(char::is_control)
                            })
                        };
                        text(&h.user, 64)
                            && text(&h.os, 128)
                            && text(&h.kernel, 128)
                            && h.uptime_s.is_none_or(timestamp_valid)
                            && h.boot_ms.is_none_or(timestamp_valid)
                    })
                    && s.network.len() <= 32
                    && s.network
                        .iter()
                        .all(|n| timestamp_valid(n.down_bps) && timestamp_valid(n.up_bps))
            }
            Self::Player(s) => {
                s.players.valid()
                    && s.active
                        .as_ref()
                        .is_none_or(|id| s.players.items.contains_key(id))
                    && s.players.items.values().all(|p| {
                        let text = |value: &str, limit| value.chars().count() <= limit;
                        text(&p.identity, 256)
                            && text(&p.title, 512)
                            && text(&p.artist, 512)
                            && text(&p.album, 512)
                            && p.art_path.as_ref().is_none_or(|path| {
                                path.starts_with('/')
                                    && path.len() <= 4096
                                    && !path.contains(['\0', '\n'])
                                    && !path.contains("://")
                            })
                            && p.position_ms.is_some() == p.position_updated_ms.is_some()
                            && p.position_ms.is_none_or(timestamp_valid)
                            && p.position_updated_ms.is_none_or(timestamp_valid)
                            && p.length_ms.is_none_or(|l| l > 0 && timestamp_valid(l))
                            && p.position_ms
                                .zip(p.length_ms)
                                .is_none_or(|(position, length)| position <= length)
                            && (p.can_control
                                || !(p.can_play_pause
                                    || p.can_next
                                    || p.can_previous
                                    || p.can_seek))
                    })
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlayerTarget {
    pub player: EntityId,
}
/// Absolute seek on the player's current track; the daemon refuses it unless the
/// topic reports `can_seek` and a known length, and clamps nothing silently.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlayerSeek {
    pub player: EntityId,
    #[schemars(range(min = 0, max = 9007199254740991_u64))]
    pub position_ms: u64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct WifiPower {
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum NetworkTarget {
    AccessPoint(EntityId),
    Connection(EntityId),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NetworkConnect {
    pub device: EntityId,
    pub target: NetworkTarget,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DeviceTarget {
    pub device: EntityId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct BluetoothPower {
    pub adapter: EntityId,
    pub powered: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ProfileTarget {
    pub profile: EntityId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioTarget {
    pub node: EntityId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioVolume {
    pub node: EntityId,
    #[schemars(range(min = 0.0, max = 1.0))]
    pub volume: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioMute {
    pub node: EntityId,
    pub muted: bool,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct AudioRoute {
    pub stream: EntityId,
    pub target: EntityId,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Refresh {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NotificationDnd {
    pub enabled: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NotificationDismiss {
    pub id: EntityId,
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub revision: u64,
    #[serde(default)]
    pub forget: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum NotificationClearScope {
    History,
    Retained,
    Popups,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NotificationClear {
    pub scope: NotificationClearScope,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NotificationActionCall {
    pub id: EntityId,
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub revision: u64,
    pub action: EntityId,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NotificationHover {
    pub id: EntityId,
    #[schemars(range(min = 1, max = 9007199254740991_u64))]
    pub revision: u64,
    pub hovered: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NotificationInhibited {
    pub inhibited: bool,
}

/// Reserved method payloads embedded in call.m / call.p. No backend is registered.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "m", content = "p")]
pub enum CoreCall {
    #[serde(rename = "notifications.set_dnd")]
    NotificationDnd(NotificationDnd),
    #[serde(rename = "notifications.dismiss")]
    NotificationDismiss(NotificationDismiss),
    #[serde(rename = "notifications.clear")]
    NotificationClear(NotificationClear),
    #[serde(rename = "notifications.invoke_action")]
    NotificationAction(NotificationActionCall),
    #[serde(rename = "notifications.set_hover")]
    NotificationHover(NotificationHover),
    #[serde(rename = "notifications.set_inhibited")]
    NotificationInhibited(NotificationInhibited),
    #[serde(rename = "network.set_wifi")]
    WifiPower(WifiPower),
    #[serde(rename = "network.connect")]
    NetworkConnect(NetworkConnect),
    #[serde(rename = "network.disconnect")]
    NetworkDisconnect(DeviceTarget),
    #[serde(rename = "bluetooth.set_power")]
    BluetoothPower(BluetoothPower),
    #[serde(rename = "bluetooth.connect")]
    BluetoothConnect(DeviceTarget),
    #[serde(rename = "bluetooth.disconnect")]
    BluetoothDisconnect(DeviceTarget),
    #[serde(rename = "bluetooth.pair")]
    BluetoothPair(DeviceTarget),
    #[serde(rename = "power.set_profile")]
    PowerProfile(ProfileTarget),
    #[serde(rename = "audio.set_default")]
    AudioDefault(AudioTarget),
    #[serde(rename = "audio.set_volume")]
    AudioVolume(AudioVolume),
    #[serde(rename = "audio.set_mute")]
    AudioMute(AudioMute),
    #[serde(rename = "audio.route")]
    AudioRoute(AudioRoute),
    #[serde(rename = "dashboard.weather.refresh")]
    WeatherRefresh(Refresh),
    #[serde(rename = "dashboard.updates.refresh")]
    UpdatesRefresh(Refresh),
    #[serde(rename = "dashboard.calendar.refresh")]
    CalendarRefresh(Refresh),
    #[serde(rename = "player.play_pause")]
    PlayerPlayPause(PlayerTarget),
    #[serde(rename = "player.next")]
    PlayerNext(PlayerTarget),
    #[serde(rename = "player.previous")]
    PlayerPrevious(PlayerTarget),
    #[serde(rename = "player.seek")]
    PlayerSeek(PlayerSeek),
}

impl CoreCall {
    pub fn valid(&self) -> bool {
        match self {
            Self::WifiPower(_)
            | Self::NotificationDnd(_)
            | Self::NotificationClear(_)
            | Self::NotificationInhibited(_)
            | Self::WeatherRefresh(_)
            | Self::UpdatesRefresh(_)
            | Self::CalendarRefresh(_) => true,
            Self::NetworkConnect(p) => {
                p.device.valid()
                    && match &p.target {
                        NetworkTarget::AccessPoint(id) | NetworkTarget::Connection(id) => {
                            id.valid()
                        }
                    }
            }
            Self::NetworkDisconnect(p)
            | Self::BluetoothConnect(p)
            | Self::BluetoothDisconnect(p)
            | Self::BluetoothPair(p) => p.device.valid(),
            Self::BluetoothPower(p) => p.adapter.valid(),
            Self::PowerProfile(p) => p.profile.valid(),
            Self::AudioDefault(p) => p.node.valid(),
            Self::AudioVolume(p) => {
                p.node.valid() && p.volume.is_finite() && (0.0..=1.0).contains(&p.volume)
            }
            Self::AudioMute(p) => p.node.valid(),
            Self::AudioRoute(p) => p.stream.valid() && p.target.valid(),
            Self::PlayerPlayPause(p) | Self::PlayerNext(p) | Self::PlayerPrevious(p) => {
                p.player.valid()
            }
            Self::PlayerSeek(p) => p.player.valid() && timestamp_valid(p.position_ms),
            Self::NotificationDismiss(p) => {
                p.id.valid() && timestamp_valid(p.revision) && p.revision > 0
            }
            Self::NotificationAction(p) => {
                p.id.valid() && p.action.valid() && timestamp_valid(p.revision) && p.revision > 0
            }
            Self::NotificationHover(p) => {
                p.id.valid() && timestamp_valid(p.revision) && p.revision > 0
            }
        }
    }
}

/// Success means the backend acknowledged the operation; topic state is authoritative.
/// No optimistic UI mutation and no automatic replay after a lost reply.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct CoreResult {
    pub changed: bool,
}

pub fn timestamp_valid(value: u64) -> bool {
    value <= MAX_SAFE_INTEGER
}
fn timestamp_seconds_valid(value: f64) -> bool {
    value.is_finite() && (0.0..=MAX_SAFE_INTEGER as f64).contains(&value)
}
fn percent_valid(value: f64) -> bool {
    value.is_finite() && (0.0..=100.0).contains(&value)
}
