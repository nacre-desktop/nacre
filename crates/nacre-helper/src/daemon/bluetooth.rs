//! N0045: Bluetooth pairing agent. `bluetooth.pair` registers a BlueZ `Agent1` on the daemon's own
//! system-bus connection for the duration of one pairing, publishes what the agent is waiting for
//! on the `bluetooth.pairing` topic and relays `bluetooth.pair.respond` / `.cancel` back to BlueZ.
//! The agent is registered for this client only, never as the default agent, so a remote
//! device's own pairing request is not intercepted. Nothing is stored, trusted or logged.
use super::hub::Shared;
use crate::Result;
use nacre_protocol::{
    core_services::{
        pairing_text_valid, BluetoothPairRespond, BluetoothPairingState, CoreCall, CoreSnapshot,
        EntityId, Health, PairingKind, PairingPrompt,
    },
    Capability, ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    cell::Cell,
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
const METHODS: [&str; 3] = [
    "bluetooth.pair",
    "bluetooth.pair.respond",
    "bluetooth.pair.cancel",
];
const BLUEZ: &str = "org.bluez";
const AGENT_PATH: &str = "/org/nacre/bluetooth/agent";
const CAPABILITY: &str = "KeyboardDisplay";
/// A prompt the user does not answer in this time is withdrawn and the pairing times out.
const PROMPT_TIMEOUT: Duration = Duration::from_secs(60);
const PAIR_TIMEOUT: Duration = Duration::from_secs(150);
const QUICK: Duration = Duration::from_secs(5);
const CONTROL_TIMEOUT: u32 = 10_000;

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
                        timeout: if method == "bluetooth.pair" {
                            PAIR_TIMEOUT.as_millis() as u32 + 5_000
                        } else {
                            CONTROL_TIMEOUT
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

    /// Resolve a client-supplied ID against what BlueZ publishes; it is never used as a path.
    async fn lookup(&self, id: &EntityId) -> std::result::Result<(String, String, bool), Failure> {
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
        let objects: Managed = reply.body().deserialize().map_err(|_| unavailable())?;
        let (path, interfaces) = objects
            .iter()
            .find(|(path, interfaces)| {
                path.as_str().starts_with("/org/bluez/")
                    && path.as_str().rsplit('/').next() == Some(id.0.as_str())
                    && interfaces.contains_key("org.bluez.Device1")
            })
            .ok_or_else(|| {
                Failure::new(
                    ErrorCode::StaleRevision,
                    "Device changed; refresh state",
                    true,
                )
                .dispatch_state(false)
            })?;
        let device = &interfaces["org.bluez.Device1"];
        let paired = device
            .get("Paired")
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false);
        let name = device
            .get("Alias")
            .and_then(|v| v.try_clone().ok())
            .and_then(|v| String::try_from(v).ok())
            .unwrap_or_default()
            .chars()
            .filter(|c| !c.is_control())
            .take(64)
            .collect();
        Ok((path.to_string(), name, paired))
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
        _ => Err(invalid()),
    };
    service.publish(&hub);
    result
}

pub async fn run(hub: Shared) -> Result<()> {
    run_on(hub, zbus::connection::Builder::system()?, PROMPT_TIMEOUT).await
}

async fn run_on(
    hub: Shared,
    builder: zbus::connection::Builder<'static>,
    prompt_timeout: Duration,
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
    });
    hub.borrow_mut().bluetooth = Some(service.clone());
    let bus = zbus::fdo::DBusProxy::new(&connection).await?;
    let mut owners = bus.receive_name_owner_changed().await?;
    let bluez: zbus::names::BusName<'_> = BLUEZ.try_into()?;
    service.set_ready(bus.name_has_owner(bluez.clone()).await?, &hub);
    loop {
        tokio::select! {
            Some(()) = updates.recv() => service.publish(&hub),
            Some(_) = futures_util::StreamExt::next(&mut owners) => {
                service.set_ready(bus.name_has_owner(bluez.clone()).await?, &hub);
            }
            else => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests;
