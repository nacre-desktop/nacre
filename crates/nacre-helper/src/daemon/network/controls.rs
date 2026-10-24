//! Admission-only controls on the reader's connection and owner generation.
use super::*;
use nacre_protocol::{
    core_services::{CoreCall, CoreResult, NetworkState, NetworkTarget, VpnState, WifiSecurity},
    Capability, Method,
};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const WIFI: u8 = 1;
const CONNECT: u8 = 2;
const DISCONNECT: u8 = 4;
const FORGET: u8 = 8;
const SCAN: u8 = 16;
const METHODS: [&str; 7] = [
    "network.set_wifi",
    "network.connect",
    "network.disconnect",
    "network.forget",
    "vpn.connect",
    "vpn.disconnect",
    "network.scan",
];
const WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
/// How long `network.scan` waits for NetworkManager to finish; below the method deadline.
const SCAN_WAIT: Duration = Duration::from_secs(7);
const SCAN_POLL: Duration = Duration::from_millis(250);
/// Stable text clients use to re-prompt; the code is the existing `validation`.
pub const SECRET_REQUIRED: &str = "Network secret required";
const WIFI_SETTINGS: &str = "802-11-wireless";
const SECURITY_SETTINGS: &str = "802-11-wireless-security";
const TIMEOUT: u32 = 10_000;

#[derive(Clone)]
pub struct Requests {
    sender: mpsc::Sender<Action>,
    capabilities: Rc<Cell<u8>>,
    owner_boundary: Rc<Cell<bool>>,
    /// Profiles this daemon just created with a secret, by device. A profile that
    /// fails authentication is deleted again so a wrong password is never saved.
    created: Rc<std::cell::RefCell<BTreeMap<EntityId, String>>>,
    pub(super) vpn: Rc<VpnTracking>,
}

/// N0062: which VPN profiles last failed to activate, and which active-connection path
/// belongs to which profile (to map NetworkManager's `StateChanged` signal back).
#[derive(Default)]
pub(super) struct VpnTracking {
    pub(super) failed: std::cell::RefCell<std::collections::BTreeSet<EntityId>>,
    pub(super) actives: std::cell::RefCell<HashMap<String, EntityId>>,
}

pub(super) struct Action {
    call: CoreCall,
    pub(super) deadline: tokio::time::Instant,
    pub(super) reply: oneshot::Sender<std::result::Result<Value, Failure>>,
}

pub(super) fn channel(owner_boundary: Rc<Cell<bool>>) -> (Requests, mpsc::Receiver<Action>) {
    let (sender, receiver) = mpsc::channel(8);
    (
        Requests {
            sender,
            capabilities: Rc::new(Cell::new(0)),
            owner_boundary,
            created: Rc::default(),
            vpn: Rc::default(),
        },
        receiver,
    )
}

pub(super) struct Readiness(pub(super) Requests);
impl Drop for Readiness {
    fn drop(&mut self) {
        self.0.disable();
    }
}

impl Requests {
    pub(super) fn disable(&self) {
        self.capabilities.set(0);
    }
    pub(super) fn enable(&self, capabilities: u8) {
        self.capabilities.set(capabilities);
    }
    fn supports(&self, method: &str) -> bool {
        let bit = match method {
            "network.set_wifi" => WIFI,
            "network.connect" => CONNECT,
            "network.disconnect" => DISCONNECT,
            "network.forget" => FORGET,
            "vpn.connect" => CONNECT,
            "vpn.disconnect" => DISCONNECT,
            "network.scan" => SCAN,
            _ => return false,
        };
        !self.owner_boundary.get() && !self.sender.is_closed() && self.capabilities.get() & bit != 0
    }

    pub fn methods(&self) -> BTreeMap<String, Method> {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../../protocol/schema/core-service-call.json"
        ))
        .expect("core call schema");
        METHODS
            .into_iter()
            .filter(|method| self.supports(method))
            .map(|method| {
                let variant = schema["oneOf"]
                    .as_array()
                    .expect("core call variants")
                    .iter()
                    .find(|variant| variant["properties"]["m"]["const"] == method)
                    .expect("network call variant");
                let params = json!({
                    "$schema": schema["$schema"],
                    "$defs": schema["$defs"],
                    "$ref": variant["properties"]["p"]["$ref"],
                });
                (
                    method.into(),
                    Method {
                        v: 1,
                        class: Capability::System,
                        timeout: TIMEOUT,
                        params,
                        result: serde_json::from_str(include_str!(
                            "../../../../../protocol/schema/core-service-result.json"
                        ))
                        .expect("core result schema"),
                    },
                )
            })
            .collect()
    }
}

pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
    timeout: Option<u32>,
) -> std::result::Result<Value, Failure> {
    let call: CoreCall =
        serde_json::from_value(json!({"m": method, "p": params})).map_err(|_| invalid())?;
    if !call.valid()
        || !matches!(
            call,
            CoreCall::WifiPower(_)
                | CoreCall::NetworkConnect(_)
                | CoreCall::NetworkDisconnect(_)
                | CoreCall::NetworkForget(_)
                | CoreCall::VpnConnect(_)
                | CoreCall::VpnDisconnect(_)
                | CoreCall::NetworkScan(_)
        )
    {
        return Err(invalid());
    }
    let requests = {
        let state = hub.borrow();
        if state.safe {
            return Err(unavailable());
        }
        state.network.clone().ok_or_else(unavailable)?
    };
    if !requests.supports(method) {
        return Err(unavailable());
    }
    let (reply, receive) = oneshot::channel();
    let deadline = tokio::time::Instant::now()
        + Duration::from_millis(u64::from(timeout.unwrap_or(TIMEOUT).min(TIMEOUT)));
    requests
        .sender
        .try_send(Action {
            call,
            deadline,
            reply,
        })
        .map_err(|error| match error {
            mpsc::error::TrySendError::Full(_) => {
                Failure::new(ErrorCode::Busy, "Network control queue is full", true)
            }
            mpsc::error::TrySendError::Closed(_) => unavailable(),
        })?;
    // Cancellation drops the receiver: queued work is skipped before dispatch.
    tokio::time::timeout_at(deadline, receive)
        .await
        .map_err(|_| unavailable().dispatch_state(true))?
        .map_err(|_| unavailable())?
}

pub(super) fn observe_failure(
    message: &zbus::Message,
    devices: &HashMap<String, EntityId>,
    failures: &mut BTreeMap<EntityId, Rc<()>>,
) {
    let header = message.header();
    if header.interface().map(|name| name.as_str()) != Some("org.freedesktop.NetworkManager.Device")
        || header.member().map(|name| name.as_str()) != Some("StateChanged")
    {
        return;
    }
    let Some(path) = header.path().map(|path| path.as_str()) else {
        return;
    };
    let Ok((state, _, reason)) = message.body().deserialize::<(u32, u32, u32)>() else {
        return;
    };
    let Some(id) = devices.get(path) else {
        return;
    };
    if state == 120 && reason == 7 {
        failures.insert(id.clone(), Rc::new(()));
    } else if state == 100 {
        failures.remove(id);
    }
}

/// A VPN activation that ends in an error after NetworkManager admitted it (missing or
/// refused secrets, timeout, service failure) is remembered as `failed` (N0062).
pub(super) fn observe_vpn_failure(message: &zbus::Message, vpn: &VpnTracking) {
    let header = message.header();
    if header.interface().map(|name| name.as_str())
        != Some("org.freedesktop.NetworkManager.Connection.Active")
        || header.member().map(|name| name.as_str()) != Some("StateChanged")
    {
        return;
    }
    let Some(path) = header.path().map(|path| path.as_str()) else {
        return;
    };
    // Deactivated (4) because of: IP config invalid, connect timeout, service start
    // timeout/failure, no secrets, login failed. User disconnects and drops are not failures.
    let Ok((4, reason)) = message.body().deserialize::<(u32, u32)>() else {
        return;
    };
    if (5..=10).contains(&reason) {
        if let Some(id) = vpn.actives.borrow().get(path) {
            vpn.failed.borrow_mut().insert(id.clone());
        }
    }
}

pub(super) fn apply_vpn_failures(state: &mut VpnState, vpn: &VpnTracking) {
    use nacre_protocol::core_services::VpnConnectionState as S;
    let mut failed = vpn.failed.borrow_mut();
    failed.retain(|id| {
        state
            .connections
            .items
            .get(id)
            .is_some_and(|c| !matches!(c.state, S::Connecting | S::Connected))
    });
    for id in failed.iter() {
        if let Some(connection) = state.connections.items.get_mut(id) {
            connection.state = S::Failed;
            connection.active = false;
        }
    }
}

pub(super) fn apply_failures(state: &mut NetworkState, failures: &mut BTreeMap<EntityId, Rc<()>>) {
    failures.retain(|id, _| {
        state.wireless_enabled
            && state
                .devices
                .items
                .get(id)
                .is_some_and(|device| !device.connected)
    });
    if !failures.is_empty() {
        state.health.error = Some(Failure::new(
            ErrorCode::Unavailable,
            "Network authentication required or cancelled",
            true,
        ));
    }
}

type Settings = HashMap<String, HashMap<String, OwnedValue>>;

fn value<'a>(
    value: impl Into<zbus::zvariant::Value<'a>>,
) -> std::result::Result<OwnedValue, Failure> {
    OwnedValue::try_from(value.into()).map_err(|_| unavailable())
}

/// Settings handed to NetworkManager for a new profile. The secret exists only in
/// this request; the daemon keeps no copy and NetworkManager owns its storage.
fn wifi_settings(
    hidden_ssid: Option<&[u8]>,
    key_mgmt: Option<&str>,
    secret: Option<&str>,
) -> std::result::Result<Settings, Failure> {
    let mut settings = Settings::new();
    if let Some(ssid) = hidden_ssid {
        settings.insert(
            "connection".into(),
            HashMap::from([
                (
                    "id".into(),
                    value(String::from_utf8_lossy(ssid).into_owned())?,
                ),
                ("type".into(), value(WIFI_SETTINGS)?),
            ]),
        );
        settings.insert(
            WIFI_SETTINGS.into(),
            HashMap::from([
                ("ssid".into(), value(ssid.to_vec())?),
                ("mode".into(), value("infrastructure")?),
                ("hidden".into(), value(true)?),
            ]),
        );
    }
    if let (Some(key_mgmt), Some(secret)) = (key_mgmt, secret) {
        // WPA-PSK takes 8..63 characters or 64 hex digits; SAE any non-empty text.
        if key_mgmt == "wpa-psk"
            && !((8..=63).contains(&secret.len())
                || (secret.len() == 64 && secret.bytes().all(|b| b.is_ascii_hexdigit())))
        {
            return Err(invalid());
        }
        settings.insert(
            SECURITY_SETTINGS.into(),
            HashMap::from([
                ("key-mgmt".into(), value(key_mgmt)?),
                ("psk".into(), value(secret)?),
            ]),
        );
    }
    Ok(settings)
}

/// Delete a profile this daemon created when its first activation failed
/// authentication, so a wrong password is not left saved. Failures are ignored:
/// the profile is only a convenience copy and the next attempt creates a new one.
pub(super) async fn settle_created(
    connection: &zbus::Connection,
    owner: &str,
    requests: &Requests,
    ids: &IdentityTable,
    state: &NetworkState,
    failures: &BTreeMap<EntityId, Rc<()>>,
) {
    let entries: Vec<_> = requests.created.borrow().clone().into_iter().collect();
    for (device, profile) in entries {
        let active = ids
            .live
            .get(&identity_key("connection", &profile))
            .and_then(|id| state.connections.items.get(id))
            .map(|saved| saved.active);
        if failures.contains_key(&device) {
            let _ = tokio::time::timeout(
                Duration::from_secs(2),
                mutate(
                    connection,
                    owner,
                    &profile,
                    "Delete",
                    "org.freedesktop.NetworkManager.Settings.Connection",
                    &(),
                ),
            )
            .await;
        }
        if active == Some(true)
            || failures.contains_key(&device)
            || !state.devices.items.contains_key(&device)
        {
            requests.created.borrow_mut().remove(&device);
        }
    }
}

fn action_device(action: &Action) -> Option<&EntityId> {
    match &action.call {
        CoreCall::NetworkConnect(p) => Some(&p.device),
        CoreCall::NetworkDisconnect(p) => Some(&p.device),
        CoreCall::NetworkScan(p) => Some(&p.device),
        _ => None,
    }
}

pub(super) fn prior_failure(
    action: &Action,
    failures: &BTreeMap<EntityId, Rc<()>>,
) -> Option<Rc<()>> {
    action_device(action)
        .and_then(|device| failures.get(device))
        .cloned()
}

pub(super) fn clear_admitted_failure(
    action: &Action,
    previous: Option<&Rc<()>>,
    failures: &mut BTreeMap<EntityId, Rc<()>>,
) {
    let Some(device) = action_device(action) else {
        return;
    };
    // Admission clears only an older observation. A failure received while the
    // method reply was pending belongs to this attempt and must survive it.
    if previous
        .zip(failures.get(device))
        .is_some_and(|(old, current)| Rc::ptr_eq(old, current))
    {
        failures.remove(device);
    }
}

pub(super) fn unavailable() -> Failure {
    Failure::new(
        ErrorCode::Unavailable,
        "Network operation unavailable or outcome uncertain",
        true,
    )
}
fn read_error(error: CoreError) -> Failure {
    match error {
        CoreError::StaleTarget => stale(),
        CoreError::PermissionDenied => Failure::new(
            ErrorCode::PermissionDenied,
            "Network state read was denied",
            false,
        )
        .dispatch_state(false),
        _ => unavailable(),
    }
}
fn invalid() -> Failure {
    Failure::new(ErrorCode::Validation, "Invalid network parameters", false).dispatch_state(false)
}
fn secret_required() -> Failure {
    Failure::new(ErrorCode::Validation, SECRET_REQUIRED, true).dispatch_state(false)
}
fn stale() -> Failure {
    Failure::new(
        ErrorCode::StaleRevision,
        "Network target is stale or incompatible",
        true,
    )
    .dispatch_state(false)
}
fn backend_error(error: zbus::Error) -> Failure {
    let code = match &error {
        zbus::Error::MethodError(name, _, _) => {
            match name.as_str().rsplit('.').next().unwrap_or("") {
                "PermissionDenied" | "NotAuthorized" | "AccessDenied" | "AuthFailed" => {
                    ErrorCode::PermissionDenied
                }
                "UnknownObject"
                | "UnknownDevice"
                | "UnknownConnection"
                | "ConnectionNotAvailable"
                | "ConnectionNotActive"
                | "NotActive"
                | "SpecificObjectNotFound"
                | "InvalidConnection"
                | "IncompatibleConnection" => ErrorCode::StaleRevision,
                _ => ErrorCode::Unavailable,
            }
        }
        _ => ErrorCode::Unavailable,
    };
    Failure::new(
        code,
        match code {
            ErrorCode::PermissionDenied => "Network operation was denied",
            ErrorCode::StaleRevision => "Network target disappeared or is incompatible",
            _ => "Network operation unavailable or outcome uncertain",
        },
        code != ErrorCode::PermissionDenied,
    )
    .dispatch_state(true)
}

async fn introspect(connection: &zbus::Connection, owner: &str) -> Option<String> {
    let reply = connection
        .call_method(
            Some(owner),
            NM_MANAGER,
            Some("org.freedesktop.DBus.Introspectable"),
            "Introspect",
            &(),
        )
        .await
        .ok()?;
    if reply.body().len() > MAX_REPLY_BYTES {
        return None;
    }
    reply.body().deserialize().ok()
}

pub(super) async fn capabilities(connection: &zbus::Connection, owner: &str) -> u8 {
    let Ok(Some(xml)) =
        tokio::time::timeout(Duration::from_secs(2), introspect(connection, owner)).await
    else {
        return 0;
    };
    // Standard D-Bus introspection includes a DOCTYPE. roxmltree performs no
    // external I/O; retain an explicit bound on the parsed tree.
    let Ok(document) = roxmltree::Document::parse_with_options(
        &xml,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            nodes_limit: 16_384,
            ..Default::default()
        },
    ) else {
        return 0;
    };
    let Some(interface) = document
        .descendants()
        .find(|node| node.has_tag_name("interface") && node.attribute("name") == Some(NM_SERVICE))
    else {
        return 0;
    };
    let method = |name| {
        interface
            .children()
            .any(|node| node.has_tag_name("method") && node.attribute("name") == Some(name))
    };
    let wifi = interface.children().any(|node| {
        node.has_tag_name("property")
            && node.attribute("name") == Some("WirelessEnabled")
            && node.attribute("type") == Some("b")
            && matches!(node.attribute("access"), Some("write" | "readwrite"))
    });
    let mut capabilities = 0;
    if wifi {
        // The radio switch and the per-device rescan are both Wi-Fi controls.
        capabilities |= WIFI | SCAN;
    }
    if method("ActivateConnection") && method("AddAndActivateConnection2") {
        capabilities |= CONNECT | FORGET;
    }
    if method("DeactivateConnection") {
        capabilities |= DISCONNECT;
    }
    capabilities
}

fn source<'a>(
    ids: &'a IdentityTable,
    kind: &str,
    id: &EntityId,
) -> std::result::Result<&'a str, Failure> {
    ids.live
        .iter()
        .find_map(|(key, candidate)| {
            let (prefix, path) = key.split_once('\0')?;
            (prefix == kind && candidate == id).then_some(path)
        })
        .ok_or_else(stale)
}

async fn mutate<A: serde::Serialize + zbus::zvariant::DynamicType>(
    connection: &zbus::Connection,
    owner: &str,
    object: &str,
    method: &str,
    interface: &str,
    args: &A,
) -> std::result::Result<zbus::Message, Failure> {
    let reply = connection
        .call_method(Some(owner), object, Some(interface), method, args)
        .await
        .map_err(backend_error)?;
    if reply.body().len() > MAX_REPLY_BYTES {
        return Err(unavailable().dispatch_state(true));
    }
    Ok(reply)
}

/// `vpn.connect` / `vpn.disconnect` (N0062): NetworkManager admission only, like the
/// network calls. Secrets stay with NetworkManager and the user's secret agents; none
/// cross this call, and a refusal later shows up as the `failed` state.
#[allow(clippy::too_many_arguments)]
async fn vpn_execute(
    connection: &zbus::Connection,
    owner: &str,
    ids: &IdentityTable,
    requests: &Requests,
    action: &Action,
    raw: &RawNetwork,
    state: &NetworkState,
    id: &EntityId,
    connect: bool,
) -> std::result::Result<Value, Failure> {
    if !requests.supports(if connect {
        "vpn.connect"
    } else {
        "vpn.disconnect"
    }) {
        return Err(unavailable());
    }
    let profile = source(ids, "connection", id)?;
    match state.connections.items.get(id) {
        Some(saved) if matches!(saved.kind.as_str(), "vpn" | "wireguard") => {}
        _ => return Err(stale()),
    }
    let active = raw
        .active_connections
        .iter()
        .find(|(_, saved)| saved == profile)
        .map(|(active, _)| active.as_str());
    let path = |value: &str| OwnedObjectPath::try_from(value.to_owned()).map_err(|_| stale());
    let (profile, active) = (path(profile)?, active.map(path).transpose()?);
    if connect == active.is_some() {
        return serde_json::to_value(CoreResult { changed: false }).map_err(|_| unavailable());
    }
    require_network_manager_owner(connection, owner)
        .await
        .map_err(|_| unavailable())?;
    if requests.owner_boundary.get()
        || action.reply.is_closed()
        || tokio::time::Instant::now() >= action.deadline
    {
        return Err(unavailable().dispatch_state(false));
    }
    if let Some(active) = active {
        mutate(
            connection,
            owner,
            NM_MANAGER,
            "DeactivateConnection",
            NM_SERVICE,
            &(active,),
        )
        .await?;
    } else {
        requests.vpn.failed.borrow_mut().remove(id);
        let reply = mutate(
            connection,
            owner,
            NM_MANAGER,
            "ActivateConnection",
            NM_SERVICE,
            &(profile, path("/")?, path("/")?),
        )
        .await?;
        let active: OwnedObjectPath = reply
            .body()
            .deserialize()
            .map_err(|_| unavailable().dispatch_state(true))?;
        requests
            .vpn
            .actives
            .borrow_mut()
            .insert(active.as_str().to_owned(), id.clone());
    }
    serde_json::to_value(CoreResult { changed: true }).map_err(|_| unavailable())
}

/// Ask for a rescan, then wait (bounded) until NetworkManager reports a newer `LastScan`, so
/// the reply marks the end of the scan the client shows progress for. A scan refused as too
/// soon after the previous one changed nothing and is not an error.
async fn request_scan(
    connection: &zbus::Connection,
    owner: &str,
    device: &str,
    action: &Action,
) -> std::result::Result<bool, Failure> {
    let last = || async {
        property::<i64>(connection, device, WIRELESS, "LastScan")
            .await
            .map_err(read_error)
    };
    let before = last().await?;
    let options = HashMap::<String, zbus::zvariant::Value>::new();
    match connection
        .call_method(
            Some(owner),
            device,
            Some(WIRELESS),
            "RequestScan",
            &(options,),
        )
        .await
    {
        Ok(_) => {}
        Err(zbus::Error::MethodError(name, _, _)) if name.as_str().ends_with(".NotAllowed") => {
            return Ok(false)
        }
        Err(error) => return Err(backend_error(error)),
    }
    let limit = (tokio::time::Instant::now() + SCAN_WAIT).min(action.deadline);
    while tokio::time::Instant::now() < limit && !action.reply.is_closed() {
        tokio::time::sleep(SCAN_POLL).await;
        if last().await.is_ok_and(|now| now != before) {
            break;
        }
    }
    Ok(true)
}

pub(super) async fn execute(
    connection: &zbus::Connection,
    owner: &str,
    ids: &mut IdentityTable,
    requests: &Requests,
    action: &Action,
) -> std::result::Result<Value, Failure> {
    require_network_manager_owner(connection, owner)
        .await
        .map_err(|_| unavailable())?;
    let raw = read_raw(connection).await.map_err(read_error)?;
    let mut fresh_ids = ids.clone();
    let state = project(raw.clone(), &mut fresh_ids).map_err(|_| unavailable())?;
    if !CoreSnapshot::Network(state.clone()).valid() {
        return Err(unavailable());
    }
    // A successful preflight is also an observation of disappearance. Retire
    // those published identities even if this action fails and the same path
    // returns before the following topic read. Do not issue unpublished IDs.
    ids.live
        .retain(|key, id| fresh_ids.live.get(key) == Some(id));
    if let CoreCall::VpnConnect(p) | CoreCall::VpnDisconnect(p) = &action.call {
        let connect = matches!(action.call, CoreCall::VpnConnect(_));
        return vpn_execute(
            connection,
            owner,
            ids,
            requests,
            action,
            &raw,
            &state,
            &p.connection,
            connect,
        )
        .await;
    }
    let method = match &action.call {
        CoreCall::WifiPower(_) => "network.set_wifi",
        CoreCall::NetworkConnect(_) => "network.connect",
        CoreCall::NetworkDisconnect(_) => "network.disconnect",
        CoreCall::NetworkForget(_) => "network.forget",
        CoreCall::NetworkScan(_) => "network.scan",
        _ => return Err(invalid()),
    };
    if !requests.supports(method) {
        return Err(unavailable());
    }
    // Resolve only previously published IDs that survived the fresh read.
    let device_path = match &action.call {
        CoreCall::NetworkConnect(p) => Some((&p.device, source(ids, "device", &p.device)?)),
        CoreCall::NetworkDisconnect(p) => Some((&p.device, source(ids, "device", &p.device)?)),
        CoreCall::NetworkScan(p) => Some((&p.device, source(ids, "device", &p.device)?)),
        _ => None,
    };
    if let Some((id, _)) = device_path {
        if !state.devices.items.contains_key(id) {
            return Err(stale());
        }
    }
    let device =
        device_path.and_then(|(_, path)| raw.devices.iter().find(|device| device.path == path));
    let path = |value: &str| OwnedObjectPath::try_from(value.to_owned()).map_err(|_| stale());
    enum Mutation {
        Wifi(bool),
        Activate(OwnedObjectPath, OwnedObjectPath, OwnedObjectPath),
        Add(Settings, &'static str, OwnedObjectPath, OwnedObjectPath),
        Deactivate(OwnedObjectPath),
        Scan(OwnedObjectPath),
        Delete(OwnedObjectPath),
        Noop,
    }
    let (mutation, mut changed) = match &action.call {
        CoreCall::WifiPower(p) => (Mutation::Wifi(p.enabled), raw.wireless_enabled != p.enabled),
        CoreCall::NetworkConnect(p) => {
            let device = device.ok_or_else(stale)?;
            let device_path = path(&device.path)?;
            let mutation = match &p.target {
                NetworkTarget::AccessPoint(id) => {
                    let point = source(ids, "access_point", id)?;
                    let fresh = state.access_points.items.get(id).ok_or_else(stale)?;
                    if device.kind != 2 || fresh.device != p.device {
                        return Err(stale());
                    }
                    match p.secret.as_deref() {
                        Some(secret) => {
                            let raw_point = device
                                .access_points
                                .iter()
                                .find(|candidate| candidate.path == point)
                                .ok_or_else(stale)?;
                            let key_mgmt = match raw_point.wpa_flags | raw_point.rsn_flags {
                                flags if flags & 0x100 != 0 => "wpa-psk",
                                flags if flags & 0x400 != 0 => "sae",
                                _ => return Err(invalid()),
                            };
                            let settings = wifi_settings(None, Some(key_mgmt), Some(secret))?;
                            Mutation::Add(settings, "to-disk", device_path, path(point)?)
                        }
                        None => {
                            // Saved profiles carry their own secrets inside NetworkManager.
                            if fresh.secured
                                && !state
                                    .connections
                                    .items
                                    .values()
                                    .any(|saved| saved.ssid.as_deref() == Some(fresh.name.as_str()))
                            {
                                return Err(secret_required());
                            }
                            Mutation::Add(Settings::new(), "volatile", device_path, path(point)?)
                        }
                    }
                }
                NetworkTarget::HiddenSsid(ssid) => {
                    if device.kind != 2 {
                        return Err(stale());
                    }
                    let key_mgmt = match p.security.unwrap_or(if p.secret.is_some() {
                        WifiSecurity::WpaPsk
                    } else {
                        WifiSecurity::Open
                    }) {
                        WifiSecurity::Open if p.secret.is_none() => None,
                        WifiSecurity::WpaPsk if p.secret.is_some() => Some("wpa-psk"),
                        WifiSecurity::Sae if p.secret.is_some() => Some("sae"),
                        WifiSecurity::Open => return Err(invalid()),
                        _ => return Err(secret_required()),
                    };
                    let settings =
                        wifi_settings(Some(ssid.as_bytes()), key_mgmt, p.secret.as_deref())?;
                    Mutation::Add(settings, "to-disk", device_path, path("/")?)
                }
                NetworkTarget::Connection(id) => {
                    let profile = source(ids, "connection", id)?;
                    let profile_state = state.connections.items.get(id).ok_or_else(stale)?;
                    if !matches!(device.kind, 1 | 2)
                        || profile_state.kind != device_kind(device.kind)
                    {
                        return Err(stale());
                    }
                    let available: Vec<OwnedObjectPath> = property(
                        connection,
                        &device.path,
                        "org.freedesktop.NetworkManager.Device",
                        "AvailableConnections",
                    )
                    .await
                    .map_err(read_error)?;
                    if available.len() > MAX_ENTITIES {
                        return Err(unavailable());
                    }
                    if !available
                        .iter()
                        .any(|candidate| candidate.as_str() == profile)
                    {
                        return Err(stale());
                    }
                    Mutation::Activate(path(profile)?, device_path, path("/")?)
                }
            };
            (mutation, true)
        }
        CoreCall::NetworkScan(_) => {
            let device = device.ok_or_else(stale)?;
            if device.kind != 2 {
                return Err(stale());
            }
            if !raw.wireless_enabled {
                return Err(unavailable().dispatch_state(false));
            }
            (Mutation::Scan(path(&device.path)?), true)
        }
        CoreCall::NetworkForget(p) => {
            let profile = source(ids, "connection", &p.connection)?;
            // Only saved Wi-Fi profiles; wired and VPN profiles are not forgotten here.
            match state.connections.items.get(&p.connection) {
                Some(saved) if saved.kind == "wifi" => {}
                _ => return Err(stale()),
            }
            (Mutation::Delete(path(profile)?), true)
        }
        CoreCall::NetworkDisconnect(_) => {
            let device = device.ok_or_else(stale)?;
            if let Some(active) = &device.active_connection {
                // A connection may span devices: never disconnect unrelated targets.
                let devices: Vec<OwnedObjectPath> = property(
                    connection,
                    active,
                    "org.freedesktop.NetworkManager.Connection.Active",
                    "Devices",
                )
                .await
                .map_err(read_error)?;
                if devices.len() != 1 || devices[0].as_str() != device.path {
                    return Err(stale());
                }
                (Mutation::Deactivate(path(active)?), true)
            } else {
                if matches!(device.state, 40..=100) {
                    return Err(unavailable());
                }
                (Mutation::Noop, false)
            }
        }
        _ => return Err(invalid()),
    };
    require_network_manager_owner(connection, owner)
        .await
        .map_err(|_| unavailable())?;
    if requests.owner_boundary.get()
        || action.reply.is_closed()
        || tokio::time::Instant::now() >= action.deadline
    {
        return Err(unavailable().dispatch_state(false));
    }
    // Destination is the unique owner, never the replaceable well-known name.
    match mutation {
        Mutation::Wifi(enabled) => {
            mutate(
                connection,
                owner,
                NM_MANAGER,
                "Set",
                "org.freedesktop.DBus.Properties",
                &(
                    NM_SERVICE,
                    "WirelessEnabled",
                    zbus::zvariant::Value::new(enabled),
                ),
            )
            .await?;
        }
        Mutation::Activate(profile, device, point) => {
            let reply = mutate(
                connection,
                owner,
                NM_MANAGER,
                "ActivateConnection",
                NM_SERVICE,
                &(profile, device, point),
            )
            .await?;
            let _: OwnedObjectPath = reply
                .body()
                .deserialize()
                .map_err(|_| unavailable().dispatch_state(true))?;
        }
        Mutation::Add(settings, persist, device, point) => {
            let options = HashMap::from([("persist", zbus::zvariant::Value::new(persist))]);
            let reply = mutate(
                connection,
                owner,
                NM_MANAGER,
                "AddAndActivateConnection2",
                NM_SERVICE,
                &(settings, device, point, options),
            )
            .await?;
            let (profile, _, _): (
                OwnedObjectPath,
                OwnedObjectPath,
                HashMap<String, OwnedValue>,
            ) = reply
                .body()
                .deserialize()
                .map_err(|_| unavailable().dispatch_state(true))?;
            if persist == "to-disk" {
                if let Some(device) = action_device(action) {
                    requests
                        .created
                        .borrow_mut()
                        .insert(device.clone(), profile.as_str().to_owned());
                }
            }
        }
        Mutation::Delete(profile) => {
            mutate(
                connection,
                owner,
                profile.as_str(),
                "Delete",
                "org.freedesktop.NetworkManager.Settings.Connection",
                &(),
            )
            .await?;
        }
        Mutation::Deactivate(active) => {
            mutate(
                connection,
                owner,
                NM_MANAGER,
                "DeactivateConnection",
                NM_SERVICE,
                &(active,),
            )
            .await?;
        }
        Mutation::Scan(device) => {
            changed = request_scan(connection, owner, device.as_str(), action).await?;
        }
        Mutation::Noop => {}
    }
    serde_json::to_value(CoreResult { changed }).map_err(|_| unavailable())
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod vpn_tests;
