//! The existing X11 width policy, driven by the shared compositor owner.
use super::{
    compositor_control::Control,
    hub::{failure, Notice, Shared},
};
use crate::{
    niri_widths::{self, x11, Policy},
    Result,
};
use nacre_protocol::{
    widths::{WidthsConfigure, WidthsStatus},
    Capability, ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{
    cell::Cell,
    collections::BTreeMap,
    os::fd::{AsFd, AsRawFd, OwnedFd},
    rc::Rc,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::{
    io::unix::AsyncFd,
    sync::{mpsc, oneshot, Notify},
};

pub const TOPIC: &str = "niri.widths";
/// A bounded internal tap on the existing reducer, not another IPC connection.
pub struct Source {
    sequence: u64,
    snapshot: Option<Value>,
    events: tokio::sync::broadcast::Sender<(u64, Value)>,
}
impl Default for Source {
    fn default() -> Self {
        Self {
            sequence: 0,
            snapshot: None,
            events: tokio::sync::broadcast::channel(nacre_protocol::MAX_EVENTS).0,
        }
    }
}
fn policy_window(row: &Value) -> Value {
    json!({"id":row["id"],"app_id":row["app_id"],"title":row["title"],"is_floating":row["is_floating"]})
}
impl Source {
    pub fn update(
        &mut self,
        windows: &serde_json::Map<String, Value>,
        event: &Value,
    ) -> Result<()> {
        if !["WindowsChanged", "WindowOpenedOrChanged", "WindowClosed"]
            .iter()
            .any(|key| event.get(key).is_some())
        {
            return Ok(());
        }
        let snapshot = Value::Object(
            windows
                .iter()
                .map(|(id, row)| (id.clone(), policy_window(row)))
                .collect(),
        );
        let projected = if event.get("WindowsChanged").is_some() {
            windows_event(snapshot.clone())?
        } else if let Some(change) = event.get("WindowOpenedOrChanged") {
            json!({"WindowOpenedOrChanged":{"window":policy_window(&change["window"])}})
        } else {
            json!({"WindowClosed":event["WindowClosed"]})
        };
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or("Width observer event sequence exhausted")?;
        self.snapshot = Some(snapshot);
        if self.events.receiver_count() > 0 && self.events.send((self.sequence, projected)).is_err()
        {
            tracing::debug!("Width observer subscriber ended");
        }
        Ok(())
    }
    fn current(&self) -> Option<(u64, Value)> {
        self.snapshot.clone().map(|value| (self.sequence, value))
    }
}
type Response = std::result::Result<Value, Failure>;
struct Request {
    params: WidthsConfigure,
    reply: oneshot::Sender<Response>,
}
#[derive(Clone)]
pub struct Requests(mpsc::Sender<Request>);
pub fn methods() -> BTreeMap<String, Method> {
    BTreeMap::from([(
        "niri.widths.configure".into(),
        Method {
            v: 1,
            class: Capability::Desktop,
            timeout: 2000,
            params: serde_json::from_str(include_str!(
                "../../../../protocol/schema/widths-configure.json"
            ))
            .expect("committed widths schema"),
            result: serde_json::from_str(include_str!(
                "../../../../protocol/schema/widths-status.json"
            ))
            .expect("committed widths schema"),
        },
    )])
}
pub async fn dispatch(hub: Shared, params: Value) -> Response {
    let params: WidthsConfigure = serde_json::from_value(params).map_err(|_| {
        failure(
            ErrorCode::Validation,
            "Invalid width observer configuration",
            false,
        )
    })?;
    params.validate()?;
    let requests = hub.borrow().widths.clone().ok_or_else(|| {
        failure(
            ErrorCode::Unavailable,
            "Width observer is unavailable",
            true,
        )
    })?;
    let (reply, receive) = oneshot::channel();
    requests
        .0
        .try_send(Request { params, reply })
        .map_err(|_| failure(ErrorCode::Busy, "Width observer queue is unavailable", true))?;
    receive
        .await
        .map_err(|_| failure(ErrorCode::Unavailable, "Width observer stopped", true))?
}

/// Preserve the foreground command while the daemon owns all policy and sockets.
/// Only an absent default endpoint (or an older daemon without this service)
/// permits the existing standalone observer. Never fall back after configuration.
pub fn observe_if_daemon(
    opening_width: f64,
    environment: &BTreeMap<String, String>,
) -> Result<Option<i32>> {
    use super::client::{self, ConnectError};
    use nacre_protocol::{framing, Id, Message, Mirror, Update};
    let explicit = std::env::var_os("NACRE_DAEMON_SOCKET").is_some();
    if !explicit
        && environment
            .get("XDG_RUNTIME_DIR")
            .is_none_or(|value| !std::path::Path::new(value).is_absolute())
    {
        return Ok(None);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    runtime.block_on(async {
        let mut client=match client::connect().await {
            Ok(client)=>client,
            Err(ConnectError::Absent) if !explicit=>return Ok(None),
            Err(error)=>return Err(error.into()),
        };
        if !client.methods.contains_key("niri.widths.configure") {
            let status=client.call("daemon.status",json!({}),None).await?;
            if !explicit && status["safe"]==false && status["services"].get("niri-widths").is_none() {
                return Ok(None);
            }
            if status["safe"]!=true {return Err("Daemon width observer is unavailable".into());}
        }
        let params=WidthsConfigure {
            connection_id:super::niri_events::identity(std::path::Path::new(environment.get("NIRI_SOCKET").ok_or("Missing Niri socket")?))?,
            display:environment.get("DISPLAY").cloned().ok_or("Missing X11 display")?,
            opening_width,
        };
        params.validate()?;
        let mut terminate=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
        let mut interrupt=tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())?;
        let mut retry=50;
        loop {
            let result=async {
                let topic=if client.methods.contains_key("niri.widths.configure") {
                    client.call("niri.widths.configure",serde_json::to_value(&params)?,Some(2000)).await?;
                    TOPIC
                }else {"daemon"};
                // The compatibility observer needs no method schemas after its
                // idempotent configuration call, nor any UI payload buffers.
                client.methods.clear();
                let mut mirror=Mirror::default();
                mirror.receive(&client.welcome);
                client.welcome=Message::Pong{id:Id::Integer(0)};
                #[cfg(target_env = "gnu")]
                #[allow(unsafe_code)]
                // SAFETY: no pointers are passed; this releases discarded
                // handshake schemas once, without an idle timer.
                unsafe { nix::libc::malloc_trim(0); }
                framing::write(&mut client.writer,&Message::Sub{topics:vec![topic.into()]}).await?;
                loop {
                    tokio::select! {
                        _=terminate.recv()=>return Ok::<i32,crate::Error>(0),
                        _=interrupt.recv()=>return Ok(0),
                        message=client.reader.next()=>{
                            let message=message?.ok_or("Daemon width observer disconnected")?;
                            if let Message::Error{err}=&message {return Err(err.clone().into());}
                            if let Message::Ping{id}=&message {framing::write(&mut client.writer,&Message::Pong{id:id.clone()}).await?;}
                            if let Update::Resync(topic)=mirror.receive(&message) {
                                framing::write(&mut client.writer,&Message::Resync{topics:vec![topic]}).await?;
                            }
                            if matches!(message,Message::Snap{..}) {retry=50;}
                        },
                    }
                }
            }.await;
            match result {
                Ok(code)=>return Ok(Some(code)),
                Err(error) if error.downcast_ref::<Failure>().is_some_and(|e|!e.retry)=>return Err(error),
                Err(_)=>eprintln!("nacre-helper: width observer disconnected; reconnecting"),
            }
            loop {
                tokio::select! {
                    _=terminate.recv()=>return Ok(Some(0)),
                    _=interrupt.recv()=>return Ok(Some(0)),
                    _=tokio::time::sleep(Duration::from_millis(retry))=>{},
                }
                retry=(retry*2).min(2000);
                match client::connect().await {
                    Ok(connected)=>{client=connected;break;},
                    Err(ConnectError::Absent)=>{},
                    Err(ConnectError::Failed(error)) if error.downcast_ref::<Failure>().is_some_and(|e|e.retry)=>{},
                    Err(error)=>return Err(error.into()),
                }
            }
        }
    })
}

#[derive(Clone, Default)]
pub struct Shutdown {
    stop: Rc<Cell<bool>>,
    changed: Rc<Notify>,
    restored: Rc<Notify>,
}
impl Shutdown {
    pub async fn finish(&self) -> Result<()> {
        let restored = self.restored.notified();
        self.stop.set(true);
        self.changed.notify_waiters();
        tokio::time::timeout(Duration::from_secs(4), restored)
            .await
            .map_err(|_| "Width observer shutdown timed out")?;
        Ok(())
    }
}
struct State {
    x11: x11::Native,
    policy: Policy,
    started: Instant,
}
impl State {
    fn resync(&mut self, windows: Value) -> Result<()> {
        let present = windows
            .as_object()
            .ok_or("Invalid width observer source")?
            .values()
            .map(|row| row["id"].as_u64().ok_or("Invalid width observer window ID"))
            .collect::<std::result::Result<std::collections::BTreeSet<_>, _>>()?;
        let closed = self
            .policy
            .windows
            .keys()
            .filter(|id| !present.contains(id))
            .copied()
            .collect::<Vec<_>>();
        for id in closed {
            self.policy.event(&json!({"WindowClosed":{"id":id}}))?;
        }
        self.policy.event(&windows_event(windows)?)?;
        self.policy
            .reconcile(&mut self.x11, self.started.elapsed().as_secs_f64())
    }
}
struct Driver {
    state: Arc<Mutex<State>>,
    io: AsyncFd<OwnedFd>,
}
#[allow(unsafe_code)]
fn observe_fd(fd: OwnedFd) -> Result<AsyncFd<OwnedFd>> {
    // SAFETY: AsyncFd owns this duplicate until deregistration. Its descriptor
    // never changes; shutdown interrupts I/O without closing or replacing it.
    Ok(
        unsafe { AsyncFd::register_with_interest(fd, tokio::io::Interest::READABLE) }
            .map_err(|_| "Cannot register X11 observer events")?,
    )
}
impl Driver {
    async fn connect() -> Result<Self> {
        let state = tokio::task::spawn_blocking(|| {
            let x11 = x11::Native::connect()?;
            let fd = x11.connection.stream().as_fd().try_clone_to_owned()?;
            Ok::<_, crate::Error>((
                State {
                    x11,
                    policy: Policy::default(),
                    started: Instant::now(),
                },
                fd,
            ))
        })
        .await??;
        Ok(Self {
            state: Arc::new(Mutex::new(state.0)),
            io: observe_fd(state.1)?,
        })
    }
    fn interrupt(&self) {
        match nix::sys::socket::shutdown(
            self.io.get_ref().as_raw_fd(),
            nix::sys::socket::Shutdown::Both,
        ) {
            Ok(()) | Err(nix::errno::Errno::ENOTCONN) => {}
            Err(_) => tracing::warn!("X11 observer connection shutdown failed"),
        }
    }
    async fn with<T: Send + 'static>(
        &self,
        operation: impl FnOnce(&mut State) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let state = self.state.clone();
        let mut task = tokio::task::spawn_blocking(move || {
            let mut state = state
                .lock()
                .map_err(|_| "Width observer state is unavailable")?;
            operation(&mut state)
        });
        match tokio::time::timeout(Duration::from_secs(2), &mut task).await {
            Ok(result) => result?,
            Err(_) => {
                // Closing our socket interrupts a synchronous X11 read, so a
                // stalled server cannot permanently occupy the blocking pool.
                self.interrupt();
                if tokio::time::timeout(Duration::from_secs(2), task)
                    .await
                    .is_err()
                {
                    tracing::error!("X11 observer operation did not stop");
                }
                Err("X11 observer operation timed out".into())
            }
        }
    }
}
impl Drop for Driver {
    fn drop(&mut self) {
        self.interrupt();
    }
}

struct Registered(Shared);
impl Drop for Registered {
    fn drop(&mut self) {
        let mut hub = self.0.borrow_mut();
        hub.widths = None;
        let mut status = hub
            .snapshot(TOPIC)
            .unwrap_or(json!({"opening_width":50.0,"relaxed":0}));
        status["available"] = json!(false);
        if hub.publish(TOPIC, status).is_err() {
            tracing::warn!("Cannot publish width observer disconnect");
        }
    }
}

fn native_windows(hub: &Shared) -> Result<Option<(u64, Value)>> {
    let hub = hub.borrow();
    if hub
        .snapshot_ref("compositor")
        .is_some_and(|v| v["available"] == true && v["backend"] == "niri")
    {
        Ok(hub.niri_width_source.current())
    } else {
        Ok(None)
    }
}
fn connected(hub: &Shared, identity: Option<&Value>) -> bool {
    let hub = hub.borrow();
    hub.snapshot_ref("compositor")
        .is_some_and(|v| v["available"] == true && v["backend"] == "niri")
        && hub.niri_width_source.snapshot.is_some()
        && identity.is_none_or(|id| {
            hub.snapshot_ref(super::niri_events::TOPIC)
                .is_some_and(|v| v["connectionId"] == *id)
        })
}
fn windows_event(windows: Value) -> Result<Value> {
    let rows = windows
        .as_object()
        .ok_or("Invalid width observer source")?
        .values()
        .cloned()
        .collect::<Vec<_>>();
    Ok(json!({"WindowsChanged":{"windows":rows}}))
}
pub async fn run(
    hub: Shared,
    control: Control,
    environment: BTreeMap<String, String>,
    opening: Rc<Cell<f64>>,
    shutdown: Shutdown,
) -> Result<()> {
    let (send, mut requests) = mpsc::channel(16);
    hub.borrow_mut().widths = Some(Requests(send));
    let _registered = Registered(hub.clone());
    let mut changes = hub.borrow().observe();
    let mut window_events = hub.borrow().niri_width_source.events.subscribe();
    loop {
        if shutdown.stop.get() {
            shutdown.restored.notify_one();
            std::future::pending::<()>().await;
        }
        if connected(&hub, None) {
            break;
        }
        tokio::select! {_=shutdown.changed.notified()=>{},change=changes.recv()=>{change.map_err(|_| "Compositor state observer ended")?;}}
    }
    let driver = Driver::connect().await?;
    // Windows present when X11 connects are initial state, never newly opened.
    let (mut sequence, initial) =
        native_windows(&hub)?.ok_or("Width observer compositor disconnected")?;
    let connection = hub
        .borrow()
        .snapshot(super::niri_events::TOPIC)
        .ok_or("Missing compositor identity")?["connectionId"]
        .clone();
    let result=async {
        driver.with(move|state| {
            state.resync(initial)
        }).await?;
        hub.borrow_mut().service("niri-widths","running",0)?;
        loop {
            if shutdown.stop.get() {break;}
            if !connected(&hub,Some(&connection)) {
                return Err("Width observer compositor connection changed".into());
            }
            // Reply handling can buffer X11 events even without descriptor
            // readiness. Drain those before sleeping on the descriptor.
            let (relaxed, deadline, actions)=driver.with(|state| {
                if state.x11.drain()? {state.policy.reconcile(&mut state.x11,state.started.elapsed().as_secs_f64())?;}
                let mut actions=Vec::new();
                state.policy.apply_openings(state.started.elapsed().as_secs_f64(),|id| {actions.push(id);Ok(())})?;
                let deadline=state.policy.opening.values().copied().reduce(f64::min).map(|at| state.started+Duration::from_secs_f64(at));
                Ok((state.policy.saved.len() as u32,deadline,actions))
            }).await?;
            for id in actions {
                let width=opening.get();
                if control.with(move |backend| {
                    use crate::compositor::{backend::Backend,niri_actions::Transport};
                    let Backend::Niri(backend)=backend else {return Err("Width observer compositor changed".into());};
                    Transport::request(&mut backend.actions.transport,serde_json::to_value(niri_ipc::Request::Action(niri_ipc::Action::SetWindowWidth{id:Some(id),change:niri_ipc::SizeChange::SetProportion(width)}))?,2.0,niri_widths::MAX_BYTES)?;
                    Ok(())
                }).await.is_err() {tracing::warn!("Opening-width action failed");}
            }
            let status=serde_json::to_value(WidthsStatus{available:true,opening_width:opening.get(),relaxed})?;
            if hub.borrow().snapshot(TOPIC).as_ref()!=Some(&status) {
                hub.borrow_mut().publish(TOPIC,status.clone())?;
            }
            let timer=async {if let Some(at)=deadline {tokio::time::sleep_until(at.into()).await;} else {std::future::pending::<()>().await;}};
            tokio::select! {
                _=shutdown.changed.notified()=>{},
                _=timer=>{},
                ready=driver.io.readable()=>{ready?.clear_ready();},
                request=requests.recv()=>{
                    let Some(request)=request else {return Err("Width observer request channel ended".into());};
                    if request.reply.is_closed(){continue;}
                    let metadata=hub.borrow().snapshot(super::niri_events::TOPIC).unwrap_or(Value::Null);
                    let answer=if request.params.display!=environment.get("DISPLAY").map(String::as_str).unwrap_or("") || metadata["connectionId"]!=request.params.connection_id {
                        Err(failure(ErrorCode::Validation,"Width observer session does not match the daemon",false))
                    }else {opening.set(request.params.opening_width);Ok(json!(WidthsStatus{available:true,opening_width:opening.get(),relaxed}))};
                    if request.reply.send(answer).is_err(){tracing::debug!("Width observer caller ended");}
                },
                event=window_events.recv()=>{
                    match event {
                        Ok((received,event)) if received==sequence+1=>{
                            sequence=received;
                            driver.with(move|state| {
                                if state.policy.event(&event)? {state.policy.reconcile(&mut state.x11,state.started.elapsed().as_secs_f64())?;}
                                Ok(())
                            }).await?;
                        },
                        Ok((received,_)) if received<=sequence=>continue,
                        Err(tokio::sync::broadcast::error::RecvError::Closed)=>return Err("Width event source ended".into()),
                        _=>{
                            tracing::warn!("Width observer resynchronizing after an event gap");
                            let (current,windows)=native_windows(&hub)?.ok_or("Width observer compositor disconnected")?;
                            sequence=current;
                            driver.with(move|state|state.resync(windows)).await?;
                        },
                    }
                },
                change=changes.recv()=>{
                    match change {
                        Ok(Notice::Changed{topic,..}) if topic=="compositor"=>{},
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{tracing::warn!("Width observer resynchronizing compositor state");},
                        Err(tokio::sync::broadcast::error::RecvError::Closed)=>return Err("Compositor state observer ended".into()),
                        _=>continue,
                    }
                    if !connected(&hub,Some(&connection)) {return Err("Width observer compositor disconnected".into());}
                },
            }
        }
        Ok::<(),crate::Error>(())
    }.await;
    let restored = driver
        .with(|state| {
            let windows = state.policy.saved.keys().copied().collect::<Vec<_>>();
            for window in windows {
                state.policy.restore(window, &mut state.x11)?;
            }
            Ok(())
        })
        .await;
    if let Err(error) = restored {
        tracing::warn!("Width observer hint restoration failed");
        if result.is_ok() {
            return Err(error);
        }
    }
    if shutdown.stop.get() {
        shutdown.restored.notify_one();
        std::future::pending::<()>().await;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::niri_widths::{Identity, SizeHints, MAX_SIZE, MIN_SIZE, X11};
    use std::collections::BTreeSet;
    #[derive(Clone)]
    struct Fixture(SizeHints);
    impl X11 for Fixture {
        fn windows(&mut self) -> Result<Vec<u32>> {
            Ok(vec![10])
        }
        fn identity(&mut self, _: u32) -> Result<Option<Identity>> {
            Ok(Some((BTreeSet::from(["fixture".into()]), "Window".into())))
        }
        fn ordinary(&mut self, _: u32) -> Result<bool> {
            Ok(true)
        }
        fn hints(&mut self, _: u32) -> Result<Option<SizeHints>> {
            Ok(Some(self.0.clone()))
        }
        fn set_hints(&mut self, _: u32, hints: &SizeHints) -> Result<()> {
            self.0 = hints.clone();
            Ok(())
        }
    }
    #[test]
    fn internal_tap_preserves_unknown_values_opening_and_close_effects() -> Result<()> {
        let mut source = Source::default();
        let mut events = source.events.subscribe();
        let mut raw = Policy::default();
        let mut projected = Policy::default();
        let hints = SizeHints {
            flags: MIN_SIZE | MAX_SIZE,
            min_width: 1600,
            min_height: 600,
            max_width: 4000,
            max_height: 2000,
            width_inc: 10,
            height_inc: 20,
            ..SizeHints::default()
        };
        let mut old = Fixture(hints.clone());
        let mut new = Fixture(hints.clone());
        raw.reconcile(&mut old, 100.0)?;
        projected.reconcile(&mut new, 100.0)?;
        for event in [
            json!({"WindowsChanged":{"windows":[]}}),
            json!({"WindowOpenedOrChanged":{"window":{"id":7,"app_id":"fixture","title":null,"is_floating":null,"unused":"not policy data"}}}),
            json!({"WindowOpenedOrChanged":{"window":{"id":7,"app_id":"fixture","title":"Window","is_floating":false}}}),
            json!({"WindowClosed":{"id":7}}),
        ] {
            let old_result = raw.event(&event)?;
            let windows = raw
                .windows
                .iter()
                .map(|(id, row)| (id.to_string(), row.clone()))
                .collect();
            source.update(&windows, &event)?;
            let (_, event) = events.try_recv()?;
            assert_eq!(projected.event(&event)?, old_result);
            raw.reconcile(&mut old, 100.0)?;
            projected.reconcile(&mut new, 100.0)?;
            assert_eq!(serde_json::to_value(&old.0)?, serde_json::to_value(&new.0)?);
            assert_eq!(raw.saved, projected.saved);
            assert_eq!(raw.new, projected.new);
            assert_eq!(raw.opening, projected.opening);
            let mut before = Vec::new();
            let mut after = Vec::new();
            raw.apply_openings(100.149, |id| {
                before.push(id);
                Ok(())
            })?;
            projected.apply_openings(100.149, |id| {
                after.push(id);
                Ok(())
            })?;
            assert_eq!(before, after);
        }
        assert_eq!(old.0.min_width, hints.min_width);
        assert!(projected.opening.is_empty() && projected.new.is_empty());
        Ok(())
    }
    #[test]
    fn raw_tap_is_bounded_and_retains_latest_resync_snapshot() -> Result<()> {
        let mut source = Source::default();
        let mut events = source.events.subscribe();
        for id in 0..300 {
            let row = json!({"id":id,"app_id":"fixture","title":null,"is_floating":null});
            source.update(
                &serde_json::Map::from_iter([(id.to_string(), row.clone())]),
                &json!({"WindowOpenedOrChanged":{"window":row}}),
            )?;
        }
        assert!(matches!(
            events.try_recv(),
            Err(tokio::sync::broadcast::error::TryRecvError::Lagged(_))
        ));
        let (seq, latest) = source.current().ok_or("Missing source")?;
        assert_eq!(seq, 300);
        assert_eq!(latest["299"]["is_floating"], Value::Null);
        assert_eq!(latest.as_object().unwrap().len(), 1);
        Ok(())
    }

    #[test]
    fn private_x11_hint_restore_and_one_shot() -> Result<()> {
        use x11rb::{
            connection::Connection,
            protocol::xproto::{AtomEnum, ConnectionExt, CreateWindowAux, PropMode, WindowClass},
            wrapper::ConnectionExt as _,
        };
        if std::env::var_os("NACRE_WIDTHS_XVFB_CHILD").is_none() {
            let output = match std::process::Command::new("xvfb-run")
                .args(["-a"])
                .arg(std::env::current_exe()?)
                .args([
                    "--exact",
                    "daemon::widths::tests::private_x11_hint_restore_and_one_shot",
                    "--nocapture",
                ])
                .env_remove("DISPLAY")
                .env_remove("WAYLAND_DISPLAY")
                .env("NACRE_WIDTHS_XVFB_CHILD", "1")
                .output()
            {
                Ok(output) => output,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                    eprintln!(
                        "private X11 widths check skipped: xvfb-run is unavailable; CI installs it"
                    );
                    return Ok(());
                }
                Err(error) => return Err(error.into()),
            };
            assert!(
                output.status.success(),
                "private X11 check failed: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            return Ok(());
        }
        let authority = std::env::var("XAUTHORITY")?;
        assert!(
            authority.starts_with("/tmp/xvfb-run."),
            "Refusing a non-lab X11 display"
        );
        let mut x11 = x11::Native::connect()?;
        let screen = &x11.connection.setup().roots[0];
        let root = screen.root;
        let window = x11.connection.generate_id()?;
        x11.connection
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
                root,
                0,
                0,
                400,
                300,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new(),
            )?
            .check()?;
        x11.connection
            .change_property8(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_CLASS,
                AtomEnum::STRING,
                b"fixture\0Fixture\0",
            )?
            .check()?;
        x11.connection
            .change_property8(
                PropMode::REPLACE,
                window,
                AtomEnum::WM_NAME,
                AtomEnum::STRING,
                b"Window",
            )?
            .check()?;
        let hints = SizeHints {
            flags: MIN_SIZE | MAX_SIZE,
            min_width: 1600,
            min_height: 600,
            max_width: 4000,
            max_height: 2000,
            width_inc: 10,
            height_inc: 20,
            ..SizeHints::default()
        };
        x11.set_hints(window, &hints)?;
        x11.connection.map_window(window)?.check()?;
        x11.connection.flush()?;
        let folder = tempfile::tempdir()?;
        let socket = folder.path().join("niri.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket)?;
        listener.set_nonblocking(true)?;
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .max_blocking_threads(2)
            .build()?;
        tokio::task::LocalSet::new().block_on(&runtime, async {
            use tokio::io::{AsyncBufReadExt, AsyncWriteExt};
            let listener = tokio::net::UnixListener::from_std(listener)?;
            let (send, mut actions) = mpsc::channel(8);
            let server = tokio::task::spawn_local(async move {
                let (stream, _) = listener.accept().await?;
                let (read, mut write) = stream.into_split();
                let mut lines = tokio::io::BufReader::new(read).lines();
                while let Some(line) = lines.next_line().await? {
                    send.send((serde_json::from_str::<Value>(&line)?, Instant::now()))
                        .await
                        .map_err(|_| "Action observer ended")?;
                    write.write_all(b"{\"Ok\":\"Handled\"}\n").await?;
                }
                Ok::<(), crate::Error>(())
            });
            let hub =
                super::super::hub::Hub::new("fixture".into(), false, Duration::from_millis(16))
                    .shared();
            hub.borrow_mut()
                .publish("compositor", json!({"available":true,"backend":"niri"}))?;
            hub.borrow_mut().publish(
                super::super::niri_events::TOPIC,
                json!({"available":true,"backend":"niri","connectionId":"fixture"}),
            )?;
            hub.borrow_mut().niri_width_source.update(
                &serde_json::Map::new(),
                &json!({"WindowsChanged":{"windows":[]}}),
            )?;
            let environment = BTreeMap::from([
                ("NACRE_COMPOSITOR".into(), "niri".into()),
                ("NIRI_SOCKET".into(), socket.to_string_lossy().into_owned()),
                ("DISPLAY".into(), std::env::var("DISPLAY")?),
            ]);
            let control =
                Control::new(crate::paths::Paths::new(folder.path()), environment.clone());
            let stop = Shutdown::default();
            let stopped = stop.clone();
            let state = hub.clone();
            let env = environment.clone();
            let opening = Rc::new(Cell::new(50.0));
            let configured = opening.clone();
            let service = tokio::task::spawn_local(async move {
                run(state, control, env, configured, stopped).await
            });
            tokio::time::timeout(Duration::from_secs(3), async {
                while !hub
                    .borrow()
                    .snapshot(TOPIC)
                    .is_some_and(|v| v["available"] == true)
                {
                    assert!(
                        !service.is_finished(),
                        "Width service ended before readiness"
                    );
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
            })
            .await?;
            let params = WidthsConfigure {
                connection_id: "wrong-session".into(),
                display: environment["DISPLAY"].clone(),
                opening_width: 52.5,
            };
            assert!(dispatch(hub.clone(), serde_json::to_value(&params)?)
                .await
                .is_err());
            assert_eq!(opening.get(), 50.0);
            let params = WidthsConfigure {
                connection_id: "fixture".into(),
                ..params
            };
            dispatch(hub.clone(), serde_json::to_value(params)?).await?;
            assert_eq!(opening.get(), 52.5);
            assert_eq!(x11.hints(window)?.unwrap().min_width, 1600);
            let started = Instant::now();
            let row = json!({"id":7,"app_id":"fixture","title":"Window","is_floating":false});
            hub.borrow_mut().niri_width_source.update(
                &serde_json::Map::from_iter([("7".into(), row.clone())]),
                &json!({"WindowOpenedOrChanged":{"window":row}}),
            )?;
            let (action, at) = tokio::time::timeout(Duration::from_secs(3), actions.recv())
                .await?
                .ok_or("Missing opening action")?;
            assert!(
                at.duration_since(started) >= Duration::from_millis(140),
                "Opening correction fired before its one-shot deadline"
            );
            assert_eq!(
                action,
                json!({"Action":{"SetWindowWidth":{"id":7,"change":{"SetProportion":52.5}}}})
            );
            let current = x11.hints(window)?.unwrap();
            assert_eq!(current.min_width, 1);
            assert_eq!(current.min_height, 600);
            assert_eq!(current.max_height, 2000);
            tokio::time::sleep(Duration::from_millis(200)).await;
            assert!(actions.try_recv().is_err(), "Opening correction repeated");
            let mut updated = current;
            updated.max_width = 1200;
            x11.set_hints(window, &updated)?;
            stop.finish().await?;
            service.abort();
            let _cancelled = service.await;
            let restored = x11.hints(window)?.unwrap();
            assert_eq!(restored.min_width, 1200);
            assert_eq!(restored.min_height, 600);
            assert_eq!(restored.max_width, 1200);
            assert_eq!(restored.max_height, 2000);
            server.abort();
            let _cancelled = server.await;
            Ok::<(), crate::Error>(())
        })?;
        x11.connection.destroy_window(window)?.check()?;
        Ok(())
    }
}
