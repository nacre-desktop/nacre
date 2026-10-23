//! Fake BlueZ on a private bus. No real Bluetooth, system bus or session bus is touched.
use super::*;
use crate::daemon::{hub::Hub, test_logs::captured_logs};
use nacre_protocol::core_services::PairingPrompt;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    time::Instant,
};
use tokio::sync::Notify;

const DEVICE: &str = "/org/bluez/hci0/dev_00_11_22_33_44_55";
const PAIRED: &str = "/org/bluez/hci0/dev_AA_BB_CC_DD_EE_FF";
const ID: &str = "dev_00_11_22_33_44_55";
const PAIRED_ID: &str = "dev_AA_BB_CC_DD_EE_FF";
const SECRET_PIN: &str = "PINSECRET7731";
const SECRET_KEY: u32 = 913742;

#[derive(Clone, Copy, Default, PartialEq)]
enum Mode {
    #[default]
    Confirm,
    Pin,
    Passkey,
    Display,
    Authorize,
    /// Wait until released or cancelled.
    Silent,
}
#[derive(Default)]
struct State {
    mode: Mode,
    registered: Vec<(String, String)>,
    unregistered: usize,
    cancelled: usize,
    pairs: usize,
    pin: Option<String>,
    passkey: Option<u32>,
    sender: Option<String>,
}
#[derive(Clone, Default)]
struct Backend {
    state: Arc<Mutex<State>>,
    release: Arc<Notify>,
}
impl Backend {
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap()
    }
}

#[derive(zbus::DBusError, Debug)]
#[zbus(prefix = "org.bluez.Error")]
enum BluezError {
    #[zbus(error)]
    ZBus(zbus::Error),
    AuthenticationRejected(String),
    AuthenticationCanceled(String),
    AuthenticationFailed(String),
}

struct AgentManager(Backend);
#[zbus::interface(name = "org.bluez.AgentManager1")]
impl AgentManager {
    fn register_agent(
        &self,
        agent: ObjectPath<'_>,
        capability: String,
        #[zbus(header)] header: Header<'_>,
    ) {
        let mut state = self.0.state();
        state.registered.push((agent.to_string(), capability));
        state.sender = header.sender().map(|s| s.to_string());
    }
    fn unregister_agent(&self, _agent: ObjectPath<'_>) {
        self.0.state().unregistered += 1;
    }
}

struct Device(Backend, bool);
#[zbus::interface(name = "org.bluez.Device1")]
impl Device {
    #[zbus(property)]
    fn paired(&self) -> bool {
        self.1
    }
    #[zbus(property)]
    fn alias(&self) -> String {
        "Fixture headphones".into()
    }
    fn cancel_pairing(&self) {
        self.0.state().cancelled += 1;
        self.0.release.notify_one();
    }
    async fn pair(
        &self,
        #[zbus(connection)] connection: &zbus::Connection,
    ) -> std::result::Result<(), BluezError> {
        let (mode, sender) = {
            let mut state = self.0.state();
            state.pairs += 1;
            (state.mode, state.sender.clone().unwrap())
        };
        let device = ObjectPath::try_from(DEVICE).unwrap();
        let reply: zbus::Result<zbus::Message> = match mode {
            Mode::Confirm => {
                connection
                    .call_method(
                        Some(sender.as_str()),
                        AGENT_PATH,
                        Some("org.bluez.Agent1"),
                        "RequestConfirmation",
                        &(device, 123456u32),
                    )
                    .await
            }
            Mode::Authorize => {
                connection
                    .call_method(
                        Some(sender.as_str()),
                        AGENT_PATH,
                        Some("org.bluez.Agent1"),
                        "RequestAuthorization",
                        &(device,),
                    )
                    .await
            }
            Mode::Pin => {
                let reply = connection
                    .call_method(
                        Some(sender.as_str()),
                        AGENT_PATH,
                        Some("org.bluez.Agent1"),
                        "RequestPinCode",
                        &(device,),
                    )
                    .await;
                if let Ok(message) = &reply {
                    let pin: String = message.body().deserialize().unwrap();
                    let accepted = pin == SECRET_PIN;
                    self.0.state().pin = Some(pin);
                    if !accepted {
                        return Err(BluezError::AuthenticationFailed("Failed".into()));
                    }
                }
                reply
            }
            Mode::Passkey => {
                let reply = connection
                    .call_method(
                        Some(sender.as_str()),
                        AGENT_PATH,
                        Some("org.bluez.Agent1"),
                        "RequestPasskey",
                        &(device,),
                    )
                    .await;
                if let Ok(message) = &reply {
                    let passkey: u32 = message.body().deserialize().unwrap();
                    self.0.state().passkey = Some(passkey);
                    if passkey != SECRET_KEY {
                        return Err(BluezError::AuthenticationFailed("Failed".into()));
                    }
                }
                reply
            }
            Mode::Display => {
                let shown = connection
                    .call_method(
                        Some(sender.as_str()),
                        AGENT_PATH,
                        Some("org.bluez.Agent1"),
                        "DisplayPasskey",
                        &(device, 7u32, 0u16),
                    )
                    .await;
                if shown.is_ok() {
                    self.0.release.notified().await;
                }
                shown
            }
            Mode::Silent => {
                self.0.release.notified().await;
                if self.0.state().cancelled > 0 {
                    return Err(BluezError::AuthenticationCanceled("Canceled".into()));
                }
                return Ok(());
            }
        };
        match reply {
            Ok(_) => Ok(()),
            Err(zbus::Error::MethodError(name, _, _)) => Err(match name.as_str() {
                "org.bluez.Error.Rejected" => BluezError::AuthenticationRejected("Rejected".into()),
                "org.bluez.Error.Canceled" => BluezError::AuthenticationCanceled("Canceled".into()),
                _ => BluezError::AuthenticationFailed("Failed".into()),
            }),
            Err(error) => Err(BluezError::ZBus(error)),
        }
    }
}

struct Bus(Child);
impl Drop for Bus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Fixture {
    _home: tempfile::TempDir,
    _bus: Bus,
    address: String,
    backend: Backend,
    bluez: Option<zbus::Connection>,
    hub: Shared,
    task: tokio::task::JoinHandle<Result<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn bluez(address: &str, backend: &Backend) -> Result<zbus::Connection> {
    Ok(zbus::connection::Builder::address(address)?
        .name(BLUEZ)?
        .serve_at("/", zbus::fdo::ObjectManager)?
        .serve_at("/org/bluez", AgentManager(backend.clone()))?
        .serve_at(DEVICE, Device(backend.clone(), false))?
        .serve_at(PAIRED, Device(backend.clone(), true))?
        .build()
        .await?)
}
async fn eventually(what: &str, mut done: impl FnMut() -> bool) {
    let start = Instant::now();
    while !done() {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "timed out: {what}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}
impl Fixture {
    async fn new(mode: Mode, with_bluez: bool, prompt_timeout: Duration) -> Result<Self> {
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
        BufReader::new(bus.0.stdout.take().unwrap()).read_line(&mut address)?;
        let address = address.trim().to_owned();
        let backend = Backend::default();
        backend.state().mode = mode;
        let bluez = if with_bluez {
            Some(bluez(&address, &backend).await?)
        } else {
            None
        };
        let hub = Hub::new("bluetooth-boot".into(), false, Duration::from_millis(1)).shared();
        initial(&hub)?;
        let task = tokio::task::spawn_local(run_on(
            hub.clone(),
            zbus::connection::Builder::address(address.as_str())?,
            prompt_timeout,
        ));
        let fixture = Self {
            _home: home,
            _bus: bus,
            address,
            backend,
            bluez,
            hub,
            task,
        };
        let expected = with_bluez;
        eventually("service state", || {
            fixture
                .state()
                .is_some_and(|s| s.health.available == expected)
        })
        .await;
        Ok(fixture)
    }
    async fn ready(mode: Mode) -> Result<Self> {
        Self::new(mode, true, Duration::from_secs(30)).await
    }
    fn state(&self) -> Option<BluetoothPairingState> {
        serde_json::from_value(self.hub.borrow().snapshot(TOPIC)?).ok()
    }
    fn prompt(&self) -> Option<PairingPrompt> {
        self.state()?.prompt
    }
    async fn wait_prompt(&self, kind: PairingKind) -> PairingPrompt {
        eventually("prompt", || self.prompt().is_some_and(|p| p.kind == kind)).await;
        self.prompt().unwrap()
    }
    async fn call(&self, method: &str, params: Value) -> std::result::Result<Value, Failure> {
        dispatch(self.hub.clone(), method, params).await
    }
    fn pair(&self, id: &str) -> tokio::task::JoinHandle<std::result::Result<Value, Failure>> {
        tokio::task::spawn_local(dispatch(
            self.hub.clone(),
            "bluetooth.pair",
            json!({"device":id}),
        ))
    }
    fn service(&self) -> Rc<Service> {
        self.hub.borrow().bluetooth.clone().unwrap()
    }
    async fn settled(&self) {
        eventually("agent unregistered and prompt gone", || {
            self.backend.state().unregistered >= 1 && self.prompt().is_none()
        })
        .await;
    }
}

fn local<F: std::future::Future<Output = Result<()>>>(f: F) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        tokio::task::LocalSet::new()
            .run_until(tokio::time::timeout(Duration::from_secs(60), f))
            .await
    })?
}

#[test]
fn confirmation_is_published_answered_and_the_agent_is_removed() -> Result<()> {
    local(async {
        let f = Fixture::ready(Mode::Confirm).await?;
        let paired = f.pair(ID);
        let prompt = f.wait_prompt(PairingKind::Confirm).await;
        assert_eq!(prompt.device.0, ID);
        assert_eq!(prompt.name, "Fixture headphones");
        assert_eq!(prompt.code.as_deref(), Some("123456"));
        assert_eq!(prompt.timeout_ms, 30_000);
        // The agent is registered for this client with the QML's capability; never the default.
        assert_eq!(
            f.backend.state().registered,
            vec![(AGENT_PATH.to_owned(), CAPABILITY.to_owned())]
        );
        assert_eq!(
            f.call("bluetooth.pair.respond", json!({"device":ID,"accept":true}))
                .await?,
            json!({"changed":true})
        );
        assert_eq!(paired.await??, json!({"changed":true}));
        f.settled().await;
        assert_eq!(f.backend.state().registered.len(), 1);
        Ok(())
    })
}

#[test]
fn authorization_pin_and_passkey_answers_reach_bluez() -> Result<()> {
    local(async {
        let f = Fixture::ready(Mode::Authorize).await?;
        let paired = f.pair(ID);
        let prompt = f.wait_prompt(PairingKind::Authorize).await;
        assert_eq!(prompt.code, None);
        f.call("bluetooth.pair.respond", json!({"device":ID,"accept":true}))
            .await?;
        assert_eq!(paired.await??, json!({"changed":true}));
        f.settled().await;

        f.backend.state().mode = Mode::Pin;
        let paired = f.pair(ID);
        f.wait_prompt(PairingKind::Pin).await;
        f.call(
            "bluetooth.pair.respond",
            json!({"device":ID,"accept":true,"pin":SECRET_PIN}),
        )
        .await?;
        assert_eq!(paired.await??, json!({"changed":true}));
        assert_eq!(f.backend.state().pin.as_deref(), Some(SECRET_PIN));

        f.backend.state().mode = Mode::Passkey;
        let paired = f.pair(ID);
        f.wait_prompt(PairingKind::Passkey).await;
        f.call(
            "bluetooth.pair.respond",
            json!({"device":ID,"accept":true,"passkey":SECRET_KEY}),
        )
        .await?;
        assert_eq!(paired.await??, json!({"changed":true}));
        assert_eq!(f.backend.state().passkey, Some(SECRET_KEY));
        Ok(())
    })
}

#[test]
fn a_wrong_pin_fails_authentication_without_echoing_it() -> Result<()> {
    local(async {
        let f = Fixture::ready(Mode::Pin).await?;
        let paired = f.pair(ID);
        f.wait_prompt(PairingKind::Pin).await;
        f.call(
            "bluetooth.pair.respond",
            json!({"device":ID,"accept":true,"pin":"0000"}),
        )
        .await?;
        let error = paired.await?.unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert_eq!(error.message, "Pairing authentication failed");
        assert!(error.retry);
        Ok(())
    })
}

#[test]
fn a_displayed_code_is_shown_and_needs_no_response() -> Result<()> {
    local(async {
        let f = Fixture::ready(Mode::Display).await?;
        let paired = f.pair(ID);
        let prompt = f.wait_prompt(PairingKind::Display).await;
        assert_eq!(prompt.code.as_deref(), Some("000007"));
        let error = f
            .call("bluetooth.pair.respond", json!({"device":ID,"accept":true}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::StaleRevision);
        f.backend.release.notify_one();
        assert_eq!(paired.await??, json!({"changed":true}));
        f.settled().await;
        Ok(())
    })
}

#[test]
fn rejection_and_cancellation_are_distinct_permission_errors() -> Result<()> {
    local(async {
        let f = Fixture::ready(Mode::Confirm).await?;
        let paired = f.pair(ID);
        f.wait_prompt(PairingKind::Confirm).await;
        f.call(
            "bluetooth.pair.respond",
            json!({"device":ID,"accept":false}),
        )
        .await?;
        let error = paired.await?.unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str(), error.retry),
            (ErrorCode::PermissionDenied, "Pairing was rejected", false)
        );
        f.settled().await;

        let paired = f.pair(ID);
        f.wait_prompt(PairingKind::Confirm).await;
        assert_eq!(
            f.call("bluetooth.pair.cancel", json!({"device":ID}))
                .await?,
            json!({"changed":true})
        );
        let error = paired.await?.unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            (ErrorCode::PermissionDenied, "Authentication was cancelled")
        );
        assert_eq!(f.backend.state().cancelled, 1);
        assert_eq!(
            f.call("bluetooth.pair.cancel", json!({"device":ID}))
                .await?,
            json!({"changed":false}),
            "nothing left to cancel"
        );
        Ok(())
    })
}

#[test]
fn an_unanswered_prompt_times_out() -> Result<()> {
    local(async {
        let f = Fixture::new(Mode::Confirm, true, Duration::from_millis(300)).await?;
        let paired = f.pair(ID);
        assert_eq!(f.wait_prompt(PairingKind::Confirm).await.timeout_ms, 300);
        let error = paired.await?.unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str(), error.retry),
            (ErrorCode::Timeout, "Pairing timed out", true)
        );
        f.settled().await;
        Ok(())
    })
}

#[test]
fn responses_must_match_the_prompt_and_the_device() -> Result<()> {
    local(async {
        let f = Fixture::ready(Mode::Confirm).await?;
        let stale = |e: Failure| e.code == ErrorCode::StaleRevision;
        assert!(stale(
            f.call("bluetooth.pair.respond", json!({"device":ID,"accept":true}))
                .await
                .unwrap_err()
        ));
        let paired = f.pair(ID);
        f.wait_prompt(PairingKind::Confirm).await;
        // A PIN for a confirmation, a response for another device, a malformed response.
        let error = f
            .call(
                "bluetooth.pair.respond",
                json!({"device":ID,"accept":true,"pin":"1234"}),
            )
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Validation);
        assert!(stale(
            f.call(
                "bluetooth.pair.respond",
                json!({"device":PAIRED_ID,"accept":true})
            )
            .await
            .unwrap_err()
        ));
        assert_eq!(
            f.call(
                "bluetooth.pair.respond",
                json!({"device":ID,"accept":false,"pin":"1234"})
            )
            .await
            .unwrap_err()
            .code,
            ErrorCode::Validation
        );
        assert!(f.prompt().is_some(), "a bad answer leaves the prompt open");
        // A second pairing is refused while one runs.
        let busy = f.call("bluetooth.pair", json!({"device":ID})).await;
        assert_eq!(busy.unwrap_err().code, ErrorCode::Busy);
        f.call("bluetooth.pair.respond", json!({"device":ID,"accept":true}))
            .await?;
        paired.await??;
        Ok(())
    })
}

#[test]
fn only_published_unpaired_devices_can_be_paired() -> Result<()> {
    local(async {
        let f = Fixture::ready(Mode::Confirm).await?;
        assert_eq!(
            f.call("bluetooth.pair", json!({"device":PAIRED_ID}))
                .await?,
            json!({"changed":false})
        );
        for id in ["dev_FF_FF_FF_FF_FF_FF", "hci0", "agent", "org.bluez"] {
            let error = f
                .call("bluetooth.pair", json!({"device":id}))
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::StaleRevision, "{id}");
        }
        for bad in ["../dev", "/org/bluez/hci0", ""] {
            let error = f
                .call("bluetooth.pair", json!({"device":bad}))
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Validation, "{bad}");
        }
        let state = f.backend.state();
        assert_eq!(state.pairs, 0);
        assert!(state.registered.is_empty(), "no agent without a pairing");
        Ok(())
    })
}

#[test]
fn only_bluez_may_call_the_agent() -> Result<()> {
    local(async {
        let f = Fixture::ready(Mode::Silent).await?;
        let attacker = zbus::connection::Builder::address(f.address.as_str())?
            .build()
            .await?;
        let me = f.service().connection.unique_name().unwrap().to_string();
        let call = |connection: zbus::Connection, method: &'static str| {
            let me = me.clone();
            async move {
                let device = ObjectPath::try_from(DEVICE).unwrap();
                let (to, path, iface) = (Some(me.as_str()), AGENT_PATH, Some("org.bluez.Agent1"));
                if method == "DisplayPasskey" {
                    connection
                        .call_method(to, path, iface, method, &(device, 1u32, 0u16))
                        .await
                } else {
                    connection
                        .call_method(to, path, iface, method, &(device, 1u32))
                        .await
                }
            }
        };
        let denied = |result: zbus::Result<zbus::Message>| {
            matches!(result, Err(zbus::Error::MethodError(name, _, _))
                if name.as_str() == "org.bluez.Error.Rejected")
        };
        // No pairing yet: even BlueZ itself is refused.
        assert!(denied(
            call(f.bluez.clone().unwrap(), "RequestConfirmation").await
        ));
        let paired = f.pair(ID);
        eventually("pair started", || f.backend.state().pairs == 1).await;
        assert!(denied(call(attacker.clone(), "RequestConfirmation").await));
        assert!(denied(call(attacker, "DisplayPasskey").await));
        assert!(
            f.prompt().is_none(),
            "a foreign caller never raises a prompt"
        );
        f.backend.release.notify_one();
        paired.await??;
        Ok(())
    })
}

#[test]
fn a_vanished_caller_cancels_the_pairing_and_removes_the_agent() -> Result<()> {
    local(async {
        let f = Fixture::ready(Mode::Silent).await?;
        let paired = f.pair(ID);
        eventually("pair started", || f.backend.state().pairs == 1).await;
        paired.abort();
        eventually("cancelled", || {
            let state = f.backend.state();
            state.cancelled == 1 && state.unregistered >= 1
        })
        .await;
        // A new pairing is possible afterwards.
        f.backend.state().cancelled = 0;
        f.backend.state().mode = Mode::Confirm;
        let again = f.pair(ID);
        f.wait_prompt(PairingKind::Confirm).await;
        f.call("bluetooth.pair.respond", json!({"device":ID,"accept":true}))
            .await?;
        again.await??;
        Ok(())
    })
}

#[test]
fn methods_are_advertised_only_while_bluez_is_present() -> Result<()> {
    local(async {
        let mut f = Fixture::new(Mode::Confirm, false, Duration::from_secs(30)).await?;
        let names = |f: &Fixture| -> Vec<String> {
            f.hub
                .borrow()
                .methods()
                .into_keys()
                .filter(|name| name.starts_with("bluetooth."))
                .collect()
        };
        assert!(names(&f).is_empty());
        let error = f
            .call("bluetooth.pair", json!({"device":ID}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        f.bluez = Some(bluez(&f.address, &f.backend).await?);
        eventually("ready", || f.state().is_some_and(|s| s.health.available)).await;
        assert_eq!(
            names(&f),
            [
                "bluetooth.pair",
                "bluetooth.pair.cancel",
                "bluetooth.pair.respond"
            ]
        );
        let methods = f.hub.borrow().methods();
        assert_eq!(methods["bluetooth.pair"].class, Capability::System);
        assert!(methods["bluetooth.pair"].timeout > 150_000);
        assert_eq!(methods["bluetooth.pair.respond"].timeout, CONTROL_TIMEOUT);
        f.bluez = None;
        eventually("gone", || f.state().is_some_and(|s| !s.health.available)).await;
        assert!(names(&f).is_empty());
        Ok(())
    })
}

#[test]
fn pins_and_passkeys_never_reach_logs_errors_state_or_debug() -> Result<()> {
    let sink = captured_logs();
    tracing::info!("pairing secret log capture active");
    let mut observed = String::new();
    local(async {
        let f = Fixture::ready(Mode::Pin).await?;
        let mut seen = String::new();
        let respond = |pin: &str| json!({"device":ID,"accept":true,"pin":pin});
        // Success, a wrong PIN, a rejected one, then a malformed one.
        let paired = f.pair(ID);
        f.wait_prompt(PairingKind::Pin).await;
        seen += &format!("{:?}", f.hub.borrow().snapshot(TOPIC));
        let call: CoreCall =
            serde_json::from_value(json!({"m":"bluetooth.pair.respond","p":respond(SECRET_PIN)}))?;
        seen += &format!("{call:?}");
        f.call("bluetooth.pair.respond", respond(SECRET_PIN))
            .await?;
        seen += &format!("{:?}", paired.await?);
        let paired = f.pair(ID);
        f.wait_prompt(PairingKind::Pin).await;
        f.call("bluetooth.pair.respond", respond("0000")).await?;
        seen += &format!("{:?}", paired.await?);
        for bad in [
            respond("tab\there"),
            json!({"device":ID,"accept":true,"pin":SECRET_PIN,"passkey":SECRET_KEY}),
            json!({"device":ID,"accept":false,"pin":SECRET_PIN}),
        ] {
            seen += &format!("{:?}", f.call("bluetooth.pair.respond", bad).await);
        }
        f.backend.state().mode = Mode::Passkey;
        let paired = f.pair(ID);
        f.wait_prompt(PairingKind::Passkey).await;
        let passkey = json!({"device":ID,"accept":true,"passkey":SECRET_KEY});
        seen += &format!(
            "{:?}",
            f.call(
                "bluetooth.pair.respond",
                json!({"device":PAIRED_ID,"accept":true,"passkey":SECRET_KEY})
            )
            .await
        );
        f.call("bluetooth.pair.respond", passkey).await?;
        seen += &format!("{:?}", paired.await?);
        seen += &format!("{:?}", f.hub.borrow().welcome());
        seen += &serde_json::to_string(&f.hub.borrow().snapshot(TOPIC))?;
        observed = seen;
        Ok(())
    })?;
    assert!(!observed.is_empty());
    for secret in [SECRET_PIN.to_owned(), SECRET_KEY.to_string()] {
        assert!(
            !observed.contains(&secret),
            "secret leaked into state or an error"
        );
    }
    let logs = String::from_utf8(sink.0.lock().unwrap().clone())?;
    assert!(
        logs.contains("pairing secret log capture active"),
        "the capture must be live to be meaningful"
    );
    for secret in [SECRET_PIN.to_owned(), SECRET_KEY.to_string()] {
        assert!(!logs.contains(&secret), "secret leaked into daemon logs");
    }
    Ok(())
}
