//! Read-only NetworkManager properties; no nmcli process or active Wi-Fi scan.
use crate::Result;
use serde_json::{json, Value};
use std::{collections::HashSet, sync::OnceLock, time::Duration};
use zbus::{
    blocking::{Connection, Proxy},
    zvariant::{OwnedObjectPath, OwnedValue},
};
const SERVICE: &str = "org.freedesktop.NetworkManager";
const MANAGER: &str = "/org/freedesktop/NetworkManager";
const UNAVAILABLE: &str = "NetworkManager is unavailable or did not respond.";
#[derive(Clone, Copy)]
pub enum IdentityKind {
    Ssids,
    Profiles,
}

#[cfg(test)]
mod identity_tests {
    use super::*;
    use std::{
        collections::HashMap,
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{
            atomic::{AtomicUsize, Ordering},
            Arc,
        },
    };
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
            vec![OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Devices/1").unwrap()]
        }
    }
    struct Device;
    #[zbus::interface(name = "org.freedesktop.NetworkManager.Device")]
    impl Device {
        #[zbus(property)]
        fn device_type(&self) -> u32 {
            2
        }
    }
    struct Wireless(Arc<AtomicUsize>);
    #[zbus::interface(name = "org.freedesktop.NetworkManager.Device.Wireless")]
    impl Wireless {
        #[zbus(property)]
        fn access_points(&self) -> Vec<OwnedObjectPath> {
            vec![
                OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/AccessPoint/1").unwrap(),
            ]
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
            "Fixture:Network\\Café".as_bytes().to_vec()
        }
    }
    struct Settings;
    #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings")]
    impl Settings {
        fn list_connections(&self) -> Vec<OwnedObjectPath> {
            vec![OwnedObjectPath::try_from("/org/freedesktop/NetworkManager/Settings/1").unwrap()]
        }
    }
    struct Profile {
        secrets: Arc<AtomicUsize>,
        huge: bool,
    }
    #[zbus::interface(name = "org.freedesktop.NetworkManager.Settings.Connection")]
    impl Profile {
        fn get_settings(&self) -> HashMap<String, HashMap<String, OwnedValue>> {
            HashMap::from([(
                "connection".into(),
                HashMap::from([(
                    "id".into(),
                    OwnedValue::from(zbus::zvariant::Str::from(if self.huge {
                        "private".repeat(400_000)
                    } else {
                        "Fixture profile".into()
                    })),
                )]),
            )])
        }
        fn get_secrets(&self, _setting: &str) -> HashMap<String, HashMap<String, OwnedValue>> {
            self.secrets.fetch_add(1, Ordering::SeqCst);
            HashMap::new()
        }
    }
    #[test]
    fn private_identity_queries_never_scan_or_read_secrets_and_bound_reply_size() -> Result<()> {
        for huge in [false, true] {
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
            BufReader::new(bus.0.stdout.take().ok_or("Missing private bus")?)
                .read_line(&mut address)?;
            let scans = Arc::new(AtomicUsize::new(0));
            let secrets = Arc::new(AtomicUsize::new(0));
            let _server = async_io::block_on(
                zbus::connection::Builder::address(address.trim())?
                    .name(SERVICE)?
                    .serve_at(MANAGER, Manager)?
                    .serve_at("/org/freedesktop/NetworkManager/Devices/1", Device)?
                    .serve_at(
                        "/org/freedesktop/NetworkManager/Devices/1",
                        Wireless(scans.clone()),
                    )?
                    .serve_at("/org/freedesktop/NetworkManager/AccessPoint/1", AccessPoint)?
                    .serve_at("/org/freedesktop/NetworkManager/Settings", Settings)?
                    .serve_at(
                        "/org/freedesktop/NetworkManager/Settings/1",
                        Profile {
                            secrets: secrets.clone(),
                            huge,
                        },
                    )?
                    .build(),
            )?;
            let ssids = redaction_names_at(IdentityKind::Ssids, Some(address.trim()))?;
            assert_eq!(ssids, vec!["Fixture:Network\\Café"]);
            let profiles = redaction_names_at(IdentityKind::Profiles, Some(address.trim()));
            if huge {
                assert_eq!(
                    profiles
                        .expect_err("Oversized settings accepted")
                        .to_string(),
                    "NetworkManager returned too much data."
                );
            } else {
                assert_eq!(profiles?, vec!["Fixture profile"]);
                let names = [ssids, vec!["Fixture profile".into()]].concat();
                assert_eq!(
                    crate::diagnostics::redact(
                        &json!("Fixture:Network\\Café Fixture profile"),
                        &names
                    )?,
                    json!("[identity] [identity]")
                );
            }
            assert_eq!(scans.load(Ordering::SeqCst), 0);
            assert_eq!(secrets.load(Ordering::SeqCst), 0);
        }
        Ok(())
    }
}
pub fn redaction_names(kind: IdentityKind) -> Result<Vec<String>> {
    redaction_names_at(kind, None)
}
fn redaction_names_at(kind: IdentityKind, address: Option<&str>) -> Result<Vec<String>> {
    async_io::block_on(futures_lite::future::or(
        async {
            let builder = match address {
                Some(address) => zbus::connection::Builder::address(address)?,
                None => zbus::connection::Builder::system()?,
            };
            let connection = builder
                .method_timeout(Duration::from_secs(3))
                .build()
                .await
                .map_err(|_| UNAVAILABLE)?;
            identities(&connection, kind).await
        },
        async {
            async_io::Timer::after(Duration::from_secs(3)).await;
            Err(UNAVAILABLE.into())
        },
    ))
}
async fn identity_property<T: TryFrom<OwnedValue>>(
    connection: &zbus::Connection,
    path: &str,
    interface: &str,
    name: &str,
) -> Result<T> {
    let reply = connection
        .call_method(
            Some(SERVICE),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(interface, name),
        )
        .await
        .map_err(|_| UNAVAILABLE)?;
    let value: OwnedValue = bounded(reply)?
        .body()
        .deserialize()
        .map_err(|_| UNAVAILABLE)?;
    T::try_from(value).map_err(|_| UNAVAILABLE.into())
}
async fn identities(connection: &zbus::Connection, kind: IdentityKind) -> Result<Vec<String>> {
    use std::collections::{BTreeSet, HashMap};
    let mut names = BTreeSet::new();
    match kind {
        IdentityKind::Ssids => {
            let reply = connection
                .call_method(Some(SERVICE), MANAGER, Some(SERVICE), "GetDevices", &())
                .await
                .map_err(|_| UNAVAILABLE)?;
            let devices: Vec<OwnedObjectPath> = bounded(reply)?
                .body()
                .deserialize()
                .map_err(|_| UNAVAILABLE)?;
            if devices.len() > 4096 {
                return Err("NetworkManager returned too much data.".into());
            }
            for path in devices {
                let kind: u32 = identity_property(
                    connection,
                    path.as_str(),
                    "org.freedesktop.NetworkManager.Device",
                    "DeviceType",
                )
                .await?;
                if kind != 2 {
                    continue;
                }
                let points: Vec<OwnedObjectPath> = identity_property(
                    connection,
                    path.as_str(),
                    "org.freedesktop.NetworkManager.Device.Wireless",
                    "AccessPoints",
                )
                .await?;
                if points.len() > 4096 {
                    return Err("NetworkManager returned too much data.".into());
                }
                for point in points {
                    let ssid: Vec<u8> = identity_property(
                        connection,
                        point.as_str(),
                        "org.freedesktop.NetworkManager.AccessPoint",
                        "Ssid",
                    )
                    .await?;
                    if ssid.len() > 32 {
                        return Err(UNAVAILABLE.into());
                    }
                    let ssid = String::from_utf8_lossy(&ssid).into_owned();
                    if !ssid.is_empty() {
                        names.insert(ssid);
                    }
                }
            }
        }
        IdentityKind::Profiles => {
            let reply = connection
                .call_method(
                    Some(SERVICE),
                    "/org/freedesktop/NetworkManager/Settings",
                    Some("org.freedesktop.NetworkManager.Settings"),
                    "ListConnections",
                    &(),
                )
                .await
                .map_err(|_| UNAVAILABLE)?;
            let paths: Vec<OwnedObjectPath> = bounded(reply)?
                .body()
                .deserialize()
                .map_err(|_| UNAVAILABLE)?;
            if paths.len() > 4096 {
                return Err("NetworkManager returned too much data.".into());
            }
            for path in paths {
                // GetSettings excludes secrets; GetSecrets is never requested.
                let reply = connection
                    .call_method(
                        Some(SERVICE),
                        path.as_str(),
                        Some("org.freedesktop.NetworkManager.Settings.Connection"),
                        "GetSettings",
                        &(),
                    )
                    .await
                    .map_err(|_| UNAVAILABLE)?;
                let settings: HashMap<String, HashMap<String, OwnedValue>> = bounded(reply)?
                    .body()
                    .deserialize()
                    .map_err(|_| UNAVAILABLE)?;
                if let Some(name) = settings.get("connection").and_then(|s| s.get("id")) {
                    let name = String::try_from(name.try_clone()?).map_err(|_| UNAVAILABLE)?;
                    if !name.is_empty() {
                        names.insert(name);
                    }
                }
            }
        }
    }
    Ok(names.into_iter().collect())
}
fn bounded(message: zbus::Message) -> Result<zbus::Message> {
    if message.body().len() > 2 * 1024 * 1024 {
        return Err("NetworkManager returned too much data.".into());
    }
    Ok(message)
}
fn property<T: TryFrom<OwnedValue>>(proxy: &Proxy<'_>, name: &str) -> Result<T> {
    let reply = proxy
        .connection()
        .call_method(
            Some(SERVICE),
            proxy.path(),
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &(proxy.interface().as_str(), name),
        )
        .map_err(|_| UNAVAILABLE)?;
    let raw: OwnedValue = bounded(reply)?
        .body()
        .deserialize()
        .map_err(|_| UNAVAILABLE)?;
    T::try_from(raw).map_err(|_| UNAVAILABLE.into())
}
fn proxy<'a>(connection: &'a Connection, path: &str, interface: &str) -> Result<Proxy<'a>> {
    Ok(zbus::blocking::proxy::Builder::new(connection)
        .destination(SERVICE)?
        .path(path.to_owned())?
        .interface(interface.to_owned())?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()?)
}
fn interface_valid(name: &str) -> bool {
    static VALID: OnceLock<regex::Regex> = OnceLock::new();
    VALID
        .get_or_init(|| regex::Regex::new(r"\A[A-Za-z0-9_][A-Za-z0-9_.-]{0,63}\z").unwrap())
        .is_match(name)
}
fn bssid_valid(name: &str) -> bool {
    static VALID: OnceLock<regex::Regex> = OnceLock::new();
    VALID
        .get_or_init(|| regex::Regex::new(r"\A(?:[0-9A-Fa-f]{2}:){5}[0-9A-Fa-f]{2}\z").unwrap())
        .is_match(name)
}
pub fn snapshot() -> Result<Value> {
    let connection = zbus::blocking::connection::Builder::system()?
        .method_timeout(Duration::from_secs(8))
        .build()
        .map_err(|_| UNAVAILABLE)?;
    let manager = proxy(&connection, MANAGER, SERVICE).map_err(|_| UNAVAILABLE)?;
    let raw: OwnedValue = property(&manager, "WirelessEnabled")?;
    let enabled =
        bool::try_from(raw).map_err(|_| "NetworkManager returned an unknown Wi-Fi radio state.")?;
    let devices: Vec<OwnedObjectPath> = bounded(
        manager
            .call_method("GetDevices", &())
            .map_err(|_| UNAVAILABLE)?,
    )?
    .body()
    .deserialize()
    .map_err(|_| UNAVAILABLE)?;
    let mut available = false;
    let mut connected = String::new();
    let mut networks = Vec::new();
    let mut seen = HashSet::new();
    for path in devices {
        let device = proxy(
            &connection,
            path.as_str(),
            "org.freedesktop.NetworkManager.Device",
        )
        .map_err(|_| UNAVAILABLE)?;
        let kind: u32 = property(&device, "DeviceType")?;
        if kind != 2 {
            continue;
        }
        available = true;
        let interface: String = property(&device, "Interface")?;
        let state: u32 = property(&device, "State")?;
        if state == 100 && connected.is_empty() && interface_valid(&interface) {
            connected = interface.clone();
        }
        let wifi = proxy(
            &connection,
            path.as_str(),
            "org.freedesktop.NetworkManager.Device.Wireless",
        )
        .map_err(|_| UNAVAILABLE)?;
        let active: OwnedObjectPath = property(&wifi, "ActiveAccessPoint")?;
        let points: Vec<OwnedObjectPath> = property(&wifi, "AccessPoints")?;
        for point in points.into_iter().take(4096 - networks.len().min(4096)) {
            let access = proxy(
                &connection,
                point.as_str(),
                "org.freedesktop.NetworkManager.AccessPoint",
            )
            .map_err(|_| UNAVAILABLE)?;
            let ssid: Vec<u8> = property(&access, "Ssid")?;
            let bssid: String = property(&access, "HwAddress")?;
            let strength: u8 = property(&access, "Strength")?;
            let frequency: u32 = property(&access, "Frequency")?;
            let Ok(ssid) = String::from_utf8(ssid) else {
                continue;
            };
            if ssid.len() > 32
                || ssid.chars().any(|v| (v as u32) < 32)
                || !bssid_valid(&bssid)
                || strength > 100
                || !(1..=100000).contains(&frequency)
                || !interface_valid(&interface)
            {
                continue;
            }
            let bssid = bssid.to_uppercase();
            if !seen.insert((bssid.clone(), interface.clone())) {
                continue;
            }
            networks.push(json!({"ssid":ssid,"bssid":bssid,"strength":strength,"frequency":frequency,"active":point==active,"interface":interface}));
        }
    }
    Ok(
        json!({"wifiEnabled":available&&enabled,"wifiAvailable":available,"wifiInterface":connected,"networks":networks}),
    )
}
pub fn execute(arguments: &[String], input: Option<Value>) -> Result<Option<Value>> {
    if !arguments.is_empty() || input.is_some() {
        return Err("Unsupported helper operation".into());
    }
    Ok(Some(snapshot()?))
}

pub fn watch() -> Result<i32> {
    use std::io::Write;
    async_io::block_on(async {
        let connection = zbus::Connection::system().await?;
        watch_events(&connection, || {
            println!("changed");
            std::io::stdout().flush()?;
            Ok(true)
        })
        .await
    })
}
pub(crate) async fn watch_events(
    connection: &zbus::Connection,
    mut emit: impl FnMut() -> Result<bool>,
) -> Result<i32> {
    watch_events_tagged(connection, |_| emit()).await
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum NetworkEvent {
    Initial,
    Changed,
    OwnerChanged,
}

pub(crate) async fn watch_events_tagged(
    connection: &zbus::Connection,
    mut emit: impl FnMut(NetworkEvent) -> Result<bool>,
) -> Result<i32> {
    watch_events_with_message(connection, |event, _| emit(event)).await
}

// The core reader may retain an authoritative transition reason even when the
// following property read has already advanced past that transient state.
pub(crate) async fn watch_events_with_message(
    connection: &zbus::Connection,
    mut emit: impl FnMut(NetworkEvent, Option<&zbus::Message>) -> Result<bool>,
) -> Result<i32> {
    use futures_lite::StreamExt;
    let events = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender(SERVICE)?
        .path_namespace(MANAGER)?
        .build();
    let owner = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .arg(0, SERVICE)?
        .build();
    let mut changes = zbus::MessageStream::for_match_rule(events, connection, Some(256)).await?;
    let mut owners = zbus::MessageStream::for_match_rule(owner, connection, Some(16)).await?;
    if !emit(NetworkEvent::Initial, None)? {
        return Ok(0);
    }
    loop {
        tokio::select! {
            event = changes.next() => {
                let message = event.ok_or("NetworkManager event stream closed")??;
                if !emit(NetworkEvent::Changed, Some(&message))? {
                    return Ok(0);
                }
            }
            event = owners.next() => {
                let message = event.ok_or("NetworkManager owner stream closed")??;
                let (_name, old_owner, new_owner): (String, String, String) = message
                    .body()
                    .deserialize()
                    .map_err(|_| "Malformed NetworkManager owner-change signal")?;
                if old_owner != new_owner && !emit(NetworkEvent::OwnerChanged, Some(&message))? {
                    return Ok(0);
                }
            }
        }
    }
}

#[cfg(test)]
mod watch_tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
    };
    struct Bus(Child);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn private_monitor_reports_device_events_without_a_polling_command() -> Result<()> {
        let home = tempfile::tempdir()?;
        let mut bus = Bus(Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address",
                &format!("--address=unix:path={}", home.path().join("bus").display()),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?);
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().ok_or("Missing private bus address")?)
            .read_line(&mut address)?;
        async_io::block_on(async {
            let server = zbus::connection::Builder::address(address.trim())?
                .build()
                .await?;
            server.request_name(SERVICE).await?;
            let client = zbus::connection::Builder::address(address.trim())?
                .build()
                .await?;
            let mut count = 0;
            let ready = std::cell::Cell::new(false);
            let wake = std::cell::RefCell::new(None::<std::task::Waker>);
            let wait = watch_events(&client, || {
                count += 1;
                ready.set(true);
                if let Some(waker) = wake.borrow_mut().take() {
                    waker.wake();
                }
                Ok(count < 2)
            });
            let publish = async {
                // Initial callback runs only after both signal rules subscribe.
                futures_lite::future::poll_fn(|context| {
                    if ready.get() {
                        std::task::Poll::Ready(())
                    } else {
                        *wake.borrow_mut() = Some(context.waker().clone());
                        std::task::Poll::Pending
                    }
                })
                .await;
                server
                    .emit_signal(
                        None::<&str>,
                        MANAGER,
                        SERVICE,
                        "DeviceAdded",
                        &(OwnedObjectPath::try_from(
                            "/org/freedesktop/NetworkManager/Devices/1",
                        )?,),
                    )
                    .await?;
                Ok::<(), crate::Error>(())
            };
            let (result, published) = futures_lite::future::or(
                async { Ok::<_, crate::Error>(futures_lite::future::zip(wait, publish).await) },
                async {
                    async_io::Timer::after(Duration::from_secs(5)).await;
                    Err("Private network monitor test timed out".into())
                },
            )
            .await?;
            published?;
            assert_eq!(result?, 0);
            assert_eq!(count, 2);
            Ok::<(), crate::Error>(())
        })
    }

    #[test]
    fn tagged_monitor_reports_network_manager_owner_replacement() -> Result<()> {
        let home = tempfile::tempdir()?;
        let mut bus = Bus(Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address",
                &format!("--address=unix:path={}", home.path().join("bus").display()),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?);
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().ok_or("Missing private bus address")?)
            .read_line(&mut address)?;
        async_io::block_on(async {
            let server = zbus::connection::Builder::address(address.trim())?
                .name(SERVICE)?
                .build()
                .await?;
            let client = zbus::connection::Builder::address(address.trim())?
                .build()
                .await?;
            let observed = std::rc::Rc::new(std::cell::Cell::new(None));
            let wake = std::rc::Rc::new(std::cell::RefCell::new(None::<std::task::Waker>));
            let event_state = observed.clone();
            let event_wake = wake.clone();
            let watch = watch_events_tagged(&client, move |event| {
                event_state.set(Some(event));
                if let Some(waker) = event_wake.borrow_mut().take() {
                    waker.wake();
                }
                Ok(event != NetworkEvent::OwnerChanged)
            });
            let replace = async move {
                futures_lite::future::poll_fn(|context| {
                    if observed.get() == Some(NetworkEvent::Initial) {
                        std::task::Poll::Ready(())
                    } else {
                        *wake.borrow_mut() = Some(context.waker().clone());
                        std::task::Poll::Pending
                    }
                })
                .await;
                observed.set(None);
                drop(server);
                let replacement = zbus::connection::Builder::address(address.trim())?
                    .name(SERVICE)?
                    .build()
                    .await?;
                futures_lite::future::poll_fn(|context| {
                    if observed.get() == Some(NetworkEvent::OwnerChanged) {
                        std::task::Poll::Ready(())
                    } else {
                        *wake.borrow_mut() = Some(context.waker().clone());
                        std::task::Poll::Pending
                    }
                })
                .await;
                drop(replacement);
                Ok::<(), crate::Error>(())
            };
            let (watched, replaced) = futures_lite::future::zip(watch, replace).await;
            replaced?;
            assert_eq!(watched?, 0);
            Ok::<(), crate::Error>(())
        })?;
        drop(bus);
        Ok(())
    }
}
