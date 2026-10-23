use super::*;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex},
};
use tokio::sync::Notify;

const DEVICE: &str = "/org/freedesktop/NetworkManager/Devices/1";
const SECOND: &str = "/org/freedesktop/NetworkManager/Devices/2";
const AP: &str = "/org/freedesktop/NetworkManager/AccessPoint/1";
const PROFILE: &str = "/org/freedesktop/NetworkManager/Settings/1";
const ACTIVE: &str = "/org/freedesktop/NetworkManager/ActiveConnection/1";
const CREATED: &str = "/org/freedesktop/NetworkManager/Settings/2";
const SECRET: &str = "correct horse battery staple 7731";
fn path(value: &str) -> OwnedObjectPath {
    OwnedObjectPath::try_from(value).unwrap()
}

#[derive(Default)]
struct State {
    wifi: bool,
    active: bool,
    gone: bool,
    point_gone: bool,
    incompatible: bool,
    lookup_mode: u8,
    shared: bool,
    auth_failed: bool,
    mode: u8,
    calls: Vec<String>,
    // What the last AddAndActivateConnection2 received (N0042).
    added_sections: Vec<String>,
    added_persist: String,
    added_ssid: Option<Vec<u8>>,
    added_hidden: bool,
    added_key_mgmt: Option<String>,
    added_psk: Option<String>,
    created: bool,
    created_active: bool,
    deleted: Vec<String>,
}
#[derive(Clone, Default)]
struct Backend {
    state: Arc<Mutex<State>>,
    started: Arc<Notify>,
    release: Arc<Notify>,
}
impl Backend {
    async fn admission(&self, method: &str) -> zbus::fdo::Result<()> {
        let mode = {
            let mut state = self.state.lock().unwrap();
            state.calls.push(method.into());
            state.mode
        };
        self.started.notify_one();
        match mode {
            1 => Err(zbus::fdo::Error::AccessDenied(
                "private payload forbidden".into(),
            )),
            2 => Err(zbus::fdo::Error::Failed("private backend detail".into())),
            4 => Err(zbus::fdo::Error::UnknownObject(
                "private vanished target".into(),
            )),
            3 | 6 => {
                self.release.notified().await;
                Ok(())
            }
            _ => Ok(()),
        }
    }
}
struct Manager(Backend);
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl Manager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        if self.0.state.lock().unwrap().gone {
            vec![]
        } else {
            vec![path(DEVICE), path(SECOND)]
        }
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        self.0.state.lock().unwrap().wifi
    }
    #[zbus(property)]
    async fn set_wireless_enabled(&self, enabled: bool) -> zbus::fdo::Result<()> {
        self.0.admission("wifi").await?;
        let mut state = self.0.state.lock().unwrap();
        if !matches!(state.mode, 5 | 6) {
            state.wifi = enabled;
        }
        Ok(())
    }
    #[zbus(property)]
    fn active_connections(&self) -> Vec<OwnedObjectPath> {
        if self.0.state.lock().unwrap().active {
            vec![path(ACTIVE)]
        } else {
            vec![]
        }
    }
    #[zbus(property)]
    fn primary_connection(&self) -> OwnedObjectPath {
        if self.0.state.lock().unwrap().active {
            path(ACTIVE)
        } else {
            path("/")
        }
    }
    async fn activate_connection(
        &self,
        profile: OwnedObjectPath,
        device: OwnedObjectPath,
        specific: OwnedObjectPath,
    ) -> zbus::fdo::Result<OwnedObjectPath> {
        assert_eq!(profile.as_str(), PROFILE);
        assert_eq!(device.as_str(), DEVICE);
        assert_eq!(specific.as_str(), "/");
        self.0.admission("connect").await?;
        let mut state = self.0.state.lock().unwrap();
        if !matches!(state.mode, 5 | 6) {
            state.active = true;
        }
        Ok(path(ACTIVE))
    }
    async fn add_and_activate_connection2(
        &self,
        settings: HashMap<String, HashMap<String, OwnedValue>>,
        device: OwnedObjectPath,
        specific: OwnedObjectPath,
        options: HashMap<String, OwnedValue>,
    ) -> zbus::fdo::Result<(
        OwnedObjectPath,
        OwnedObjectPath,
        HashMap<String, OwnedValue>,
    )> {
        assert_eq!(device.as_str(), DEVICE);
        let hidden = specific.as_str() == "/";
        assert!(hidden || specific.as_str() == AP);
        let text = |section: &str, key: &str| -> Option<String> {
            let value = settings.get(section)?.get(key)?;
            <&str>::try_from(value).ok().map(str::to_owned)
        };
        {
            let mut state = self.0.state.lock().unwrap();
            let mut sections: Vec<String> = settings.keys().cloned().collect();
            sections.sort();
            state.added_sections = sections;
            state.added_persist = <&str>::try_from(&options["persist"]).unwrap().to_owned();
            state.added_ssid = settings
                .get("802-11-wireless")
                .and_then(|section| section.get("ssid"))
                .and_then(|value| Vec::<u8>::try_from(value.try_clone().unwrap()).ok());
            state.added_hidden = settings
                .get("802-11-wireless")
                .and_then(|section| section.get("hidden"))
                .is_some_and(|value| bool::try_from(value.try_clone().unwrap()).unwrap());
            state.added_key_mgmt = text("802-11-wireless-security", "key-mgmt");
            state.added_psk = text("802-11-wireless-security", "psk");
        }
        self.0.admission("access_point").await?;
        let mut state = self.0.state.lock().unwrap();
        let new = state.added_persist == "to-disk";
        if !matches!(state.mode, 5 | 6) {
            state.active = true;
            state.created_active = new;
        }
        state.created |= new;
        if new {
            state.deleted.retain(|object| object != CREATED);
        }
        Ok((
            path(if new { CREATED } else { PROFILE }),
            path(ACTIVE),
            HashMap::new(),
        ))
    }
    async fn deactivate_connection(&self, active: OwnedObjectPath) -> zbus::fdo::Result<()> {
        assert_eq!(active.as_str(), ACTIVE);
        self.0.admission("disconnect").await?;
        self.0.state.lock().unwrap().active = false;
        Ok(())
    }
}
struct ReadOnlyManager;
#[zbus::interface(name = "org.freedesktop.NetworkManager")]
impl ReadOnlyManager {
    fn get_devices(&self) -> Vec<OwnedObjectPath> {
        vec![]
    }
    #[zbus(property)]
    fn wireless_enabled(&self) -> bool {
        false
    }
    #[zbus(property)]
    fn active_connections(&self) -> Vec<OwnedObjectPath> {
        vec![]
    }
    #[zbus(property)]
    fn primary_connection(&self) -> OwnedObjectPath {
        path("/")
    }
}
struct DeviceFixture(Backend, bool);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
impl DeviceFixture {
    #[zbus(property)]
    fn device_type(&self) -> u32 {
        if self.1 {
            1
        } else {
            2
        }
    }
    #[zbus(property)]
    fn interface(&self) -> &str {
        "fixture0"
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        let state = self.0.state.lock().unwrap();
        if state.auth_failed {
            120
        } else if state.active {
            100
        } else {
            30
        }
    }
    #[zbus(property)]
    fn state_reason(&self) -> (u32, u32) {
        let state = self.0.state.lock().unwrap();
        if state.auth_failed {
            (120, 7)
        } else if state.active {
            (100, 0)
        } else {
            (30, 0)
        }
    }
    #[zbus(property)]
    fn active_connection(&self) -> OwnedObjectPath {
        if !self.1 && self.0.state.lock().unwrap().active {
            path(ACTIVE)
        } else {
            path("/")
        }
    }
    #[zbus(property)]
    fn available_connections(&self) -> zbus::fdo::Result<Vec<OwnedObjectPath>> {
        let state = self.0.state.lock().unwrap();
        match state.lookup_mode {
            1 => Err(zbus::fdo::Error::UnknownObject(
                "private vanished device".into(),
            )),
            2 => Err(zbus::fdo::Error::Failed("private service failure".into())),
            3 => Err(zbus::fdo::Error::AccessDenied(
                "private policy failure".into(),
            )),
            _ => Ok(if state.incompatible {
                vec![]
            } else {
                vec![path(PROFILE)]
            }),
        }
    }
}
struct Wireless(Backend);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
impl Wireless {
    #[zbus(property)]
    fn active_access_point(&self) -> OwnedObjectPath {
        if self.0.state.lock().unwrap().active {
            path(AP)
        } else {
            path("/")
        }
    }
    #[zbus(property)]
    fn access_points(&self) -> Vec<OwnedObjectPath> {
        if self.0.state.lock().unwrap().point_gone {
            vec![]
        } else {
            vec![path(AP)]
        }
    }
}
struct Point;
#[zbus::interface(name = "org.freedesktop.NetworkManager.AccessPoint")]
impl Point {
    #[zbus(property)]
    fn ssid(&self) -> Vec<u8> {
        b"Fixture".to_vec()
    }
    #[zbus(property)]
    fn strength(&self) -> u8 {
        60
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
        // PSK key management, as a WPA2-Personal access point reports it.
        0x100
    }
}
struct Settings(Backend);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
impl Settings {
    fn list_connections(&self) -> Vec<OwnedObjectPath> {
        let state = self.0.state.lock().unwrap();
        [(PROFILE, true), (CREATED, state.created)]
            .into_iter()
            .filter(|(object, exists)| *exists && !state.deleted.iter().any(|d| d == object))
            .map(|(object, _)| path(object))
            .collect()
    }
}
struct Profile(Backend, &'static str);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
impl Profile {
    fn delete(&self) {
        let mut state = self.0.state.lock().unwrap();
        state.calls.push("forget".into());
        state.deleted.push(self.1.into());
    }
    fn get_settings(&self) -> HashMap<String, HashMap<String, OwnedValue>> {
        let text = |value: &str| OwnedValue::from(zbus::zvariant::Str::from(value.to_owned()));
        let saved = self.1 == PROFILE;
        let mut settings = HashMap::from([
            (
                "connection".into(),
                HashMap::from([
                    ("id".into(), text(if saved { "Fixture" } else { "Hidden" })),
                    ("type".into(), text("802-11-wireless")),
                    ("autoconnect".into(), OwnedValue::from(!saved)),
                    ("timestamp".into(), OwnedValue::from(1_700_000_000u64)),
                ]),
            ),
            (
                "802-11-wireless".into(),
                HashMap::from([(
                    "ssid".into(),
                    zbus::zvariant::Value::from(b"Fixture".to_vec())
                        .try_into()
                        .unwrap(),
                )]),
            ),
        ]);
        if saved {
            settings.insert(
                "802-11-wireless-security".into(),
                HashMap::from([("key-mgmt".into(), text("wpa-psk"))]),
            );
        }
        settings
    }
}
struct Active(Backend);
#[zbus::interface(name = "org.freedesktop.NetworkManager.Connection.Active")]
impl Active {
    #[zbus(property)]
    fn connection(&self) -> OwnedObjectPath {
        path(if self.0.state.lock().unwrap().created_active {
            CREATED
        } else {
            PROFILE
        })
    }
    #[zbus(property)]
    fn devices(&self) -> zbus::fdo::Result<Vec<OwnedObjectPath>> {
        let state = self.0.state.lock().unwrap();
        match state.lookup_mode {
            1 => Err(zbus::fdo::Error::UnknownObject(
                "private vanished active connection".into(),
            )),
            2 => Err(zbus::fdo::Error::Failed("private service failure".into())),
            3 => Err(zbus::fdo::Error::AccessDenied(
                "private policy failure".into(),
            )),
            _ => Ok(if state.shared {
                vec![path(DEVICE), path("/other")]
            } else {
                vec![path(DEVICE)]
            }),
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
    server: zbus::Connection,
    hub: Shared,
    task: tokio::task::JoinHandle<crate::Result<()>>,
    events: tokio::sync::mpsc::UnboundedReceiver<crate::network_state::NetworkEvent>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn server(address: &str, backend: Backend) -> crate::Result<zbus::Connection> {
        Self::server_with(address, backend, false).await
    }
    async fn server_with(
        address: &str,
        backend: Backend,
        read_only: bool,
    ) -> crate::Result<zbus::Connection> {
        let builder = zbus::connection::Builder::address(address)?.name(NM_SERVICE)?;
        let builder = if read_only {
            builder.serve_at(NM_MANAGER, ReadOnlyManager)?
        } else {
            builder.serve_at(NM_MANAGER, Manager(backend.clone()))?
        };
        Ok(builder
            .serve_at(DEVICE, DeviceFixture(backend.clone(), false))?
            .serve_at(SECOND, DeviceFixture(backend.clone(), true))?
            .serve_at(DEVICE, Wireless(backend.clone()))?
            .serve_at(AP, Point)?
            .serve_at(
                "/org/freedesktop/NetworkManager/Settings",
                Settings(backend.clone()),
            )?
            .serve_at(PROFILE, Profile(backend.clone(), PROFILE))?
            .serve_at(CREATED, Profile(backend.clone(), CREATED))?
            .serve_at(ACTIVE, Active(backend))?
            .build()
            .await?)
    }
    async fn new() -> crate::Result<Self> {
        Self::new_with(false).await
    }
    async fn new_with(read_only: bool) -> crate::Result<Self> {
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
        let server = Self::server_with(&address, backend.clone(), read_only).await?;
        let client = zbus::connection::Builder::address(address.as_str())?
            .method_timeout(Duration::from_secs(2))
            .build()
            .await?;
        let hub =
            crate::daemon::hub::Hub::new("controls-boot-a".into(), false, Duration::from_millis(1))
                .shared();
        publish_core_starting(&hub)?;
        let mut notices = hub.borrow().observe();
        let projection = new_projection(&hub, &Cell::new(0))?;
        let (events_tx, events) = tokio::sync::mpsc::unbounded_channel();
        let task = tokio::task::spawn_local(observe_on_connection(
            hub.clone(),
            Rc::new(Cell::new(0)),
            projection,
            client,
            ObserveHooks {
                watcher_events: Some(events_tx),
                ..ObserveHooks::default()
            },
        ));
        loop {
            if hub
                .borrow()
                .snapshot(CORE_TOPIC)
                .is_some_and(|s| s["health"]["available"] == true)
            {
                break;
            }
            notices.recv().await?;
        }
        Ok(Self {
            _home: home,
            _bus: bus,
            address,
            backend,
            server,
            hub,
            task,
            events,
        })
    }
    fn snapshot(&self) -> NetworkState {
        serde_json::from_value(self.hub.borrow().snapshot(CORE_TOPIC).unwrap()).unwrap()
    }
    async fn wait(&self, predicate: impl Fn(&NetworkState) -> bool) -> NetworkState {
        let mut notices = self.hub.borrow().observe();
        loop {
            let state = self.snapshot();
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
    fn device(&self) -> EntityId {
        self.snapshot().devices.order[0].clone()
    }
}

#[tokio::test(flavor = "current_thread")]
async fn network_controls_admission_and_authoritative_readback() -> crate::Result<()> {
    tokio::time::timeout(Duration::from_secs(20), tokio::task::LocalSet::new().run_until(async {
        let f = Fixture::new().await?;
        let state = f.snapshot();
        assert_eq!(f.call("network.set_wifi", json!({"enabled":true})).await?, json!({"changed":true}));
        f.wait(|s| s.wireless_enabled).await;
        assert_eq!(f.call("network.set_wifi", json!({"enabled":true})).await?, json!({"changed":false}));
        for target in [json!({"kind":"access_point","id":state.access_points.order[0]}), json!({"kind":"connection","id":state.connections.order[0]})] {
            assert_eq!(f.call("network.connect", json!({"device":f.device(),"target":target})).await?, json!({"changed":true}));
            f.wait(|s| s.devices.items.values().any(|d| d.connected) && s.primary_connection.is_some()).await;
            assert_eq!(f.call("network.disconnect", json!({"device":f.device()})).await?, json!({"changed":true}));
            f.wait(|s| s.primary_connection.is_none() && s.devices.items.values().all(|d| !d.connected)).await;
        }
        assert_eq!(f.call("network.disconnect", json!({"device":f.device()})).await?, json!({"changed":false}));
        // Acknowledgement alone never changes the state; later auth failure is a topic error.
        f.backend.state.lock().unwrap().mode = 5;
        assert_eq!(f.call("network.connect", json!({"device":f.device(),"target":{"kind":"access_point","id":state.access_points.order[0]}})).await?, json!({"changed":true}));
        assert!(f.snapshot().devices.items.values().all(|d| !d.connected));
        f.backend.state.lock().unwrap().auth_failed = true;
        f.signal().await?;
        let failed = f.wait(|s| s.health.error.is_some()).await;
        assert!(failed.health.available);
        let error = failed.health.error.unwrap();
        assert_eq!(error.message, "Network authentication required or cancelled");
        assert!(error.retry);
        f.backend.state.lock().unwrap().auth_failed = false;
        f.signal().await?;
        f.wait(|s| s.health.error.is_none()).await;
        Ok::<(), crate::Error>(())
    })).await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn network_controls_stale_wrong_kind_cross_device_and_denied() -> crate::Result<()> {
    tokio::time::timeout(Duration::from_secs(20), tokio::task::LocalSet::new().run_until(async {
        let f = Fixture::new().await?;
        let state = f.snapshot();
        let device = f.device();
        for params in [json!({"device":"net:old:1"}), json!({"device":state.connections.order[0]})] {
            assert_eq!(f.call("network.disconnect", params).await.unwrap_err().code, ErrorCode::StaleRevision);
        }
        for target in [json!({"kind":"access_point","id":device}), json!({"kind":"connection","id":state.access_points.order[0]})] {
            assert_eq!(f.call("network.connect", json!({"device":device,"target":target})).await.unwrap_err().code, ErrorCode::StaleRevision);
        }
        assert_eq!(f.call("network.connect", json!({"device":state.devices.order[1],"target":{"kind":"access_point","id":state.access_points.order[0]}})).await.unwrap_err().code, ErrorCode::StaleRevision);
        f.backend.state.lock().unwrap().incompatible = true;
        assert_eq!(f.call("network.connect", json!({"device":device,"target":{"kind":"connection","id":state.connections.order[0]}})).await.unwrap_err().code, ErrorCode::StaleRevision);
        f.backend.state.lock().unwrap().point_gone = true;
        assert_eq!(f.call("network.connect", json!({"device":device,"target":{"kind":"access_point","id":state.access_points.order[0]}})).await.unwrap_err().code, ErrorCode::StaleRevision);
        f.backend.state.lock().unwrap().gone = true;
        assert_eq!(f.call("network.disconnect", json!({"device":device})).await.unwrap_err().code, ErrorCode::StaleRevision);
        f.backend.state.lock().unwrap().gone = false;
        assert!(f.backend.state.lock().unwrap().calls.is_empty());
        for (mode, code) in [(1, ErrorCode::PermissionDenied), (2, ErrorCode::Unavailable), (4, ErrorCode::StaleRevision)] {
            f.backend.state.lock().unwrap().mode = mode;
            let error = f.call("network.set_wifi", json!({"enabled":true})).await.unwrap_err();
            assert_eq!(error.code, code);
            assert!(!error.message.contains("private"));
            assert!(!f.snapshot().wireless_enabled);
        }
        for params in [json!({}), json!({"enabled":"yes"})] {
            assert_eq!(f.call("network.set_wifi", params).await.unwrap_err().code, ErrorCode::Validation);
        }
        assert_eq!(f.call("network.disconnect", json!({"device":"/untrusted/path"})).await.unwrap_err().code, ErrorCode::Validation);
        f.backend.state.lock().unwrap().mode = 0;
        f.backend.state.lock().unwrap().incompatible = false;
        for (mode, code) in [(1, ErrorCode::StaleRevision), (2, ErrorCode::Unavailable), (3, ErrorCode::PermissionDenied)] {
            f.backend.state.lock().unwrap().lookup_mode = mode;
            let profile = f.snapshot().connections.order[0].clone();
            assert_eq!(f.call("network.connect", json!({"device":f.device(),"target":{"kind":"connection","id":profile}})).await.unwrap_err().code, code);
            f.backend.state.lock().unwrap().active = true;
            assert_eq!(f.call("network.disconnect", json!({"device":f.device()})).await.unwrap_err().code, code);
        }
        f.backend.state.lock().unwrap().lookup_mode = 0;
        f.backend.state.lock().unwrap().active = true;
        f.backend.state.lock().unwrap().shared = true;
        assert_eq!(f.call("network.disconnect", json!({"device":f.device()})).await.unwrap_err().code, ErrorCode::StaleRevision);
        Ok::<(), crate::Error>(())
    })).await??;
    Ok(())
}

async fn socket_client(
    hub: Shared,
) -> crate::Result<(
    tokio::net::unix::OwnedWriteHalf,
    tokio::io::BufReader<tokio::net::unix::OwnedReadHalf>,
    tokio::task::JoinHandle<std::result::Result<(), Failure>>,
    Message,
)> {
    let (server, client) = tokio::net::UnixStream::pair()?;
    let task = tokio::task::spawn_local(crate::daemon::session::serve(server, hub));
    let (reader, mut writer) = client.into_split();
    let mut reader = tokio::io::BufReader::new(reader);
    framing::write(
        &mut writer,
        &Message::Hello {
            proto: 1,
            min: Some(1),
            client: "private-network-fixture".into(),
        },
    )
    .await?;
    let welcome = framing::read(&mut reader).await?.unwrap();
    Ok((writer, reader, task, welcome))
}
fn advertised(welcome: Message) -> BTreeMap<String, Method> {
    let Message::Welcome { methods, .. } = welcome else {
        panic!("expected welcome");
    };
    methods
        .into_iter()
        .filter(|(name, _)| name.starts_with("network."))
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn network_controls_welcome_and_boot_scoping() -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let hub = crate::daemon::hub::Hub::new("empty".into(), false, Duration::from_millis(1))
                .shared();
            let (writer, reader, task, welcome) = socket_client(hub.clone()).await?;
            assert!(advertised(welcome).is_empty());
            assert_eq!(
                dispatch(hub, "network.set_wifi", json!({"enabled":true}), None)
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::Unavailable
            );
            drop(writer);
            drop(reader);
            task.await??;
            let read_only = Fixture::new_with(true).await?;
            assert!(read_only.snapshot().health.available);
            let (writer, reader, task, welcome) = socket_client(read_only.hub.clone()).await?;
            assert!(advertised(welcome).is_empty());
            assert_eq!(
                read_only
                    .call("network.set_wifi", json!({"enabled":true}))
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::Unavailable
            );
            drop(writer);
            drop(reader);
            task.await??;
            let f = Fixture::new().await?;
            let (mut writer, mut reader, task, welcome) = socket_client(f.hub.clone()).await?;
            let methods = advertised(welcome);
            assert_eq!(methods.len(), 4);
            assert!(methods.values().all(|m| m.v == 1 && m.timeout == TIMEOUT));
            framing::write(
                &mut writer,
                &Message::Call {
                    id: Id::Integer(1),
                    m: "network.set_wifi".into(),
                    p: json!({"enabled":true}),
                    timeout: None,
                },
            )
            .await?;
            let Message::Ret {
                reply: nacre_protocol::Reply::Ok { ok },
                ..
            } = framing::read(&mut reader).await?.unwrap()
            else {
                panic!("expected success");
            };
            assert_eq!(ok, json!({"changed":true}));
            f.wait(|s| s.wireless_enabled).await;
            drop(writer);
            drop(reader);
            task.await??;
            let old = f.device();
            let other = crate::daemon::hub::Hub::new(
                "controls-boot-b".into(),
                false,
                Duration::from_millis(1),
            )
            .shared();
            let mut a = new_projection(&f.hub, &Cell::new(0))?;
            let mut b = new_projection(&other, &Cell::new(0))?;
            assert_ne!(
                a.ids.id("device", DEVICE).unwrap(),
                b.ids.id("device", DEVICE).unwrap()
            );
            assert!(source(&b.ids, "device", &old).is_err());
            f.hub.borrow_mut().safe = true;
            assert!(advertised(f.hub.borrow().welcome()).is_empty());
            assert_eq!(
                f.call("network.set_wifi", json!({"enabled":false}))
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::Unavailable
            );
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn network_controls_owner_replacement_never_replays() -> crate::Result<()> {
    tokio::time::timeout(Duration::from_secs(20), tokio::task::LocalSet::new().run_until(async {
        let mut f = Fixture::new().await?;
        let old_device = f.device();
        f.backend.state.lock().unwrap().mode = 3;
        let hub = f.hub.clone();
        let call = tokio::task::spawn_local(dispatch(hub, "network.connect", json!({"device":old_device,"target":{"kind":"connection","id":f.snapshot().connections.order[0]}}), None));
        f.backend.started.notified().await;
        assert!(f.server.release_name(NM_SERVICE).await?);
        assert_eq!(call.await?.unwrap_err().code, ErrorCode::Unavailable);
        f.wait(|s| !s.health.available && s.health.stale).await;
        assert!(advertised(f.hub.borrow().welcome()).is_empty());
        assert!((&mut f.task).await?.is_err());
        let replacement = Backend::default();
        f.server = Fixture::server(&f.address, replacement.clone()).await?;
        let connection = zbus::connection::Builder::address(f.address.as_str())?.method_timeout(Duration::from_secs(2)).build().await?;
        let projection = new_projection(&f.hub, &Cell::new(1))?;
        f.task = tokio::task::spawn_local(observe_on_connection(f.hub.clone(), Rc::new(Cell::new(0)), projection, connection, ObserveHooks::default()));
        f.wait(|s| s.health.available).await;
        assert_ne!(old_device, f.device());
        assert_eq!(advertised(f.hub.borrow().welcome()).len(), 4);
        assert!(replacement.state.lock().unwrap().calls.is_empty());
        assert_eq!(f.call("network.disconnect", json!({"device":old_device})).await.unwrap_err().code, ErrorCode::StaleRevision);
        assert!(replacement.state.lock().unwrap().calls.is_empty());
        // Only this new explicit action reaches the replacement owner.
        assert_eq!(f.call("network.set_wifi", json!({"enabled":true})).await?, json!({"changed":true}));
        assert_eq!(replacement.state.lock().unwrap().calls, ["wifi"]);
        f.backend.release.notify_one();
        Ok::<(), crate::Error>(())
    })).await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn network_controls_link_loss_skips_queued_actions() -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            f.backend.state.lock().unwrap().mode = 3;
            let (mut writer, reader, task, _) = socket_client(f.hub.clone()).await?;
            for id in [1, 2] {
                framing::write(
                    &mut writer,
                    &Message::Call {
                        id: Id::Integer(id),
                        m: "network.set_wifi".into(),
                        p: json!({"enabled":true}),
                        timeout: None,
                    },
                )
                .await?;
            }
            f.backend.started.notified().await;
            drop(writer);
            drop(reader);
            task.await??; // Disconnect cancels the actual session's pending receivers.
            f.backend.release.notify_one();
            f.wait(|s| s.wireless_enabled).await;
            let (writer, reader, task, welcome) = socket_client(f.hub.clone()).await?;
            assert_eq!(advertised(welcome).len(), 4);
            // Reader and actor share one queue; a roundtrip fences the skipped call.
            assert_eq!(
                f.call("network.disconnect", json!({"device":f.device()}))
                    .await?,
                json!({"changed":false})
            );
            assert_eq!(f.backend.state.lock().unwrap().calls, ["wifi"]);
            drop(writer);
            drop(reader);
            task.await??;
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn network_controls_timeout_and_queue_bound() -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            f.backend.state.lock().unwrap().mode = 3;
            let (mut writer, mut reader, session, _) = socket_client(f.hub.clone()).await?;
            framing::write(
                &mut writer,
                &Message::Call {
                    id: Id::Integer(90),
                    m: "network.set_wifi".into(),
                    p: json!({"enabled":true}),
                    timeout: Some(500),
                },
            )
            .await?;
            f.backend.started.notified().await;
            let requests = f.hub.borrow().network.clone().unwrap();
            let mut receivers = vec![];
            for _ in 0..8 {
                let (reply, receive) = oneshot::channel();
                requests
                    .sender
                    .try_send(Action {
                        call: CoreCall::WifiPower(nacre_protocol::core_services::WifiPower {
                            enabled: false,
                        }),
                        deadline: tokio::time::Instant::now(),
                        reply,
                    })
                    .unwrap();
                receivers.push(receive);
            }
            assert_eq!(
                f.call("network.set_wifi", json!({"enabled":false}))
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::Busy
            );
            let Message::Ret {
                reply: nacre_protocol::Reply::Err { err },
                ..
            } = framing::read(&mut reader).await?.unwrap()
            else {
                panic!("expected unavailable");
            };
            assert_eq!(err.code, ErrorCode::Unavailable);
            assert!(!f.snapshot().wireless_enabled);
            f.backend.release.notify_one();
            f.wait(|s| s.wireless_enabled).await;
            for receive in receivers {
                assert!(receive.await?.is_err());
            }
            assert_eq!(f.backend.state.lock().unwrap().calls, ["wifi"]);
            drop(writer);
            drop(reader);
            session.await??;
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn network_controls_preflight_retires_vanished_ids() -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            let old = f.device();
            f.backend.state.lock().unwrap().gone = true;
            assert_eq!(
                f.call("network.disconnect", json!({"device":old}))
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::StaleRevision
            );
            // Return the identical path before the actor's debounced readback. A
            // disappearance observed by preflight must still retire the old ID.
            f.backend.state.lock().unwrap().gone = false;
            f.wait(|state| state.devices.order.first().is_some_and(|id| id != &old))
                .await;
            assert_eq!(
                f.call("network.disconnect", json!({"device":old}))
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::StaleRevision
            );
            assert_eq!(
                f.call("network.disconnect", json!({"device":f.device()}))
                    .await?,
                json!({"changed":false})
            );
            assert!(f.backend.state.lock().unwrap().calls.is_empty());
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn network_controls_transient_auth_failure_survives_readback() -> crate::Result<()> {
    tokio::time::timeout(Duration::from_secs(20), tokio::task::LocalSet::new().run_until(async {
        let f = Fixture::new().await?;
        f.call("network.set_wifi", json!({"enabled":true})).await?;
        f.wait(|s| s.wireless_enabled).await;
        let target = json!({"device":f.device(),"target":{"kind":"access_point","id":f.snapshot().access_points.order[0]}});
        f.backend.state.lock().unwrap().mode = 5;
        assert_eq!(f.call("network.connect", target.clone()).await?, json!({"changed":true}));
        // Current properties already say disconnected with reason NONE. Only
        // the real transition signals retain the failed authentication reason.
        for transition in [(120u32, 60u32, 7u32), (30, 120, 0)] {
            f.server.emit_signal(None::<&str>, DEVICE,
                "org.freedesktop.NetworkManager.Device", "StateChanged", &transition).await?;
        }
        let state = f.wait(|s| s.health.error.is_some()).await;
        assert!(state.health.available && !state.health.stale);
        assert!(state.devices.items.values().all(|device| !device.connected));
        assert_eq!(state.health.error.unwrap().message, "Network authentication required or cancelled");
        f.backend.state.lock().unwrap().mode = 0;
        f.call("network.connect", target).await?;
        f.wait(|s| s.health.error.is_none() && s.devices.items.values().any(|device| device.connected)).await;
        Ok::<(), crate::Error>(())
    })).await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn network_controls_failure_before_admission_reply_is_retained() -> crate::Result<()> {
    tokio::time::timeout(Duration::from_secs(20), tokio::task::LocalSet::new().run_until(async {
        let mut f = Fixture::new().await?;
        f.call("network.set_wifi", json!({"enabled":true})).await?;
        f.wait(|s| s.wireless_enabled).await;
        f.backend.started.notified().await; // Consume the earlier radio setter's witness.
        while f.events.try_recv().is_ok() {}
        let target = json!({"device":f.device(),"target":{"kind":"access_point","id":f.snapshot().access_points.order[0]}});
        f.backend.state.lock().unwrap().mode = 6;
        let call = tokio::task::spawn_local(dispatch(f.hub.clone(), "network.connect", target.clone(), None));
        f.backend.started.notified().await;
        for transition in [(120u32, 60u32, 7u32), (30, 120, 0)] {
            f.server.emit_signal(None::<&str>, DEVICE,
                "org.freedesktop.NetworkManager.Device", "StateChanged", &transition).await?;
            assert_eq!(f.events.recv().await, Some(crate::network_state::NetworkEvent::Changed));
        }
        // The real watcher has consumed the failure before the server accepts.
        f.backend.release.notify_one();
        assert_eq!(call.await??, json!({"changed":true}));
        f.wait(|s| s.health.error.as_ref().is_some_and(|error| error.message == "Network authentication required or cancelled")).await;
        f.backend.state.lock().unwrap().mode = 0;
        f.call("network.connect", target).await?;
        f.wait(|s| s.health.error.is_none() && s.devices.items.values().any(|device| device.connected)).await;
        Ok::<(), crate::Error>(())
    })).await??;
    Ok(())
}

fn id_named(state: &NetworkState, name: &str) -> EntityId {
    state
        .connections
        .items
        .iter()
        .find(|(_, saved)| saved.name == name)
        .map(|(id, _)| id.clone())
        .unwrap()
}
fn connect_params(f: &Fixture, target: Value, extra: Value) -> Value {
    let mut params = json!({"device":f.device(),"target":target});
    for (key, value) in extra.as_object().unwrap() {
        params[key] = value.clone();
    }
    params
}
fn created_profiles(f: &Fixture) -> usize {
    f.hub
        .borrow()
        .network
        .as_ref()
        .unwrap()
        .created
        .borrow()
        .len()
}

#[tokio::test(flavor = "current_thread")]
async fn network_secrets_join_saved_list_forget_and_needs_secret() -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            f.call("network.set_wifi", json!({"enabled":true})).await?;
            f.wait(|s| s.wireless_enabled).await;
            let state = f.snapshot();
            let saved = &state.connections.items[&state.connections.order[0]];
            assert_eq!(saved.ssid.as_deref(), Some("Fixture"));
            assert_eq!(saved.security.as_deref(), Some("wpa_psk"));
            assert_eq!(saved.autoconnect, Some(false));
            assert_eq!(saved.last_used, Some(1_700_000_000));
            let point = json!({"kind":"access_point","id":state.access_points.order[0]});
            // Validation fails before the backend sees anything.
            for params in [
                connect_params(&f, point.clone(), json!({"secret":"short"})),
                connect_params(&f, point.clone(), json!({"secret":""})),
                connect_params(&f, point.clone(), json!({"security":"sae"})),
                connect_params(
                    &f,
                    json!({"kind":"connection","id":state.connections.order[0]}),
                    json!({"secret":SECRET}),
                ),
                connect_params(
                    &f,
                    json!({"kind":"hidden_ssid","id":"x".repeat(33)}),
                    json!({}),
                ),
                connect_params(&f, json!({"kind":"hidden_ssid","id":""}), json!({})),
            ] {
                assert_eq!(
                    f.call("network.connect", params).await.unwrap_err().code,
                    ErrorCode::Validation
                );
            }
            assert!(f.backend.state.lock().unwrap().calls == ["wifi"]);
            // Join the secured access point with a secret: saved by NetworkManager, tracked until active.
            assert_eq!(
                f.call(
                    "network.connect",
                    connect_params(&f, point.clone(), json!({"secret":SECRET}))
                )
                .await?,
                json!({"changed":true})
            );
            {
                let backend = f.backend.state.lock().unwrap();
                assert_eq!(backend.added_sections, ["802-11-wireless-security"]);
                assert_eq!(backend.added_persist, "to-disk");
                assert_eq!(backend.added_key_mgmt.as_deref(), Some("wpa-psk"));
                assert_eq!(backend.added_psk.as_deref(), Some(SECRET));
            }
            let joined = f
                .wait(|s| {
                    s.connections.items.len() == 2
                        && s.devices.items.values().any(|d| d.connected)
                        && s.connections
                            .items
                            .values()
                            .any(|c| c.name == "Hidden" && c.active)
                })
                .await;
            assert_eq!(created_profiles(&f), 0, "a working profile is kept");
            assert!(f.backend.state.lock().unwrap().deleted.is_empty());
            f.call("network.disconnect", json!({"device":f.device()}))
                .await?;
            // Forget both saved profiles.
            let created = id_named(&joined, "Hidden");
            assert_eq!(
                f.call("network.forget", json!({"connection":created}))
                    .await?,
                json!({"changed":true})
            );
            f.wait(|s| s.connections.items.len() == 1).await;
            assert_eq!(f.backend.state.lock().unwrap().deleted, [CREATED]);
            // Stale, wrong-kind and malformed targets.
            for (params, code) in [
                (json!({"connection":created}), ErrorCode::StaleRevision),
                (json!({"connection":f.device()}), ErrorCode::StaleRevision),
                (
                    json!({"connection":"/org/freedesktop/x"}),
                    ErrorCode::Validation,
                ),
            ] {
                assert_eq!(
                    f.call("network.forget", params).await.unwrap_err().code,
                    code
                );
            }
            assert_eq!(f.backend.state.lock().unwrap().deleted, [CREATED]);
            // Secured access point without saved profile or secret: the UI must prompt.
            f.call(
                "network.forget",
                json!({"connection":id_named(&f.snapshot(), "Fixture")}),
            )
            .await?;
            f.wait(|s| s.connections.items.is_empty()).await;
            let calls = f.backend.state.lock().unwrap().calls.len();
            let error = f
                .call("network.connect", connect_params(&f, point, json!({})))
                .await
                .unwrap_err();
            assert_eq!(
                (
                    error.code,
                    error.message.as_str(),
                    error.retry,
                    error.dispatched
                ),
                (ErrorCode::Validation, SECRET_REQUIRED, true, Some(false))
            );
            assert_eq!(f.backend.state.lock().unwrap().calls.len(), calls);
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn network_secrets_hidden_ssid_variants() -> crate::Result<()> {
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            f.call("network.set_wifi", json!({"enabled":true})).await?;
            f.wait(|s| s.wireless_enabled).await;
            let hidden = json!({"kind":"hidden_ssid","id":"Hidden Net"});
            f.call(
                "network.connect",
                connect_params(&f, hidden.clone(), json!({"secret":SECRET})),
            )
            .await?;
            {
                let backend = f.backend.state.lock().unwrap();
                assert_eq!(
                    backend.added_sections,
                    ["802-11-wireless", "802-11-wireless-security", "connection"]
                );
                assert_eq!(
                    backend.added_ssid.as_deref(),
                    Some(b"Hidden Net".as_slice())
                );
                assert!(backend.added_hidden);
                assert_eq!(
                    (
                        backend.added_key_mgmt.as_deref(),
                        backend.added_psk.as_deref()
                    ),
                    (Some("wpa-psk"), Some(SECRET))
                );
                assert_eq!(backend.added_persist, "to-disk");
            }
            f.call(
                "network.connect",
                connect_params(
                    &f,
                    hidden.clone(),
                    json!({"secret":SECRET,"security":"sae"}),
                ),
            )
            .await?;
            assert_eq!(
                f.backend.state.lock().unwrap().added_key_mgmt.as_deref(),
                Some("sae")
            );
            f.call(
                "network.connect",
                connect_params(&f, hidden.clone(), json!({})),
            )
            .await?;
            {
                let backend = f.backend.state.lock().unwrap();
                assert_eq!(backend.added_sections, ["802-11-wireless", "connection"]);
                assert!(backend.added_hidden && backend.added_psk.is_none());
            }
            // A secure choice needs a secret; an open choice cannot carry one.
            let error = f
                .call(
                    "network.connect",
                    connect_params(&f, hidden.clone(), json!({"security":"wpa_psk"})),
                )
                .await
                .unwrap_err();
            assert_eq!(
                (error.code, error.message.as_str()),
                (ErrorCode::Validation, SECRET_REQUIRED)
            );
            let error = f
                .call(
                    "network.connect",
                    connect_params(
                        &f,
                        hidden.clone(),
                        json!({"secret":SECRET,"security":"open"}),
                    ),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::Validation);
            // Hidden joins need a Wi-Fi device.
            let wired = f.snapshot().devices.order[1].clone();
            let error = f
                .call(
                    "network.connect",
                    json!({"device":wired,"target":hidden,"secret":SECRET}),
                )
                .await
                .unwrap_err();
            assert_eq!(error.code, ErrorCode::StaleRevision);
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn network_secrets_wrong_password_deletes_new_profile_and_allows_retry() -> crate::Result<()>
{
    tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            f.call("network.set_wifi", json!({"enabled":true})).await?;
            f.wait(|s| s.wireless_enabled).await;
            let point = json!({"kind":"access_point","id":f.snapshot().access_points.order[0]});
            f.backend.state.lock().unwrap().mode = 5;
            f.call(
                "network.connect",
                connect_params(&f, point.clone(), json!({"secret":"definitely wrong"})),
            )
            .await?;
            f.wait(|s| s.connections.items.len() == 2).await;
            assert_eq!(created_profiles(&f), 1);
            for transition in [(120u32, 60u32, 7u32), (30, 120, 0)] {
                f.server
                    .emit_signal(
                        None::<&str>,
                        DEVICE,
                        "org.freedesktop.NetworkManager.Device",
                        "StateChanged",
                        &transition,
                    )
                    .await?;
            }
            // The retryable topic error is the re-prompt signal; the bad profile is gone.
            let failed = f.wait(|s| s.health.error.is_some()).await;
            let error = failed.health.error.unwrap();
            assert!(error.retry && error.message == "Network authentication required or cancelled");
            // Deletion happened before that publication; NetworkManager then signals the removal.
            assert_eq!(f.backend.state.lock().unwrap().deleted, [CREATED]);
            f.signal().await?;
            f.wait(|s| s.connections.items.len() == 1).await;
            assert_eq!(created_profiles(&f), 0);
            f.backend.state.lock().unwrap().mode = 0;
            f.call(
                "network.connect",
                connect_params(&f, point, json!({"secret":SECRET})),
            )
            .await?;
            f.wait(|s| {
                s.health.error.is_none()
                    && s.connections.items.len() == 2
                    && s.devices.items.values().any(|d| d.connected)
            })
            .await;
            assert_eq!(
                created_profiles(&f),
                0,
                "the working profile is kept and untracked"
            );
            Ok::<(), crate::Error>(())
        }),
    )
    .await??;
    Ok(())
}

#[derive(Clone, Default)]
struct LogSink(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for LogSink {
    fn write(&mut self, data: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(data);
        Ok(data.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Process-wide capture at TRACE, so zbus background threads are included too.
fn captured_logs() -> LogSink {
    static SINK: std::sync::OnceLock<LogSink> = std::sync::OnceLock::new();
    SINK.get_or_init(|| {
        let sink = LogSink::default();
        let writer = sink.clone();
        let _ = tracing::subscriber::set_global_default(
            tracing_subscriber::fmt()
                .with_max_level(tracing::Level::TRACE)
                .with_ansi(false)
                .with_writer(move || writer.clone())
                .finish(),
        );
        sink
    })
    .clone()
}

#[tokio::test(flavor = "current_thread")]
async fn network_secrets_never_reach_logs_errors_state_or_debug() -> crate::Result<()> {
    let sink = captured_logs();
    tracing::info!("secret log capture active");
    let observed = tokio::time::timeout(
        Duration::from_secs(20),
        tokio::task::LocalSet::new().run_until(async {
            let f = Fixture::new().await?;
            f.call("network.set_wifi", json!({"enabled":true})).await?;
            f.wait(|s| s.wireless_enabled).await;
            let point = json!({"kind":"access_point","id":f.snapshot().access_points.order[0]});
            let mut observed = String::new();
            let bad = [
                connect_params(&f, point.clone(), json!({"secret":SECRET})),
                connect_params(
                    &f,
                    json!({"kind":"connection","id":f.snapshot().connections.order[0]}),
                    json!({"secret":SECRET}),
                ),
                connect_params(
                    &f,
                    json!({"kind":"hidden_ssid","id":"Net"}),
                    json!({"secret":SECRET,"security":"open"}),
                ),
            ];
            let call: CoreCall =
                serde_json::from_value(json!({"m":"network.connect","p":bad[0]})).unwrap();
            observed += &format!("{call:?}");
            // Admission failures (denied, broken backend) and a wrong-password failure.
            for mode in [1, 2, 4, 5] {
                f.backend.state.lock().unwrap().mode = mode;
                for params in bad.iter().cloned() {
                    if let Err(error) = f.call("network.connect", params).await {
                        observed += &serde_json::to_string(&error)?;
                    }
                }
            }
            f.server
                .emit_signal(
                    None::<&str>,
                    DEVICE,
                    "org.freedesktop.NetworkManager.Device",
                    "StateChanged",
                    &(120u32, 60u32, 7u32),
                )
                .await?;
            let failed = f.wait(|s| s.health.error.is_some()).await;
            observed += &serde_json::to_string(&failed)?;
            observed += &serde_json::to_string(&f.hub.borrow().snapshot(CORE_TOPIC))?;
            observed += &format!("{:?}", f.hub.borrow().welcome());
            Ok::<String, crate::Error>(observed)
        }),
    )
    .await??;
    assert!(!observed.contains(SECRET) && !observed.contains("definitely"));
    let logs = String::from_utf8(sink.0.lock().unwrap().clone())?;
    assert!(
        logs.contains("secret log capture active"),
        "the capture must be live to be meaningful"
    );
    assert!(!logs.contains(SECRET), "secret leaked into daemon logs");
    Ok(())
}
