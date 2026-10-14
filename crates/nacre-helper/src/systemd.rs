//! Shared native user-manager boundary for menu readers and owned unit actions.
use crate::Result;
use std::{collections::HashMap, time::Duration};
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

#[derive(zbus::zvariant::Type)]
#[zvariant(signature = "a{sv}")]
struct OrderedProperties(Vec<(String, OwnedValue)>);
impl<'de> serde::Deserialize<'de> for OrderedProperties {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = OrderedProperties;
            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("ordered D-Bus properties")
            }
            fn visit_map<A: serde::de::MapAccess<'de>>(
                self,
                mut map: A,
            ) -> std::result::Result<Self::Value, A::Error> {
                let mut values = Vec::new();
                while let Some(item) = map.next_entry()? {
                    values.push(item);
                }
                Ok(OrderedProperties(values))
            }
        }
        deserializer.deserialize_map(Visitor)
    }
}
fn unit_path(name: &str) -> String {
    let mut encoded = String::new();
    for (index, byte) in name.bytes().enumerate() {
        if byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit()) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("_{byte:02x}"));
        }
    }
    if encoded.is_empty() {
        encoded.push('_');
    }
    format!("/org/freedesktop/systemd1/unit/{encoded}")
}
pub fn report_units(
    names: &[String],
    properties: &[String],
    timeout: Duration,
) -> Result<(i32, String)> {
    report_units_at(names, properties, timeout, None)
}
pub fn failed_units(timeout: Duration) -> Result<String> {
    type Unit = (
        String,
        String,
        String,
        String,
        String,
        String,
        OwnedObjectPath,
        u32,
        String,
        OwnedObjectPath,
    );
    async_io::block_on(futures_lite::future::or(
        async {
            let connection = zbus::connection::Builder::session()?
                .method_timeout(timeout)
                .build()
                .await?;
            let reply = connection
                .call_method(
                    Some("org.freedesktop.systemd1"),
                    "/org/freedesktop/systemd1",
                    Some("org.freedesktop.systemd1.Manager"),
                    "ListUnitsFiltered",
                    &(vec!["failed"],),
                )
                .await?;
            if reply.body().len() > 2 * 1024 * 1024 {
                return Err("Systemd returned too much data".into());
            }
            let units: Vec<Unit> = reply.body().deserialize()?;
            let mut rows = Vec::new();
            for (name, description, load, active, sub, _, _, _, _, _) in units {
                rows.push(format!("{name} {load} {active} {sub} {description}"));
            }
            rows.sort();
            Ok(if rows.is_empty() {
                String::new()
            } else {
                rows.join("\n") + "\n"
            })
        },
        async {
            async_io::Timer::after(timeout).await;
            Err("Failed-unit query timed out".into())
        },
    ))
}
fn report_units_at(
    names: &[String],
    properties: &[String],
    timeout: Duration,
    address: Option<&str>,
) -> Result<(i32, String)> {
    async_io::block_on(futures_lite::future::or(
        async {
            let builder = match address {
                Some(address) => zbus::connection::Builder::address(address)?,
                None => zbus::connection::Builder::session()?,
            };
            let connection = builder.method_timeout(timeout).build().await?;
            let mut text = String::new();
            let mut first = true;
            for name in names {
                let reply = match connection
                    .call_method(
                        Some("org.freedesktop.systemd1"),
                        unit_path(name),
                        Some("org.freedesktop.DBus.Properties"),
                        "GetAll",
                        &"",
                    )
                    .await
                {
                    Ok(reply) => reply,
                    Err(error) => {
                        eprintln!("Service properties unavailable: {error}");
                        return Ok((1, text));
                    }
                };
                if reply.body().len() > 2 * 1024 * 1024 {
                    return Err("Systemd returned too much data".into());
                }
                let values: OrderedProperties = reply.body().deserialize()?;
                if !first {
                    text.push('\n');
                }
                first = false;
                for (name, value) in values.0 {
                    if !properties.contains(&name) {
                        continue;
                    }
                    let value = match &*value {
                        zbus::zvariant::Value::Str(s) => s.to_string(),
                        zbus::zvariant::Value::I32(value) => value.to_string(),
                        zbus::zvariant::Value::U32(value) => value.to_string(),
                        _ => return Err("Malformed systemd report property".into()),
                    };
                    text.push_str(&name);
                    text.push('=');
                    text.push_str(&value);
                    text.push('\n');
                }
            }
            Ok((0, text))
        },
        async {
            async_io::Timer::after(timeout).await;
            Err("Systemd report query timed out".into())
        },
    ))
}

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub active: bool,
    pub pid: u32,
    pub invocation: String,
}
impl Status {
    pub fn owner(&self) -> String {
        if self.active && self.pid != 0 && !self.invocation.is_empty() {
            format!("{}:{}", self.pid, self.invocation)
        } else {
            String::new()
        }
    }
}
fn absent(error: &zbus::Error) -> bool {
    matches!(error,zbus::Error::MethodError(name,_,_) if name.as_str()=="org.freedesktop.systemd1.NoSuchUnit")
}
fn is_active_state(state: &str) -> bool {
    matches!(state, "active" | "reloading" | "refreshing")
}
async fn query_active(connection: &zbus::Connection, name: &str) -> Result<bool> {
    let manager = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await?;
    let path: OwnedObjectPath = match manager.call("GetUnit", &name).await {
        Ok(path) => path,
        Err(error) if absent(&error) => return Ok(false),
        Err(error) => return Err(error.into()),
    };
    let reply = connection
        .call_method(
            Some("org.freedesktop.systemd1"),
            path,
            Some("org.freedesktop.DBus.Properties"),
            "Get",
            &("org.freedesktop.systemd1.Unit", "ActiveState"),
        )
        .await?;
    let state = String::try_from(reply.body().deserialize::<OwnedValue>()?)?;
    Ok(is_active_state(&state))
}
pub fn active(name: &str) -> Result<bool> {
    active_at(name, None)
}
fn active_at(name: &str, address: Option<&str>) -> Result<bool> {
    async_io::block_on(futures_lite::future::or(
        async {
            let builder = match address {
                Some(address) => zbus::connection::Builder::address(address)?,
                None => zbus::connection::Builder::session()?,
            };
            let connection = builder
                .method_timeout(Duration::from_secs(5))
                .build()
                .await?;
            query_active(&connection, name).await
        },
        async {
            async_io::Timer::after(Duration::from_secs(5)).await;
            Err("Systemd state query timed out after 5 seconds".into())
        },
    ))
}
async fn query(connection: &zbus::Connection, name: &str) -> Result<Status> {
    let manager = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        "/org/freedesktop/systemd1",
        "org.freedesktop.systemd1.Manager",
    )
    .await?;
    let path: OwnedObjectPath = match manager.call("GetUnit", &name).await {
        Ok(path) => path,
        Err(error) if absent(&error) => return Ok(Status::default()),
        Err(error) => return Err(error.into()),
    };
    let unit = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        path.clone(),
        "org.freedesktop.systemd1.Unit",
    )
    .await?;
    let state: String = unit.get_property("ActiveState").await?;
    let invocation: Vec<u8> = unit.get_property("InvocationID").await?;
    let service = zbus::Proxy::new(
        connection,
        "org.freedesktop.systemd1",
        path,
        "org.freedesktop.systemd1.Service",
    )
    .await?;
    let pid: u32 = service.get_property("MainPID").await?;
    Ok(Status {
        active: state == "active",
        pid,
        invocation: invocation
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    })
}
pub fn status(name: &str) -> Result<Status> {
    status_at(name, None)
}
fn status_at(name: &str, address: Option<&str>) -> Result<Status> {
    async_io::block_on(futures_lite::future::or(
        async {
            let builder = match address {
                Some(address) => zbus::connection::Builder::address(address)?,
                None => zbus::connection::Builder::session()?,
            };
            let connection = builder
                .method_timeout(Duration::from_secs(5))
                .build()
                .await?;
            query(&connection, name).await
        },
        async {
            async_io::Timer::after(Duration::from_secs(5)).await;
            Err("Systemd state query timed out after 5 seconds".into())
        },
    ))
}
pub fn stop(name: &str) -> Result<()> {
    crate::menu_registry::systemd_job("StopUnit", &(name, "replace"))
}
pub fn wait_owner(name: &str) -> Result<String> {
    wait_owner_at(name, None)
}
fn wait_owner_at(name: &str, address: Option<&str>) -> Result<String> {
    use futures_lite::StreamExt;
    async_io::block_on(futures_lite::future::or(
        async {
            let builder = match address {
                Some(address) => zbus::connection::Builder::address(address)?,
                None => zbus::connection::Builder::session()?,
            };
            let connection = builder
                .method_timeout(Duration::from_secs(2))
                .build()
                .await?;
            let manager = zbus::Proxy::new(
                &connection,
                "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1",
                "org.freedesktop.systemd1.Manager",
            )
            .await?;
            let path: OwnedObjectPath = match manager.call("GetUnit", &name).await {
                Ok(path) => path,
                Err(error) if absent(&error) => return Ok(String::new()),
                Err(error) => return Err(error.into()),
            };
            let properties = zbus::Proxy::new(
                &connection,
                "org.freedesktop.systemd1",
                path,
                "org.freedesktop.DBus.Properties",
            )
            .await?;
            let mut changed = properties.receive_signal("PropertiesChanged").await?;
            let _: () = manager.call("Subscribe", &()).await?;
            loop {
                let owner = query(&connection, name).await?.owner();
                if !owner.is_empty() {
                    return Ok(owner);
                }
                if changed.next().await.is_none() {
                    return Err("Service activation event source closed".into());
                }
            }
        },
        async {
            async_io::Timer::after(Duration::from_secs(2)).await;
            Ok(String::new())
        },
    ))
}
pub fn start(name: &str, argv: &[String]) -> Result<()> {
    let first = argv.first().ok_or("Missing service command")?;
    let executable = if first.contains('/') {
        std::path::absolute(first)?
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|root| root.join(first))
            .find(|path| path.is_file())
            .ok_or("Service command is unavailable")?
    };
    let executable = std::path::absolute(executable)?;
    let executable = executable.to_str().ok_or("Service command is not UTF-8")?;
    let mut argv = argv.to_vec();
    argv[0] = executable.into();
    let description = format!(
        "[systemd-run] {}",
        argv.iter()
            .map(|word| crate::menu_registry::description_word(word))
            .collect::<Vec<_>>()
            .join(" ")
    );
    let properties: Vec<(&str, OwnedValue)> = vec![
        (
            "Description",
            OwnedValue::try_from(zbus::zvariant::Value::new(description))?,
        ),
        (
            "CollectMode",
            OwnedValue::try_from(zbus::zvariant::Value::new("inactive-or-failed"))?,
        ),
        ("AddRef", OwnedValue::from(true)),
        (
            "ExecStart",
            OwnedValue::try_from(zbus::zvariant::Value::new(vec![(executable, argv, false)]))?,
        ),
    ];
    crate::menu_registry::systemd_job(
        "StartTransientUnit",
        &(
            name,
            "fail",
            properties,
            Vec::<(String, Vec<(String, OwnedValue)>)>::new(),
        ),
    )
}
pub fn interrupt_and_wait(name: &str, expected_owner: &str) -> Result<()> {
    interrupt_at(name, expected_owner, None)
}
fn interrupt_at(name: &str, expected_owner: &str, address: Option<&str>) -> Result<()> {
    use futures_lite::StreamExt;
    async_io::block_on(futures_lite::future::or(
        async {
            let builder = match address {
                Some(address) => zbus::connection::Builder::address(address)?,
                None => zbus::connection::Builder::session()?,
            };
            let connection = builder
                .method_timeout(Duration::from_secs(5))
                .build()
                .await?;
            let manager = zbus::Proxy::new(
                &connection,
                "org.freedesktop.systemd1",
                "/org/freedesktop/systemd1",
                "org.freedesktop.systemd1.Manager",
            )
            .await?;
            let path: OwnedObjectPath = manager.call("GetUnit", &name).await?;
            let properties = zbus::Proxy::new(
                &connection,
                "org.freedesktop.systemd1",
                path,
                "org.freedesktop.DBus.Properties",
            )
            .await?;
            let mut changed = properties.receive_signal("PropertiesChanged").await?;
            let _: () = manager.call("Subscribe", &()).await?;
            if query(&connection, name).await?.owner() != expected_owner {
                return Err("This recording belongs to another controller".into());
            }
            let _: () = manager.call("KillUnit", &(name, "all", 2i32)).await?;
            if !query(&connection, name).await?.active {
                return Ok(());
            }
            while let Some(message) = changed.next().await {
                let (interface, values, _): (String, HashMap<String, OwnedValue>, Vec<String>) =
                    message.body().deserialize()?;
                if interface == "org.freedesktop.systemd1.Unit" {
                    if let Some(state) = values.get("ActiveState") {
                        if String::try_from(state.try_clone()?)? != "active" {
                            return Ok(());
                        }
                    }
                }
            }
            Err("Recording state event source closed".into())
        },
        async {
            async_io::Timer::after(Duration::from_secs(5)).await;
            Err("Recording is still finishing; try again shortly".into())
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        io::{BufRead, BufReader},
        process::{Child, Command, Stdio},
        sync::{
            atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
            Arc,
        },
    };
    struct Unit {
        active: Arc<AtomicBool>,
    }
    #[zbus::interface(name = "org.freedesktop.systemd1.Unit")]
    impl Unit {
        #[zbus(property)]
        fn active_state(&self) -> String {
            if self.active.load(Ordering::SeqCst) {
                "active"
            } else {
                "inactive"
            }
            .into()
        }
        #[zbus(property, name = "InvocationID")]
        fn invocation_id(&self) -> Vec<u8> {
            vec![0x11; 16]
        }
    }
    struct Service {
        pid: Arc<AtomicU32>,
    }
    #[zbus::interface(name = "org.freedesktop.systemd1.Service")]
    impl Service {
        #[zbus(property, name = "MainPID")]
        fn main_pid(&self) -> u32 {
            self.pid.load(Ordering::SeqCst)
        }
    }
    struct Manager {
        active: Arc<AtomicBool>,
        kills: Arc<AtomicUsize>,
        hang: Arc<AtomicBool>,
    }
    #[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
    impl Manager {
        fn subscribe(&self) {}
        fn get_unit(&self, _name: &str) -> OwnedObjectPath {
            OwnedObjectPath::try_from("/org/freedesktop/systemd1/unit/test").expect("fixture path")
        }
        async fn kill_unit(
            &self,
            name: &str,
            who: &str,
            signal: i32,
            #[zbus(connection)] connection: &zbus::Connection,
        ) -> zbus::fdo::Result<()> {
            assert_eq!(name, "nacre-screen-recording.service");
            assert_eq!(who, "all");
            assert_eq!(signal, 2);
            self.kills.fetch_add(1, Ordering::SeqCst);
            if !self.hang.load(Ordering::SeqCst) {
                self.active.store(false, Ordering::SeqCst);
                connection
                    .emit_signal(
                        None::<&str>,
                        "/org/freedesktop/systemd1/unit/test",
                        "org.freedesktop.DBus.Properties",
                        "PropertiesChanged",
                        &(
                            "org.freedesktop.systemd1.Unit",
                            HashMap::from([(
                                "ActiveState",
                                OwnedValue::try_from(zbus::zvariant::Value::new("inactive"))
                                    .expect("fixture string variant"),
                            )]),
                            Vec::<String>::new(),
                        ),
                    )
                    .await?;
            }
            Ok(())
        }
    }
    struct Daemon(Child);
    impl Drop for Daemon {
        fn drop(&mut self) {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    #[test]
    fn private_manager_owner_guard_and_state_events() -> Result<()> {
        let root = tempfile::tempdir()?;
        let mut daemon = Daemon(
            Command::new("dbus-daemon")
                .args([
                    "--session",
                    "--nofork",
                    "--print-address",
                    &format!("--address=unix:path={}", root.path().join("bus").display()),
                ])
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()?,
        );
        let mut address = String::new();
        BufReader::new(daemon.0.stdout.take().ok_or("Missing fixture address")?)
            .read_line(&mut address)?;
        let address = address.trim();
        let active = Arc::new(AtomicBool::new(true));
        let kills = Arc::new(AtomicUsize::new(0));
        let hang = Arc::new(AtomicBool::new(false));
        let pid = Arc::new(AtomicU32::new(7));
        let _server = async_io::block_on(
            zbus::connection::Builder::address(address)?
                .name("org.freedesktop.systemd1")?
                .serve_at(
                    "/org/freedesktop/systemd1",
                    Manager {
                        active: active.clone(),
                        kills: kills.clone(),
                        hang: hang.clone(),
                    },
                )?
                .serve_at(
                    "/org/freedesktop/systemd1/unit/test",
                    Unit {
                        active: active.clone(),
                    },
                )?
                .serve_at(
                    "/org/freedesktop/systemd1/unit/test",
                    Service { pid: pid.clone() },
                )?
                .build(),
        )?;
        let state = status_at("nacre-screen-recording.service", Some(address))?;
        assert!(active_at("nacre-screen-recording.service", Some(address))?);
        assert_eq!(state.owner(), format!("7:{}", "11".repeat(16)));
        assert_eq!(
            wait_owner_at("nacre-screen-recording.service", Some(address))?,
            state.owner()
        );
        pid.store(0, Ordering::SeqCst);
        let publisher = _server.clone();
        let activated = pid.clone();
        let transition = std::thread::spawn(move || -> Result<()> {
            std::thread::sleep(Duration::from_millis(50));
            activated.store(7, Ordering::SeqCst);
            async_io::block_on(publisher.emit_signal(
                None::<&str>,
                "/org/freedesktop/systemd1/unit/test",
                "org.freedesktop.DBus.Properties",
                "PropertiesChanged",
                &(
                    "org.freedesktop.systemd1.Service",
                    HashMap::from([("MainPID", OwnedValue::from(7u32))]),
                    Vec::<String>::new(),
                ),
            ))?;
            Ok(())
        });
        assert_eq!(
            wait_owner_at("nacre-screen-recording.service", Some(address))?,
            state.owner()
        );
        transition
            .join()
            .map_err(|_| "Fixture transition failed")??;
        pid.store(0, Ordering::SeqCst);
        assert_eq!(
            wait_owner_at("nacre-screen-recording.service", Some(address))?,
            ""
        );
        pid.store(7, Ordering::SeqCst);
        let error = interrupt_at("nacre-screen-recording.service", "7:foreign", Some(address))
            .expect_err("Foreign invocation must not be signaled");
        assert_eq!(
            error.to_string(),
            "This recording belongs to another controller"
        );
        assert_eq!(kills.load(Ordering::SeqCst), 0);
        interrupt_at(
            "nacre-screen-recording.service",
            &state.owner(),
            Some(address),
        )?;
        assert_eq!(kills.load(Ordering::SeqCst), 1);
        assert!(!status_at("nacre-screen-recording.service", Some(address))?.active);
        assert!(!active_at("nacre-screen-recording.service", Some(address))?);
        active.store(true, Ordering::SeqCst);
        hang.store(true, Ordering::SeqCst);
        let error = interrupt_at(
            "nacre-screen-recording.service",
            &state.owner(),
            Some(address),
        )
        .expect_err("Missing state transition must time out");
        assert_eq!(
            error.to_string(),
            "Recording is still finishing; try again shortly"
        );
        Ok(())
    }
}
