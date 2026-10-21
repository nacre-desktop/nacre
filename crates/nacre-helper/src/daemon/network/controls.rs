//! Admission-only controls on the reader's connection and owner generation.
use super::*;
use nacre_protocol::{
    core_services::{CoreCall, CoreResult, NetworkTarget},
    Capability, Method,
};
use serde_json::Value;
use tokio::sync::{mpsc, oneshot};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

const WIFI: u8 = 1;
const CONNECT: u8 = 2;
const DISCONNECT: u8 = 4;
const TIMEOUT: u32 = 10_000;

#[derive(Clone)]
pub struct Requests {
    sender: mpsc::Sender<Action>,
    capabilities: Rc<Cell<u8>>,
    owner_boundary: Rc<Cell<bool>>,
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
            _ => return false,
        };
        !self.owner_boundary.get() && !self.sender.is_closed() && self.capabilities.get() & bit != 0
    }

    pub fn methods(&self) -> BTreeMap<String, Method> {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../../protocol/schema/core-service-call.json"
        ))
        .expect("core call schema");
        ["network.set_wifi", "network.connect", "network.disconnect"]
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
            CoreCall::WifiPower(_) | CoreCall::NetworkConnect(_) | CoreCall::NetworkDisconnect(_)
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

fn action_device(action: &Action) -> Option<&EntityId> {
    match &action.call {
        CoreCall::NetworkConnect(p) => Some(&p.device),
        CoreCall::NetworkDisconnect(p) => Some(&p.device),
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
        capabilities |= WIFI;
    }
    if method("ActivateConnection") && method("AddAndActivateConnection2") {
        capabilities |= CONNECT;
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
    method: &str,
    interface: &str,
    args: &A,
) -> std::result::Result<zbus::Message, Failure> {
    let reply = connection
        .call_method(Some(owner), NM_MANAGER, Some(interface), method, args)
        .await
        .map_err(backend_error)?;
    if reply.body().len() > MAX_REPLY_BYTES {
        return Err(unavailable().dispatch_state(true));
    }
    Ok(reply)
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
    let method = match &action.call {
        CoreCall::WifiPower(_) => "network.set_wifi",
        CoreCall::NetworkConnect(_) => "network.connect",
        CoreCall::NetworkDisconnect(_) => "network.disconnect",
        _ => return Err(invalid()),
    };
    if !requests.supports(method) {
        return Err(unavailable());
    }
    // Resolve only previously published IDs that survived the fresh read.
    let device_path = match &action.call {
        CoreCall::NetworkConnect(p) => Some((&p.device, source(ids, "device", &p.device)?)),
        CoreCall::NetworkDisconnect(p) => Some((&p.device, source(ids, "device", &p.device)?)),
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
        Add(OwnedObjectPath, OwnedObjectPath),
        Deactivate(OwnedObjectPath),
        Noop,
    }
    let (mutation, changed) = match &action.call {
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
                    Mutation::Add(device_path, path(point)?)
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
        Mutation::Add(device, point) => {
            let settings: HashMap<String, HashMap<String, OwnedValue>> = HashMap::new();
            let options = HashMap::from([("persist", zbus::zvariant::Value::new("volatile"))]);
            let reply = mutate(
                connection,
                owner,
                "AddAndActivateConnection2",
                NM_SERVICE,
                &(settings, device, point, options),
            )
            .await?;
            let _: (
                OwnedObjectPath,
                OwnedObjectPath,
                HashMap<String, OwnedValue>,
            ) = reply
                .body()
                .deserialize()
                .map_err(|_| unavailable().dispatch_state(true))?;
        }
        Mutation::Deactivate(active) => {
            mutate(
                connection,
                owner,
                "DeactivateConnection",
                NM_SERVICE,
                &(active,),
            )
            .await?;
        }
        Mutation::Noop => {}
    }
    serde_json::to_value(CoreResult { changed }).map_err(|_| unavailable())
}

#[cfg(test)]
mod tests;
