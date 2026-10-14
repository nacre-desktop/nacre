//! Unit signals plus the bounded environment refresh required by systemd.
use crate::{
    niri_acceptance_monitor::{snapshot_records, UNITS},
    Result,
};
use futures_lite::StreamExt;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    time::{Duration, Instant},
};
use zbus::{
    message::Type,
    zvariant::{OwnedObjectPath, OwnedValue},
    Connection, MatchRule, MessageStream, Proxy,
};
const NAME: &str = "org.freedesktop.systemd1";
const ROOT: &str = "/org/freedesktop/systemd1";
const MANAGER: &str = "org.freedesktop.systemd1.Manager";

pub struct Watch {
    connection: Connection,
    signals: MessageStream,
    owners: MessageStream,
    records: Vec<Value>,
    paths: BTreeMap<String, usize>,
    environment: Vec<String>,
    next_environment: Instant,
    owner: bool,
    dirty: bool,
}
pub enum Update {
    Changed,
    Timeout,
    Disconnected,
}
impl Watch {
    pub async fn connect() -> Result<Self> {
        let connection = zbus::connection::Builder::session()?
            .method_timeout(Duration::from_secs(4))
            .build()
            .await?;
        let signals = MessageStream::for_match_rule(
            MatchRule::builder()
                .msg_type(Type::Signal)
                .sender(NAME)?
                .path_namespace(ROOT)?
                .build(),
            &connection,
            Some(128),
        )
        .await?;
        let owners = MessageStream::for_match_rule(
            MatchRule::builder()
                .msg_type(Type::Signal)
                .sender("org.freedesktop.DBus")?
                .interface("org.freedesktop.DBus")?
                .member("NameOwnerChanged")?
                .arg(0, NAME)?
                .build(),
            &connection,
            Some(16),
        )
        .await?;
        let mut watch = Self {
            connection,
            signals,
            owners,
            records: Vec::new(),
            paths: BTreeMap::new(),
            environment: Vec::new(),
            next_environment: Instant::now(),
            owner: false,
            dirty: true,
        };
        if let Err(error) = watch.initialize().await {
            if unavailable(error.as_ref()) {
                watch.dirty = false;
            }
            eprintln!("Systemd observation initialization failed: {error}");
        }
        Ok(watch)
    }
    async fn manager(&self) -> Result<Proxy<'_>> {
        Ok(Proxy::new(&self.connection, NAME, ROOT, MANAGER).await?)
    }
    async fn initialize(&mut self) -> Result<()> {
        if !self.owner {
            let manager = self.manager().await?;
            let _: () = manager.call("Subscribe", &()).await?;
            self.owner = true;
        }
        self.refresh().await
    }
    async fn refresh_unit(&mut self, index: usize) -> Result<()> {
        let manager = self.manager().await?;
        let path: OwnedObjectPath = match manager.call("GetUnit", &(UNITS[index],)).await {
            Ok(path) => path,
            Err(zbus::Error::MethodError(name, _, _))
                if name.as_str() == "org.freedesktop.systemd1.NoSuchUnit" =>
            {
                self.paths.retain(|_, value| *value != index);
                self.records[index] =
                    json!({"Id":UNITS[index],"ActiveState":"inactive","MainPID":"0"});
                return Ok(());
            }
            Err(error) => return Err(error.into()),
        };
        let unit = Proxy::new(
            &self.connection,
            NAME,
            path.as_str(),
            "org.freedesktop.systemd1.Unit",
        )
        .await?;
        let id: String = unit.get_property("Id").await?;
        let active: String = unit.get_property("ActiveState").await?;
        let service = Proxy::new(
            &self.connection,
            NAME,
            path.as_str(),
            "org.freedesktop.systemd1.Service",
        )
        .await?;
        let pid: u32 = service.get_property("MainPID").await?;
        self.records[index] = json!({"Id":id,"ActiveState":active,"MainPID":pid.to_string()});
        self.paths.retain(|_, value| *value != index);
        self.paths.insert(path.to_string(), index);
        Ok(())
    }
    async fn environment(&mut self) -> Result<()> {
        self.environment = self.manager().await?.get_property("Environment").await?;
        self.next_environment = Instant::now() + Duration::from_secs(2);
        Ok(())
    }
    async fn refresh(&mut self) -> Result<()> {
        self.records = UNITS
            .iter()
            .map(|name| json!({"Id":name,"ActiveState":"inactive","MainPID":"0"}))
            .collect();
        self.paths.clear();
        for index in 0..UNITS.len() {
            self.refresh_unit(index).await?;
        }
        self.environment().await?;
        self.dirty = false;
        Ok(())
    }
    pub fn snapshot(&self) -> Result<Value> {
        if self.dirty {
            return snapshot_records(&[], &[]);
        }
        snapshot_records(&self.records, &self.environment)
    }
    pub async fn next(&mut self, deadline: Instant) -> Result<Update> {
        enum Event {
            Signal(Option<zbus::Result<zbus::Message>>),
            Owner(Option<zbus::Result<zbus::Message>>),
            Timer,
        }
        let timer = if self.owner || self.dirty {
            self.next_environment.min(deadline)
        } else {
            deadline
        };
        let event = futures_lite::future::race(
            async { Event::Signal(self.signals.next().await) },
            futures_lite::future::race(async { Event::Owner(self.owners.next().await) }, async {
                async_io::Timer::at(timer).await;
                Event::Timer
            }),
        )
        .await;
        if Instant::now() >= deadline {
            return Ok(Update::Timeout);
        }
        let result = match event {
            Event::Timer => {
                self.next_environment = Instant::now() + Duration::from_secs(2);
                if self.dirty {
                    self.initialize().await
                } else if self.owner {
                    self.environment().await
                } else {
                    Ok(())
                }
            }
            Event::Owner(Some(Ok(message))) => {
                let (_, _, new): (String, String, String) = message.body().deserialize()?;
                self.records.clear();
                self.paths.clear();
                self.environment.clear();
                self.owner = false;
                if new.is_empty() {
                    self.dirty = false;
                    Ok(())
                } else {
                    self.dirty = true;
                    self.initialize().await
                }
            }
            Event::Signal(Some(Ok(message))) => self.signal(&message).await,
            Event::Signal(None) | Event::Owner(None) => return Ok(Update::Disconnected),
            Event::Signal(Some(Err(error))) | Event::Owner(Some(Err(error))) => {
                return Err(error.into())
            }
        };
        if let Err(error) = result {
            self.dirty = true;
            self.next_environment = Instant::now() + Duration::from_secs(2);
            eprintln!("Systemd observation refresh failed: {error}");
        }
        Ok(Update::Changed)
    }
    async fn signal(&mut self, message: &zbus::Message) -> Result<()> {
        let header = message.header();
        let Some(member) = header.member() else {
            return Ok(());
        };
        match member.as_str() {
            "UnitNew" | "UnitRemoved" => {
                let (name, _): (String, OwnedObjectPath) = message.body().deserialize()?;
                if UNITS.contains(&name.as_str()) {
                    self.refresh().await?;
                }
            }
            "PropertiesChanged" => {
                let Some(path) = header.path() else {
                    return Ok(());
                };
                if self.paths.contains_key(path.as_str()) {
                    let (_, changed, invalidated): (
                        String,
                        HashMap<String, OwnedValue>,
                        Vec<String>,
                    ) = message.body().deserialize()?;
                    if ["Id", "ActiveState", "MainPID"].iter().any(|name| {
                        changed.contains_key(*name) || invalidated.iter().any(|key| key == name)
                    }) {
                        // Read all relevant units after a signal: combining one
                        // fresh unit with other stale cache entries can invent
                        // an isolation violation during an ordered transition.
                        self.refresh().await?;
                    }
                } else if path.as_str() == ROOT {
                    let (_, changed, invalidated): (
                        String,
                        HashMap<String, OwnedValue>,
                        Vec<String>,
                    ) = message.body().deserialize()?;
                    if changed.contains_key("Environment")
                        || invalidated.iter().any(|name| name == "Environment")
                    {
                        self.environment().await?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
}

fn unavailable(error: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    matches!(error.downcast_ref::<zbus::Error>(),Some(zbus::Error::MethodError(name,_,_)) if ["org.freedesktop.DBus.Error.ServiceUnknown","org.freedesktop.DBus.Error.NameHasNoOwner"].contains(&name.as_str()))
}
