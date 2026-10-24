//! N0062: the `vpn` topic and `vpn.connect` / `vpn.disconnect` against a fake NetworkManager
//! on a private bus. No real VPN, secret or network is involved.
use super::*;
use nacre_protocol::core_services::{VpnConnectionState as S, VpnState};
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};

const WORK: &str = "/org/freedesktop/NetworkManager/Settings/1";
const HOME: &str = "/org/freedesktop/NetworkManager/Settings/2";
const WIRED: &str = "/org/freedesktop/NetworkManager/Settings/3";
const ACTIVE_BASE: &str = "/org/freedesktop/NetworkManager/ActiveConnection/";
const UUID_WORK: &str = "aaaaaaaa-1111-4222-8333-444444444444";
const UUID_HOME: &str = "bbbbbbbb-1111-4222-8333-444444444444";
const SECRET: &str = "never-published-vpn-secret-4417";

fn path(value: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(value).unwrap()
}

#[derive(Default)]
struct State {
    /// (active path, profile path, ActiveConnection.State)
    active: Vec<(String, String, u32)>,
    calls: Vec<String>,
    secrets_read: usize,
    deny: bool,
}
#[derive(Clone, Default)]
struct Backend(Arc<Mutex<State>>);

struct Manager(Backend);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![]
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn active_connections(&self) -> Vec<OwnedObjectPath> {
        let state = self.0 .0.lock().unwrap();
        state.active.iter().map(|(a, _, _)| path(a)).collect()
    }
    #[zbus(property)]
    fn primary_connection(&self) -> OwnedObjectPath {
        path("/")
    }
    fn activate_connection(
        &self,
        profile: OwnedObjectPath,
        device: OwnedObjectPath,
        specific: OwnedObjectPath,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!((device.as_str(), specific.as_str()), ("/", "/"));
        let mut state = self.0 .0.lock().unwrap();
        state.calls.push(format!("activate {}", profile.as_str()));
        if state.deny {
            return Err(zbus::fdo::Error::AccessDenied("private detail".into()));
        }
        let active = format!(
            "{ACTIVE_BASE}{}",
            if profile.as_str() == WORK { 1 } else { 2 }
        );
        state.active.push((active.clone(), profile.to_string(), 1));
        Ok(path(&active))
    }
    fn add_and_activate_connection2(
        &self,
        _settings: HashMap<String, HashMap<String, OwnedValue>>,
        _device: OwnedObjectPath,
        _specific: OwnedObjectPath,
        _options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<(
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, OwnedValue>,
    )> {
        Err(zbus::fdo::Error::NotSupported("not used".into()))
    }
    fn deactivate_connection(&self, active: OwnedObjectPath) {
        let mut state = self.0 .0.lock().unwrap();
        state.calls.push(format!("deactivate {}", active.as_str()));
        state.active.retain(|(a, _, _)| a != active.as_str());
    }
}
struct Settings;
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl Settings {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        // Deliberately not in display order.
        vec![path(WORK), path(WIRED), path(HOME)]
    }
}
struct Profile(Backend, &'static str);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Profile {
    fn get_settings(&self) -> HashMap<String, HashMap<String, OwnedValue>> {
        let text = |value: &str| OwnedValue::from(zbus::zvariant::Str::from(value.to_owned()));
        let (id, kind, uuid) = match self.1 {
            WORK => ("Work VPN", "vpn", UUID_WORK),
            HOME => ("home WG", "wireguard", UUID_HOME),
            _ => (
                "Wired",
                "802-3-ethernet",
                "cccccccc-1111-4222-8333-444444444444",
            ),
        };
        let mut settings = HashMap::from([(
            "connection".into(),
            HashMap::from([
                ("id".into(), text(id)),
                ("type".into(), text(kind)),
                ("uuid".into(), text(uuid)),
            ]),
        )]);
        if kind == "vpn" {
            // GetSettings never returns secrets; a faulty daemon must still not publish these.
            settings.insert(
                "vpn".into(),
                HashMap::from([("user-name".into(), text("fixture-user"))]),
            );
        }
        settings
    }
    fn get_secrets(&self, _setting: &str) -> HashMap<String, HashMap<String, OwnedValue>> {
        self.0 .0.lock().unwrap().secrets_read += 1;
        HashMap::from([(
            "vpn".into(),
            HashMap::from([(
                "secrets".into(),
                OwnedValue::from(zbus::zvariant::Str::from(SECRET.to_owned())),
            )]),
        )])
    }
}
struct Active(Backend, &'static str);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn connection(&self) -> OwnedObjectPath {
        let state = self.0 .0.lock().unwrap();
        let profile = state
            .active
            .iter()
            .find(|(a, _, _)| a == self.1)
            .map_or("/", |(_, p, _)| p.as_str());
        path(profile)
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        let state = self.0 .0.lock().unwrap();
        state
            .active
            .iter()
            .find(|(a, _, _)| a == self.1)
            .map_or(0, |(_, _, s)| *s)
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
    backend: Backend,
    server: zbus::Connection,
    hub: Shared,
    task: tokio::task::JoinHandle<crate::Result<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new() -> crate::Result<Self> {
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
        let server = zbus::connection::Builder::address(address.as_str())?
            .name(NM_SERVICE)?
            .serve_at(NM_MANAGER, Manager(backend.clone()))?
            .serve_at("/org/freedesktop/NetworkManager/Settings", Settings)?
            .serve_at(WORK, Profile(backend.clone(), WORK))?
            .serve_at(HOME, Profile(backend.clone(), HOME))?
            .serve_at(WIRED, Profile(backend.clone(), WIRED))?
            .serve_at(
                format!("{ACTIVE_BASE}1"),
                Active(
                    backend.clone(),
                    "/org/freedesktop/NetworkManager/ActiveConnection/1",
                ),
            )?
            .serve_at(
                format!("{ACTIVE_BASE}2"),
                Active(
                    backend.clone(),
                    "/org/freedesktop/NetworkManager/ActiveConnection/2",
                ),
            )?
            .build()
            .await?;
        let client = zbus::connection::Builder::address(address.as_str())?
            .method_timeout(Duration::from_secs(2))
            .build()
            .await?;
        let hub = crate::daemon::hub::Hub::new("vpn-boot".into(), false, Duration::from_millis(1))
            .shared();
        publish_core_starting(&hub)?;
        // Starting state: unavailable and empty, before any NetworkManager read.
        let starting: VpnState = serde_json::from_value(hub.borrow().snapshot(VPN_TOPIC).unwrap())?;
        assert!(!starting.health.available && starting.connections.order.is_empty());
        let mut notices = hub.borrow().observe();
        let projection = new_projection(&hub, &Cell::new(0))?;
        let task = tokio::task::spawn_local(observe_on_connection(
            hub.clone(),
            Rc::new(Cell::new(0)),
            projection,
            client,
            ObserveHooks::default(),
        ));
        loop {
            if hub
                .borrow()
                .snapshot(VPN_TOPIC)
                .is_some_and(|s| s["health"]["available"] == true)
                && hub
                    .borrow()
                    .network
                    .as_ref()
                    .is_some_and(|r| !r.methods().is_empty())
            {
                break;
            }
            notices.recv().await?;
        }
        Ok(Self {
            _home: home,
            _bus: bus,
            backend,
            server,
            hub,
            task,
        })
    }
    fn vpn(&self) -> VpnState {
        serde_json::from_value(self.hub.borrow().snapshot(VPN_TOPIC).unwrap()).unwrap()
    }
    async fn wait(&self, predicate: impl Fn(&VpnState) -> bool) -> VpnState {
        let mut notices = self.hub.borrow().observe();
        loop {
            let state = self.vpn();
            if predicate(&state) {
                return state;
            }
            notices.recv().await.unwrap();
        }
    }
    async fn signal(&self) -> crate::Result<()> {
        self.server
            .emit_signal(
                None::<&str>,
                NM_MANAGER,
                NM_SERVICE,
                "StateChanged",
                &(70u32,),
            )
            .await?;
        Ok(())
    }
    async fn call(&self, method: &str, params: Value) -> std::result::Result<Value, Failure> {
        dispatch(self.hub.clone(), method, params, None).await
    }
    fn id(&self, uuid: &str) -> EntityId {
        let state = self.vpn();
        state
            .connections
            .items
            .iter()
            .find(|(_, c)| c.uuid == uuid)
            .map(|(id, _)| id.clone())
            .unwrap()
    }
}

fn set_state(f: &Fixture, state: u32) {
    for entry in f.backend.0.lock().unwrap().active.iter_mut() {
        entry.2 = state;
    }
}

#[tokio::test(flavor = "current_thread")]
async fn vpn_topic_lists_only_vpn_profiles_with_network_ids_and_no_secrets() -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            let vpn = f.vpn();
            assert!(vpn.health.available && !vpn.health.stale && vpn.health.error.is_none());
            assert!(CoreSnapshot::Vpn(vpn.clone()).valid());
            // Ordered by name (case-insensitive): "home WG" before "Work VPN"; no wired profile.
            let rows: Vec<_> = vpn
                .connections
                .order
                .iter()
                .map(|id| {
                    let c = &vpn.connections.items[id];
                    (
                        c.name.as_str(),
                        c.kind.as_str(),
                        c.uuid.as_str(),
                        c.state,
                        c.active,
                    )
                })
                .collect();
            assert_eq!(
                rows,
                vec![
                    ("home WG", "wireguard", UUID_HOME, S::Disconnected, false),
                    ("Work VPN", "vpn", UUID_WORK, S::Disconnected, false),
                ]
            );
            // The IDs are the `network` topic's connection IDs.
            let network: NetworkState =
                serde_json::from_value(f.hub.borrow().snapshot(CORE_TOPIC).unwrap())?;
            assert!(vpn
                .connections
                .order
                .iter()
                .all(|id| network.connections.items.contains_key(id)));
            let text = serde_json::to_string(&f.hub.borrow().snapshot(VPN_TOPIC).unwrap())?;
            assert!(!text.contains(SECRET) && !text.contains("fixture-user"));
            assert_eq!(f.backend.0.lock().unwrap().secrets_read, 0);
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn vpn_connect_and_disconnect_are_admission_with_state_readback() -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            let work = f.id(UUID_WORK);
            assert_eq!(
                f.call("vpn.disconnect", json!({"connection":work})).await?,
                json!({"changed":false})
            );
            assert_eq!(
                f.call("vpn.connect", json!({"connection":work})).await?,
                json!({"changed":true})
            );
            assert_eq!(
                f.backend.0.lock().unwrap().calls,
                [format!("activate {WORK}")]
            );
            // Admission is not completion: the topic follows NetworkManager's own state.
            f.wait(|v| {
                v.connections.items[&work].state == S::Connecting
                    && v.connections.items[&work].active
            })
            .await;
            set_state(&f, 2);
            f.signal().await?;
            let state = f
                .wait(|v| v.connections.items[&work].state == S::Connected)
                .await;
            assert!(state.connections.items[&work].active);
            assert_eq!(
                f.call("vpn.connect", json!({"connection":work})).await?,
                json!({"changed":false})
            );
            set_state(&f, 3);
            f.signal().await?;
            f.wait(|v| v.connections.items[&work].state == S::Disconnecting)
                .await;
            set_state(&f, 2);
            f.signal().await?;
            assert_eq!(
                f.call("vpn.disconnect", json!({"connection":work})).await?,
                json!({"changed":true})
            );
            f.wait(|v| {
                v.connections.items[&work].state == S::Disconnected
                    && !v.connections.items[&work].active
            })
            .await;
            assert_eq!(
                f.backend.0.lock().unwrap().calls.last().unwrap(),
                &format!("deactivate {ACTIVE_BASE}1")
            );
            assert_eq!(f.backend.0.lock().unwrap().secrets_read, 0);
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn vpn_activation_that_fails_later_is_failed_until_retried() -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            let home = f.id(UUID_HOME);
            assert_eq!(
                f.call("vpn.connect", json!({"connection":home})).await?,
                json!({"changed":true})
            );
            f.wait(|v| v.connections.items[&home].state == S::Connecting)
                .await;
            // NetworkManager gives up (reason 9, no secrets) and drops the active connection.
            f.backend.0.lock().unwrap().active.clear();
            f.server
                .emit_signal(
                    None::<&str>,
                    format!("{ACTIVE_BASE}2"),
                    "org.freedesktop.NetworkManager.Connection.Active",
                    "StateChanged",
                    &(4u32, 9u32),
                )
                .await?;
            let state = f
                .wait(|v| v.connections.items[&home].state == S::Failed)
                .await;
            assert!(!state.connections.items[&home].active);
            assert!(CoreSnapshot::Vpn(state).valid());
            // A user disconnect (reason 2) is not a failure; the next attempt clears `failed`.
            assert_eq!(
                f.call("vpn.connect", json!({"connection":home})).await?,
                json!({"changed":true})
            );
            f.wait(|v| v.connections.items[&home].state == S::Connecting)
                .await;
            set_state(&f, 2);
            f.backend.0.lock().unwrap().active.clear();
            f.server
                .emit_signal(
                    None::<&str>,
                    format!("{ACTIVE_BASE}2"),
                    "org.freedesktop.NetworkManager.Connection.Active",
                    "StateChanged",
                    &(4u32, 2u32),
                )
                .await?;
            f.wait(|v| v.connections.items[&home].state == S::Disconnected)
                .await;
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn vpn_calls_reject_stale_wrong_kind_denied_and_invalid() -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            let network: NetworkState =
                serde_json::from_value(f.hub.borrow().snapshot(CORE_TOPIC).unwrap())?;
            let wired = network
                .connections
                .order
                .iter()
                .find(|id| network.connections.items[*id].kind == "ethernet")
                .unwrap()
                .clone();
            for (method, target) in [
                ("vpn.connect", wired.clone()),
                ("vpn.disconnect", wired),
                ("vpn.connect", EntityId("net:never:1:99".into())),
            ] {
                let error = f
                    .call(method, json!({"connection":target}))
                    .await
                    .unwrap_err();
                assert_eq!(error.code, ErrorCode::StaleRevision);
            }
            let error = f
                .call("vpn.connect", json!({"connection":"../x"}))
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Validation);
            let error = f.call("vpn.connect", json!({})).await.unwrap_err();
            assert_eq!(error.code, ErrorCode::Validation);
            f.backend.0.lock().unwrap().deny = true;
            let error = f
                .call("vpn.connect", json!({"connection":f.id(UUID_WORK)}))
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::PermissionDenied);
            assert!(!error.message.contains("private detail"));
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn vpn_is_unavailable_without_networkmanager_and_methods_follow_readiness(
) -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            // No NetworkManager task at all: the topic is the unavailable starting state and
            // no vpn method is advertised or accepted.
            let hub = crate::daemon::hub::Hub::new("none".into(), false, Duration::from_millis(1))
                .shared();
            publish_core_starting(&hub)?;
            let vpn: VpnState = serde_json::from_value(hub.borrow().snapshot(VPN_TOPIC).unwrap())?;
            assert!(!vpn.health.available && vpn.connections.items.is_empty());
            assert!(!hub.borrow().methods().keys().any(|m| m.starts_with("vpn.")));
            let error = dispatch(hub, "vpn.connect", json!({"connection":"x"}), None)
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Unavailable);

            // A live fixture advertises both methods; a failed read keeps the profiles, stale.
            let f = Fixture::new().await?;
            let methods = f.hub.borrow().methods();
            for name in ["vpn.connect", "vpn.disconnect"] {
                assert!((1..=120_000).contains(&methods[name].timeout));
            }
            f.hub
                .borrow()
                .welcome()
                .validate()
                .expect("welcome with vpn methods validates");
            let before = f.vpn();
            let mut projection = new_projection(&f.hub, &Cell::new(5))?;
            projection.publish_failure(&f.hub, CoreError::Unavailable)?;
            let stale = f.vpn();
            assert!(!stale.health.available && stale.health.stale && stale.health.error.is_some());
            assert_eq!(stale.connections, before.connections);
            assert!(!f
                .hub
                .borrow()
                .methods()
                .keys()
                .any(|m| m.starts_with("vpn.")));
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}
