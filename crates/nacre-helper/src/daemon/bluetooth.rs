//! N0047 adds the `bluetooth` state topic (adapters, devices) read from BlueZ's ObjectManager,
//! `bluetooth.set_power`, `.set_discovery` (auto-stops), `.connect` and `.disconnect`.
//! N0045: Bluetooth pairing agent. `bluetooth.pair` registers a BlueZ `Agent1` on the daemon's own
//! system-bus connection for the duration of one pairing, publishes what the agent is waiting for
//! on the `bluetooth.pairing` topic and relays `bluetooth.pair.respond` / `.cancel` back to BlueZ.
//! The agent is registered for this client only, never as the default agent, so a remote
//! device's own pairing request is not intercepted. Nothing is stored, trusted or logged.
use super::hub::Shared;
use crate::Result;
use nacre_protocol::{
    core_services::{
        pairing_text_valid, BluetoothAdapter, BluetoothDevice, BluetoothPairRespond,
        BluetoothPairingState, BluetoothState, CoreCall, CoreSnapshot, Entities, EntityId, Health,
        PairingKind, PairingPrompt,
    },
    Capability, ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    cell::{Cell, RefCell},
    collections::HashMap,
    rc::Rc,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};
use tokio::sync::{mpsc, oneshot};
use zbus::{
    message::Header,
    zvariant::{ObjectPath, OwnedObjectPath, OwnedValue},
};

pub const TOPIC: &str = "bluetooth.pairing";
pub const STATE_TOPIC: &str = "bluetooth";
const METHODS: [&str; 7] = [
    "bluetooth.pair",
    "bluetooth.pair.respond",
    "bluetooth.pair.cancel",
    "bluetooth.set_power",
    "bluetooth.set_discovery",
    "bluetooth.connect",
    "bluetooth.disconnect",
];
const BLUEZ: &str = "org.bluez";
const AGENT_PATH: &str = "/org/nacre/bluetooth/agent";
const CAPABILITY: &str = "KeyboardDisplay";
/// A prompt the user does not answer in this time is withdrawn and the pairing times out.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(60);
/// Advertised as PAIR_TIMEOUT + 5 s, which must stay within the protocol's 120 s method limit.
const PAIR_TIMEOUT: Duration = Duration::from_secs(110);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(30);
const QUICK: Duration = Duration::from_secs(5);
const CONTROL_TIMEOUT: u32 = 10_000;
/// A discovery started by `bluetooth.set_discovery` stops by itself after this time.
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(60);
/// BlueZ signals are coalesced into one state read.
const REFRESH_DELAY: Duration = Duration::from_millis(100);

/// The user's answer. Never printed: it may hold a PIN or passkey.
enum Answer {
    Accept,
    Pin(String),
    Passkey(u32),
    Reject,
}
#[derive(Clone, Copy, PartialEq)]
enum Outcome {
    Open,
    TimedOut,
    Rejected,
    Cancelled,
}
struct Pending {
    kind: PairingKind,
    reply: oneshot::Sender<Answer>,
}
struct Session {
    id: EntityId,
    path: String,
    name: String,
    pending: Option<Pending>,
    shown: Option<(PairingKind, Option<String>)>,
    outcome: Outcome,
}
#[derive(Default)]
struct Inner {
    /// Unique bus name of BlueZ while a pairing runs; only it may call the agent.
    owner: Option<String>,
    session: Option<Session>,
}
fn lock(inner: &Mutex<Inner>) -> MutexGuard<'_, Inner> {
    inner.lock().unwrap_or_else(PoisonError::into_inner)
}

#[derive(zbus::DBusError, Debug)]
#[zbus(prefix = "org.bluez.Error")]
enum AgentError {
    #[zbus(error)]
    ZBus(zbus::Error),
    Rejected(String),
    Canceled(String),
}
fn rejected() -> AgentError {
    AgentError::Rejected("Rejected".into())
}

struct Agent {
    inner: Arc<Mutex<Inner>>,
    changed: mpsc::UnboundedSender<()>,
    prompt_timeout: Duration,
}
impl Agent {
    fn session<'a>(
        inner: &'a mut Inner,
        header: &Header<'_>,
        device: &ObjectPath<'_>,
    ) -> std::result::Result<&'a mut Session, AgentError> {
        let sender = header.sender().map(|name| name.as_str());
        let Inner { owner, session } = inner;
        if sender.is_none() || sender != owner.as_deref() {
            return Err(rejected());
        }
        session
            .as_mut()
            .filter(|s| s.path == device.as_str())
            .ok_or_else(rejected)
    }
    /// Publish a prompt and wait for its answer, a cancellation or the timeout.
    async fn ask(
        &self,
        header: &Header<'_>,
        device: &ObjectPath<'_>,
        kind: PairingKind,
        code: Option<String>,
    ) -> std::result::Result<Answer, AgentError> {
        let receive = {
            let mut inner = lock(&self.inner);
            let session = Self::session(&mut inner, header, device)?;
            if session.pending.is_some() {
                return Err(rejected());
            }
            let (reply, receive) = oneshot::channel();
            session.pending = Some(Pending { kind, reply });
            session.shown = Some((kind, code));
            receive
        };
        let _ = self.changed.send(());
        let answer = tokio::time::timeout(self.prompt_timeout, receive).await;
        if let Some(session) = lock(&self.inner).session.as_mut() {
            session.pending = None;
            session.shown = None;
            if answer.is_err() {
                session.outcome = Outcome::TimedOut;
            }
        }
        let _ = self.changed.send(());
        match answer {
            Ok(Ok(Answer::Reject)) => Err(rejected()),
            Ok(Ok(answer)) => Ok(answer),
            _ => Err(AgentError::Canceled("Canceled".into())),
        }
    }
    fn show(
        &self,
        header: &Header<'_>,
        device: &ObjectPath<'_>,
        code: String,
    ) -> std::result::Result<(), AgentError> {
        if !pairing_text_valid(&code) {
            return Err(rejected());
        }
        {
            let mut inner = lock(&self.inner);
            Self::session(&mut inner, header, device)?.shown =
                Some((PairingKind::Display, Some(code)));
        }
        let _ = self.changed.send(());
        Ok(())
    }
    async fn authorize(
        &self,
        header: &Header<'_>,
        device: &ObjectPath<'_>,
    ) -> std::result::Result<(), AgentError> {
        match self
            .ask(header, device, PairingKind::Authorize, None)
            .await?
        {
            Answer::Accept => Ok(()),
            _ => Err(rejected()),
        }
    }
    /// BlueZ withdrew the request: dropping the sender wakes `ask` with a cancellation.
    fn clear(&self) {
        if let Some(session) = lock(&self.inner).session.as_mut() {
            session.pending = None;
            session.shown = None;
        }
        let _ = self.changed.send(());
    }
}

#[zbus::interface(name = "org.bluez.Agent1")]
impl Agent {
    fn release(&self) {
        self.clear();
    }
    async fn request_pin_code(
        &self,
        device: ObjectPath<'_>,
        #[zbus(header)] header: Header<'_>,
    ) -> std::result::Result<String, AgentError> {
        match self.ask(&header, &device, PairingKind::Pin, None).await? {
            Answer::Pin(pin) => Ok(pin),
            _ => Err(rejected()),
        }
    }
    fn display_pin_code(
        &self,
        device: ObjectPath<'_>,
        pincode: String,
        #[zbus(header)] header: Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.show(&header, &device, pincode)
    }
    async fn request_passkey(
        &self,
        device: ObjectPath<'_>,
        #[zbus(header)] header: Header<'_>,
    ) -> std::result::Result<u32, AgentError> {
        match self
            .ask(&header, &device, PairingKind::Passkey, None)
            .await?
        {
            Answer::Passkey(passkey) => Ok(passkey),
            _ => Err(rejected()),
        }
    }
    fn display_passkey(
        &self,
        device: ObjectPath<'_>,
        passkey: u32,
        _entered: u16,
        #[zbus(header)] header: Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.show(&header, &device, format!("{:06}", passkey % 1_000_000))
    }
    async fn request_confirmation(
        &self,
        device: ObjectPath<'_>,
        passkey: u32,
        #[zbus(header)] header: Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        let code = Some(format!("{:06}", passkey % 1_000_000));
        match self
            .ask(&header, &device, PairingKind::Confirm, code)
            .await?
        {
            Answer::Accept => Ok(()),
            _ => Err(rejected()),
        }
    }
    async fn request_authorization(
        &self,
        device: ObjectPath<'_>,
        #[zbus(header)] header: Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.authorize(&header, &device).await
    }
    async fn authorize_service(
        &self,
        device: ObjectPath<'_>,
        _uuid: String,
        #[zbus(header)] header: Header<'_>,
    ) -> std::result::Result<(), AgentError> {
        self.authorize(&header, &device).await
    }
    fn cancel(&self) {
        self.clear();
    }
}

pub struct Service {
    connection: zbus::Connection,
    inner: Arc<Mutex<Inner>>,
    changed: mpsc::UnboundedSender<()>,
    ready: Cell<bool>,
    prompt_timeout: Duration,
    discovery_timeout: Duration,
    /// Adapter path -> the task that stops its discovery.
    discovery: RefCell<HashMap<String, tokio::task::JoinHandle<()>>>,
    /// Last good state, shown stale when BlueZ stops answering.
    last: RefCell<BluetoothState>,
}

fn invalid() -> Failure {
    Failure::new(ErrorCode::Validation, "Invalid pairing parameters", false).dispatch_state(false)
}
fn unavailable() -> Failure {
    Failure::new(
        ErrorCode::Unavailable,
        "Bluetooth pairing unavailable",
        true,
    )
}
fn failed(outcome: Outcome) -> Failure {
    match outcome {
        Outcome::TimedOut => Failure::new(ErrorCode::Timeout, "Pairing timed out", true),
        Outcome::Rejected => {
            Failure::new(ErrorCode::PermissionDenied, "Pairing was rejected", false)
        }
        _ => Failure::new(
            ErrorCode::PermissionDenied,
            "Authentication was cancelled",
            false,
        ),
    }
    .dispatch_state(true)
}

type Managed = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

fn flag(properties: &HashMap<String, OwnedValue>, key: &str) -> bool {
    properties
        .get(key)
        .and_then(|v| bool::try_from(v).ok())
        .unwrap_or(false)
}
/// A BlueZ string without control characters, as the UI may show it.
fn text(properties: &HashMap<String, OwnedValue>, key: &str) -> Option<String> {
    let value = properties.get(key)?.try_clone().ok()?;
    let text: String = String::try_from(value)
        .ok()?
        .chars()
        .filter(|c| !c.is_control())
        .take(64)
        .collect();
    Some(text)
}
fn unavailable_state() -> BluetoothState {
    fn empty<T>() -> Entities<T> {
        Entities {
            items: Default::default(),
            order: Vec::new(),
        }
    }
    BluetoothState {
        health: Health {
            available: false,
            stale: false,
            error: Some(unavailable()),
        },
        adapters: empty(),
        devices: empty(),
    }
}
/// Adapters and devices BlueZ publishes under `/org/bluez/<adapter>[/<device>]`, ordered like the
/// QML page: connected first, then by name.
fn build_state(objects: &Managed, pairing: Option<&EntityId>) -> BluetoothState {
    let id_of = |path: &str| EntityId(path.rsplit('/').next().unwrap_or_default().into());
    let mut state = unavailable_state();
    state.health = Health {
        available: true,
        stale: false,
        error: None,
    };
    let mut adapters = Vec::new();
    let mut devices = Vec::new();
    for (path, interfaces) in objects {
        let path = path.as_str();
        let Some(rest) = path.strip_prefix("/org/bluez/") else {
            continue;
        };
        let id = id_of(path);
        if !id.valid() {
            continue;
        }
        if let (Some(adapter), false) = (interfaces.get("org.bluez.Adapter1"), rest.contains('/')) {
            adapters.push((
                id,
                BluetoothAdapter {
                    name: text(adapter, "Alias").unwrap_or_default(),
                    powered: flag(adapter, "Powered"),
                    discovering: flag(adapter, "Discovering"),
                },
            ));
        } else if let (Some(device), Some((parent, _))) =
            (interfaces.get("org.bluez.Device1"), rest.split_once('/'))
        {
            let battery = interfaces
                .get("org.bluez.Battery1")
                .and_then(|b| b.get("Percentage"))
                .and_then(|v| u8::try_from(v).ok())
                .filter(|b| *b <= 100);
            devices.push((
                id.clone(),
                BluetoothDevice {
                    adapter: EntityId(parent.into()),
                    name: text(device, "Alias")
                        .or_else(|| text(device, "Name"))
                        .unwrap_or_default(),
                    connected: flag(device, "Connected"),
                    paired: flag(device, "Paired"),
                    trusted: flag(device, "Trusted"),
                    pairing: pairing == Some(&id),
                    battery,
                    icon: text(device, "Icon"),
                    rssi: device
                        .get("RSSI")
                        .and_then(|v| i16::try_from(v).ok())
                        .filter(|r| (-127..=20).contains(r)),
                },
            ));
        }
    }
    adapters.sort_by(|a, b| a.0 .0.cmp(&b.0 .0));
    devices.retain(|(_, d)| adapters.iter().any(|(id, _)| *id == d.adapter));
    devices.sort_by(|a, b| {
        b.1.connected
            .cmp(&a.1.connected)
            .then_with(|| a.1.name.to_lowercase().cmp(&b.1.name.to_lowercase()))
            .then_with(|| a.0 .0.cmp(&b.0 .0))
    });
    devices.truncate(2048);
    state.adapters.order = adapters.iter().map(|(id, _)| id.clone()).collect();
    state.adapters.items = adapters.into_iter().collect();
    state.devices.order = devices.iter().map(|(id, _)| id.clone()).collect();
    state.devices.items = devices.into_iter().collect();
    state
}

impl Service {
    pub fn methods(&self) -> std::collections::BTreeMap<String, Method> {
        if !self.ready.get() {
            return Default::default();
        }
        let schema: Value = serde_json::from_str(include_str!(
            "../../../../protocol/schema/core-service-call.json"
        ))
        .expect("core call schema");
        METHODS
            .into_iter()
            .map(|method| {
                let variant = schema["oneOf"]
                    .as_array()
                    .expect("core call variants")
                    .iter()
                    .find(|variant| variant["properties"]["m"]["const"] == method)
                    .expect("bluetooth call variant");
                (
                    method.into(),
                    Method {
                        v: 1,
                        class: Capability::System,
                        timeout: match method {
                            "bluetooth.pair" => PAIR_TIMEOUT.as_millis() as u32 + 5_000,
                            "bluetooth.connect" => CONNECT_TIMEOUT.as_millis() as u32 + 5_000,
                            _ => CONTROL_TIMEOUT,
                        },
                        params: json!({
                            "$schema": schema["$schema"],
                            "$defs": schema["$defs"],
                            "$ref": variant["properties"]["p"]["$ref"],
                        }),
                        result: serde_json::from_str(include_str!(
                            "../../../../protocol/schema/core-service-result.json"
                        ))
                        .expect("core result schema"),
                    },
                )
            })
            .collect()
    }

    fn state(&self) -> BluetoothPairingState {
        let ready = self.ready.get();
        let inner = lock(&self.inner);
        BluetoothPairingState {
            health: Health {
                available: ready,
                stale: false,
                error: (!ready).then(unavailable),
            },
            prompt: inner.session.as_ref().and_then(|s| {
                let (kind, code) = s.shown.clone()?;
                Some(PairingPrompt {
                    device: s.id.clone(),
                    kind,
                    name: s.name.clone(),
                    code,
                    timeout_ms: (self.prompt_timeout.as_millis() as u32).max(1),
                })
            }),
        }
    }
    fn publish(&self, hub: &Shared) {
        let state = self.state();
        debug_assert!(CoreSnapshot::BluetoothPairing(state.clone()).valid());
        match serde_json::to_value(state) {
            Ok(data) => {
                if hub.borrow_mut().publish(TOPIC, data).is_err() {
                    tracing::warn!("Bluetooth pairing state could not be published");
                }
            }
            Err(_) => tracing::warn!("Bluetooth pairing state could not be encoded"),
        }
    }

    async fn call(
        &self,
        path: &str,
        interface: &str,
        method: &str,
        body: &(impl serde::Serialize + zbus::zvariant::DynamicType),
        limit: Duration,
    ) -> std::result::Result<zbus::Message, zbus::Error> {
        tokio::time::timeout(
            limit,
            self.connection
                .call_method(Some(BLUEZ), path, Some(interface), method, body),
        )
        .await
        .map_err(|_| zbus::Error::InputOutput(Arc::new(std::io::ErrorKind::TimedOut.into())))?
    }

    async fn managed(&self) -> std::result::Result<Managed, Failure> {
        let reply = self
            .call(
                "/",
                "org.freedesktop.DBus.ObjectManager",
                "GetManagedObjects",
                &(),
                QUICK,
            )
            .await
            .map_err(|_| unavailable())?;
        reply.body().deserialize().map_err(|_| unavailable())
    }

    /// Resolve a client-supplied ID against what BlueZ publishes; it is never used as a path.
    async fn find(
        &self,
        id: &EntityId,
        interface: &str,
    ) -> std::result::Result<(String, HashMap<String, OwnedValue>), Failure> {
        let objects = self.managed().await?;
        objects
            .into_iter()
            .find_map(|(path, mut interfaces)| {
                (path.as_str().starts_with("/org/bluez/")
                    && path.as_str().rsplit('/').next() == Some(id.0.as_str()))
                .then(|| interfaces.remove(interface))
                .flatten()
                .map(|properties| (path.to_string(), properties))
            })
            .ok_or_else(|| {
                Failure::new(
                    ErrorCode::StaleRevision,
                    "Device changed; refresh state",
                    true,
                )
                .dispatch_state(false)
            })
    }

    async fn lookup(&self, id: &EntityId) -> std::result::Result<(String, String, bool), Failure> {
        let (path, device) = self.find(id, "org.bluez.Device1").await?;
        Ok((
            path,
            text(&device, "Alias").unwrap_or_default(),
            flag(&device, "Paired"),
        ))
    }

    /// Read BlueZ and publish the `bluetooth` topic.
    async fn refresh(&self, hub: &Shared) {
        let ready = self.ready.get();
        let state = match (ready, self.managed().await) {
            (true, Ok(objects)) => {
                let paired_with = lock(&self.inner).session.as_ref().map(|s| s.id.clone());
                let state = build_state(&objects, paired_with.as_ref());
                *self.last.borrow_mut() = state.clone();
                state
            }
            (true, Err(error)) => {
                let mut last = self.last.borrow().clone();
                last.health = Health {
                    available: true,
                    stale: true,
                    error: Some(error),
                };
                last
            }
            (false, _) => unavailable_state(),
        };
        debug_assert!(CoreSnapshot::Bluetooth(state.clone()).valid());
        match serde_json::to_value(state) {
            Ok(data) => {
                if hub.borrow_mut().publish(STATE_TOPIC, data).is_err() {
                    tracing::warn!("Bluetooth state could not be published");
                }
            }
            Err(_) => tracing::warn!("Bluetooth state could not be encoded"),
        }
    }

    async fn set_power(
        &self,
        adapter: EntityId,
        powered: bool,
    ) -> std::result::Result<Value, Failure> {
        let (path, properties) = self.find(&adapter, "org.bluez.Adapter1").await?;
        if flag(&properties, "Powered") == powered {
            return Ok(json!({"changed":false}));
        }
        self.call(
            &path,
            "org.freedesktop.DBus.Properties",
            "Set",
            &(
                "org.bluez.Adapter1",
                "Powered",
                zbus::zvariant::Value::from(powered),
            ),
            QUICK,
        )
        .await
        .map_err(|_| unavailable().dispatch_state(true))?;
        if !powered {
            self.stop_timer(&path);
        }
        Ok(json!({"changed":true}))
    }

    fn stop_timer(&self, path: &str) {
        if let Some(task) = self.discovery.borrow_mut().remove(path) {
            task.abort();
        }
    }

    async fn set_discovery(
        self: &Rc<Self>,
        adapter: EntityId,
        discovering: bool,
    ) -> std::result::Result<Value, Failure> {
        let (path, properties) = self.find(&adapter, "org.bluez.Adapter1").await?;
        let method = if discovering {
            "StartDiscovery"
        } else {
            "StopDiscovery"
        };
        let timer_running = self.discovery.borrow().contains_key(&path);
        if !discovering && !timer_running && !flag(&properties, "Discovering") {
            return Ok(json!({"changed":false}));
        }
        let result = self
            .call(&path, "org.bluez.Adapter1", method, &(), QUICK)
            .await;
        if let Err(error) = result {
            // Already scanning (start) or not scanning through this client (stop): as wanted.
            let benign = matches!(&error, zbus::Error::MethodError(name, _, _)
            if matches!(
                (discovering, name.as_str()),
                (true, "org.bluez.Error.InProgress")
                    | (false, "org.bluez.Error.Failed" | "org.bluez.Error.NotReady")
            ));
            if !benign {
                return Err(unavailable().dispatch_state(true));
            }
            if !discovering {
                self.stop_timer(&path);
                return Ok(json!({"changed":false}));
            }
        }
        self.stop_timer(&path);
        if discovering {
            let service = self.clone();
            let stop = path.clone();
            let task = tokio::task::spawn_local(async move {
                tokio::time::sleep(service.discovery_timeout).await;
                service.discovery.borrow_mut().remove(&stop);
                let _ = service
                    .call(&stop, "org.bluez.Adapter1", "StopDiscovery", &(), QUICK)
                    .await;
                let _ = service.changed.send(());
            });
            self.discovery.borrow_mut().insert(path, task);
        }
        Ok(json!({"changed":true}))
    }

    async fn link(&self, device: EntityId, connect: bool) -> std::result::Result<Value, Failure> {
        let (path, properties) = self.find(&device, "org.bluez.Device1").await?;
        if flag(&properties, "Connected") == connect {
            return Ok(json!({"changed":false}));
        }
        let (method, limit) = if connect {
            ("Connect", CONNECT_TIMEOUT)
        } else {
            ("Disconnect", QUICK)
        };
        match self
            .call(&path, "org.bluez.Device1", method, &(), limit)
            .await
        {
            Ok(_) => Ok(json!({"changed":true})),
            Err(zbus::Error::MethodError(name, _, _)) => Err(match name.as_str() {
                "org.bluez.Error.AlreadyConnected" | "org.bluez.Error.NotConnected" => {
                    return Ok(json!({"changed":false}))
                }
                "org.bluez.Error.InProgress" => {
                    Failure::new(ErrorCode::Busy, "Bluetooth device is busy", true)
                        .dispatch_state(true)
                }
                _ => Failure::new(ErrorCode::Unavailable, "Bluetooth connection failed", true)
                    .dispatch_state(true),
            }),
            Err(_) => Err(
                Failure::new(ErrorCode::Timeout, "Bluetooth connection timed out", true)
                    .dispatch_state(true),
            ),
        }
    }

    async fn pair(
        self: &Rc<Self>,
        hub: &Shared,
        id: EntityId,
    ) -> std::result::Result<Value, Failure> {
        let (path, name, paired) = self.lookup(&id).await?;
        if paired {
            return Ok(json!({"changed":false}));
        }
        let owner = zbus::fdo::DBusProxy::new(&self.connection)
            .await
            .map_err(|_| unavailable())?
            .get_name_owner(BLUEZ.try_into().map_err(|_| unavailable())?)
            .await
            .map_err(|_| unavailable())?;
        {
            let mut inner = lock(&self.inner);
            if inner.session.is_some() {
                return Err(
                    Failure::new(ErrorCode::Busy, "Another pairing is in progress", true)
                        .dispatch_state(false),
                );
            }
            inner.owner = Some(owner.to_string());
            inner.session = Some(Session {
                id,
                path: path.clone(),
                name,
                pending: None,
                shown: None,
                outcome: Outcome::Open,
            });
        }
        let mut guard = Guard(Some(self.clone()), path.clone());
        let result = self.pair_session(&path).await;
        guard.0 = None;
        let outcome = self.finish(&path, false).await;
        self.publish(hub);
        match (outcome, result) {
            (Outcome::Open, Ok(changed)) => Ok(json!({"changed":changed})),
            (Outcome::Open, Err(failure)) => Err(failure),
            (outcome, _) => Err(failed(outcome)),
        }
    }

    async fn pair_session(&self, path: &str) -> std::result::Result<bool, Failure> {
        let agent = ObjectPath::try_from(AGENT_PATH).map_err(|_| unavailable())?;
        let register = (agent.clone(), CAPABILITY);
        let manager = "org.bluez.AgentManager1";
        if let Err(error) = self
            .call("/org/bluez", manager, "RegisterAgent", &register, QUICK)
            .await
        {
            // A stale registration of ours (an earlier crash): replace it once.
            let stale = matches!(&error, zbus::Error::MethodError(name, _, _)
                if name.as_str() == "org.bluez.Error.AlreadyExists");
            if !stale
                || self
                    .call("/org/bluez", manager, "UnregisterAgent", &(agent,), QUICK)
                    .await
                    .is_err()
                || self
                    .call("/org/bluez", manager, "RegisterAgent", &register, QUICK)
                    .await
                    .is_err()
            {
                return Err(unavailable().dispatch_state(false));
            }
        }
        match self
            .call(path, "org.bluez.Device1", "Pair", &(), PAIR_TIMEOUT)
            .await
        {
            Ok(_) => Ok(true),
            Err(zbus::Error::MethodError(name, _, _)) => Err(match name.as_str() {
                "org.bluez.Error.AlreadyExists" => return Ok(false),
                "org.bluez.Error.AuthenticationTimeout" => failed(Outcome::TimedOut),
                "org.bluez.Error.AuthenticationRejected" => failed(Outcome::Rejected),
                "org.bluez.Error.AuthenticationCanceled" => failed(Outcome::Cancelled),
                "org.bluez.Error.InProgress" => {
                    Failure::new(ErrorCode::Busy, "Another pairing is in progress", true)
                        .dispatch_state(true)
                }
                "org.bluez.Error.AuthenticationFailed" => Failure::new(
                    ErrorCode::PermissionDenied,
                    "Pairing authentication failed",
                    true,
                )
                .dispatch_state(true),
                _ => unavailable().dispatch_state(true),
            }),
            Err(_) => Err(unavailable().dispatch_state(true)),
        }
    }

    /// Leave no registration or prompt behind. Returns how the session ended.
    async fn finish(&self, path: &str, cancel: bool) -> Outcome {
        let outcome = {
            let mut inner = lock(&self.inner);
            inner.owner = None;
            inner
                .session
                .take()
                .map_or(Outcome::Open, |session| session.outcome)
        };
        let _ = self.changed.send(());
        if cancel {
            let _ = self
                .call(path, "org.bluez.Device1", "CancelPairing", &(), QUICK)
                .await;
        }
        if let Ok(agent) = ObjectPath::try_from(AGENT_PATH) {
            let _ = self
                .call(
                    "/org/bluez",
                    "org.bluez.AgentManager1",
                    "UnregisterAgent",
                    &(agent,),
                    QUICK,
                )
                .await;
        }
        outcome
    }

    fn respond(&self, request: BluetoothPairRespond) -> std::result::Result<Value, Failure> {
        let stale = || {
            Failure::new(
                ErrorCode::StaleRevision,
                "No pairing prompt is waiting",
                false,
            )
            .dispatch_state(false)
        };
        let mut inner = lock(&self.inner);
        let session = inner
            .session
            .as_mut()
            .filter(|s| s.id == request.device)
            .ok_or_else(stale)?;
        let kind = session.pending.as_ref().ok_or_else(stale)?.kind;
        let answer = match (request.accept, kind, request.pin, request.passkey) {
            (false, _, _, _) => Answer::Reject,
            (true, PairingKind::Pin, Some(pin), None) => Answer::Pin(pin),
            (true, PairingKind::Passkey, None, Some(passkey)) => Answer::Passkey(passkey),
            (true, PairingKind::Confirm | PairingKind::Authorize, None, None) => Answer::Accept,
            _ => return Err(invalid()),
        };
        if matches!(answer, Answer::Reject) {
            session.outcome = Outcome::Rejected;
        }
        let pending = session.pending.take().ok_or_else(stale)?;
        pending.reply.send(answer).map_err(|_| stale())?;
        Ok(json!({"changed":true}))
    }

    async fn cancel(&self, device: EntityId) -> std::result::Result<Value, Failure> {
        let path = {
            let mut inner = lock(&self.inner);
            let Some(session) = inner.session.as_mut().filter(|s| s.id == device) else {
                return Ok(json!({"changed":false}));
            };
            session.outcome = Outcome::Cancelled;
            session.pending = None;
            session.path.clone()
        };
        let _ = self.changed.send(());
        let _ = self
            .call(&path, "org.bluez.Device1", "CancelPairing", &(), QUICK)
            .await;
        Ok(json!({"changed":true}))
    }

    /// BlueZ appeared or vanished. A vanished BlueZ abandons any waiting prompt.
    fn set_ready(&self, ready: bool, hub: &Shared) {
        self.ready.set(ready);
        if !ready {
            if let Some(session) = lock(&self.inner).session.as_mut() {
                session.pending = None;
                session.shown = None;
            }
        }
        self.publish(hub);
    }
}

/// A caller that goes away mid-pairing still gets the pairing cancelled and the agent removed.
struct Guard(Option<Rc<Service>>, String);
impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(service) = self.0.take() {
            let path = std::mem::take(&mut self.1);
            tokio::task::spawn_local(async move {
                service.finish(&path, true).await;
            });
        }
    }
}

pub fn initial(hub: &Shared) -> Result<()> {
    let state = BluetoothPairingState {
        health: Health {
            available: false,
            stale: false,
            error: Some(unavailable()),
        },
        prompt: None,
    };
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    hub.borrow_mut()
        .publish(STATE_TOPIC, serde_json::to_value(unavailable_state())?)?;
    Ok(())
}

pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    let call: CoreCall =
        serde_json::from_value(json!({"m": method, "p": params})).map_err(|_| invalid())?;
    if !call.valid() {
        return Err(invalid());
    }
    let service = {
        let state = hub.borrow();
        if state.safe {
            return Err(unavailable());
        }
        state
            .bluetooth
            .clone()
            .filter(|service| service.ready.get())
            .ok_or_else(unavailable)?
    };
    let result = match call {
        CoreCall::BluetoothPair(target) => service.pair(&hub, target.device).await,
        CoreCall::BluetoothPairRespond(request) => service.respond(request),
        CoreCall::BluetoothPairCancel(target) => service.cancel(target.device).await,
        CoreCall::BluetoothPower(p) => service.set_power(p.adapter, p.powered).await,
        CoreCall::BluetoothDiscovery(p) => service.set_discovery(p.adapter, p.discovering).await,
        CoreCall::BluetoothConnect(target) => service.link(target.device, true).await,
        CoreCall::BluetoothDisconnect(target) => service.link(target.device, false).await,
        _ => Err(invalid()),
    };
    service.publish(&hub);
    service.refresh(&hub).await;
    result
}

pub async fn run(hub: Shared) -> Result<()> {
    run_on(
        hub,
        zbus::connection::Builder::system()?,
        PROMPT_TIMEOUT,
        DISCOVERY_TIMEOUT,
    )
    .await
}

async fn run_on(
    hub: Shared,
    builder: zbus::connection::Builder<'static>,
    prompt_timeout: Duration,
    discovery_timeout: Duration,
) -> Result<()> {
    let (changed, mut updates) = mpsc::unbounded_channel();
    let inner = Arc::new(Mutex::new(Inner::default()));
    let connection = builder
        .method_timeout(PAIR_TIMEOUT + QUICK)
        .serve_at(
            AGENT_PATH,
            Agent {
                inner: inner.clone(),
                changed: changed.clone(),
                prompt_timeout,
            },
        )?
        .build()
        .await?;
    let service = Rc::new(Service {
        connection: connection.clone(),
        inner,
        changed,
        ready: Cell::new(false),
        prompt_timeout,
        discovery_timeout,
        discovery: RefCell::default(),
        last: RefCell::new(unavailable_state()),
    });
    hub.borrow_mut().bluetooth = Some(service.clone());
    let bus = zbus::fdo::DBusProxy::new(&connection).await?;
    let mut owners = bus.receive_name_owner_changed().await?;
    let bluez: zbus::names::BusName<'_> = BLUEZ.try_into()?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(BLUEZ)?
        .build();
    let mut signals = zbus::MessageStream::for_match_rule(rule, &connection, Some(256)).await?;
    service.set_ready(bus.name_has_owner(bluez.clone()).await?, &hub);
    service.refresh(&hub).await;
    let mut dirty: Option<tokio::time::Instant> = None;
    loop {
        tokio::select! {
            Some(()) = updates.recv() => {
                service.publish(&hub);
                service.refresh(&hub).await;
            }
            Some(_) = futures_util::StreamExt::next(&mut owners) => {
                service.set_ready(bus.name_has_owner(bluez.clone()).await?, &hub);
                service.refresh(&hub).await;
            }
            Some(_) = futures_util::StreamExt::next(&mut signals) => {
                dirty.get_or_insert(tokio::time::Instant::now() + REFRESH_DELAY);
            }
            () = async {
                match dirty {
                    Some(at) => tokio::time::sleep_until(at).await,
                    None => std::future::pending().await,
                }
            } => {
                dirty = None;
                if service.ready.get() {
                    service.refresh(&hub).await;
                }
            }
            else => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
