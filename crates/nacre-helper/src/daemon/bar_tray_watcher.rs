//! Small fallback StatusNotifierWatcher used only while no other watcher owns the name.
use crate::Result;
use futures_lite::StreamExt;
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};
use zbus::{
    fdo::{RequestNameFlags, RequestNameReply},
    message::Header,
    object_server::SignalEmitter,
    Connection,
};

const NAME: &str = "org.kde.StatusNotifierWatcher";
const PATH: &str = "/StatusNotifierWatcher";
const MAX_ITEMS: usize = 256;

#[derive(Clone, Debug)]
struct Registration {
    advertised: String,
    registered_name: String,
    owner: String,
    path: String,
}

#[derive(Default)]
struct State {
    items: BTreeMap<String, Registration>,
    hosts: BTreeMap<String, String>,
}

#[derive(Clone, Default)]
struct Watcher(Arc<Mutex<State>>);

impl Watcher {
    fn lock(&self) -> zbus::fdo::Result<std::sync::MutexGuard<'_, State>> {
        self.0
            .lock()
            .map_err(|_| zbus::fdo::Error::Failed("Watcher state is unavailable".into()))
    }

    async fn register_item(
        &self,
        service: &str,
        sender: &str,
        connection: &Connection,
        emitter: &SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let registration = resolve_registration(service, sender, connection).await?;
        let advertised = registration.advertised.clone();
        {
            let mut state = self.lock()?;
            if state
                .items
                .values()
                .any(|item| item.owner == registration.owner && item.path == registration.path)
            {
                return Ok(());
            }
            if state.items.len() >= MAX_ITEMS {
                return Err(zbus::fdo::Error::LimitsExceeded(
                    "Status notifier item limit reached".into(),
                ));
            }
            state.items.insert(advertised.clone(), registration);
        }
        Self::status_notifier_item_registered(emitter, &advertised).await?;
        self.registered_status_notifier_items_changed(emitter)
            .await?;
        Ok(())
    }

    async fn register_host(
        &self,
        service: &str,
        sender: &str,
        connection: &Connection,
        emitter: &SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        if service.is_empty() || service.len() > 255 {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Invalid host service name".into(),
            ));
        }
        let name = zbus::names::BusName::try_from(service)
            .map_err(|_| zbus::fdo::Error::InvalidArgs("Invalid host service name".into()))?;
        let owner = zbus::fdo::DBusProxy::new(connection)
            .await?
            .get_name_owner(name)
            .await
            .map_err(|_| {
                zbus::fdo::Error::AccessDenied("Host service is not owned by caller".into())
            })?;
        if owner.as_str() != sender {
            return Err(zbus::fdo::Error::AccessDenied(
                "Host service is not owned by caller".into(),
            ));
        }
        let newly_registered = {
            let mut state = self.lock()?;
            if !state.hosts.contains_key(sender) && state.hosts.len() >= MAX_ITEMS {
                return Err(zbus::fdo::Error::LimitsExceeded(
                    "Status notifier host limit reached".into(),
                ));
            }
            state
                .hosts
                .insert(sender.to_owned(), service.to_owned())
                .is_none()
        };
        if newly_registered {
            Self::status_notifier_host_registered(emitter).await?;
            self.is_status_notifier_host_registered_changed(emitter)
                .await?;
        }
        Ok(())
    }

    async fn owner_changed(
        &self,
        name: &str,
        old_owner: &str,
        new_owner: &str,
        emitter: &SignalEmitter<'_>,
    ) -> Result<bool> {
        if old_owner.is_empty() || old_owner == new_owner {
            return Ok(false);
        }
        let (removed, host_removed) = {
            let mut state = self.0.lock().map_err(|_| "Watcher state is unavailable")?;
            let ids: Vec<String> = state
                .items
                .iter()
                .filter(|(_, item)| {
                    item.owner == old_owner && (item.registered_name == name || item.owner == name)
                })
                .map(|(id, _)| id.clone())
                .collect();
            for id in &ids {
                state.items.remove(id);
            }
            let host_removed = if state
                .hosts
                .get(old_owner)
                .is_some_and(|registered| registered == name)
                || name == old_owner
            {
                state.hosts.remove(old_owner).is_some()
            } else {
                false
            };
            (ids, host_removed)
        };
        for id in &removed {
            Self::status_notifier_item_unregistered(emitter, id).await?;
        }
        if !removed.is_empty() {
            self.registered_status_notifier_items_changed(emitter)
                .await?;
        }
        if host_removed {
            self.is_status_notifier_host_registered_changed(emitter)
                .await?;
        }
        Ok(name == NAME && !old_owner.is_empty())
    }
}

#[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
impl Watcher {
    #[zbus(property)]
    fn protocol_version(&self) -> i32 {
        0
    }

    #[zbus(property)]
    fn is_status_notifier_host_registered(&self) -> bool {
        self.0.lock().is_ok_and(|state| !state.hosts.is_empty())
    }

    #[zbus(property)]
    fn registered_status_notifier_items(&self) -> Vec<String> {
        self.0
            .lock()
            .map(|state| state.items.keys().cloned().collect())
            .unwrap_or_default()
    }

    #[zbus(signal)]
    async fn status_notifier_item_registered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_item_unregistered(
        emitter: &SignalEmitter<'_>,
        service: &str,
    ) -> zbus::Result<()>;

    #[zbus(signal)]
    async fn status_notifier_host_registered(emitter: &SignalEmitter<'_>) -> zbus::Result<()>;

    async fn register_status_notifier_item(
        &self,
        service: String,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header
            .sender()
            .map(|name| name.to_string())
            .ok_or_else(|| {
                zbus::fdo::Error::AccessDenied("Missing D-Bus caller identity".into())
            })?;
        self.register_item(&service, &sender, connection, &emitter)
            .await
    }

    async fn register_status_notifier_host(
        &self,
        service: String,
        #[zbus(header)] header: Header<'_>,
        #[zbus(connection)] connection: &Connection,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> zbus::fdo::Result<()> {
        let sender = header
            .sender()
            .map(|name| name.to_string())
            .ok_or_else(|| {
                zbus::fdo::Error::AccessDenied("Missing D-Bus caller identity".into())
            })?;
        self.register_host(&service, &sender, connection, &emitter)
            .await
    }
}

/// Observe external ownership; only request the standard name when it is free.
pub async fn run() -> Result<()> {
    run_on_connection(Connection::session().await?).await
}

async fn run_on_connection(monitor: Connection) -> Result<()> {
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .arg(0, NAME)?
        .build();
    let mut changes = zbus::MessageStream::for_match_rule(rule, &monitor, Some(32)).await?;
    loop {
        if name_owner(&monitor).await.is_ok() {
            wait_for_release(&mut changes).await?;
            continue;
        }
        let watcher = Watcher::default();
        let candidate = monitor.clone();
        let owner_rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender("org.freedesktop.DBus")?
            .interface("org.freedesktop.DBus")?
            .member("NameOwnerChanged")?
            .build();
        let mut owners =
            zbus::MessageStream::for_match_rule(owner_rule, &candidate, Some(256)).await?;
        candidate.object_server().at(PATH, watcher.clone()).await?;
        match candidate
            .request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into())
            .await
        {
            Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner) => {
                serve_owned(&candidate, watcher, &mut owners).await?;
                candidate.object_server().remove::<Watcher, _>(PATH).await?;
            }
            Ok(RequestNameReply::Exists) | Err(zbus::Error::NameTaken) => {
                candidate.object_server().remove::<Watcher, _>(PATH).await?;
                wait_for_release(&mut changes).await?;
            }
            Ok(RequestNameReply::InQueue) => {
                return Err("Fallback tray watcher unexpectedly queued for ownership".into())
            }
            Err(error) => return Err(error.into()),
        }
    }
}

async fn serve_owned(
    connection: &Connection,
    watcher: Watcher,
    owners: &mut zbus::MessageStream,
) -> Result<()> {
    let emitter = SignalEmitter::new(connection, PATH)?;
    loop {
        let message = owners.next().await.ok_or("D-Bus owner stream closed")??;
        let (name, old_owner, new_owner): (String, String, String) =
            message.body().deserialize()?;
        if watcher
            .owner_changed(&name, &old_owner, &new_owner, &emitter)
            .await?
        {
            return Ok(());
        }
    }
}

async fn wait_for_release(stream: &mut zbus::MessageStream) -> Result<()> {
    loop {
        let message = stream.next().await.ok_or("D-Bus owner stream closed")??;
        let (name, old_owner, new_owner): (String, String, String) =
            message.body().deserialize()?;
        if name == NAME && !old_owner.is_empty() && old_owner != new_owner {
            return Ok(());
        }
    }
}

async fn name_owner(connection: &Connection) -> Result<String> {
    let name = zbus::names::BusName::try_from(NAME)?;
    Ok(zbus::fdo::DBusProxy::new(connection)
        .await?
        .get_name_owner(name)
        .await?
        .to_string())
}

async fn resolve_registration(
    service: &str,
    sender: &str,
    connection: &Connection,
) -> zbus::fdo::Result<Registration> {
    if service.is_empty() || service.len() > 511 || service.chars().any(char::is_control) {
        return Err(zbus::fdo::Error::InvalidArgs(
            "Invalid item registration".into(),
        ));
    }
    let (registered_name, owner, path, advertised) = if service.starts_with('/') {
        validate_path(service)?;
        (
            sender.to_owned(),
            sender.to_owned(),
            service.to_owned(),
            format!("{sender}{service}"),
        )
    } else {
        let (name, path) = service
            .split_once('/')
            .unwrap_or((service, "StatusNotifierItem"));
        if path.starts_with('/') {
            return Err(zbus::fdo::Error::InvalidArgs(
                "Invalid item object path".into(),
            ));
        }
        let path = format!("/{path}");
        let bus_name = zbus::names::BusName::try_from(name)
            .map_err(|_| zbus::fdo::Error::InvalidArgs("Invalid item service name".into()))?;
        let owner = zbus::fdo::DBusProxy::new(connection)
            .await?
            .get_name_owner(bus_name)
            .await
            .map_err(|_| {
                zbus::fdo::Error::AccessDenied("Item service is not owned by caller".into())
            })?
            .to_string();
        if owner != sender {
            return Err(zbus::fdo::Error::AccessDenied(
                "Item service is not owned by caller".into(),
            ));
        }
        validate_path(&path)?;
        let advertised = format!("{name}{path}");
        (name.to_owned(), owner, path, advertised)
    };
    Ok(Registration {
        advertised,
        registered_name,
        owner,
        path,
    })
}

fn validate_path(path: &str) -> zbus::fdo::Result<()> {
    let parsed = zbus::zvariant::ObjectPath::try_from(path)
        .map_err(|_| zbus::fdo::Error::InvalidArgs("Invalid item object path".into()))?;
    if parsed.as_str().len() > 255 {
        return Err(zbus::fdo::Error::InvalidArgs(
            "Item object path is too long".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        time::Duration,
    };
    const INTERFACE: &str = "org.kde.StatusNotifierWatcher";

    struct Bus(Child);
    impl Drop for Bus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    async fn private_bus() -> Result<(tempfile::TempDir, Bus, String, Connection, Connection)> {
        let dir = tempfile::tempdir()?;
        let mut child = Bus(Command::new("dbus-daemon")
            .args([
                "--session",
                "--nofork",
                "--print-address",
                &format!("--address=unix:path={}", dir.path().join("bus").display()),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?);
        let mut address = String::new();
        BufReader::new(child.0.stdout.take().ok_or("Missing bus address")?)
            .read_line(&mut address)?;
        let address = address.trim().to_owned();
        let existing = zbus::connection::Builder::address(address.as_str())?
            .name(NAME)?
            .build()
            .await?;
        let monitor = zbus::connection::Builder::address(address.as_str())?
            .build()
            .await?;
        Ok((dir, child, address, existing, monitor))
    }

    #[test]
    fn item_registration_checks_owner_path_and_limits() {
        assert!(validate_path("/StatusNotifierItem").is_ok());
        assert!(validate_path("relative/path").is_err());
        assert!(validate_path(&format!("/{}", "x/".repeat(130))).is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn fallback_claim_registration_and_owner_loss_are_bounded() -> Result<()> {
        let (_dir, _bus, address, external_owner, monitor) = private_bus().await?;
        // A pre-existing owner must retain the name; the fallback request is never queued.
        let candidate = zbus::connection::Builder::address(address.as_str())?
            .serve_at(PATH, Watcher::default())?
            .build()
            .await?;
        assert!(matches!(
            candidate
                .request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into())
                .await,
            Err(zbus::Error::NameTaken)
        ));
        drop(candidate);
        external_owner.close().await?;

        let watcher = Watcher::default();
        let service = zbus::connection::Builder::address(address.as_str())?
            .serve_at(PATH, watcher.clone())?
            .build()
            .await?;
        let mut owners = zbus::MessageStream::for_match_rule(
            zbus::MatchRule::builder()
                .msg_type(zbus::message::Type::Signal)
                .sender("org.freedesktop.DBus")?
                .interface("org.freedesktop.DBus")?
                .member("NameOwnerChanged")?
                .build(),
            &service,
            Some(256),
        )
        .await?;
        assert_eq!(
            service
                .request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into())
                .await?,
            RequestNameReply::PrimaryOwner
        );
        let client = zbus::connection::Builder::address(address.as_str())?
            .name("org.example.Notifier")?
            .build()
            .await?;
        let proxy = zbus::proxy::Builder::<zbus::Proxy<'_>>::new(&monitor)
            .destination(NAME)?
            .path(PATH)?
            .interface(INTERFACE)?
            .cache_properties(zbus::proxy::CacheProperties::No)
            .build()
            .await?;
        let client_proxy = zbus::Proxy::new(&client, NAME, PATH, INTERFACE).await?;
        client_proxy
            .call::<_, _, ()>("RegisterStatusNotifierItem", &"/StatusNotifierItem")
            .await?;
        let registered: Vec<String> = proxy.get_property("RegisteredStatusNotifierItems").await?;
        let client_owner = client
            .unique_name()
            .ok_or("Missing client unique name")?
            .to_string();
        assert_eq!(
            registered,
            vec![format!("{client_owner}/StatusNotifierItem")]
        );
        assert!(
            !proxy
                .get_property::<bool>("IsStatusNotifierHostRegistered")
                .await?
        );
        client_proxy
            .call::<_, _, ()>("RegisterStatusNotifierHost", &"org.example.Notifier")
            .await?;
        assert!(
            proxy
                .get_property::<bool>("IsStatusNotifierHostRegistered")
                .await?
        );
        let attacker = zbus::connection::Builder::address(address.as_str())?
            .build()
            .await?;
        let attacker_proxy = zbus::Proxy::new(&attacker, NAME, PATH, INTERFACE).await?;
        assert!(attacker_proxy
            .call::<_, _, ()>(
                "RegisterStatusNotifierItem",
                &"org.example.Notifier/StatusNotifierItem"
            )
            .await
            .is_err());

        drop(client_proxy);
        client.close().await?;
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                let event = owners.next().await.ok_or("Missing owner event")??;
                let (name, old, new): (String, String, String) = event.body().deserialize()?;
                watcher
                    .owner_changed(&name, &old, &new, &SignalEmitter::new(&service, PATH)?)
                    .await?;
                let registered: Vec<String> =
                    proxy.get_property("RegisteredStatusNotifierItems").await?;
                if registered.is_empty() {
                    break;
                }
            }
            Ok::<(), crate::Error>(())
        })
        .await??;
        assert!(
            !proxy
                .get_property::<bool>("IsStatusNotifierHostRegistered")
                .await?
        );
        Ok(())
    }
}
