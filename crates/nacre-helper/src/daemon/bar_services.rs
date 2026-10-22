//! Bar-facing actions backed by output discovery and an existing SNI watcher.
use super::hub::{failure, Shared};
use crate::{light_devices, process, Result};
use nacre_protocol::{bar_services::*, ErrorCode, Failure, Method};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Arc, OnceLock},
    time::Duration,
};

static BRIGHTNESS_LOCK: OnceLock<Arc<tokio::sync::Mutex<()>>> = OnceLock::new();

const WATCHER: &str = "org.kde.StatusNotifierWatcher";
const WATCHER_PATH: &str = "/StatusNotifierWatcher";
const WATCHER_IFACE: &str = "org.kde.StatusNotifierWatcher";
const ITEM_IFACE: &str = "org.kde.StatusNotifierItem";

pub fn available_methods(hub: &super::hub::Hub) -> BTreeMap<String, Method> {
    if hub.safe {
        return BTreeMap::new();
    }
    let mut methods = nacre_protocol::bar_services::methods();
    let tray_ready = hub
        .services
        .get("tray")
        .is_some_and(|state| state["status"] == "running");
    let compositor = hub.snapshot("compositor").unwrap_or(Value::Null);
    let has_outputs = compositor["available"] == true
        && compositor["outputs"]
            .as_object()
            .is_some_and(|outputs| !outputs.is_empty());
    let brightness_tool = program_available("ddcutil") || program_available("brightnessctl");
    methods.retain(|name, _| {
        if name.starts_with("tray.") {
            tray_ready
        } else {
            has_outputs && brightness_tool
        }
    });
    methods
}

pub async fn run_tray(hub: Shared) -> Result<()> {
    let cleanup = TrayCleanup(hub.clone());
    let result = run_tray_inner(hub).await;
    drop(cleanup);
    result
}

struct TrayCleanup(Shared);
impl Drop for TrayCleanup {
    fn drop(&mut self) {
        if let Err(error) = self.0.borrow_mut().service("tray", "waiting", 0) {
            tracing::warn!(code=?error.code, "Cannot clear tray service status");
        }
        if let Err(error) = self.0.borrow_mut().publish("tray", empty_tray(false)) {
            tracing::warn!(code=?error.code, "Cannot clear tray snapshot");
        }
    }
}

async fn run_tray_inner(hub: Shared) -> Result<()> {
    use futures_lite::StreamExt;
    let connection = zbus::Connection::session().await?;
    let owner_rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.DBus")?
        .interface("org.freedesktop.DBus")?
        .member("NameOwnerChanged")?
        .build();
    let mut owners = zbus::MessageStream::for_match_rule(owner_rule, &connection, Some(16)).await?;
    loop {
        let watcher_owner = match name_owner(&connection, WATCHER).await {
            Ok(owner) => owner,
            Err(_) => {
                hub.borrow_mut().service("tray", "waiting", 0)?;
                hub.borrow_mut().publish("tray", empty_tray(false))?;
                let owner = owners.next().await.ok_or("Tray owner stream closed")??;
                let (_name, _old, new): (String, String, String) = owner.body().deserialize()?;
                if new.is_empty() {
                    continue;
                }
                continue;
            }
        };
        let signal_rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .sender(watcher_owner.as_str())?
            .path(WATCHER_PATH)?
            .build();
        let item_rule = zbus::MatchRule::builder()
            .msg_type(zbus::message::Type::Signal)
            .interface(ITEM_IFACE)?
            .build();
        let mut signals =
            zbus::MessageStream::for_match_rule(signal_rule, &connection, Some(128)).await?;
        let mut item_signals =
            zbus::MessageStream::for_match_rule(item_rule, &connection, Some(128)).await?;
        if name_owner(&connection, WATCHER).await.ok().as_deref() != Some(watcher_owner.as_str()) {
            continue;
        }
        register_tray_host(&connection, &watcher_owner).await?;
        hub.borrow_mut().service("tray", "running", 0)?;
        publish_tray(&hub, &connection, &watcher_owner).await?;
        loop {
            tokio::select! {
                signal = signals.next() => {
                    let _signal = signal.ok_or("Tray watcher signal stream closed")??;
                    publish_tray(&hub, &connection, &watcher_owner).await?;
                }
                signal = item_signals.next() => {
                    let signal = signal.ok_or("Tray item signal stream closed")??;
                    if let Some(sender) = signal.header().sender().map(|sender| sender.to_string()) {
                        if registered_owners(&connection, &watcher_owner).await?.contains(&sender) {
                            publish_tray(&hub, &connection, &watcher_owner).await?;
                        }
                    }
                }
                owner = owners.next() => {
                    let owner = owner.ok_or("Tray watcher owner stream closed")??;
                    let (name, _old, _new): (String, String, String) = owner.body().deserialize()?;
                    if name == WATCHER {
                        hub.borrow_mut().service("tray", "waiting", 0)?;
                        hub.borrow_mut().publish("tray", empty_tray(false))?;
                        break;
                    }
                    if registered_names(&connection, &watcher_owner).await?.contains(&name) {
                        publish_tray(&hub, &connection, &watcher_owner).await?;
                    }
                }
            }
        }
    }
}

pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::Unavailable,
            "Bar service actions are disabled in safe mode",
            false,
        ));
    }
    match method {
        "brightness.set" => {
            let request: BrightnessSet = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            set_brightness(&hub, &request.output, i32::from(request.percent), false).await
        }
        "brightness.adjust" => {
            let request: BrightnessAdjust =
                serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            set_brightness(&hub, &request.output, i32::from(request.delta), true).await
        }
        "tray.activate" | "tray.secondary_activate" | "tray.context_menu" => {
            let request: TrayItemAction = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            tray_action(&hub, method, &request).await?;
            Ok(json!({"changed":true}))
        }
        "tray.scroll" => {
            let request: TrayScroll = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            if request.delta == 0 {
                return Ok(json!({"changed":false}));
            }
            tray_scroll(&hub, &request).await?;
            Ok(json!({"changed":true}))
        }
        _ => Err(failure(
            ErrorCode::UnknownMethod,
            "Unknown bar service method",
            false,
        )),
    }
}

async fn set_brightness(
    hub: &Shared,
    output: &str,
    amount: i32,
    relative: bool,
) -> std::result::Result<Value, Failure> {
    let compositor = hub
        .borrow()
        .snapshot("compositor")
        .filter(|state| state["available"] == true)
        .ok_or_else(|| failure(ErrorCode::Unavailable, "Outputs are unavailable", true))?;
    let outputs = compositor["outputs"]
        .as_object()
        .ok_or_else(|| failure(ErrorCode::Unavailable, "Outputs are unavailable", true))?;
    let descriptor = outputs
        .get(output)
        .ok_or_else(|| failure(ErrorCode::Unavailable, "Output is unavailable", true))?;
    let screen = descriptor
        .get("name")
        .and_then(Value::as_str)
        .unwrap_or(output);
    let screens = outputs
        .values()
        .filter_map(|row| {
            row.get("name")
                .and_then(Value::as_str)
                .map(|name| json!(name))
        })
        .collect::<Vec<_>>();
    let screen = screen.to_owned();
    let control = hub.borrow().compositor.clone().ok_or_else(|| {
        failure(
            ErrorCode::Unavailable,
            "Compositor control is unavailable",
            true,
        )
    })?;
    let native_outputs = control
        .with(|backend| {
            Ok(backend.query("monitors", Duration::from_secs(5), Some(4 * 1024 * 1024))?)
        })
        .await
        .map_err(|_| {
            failure(
                ErrorCode::Unavailable,
                "Current outputs are unavailable",
                true,
            )
        })?;
    let present = native_outputs
        .as_array()
        .into_iter()
        .flatten()
        .chain(
            native_outputs
                .as_object()
                .into_iter()
                .flat_map(|rows| rows.values()),
        )
        .any(|row| row["name"] == screen || row["id"] == output);
    if !present {
        return Err(failure(
            ErrorCode::StaleRevision,
            "Output disappeared; refresh outputs",
            true,
        ));
    }
    let lane = BRIGHTNESS_LOCK
        .get_or_init(|| Arc::new(tokio::sync::Mutex::new(())))
        .clone();
    let guard = lane
        .try_lock_owned()
        .map_err(|_| failure(ErrorCode::Busy, "Brightness controls are busy", true))?;
    tokio::task::spawn_blocking(move || {
        let _guard = guard;
        set_brightness_native(&screen, screens, amount, relative)
    })
    .await
    .map_err(|_| {
        failure(ErrorCode::Internal, "Brightness worker failed", false).dispatch_state(true)
    })?
    .map_err(|_| {
        failure(ErrorCode::Io, "Output brightness update failed", false).dispatch_state(true)
    })?;
    Ok(json!({"changed":true}))
}

fn set_brightness_native(
    screen: &str,
    screens: Vec<Value>,
    amount: i32,
    relative: bool,
) -> Result<()> {
    let discovered = light_devices::discover(&[json!(screen)], Path::new("/sys"), &mut native_run)?;
    let internal = screen.to_ascii_lowercase().starts_with("edp")
        || screen.to_ascii_lowercase().starts_with("lvds");
    let device = if internal {
        let internal_count = screens
            .iter()
            .filter(|name| {
                name.as_str().is_some_and(|name| {
                    let name = name.to_ascii_lowercase();
                    name.starts_with("edp") || name.starts_with("lvds")
                })
            })
            .count();
        if internal_count != 1 {
            return Err("Internal output brightness cannot be mapped uniquely".into());
        }
        discovered.get("backlight").filter(|row| !row.is_null())
    } else {
        discovered["ddc"]
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["screen"] == screen))
    }
    .ok_or("Output brightness control is unavailable")?;
    let maximum = device["maximum"]
        .as_i64()
        .filter(|value| *value > 0)
        .ok_or("Output brightness range is unavailable")?;
    let current = device["current"]
        .as_i64()
        .ok_or("Output brightness is unavailable")?;
    let value = if relative {
        current + (maximum * i64::from(amount) / 100)
    } else {
        maximum * i64::from(amount) / 100
    };
    light_devices::write(device, value, Path::new("/sys"), &mut native_run)?;
    Ok(())
}

fn native_run(program: &str, args: &[String], timeout: Duration) -> Result<String> {
    let output = process::checked(program, args, None, timeout)?;
    Ok(String::from_utf8(output.stdout)?)
}

async fn tray_action(
    hub: &Shared,
    method: &str,
    request: &TrayItemAction,
) -> std::result::Result<(), Failure> {
    if !published_tray_item(hub, &request.item) {
        return Err(stale());
    }
    let connection = zbus::Connection::session()
        .await
        .map_err(|_| unavailable("Tray is unavailable"))?;
    tray_action_on_connection(&connection, method, request).await
}
async fn tray_action_on_connection(
    connection: &zbus::Connection,
    method: &str,
    request: &TrayItemAction,
) -> std::result::Result<(), Failure> {
    let (owner, path) = resolve_tray(connection, &request.item).await?;
    let proxy = zbus::Proxy::new(connection, owner.as_str(), path.as_str(), ITEM_IFACE)
        .await
        .map_err(|_| stale())?;
    match method {
        "tray.activate" => {
            proxy
                .call::<_, _, ()>("Activate", &(i32::from(request.x), i32::from(request.y)))
                .await
        }
        "tray.secondary_activate" => {
            proxy
                .call::<_, _, ()>(
                    "SecondaryActivate",
                    &(i32::from(request.x), i32::from(request.y)),
                )
                .await
        }
        _ => {
            proxy
                .call::<_, _, ()>("ContextMenu", &(i32::from(request.x), i32::from(request.y)))
                .await
        }
    }
    .map_err(|_| action_failed())?;
    Ok(())
}

async fn tray_scroll(hub: &Shared, request: &TrayScroll) -> std::result::Result<(), Failure> {
    if !published_tray_item(hub, &request.item) {
        return Err(stale());
    }
    let connection = zbus::Connection::session()
        .await
        .map_err(|_| unavailable("Tray is unavailable"))?;
    tray_scroll_on_connection(&connection, request).await
}
async fn tray_scroll_on_connection(
    connection: &zbus::Connection,
    request: &TrayScroll,
) -> std::result::Result<(), Failure> {
    let (owner, path) = resolve_tray(connection, &request.item).await?;
    let proxy = zbus::Proxy::new(connection, owner.as_str(), path.as_str(), ITEM_IFACE)
        .await
        .map_err(|_| stale())?;
    proxy
        .call::<_, _, ()>(
            "Scroll",
            &(
                i32::from(request.delta),
                match request.axis {
                    TrayScrollAxis::Horizontal => "horizontal",
                    TrayScrollAxis::Vertical => "vertical",
                },
            ),
        )
        .await
        .map_err(|_| action_failed())?;
    Ok(())
}

async fn resolve_tray(
    connection: &zbus::Connection,
    id: &str,
) -> std::result::Result<(String, String), Failure> {
    let watcher_owner = name_owner(connection, WATCHER)
        .await
        .map_err(|_| unavailable("Tray watcher is unavailable"))?;
    let registered: Vec<String> = zbus::Proxy::new(
        connection,
        watcher_owner.as_str(),
        WATCHER_PATH,
        WATCHER_IFACE,
    )
    .await
    .map_err(|_| unavailable("Tray watcher is unavailable"))?
    .get_property("RegisteredStatusNotifierItems")
    .await
    .map_err(|_| unavailable("Tray watcher is unavailable"))?;
    for entry in registered.into_iter().take(256) {
        let Some((bus_name, path)) = registered_parts(&entry) else {
            continue;
        };
        let Ok(owner) = name_owner(connection, &bus_name).await else {
            continue;
        };
        if tray_id(connection.server_guid().as_str(), &owner, &path) == id {
            return Ok((owner, path));
        }
    }
    Err(stale())
}

async fn registered_entries(
    connection: &zbus::Connection,
    watcher_owner: &str,
) -> zbus::Result<Vec<String>> {
    let proxy = zbus::Proxy::new(connection, watcher_owner, WATCHER_PATH, WATCHER_IFACE).await?;
    proxy.get_property("RegisteredStatusNotifierItems").await
}

async fn register_tray_host(connection: &zbus::Connection, owner: &str) -> Result<()> {
    let watcher = zbus::Proxy::new(connection, owner, WATCHER_PATH, WATCHER_IFACE).await?;
    let host = connection
        .unique_name()
        .ok_or("Missing tray host identity")?;
    watcher
        .call::<_, _, ()>("RegisterStatusNotifierHost", &host.as_str())
        .await?;
    Ok(())
}

async fn registered_owners(
    connection: &zbus::Connection,
    watcher_owner: &str,
) -> Result<std::collections::BTreeSet<String>> {
    let mut owners = std::collections::BTreeSet::new();
    for entry in registered_entries(connection, watcher_owner)
        .await?
        .into_iter()
        .take(256)
    {
        if let Some((name, _)) = registered_parts(&entry) {
            if let Ok(owner) = name_owner(connection, &name).await {
                owners.insert(owner);
            }
        }
    }
    Ok(owners)
}

async fn registered_names(
    connection: &zbus::Connection,
    watcher_owner: &str,
) -> Result<std::collections::BTreeSet<String>> {
    Ok(registered_entries(connection, watcher_owner)
        .await?
        .into_iter()
        .take(256)
        .filter_map(|entry| registered_parts(&entry).map(|(name, _)| name))
        .collect())
}

async fn publish_tray(
    hub: &Shared,
    connection: &zbus::Connection,
    watcher_owner: &str,
) -> Result<()> {
    let registered = registered_entries(connection, watcher_owner).await?;
    let mut items = serde_json::Map::new();
    let mut order = Vec::new();
    for entry in registered.into_iter().take(256) {
        let Some((bus_name, path)) = registered_parts(&entry) else {
            continue;
        };
        let Ok(owner) = name_owner(connection, &bus_name).await else {
            continue;
        };
        let Ok(proxy) =
            zbus::Proxy::new(connection, owner.as_str(), path.as_str(), ITEM_IFACE).await
        else {
            continue;
        };
        let title: String = proxy.get_property("Title").await.unwrap_or_default();
        let title: String = title.chars().take(256).collect();
        let status: String = proxy
            .get_property("Status")
            .await
            .unwrap_or_else(|_| "Active".into());
        let item_is_menu: bool = proxy.get_property("ItemIsMenu").await.unwrap_or(false);
        let icon: Option<String> =
            proxy
                .get_property::<String>("IconName")
                .await
                .ok()
                .filter(|name| {
                    name.len() <= 256
                        && !name.is_empty()
                        && name
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
                });
        let id = tray_id(connection.server_guid().as_str(), &owner, &path);
        if items.contains_key(&id) {
            continue;
        }
        order.push(id.clone());
        items.insert(id, json!({"title":title,"visible":status != "Passive","item_is_menu":item_is_menu,"attention":status == "NeedsAttention","icon":icon,"menu":{"items":{},"order":[]}}));
    }
    hub.borrow_mut().publish(
        "tray",
        json!({"health":{"available":true,"stale":false},"items":{"items":items,"order":order}}),
    )?;
    Ok(())
}

fn empty_tray(available: bool) -> Value {
    json!({"health":{"available":available,"stale":false},"items":{"items":{},"order":[]}})
}
fn tray_id(bus: &str, owner: &str, path: &str) -> String {
    let digest = Sha256::digest(format!("{bus}\0{owner}\0{path}").as_bytes());
    format!(
        "sni:{}",
        digest[..16]
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>()
    )
}
fn registered_parts(entry: &str) -> Option<(String, String)> {
    let (name, path) = entry
        .split_once('/')
        .unwrap_or((entry, "StatusNotifierItem"));
    zbus::names::BusName::try_from(name).ok()?;
    zbus::zvariant::ObjectPath::try_from(format!("/{path}")).ok()?;
    if name.is_empty()
        || name.len() > 255
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_.:-".contains(&b))
        || path.is_empty()
        || path.len() > 255
        || !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_/".contains(&b))
        || path.split('/').any(|part| part == "." || part == "..")
    {
        return None;
    }
    Some((name.into(), format!("/{path}")))
}
async fn name_owner(connection: &zbus::Connection, name: &str) -> Result<String> {
    let name = zbus::names::BusName::try_from(name)?;
    let owner = zbus::fdo::DBusProxy::new(connection)
        .await?
        .get_name_owner(name)
        .await?;
    Ok(owner.to_string())
}
fn published_tray_item(hub: &Shared, id: &str) -> bool {
    hub.borrow()
        .snapshot("tray")
        .is_some_and(|state| state["items"]["items"].get(id).is_some())
}
fn program_available(program: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::var_os("PATH").is_some_and(|path| {
        std::env::split_paths(&path).any(|directory| {
            std::fs::metadata(directory.join(program))
                .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
        })
    })
}
fn invalid() -> Failure {
    failure(
        ErrorCode::Validation,
        "Invalid bar action parameters",
        false,
    )
}
fn stale() -> Failure {
    failure(
        ErrorCode::StaleRevision,
        "Tray item is stale; refresh tray state",
        true,
    )
}
fn action_failed() -> Failure {
    failure(
        ErrorCode::Io,
        "Tray action failed; it may already have completed",
        false,
    )
    .dispatch_state(true)
}
fn unavailable(message: &str) -> Failure {
    failure(ErrorCode::Unavailable, message, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{Arc, Mutex},
    };

    struct PrivateBus(Child);
    impl Drop for PrivateBus {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }

    struct Watcher {
        items: Vec<String>,
        hosts: Mutex<Vec<String>>,
    }
    #[zbus::interface(name = "org.kde.StatusNotifierWatcher")]
    impl Watcher {
        #[zbus(property)]
        fn registered_status_notifier_items(&self) -> Vec<String> {
            self.items.clone()
        }
        #[zbus(property)]
        fn is_status_notifier_host_registered(&self) -> bool {
            !self.hosts.lock().unwrap().is_empty()
        }
        fn register_status_notifier_host(
            &self,
            host: String,
            #[zbus(header)] header: zbus::message::Header<'_>,
        ) -> zbus::fdo::Result<()> {
            if header.sender().map(|sender| sender.as_str()) != Some(host.as_str()) {
                return Err(zbus::fdo::Error::AccessDenied(
                    "Fixture host identity mismatch".into(),
                ));
            }
            self.hosts.lock().unwrap().push(host);
            Ok(())
        }
    }

    #[derive(Clone, Default)]
    struct Item(Arc<Mutex<Vec<(String, i32, i32)>>>);
    #[zbus::interface(name = "org.kde.StatusNotifierItem")]
    impl Item {
        #[zbus(property)]
        fn title(&self) -> String {
            "Fixture item".into()
        }
        #[zbus(property)]
        fn status(&self) -> String {
            "Active".into()
        }
        #[zbus(property)]
        fn item_is_menu(&self) -> bool {
            false
        }
        #[zbus(property)]
        fn icon_name(&self) -> String {
            "fixture-icon".into()
        }
        fn activate(&self, x: i32, y: i32) {
            self.0.lock().unwrap().push(("Activate".into(), x, y));
        }
        fn secondary_activate(&self, x: i32, y: i32) {
            self.0
                .lock()
                .unwrap()
                .push(("SecondaryActivate".into(), x, y));
        }
        fn context_menu(&self, x: i32, y: i32) {
            self.0.lock().unwrap().push(("ContextMenu".into(), x, y));
        }
        fn scroll(&self, delta: i32, orientation: &str) {
            self.0.lock().unwrap().push((orientation.into(), delta, 0));
        }
    }

    async fn private_fixture() -> crate::Result<(
        tempfile::TempDir,
        PrivateBus,
        zbus::Connection,
        zbus::Connection,
        zbus::Connection,
        Arc<Mutex<Vec<(String, i32, i32)>>>,
    )> {
        let home = tempfile::tempdir()?;
        let mut bus = PrivateBus(
            Command::new("dbus-daemon")
                .args([
                    "--session",
                    "--nofork",
                    "--print-address",
                    &format!("--address=unix:path={}", home.path().join("bus").display()),
                ])
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .stdin(Stdio::null())
                .spawn()?,
        );
        let mut address = String::new();
        BufReader::new(bus.0.stdout.take().ok_or("Missing fixture bus address")?)
            .read_line(&mut address)?;
        let address = address.trim().to_owned();
        let watcher = zbus::connection::Builder::address(address.as_str())?
            .name(WATCHER)?
            .serve_at(
                WATCHER_PATH,
                Watcher {
                    items: vec!["org.example.Bar/StatusNotifierItem".into()],
                    hosts: Mutex::new(Vec::new()),
                },
            )?
            .build()
            .await?;
        let actions = Arc::new(Mutex::new(Vec::new()));
        let item = zbus::connection::Builder::address(address.as_str())?
            .name("org.example.Bar")?
            .serve_at("/StatusNotifierItem", Item(actions.clone()))?
            .build()
            .await?;
        let client = zbus::connection::Builder::address(address.as_str())?
            .build()
            .await?;
        Ok((home, bus, watcher, item, client, actions))
    }

    #[test]
    fn notifier_registration_is_split_into_validated_owner_and_path() {
        assert_eq!(
            registered_parts("org.example.App/StatusNotifierItem"),
            Some(("org.example.App".into(), "/StatusNotifierItem".into()))
        );
        assert_eq!(
            registered_parts("org.example.App"),
            Some(("org.example.App".into(), "/StatusNotifierItem".into()))
        );
        assert!(registered_parts("../bad/Path").is_none());
        assert_ne!(
            tray_id("bus-a", ":1.20", "/Item"),
            tray_id("bus-a", ":1.21", "/Item")
        );
        assert_ne!(
            tray_id("bus-a", ":1.20", "/Item"),
            tray_id("bus-b", ":1.20", "/Item")
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn registered_item_owner_and_methods_are_checked_on_private_bus() -> crate::Result<()> {
        let (_home, _bus, _watcher, _item, connection, actions) = private_fixture().await?;
        let owner = name_owner(&connection, "org.example.Bar").await?;
        let watcher_owner = name_owner(&connection, WATCHER).await?;
        register_tray_host(&connection, &watcher_owner).await?;
        assert!(
            zbus::Proxy::new(
                &connection,
                watcher_owner.as_str(),
                WATCHER_PATH,
                WATCHER_IFACE
            )
            .await?
            .get_property::<bool>("IsStatusNotifierHostRegistered")
            .await?
        );
        let id = tray_id(
            connection.server_guid().as_str(),
            &owner,
            "/StatusNotifierItem",
        );
        let hub = crate::daemon::hub::Hub::new("fixture".into(), false, Duration::from_millis(1))
            .shared();
        publish_tray(&hub, &connection, &name_owner(&connection, WATCHER).await?).await?;
        for (method, request) in [
            (
                "tray.activate",
                TrayItemAction {
                    item: id.clone(),
                    x: 17,
                    y: 23,
                },
            ),
            (
                "tray.secondary_activate",
                TrayItemAction {
                    item: id.clone(),
                    x: 18,
                    y: 24,
                },
            ),
            (
                "tray.context_menu",
                TrayItemAction {
                    item: id.clone(),
                    x: 19,
                    y: 25,
                },
            ),
        ] {
            tray_action_on_connection(&connection, method, &request)
                .await
                .map_err(|failure| failure.message)?;
        }
        tray_scroll_on_connection(
            &connection,
            &TrayScroll {
                item: id.clone(),
                axis: TrayScrollAxis::Horizontal,
                delta: -2,
            },
        )
        .await
        .map_err(|failure| failure.message)?;
        assert!(tray_action_on_connection(
            &connection,
            "tray.activate",
            &TrayItemAction {
                item: "sni:stale".into(),
                x: 0,
                y: 0
            }
        )
        .await
        .is_err());
        assert_eq!(
            *actions.lock().unwrap(),
            vec![
                ("Activate".into(), 17, 23),
                ("SecondaryActivate".into(), 18, 24),
                ("ContextMenu".into(), 19, 25),
                ("horizontal".into(), -2, 0),
            ]
        );
        Ok(())
    }
}
