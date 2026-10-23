//! Login/resume/display observer on the daemon runtime; policy comes from session_watch.
use super::{
    compositor_control::Control,
    hub::{failure, Notice, Shared},
};
use crate::{json as legacy_json, paths::Paths, process, session_watch, Result};
use futures_util::StreamExt;
use nacre_protocol::{Capability, ErrorCode, Failure, Method, Reply};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, process::Command, time::Duration};
use tokio::sync::{mpsc, oneshot};

pub struct Check {
    event: String,
    reply: oneshot::Sender<std::result::Result<Value, Failure>>,
}
#[derive(Clone)]
pub struct Requests(mpsc::Sender<Check>);
impl Requests {
    pub async fn check(&self, event: &str) -> std::result::Result<Value, Failure> {
        let (reply, result) = oneshot::channel();
        self.0
            .try_send(Check {
                event: event.into(),
                reply,
            })
            .map_err(|error| match error {
                mpsc::error::TrySendError::Full(_) => {
                    failure(ErrorCode::Busy, "Session observer is busy", true)
                }
                mpsc::error::TrySendError::Closed(_) => failure(
                    ErrorCode::Unavailable,
                    "Session observer is unavailable",
                    true,
                ),
            })?;
        result
            .await
            .map_err(|_| failure(ErrorCode::Unavailable, "Session observer ended", true))?
    }
}
pub fn methods() -> BTreeMap<String, Method> {
    ["session.check","session.ready","session.watch"].into_iter().map(|name| (name.into(),Method {
        v:1,class:Capability::Desktop,timeout:if name=="session.ready" {60000}else{30000},
        params:if name=="session.check" {json!({"type":"object","properties":{"event":{"type":"string"}},"additionalProperties":false})}else{json!({"type":"object","additionalProperties":false})},
        result:json!({"type":"object"}),
    })).collect()
}
pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    let object = params
        .as_object()
        .ok_or_else(|| failure(ErrorCode::Validation, "Expected session parameters", false))?;
    if object
        .keys()
        .any(|key| key != "event" || method != "session.check")
    {
        return Err(failure(
            ErrorCode::Validation,
            "Unknown session parameter",
            false,
        ));
    }
    if method == "session.watch" {
        return Ok(
            json!({"status":hub.borrow().services.get("session-watch").cloned().unwrap_or(json!({"status":"starting"}))}),
        );
    }
    let requests = hub.borrow().session.clone().ok_or_else(|| {
        failure(
            ErrorCode::Unavailable,
            "Session observer is unavailable",
            true,
        )
    })?;
    if method == "session.check" {
        let event = match object.get("event") {
            None => "manual",
            Some(Value::String(value)) => value,
            Some(_) => {
                return Err(failure(
                    ErrorCode::Validation,
                    "Expected a session event string",
                    false,
                ))
            }
        };
        return requests.check(event).await;
    }
    if method != "session.ready" {
        return Err(failure(
            ErrorCode::UnknownMethod,
            "Unknown session method",
            false,
        ));
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    while tokio::time::Instant::now() < deadline {
        let value = requests.check("startup-readiness").await?;
        if session_watch::is_ready(&value) {
            return Ok(value);
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
    requests.check("startup-timeout").await
}

pub fn wire(record: &Value) -> Result<Value> {
    let mut value = record.clone();
    let mut monitors = serde_json::Map::new();
    let mut order = Vec::new();
    for monitor in record["monitors"]
        .as_array()
        .ok_or("Invalid session monitors")?
    {
        let name = monitor["name"]
            .as_str()
            .ok_or("Invalid session monitor name")?;
        if monitors.insert(name.into(), monitor.clone()).is_some() {
            return Err("Duplicate session monitor name".into());
        }
        order.push(name.to_owned());
    }
    value["monitors"] = Value::Object(monitors);
    value["order"] = json!(order);
    Ok(value)
}
pub fn legacy(record: &Value) -> Result<Value> {
    let mut value = record.clone();
    let monitors = record["monitors"]
        .as_object()
        .ok_or("Invalid session monitor collection")?;
    let mut rows = Vec::new();
    for name in record["order"]
        .as_array()
        .ok_or("Invalid session monitor order")?
    {
        rows.push(
            monitors
                .get(name.as_str().ok_or("Invalid session monitor name")?)
                .ok_or("Missing session monitor")?
                .clone(),
        );
    }
    value["monitors"] = json!(rows);
    value
        .as_object_mut()
        .ok_or("Invalid session record")?
        .remove("order");
    Ok(value)
}

pub fn client(action: &str, event: &str) -> Result<Option<i32>> {
    let explicit = std::env::var_os("NACRE_DAEMON_SOCKET").is_some();
    if !explicit
        && std::env::var_os("XDG_RUNTIME_DIR")
            .is_none_or(|path| !std::path::Path::new(&path).is_absolute())
    {
        return Ok(None);
    }
    let endpoint = super::socket::path()?;
    if let Err(error) = super::socket::validate_endpoint(&endpoint) {
        if !explicit
            && error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound)
        {
            return Ok(None);
        }
        return Err(error);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    let mut connection = match runtime.block_on(super::client::connect()) {
        Ok(connection) => connection,
        Err(super::client::ConnectError::Absent) if !explicit => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let method = format!("session.{action}");
    if !connection.methods.contains_key(&method) {
        let status = runtime.block_on(connection.call("daemon.status", json!({}), Some(1000)))?;
        if status["safe"] == true {
            if action == "watch" {
                eprintln!("Session observer is paused in daemon safe mode");
                return Ok(Some(0));
            }
            return Err("Session observer is paused in daemon safe mode".into());
        }
        if !explicit {
            return Ok(None);
        }
        return Err("Selected daemon does not provide the session observer".into());
    }
    let params = if action == "check" {
        json!({"event":event})
    } else {
        json!({})
    };
    let result = runtime.block_on(connection.call(&method, params, None))?;
    if action == "watch" {
        if result["status"]["status"] != "running" {
            eprintln!("Daemon session observer is not yet running; see daemon status");
        }
    } else {
        println!("{}", legacy_json::dumps(&legacy(&result)?, false)?);
    }
    Ok(Some(0))
}

struct Observer {
    hub: Shared,
    paths: Paths,
    control: Control,
    wallet: Option<zbus::Connection>,
}
impl Observer {
    async fn apply(&mut self, effects: &[session_watch::Effect]) -> Result<()> {
        for effect in effects {
            match effect {
                session_watch::Effect::Check(event) => {
                    self.check(event).await?;
                }
                session_watch::Effect::Restore => self.restore().await?,
                session_watch::Effect::Wait(duration) => tokio::time::sleep(*duration).await,
            }
        }
        Ok(())
    }
    async fn desktop(&self) -> Result<String> {
        let forwarded = self.hub.borrow_mut().forward(
            "ui.dispatch",
            json!({"target":"nacre","method":"state","args":[]}),
            Some(5000),
        );
        match forwarded {
            Ok((id, response)) => {
                let _route = super::session::Route {
                    hub: self.hub.clone(),
                    id,
                };
                let result = tokio::time::timeout(Duration::from_secs(5), response).await;
                return match result {
                    Ok(Ok(Reply::Ok {
                        ok: Value::String(value),
                    })) => Ok(value),
                    Ok(Ok(Reply::Ok { ok })) => Ok(legacy_json::dumps(&ok, false)?),
                    _ => {
                        tracing::warn!("Session UI state call failed after dispatch");
                        Ok(String::new())
                    }
                };
            }
            Err(error) if error.dispatched == Some(false) => {}
            Err(_) => {
                tracing::warn!("Session UI provider is unavailable");
                return Ok(String::new());
            }
        }
        let executable = self.paths.share.join("shell/bin/qs");
        crate::process::offload(move || -> Result<String> {
            let mut command = Command::new(executable);
            command.args(["-c", "nacre", "ipc", "call", "nacre", "state"]);
            match process::capture(command, None, Duration::from_secs(5)) {
                Ok(output) if output.status.success() => {
                    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
                }
                _ => {
                    tracing::warn!("Session legacy UI state is unavailable");
                    Ok(String::new())
                }
            }
        })
        .await?
    }
    async fn wallet(&mut self) -> Result<&'static str> {
        if self.wallet.is_none() {
            self.wallet = Some(
                zbus::connection::Builder::session()?
                    .method_timeout(Duration::from_secs(5))
                    .build()
                    .await?,
            );
        }
        let proxy = zbus::Proxy::new(
            self.wallet.as_ref().ok_or("Missing wallet connection")?,
            "org.freedesktop.secrets",
            "/org/freedesktop/secrets/aliases/default",
            "org.freedesktop.Secret.Collection",
        )
        .await?;
        let locked: bool = proxy.get_property("Locked").await?;
        Ok(if locked { "b true" } else { "b false" })
    }
    async fn check(&mut self, event: &str) -> Result<Value> {
        let desktop = self.desktop().await?;
        let wallet = match self.wallet().await {
            Ok(value) => value,
            Err(_) => {
                tracing::warn!("Session wallet is unavailable");
                self.wallet = None;
                ""
            }
        };
        let (monitors, windows, errors) = self
            .control
            .with(|backend| {
                let monitors = backend
                    .query("monitors", Duration::from_secs(5), None)
                    .unwrap_or_else(|_| {
                        tracing::warn!("Session monitors are unavailable");
                        json!([])
                    });
                let windows = backend
                    .query("windows", Duration::from_secs(5), None)
                    .unwrap_or_else(|_| {
                        tracing::warn!("Session windows are unavailable");
                        json!([])
                    });
                let errors = backend
                    .module_call("config_errors", &[], &json!({"timeout":5}))
                    .unwrap_or_else(|_| {
                        tracing::warn!("Session configuration errors are unavailable");
                        json!("")
                    });
                Ok((monitors, windows, errors))
            })
            .await?;
        let record = session_watch::record(
            event,
            &chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.6f+00:00")
                .to_string(),
            &desktop,
            wallet,
            &monitors,
            &windows,
            &errors,
        )?;
        let directory = self.paths.state.join("session");
        let saved = record.clone();
        crate::process::offload(move || session_watch::publish(&directory, &saved)).await??;
        let value = wire(&record)?;
        self.hub
            .borrow_mut()
            .publish("session", json!({"available":true,"record":value}))?;
        Ok(value)
    }
    async fn restore(&self) -> Result<()> {
        let path = self.paths.config.join("desktop.json");
        self.control
            .with(move |backend| {
                if backend.capabilities()["monitorConfiguration"] == false {
                    return Ok(());
                }
                let data = match fs::read(path) {
                    Ok(bytes) => serde_json::from_slice::<Value>(&bytes)?,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                    Err(error) => return Err(error.into()),
                };
                let monitors = backend
                    .query("monitors", Duration::from_secs(5), None)
                    .unwrap_or_else(|_| {
                        tracing::warn!("Session restore monitors are unavailable");
                        json!([])
                    });
                let saved = session_watch::connected_displays(
                    &backend.capabilities(),
                    Some(&data),
                    &monitors,
                )?;
                if saved.as_array().is_some_and(|saved| !saved.is_empty()) {
                    backend.action("configure_monitors", &[saved], &json!({"timeout":null}))?;
                }
                Ok(())
            })
            .await
    }
}

pub async fn run(hub: Shared, paths: Paths, control: Control) -> Result<()> {
    let result = observe(hub.clone(), paths, control).await;
    if result.is_err() {
        let mut last = hub
            .borrow()
            .snapshot("session")
            .unwrap_or_else(|| json!({}));
        last["available"] = json!(false);
        last["error"] = json!("Session observer is unavailable");
        if let Err(error) = hub.borrow_mut().publish("session", last) {
            tracing::error!(code=?error.code,"Cannot publish session observer failure");
        }
    }
    result
}
async fn observe(hub: Shared, paths: Paths, control: Control) -> Result<()> {
    let (sender, mut commands) = mpsc::channel::<Check>(16);
    hub.borrow_mut().session = Some(Requests(sender));
    let mut observer = Observer {
        hub: hub.clone(),
        paths,
        control,
        wallet: None,
    };
    observer.apply(session_watch::startup_effects()).await?;
    let connection = zbus::connection::Builder::system()?
        .method_timeout(Duration::from_secs(5))
        .build()
        .await?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .sender("org.freedesktop.login1")?
        .path("/org/freedesktop/login1")?
        .interface("org.freedesktop.login1.Manager")?
        .member("PrepareForSleep")?
        .build();
    let mut sleep = zbus::MessageStream::for_match_rule(rule, &connection, Some(128)).await?;
    let mut events = hub.borrow().observe();
    let failures = hub
        .borrow()
        .services
        .get("session-watch")
        .and_then(|s| s["failures"].as_u64())
        .unwrap_or(0) as usize;
    hub.borrow_mut()
        .service("session-watch", "running", failures)?;
    loop {
        tokio::select! {
            message=sleep.next()=>{
                let (sleeping,):(bool,)=message.ok_or("Session system-bus stream ended")??.body().deserialize()?;
                observer.apply(session_watch::effects(session_watch::Event::Sleep(sleeping))).await?;
            },
            command=commands.recv()=>{
                let command=command.ok_or("Session request channel ended")?;
                if !command.reply.is_closed() {
                    let result=observer.check(&command.event).await.map_err(|_|{tracing::warn!("Session evidence request failed");failure(ErrorCode::Io,"Session evidence could not be published",true)});
                    if command.reply.send(result).is_err() {tracing::debug!("Session evidence caller ended");}
                }
            },
            notice=events.recv()=>{
                let outputs=match notice {
                    Ok(Notice::Changed{topic,available:Some(false)}) if topic=="compositor"=>return Err("Session compositor stream ended".into()),
                    Ok(Notice::Event{topic,name,data})=>(topic=="compositor" && name=="outputs-changed") || (topic==super::niri_events::TOPIC && data["type"]=="outputs-changed"),
                    // Output notifications have a fixed small payload. A large
                    // window/snapshot event requests state resync, not a display
                    // restore. Ring overflow below can lose output notifications
                    // and must still reconcile the current displays.
                    Ok(Notice::Resync(_))=>false,
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{tracing::warn!("Session events overflowed; reconciling current displays");true},
                    Err(tokio::sync::broadcast::error::RecvError::Closed)=>return Err("Session compositor observer ended".into()),
                    _=>false,
                };
                if outputs {observer.apply(session_watch::effects(session_watch::Event::Outputs)).await?;}
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test(flavor = "current_thread")]
    async fn cancelling_session_state_read_releases_the_ui_route() -> Result<()> {
        let home = tempfile::tempdir()?;
        let hub = super::super::hub::Hub::new("fixture".into(), false, Duration::from_millis(16))
            .shared();
        let (provider, _) = hub.borrow_mut().add_client()?;
        hub.borrow_mut()
            .register(provider, BTreeMap::from([("ui.dispatch".into(), 1)]))?;
        let paths = Paths::new(home.path());
        let observer = Observer {
            hub: hub.clone(),
            control: Control::new(paths.clone(), BTreeMap::new()),
            paths,
            wallet: None,
        };
        let mut reading = Box::pin(observer.desktop());
        assert!(futures_util::poll!(&mut reading).is_pending());
        let packet = hub
            .borrow_mut()
            .next(provider, std::time::Instant::now())
            .0
            .ok_or("Missing UI state request")?;
        let nacre_protocol::Message::Call { id, .. } = packet.message else {
            return Err("Unexpected UI request".into());
        };
        drop(reading);
        hub.borrow_mut().answer(
            provider,
            &id,
            Reply::Ok {
                ok: json!({"launcherMode":"apps"}),
            },
        )?;
        Ok(())
    }
    #[test]
    fn frozen_records_survive_wire_collections_without_changed_stdout() -> Result<()> {
        let fixture: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/session-watch-reference.json"
        ))?;
        let records = fixture["records"]
            .as_array()
            .ok_or("Missing frozen records")?;
        for case in records {
            let original = &case["result"];
            let projected = wire(original)?;
            assert!(projected["monitors"].is_object());
            let restored = legacy(&projected)?;
            assert_eq!(
                legacy_json::dumps(&restored, false)?,
                legacy_json::dumps(original, false)?,
                "{}",
                case["name"]
            );
        }
        assert_eq!(records.len(), 4);
        Ok(())
    }
    #[tokio::test(flavor = "current_thread")]
    async fn invalid_parameters_never_reach_the_session_actor() {
        let hub = super::super::hub::Hub::new("fixture".into(), false, Duration::from_millis(16))
            .shared();
        let (sender, mut receiver) = mpsc::channel(1);
        hub.borrow_mut().session = Some(Requests(sender));
        for (method, params) in [
            ("session.check", json!({"path":"/not-a-store"})),
            ("session.check", json!({"event":[] })),
            ("session.ready", json!({"event":"unexpected"})),
        ] {
            assert_eq!(
                dispatch(hub.clone(), method, params)
                    .await
                    .unwrap_err()
                    .code,
                ErrorCode::Validation
            );
            assert!(matches!(
                receiver.try_recv(),
                Err(mpsc::error::TryRecvError::Empty)
            ));
        }
    }
}
