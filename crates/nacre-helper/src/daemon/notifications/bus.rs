//! Freedesktop ingress. Nacre owns admission and policy; Bead supplies Center.
use super::{Command, Ingress, Response};
use nacre_protocol::{ErrorCode, Failure};
use std::collections::HashMap;
use zbus::{message::Header, zvariant::OwnedValue};

pub const NAME: &str = "org.freedesktop.Notifications";
pub const PATH: &str = "/org/freedesktop/Notifications";
pub type Pixbuf = (i32, i32, i32, bool, i32, i32, Vec<u8>);

pub enum Image {
    Path(String),
    Data(Pixbuf),
}

pub struct Notify {
    pub sender: String,
    pub app_name: String,
    pub replaces: u32,
    pub icon: String,
    pub summary: String,
    pub body: String,
    pub actions: Vec<(String, String)>,
    pub urgency: u8,
    pub resident: bool,
    pub transient: bool,
    pub desktop_entry: Option<String>,
    pub image: Option<Image>,
    pub timeout: i32,
}

fn rejected() -> zbus::fdo::Error {
    zbus::fdo::Error::InvalidArgs("Invalid notification request".into())
}

fn native_error(error: Failure) -> zbus::fdo::Error {
    // Never put arguments, source paths or upstream errors on the bus.
    match error.code {
        ErrorCode::Validation | ErrorCode::StaleRevision => rejected(),
        ErrorCode::TooLarge | ErrorCode::Busy => {
            zbus::fdo::Error::LimitsExceeded("Notification capacity reached".into())
        }
        ErrorCode::PermissionDenied => {
            zbus::fdo::Error::AccessDenied("Notification request denied".into())
        }
        _ => zbus::fdo::Error::Failed("Notification service unavailable".into()),
    }
}

fn sender(header: &Header<'_>) -> zbus::fdo::Result<String> {
    header
        .sender()
        .map(ToString::to_string)
        .ok_or_else(rejected)
}

#[derive(Clone)]
struct Native(Ingress);

#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Native {
    fn get_capabilities(&self) -> Vec<&str> {
        let mut result = vec!["actions", "body"];
        if self.0.storage_writable() {
            result.push("persistence");
        }
        result
    }

    fn get_server_information(&self) -> (&str, &str, &str, &str) {
        ("Nacre", "Nacre", env!("CARGO_PKG_VERSION"), "1.2")
    }

    #[allow(clippy::too_many_arguments)]
    async fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        app_icon: String,
        summary: String,
        body: String,
        actions: Vec<String>,
        mut hints: HashMap<String, OwnedValue>,
        expire_timeout: i32,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<u32> {
        if !actions.len().is_multiple_of(2) || actions.len() > 32 {
            return Err(rejected());
        }
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v).ok())
            .unwrap_or(1)
            .min(2);
        let resident = hints
            .get("resident")
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false);
        let transient = hints
            .get("transient")
            .and_then(|v| bool::try_from(v).ok())
            .unwrap_or(false);
        let desktop_entry = hints
            .remove("desktop-entry")
            .map(String::try_from)
            .transpose()
            .map_err(|_| rejected())?;
        let image = if let Some(data) = hints
            .remove("image-data")
            .or_else(|| hints.remove("image_data"))
        {
            Some(Image::Data(Pixbuf::try_from(data).map_err(|_| rejected())?))
        } else {
            hints
                .remove("image-path")
                .or_else(|| hints.remove("image_path"))
                .map(|v| String::try_from(v).map(Image::Path))
                .transpose()
                .map_err(|_| rejected())?
        };
        let actions = actions
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| (p[0].clone(), p[1].clone()))
            .collect::<Vec<_>>();
        let image_bytes = match &image {
            Some(Image::Data(data)) => data.6.len(),
            Some(Image::Path(path)) => path.len(),
            None => 0,
        };
        let bytes = app_name
            .len()
            .saturating_add(app_icon.len())
            .saturating_add(summary.len())
            .saturating_add(body.len())
            .saturating_add(image_bytes)
            .saturating_add(desktop_entry.as_ref().map_or(0, String::len))
            .saturating_add(
                actions
                    .iter()
                    .map(|(k, l)| k.len().saturating_add(l.len()))
                    .sum::<usize>(),
            );
        if [&app_name, &app_icon, &summary, &body]
            .iter()
            .any(|s| s.len() > 8192 || s.contains('\0'))
            || desktop_entry
                .as_ref()
                .is_some_and(|s| s.len() > 1024 || s.contains('\0'))
            || actions.iter().any(|(k, l)| {
                k.is_empty()
                    || k.len() > 256
                    || l.len() > 1024
                    || k.contains('\0')
                    || l.contains('\0')
            })
            || expire_timeout < -1
            || image_bytes > 4 * 1024 * 1024
        {
            return Err(rejected());
        }
        let request = Notify {
            sender: sender(&header)?,
            app_name,
            replaces: replaces_id,
            icon: app_icon,
            summary,
            body,
            actions,
            urgency,
            resident,
            transient,
            desktop_entry,
            image,
            timeout: expire_timeout,
        };
        match self
            .0
            .call(Command::Notify(request), bytes, None)
            .await
            .map_err(native_error)?
        {
            Response::NativeId(id) => Ok(id),
            _ => Err(zbus::fdo::Error::Failed(
                "Notification processing failed".into(),
            )),
        }
    }

    async fn close_notification(&self, id: u32) -> zbus::fdo::Result<()> {
        self.0
            .call(Command::Close(id), 16, None)
            .await
            .map_err(native_error)?;
        Ok(())
    }
}

#[derive(Clone)]
struct Inhibition(Ingress);

#[zbus::interface(name = "org.nacre.Bead.Notifications")]
impl Inhibition {
    async fn inhibit(
        &self,
        reason: String,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<u32> {
        if reason.len() > 1024 || reason.contains('\0') {
            return Err(rejected());
        }
        let bytes = reason.len();
        match self
            .0
            .call(Command::Inhibit(sender(&header)?, reason), bytes, None)
            .await
            .map_err(native_error)?
        {
            Response::NativeId(cookie) => Ok(cookie),
            _ => Err(zbus::fdo::Error::Failed(
                "Notification processing failed".into(),
            )),
        }
    }

    async fn uninhibit(
        &self,
        cookie: u32,
        #[zbus(header)] header: Header<'_>,
    ) -> zbus::fdo::Result<()> {
        self.0
            .call(Command::Uninhibit(sender(&header)?, cookie), 16, None)
            .await
            .map_err(native_error)?;
        Ok(())
    }

    #[zbus(property)]
    fn inhibited(&self) -> bool {
        self.0.inhibited()
    }
}

pub struct Owned {
    pub connection: zbus::Connection,
    native: bool,
    inhibition: bool,
    owned: bool,
}

impl Owned {
    pub async fn start(connection: zbus::Connection, ingress: Ingress) -> Result<Self, Failure> {
        let mut service = Self {
            connection,
            native: false,
            inhibition: false,
            owned: false,
        };
        let result = async {
            if !service
                .connection
                .object_server()
                .at(PATH, Native(ingress.clone()))
                .await
                .map_err(|_| unavailable())?
            {
                return Err(unavailable());
            }
            service.native = true;
            if !service
                .connection
                .object_server()
                .at(PATH, Inhibition(ingress))
                .await
                .map_err(|_| unavailable())?
            {
                return Err(unavailable());
            }
            service.inhibition = true;
            let reply = service
                .connection
                .request_name_with_flags(NAME, zbus::fdo::RequestNameFlags::DoNotQueue.into())
                .await
                .map_err(|_| unavailable())?;
            if reply != zbus::fdo::RequestNameReply::PrimaryOwner {
                return Err(unavailable());
            }
            service.owned = true;
            Ok(())
        }
        .await;
        if let Err(error) = result {
            service.stop().await;
            return Err(error);
        }
        Ok(service)
    }

    pub async fn stop(&mut self) {
        if self.native {
            let _ = self
                .connection
                .object_server()
                .remove::<Native, _>(PATH)
                .await;
            self.native = false;
        }
        if self.inhibition {
            let _ = self
                .connection
                .object_server()
                .remove::<Inhibition, _>(PATH)
                .await;
            self.inhibition = false;
        }
        if self.owned {
            let _ = self.connection.release_name(NAME).await;
            self.owned = false;
        }
    }
}

pub fn unavailable() -> Failure {
    Failure::new(
        ErrorCode::Unavailable,
        "Notification ownership is unavailable",
        true,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::StreamExt;
    use std::{process::Command as ProcessCommand, time::Duration};
    use tokio::time::timeout;
    use zbus::fdo::{DBusProxy, RequestNameFlags, RequestNameReply};

    #[test]
    fn fdo_contract_runs_only_inside_a_private_bus() {
        let executable = std::env::current_exe().expect("test executable path");
        let status = ProcessCommand::new("dbus-run-session")
            .arg("--")
            .arg(executable)
            .args([
                "--exact",
                "daemon::notifications::bus::tests::private_bus_contract",
                "--nocapture",
            ])
            .env("NACRE_PRIVATE_NOTIFICATIONS_BUS", "1")
            .status()
            .expect("dbus-run-session is required for the private notification bus fixture");
        assert!(status.success(), "private notification bus fixture failed");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn private_bus_contract() -> std::result::Result<(), Box<dyn std::error::Error>> {
        if std::env::var("NACRE_PRIVATE_NOTIFICATIONS_BUS").as_deref() != Ok("1") {
            return Ok(());
        }
        assert!(std::env::var_os("DBUS_SESSION_BUS_ADDRESS").is_some());

        // A pre-existing owner makes acquisition fail immediately. The failed
        // service attempt must leave that owner intact and release its objects.
        let owner = zbus::Connection::session().await?;
        let reply = owner
            .request_name_with_flags(NAME, RequestNameFlags::DoNotQueue.into())
            .await?;
        assert_eq!(reply, RequestNameReply::PrimaryOwner);
        let service_connection = zbus::Connection::session().await?;
        let (refused_requests, _refused_receiver) = super::super::channel();
        let refused = Owned::start(service_connection, refused_requests.0.clone()).await;
        assert!(refused.is_err());
        let dbus = DBusProxy::new(&owner).await?;
        assert!(
            dbus.name_has_owner(zbus::names::BusName::try_from(NAME)?)
                .await?
        );
        drop(dbus);
        drop(owner);

        let inspector = zbus::Connection::session().await?;
        let dbus = DBusProxy::new(&inspector).await?;
        timeout(Duration::from_secs(2), async {
            loop {
                if !dbus
                    .name_has_owner(zbus::names::BusName::try_from(NAME).expect("valid bus name"))
                    .await
                    .unwrap_or(true)
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await?;

        // Successful ownership exposes conditional persistence capability and
        // routes an FDO Notify call through the bounded ingress channel.
        let service_connection = zbus::Connection::session().await?;
        let (requests, mut receive) = super::super::channel();
        requests
            .0
            .ready
            .store(true, std::sync::atomic::Ordering::Release);
        let mut owned = Owned::start(service_connection, requests.0.clone()).await?;
        let client = zbus::Connection::session().await?;
        let proxy = zbus::Proxy::new(&client, NAME, PATH, NAME).await?;
        let unavailable_caps: Vec<String> = proxy.call("GetCapabilities", &()).await?;
        assert!(unavailable_caps.contains(&"actions".into()));
        assert!(!unavailable_caps.contains(&"persistence".into()));
        requests
            .0
            .writable
            .store(true, std::sync::atomic::Ordering::Release);
        let ready_caps: Vec<String> = proxy.call("GetCapabilities", &()).await?;
        assert!(ready_caps.contains(&"persistence".into()));

        let receiver = tokio::spawn(async move {
            let request = receive.recv().await.expect("Notify enters daemon ingress");
            match request.command {
                Command::Notify(notice) => {
                    assert_eq!(notice.app_name, "Fixture app");
                    assert_eq!(notice.summary, "Fixture summary");
                    assert_eq!(notice.body, "Plain text body");
                    assert_eq!(notice.actions, vec![("default".into(), "Open".into())]);
                    assert_eq!(notice.timeout, -1);
                    assert!(notice.sender.starts_with(':'));
                    let _ = request.reply.send(Ok(Response::NativeId(41)));
                }
                _ => panic!("expected a notification request"),
            }
        });
        let empty_hints = HashMap::<String, OwnedValue>::new();
        let id: u32 = proxy
            .call(
                "Notify",
                &(
                    "Fixture app",
                    0u32,
                    "fixture-icon",
                    "Fixture summary",
                    "Plain text body",
                    vec!["default", "Open"],
                    empty_hints,
                    -1i32,
                ),
            )
            .await?;
        assert_eq!(id, 41);
        receiver.await?;

        let oversized_body = "x".repeat(8193);
        let oversize: zbus::Result<u32> = proxy
            .call(
                "Notify",
                &(
                    "Fixture app",
                    0u32,
                    "",
                    "Too large",
                    oversized_body,
                    Vec::<String>::new(),
                    HashMap::<String, OwnedValue>::new(),
                    -1i32,
                ),
            )
            .await;
        assert!(oversize.is_err());

        // Signals are directed to the original unique owner; unrelated clients
        // connected to the same private bus do not receive them.
        let other = zbus::Connection::session().await?;
        let mut target_signal = proxy.receive_signal("NotificationClosed").await?;
        let other_proxy = zbus::Proxy::new(&other, NAME, PATH, NAME).await?;
        let mut other_signal = other_proxy.receive_signal("NotificationClosed").await?;
        let destination = client
            .unique_name()
            .expect("client has unique bus name")
            .to_string();
        owned
            .connection
            .emit_signal(
                Some(destination.as_str()),
                PATH,
                NAME,
                "NotificationClosed",
                &(41u32, 2u32),
            )
            .await?;
        let message = timeout(Duration::from_secs(1), target_signal.next())
            .await?
            .expect("target gets signal");
        let (native_id, reason): (u32, u32) = message.body().deserialize()?;
        assert_eq!((native_id, reason), (41, 2));
        assert!(timeout(Duration::from_millis(100), other_signal.next())
            .await
            .is_err());

        owned.stop().await;
        assert!(
            !dbus
                .name_has_owner(zbus::names::BusName::try_from(NAME)?)
                .await?
        );
        Ok(())
    }
}
