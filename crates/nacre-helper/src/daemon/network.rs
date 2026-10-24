//! One NetworkManager signal source; one-release clients retain `changed` lines.
mod controls;
pub use controls::{dispatch, Requests};

use super::{client, hub::Shared};
use crate::Result;
use nacre_protocol::{
    core_services::{
        AccessPoint, Connection, CoreSnapshot, Device, Entities, EntityId, Health, NetworkState,
        VpnConnection, VpnConnectionState, VpnState,
    },
    framing,
    network::NetworkWatch,
    ErrorCode, Failure, Id, Message, Mirror, Update, MAX_SAFE_INTEGER,
};
use serde_json::json;
use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap, HashSet},
    io::Write,
    path::Path,
    rc::Rc,
    time::Duration,
};

pub const TOPIC: &str = "network.watch";
pub const CORE_TOPIC: &str = "network";
/// N0062: saved VPN profiles, derived from the same NetworkManager snapshot as `network`.
pub const VPN_TOPIC: &str = "vpn";

const NM_SERVICE: &str = "org.freedesktop.NetworkManager";
const NM_MANAGER: &str = "/org/freedesktop/NetworkManager";
const MAX_ENTITIES: usize = 2048;
const MAX_REPLY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug)]
enum CoreError {
    Unavailable,
    TooLarge,
    Invalid,
    OwnerChanged,
    StaleTarget,
    PermissionDenied,
}

#[derive(Clone, Debug)]
struct IdentityTable {
    namespace: String,
    generation: u64,
    next: u64,
    live: HashMap<String, EntityId>,
}

impl IdentityTable {
    fn new(generation: u64) -> Self {
        Self {
            namespace: String::new(),
            generation,
            next: 0,
            live: HashMap::new(),
        }
    }

    fn id(&mut self, kind: &str, source: &str) -> std::result::Result<EntityId, CoreError> {
        let key = format!("{kind}\0{source}");
        if let Some(id) = self.live.get(&key) {
            return Ok(id.clone());
        }
        self.next = self.next.checked_add(1).ok_or(CoreError::TooLarge)?;
        let id = EntityId(format!(
            "net:{}:{}:{}",
            self.namespace, self.generation, self.next
        ));
        if !id.valid() {
            return Err(CoreError::TooLarge);
        }
        self.live.insert(key, id.clone());
        Ok(id)
    }

    fn retain(&mut self, sources: &HashSet<String>) {
        self.live.retain(|source, _| sources.contains(source));
    }
}

struct NetworkProjection {
    ids: IdentityTable,
    last_good: Option<NetworkState>,
    #[cfg(test)]
    publication_witness: Option<tokio::sync::mpsc::UnboundedSender<NetworkState>>,
}

impl NetworkProjection {
    fn new(last_good: Option<NetworkState>, generation: u64) -> Self {
        Self {
            ids: IdentityTable::new(generation),
            last_good,
            #[cfg(test)]
            publication_witness: None,
        }
    }

    fn publish_failure(&mut self, hub: &Shared, error: CoreError) -> Result<()> {
        if let Some(requests) = &hub.borrow().network {
            requests.disable();
        }
        let state = unavailable_state(self.last_good.clone(), error);
        hub.borrow_mut()
            .publish(CORE_TOPIC, serde_json::to_value(&state)?)?;
        publish_vpn_unavailable(hub, &state.health)?;
        #[cfg(test)]
        if let Some(witness) = &self.publication_witness {
            let _ = witness.send(state);
        }
        Ok(())
    }
}

pub fn publish_core_starting(hub: &Shared) -> Result<()> {
    let state = NetworkState {
        health: Health {
            available: false,
            stale: false,
            error: None,
        },
        wireless_enabled: false,
        devices: empty_entities(),
        access_points: empty_entities(),
        connections: empty_entities(),
        primary_connection: None,
    };
    hub.borrow_mut()
        .publish(CORE_TOPIC, serde_json::to_value(state)?)?;
    let vpn = VpnState {
        health: Health {
            available: false,
            stale: false,
            error: None,
        },
        connections: empty_entities(),
    };
    hub.borrow_mut()
        .publish(VPN_TOPIC, serde_json::to_value(vpn)?)?;
    Ok(())
}

/// The `vpn` topic when NetworkManager cannot be read: the last good profiles stay,
/// marked stale, exactly like `network`.
fn publish_vpn_unavailable(hub: &Shared, health: &Health) -> Result<()> {
    let kept = health
        .stale
        .then(|| hub.borrow().snapshot(VPN_TOPIC))
        .flatten()
        .and_then(|value| serde_json::from_value::<VpnState>(value).ok())
        .map_or_else(empty_entities, |state| state.connections);
    let state = VpnState {
        health: health.clone(),
        connections: kept,
    };
    hub.borrow_mut()
        .publish(VPN_TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

fn unavailable_state(last_good: Option<NetworkState>, error: CoreError) -> NetworkState {
    let (code, message) = match error {
        CoreError::TooLarge => (
            ErrorCode::TooLarge,
            "Network state exceeds the supported limit",
        ),
        CoreError::Unavailable => (ErrorCode::Unavailable, "NetworkManager is unavailable"),
        CoreError::PermissionDenied => {
            (ErrorCode::PermissionDenied, "Network state read was denied")
        }
        CoreError::Invalid | CoreError::StaleTarget => {
            (ErrorCode::Unavailable, "Network state is unavailable")
        }
        CoreError::OwnerChanged => (
            ErrorCode::Unavailable,
            "NetworkManager service owner changed",
        ),
    };
    let last_good = if matches!(error, CoreError::TooLarge) {
        None
    } else {
        last_good
    };
    match last_good {
        Some(mut state) => {
            state.health = Health {
                available: false,
                stale: true,
                error: Some(Failure::new(code, message, true)),
            };
            state
        }
        None => NetworkState {
            health: Health {
                available: false,
                stale: false,
                error: Some(Failure::new(code, message, true)),
            },
            wireless_enabled: false,
            devices: empty_entities(),
            access_points: empty_entities(),
            connections: empty_entities(),
            primary_connection: None,
        },
    }
}

fn empty_entities<T>() -> Entities<T> {
    Entities {
        items: BTreeMap::new(),
        order: Vec::new(),
    }
}

fn next_identity_generation(counter: &Cell<u64>) -> Result<u64> {
    let next = counter
        .get()
        .checked_add(1)
        .filter(|value| *value <= MAX_SAFE_INTEGER)
        .ok_or("Network service generation exhausted")?;
    counter.set(next);
    Ok(next)
}

#[derive(Clone, Debug)]
struct RawAccessPoint {
    path: String,
    ssid: Vec<u8>,
    strength: u8,
    flags: u32,
    wpa_flags: u32,
    rsn_flags: u32,
    frequency: Option<u32>,
}

#[derive(Clone, Debug)]
struct RawDevice {
    path: String,
    interface: String,
    kind: u32,
    state: u32,
    active_connection: Option<String>,
    authentication_failed: bool,
    active_access_point: Option<String>,
    access_points: Vec<RawAccessPoint>,
}

#[derive(Clone, Debug, Default)]
struct RawProfile {
    path: String,
    name: String,
    kind: String,
    uuid: String,
    ssid: Option<Vec<u8>>,
    security: Option<String>,
    autoconnect: Option<bool>,
    last_used: Option<u64>,
}

#[derive(Clone, Debug)]
struct RawNetwork {
    wireless_enabled: bool,
    devices: Vec<RawDevice>,
    profiles: Vec<RawProfile>,
    active_connections: Vec<(String, String)>,
    /// NetworkManager `ActiveConnection.State` of active VPN profiles only, by active path.
    vpn_states: HashMap<String, u32>,
    primary_connection: Option<String>,
}

#[cfg(test)]
async fn read_snapshot(
    connection: &zbus::Connection,
    ids: &mut IdentityTable,
) -> std::result::Result<NetworkState, CoreError> {
    let (state, candidate_ids, _) = read_snapshot_candidate(connection, ids).await?;
    *ids = candidate_ids;
    Ok(state)
}

async fn read_snapshot_candidate(
    connection: &zbus::Connection,
    ids: &IdentityTable,
) -> std::result::Result<(NetworkState, IdentityTable, VpnCandidate), CoreError> {
    let mut candidate_ids = ids.clone();
    let raw = read_raw(connection).await?;
    let state = project(raw.clone(), &mut candidate_ids)?;
    if !CoreSnapshot::Network(state.clone()).valid() {
        return Err(CoreError::Invalid);
    }
    let vpn = project_vpn(&raw, &candidate_ids);
    if !CoreSnapshot::Vpn(vpn.state.clone()).valid() {
        return Err(CoreError::Invalid);
    }
    Ok((state, candidate_ids, vpn))
}

async fn read_raw(connection: &zbus::Connection) -> std::result::Result<RawNetwork, CoreError> {
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};

    let wireless_enabled =
        property::<bool>(connection, NM_MANAGER, NM_SERVICE, "WirelessEnabled").await?;
    let device_paths: Vec<OwnedObjectPath> =
        call(connection, NM_MANAGER, NM_SERVICE, "GetDevices", &()).await?;
    let active_paths: Vec<OwnedObjectPath> =
        property(connection, NM_MANAGER, NM_SERVICE, "ActiveConnections").await?;
    let primary_path: OwnedObjectPath =
        property(connection, NM_MANAGER, NM_SERVICE, "PrimaryConnection").await?;

    if device_paths.len() > MAX_ENTITIES || active_paths.len() > MAX_ENTITIES {
        return Err(CoreError::TooLarge);
    }

    let mut devices = Vec::with_capacity(device_paths.len());
    let mut access_point_count = 0usize;
    for path in device_paths {
        let object = path.as_str().to_owned();
        let kind = property::<u32>(
            connection,
            &object,
            "org.freedesktop.NetworkManager.Device",
            "DeviceType",
        )
        .await?;
        let interface = display_text(
            &property::<String>(
                connection,
                &object,
                "org.freedesktop.NetworkManager.Device",
                "Interface",
            )
            .await?,
            64,
        );
        let state = property::<u32>(
            connection,
            &object,
            "org.freedesktop.NetworkManager.Device",
            "State",
        )
        .await?;
        let active_connection = optional_path(
            property::<OwnedObjectPath>(
                connection,
                &object,
                "org.freedesktop.NetworkManager.Device",
                "ActiveConnection",
            )
            .await?,
        );

        let (active_access_point, points) = if kind == 2 {
            let active = property::<OwnedObjectPath>(
                connection,
                &object,
                "org.freedesktop.NetworkManager.Device.Wireless",
                "ActiveAccessPoint",
            )
            .await?;
            let points: Vec<OwnedObjectPath> = property(
                connection,
                &object,
                "org.freedesktop.NetworkManager.Device.Wireless",
                "AccessPoints",
            )
            .await?;
            access_point_count = access_point_count
                .checked_add(points.len())
                .ok_or(CoreError::TooLarge)?;
            if access_point_count > MAX_ENTITIES {
                return Err(CoreError::TooLarge);
            }
            (optional_path(active), points)
        } else {
            (None, Vec::new())
        };

        let mut access_points = Vec::with_capacity(points.len());
        for point in points {
            let point_path = point.as_str().to_owned();
            let ssid: Vec<u8> = property(
                connection,
                &point_path,
                "org.freedesktop.NetworkManager.AccessPoint",
                "Ssid",
            )
            .await?;
            if ssid.len() > 32 {
                return Err(CoreError::Invalid);
            }
            access_points.push(RawAccessPoint {
                ssid,
                strength: property(
                    connection,
                    &point_path,
                    "org.freedesktop.NetworkManager.AccessPoint",
                    "Strength",
                )
                .await?,
                flags: property(
                    connection,
                    &point_path,
                    "org.freedesktop.NetworkManager.AccessPoint",
                    "Flags",
                )
                .await?,
                wpa_flags: property(
                    connection,
                    &point_path,
                    "org.freedesktop.NetworkManager.AccessPoint",
                    "WpaFlags",
                )
                .await?,
                rsn_flags: property(
                    connection,
                    &point_path,
                    "org.freedesktop.NetworkManager.AccessPoint",
                    "RsnFlags",
                )
                .await?,
                // Optional detail: a missing or odd reading never fails the state.
                frequency: property::<u32>(
                    connection,
                    &point_path,
                    "org.freedesktop.NetworkManager.AccessPoint",
                    "Frequency",
                )
                .await
                .ok()
                .filter(|mhz| (1..=100_000).contains(mhz)),
                path: point_path,
            });
        }
        let authentication_failed = state == 120
            && property::<(u32, u32)>(
                connection,
                &object,
                "org.freedesktop.NetworkManager.Device",
                "StateReason",
            )
            .await
            .is_ok_and(|(_, reason)| reason == 7);
        devices.push(RawDevice {
            path: object,
            interface,
            kind,
            state,
            authentication_failed,
            active_connection,
            active_access_point,
            access_points,
        });
    }

    let settings_paths: Vec<OwnedObjectPath> = call(
        connection,
        "/org/freedesktop/NetworkManager/Settings",
        "org.freedesktop.NetworkManager.Settings",
        "ListConnections",
        &(),
    )
    .await?;
    if settings_paths.len() > MAX_ENTITIES {
        return Err(CoreError::TooLarge);
    }
    let mut profiles = Vec::with_capacity(settings_paths.len());
    for path in settings_paths {
        // NetworkManager GetSettings omits secrets; never call GetSecrets.
        let reply = call_raw(
            connection,
            path.as_str(),
            "org.freedesktop.NetworkManager.Settings.Connection",
            "GetSettings",
            &(),
        )
        .await?;
        let settings: HashMap<String, HashMap<String, OwnedValue>> = decode_reply(reply)?;
        let connection_settings = settings.get("connection").ok_or(CoreError::Unavailable)?;
        let name = display_text(&variant_string(connection_settings.get("id"))?, 128);
        let kind = display_text(&variant_string(connection_settings.get("type"))?, 64);
        let uuid = connection_settings
            .get("uuid")
            .and_then(|v| v.downcast_ref::<&str>().ok())
            .map(str::to_owned)
            .unwrap_or_default();
        // Saved Wi-Fi details (N0042). GetSettings never returns secret values.
        let get = |section: &str, key: &str| settings.get(section)?.get(key)?.try_clone().ok();
        let wifi = kind == "802-11-wireless";
        let security = wifi.then(|| match settings.get("802-11-wireless-security") {
            None => "open",
            Some(section) => match section
                .get("key-mgmt")
                .and_then(|v| v.downcast_ref::<&str>().ok())
            {
                Some("none") => "wep",
                Some("wpa-psk") => "wpa_psk",
                Some("sae") => "sae",
                Some("wpa-eap" | "ieee8021x" | "wpa-eap-suite-b-192") => "eap",
                _ => "other",
            },
        });
        profiles.push(RawProfile {
            path: path.as_str().to_owned(),
            name,
            ssid: wifi
                .then(|| get("802-11-wireless", "ssid").and_then(|v| Vec::<u8>::try_from(v).ok()))
                .flatten()
                .filter(|ssid| ssid.len() <= 32),
            security: security.map(Into::into),
            autoconnect: wifi.then(|| {
                get("connection", "autoconnect")
                    .and_then(|v| bool::try_from(v).ok())
                    .unwrap_or(true)
            }),
            last_used: get("connection", "timestamp")
                .and_then(|v| u64::try_from(v).ok())
                .filter(|seconds| (1..=MAX_SAFE_INTEGER).contains(seconds)),
            uuid,
            kind,
        });
    }

    let mut active_connections = Vec::with_capacity(active_paths.len());
    let mut vpn_states = HashMap::new();
    for path in active_paths {
        let saved: OwnedObjectPath = property(
            connection,
            path.as_str(),
            "org.freedesktop.NetworkManager.Connection.Active",
            "Connection",
        )
        .await?;
        if saved.as_str() != "/" {
            if profiles
                .iter()
                .any(|p| p.path == saved.as_str() && matches!(p.kind.as_str(), "vpn" | "wireguard"))
            {
                let state = property::<u32>(
                    connection,
                    path.as_str(),
                    "org.freedesktop.NetworkManager.Connection.Active",
                    "State",
                )
                .await?;
                vpn_states.insert(path.as_str().to_owned(), state);
            }
            active_connections.push((path.as_str().to_owned(), saved.as_str().to_owned()));
        }
    }

    Ok(RawNetwork {
        wireless_enabled,
        devices,
        profiles,
        active_connections,
        vpn_states,
        primary_connection: optional_path(primary_path),
    })
}

fn project(
    raw: RawNetwork,
    ids: &mut IdentityTable,
) -> std::result::Result<NetworkState, CoreError> {
    let mut seen = HashSet::new();
    let mut devices = empty_entities();
    let mut device_profile = HashMap::<String, Option<String>>::new();
    let mut points = Vec::new();

    if raw.devices.len() > MAX_ENTITIES || raw.profiles.len() > MAX_ENTITIES {
        return Err(CoreError::TooLarge);
    }
    let point_count = raw.devices.iter().try_fold(0usize, |count, device| {
        count
            .checked_add(device.access_points.len())
            .filter(|count| *count <= MAX_ENTITIES)
            .ok_or(CoreError::TooLarge)
    })?;
    if point_count > MAX_ENTITIES {
        return Err(CoreError::TooLarge);
    }
    let authentication_failed = raw
        .devices
        .iter()
        .any(|device| device.authentication_failed);
    for device in raw.devices {
        let id = identity(ids, &mut seen, "device", &device.path)?;
        if devices.items.contains_key(&id) {
            return Err(CoreError::Invalid);
        }
        let profile = device.active_connection.clone();
        device_profile.insert(device.path.clone(), profile);
        devices.order.push(id.clone());
        devices.items.insert(
            id,
            Device {
                name: display_text(&device.interface, 64),
                kind: device_kind(device.kind).into(),
                connected: device.state == 100,
            },
        );
        points.push((
            device.path,
            device.active_access_point,
            device.access_points,
        ));
    }

    let mut profiles = empty_entities();
    let mut profile_ids = HashMap::new();
    for profile in raw.profiles {
        let id = identity(ids, &mut seen, "connection", &profile.path)?;
        if profiles.items.contains_key(&id) {
            return Err(CoreError::Invalid);
        }
        profile_ids.insert(profile.path, id.clone());
        profiles.order.push(id.clone());
        profiles.items.insert(
            id,
            Connection {
                name: display_text(&profile.name, 128),
                kind: connection_kind(&profile.kind),
                active: false,
                ssid: profile
                    .ssid
                    .map(|ssid| display_text(&String::from_utf8_lossy(&ssid), 32)),
                security: profile.security,
                autoconnect: profile.autoconnect,
                last_used: profile.last_used,
            },
        );
    }

    let mut active_profile_ids = HashMap::new();
    for (active_path, profile_path) in raw.active_connections {
        if let Some(id) = profile_ids.get(&profile_path) {
            if let Some(profile) = profiles.items.get_mut(id) {
                profile.active = true;
            }
            active_profile_ids.insert(active_path, id.clone());
        }
    }
    let primary_connection = raw
        .primary_connection
        .as_ref()
        .and_then(|path| active_profile_ids.get(path))
        .filter(|id| {
            profiles
                .items
                .get(*id)
                .is_some_and(|profile| profile.active)
        })
        .cloned();

    let mut access_points = empty_entities();
    for (device_path, active_point, device_points) in points {
        let Some(device_id) = ids.live.get(&identity_key("device", &device_path)).cloned() else {
            return Err(CoreError::Invalid);
        };
        let profile_id = device_profile
            .get(&device_path)
            .and_then(|active| active.as_ref())
            .and_then(|active| active_profile_ids.get(active))
            .cloned();
        for point in device_points {
            let id = identity(ids, &mut seen, "access_point", &point.path)?;
            if access_points.items.contains_key(&id) {
                return Err(CoreError::Invalid);
            }
            let active = active_point.as_deref() == Some(point.path.as_str());
            access_points.order.push(id.clone());
            access_points.items.insert(
                id,
                AccessPoint {
                    device: device_id.clone(),
                    name: display_text(&String::from_utf8_lossy(&point.ssid), 32),
                    strength: point.strength,
                    secured: point.flags & 1 != 0 || point.wpa_flags != 0 || point.rsn_flags != 0,
                    active,
                    connection: if active { profile_id.clone() } else { None },
                    frequency_mhz: point.frequency,
                },
            );
        }
    }

    ids.retain(&seen);
    Ok(NetworkState {
        health: Health {
            available: true,
            stale: false,
            error: authentication_failed.then(|| {
                Failure::new(
                    ErrorCode::Unavailable,
                    "Network authentication required or cancelled",
                    true,
                )
            }),
        },
        wireless_enabled: raw.wireless_enabled,
        devices,
        access_points,
        connections: profiles,
        primary_connection,
    })
}

/// What one snapshot says about VPN profiles, plus the active-connection paths the
/// failure watcher needs to map a NetworkManager signal back to a profile ID.
struct VpnCandidate {
    state: VpnState,
    actives: HashMap<String, EntityId>,
}

/// Saved `vpn` and `wireguard` profiles with the IDs `project` issued for them (N0062).
/// `active` follows `nmcli connection show` (QML): any active connection for the profile.
fn project_vpn(raw: &RawNetwork, ids: &IdentityTable) -> VpnCandidate {
    let mut rows = Vec::new();
    for profile in &raw.profiles {
        if !matches!(profile.kind.as_str(), "vpn" | "wireguard")
            || !nacre_protocol::core_services::uuid_valid(&profile.uuid)
        {
            continue;
        }
        if let Some(id) = ids.live.get(&identity_key("connection", &profile.path)) {
            rows.push((profile, id.clone()));
        }
    }
    rows.sort_by_cached_key(|(profile, _)| (profile.name.to_lowercase(), profile.uuid.clone()));
    let mut actives = HashMap::new();
    let mut active_state = BTreeMap::new();
    for (active_path, profile_path) in &raw.active_connections {
        if let Some((_, id)) = rows
            .iter()
            .find(|(profile, _)| &profile.path == profile_path)
        {
            actives.insert(active_path.clone(), id.clone());
            active_state.insert(
                id.clone(),
                match raw.vpn_states.get(active_path) {
                    Some(2) => VpnConnectionState::Connected,
                    Some(3 | 4) => VpnConnectionState::Disconnecting,
                    _ => VpnConnectionState::Connecting,
                },
            );
        }
    }
    let mut connections = empty_entities();
    for (profile, id) in rows {
        let state = active_state
            .get(&id)
            .copied()
            .unwrap_or(VpnConnectionState::Disconnected);
        connections.order.push(id.clone());
        connections.items.insert(
            id,
            VpnConnection {
                name: display_text(&profile.name, 128),
                kind: profile.kind.clone(),
                uuid: profile.uuid.clone(),
                state,
                active: state != VpnConnectionState::Disconnected,
            },
        );
    }
    VpnCandidate {
        state: VpnState {
            health: Health {
                available: true,
                stale: false,
                error: None,
            },
            connections,
        },
        actives,
    }
}

fn identity(
    ids: &mut IdentityTable,
    seen: &mut HashSet<String>,
    kind: &str,
    source: &str,
) -> std::result::Result<EntityId, CoreError> {
    let key = identity_key(kind, source);
    seen.insert(key);
    ids.id(kind, source)
}

fn identity_key(kind: &str, source: &str) -> String {
    format!("{kind}\0{source}")
}

fn optional_path(path: zbus::zvariant::OwnedObjectPath) -> Option<String> {
    (path.as_str() != "/").then(|| path.as_str().to_owned())
}

fn device_kind(kind: u32) -> &'static str {
    match kind {
        1 => "ethernet",
        2 => "wifi",
        _ => "unknown",
    }
}

fn connection_kind(kind: &str) -> String {
    match kind {
        "802-3-ethernet" => "ethernet".into(),
        "802-11-wireless" => "wifi".into(),
        other => display_text(other, 64),
    }
}

fn display_text(value: &str, max_chars: usize) -> String {
    let mut output = String::new();
    for character in value.chars().take(max_chars) {
        if character == '\n'
            || character == '\t'
            || (!character.is_control()
                && !matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        {
            output.push(character);
        } else {
            output.push_str(&format!("\\u{{{:x}}}", u32::from(character)));
        }
    }
    output
}

async fn property<T>(
    connection: &zbus::Connection,
    path: &str,
    interface: &str,
    name: &str,
) -> std::result::Result<T, CoreError>
where
    T: TryFrom<zbus::zvariant::OwnedValue>,
{
    let reply = call_raw(
        connection,
        path,
        "org.freedesktop.DBus.Properties",
        "Get",
        &(interface, name),
    )
    .await?;
    let value: zbus::zvariant::OwnedValue = decode_reply(reply)?;
    T::try_from(value).map_err(|_| CoreError::Unavailable)
}

async fn call<T, A>(
    connection: &zbus::Connection,
    path: &str,
    interface: &str,
    method: &str,
    args: &A,
) -> std::result::Result<T, CoreError>
where
    T: serde::de::DeserializeOwned + zbus::zvariant::Type,
    A: serde::Serialize + zbus::zvariant::DynamicType,
{
    decode_reply(call_raw(connection, path, interface, method, args).await?)
}

async fn call_raw<A>(
    connection: &zbus::Connection,
    path: &str,
    interface: &str,
    method: &str,
    args: &A,
) -> std::result::Result<zbus::Message, CoreError>
where
    A: serde::Serialize + zbus::zvariant::DynamicType,
{
    let reply = connection
        .call_method(Some(NM_SERVICE), path, Some(interface), method, args)
        .await
        .map_err(|error| match &error {
            zbus::Error::MethodError(name, _, _)
                if matches!(
                    name.as_str().rsplit('.').next(),
                    Some("UnknownObject" | "UnknownDevice" | "UnknownConnection")
                ) =>
            {
                CoreError::StaleTarget
            }
            zbus::Error::MethodError(name, _, _)
                if matches!(
                    name.as_str().rsplit('.').next(),
                    Some("PermissionDenied" | "NotAuthorized" | "AccessDenied" | "AuthFailed")
                ) =>
            {
                CoreError::PermissionDenied
            }
            _ => CoreError::Unavailable,
        })?;
    if reply.body().len() > MAX_REPLY_BYTES {
        return Err(CoreError::TooLarge);
    }
    Ok(reply)
}

async fn network_manager_owner(
    connection: &zbus::Connection,
) -> std::result::Result<String, CoreError> {
    let reply = connection
        .call_method(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            Some("org.freedesktop.DBus"),
            "GetNameOwner",
            &(NM_SERVICE,),
        )
        .await
        .map_err(|_| CoreError::Unavailable)?;
    if reply.body().len() > MAX_REPLY_BYTES {
        return Err(CoreError::TooLarge);
    }
    reply
        .body()
        .deserialize()
        .map_err(|_| CoreError::Unavailable)
}

async fn require_network_manager_owner(
    connection: &zbus::Connection,
    expected: &str,
) -> std::result::Result<(), CoreError> {
    match network_manager_owner(connection).await {
        Ok(owner) if owner == expected => Ok(()),
        Ok(_) | Err(_) => Err(CoreError::OwnerChanged),
    }
}

async fn read_from_expected_owner<T>(
    connection: &zbus::Connection,
    expected: &str,
    read: impl std::future::Future<Output = std::result::Result<T, CoreError>>,
) -> std::result::Result<T, CoreError> {
    require_network_manager_owner(connection, expected).await?;
    let result = read.await;
    require_network_manager_owner(connection, expected).await?;
    result
}

async fn cancel_read_on_owner_change<T>(
    owner_changed: &mut tokio::sync::oneshot::Receiver<()>,
    watcher: &mut AbortWatcher,
    read: impl std::future::Future<Output = std::result::Result<T, CoreError>>,
) -> std::result::Result<T, CoreError> {
    tokio::select! {
        biased;
        changed = owner_changed => Err(if changed.is_ok() {
            CoreError::OwnerChanged
        } else {
            CoreError::Unavailable
        }),
        _ = watcher.wait() => Err(CoreError::Unavailable),
        result = read => result,
    }
}

fn decode_reply<T>(reply: zbus::Message) -> std::result::Result<T, CoreError>
where
    T: serde::de::DeserializeOwned + zbus::zvariant::Type,
{
    reply
        .body()
        .deserialize()
        .map_err(|_| CoreError::Unavailable)
}

fn variant_string(
    value: Option<&zbus::zvariant::OwnedValue>,
) -> std::result::Result<String, CoreError> {
    let value = value
        .ok_or(CoreError::Unavailable)?
        .try_clone()
        .map_err(|_| CoreError::Unavailable)?;
    String::try_from(value).map_err(|_| CoreError::Unavailable)
}

pub fn publish(hub: &Shared, generation: u64, available: bool, error: Option<&str>) -> Result<()> {
    let state = NetworkWatch {
        available,
        generation,
        error: error.map(str::to_owned),
    };
    state.validate()?;
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

pub async fn run(
    hub: Shared,
    generation: Rc<Cell<u64>>,
    identity_generation: Rc<Cell<u64>>,
) -> Result<()> {
    let result = observe(hub.clone(), generation.clone(), identity_generation).await;
    if result.is_err()
        && publish(
            &hub,
            generation.get(),
            false,
            Some("Network monitoring is unavailable"),
        )
        .is_err()
    {
        tracing::error!("Cannot publish network monitor failure");
    }
    result
}

async fn observe(
    hub: Shared,
    generation: Rc<Cell<u64>>,
    identity_generation: Rc<Cell<u64>>,
) -> Result<()> {
    observe_with_hooks(
        hub,
        generation,
        identity_generation,
        ObserveHooks::default(),
    )
    .await
}

async fn observe_with_hooks(
    hub: Shared,
    generation: Rc<Cell<u64>>,
    identity_generation: Rc<Cell<u64>>,
    hooks: ObserveHooks,
) -> Result<()> {
    let mut first_projection = Some(new_projection(&hub, &identity_generation)?);
    let builder = {
        #[cfg(test)]
        if hooks.connection_failure == Some(ConnectionFailure::Builder) {
            None
        } else {
            zbus::connection::Builder::system()
                .map(|builder| builder.method_timeout(Duration::from_secs(5)))
                .ok()
        }
        #[cfg(not(test))]
        {
            zbus::connection::Builder::system()
                .map(|builder| builder.method_timeout(Duration::from_secs(5)))
                .ok()
        }
    };
    let Some(builder) = builder else {
        first_projection
            .as_mut()
            .expect("initial projection is present before connection setup")
            .publish_failure(&hub, CoreError::Unavailable)?;
        return Err("NetworkManager monitoring connection is unavailable".into());
    };
    let connection = {
        #[cfg(test)]
        if hooks.connection_failure == Some(ConnectionFailure::Build) {
            None
        } else {
            match tokio::time::timeout(Duration::from_secs(5), builder.build()).await {
                Ok(Ok(connection)) => Some(connection),
                _ => None,
            }
        }
        #[cfg(not(test))]
        {
            match tokio::time::timeout(Duration::from_secs(5), builder.build()).await {
                Ok(Ok(connection)) => Some(connection),
                _ => None,
            }
        }
    };
    let Some(connection) = connection else {
        first_projection
            .as_mut()
            .expect("initial projection is present before connection setup")
            .publish_failure(&hub, CoreError::Unavailable)?;
        return Err("NetworkManager monitoring connection is unavailable".into());
    };

    // Keep the legacy network.watch stream alive across core identity rotations.
    // It shares this connection with the per-owner reader, so old clients still
    // receive every owner edge without retaining a stale core snapshot reader.
    let (event_sender, mut event_rx) = tokio::sync::watch::channel(0u64);
    let event_sequence = Rc::new(Cell::new(0u64));
    let event_sequence_for_watcher = event_sequence.clone();
    let event_connection = connection.clone();
    let event_hub = hub.clone();
    let event_generation = generation.clone();
    let legacy_watcher = tokio::task::spawn_local(async move {
        let mut running = false;
        crate::network_state::watch_events_tagged(&event_connection, |_event| {
            let next = event_generation
                .get()
                .checked_add(1)
                .filter(|n| *n <= MAX_SAFE_INTEGER)
                .ok_or("Network event generation exhausted")?;
            event_generation.set(next);
            publish(&event_hub, next, true, None)?;
            let sequence = event_sequence_for_watcher
                .get()
                .checked_add(1)
                .ok_or("Network event sequence exhausted")?;
            event_sequence_for_watcher.set(sequence);
            event_sender.send_replace(sequence);
            if !running {
                let failures = event_hub
                    .borrow()
                    .services
                    .get("network-watch")
                    .and_then(|v| v["failures"].as_u64())
                    .unwrap_or(0) as usize;
                event_hub
                    .borrow_mut()
                    .service("network-watch", "running", failures)?;
                running = true;
            }
            Ok(true)
        })
        .await
    });
    let mut legacy_watcher = AbortWatcher(Some(legacy_watcher));
    let mut hooks = Some(hooks);

    let result = async {
        loop {
            tokio::select! {
                watcher = legacy_watcher.wait() => {
                    let _ = watcher;
                    return Err("NetworkManager event watcher stopped".into());
                }
                event = event_rx.changed() => event.map_err(|_| "NetworkManager event watcher stopped")?,
            };
            let mut projection = match first_projection.take() {
                Some(projection) => projection,
                None => new_projection(&hub, &identity_generation)?,
            };
            if network_manager_owner(&connection).await.is_err() {
                projection.publish_failure(&hub, CoreError::Unavailable)?;
                continue;
            }
            let result = observe_on_connection(
                hub.clone(),
                generation.clone(),
                projection,
                connection.clone(),
                hooks.take().unwrap_or_default(),
            )
            .await;
            if result.is_err() && core_reader_failure_is_retryable(&hub) {
                // The legacy watcher owns the bus lifetime; retire only this
                // core reader when its owner pin is unavailable and retry on
                // the next persistent-watcher event.
                continue;
            }
            return result;
        }
    }
    .await;
    legacy_watcher.abort_and_join().await;
    result
}

fn core_reader_failure_is_retryable(hub: &Shared) -> bool {
    hub.borrow()
        .snapshot(CORE_TOPIC)
        .and_then(|value| serde_json::from_value::<NetworkState>(value).ok())
        .is_some_and(|state| {
            !state.health.available
                && state
                    .health
                    .error
                    .is_some_and(|failure| failure.code == ErrorCode::Unavailable)
        })
}

fn new_projection(hub: &Shared, identity_generation: &Cell<u64>) -> Result<NetworkProjection> {
    let last_good = hub
        .borrow()
        .snapshot(CORE_TOPIC)
        .and_then(|state| serde_json::from_value::<NetworkState>(state).ok())
        .filter(|state| state.health.available || state.health.stale);
    let service_generation = next_identity_generation(identity_generation)?;
    let mut projection = NetworkProjection::new(last_good, service_generation);
    use sha2::{Digest, Sha256};
    projection.ids.namespace = format!("{:x}", Sha256::digest(hub.borrow().boot.as_bytes()));
    Ok(projection)
}

#[derive(Default)]
struct ObserveHooks {
    #[cfg(test)]
    connection_failure: Option<ConnectionFailure>,
    #[cfg(test)]
    watcher_dropped: Option<tokio::sync::oneshot::Sender<()>>,
    #[cfg(test)]
    watcher_events: Option<tokio::sync::mpsc::UnboundedSender<crate::network_state::NetworkEvent>>,
    #[cfg(test)]
    snapshot_gate: Option<SnapshotReadGate>,
    #[cfg(test)]
    before_publish_gate: Option<(
        tokio::sync::oneshot::Sender<()>,
        tokio::sync::oneshot::Receiver<()>,
    )>,
    #[cfg(test)]
    publication_witness: Option<tokio::sync::mpsc::UnboundedSender<NetworkState>>,
}

#[cfg(test)]
#[derive(Clone, Copy, Eq, PartialEq)]
enum ConnectionFailure {
    Builder,
    Build,
}

#[cfg(test)]
struct SnapshotReadGate {
    started: tokio::sync::oneshot::Sender<()>,
    release: tokio::sync::oneshot::Receiver<()>,
    resumed: tokio::sync::oneshot::Sender<()>,
}

impl ObserveHooks {
    async fn after_snapshot_read(&mut self) {
        #[cfg(test)]
        if let Some(gate) = self.snapshot_gate.take() {
            let _ = gate.started.send(());
            if gate.release.await.is_ok() {
                let _ = gate.resumed.send(());
            }
        }
        #[cfg(not(test))]
        let _ = self;
    }

    async fn before_publish(&mut self) {
        #[cfg(test)]
        if let Some((started, release)) = self.before_publish_gate.take() {
            let _ = started.send(());
            let _ = release.await;
        }
        #[cfg(not(test))]
        let _ = self;
    }
}

#[cfg(test)]
struct WatcherDropSignal(Option<tokio::sync::oneshot::Sender<()>>);

#[cfg(test)]
impl Drop for WatcherDropSignal {
    fn drop(&mut self) {
        if let Some(signal) = self.0.take() {
            let _ = signal.send(());
        }
    }
}

async fn observe_on_connection(
    hub: Shared,
    _generation: Rc<Cell<u64>>,
    mut projection: NetworkProjection,
    connection: zbus::Connection,
    mut hooks: ObserveHooks,
) -> Result<()> {
    #[cfg(test)]
    {
        projection.publication_witness = hooks.publication_witness.take();
    }
    let expected_owner = match network_manager_owner(&connection).await {
        Ok(owner) => owner,
        Err(error) => {
            projection.publish_failure(&hub, error)?;
            return Err("NetworkManager monitoring connection is unavailable".into());
        }
    };
    let (refresh, mut refreshes) = tokio::sync::mpsc::channel(1);
    let (owner_changed_tx, mut owner_changed) = tokio::sync::oneshot::channel();
    let owner_boundary = Rc::new(Cell::new(false));
    let (requests, mut actions) = controls::channel(owner_boundary.clone());
    let event_vpn = requests.vpn.clone();
    let authentication_failures =
        Rc::new(std::cell::RefCell::new(std::collections::BTreeMap::new()));
    let event_authentication_failures = authentication_failures.clone();
    let published_devices = Rc::new(std::cell::RefCell::new(HashMap::new()));
    let event_devices = published_devices.clone();
    let event_connection = connection.clone();
    let event_refresh = refresh.clone();
    let event_owner_boundary = owner_boundary.clone();
    let mut owner_changed_tx = Some(owner_changed_tx);
    #[cfg(test)]
    let watcher_dropped = hooks.watcher_dropped.take();
    #[cfg(test)]
    let watcher_events = hooks.watcher_events.take();
    let watcher = tokio::task::spawn_local(async move {
        #[cfg(test)]
        let _drop_signal = WatcherDropSignal(watcher_dropped);
        crate::network_state::watch_events_with_message(&event_connection, |event, message| {
            if let Some(message) = message {
                controls::observe_failure(
                    message,
                    &event_devices.borrow(),
                    &mut event_authentication_failures.borrow_mut(),
                );
                controls::observe_vpn_failure(message, &event_vpn);
            }
            #[cfg(test)]
            if let Some(events) = &watcher_events {
                let _ = events.send(event);
            }
            if event == crate::network_state::NetworkEvent::OwnerChanged {
                event_owner_boundary.set(true);
                if let Some(signal) = owner_changed_tx.take() {
                    let _ = signal.send(());
                }
                return Ok(false);
            }
            if matches!(
                event_refresh.try_send(()),
                Err(tokio::sync::mpsc::error::TrySendError::Closed(()))
            ) {
                return Ok(false);
            }
            Ok(true)
        })
        .await
    });
    let mut watcher = AbortWatcher(Some(watcher));
    drop(refresh);

    hub.borrow_mut().network = Some(requests.clone());
    let _readiness = controls::Readiness(requests.clone());
    let loop_result: Result<()> = async {
        loop {
            let refresh = tokio::select! {
                biased;
                _ = &mut owner_changed => return Err("NetworkManager owner changed or watcher stopped".into()),
                _ = watcher.wait() => return Err("NetworkManager event watcher stopped".into()),
                action = actions.recv() => {
                    let Some(action) = action else { return Err("Network control queue ended".into()); };
                    if action.reply.is_closed() { continue; }
                    let previous_failure = controls::prior_failure(&action, &authentication_failures.borrow());
                    let result = tokio::select! {
                        biased;
                        _ = &mut owner_changed => {
                            let _ = action.reply.send(Err(controls::unavailable()));
                            return Err("NetworkManager owner changed during action".into());
                        }
                        _ = watcher.wait() => {
                            let _ = action.reply.send(Err(controls::unavailable()));
                            return Err("NetworkManager watcher ended during action".into());
                        }
                        result = tokio::time::timeout_at(action.deadline,
                            controls::execute(&connection, &expected_owner, &mut projection.ids, &requests, &action)) => {
                            result.unwrap_or_else(|_| Err(controls::unavailable().dispatch_state(true)))
                        }
                    };
                    if result.is_ok() {
                        controls::clear_admitted_failure(&action, previous_failure.as_ref(), &mut authentication_failures.borrow_mut());
                    }
                    let _ = action.reply.send(result);
                    // Even an ambiguous failure is reconciled, never replayed.
                    Some(())
                }
                refresh = refreshes.recv() => refresh,
            };
            if refresh.is_none() {
                return Err("NetworkManager event watcher stopped".into());
            }

            // A short event debounce collapses NetworkManager's multi-property
            // updates into one coherent, complete snapshot. It remains event-driven.
            tokio::select! {
                _ = &mut owner_changed => return Err("NetworkManager owner changed".into()),
                _ = watcher.wait() => return Err("NetworkManager event watcher stopped".into()),
                _ = tokio::time::sleep(Duration::from_millis(20)) => {}
            }
            while refreshes.try_recv().is_ok() {}

            let snapshot = cancel_read_on_owner_change(
                &mut owner_changed,
                &mut watcher,
                read_from_expected_owner(
                    &connection,
                    &expected_owner,
                    async {
                        let snapshot = tokio::time::timeout(Duration::from_secs(10),
                            read_snapshot_candidate(&connection, &projection.ids)).await
                            .unwrap_or(Err(CoreError::Unavailable));
                        hooks.after_snapshot_read().await;
                        snapshot
                    },
                ),
            )
            .await;
            if matches!(&snapshot, Err(CoreError::OwnerChanged)) {
                owner_boundary.set(true);
                return Err("NetworkManager owner changed during snapshot".into());
            }

            let (mut state, candidate_ids, vpn) = match snapshot {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    projection.publish_failure(&hub, error)?;
                    continue;
                }
            };
            let data = serde_json::to_value(&state)?;
            if let Err(error) = framing::encode(&Message::Snap {
                topic: CORE_TOPIC.into(),
                seq: 0,
                data: data.clone(),
            }) {
                let error = if error.code == ErrorCode::TooLarge {
                    CoreError::TooLarge
                } else {
                    CoreError::Invalid
                };
                projection.publish_failure(&hub, error)?;
                continue;
            }
            let capabilities = controls::capabilities(&connection, &expected_owner).await;
            hooks.before_publish().await;
            tokio::select! {
                biased;
                _ = &mut owner_changed => return Err("NetworkManager owner changed before publish".into()),
                _ = watcher.wait() => return Err("NetworkManager event watcher stopped before publish".into()),
                current = require_network_manager_owner(&connection, &expected_owner) => {
                    if current.is_err() {
                        owner_boundary.set(true);
                        return Err("NetworkManager owner changed before publish".into());
                    }
                }
            }
            if owner_boundary.get() {
                return Err("NetworkManager owner changed before publish".into());
            }
            let failures = authentication_failures.borrow().clone();
            controls::settle_created(&connection, &expected_owner, &requests, &candidate_ids, &state, &failures).await;
            controls::apply_failures(&mut state, &mut authentication_failures.borrow_mut());
            let data = serde_json::to_value(&state)?;
            if let Err(error) = hub.borrow_mut().publish(CORE_TOPIC, data) {
                if error.code == ErrorCode::TooLarge {
                    projection.publish_failure(&hub, CoreError::TooLarge)?;
                    continue;
                }
                return Err(error.into());
            }
            #[cfg(test)]
            if let Some(witness) = &projection.publication_witness {
                let _ = witness.send(state.clone());
            }
            *requests.vpn.actives.borrow_mut() = vpn.actives;
            let mut vpn_state = vpn.state;
            controls::apply_vpn_failures(&mut vpn_state, &requests.vpn);
            hub.borrow_mut()
                .publish(VPN_TOPIC, serde_json::to_value(&vpn_state)?)?;
            projection.ids = candidate_ids;
            *published_devices.borrow_mut() = projection.ids.live.iter().filter_map(|(key, id)| {
                key.strip_prefix("device\0").map(|path| (path.to_owned(), id.clone()))
            }).collect();
            requests.enable(capabilities);
            projection.last_good = Some(state);
        }
    }
    .await;
    watcher.abort_and_join().await;
    projection.publish_failure(
        &hub,
        if owner_boundary.get() {
            CoreError::OwnerChanged
        } else {
            CoreError::Unavailable
        },
    )?;
    loop_result?;
    Err("Network monitor ended".into())
}

struct AbortWatcher(Option<tokio::task::JoinHandle<Result<i32>>>);

impl AbortWatcher {
    async fn wait(&mut self) -> Option<std::result::Result<Result<i32>, tokio::task::JoinError>> {
        let result = self.0.as_mut()?.await;
        self.0.take();
        match &result {
            Ok(Err(_)) => tracing::warn!("NetworkManager event watcher returned an error"),
            Err(error) if !error.is_cancelled() => {
                tracing::warn!("NetworkManager event watcher task failed")
            }
            Ok(Ok(_)) | Err(_) => {}
        }
        Some(result)
    }

    async fn abort_and_join(&mut self) {
        if let Some(task) = self.0.take() {
            task.abort();
            match task.await {
                Ok(Ok(_)) => {}
                Err(error) if error.is_cancelled() => {}
                Ok(Err(_)) | Err(_) => {
                    tracing::warn!("NetworkManager event watcher ended during cleanup");
                }
            }
        }
    }
}

impl Drop for AbortWatcher {
    fn drop(&mut self) {
        if let Some(task) = &self.0 {
            task.abort();
        }
    }
}

fn changed(last: Option<u64>, state: &NetworkWatch) -> Result<u64> {
    state.validate()?;
    if !state.available {
        return Err("Daemon network monitoring is unavailable".into());
    }
    match last {
        Some(last) => state
            .generation
            .checked_sub(last)
            .ok_or_else(|| "Network event generation went backwards".into()),
        None => Ok(1),
    }
}

async fn watch_messages(mut connection: client::Client) -> Result<i32> {
    let mut mirror = Mirror::default();
    mirror.receive(&connection.welcome);
    connection.methods.clear();
    connection.welcome = Message::Ping { id: Id::Integer(0) };
    framing::write(
        &mut connection.writer,
        &Message::Sub {
            topics: vec![TOPIC.into()],
        },
    )
    .await?;
    let mut last = None;
    let mut ready = false;
    loop {
        let read = connection.reader.next();
        let message = if ready {
            read.await?
        } else {
            tokio::time::timeout(Duration::from_secs(10), read).await??
        }
        .ok_or("Daemon network stream ended")?;
        if let Message::Error { err } = &message {
            return Err(err.clone().into());
        }
        match mirror.receive(&message) {
            Update::Resync(topic) => {
                framing::write(
                    &mut connection.writer,
                    &Message::Resync {
                        topics: vec![topic],
                    },
                )
                .await?;
            }
            Update::Changed(topic) if topic == TOPIC => {
                let state: NetworkWatch = serde_json::from_value(mirror.topics[TOPIC].1.clone())?;
                state.validate()?;
                if !ready && !state.available && state.error.is_none() {
                    continue;
                }
                let count = changed(last, &state)?;
                // A coalesced snapshot may cover several signals. Preserve each
                // legacy line without sending one wire frame per source event.
                for _ in 0..count {
                    writeln!(std::io::stdout(), "changed")?;
                }
                std::io::stdout().flush()?;
                last = Some(state.generation);
                ready = true;
            }
            _ => {}
        }
    }
}

pub fn watch() -> Result<Option<i32>> {
    let explicit = std::env::var_os("NACRE_DAEMON_SOCKET").is_some();
    if !explicit && std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|p| !Path::new(&p).is_absolute())
    {
        return Ok(None);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    runtime.block_on(async {
        let mut connection = match client::connect().await {
            Ok(connection) => connection,
            Err(client::ConnectError::Absent) if !explicit => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !matches!(&connection.welcome, Message::Welcome { topics, .. } if topics.get(TOPIC) == Some(&1)) {
            let status = connection.call("daemon.status", json!({}), Some(1000)).await?;
            if !explicit && status["safe"] != true { return Ok(None); }
            return Err("Daemon network watch is unavailable".into());
        }
        tokio::select! {
            result = watch_messages(connection) => result.map(Some),
            closed = client::output_closed() => { closed?; Ok(Some(0)) },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coalesced_changes_preserve_line_count_but_initial_state_only_refreshes_once() {
        let mut state = NetworkWatch {
            available: true,
            generation: 8,
            error: None,
        };
        assert_eq!(changed(None, &state).unwrap(), 1);
        assert_eq!(changed(Some(5), &state).unwrap(), 3);
        assert_eq!(changed(Some(8), &state).unwrap(), 0);
        assert!(changed(Some(9), &state).is_err());
        state.available = false;
        assert!(changed(Some(8), &state).is_err());
        state.available = true;
        state.generation = MAX_SAFE_INTEGER + 1;
        assert!(changed(None, &state).is_err());
    }
}

#[cfg(test)]
mod core_tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };
    use zbus::zvariant::{OwnedObjectPath, OwnedValue};

    const DEVICE: &str = "/org/freedesktop/NetworkManager/Devices/1";
    const AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";
    const PROFILE_ETHERNET: &str = "/org/freedesktop/NetworkManager/Settings/1";
    const PROFILE_WIFI: &str = "/org/freedesktop/NetworkManager/Settings/2";
    const ACTIVE_ETHERNET: &str = "/org/freedesktop/NetworkManager/ActiveConnection/1";
    const ACTIVE_WIFI: &str = "/org/freedesktop/NetworkManager/ActiveConnection/2";

    struct TestBus(Child);

    impl Drop for TestBus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    fn private_bus() -> Result<(tempfile::TempDir, TestBus, String)> {
        let home = tempfile::tempdir()?;
        let mut bus = TestBus(
            Command::new("dbus-daemon")
                .args([
                    "--session",
                    "--nofork",
                    "--print-address",
                    &format!("--address=unix:path={}", home.path().join("bus").display()),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        );
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().ok_or("Missing private bus address")?)
            .read_line(&mut address)?;
        Ok((home, bus, address.trim().to_owned()))
    }

    struct FixtureManager;

    #[zbus::interface(name = "org.freedesktop.NetworkManager")]
    impl FixtureManager {
        fn get_devices(&self) -> Vec<OwnedObjectPath> {
            Vec::new()
        }

        #[zbus(property)]
        fn active_connections(&self) -> Vec<OwnedObjectPath> {
            Vec::new()
        }

        #[zbus(property)]
        fn primary_connection(&self) -> OwnedObjectPath {
            OwnedObjectPath::try_from("/").unwrap()
        }

        #[zbus(property)]
        fn wireless_enabled(&self) -> bool {
            false
        }
    }

    struct FixtureSettings;

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
    impl FixtureSettings {
        fn list_connections(&self) -> Vec<OwnedObjectPath> {
            vec![OwnedObjectPath::try_from(PROFILE_WIFI).unwrap()]
        }
    }

    struct FixtureProfile;

    #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
    impl FixtureProfile {
        fn get_settings(&self) -> HashMap<String, HashMap<String, OwnedValue>> {
            use zbus::zvariant::Value as Variant;
            HashMap::from([(
                "connection".into(),
                HashMap::from([
                    (
                        "id".into(),
                        OwnedValue::try_from(Variant::new("Owner fixture")).unwrap(),
                    ),
                    (
                        "type".into(),
                        OwnedValue::try_from(Variant::new("802-11-wireless")).unwrap(),
                    ),
                ]),
            )])
        }
    }

    async fn owner_fixture(address: &str) -> Result<zbus::Connection> {
        Ok(zbus::connection::Builder::address(address)?
            .name(NM_SERVICE)?
            .serve_at(NM_MANAGER, FixtureManager)?
            .serve_at("/org/freedesktop/NetworkManager/Settings", FixtureSettings)?
            .serve_at(PROFILE_WIFI, FixtureProfile)?
            .build()
            .await?)
    }

    async fn fixture_client(address: &str) -> Result<zbus::Connection> {
        Ok(zbus::connection::Builder::address(address)?
            .method_timeout(Duration::from_secs(3))
            .build()
            .await?)
    }

    async fn fixture_owner(connection: &zbus::Connection) -> Result<String> {
        network_manager_owner(connection)
            .await
            .map_err(|_| "Private fixture owner lookup failed".into())
    }

    struct FixtureSupervisor {
        hub: Shared,
        generation: Rc<Cell<u64>>,
        identity_generation: Rc<Cell<u64>>,
    }

    impl FixtureSupervisor {
        fn new(hub: Shared) -> Self {
            Self {
                hub,
                generation: Rc::new(Cell::new(0)),
                identity_generation: Rc::new(Cell::new(0)),
            }
        }

        fn start(
            &self,
            connection: zbus::Connection,
            hooks: ObserveHooks,
        ) -> tokio::task::JoinHandle<Result<()>> {
            let hub = self.hub.clone();
            let generation = self.generation.clone();
            let identity_generation = self.identity_generation.clone();
            tokio::task::spawn_local(async move {
                let next = generation
                    .get()
                    .checked_add(1)
                    .filter(|value| *value <= MAX_SAFE_INTEGER)
                    .ok_or("Network event generation exhausted")?;
                publish(&hub, next, true, None)?;
                generation.set(next);
                let projection = new_projection(&hub, &identity_generation)?;
                observe_on_connection(hub, generation, projection, connection, hooks).await
            })
        }
    }

    async fn wait_for_core_health(
        hub: &Shared,
        notices: &mut tokio::sync::broadcast::Receiver<super::super::hub::Notice>,
        available: bool,
        stale: bool,
    ) -> NetworkState {
        loop {
            let notice = notices
                .recv()
                .await
                .expect("private hub observer remains open");
            if !matches!(
                notice,
                super::super::hub::Notice::Changed { ref topic, .. } if topic == CORE_TOPIC
            ) {
                continue;
            }
            let Some(snapshot) = hub.borrow().snapshot(CORE_TOPIC) else {
                continue;
            };
            let state: NetworkState =
                serde_json::from_value(snapshot).expect("valid network topic");
            if state.health.available == available && state.health.stale == stale {
                return state;
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn core_network_connection_failure_retires_last_good_snapshot() -> Result<()> {
        for failure in [ConnectionFailure::Builder, ConnectionFailure::Build] {
            let mut ids = IdentityTable::new(1);
            let last_good = project(raw_fixture(), &mut ids)
                .map_err(|_| "Private raw network fixture failed")?;
            let hub = super::super::hub::Hub::new(
                "connection-failure-test".into(),
                false,
                Duration::from_millis(16),
            )
            .shared();
            hub.borrow_mut()
                .publish(CORE_TOPIC, serde_json::to_value(last_good.clone())?)?;
            publish(&hub, 7, true, None)?;
            let previous_watch = hub
                .borrow()
                .snapshot(TOPIC)
                .expect("network watch snapshot");
            let identity_generation = Rc::new(Cell::new(41));

            let result = observe_with_hooks(
                hub.clone(),
                Rc::new(Cell::new(7)),
                identity_generation.clone(),
                ObserveHooks {
                    connection_failure: Some(failure),
                    ..ObserveHooks::default()
                },
            )
            .await;
            assert!(result.is_err());

            let unavailable: NetworkState = serde_json::from_value(
                hub.borrow()
                    .snapshot(CORE_TOPIC)
                    .expect("core network snapshot"),
            )?;
            assert!(!unavailable.health.available);
            assert!(unavailable.health.stale);
            assert_eq!(unavailable.devices, last_good.devices);
            assert_eq!(unavailable.access_points, last_good.access_points);
            assert_eq!(unavailable.connections, last_good.connections);
            assert_eq!(
                unavailable.health.error.expect("unavailable failure").code,
                ErrorCode::Unavailable
            );
            assert_eq!(identity_generation.get(), 42);
            assert_eq!(hub.borrow().snapshot(TOPIC), Some(previous_watch));
        }
        Ok(())
    }

    #[test]
    fn core_network_service_generation_advances_before_each_observer() {
        let generation = Cell::new(0);
        assert_eq!(next_identity_generation(&generation).unwrap(), 1);
        assert_eq!(next_identity_generation(&generation).unwrap(), 2);
        generation.set(MAX_SAFE_INTEGER);
        assert!(next_identity_generation(&generation).is_err());
        assert_eq!(generation.get(), MAX_SAFE_INTEGER);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn core_network_owner_loss_between_precheck_and_reader_start_is_retryable() -> Result<()>
    {
        let (_home, _bus, address) = private_bus()?;
        tokio::task::LocalSet::new()
            .run_until(async {
                let server = owner_fixture(&address).await?;
                let client = fixture_client(&address).await?;
                let pinned_owner = network_manager_owner(&client)
                    .await
                    .map_err(|_| "Initial private owner precheck failed")?;
                let hub = super::super::hub::Hub::new(
                    "owner-precheck-race-test".into(),
                    false,
                    Duration::from_millis(16),
                )
                .shared();
                let mut ids = IdentityTable::new(40);
                let last_good = project(raw_fixture(), &mut ids)
                    .map_err(|_| "Private raw network fixture failed")?;
                hub.borrow_mut()
                    .publish(CORE_TOPIC, serde_json::to_value(last_good.clone())?)?;
                publish(&hub, 7, true, None)?;
                let previous_watch = hub.borrow().snapshot(TOPIC).expect("network watch state");
                let identity_generation = Rc::new(Cell::new(41));
                let projection = new_projection(&hub, &identity_generation)?;

                // Force the same boundary between observe's owner precheck and
                // observe_on_connection's independent reader-start owner pin.
                assert!(server.release_name(NM_SERVICE).await?);
                assert!(network_manager_owner(&client).await.is_err());
                let result = observe_on_connection(
                    hub.clone(),
                    Rc::new(Cell::new(7)),
                    projection,
                    client,
                    ObserveHooks::default(),
                )
                .await;
                assert!(result.is_err());
                assert!(core_reader_failure_is_retryable(&hub));

                let unavailable: NetworkState = serde_json::from_value(
                    hub.borrow()
                        .snapshot(CORE_TOPIC)
                        .expect("core network state after owner loss"),
                )?;
                assert!(!unavailable.health.available);
                assert!(unavailable.health.stale);
                assert_eq!(unavailable.devices, last_good.devices);
                assert_eq!(unavailable.access_points, last_good.access_points);
                assert_eq!(unavailable.connections, last_good.connections);
                assert_eq!(identity_generation.get(), 42);
                assert_eq!(hub.borrow().snapshot(TOPIC), Some(previous_watch));
                assert_ne!(pinned_owner, "");
                Ok::<(), crate::Error>(())
            })
            .await?;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn core_network_owner_replacement_retires_same_path_ids() -> Result<()> {
        let (_home, _bus, address) = private_bus()?;
        tokio::time::timeout(
            Duration::from_secs(20),
            tokio::task::LocalSet::new().run_until(async {
                let old_server = owner_fixture(&address).await?;
                let client = fixture_client(&address).await?;
                let old_owner = fixture_owner(&client).await?;
                let hub = super::super::hub::Hub::new(
                    "owner-replacement-test".into(),
                    false,
                    Duration::from_millis(16),
                )
                .shared();
                publish_core_starting(&hub)?;
                let mut notices = hub.borrow().observe();
                let supervisor = FixtureSupervisor::new(hub.clone());
                let old_task = supervisor.start(client.clone(), ObserveHooks::default());
                let old_state = wait_for_core_health(&hub, &mut notices, true, false).await;
                let old_id = old_state.connections.order[0].clone();
                assert!(CoreSnapshot::Network(old_state.clone()).valid());

                assert!(old_server.release_name(NM_SERVICE).await?);
                let stale = wait_for_core_health(&hub, &mut notices, false, true).await;
                assert_eq!(stale.connections.order, old_state.connections.order);
                assert!(old_task.await?.is_err());

                let new_server = owner_fixture(&address).await?;
                let new_client = fixture_client(&address).await?;
                let new_owner = fixture_owner(&new_client).await?;
                assert_ne!(old_owner, new_owner);
                let new_task = supervisor.start(new_client, ObserveHooks::default());
                let new_state = wait_for_core_health(&hub, &mut notices, true, false).await;
                assert_eq!(
                    new_state.connections.items[&new_state.connections.order[0]].name,
                    old_state.connections.items[&old_state.connections.order[0]].name
                );
                assert_ne!(old_id, new_state.connections.order[0]);
                assert!(CoreSnapshot::Network(new_state.clone()).valid());

                assert!(new_server.release_name(NM_SERVICE).await?);
                let _ = wait_for_core_health(&hub, &mut notices, false, true).await;
                assert!(new_task.await?.is_err());
                Ok::<(), crate::Error>(())
            }),
        )
        .await
        .map_err(|_| "N5 private network lifecycle fixture timed out")??;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn core_network_old_owner_snapshot_never_publishes_after_replacement() -> Result<()> {
        let (_home, _bus, address) = private_bus()?;
        tokio::time::timeout(
            Duration::from_secs(20),
            tokio::task::LocalSet::new().run_until(async {
                let old_server = owner_fixture(&address).await?;
                let client = fixture_client(&address).await?;
                let old_owner = fixture_owner(&client).await?;
                let (last_good, _, _) = read_snapshot_candidate(&client, &IdentityTable::new(99))
                    .await
                    .map_err(|_| "Initial private snapshot failed")?;
                let hub = super::super::hub::Hub::new(
                    "owner-replacement-test".into(),
                    false,
                    Duration::from_millis(16),
                )
                .shared();
                hub.borrow_mut()
                    .publish(CORE_TOPIC, serde_json::to_value(last_good.clone())?)?;
                let mut notices = hub.borrow().observe();
                let supervisor = FixtureSupervisor::new(hub.clone());
                let (read_started_tx, read_started_rx) = tokio::sync::oneshot::channel();
                let (read_release_tx, read_release_rx) = tokio::sync::oneshot::channel();
                let (read_resumed_tx, read_resumed_rx) = tokio::sync::oneshot::channel();
                let (read_events_tx, mut read_events_rx) = tokio::sync::mpsc::unbounded_channel();
                let (published_tx, mut published_rx) = tokio::sync::mpsc::unbounded_channel();
                let hooks = ObserveHooks {
                    snapshot_gate: Some(SnapshotReadGate {
                        started: read_started_tx,
                        release: read_release_rx,
                        resumed: read_resumed_tx,
                    }),
                    watcher_events: Some(read_events_tx),
                    publication_witness: Some(published_tx),
                    ..ObserveHooks::default()
                };
                let old_task = supervisor.start(client, hooks);
                assert_eq!(
                    read_events_rx.recv().await,
                    Some(crate::network_state::NetworkEvent::Initial)
                );
                read_started_rx.await?;

                // The real observer has completed its D-Bus snapshot read, but
                // its owner-checked read future is held before it can commit.
                assert!(old_server.release_name(NM_SERVICE).await?);
                let replacement = owner_fixture(&address).await?;
                let replacement_client = fixture_client(&address).await?;
                let replacement_owner = fixture_owner(&replacement_client).await?;
                assert_ne!(old_owner, replacement_owner);
                assert_eq!(
                    read_events_rx.recv().await,
                    Some(crate::network_state::NetworkEvent::OwnerChanged)
                );
                let _ = read_release_tx.send(());
                assert!(read_resumed_rx.await.is_err());
                let stale = wait_for_core_health(&hub, &mut notices, false, true).await;
                assert!(old_task.await?.is_err());
                assert_eq!(stale.connections.order, last_good.connections.order);
                assert!(!stale.health.available);
                let witnessed: Vec<_> =
                    std::iter::from_fn(|| published_rx.try_recv().ok()).collect();
                assert!(
                    witnessed
                        .iter()
                        .any(|state| !state.health.available && state.health.stale),
                    "owner loss publishes stale health"
                );
                assert!(
                    witnessed.iter().all(|state| !state.health.available),
                    "old-owner candidate never publishes as healthy"
                );

                // Start a replacement generation and hold its freshly read
                // candidate immediately before the final synchronous publish.
                while notices.try_recv().is_ok() {}
                let (publish_started_tx, publish_started_rx) = tokio::sync::oneshot::channel();
                let (publish_release_tx, publish_release_rx) = tokio::sync::oneshot::channel();
                let (replacement_published_tx, mut replacement_published_rx) =
                    tokio::sync::mpsc::unbounded_channel();
                let replacement_task = supervisor.start(
                    replacement_client.clone(),
                    ObserveHooks {
                        before_publish_gate: Some((publish_started_tx, publish_release_rx)),
                        publication_witness: Some(replacement_published_tx),
                        ..ObserveHooks::default()
                    },
                );
                publish_started_rx.await?;
                let second_owner = fixture_owner(&replacement_client).await?;
                assert!(replacement.release_name(NM_SERVICE).await?);
                let next_server = owner_fixture(&address).await?;
                let latest_owner = fixture_owner(&fixture_client(&address).await?).await?;
                assert_ne!(second_owner, latest_owner);
                publish_release_tx
                    .send(())
                    .map_err(|_| "publish gate was dropped")?;
                assert!(replacement_task.await?.is_err());
                let stale: NetworkState = serde_json::from_value(
                    hub.borrow()
                        .snapshot(CORE_TOPIC)
                        .expect("stale network snapshot"),
                )?;
                assert!(!stale.health.available);
                assert!(stale.health.stale);
                assert_eq!(stale.connections.order, last_good.connections.order);
                let replacement_witnessed: Vec<_> =
                    std::iter::from_fn(|| replacement_published_rx.try_recv().ok()).collect();
                assert!(
                    replacement_witnessed
                        .iter()
                        .any(|state| !state.health.available && state.health.stale),
                    "replacement owner loss publishes stale health"
                );
                assert!(
                    replacement_witnessed
                        .iter()
                        .all(|state| !state.health.available),
                    "replacement owner candidate never publishes as healthy"
                );
                drop(next_server);
                Ok::<(), crate::Error>(())
            }),
        )
        .await
        .map_err(|_| "N5 private network lifecycle fixture timed out")??;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn core_network_service_exit_drops_signal_watcher() -> Result<()> {
        let (_home, _bus, address) = private_bus()?;
        tokio::time::timeout(
            Duration::from_secs(20),
            tokio::task::LocalSet::new().run_until(async {
                let server = owner_fixture(&address).await?;
                let client = fixture_client(&address).await?;
                let hub = super::super::hub::Hub::new(
                    "watcher-lifecycle-test".into(),
                    false,
                    Duration::from_millis(16),
                )
                .shared();
                publish_core_starting(&hub)?;
                let mut notices = hub.borrow().observe();
                let supervisor = FixtureSupervisor::new(hub.clone());
                let (error_dropped_tx, error_dropped_rx) = tokio::sync::oneshot::channel();
                let (error_events_tx, mut error_events_rx) = tokio::sync::mpsc::unbounded_channel();
                let error_task = supervisor.start(
                    client.clone(),
                    ObserveHooks {
                        watcher_dropped: Some(error_dropped_tx),
                        watcher_events: Some(error_events_tx),
                        ..ObserveHooks::default()
                    },
                );
                assert_eq!(
                    error_events_rx.recv().await,
                    Some(crate::network_state::NetworkEvent::Initial)
                );
                let _ = wait_for_core_health(&hub, &mut notices, true, false).await;
                assert!(server.release_name(NM_SERVICE).await?);
                assert_eq!(
                    error_events_rx.recv().await,
                    Some(crate::network_state::NetworkEvent::OwnerChanged)
                );
                let _ = wait_for_core_health(&hub, &mut notices, false, true).await;
                assert!(error_task.await?.is_err());
                error_dropped_rx.await?;

                let server = owner_fixture(&address).await?;
                let client = fixture_client(&address).await?;
                let (cancel_dropped_tx, cancel_dropped_rx) = tokio::sync::oneshot::channel();
                let (cancel_events_tx, mut cancel_events_rx) =
                    tokio::sync::mpsc::unbounded_channel();
                let cancel_task = supervisor.start(
                    client.clone(),
                    ObserveHooks {
                        watcher_dropped: Some(cancel_dropped_tx),
                        watcher_events: Some(cancel_events_tx),
                        ..ObserveHooks::default()
                    },
                );
                assert_eq!(
                    cancel_events_rx.recv().await,
                    Some(crate::network_state::NetworkEvent::Initial)
                );
                let _ = wait_for_core_health(&hub, &mut notices, true, false).await;
                let before = hub.borrow().snapshot(TOPIC).expect("network watch topic")
                    ["generation"]
                    .as_u64()
                    .expect("network watch generation");
                cancel_task.abort();
                let cancellation = match cancel_task.await {
                    Err(error) => error,
                    Ok(_) => panic!("cancelled observer unexpectedly returned normally"),
                };
                assert!(cancellation.is_cancelled());
                cancel_dropped_rx.await?;
                server
                    .emit_signal(
                        None::<&str>,
                        NM_MANAGER,
                        NM_SERVICE,
                        "DeviceAdded",
                        &(OwnedObjectPath::try_from(DEVICE)?,),
                    )
                    .await?;
                let _ = client
                    .call_method(
                        Some(NM_SERVICE),
                        NM_MANAGER,
                        Some(NM_SERVICE),
                        "GetDevices",
                        &(),
                    )
                    .await?;
                assert!(cancel_events_rx.try_recv().is_err());
                let after = hub.borrow().snapshot(TOPIC).expect("network watch topic")
                    ["generation"]
                    .as_u64()
                    .expect("network watch generation");
                assert_eq!(before, after, "cancelled observer left no live watcher");
                Ok::<(), crate::Error>(())
            }),
        )
        .await
        .map_err(|_| "N5 private network lifecycle fixture timed out")??;
        Ok(())
    }

    fn raw_fixture() -> RawNetwork {
        RawNetwork {
            wireless_enabled: true,
            devices: vec![RawDevice {
                path: DEVICE.into(),
                interface: "wlan0".into(),
                kind: 2,
                state: 100,
                authentication_failed: false,
                active_connection: Some(ACTIVE_WIFI.into()),
                active_access_point: Some(AP.into()),
                access_points: vec![RawAccessPoint {
                    path: AP.into(),
                    ssid: b"Fixture Wi-Fi".to_vec(),
                    strength: 73,
                    flags: 1,
                    wpa_flags: 0,
                    rsn_flags: 0,
                    frequency: Some(5180),
                }],
            }],
            profiles: vec![
                RawProfile {
                    path: PROFILE_ETHERNET.into(),
                    name: "Wired fixture".into(),
                    kind: "802-3-ethernet".into(),
                    ..Default::default()
                },
                RawProfile {
                    path: PROFILE_WIFI.into(),
                    name: "Wireless fixture".into(),
                    kind: "802-11-wireless".into(),
                    ..Default::default()
                },
            ],
            active_connections: vec![
                (ACTIVE_ETHERNET.into(), PROFILE_ETHERNET.into()),
                (ACTIVE_WIFI.into(), PROFILE_WIFI.into()),
            ],
            vpn_states: HashMap::new(),
            primary_connection: Some(ACTIVE_WIFI.into()),
        }
    }

    #[test]
    fn core_network_ids_survive_update() {
        let mut ids = IdentityTable::new(1);
        let first = project(raw_fixture(), &mut ids).unwrap();
        let second = project(raw_fixture(), &mut ids).unwrap();
        assert_eq!(first.devices.order, second.devices.order);
        assert_eq!(first.access_points.order, second.access_points.order);
        assert_eq!(first.connections.order, second.connections.order);
    }

    #[test]
    fn core_network_removed_ids_never_reused() {
        let mut ids = IdentityTable::new(1);
        let original = project(raw_fixture(), &mut ids).unwrap();
        let original_device = original.devices.order[0].clone();
        let original_point = original.access_points.order[0].clone();
        let original_profile = original.connections.order[0].clone();
        let removed = project(
            RawNetwork {
                devices: Vec::new(),
                profiles: Vec::new(),
                active_connections: Vec::new(),
                ..raw_fixture()
            },
            &mut ids,
        )
        .unwrap();
        assert!(removed.devices.items.is_empty());
        assert!(removed.access_points.items.is_empty());
        assert!(removed.connections.items.is_empty());
        let returned = project(raw_fixture(), &mut ids).unwrap();
        assert_ne!(original_device, returned.devices.order[0]);
        assert_ne!(original_point, returned.access_points.order[0]);
        assert_ne!(original_profile, returned.connections.order[0]);
        assert!(returned.devices.order[0].valid());
        assert!(returned.access_points.order[0].valid());
        assert!(returned.connections.order[0].valid());
    }

    #[test]
    fn core_network_primary_uses_active_mapping() {
        let mut ids = IdentityTable::new(1);
        let mut raw = raw_fixture();
        raw.active_connections.reverse();
        let state = project(raw, &mut ids).unwrap();
        let primary = state
            .primary_connection
            .as_ref()
            .expect("mapped primary profile");
        assert_eq!(state.connections.items[primary].name, "Wireless fixture");
        assert!(state.connections.items[primary].active);
        assert!(CoreSnapshot::Network(state).valid());

        let mut unresolved = raw_fixture();
        unresolved.primary_connection =
            Some("/org/freedesktop/NetworkManager/ActiveConnection/missing".into());
        let state = project(unresolved, &mut ids).unwrap();
        assert!(state.primary_connection.is_none());
        assert!(CoreSnapshot::Network(state).valid());
    }

    #[test]
    fn core_network_ap_device_references_are_valid() {
        let mut ids = IdentityTable::new(1);
        let state = project(raw_fixture(), &mut ids).unwrap();
        assert!(CoreSnapshot::Network(state.clone()).valid());
        let point = &state.access_points.items[&state.access_points.order[0]];
        assert!(state.devices.items.contains_key(&point.device));
        assert_eq!(point.connection.as_ref(), state.primary_connection.as_ref());
        assert_eq!(point.frequency_mhz, Some(5180));
    }

    #[test]
    fn core_network_display_text_escapes_controls_and_bidi_markers() {
        assert_eq!(
            display_text("Office\u{202e}\u{0001}\nWi-Fi", 64),
            "Office\\u{202e}\\u{1}\nWi-Fi"
        );
    }

    #[test]
    fn core_network_backend_loss_marks_last_good_stale() {
        let mut ids = IdentityTable::new(1);
        let good = project(raw_fixture(), &mut ids).unwrap();
        let hub = super::super::hub::Hub::new(
            "network-state-test".into(),
            false,
            Duration::from_millis(16),
        )
        .shared();
        hub.borrow_mut()
            .publish(CORE_TOPIC, serde_json::to_value(good.clone()).unwrap())
            .unwrap();
        let mut projection = NetworkProjection::new(Some(good.clone()), 2);
        projection
            .publish_failure(&hub, CoreError::Unavailable)
            .unwrap();
        let stale: NetworkState =
            serde_json::from_value(hub.borrow().snapshot(CORE_TOPIC).expect("network snapshot"))
                .unwrap();
        assert!(stale.health.stale);
        assert!(!stale.health.available);
        assert_eq!(stale.devices, good.devices);
        assert_eq!(stale.wireless_enabled, good.wireless_enabled);
        assert_eq!(stale.health.error.unwrap().code, ErrorCode::Unavailable);
        let initial = unavailable_state(None, CoreError::Unavailable);
        assert!(!initial.health.available);
        assert!(!initial.health.stale);
        assert!(initial.devices.items.is_empty());
    }

    #[test]
    fn core_network_starting_publishes_unavailable_empty_snapshot() {
        let hub = super::super::hub::Hub::new(
            "network-starting-test".into(),
            false,
            Duration::from_millis(16),
        )
        .shared();
        publish_core_starting(&hub).unwrap();
        let state: NetworkState =
            serde_json::from_value(hub.borrow().snapshot(CORE_TOPIC).expect("network snapshot"))
                .unwrap();
        assert!(!state.health.available);
        assert!(!state.health.stale);
        assert!(state.health.error.is_none());
        assert!(state.devices.items.is_empty());
        assert!(state.access_points.items.is_empty());
        assert!(state.connections.items.is_empty());
        assert!(state.primary_connection.is_none());
        assert!(CoreSnapshot::Network(state).valid());
    }

    #[test]
    fn core_network_collection_limit_never_emits_dangling_refs() {
        let too_many_devices = (0..=MAX_ENTITIES)
            .map(|index| RawDevice {
                path: format!("/org/freedesktop/NetworkManager/Devices/{index}"),
                interface: format!("eth{index}"),
                kind: 1,
                state: 30,
                authentication_failed: false,
                active_connection: None,
                active_access_point: None,
                access_points: Vec::new(),
            })
            .collect();
        let too_many_points = (0..=MAX_ENTITIES)
            .map(|index| RawAccessPoint {
                path: format!("/org/freedesktop/NetworkManager/AccessPoint/{index}"),
                ssid: format!("Fixture {index}").into_bytes(),
                strength: 50,
                flags: 0,
                wpa_flags: 0,
                rsn_flags: 0,
                frequency: None,
            })
            .collect();
        let mut raw_points = raw_fixture();
        raw_points.devices[0].access_points = too_many_points;
        let too_many_profiles = (0..=MAX_ENTITIES)
            .map(|index| RawProfile {
                path: format!("/org/freedesktop/NetworkManager/Settings/{index}"),
                name: format!("Profile {index}"),
                kind: "802-3-ethernet".into(),
                ..Default::default()
            })
            .collect();

        for raw in [
            RawNetwork {
                devices: too_many_devices,
                ..raw_fixture()
            },
            raw_points,
            RawNetwork {
                profiles: too_many_profiles,
                ..raw_fixture()
            },
        ] {
            let mut ids = IdentityTable::new(1);
            assert!(matches!(project(raw, &mut ids), Err(CoreError::TooLarge)));
            let unavailable = unavailable_state(None, CoreError::TooLarge);
            assert!(!unavailable.health.available);
            assert!(CoreSnapshot::Network(unavailable.clone()).valid());
            assert!(unavailable.devices.items.is_empty());
            assert!(unavailable.access_points.items.is_empty());
            assert!(unavailable.connections.items.is_empty());
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn core_network_never_scans_or_reads_secrets() -> Result<()> {
        use std::{
            io::{BufRead, BufReader},
            process::{Child, Command, Stdio},
            sync::{
                atomic::{AtomicUsize, Ordering},
                Arc,
            },
        };
        use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value as Variant};

        struct Bus(Child);
        impl Drop for Bus {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        struct Manager;
        #[zbus::interface(name = "org.freedesktop.NetworkManager")]
        impl Manager {
            fn get_devices(&self) -> Vec<OwnedObjectPath> {
                vec![OwnedObjectPath::try_from(DEVICE).unwrap()]
            }
            #[zbus(property)]
            fn active_connections(&self) -> Vec<OwnedObjectPath> {
                vec![OwnedObjectPath::try_from(ACTIVE_WIFI).unwrap()]
            }
            #[zbus(property)]
            fn primary_connection(&self) -> OwnedObjectPath {
                OwnedObjectPath::try_from(ACTIVE_WIFI).unwrap()
            }
            #[zbus(property)]
            fn wireless_enabled(&self) -> bool {
                true
            }
        }
        struct Device;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
        impl Device {
            #[zbus(property)]
            fn device_type(&self) -> u32 {
                2
            }
            #[zbus(property)]
            fn interface(&self) -> String {
                "wlan0".into()
            }
            #[zbus(property)]
            fn state(&self) -> u32 {
                100
            }
            #[zbus(property)]
            fn active_connection(&self) -> OwnedObjectPath {
                OwnedObjectPath::try_from(ACTIVE_WIFI).unwrap()
            }
        }
        struct Wireless(Arc<AtomicUsize>);
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
        impl Wireless {
            #[zbus(property)]
            fn active_access_point(&self) -> OwnedObjectPath {
                OwnedObjectPath::try_from(AP).unwrap()
            }
            #[zbus(property)]
            fn access_points(&self) -> Vec<OwnedObjectPath> {
                vec![OwnedObjectPath::try_from(AP).unwrap()]
            }
            fn request_scan(&self, _options: HashMap<String, OwnedValue>) {
                self.0.fetch_add(1, Ordering::SeqCst);
            }
        }
        struct AccessPoint;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
        impl AccessPoint {
            #[zbus(property)]
            fn ssid(&self) -> Vec<u8> {
                b"Fixture Wi-Fi".to_vec()
            }
            #[zbus(property)]
            fn strength(&self) -> u8 {
                73
            }
            #[zbus(property)]
            fn flags(&self) -> u32 {
                1
            }
            #[zbus(property)]
            fn wpa_flags(&self) -> u32 {
                0
            }
            #[zbus(property)]
            fn rsn_flags(&self) -> u32 {
                0
            }
            #[zbus(property)]
            fn frequency(&self) -> u32 {
                5180
            }
        }
        struct Settings;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
        impl Settings {
            fn list_connections(&self) -> Vec<OwnedObjectPath> {
                vec![OwnedObjectPath::try_from(PROFILE_WIFI).unwrap()]
            }
        }
        struct Profile(Arc<AtomicUsize>);
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
        impl Profile {
            fn get_settings(&self) -> HashMap<String, HashMap<String, OwnedValue>> {
                HashMap::from([(
                    "connection".into(),
                    HashMap::from([
                        (
                            "id".into(),
                            OwnedValue::try_from(Variant::new("Wireless fixture")).unwrap(),
                        ),
                        (
                            "type".into(),
                            OwnedValue::try_from(Variant::new("802-11-wireless")).unwrap(),
                        ),
                    ]),
                )])
            }
            fn get_secrets(&self, _setting: &str) -> HashMap<String, HashMap<String, OwnedValue>> {
                self.0.fetch_add(1, Ordering::SeqCst);
                HashMap::new()
            }
        }
        struct Active;
        #[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
        impl Active {
            #[zbus(property)]
            fn connection(&self) -> OwnedObjectPath {
                OwnedObjectPath::try_from(PROFILE_WIFI).unwrap()
            }
        }

        let home = tempfile::tempdir()?;
        let mut bus = Bus(Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address",
                &format!("--address=unix:path={}", home.path().join("bus").display()),
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .stdin(Stdio::null())
            .spawn()?);
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().ok_or("Missing private bus address")?)
            .read_line(&mut address)?;
        let scans = Arc::new(AtomicUsize::new(0));
        let secrets = Arc::new(AtomicUsize::new(0));
        let server = zbus::connection::Builder::address(address.trim())?
            .name(NM_SERVICE)?
            .serve_at(NM_MANAGER, Manager)?
            .serve_at(DEVICE, Device)?
            .serve_at(DEVICE, Wireless(scans.clone()))?
            .serve_at(AP, AccessPoint)?
            .serve_at("/org/freedesktop/NetworkManager/Settings", Settings)?
            .serve_at(PROFILE_WIFI, Profile(secrets.clone()))?
            .serve_at(ACTIVE_WIFI, Active)?
            .build()
            .await?;
        let client = zbus::connection::Builder::address(address.trim())?
            .method_timeout(Duration::from_secs(3))
            .build()
            .await?;
        let mut ids = IdentityTable::new(1);
        let state = read_snapshot(&client, &mut ids)
            .await
            .expect("private NetworkManager snapshot");
        assert!(CoreSnapshot::Network(state.clone()).valid());
        assert_eq!(state.devices.items.len(), 1);
        assert_eq!(state.access_points.items.len(), 1);
        assert_eq!(
            state.access_points.items[&state.access_points.order[0]].frequency_mhz,
            Some(5180)
        );
        assert_eq!(state.connections.items.len(), 1);
        assert_eq!(scans.load(Ordering::SeqCst), 0);
        assert_eq!(secrets.load(Ordering::SeqCst), 0);
        let serialized = serde_json::to_string(&state)?;
        assert!(!serialized.contains(DEVICE));
        assert!(!serialized.contains(AP));
        assert!(!serialized.contains(PROFILE_WIFI));
        drop(client);
        drop(server);
        drop(bus);
        Ok(())
    }
}
