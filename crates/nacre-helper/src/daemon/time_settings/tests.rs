//! Fake timedated on a private bus. The real clock, polkit and the system and session buses are
//! never touched.
use super::*;
use crate::daemon::hub::Hub;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, MutexGuard},
    time::Instant,
};
use zbus::object_server::SignalEmitter;

struct State {
    timezone: String,
    ntp: bool,
    can_ntp: bool,
    deny: bool,
    /// `(method, interactive)` per accepted or refused write.
    writes: Vec<(String, bool)>,
}
#[derive(Clone)]
struct Backend(Arc<Mutex<State>>);
impl Backend {
    fn new(can_ntp: bool) -> Self {
        Self(Arc::new(Mutex::new(State {
            timezone: "Europe/Oslo".into(),
            ntp: true,
            can_ntp,
            deny: false,
            writes: Vec::new(),
        })))
    }
    fn state(&self) -> MutexGuard<'_, State> {
        self.0.lock().unwrap()
    }
}
#[derive(zbus::DBusError, Debug)]
#[zbus(prefix = "org.freedesktop.DBus.Error")]
enum FakeError {
    #[zbus(error)]
    ZBus(zbus::Error),
    AccessDenied(String),
}
struct Timedate(Backend);
#[zbus::interface(name = "org.freedesktop.timedate1")]
impl Timedate {
    #[zbus(property)]
    fn timezone(&self) -> String {
        self.0.state().timezone.clone()
    }
    #[zbus(property, name = "NTP")]
    fn ntp(&self) -> bool {
        self.0.state().ntp
    }
    #[zbus(property, name = "CanNTP")]
    fn can_ntp(&self) -> bool {
        self.0.state().can_ntp
    }
    fn list_timezones(&self) -> Vec<String> {
        [
            "Europe/Oslo",
            "America/Chicago",
            "UTC",
            "bad zone",
            "../etc",
        ]
        .map(String::from)
        .to_vec()
    }
    async fn set_timezone(
        &self,
        zone: String,
        interactive: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> std::result::Result<(), FakeError> {
        {
            let mut state = self.0.state();
            state.writes.push(("SetTimezone".into(), interactive));
            if state.deny {
                return Err(FakeError::AccessDenied("Access denied".into()));
            }
            state.timezone = zone;
        }
        self.timezone_changed(&emitter).await?;
        Ok(())
    }
    #[zbus(name = "SetNTP")]
    async fn set_ntp(
        &self,
        enabled: bool,
        interactive: bool,
        #[zbus(signal_emitter)] emitter: SignalEmitter<'_>,
    ) -> std::result::Result<(), FakeError> {
        {
            let mut state = self.0.state();
            state.writes.push(("SetNTP".into(), interactive));
            if state.deny {
                return Err(FakeError::AccessDenied("Access denied".into()));
            }
            state.ntp = enabled;
        }
        self.n_t_p_changed(&emitter).await?;
        Ok(())
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
    service: Option<Connection>,
    hub: Shared,
    task: tokio::task::JoinHandle<Result<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn timedated(address: &str, backend: &Backend) -> Result<Connection> {
    Ok(zbus::connection::Builder::address(address)?
        .name(DEST)?
        .serve_at(PATH, Timedate(backend.clone()))?
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
    async fn new(with_service: bool, can_ntp: bool) -> Result<Self> {
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
        let backend = Backend::new(can_ntp);
        let service = if with_service {
            Some(timedated(&address, &backend).await?)
        } else {
            None
        };
        let hub = Hub::new("time-boot".into(), false, Duration::from_millis(1)).shared();
        initial(&hub)?;
        let task = tokio::task::spawn_local(run_on(
            hub.clone(),
            zbus::connection::Builder::address(address.as_str())?,
        ));
        Ok(Self {
            _home: home,
            _bus: bus,
            address,
            backend,
            service,
            hub,
            task,
        })
    }
    fn state(&self) -> TimeState {
        serde_json::from_value(self.hub.borrow().snapshot(TIME_TOPIC).unwrap()).unwrap()
    }
    async fn ready() -> Result<Self> {
        let fixture = Self::new(true, true).await?;
        eventually("state", || fixture.state().health.available).await;
        Ok(fixture)
    }
    async fn call(&self, method: &str, params: Value) -> std::result::Result<Value, Failure> {
        dispatch(self.hub.clone(), method, params).await
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
fn the_topic_mirrors_timedated_and_methods_are_advertised() -> Result<()> {
    local(async {
        let f = Fixture::ready().await?;
        let state = f.state();
        state.validate().unwrap();
        assert_eq!(state.timezone, "Europe/Oslo");
        assert_eq!(
            state.ntp,
            Some(TimeNtp {
                supported: true,
                enabled: true
            })
        );
        let methods = available_methods(&f.hub.borrow());
        assert_eq!(
            methods.keys().map(String::as_str).collect::<Vec<_>>(),
            ["time.set_ntp", "time.set_timezone", "time.zones"]
        );
        Ok(())
    })
}

#[test]
fn zones_come_from_timedated_without_invalid_names() -> Result<()> {
    local(async {
        let f = Fixture::ready().await?;
        let reply = f.call("time.zones", json!({})).await.unwrap();
        assert_eq!(
            reply,
            json!({"zones":["Europe/Oslo","America/Chicago","UTC"]})
        );
        assert!(f.call("time.zones", json!({"x":1})).await.is_err());
        Ok(())
    })
}

#[test]
fn setting_the_zone_allows_the_polkit_prompt_and_updates_the_topic() -> Result<()> {
    local(async {
        let f = Fixture::ready().await?;
        let reply = f
            .call("time.set_timezone", json!({"zone":"America/Chicago"}))
            .await
            .unwrap();
        assert_eq!(reply, json!({"changed":true}));
        assert_eq!(
            f.backend.state().writes,
            vec![("SetTimezone".to_owned(), true)]
        );
        assert_eq!(f.state().timezone, "America/Chicago");
        // The same zone again is acknowledged without bothering timedated or polkit.
        let reply = f
            .call("time.set_timezone", json!({"zone":"America/Chicago"}))
            .await
            .unwrap();
        assert_eq!(reply, json!({"changed":false}));
        assert_eq!(f.backend.state().writes.len(), 1);
        Ok(())
    })
}

#[test]
fn bad_zone_names_never_reach_the_bus() -> Result<()> {
    local(async {
        let f = Fixture::ready().await?;
        for params in [
            json!({"zone":"../etc/passwd"}),
            json!({"zone":""}),
            json!({"zone":"Europe/Oslo","extra":1}),
            json!({}),
        ] {
            let error = f.call("time.set_timezone", params).await.unwrap_err();
            assert_eq!(error.code, ErrorCode::Validation);
            assert_eq!(error.dispatched, Some(false));
        }
        assert!(f.backend.state().writes.is_empty());
        Ok(())
    })
}

#[test]
fn a_refused_prompt_is_a_permission_error_and_changes_nothing() -> Result<()> {
    local(async {
        let f = Fixture::ready().await?;
        f.backend.state().deny = true;
        let error = f
            .call("time.set_timezone", json!({"zone":"UTC"}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert_eq!(error.dispatched, Some(false));
        assert_eq!(f.state().timezone, "Europe/Oslo");
        let error = f
            .call("time.set_ntp", json!({"enabled":false}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        assert!(f.state().ntp.unwrap().enabled);
        Ok(())
    })
}

#[test]
fn ntp_switches_through_timedated_and_needs_a_service() -> Result<()> {
    local(async {
        let f = Fixture::ready().await?;
        let reply = f
            .call("time.set_ntp", json!({"enabled":false}))
            .await
            .unwrap();
        assert_eq!(reply, json!({"changed":true}));
        assert_eq!(f.backend.state().writes, vec![("SetNTP".to_owned(), true)]);
        assert!(!f.state().ntp.unwrap().enabled);
        let reply = f
            .call("time.set_ntp", json!({"enabled":false}))
            .await
            .unwrap();
        assert_eq!(reply, json!({"changed":false}));
        assert!(f
            .call("time.set_ntp", json!({"enabled":"yes"}))
            .await
            .is_err());

        let unsupported = Fixture::new(true, false).await?;
        eventually("state", || unsupported.state().health.available).await;
        let error = unsupported
            .call("time.set_ntp", json!({"enabled":false}))
            .await
            .unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);
        assert!(unsupported.backend.state().writes.is_empty());
        Ok(())
    })
}

#[test]
fn outside_changes_are_published() -> Result<()> {
    local(async {
        let f = Fixture::ready().await?;
        f.backend.state().timezone = "UTC".into();
        let service = f.service.as_ref().unwrap();
        let object = service
            .object_server()
            .interface::<_, Timedate>(PATH)
            .await?;
        object
            .get()
            .await
            .timezone_changed(object.signal_emitter())
            .await?;
        eventually("zone", || f.state().timezone == "UTC").await;
        Ok(())
    })
}

#[test]
fn without_timedated_nothing_is_advertised_until_it_appears() -> Result<()> {
    local(async {
        let mut f = Fixture::new(false, true).await?;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let state = f.state();
        assert!(!state.health.available && state.ntp.is_none());
        assert!(available_methods(&f.hub.borrow()).is_empty());
        let error = f.call("time.zones", json!({})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::Unavailable);

        f.service = Some(timedated(&f.address, &f.backend).await?);
        eventually("available", || f.state().health.available).await;
        assert_eq!(available_methods(&f.hub.borrow()).len(), 3);
        Ok(())
    })
}

#[test]
fn safe_mode_refuses_and_advertises_nothing() -> Result<()> {
    local(async {
        let hub = Hub::new("safe".into(), true, Duration::from_millis(1)).shared();
        assert!(available_methods(&hub.borrow()).is_empty());
        let error = dispatch(hub, "time.zones", json!({})).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::PermissionDenied);
        Ok(())
    })
}
