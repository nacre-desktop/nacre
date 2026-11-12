//! Daemon-owned focus timer. State persists in the legacy `dashboard-focus.json`
//! shape so the helper `dashboard-data` reader keeps working. Do Not Disturb is
//! changed only through the settings writer; the notifications producer observes
//! that store, so the two never write one another.
use super::{
    super::hub::{failure, Notice, Shared},
    super::notifications,
    watch::Watch,
};
use crate::{paths::Paths, Result};
use nacre_protocol::{
    core_services::Health,
    dashboard_extras::{DashboardExtrasCall, DashboardFocus, FocusStatus, FOCUS_MAX_MINUTES},
    ErrorCode, Failure,
};
use rustix::fs::inotify::WatchFlags;
use serde_json::{json, Value};
use std::{cell::RefCell, rc::Rc, time::Duration};
use tokio::sync::{Mutex, Notify};

pub const TOPIC: &str = "dashboard.focus";
const FILE: &str = "dashboard-focus.json";
/// Only sessions this daemon started are completed by its timer. A session the
/// legacy helper wrote is mirrored and may be controlled but not auto-completed,
/// so QML and the daemon never both notify.
const OWNER: &str = "nacre-daemon";
const MAX_MS: u64 = FOCUS_MAX_MINUTES as u64 * 60_000;
/// Wall-clock recheck while a session runs, so suspend cannot oversleep it.
const TICK: Duration = Duration::from_secs(30);

#[derive(Clone, Debug, PartialEq)]
struct State {
    status: FocusStatus,
    total_ms: u64,
    deadline_ms: u64,
    remaining_ms: u64,
    previous_dnd: bool,
    owns_dnd: bool,
    daemon: bool,
}
impl State {
    fn idle() -> Self {
        Self {
            status: FocusStatus::Idle,
            total_ms: 0,
            deadline_ms: 0,
            remaining_ms: 0,
            previous_dnd: false,
            owns_dnd: false,
            daemon: false,
        }
    }
    fn live(&self) -> bool {
        matches!(self.status, FocusStatus::Running | FocusStatus::Paused)
    }
    fn parse(value: &Value) -> Option<Self> {
        let ms = |key: &str| -> Option<u64> {
            match value.get(key) {
                None | Some(Value::Null) => Some(0),
                Some(v) => v
                    .as_f64()
                    .filter(|n| n.is_finite() && *n >= 0.0)
                    .map(|n| ((n * 1000.0).round() as u64).min(u64::MAX / 2)),
            }
        };
        let flag = |key: &str| match value.get(key) {
            None | Some(Value::Null) => Some(false),
            Some(v) => v.as_bool(),
        };
        let status = match value
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("idle")
        {
            "idle" | "" => FocusStatus::Idle,
            "running" => FocusStatus::Running,
            "paused" => FocusStatus::Paused,
            "done" => FocusStatus::Done,
            _ => return None,
        };
        let state = Self {
            status,
            total_ms: ms("total")?,
            // The legacy file stores the deadline as Unix seconds.
            deadline_ms: ms("deadline")?,
            remaining_ms: ms("remaining")?,
            previous_dnd: flag("previousDnd")?,
            owns_dnd: flag("ownsDnd")?,
            daemon: value.get("owner").and_then(Value::as_str) == Some(OWNER),
        };
        let sane = state.total_ms <= MAX_MS
            && state.remaining_ms <= MAX_MS
            && (!state.live() || state.total_ms > 0)
            && (state.status != FocusStatus::Running || state.deadline_ms > 0);
        sane.then_some(state)
    }
    fn file(&self) -> Value {
        let status = match self.status {
            FocusStatus::Idle => "idle",
            FocusStatus::Running => "running",
            FocusStatus::Paused => "paused",
            FocusStatus::Done => "done",
        };
        let mut value = json!({"status":status,"total":self.total_ms as f64 / 1000.0,
            "remaining":self.remaining_ms as f64 / 1000.0,"deadline":self.deadline_ms as f64 / 1000.0,
            "previousDnd":self.previous_dnd,"ownsDnd":self.owns_dnd});
        if self.daemon {
            value["owner"] = json!(OWNER);
        }
        value
    }
    fn topic(&self, health: Health) -> DashboardFocus {
        let running = self.status == FocusStatus::Running;
        DashboardFocus {
            health,
            status: self.status,
            total_s: self.total_ms.div_ceil(1000) as u32,
            deadline_ms: running.then_some(self.deadline_ms),
            remaining_s: (self.status == FocusStatus::Paused)
                .then(|| self.remaining_ms.div_ceil(1000) as u32),
            owns_dnd: self.owns_dnd && self.live(),
            previous_dnd: self.previous_dnd,
        }
    }
}

fn wall_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}
fn unavailable() -> Failure {
    failure(ErrorCode::Unavailable, "Focus state is unavailable", true)
}

pub struct Focus {
    paths: Paths,
    state: RefCell<State>,
    error: RefCell<Option<Failure>>,
    /// Serializes every transition, including its Do Not Disturb change.
    gate: Mutex<()>,
    wake: Notify,
}

impl Focus {
    pub fn new(paths: Paths) -> Rc<Self> {
        Rc::new(Self {
            paths,
            state: RefCell::new(State::idle()),
            error: RefCell::new(None),
            gate: Mutex::new(()),
            wake: Notify::new(),
        })
    }
    fn path(&self) -> std::path::PathBuf {
        self.paths.state.join(FILE)
    }
    fn health(&self) -> Health {
        let error = self.error.borrow().clone();
        Health {
            available: error.is_none(),
            stale: error.is_some(),
            error,
        }
    }
    pub fn publish(&self, hub: &Shared) -> std::result::Result<(), Failure> {
        let topic = self.state.borrow().topic(self.health());
        topic.validate()?;
        hub.borrow_mut().publish(
            TOPIC,
            serde_json::to_value(topic)
                .map_err(|_| failure(ErrorCode::Internal, "Focus encoding failed", false))?,
        )
    }
    /// Read the file; keep the last good state and report unavailable on damage.
    fn reload(&self, hub: &Shared) {
        let read = crate::state::read_strict(&self.path(), json!({}))
            .ok()
            .and_then(|v| State::parse(&v));
        match read {
            Some(state) => {
                *self.error.borrow_mut() = None;
                *self.state.borrow_mut() = state;
            }
            None => *self.error.borrow_mut() = Some(unavailable()),
        }
        if self.publish(hub).is_err() {
            tracing::error!("Cannot publish focus state");
        }
        self.wake.notify_one();
    }
    fn save(&self, state: &State) -> std::result::Result<(), Failure> {
        crate::files::json_write(&self.path(), &state.file())
            .map_err(|_| failure(ErrorCode::Io, "Focus state could not be saved", true))
    }
    fn commit(&self, hub: &Shared, state: State) -> std::result::Result<(), Failure> {
        self.save(&state)?;
        *self.state.borrow_mut() = state;
        *self.error.borrow_mut() = None;
        self.publish(hub)?;
        self.wake.notify_one();
        Ok(())
    }

    pub async fn call(
        &self,
        hub: &Shared,
        call: DashboardExtrasCall,
    ) -> std::result::Result<Value, Failure> {
        let _turn = self.gate.lock().await;
        self.reload(hub);
        if self.error.borrow().is_some() {
            return Err(unavailable());
        }
        let current = self.state.borrow().clone();
        let changed = match call {
            DashboardExtrasCall::FocusStart(p) => {
                if current.live() {
                    return Err(failure(
                        ErrorCode::InvalidRequest,
                        "A focus session is already active",
                        false,
                    ));
                }
                let previous = notifications::dnd(hub).await?;
                let total_ms = u64::from(p.minutes) * 60_000;
                let started = State {
                    status: FocusStatus::Running,
                    total_ms,
                    deadline_ms: wall_ms() + total_ms,
                    remaining_ms: total_ms,
                    previous_dnd: previous,
                    owns_dnd: !previous,
                    daemon: true,
                };
                self.commit(hub, started)?;
                if !previous {
                    if let Err(error) = notifications::set_dnd_inner(hub, true).await {
                        // Never leave a session that promised silence running without it.
                        self.commit(hub, State::idle())?;
                        return Err(error);
                    }
                }
                true
            }
            DashboardExtrasCall::FocusPause(_) if current.status == FocusStatus::Running => {
                let mut next = current;
                next.status = FocusStatus::Paused;
                next.remaining_ms = next.deadline_ms.saturating_sub(wall_ms());
                next.deadline_ms = 0;
                self.commit(hub, next)?;
                true
            }
            DashboardExtrasCall::FocusResume(_) if current.status == FocusStatus::Paused => {
                let mut next = current;
                next.status = FocusStatus::Running;
                next.deadline_ms = wall_ms() + next.remaining_ms;
                next.remaining_ms = 0;
                self.commit(hub, next)?;
                true
            }
            DashboardExtrasCall::FocusStop(_) if current.live() => {
                self.finish(hub, current, FocusStatus::Idle, false).await?;
                true
            }
            DashboardExtrasCall::FocusPause(_)
            | DashboardExtrasCall::FocusResume(_)
            | DashboardExtrasCall::FocusStop(_) => false,
            _ => return Err(failure(ErrorCode::UnknownMethod, "Unknown method", false)),
        };
        Ok(json!({"changed":changed}))
    }

    /// End a live session and restore Do Not Disturb only if this session turned it
    /// on and it is still on. Preserve the claim through the first restore failure;
    /// expiry may abandon restoration after a failed readiness-driven retry.
    async fn finish(
        &self,
        hub: &Shared,
        current: State,
        status: FocusStatus,
        abandon_restore: bool,
    ) -> std::result::Result<(), Failure> {
        let restore = async {
            if current.owns_dnd && notifications::dnd(hub).await? {
                notifications::set_dnd_inner(hub, current.previous_dnd).await?;
            }
            Ok::<(), Failure>(())
        }
        .await;
        if let Err(error) = restore {
            if !abandon_restore {
                return Err(error);
            }
            tracing::warn!(
                ?error,
                "Focus completed without restoring Do Not Disturb after retry"
            );
        }
        let mut next = current;
        next.status = status;
        if status == FocusStatus::Idle {
            next.total_ms = 0;
        }
        next.remaining_ms = 0;
        next.deadline_ms = 0;
        next.owns_dnd = false;
        self.commit(hub, next)
    }

    /// Return whether completion must wait for a settings/readiness change.
    async fn expired(&self, hub: &Shared, abandon_restore: bool) -> bool {
        let _turn = self.gate.lock().await;
        self.reload(hub);
        let current = self.state.borrow().clone();
        if current.status != FocusStatus::Running
            || !current.daemon
            || current.deadline_ms > wall_ms()
        {
            return false;
        }
        if let Err(error) = self
            .finish(hub, current, FocusStatus::Done, abandon_restore)
            .await
        {
            tracing::warn!(?error, "Focus completion deferred until settings recover");
            return true;
        }
        notify_complete().await;
        false
    }

    /// The user turning DND off ends our claim on it, as QML's focus-release did.
    async fn dnd_changed(&self, hub: &Shared) {
        let _turn = self.gate.lock().await;
        let current = self.state.borrow().clone();
        if current.live() && current.owns_dnd && matches!(notifications::dnd(hub).await, Ok(false))
        {
            let mut next = current;
            next.owns_dnd = false;
            if self.commit(hub, next).is_err() {
                tracing::warn!("Focus Do Not Disturb release could not be saved");
            }
        }
    }

    pub async fn run(self: Rc<Self>, hub: Shared) -> Result<()> {
        {
            let _turn = self.gate.lock().await;
            self.reload(&hub);
        }
        let mut notices = hub.borrow().observe();
        let mut watch = Watch::new(vec![(
            self.paths.state.clone(),
            WatchFlags::CLOSE_WRITE | WatchFlags::MOVED_TO,
        )])?;
        let mut deferred = false;
        loop {
            let (running, daemon, deadline) = {
                let state = self.state.borrow();
                (
                    state.status == FocusStatus::Running,
                    state.daemon,
                    state.deadline_ms,
                )
            };
            let sleep = async {
                if running && daemon && !deferred {
                    let left = Duration::from_millis(deadline.saturating_sub(wall_ms()));
                    tokio::time::sleep(left.min(TICK)).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };
            tokio::select! {
                _ = sleep => {
                    deferred = self.expired(&hub, false).await;
                    // Readiness or terminal failure may predate expiry; no new
                    // event is needed to drive the final retry in a settled service.
                    let settled = hub.borrow().services.get("settings")
                        .is_some_and(|s| s["status"] == "running" || s["status"] == "disabled");
                    if deferred && settled {
                        deferred = self.expired(&hub, true).await;
                    }
                },
                _ = self.wake.notified() => {},
                names = watch.next() => {
                    if names?.iter().any(|n| n == FILE) {
                        let _turn = self.gate.lock().await;
                        self.reload(&hub);
                        deferred = false;
                    }
                },
                notice = notices.recv() => {
                    match notice {
                        Ok(Notice::Changed { topic, .. }) if topic == "settings.store.desktop" || topic == "daemon" => {},
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {},
                        Ok(_) => continue,
                        Err(_) => return Err("Daemon notices ended".into()),
                    }
                    self.dnd_changed(&hub).await;
                    // Other daemon changes must not spend the retry while settings
                    // is still starting or recovering through its supervisor.
                    let settled = hub.borrow().services.get("settings")
                        .is_some_and(|s| s["status"] == "running" || s["status"] == "disabled");
                    if deferred && settled {
                        deferred = self.expired(&hub, true).await;
                    }
                },
            }
        }
    }
}

async fn notify_complete() {
    let mut command = tokio::process::Command::new("notify-send");
    command
        .args([
            "--app-name=Nacre Focus",
            "--icon=nacre-notifications",
            &crate::i18n::tr("focus.complete.title", "Focus complete"),
            &crate::i18n::tr("focus.complete.body", "Take a moment to rest."),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    if let Ok(mut child) = command.spawn() {
        let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::{compositor_control, hub::Hub, settings};

    #[tokio::test(start_paused = true)]
    async fn expired_session_retains_dnd_until_settings_are_ready() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let home = tempfile::tempdir().unwrap();
                let paths = Paths::new(home.path());
                std::fs::create_dir_all(&paths.config).unwrap();
                std::fs::create_dir_all(&paths.state).unwrap();
                std::fs::write(
                    paths.config.join("desktop.json"),
                    r#"{"version":3,"dnd":true}"#,
                )
                .unwrap();
                let hub = Hub::new(
                    "focus-restart-test".into(),
                    false,
                    Duration::from_millis(16),
                )
                .shared();
                let socket = home.path().join("niri.sock");
                let listener = tokio::net::UnixListener::bind(&socket).unwrap();
                let compositor = tokio::task::spawn_local(async move {
                    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
                    loop {
                        let (stream, _) = listener.accept().await.unwrap();
                        let mut stream = BufReader::new(stream);
                        let mut request = String::new();
                        while stream.read_line(&mut request).await.unwrap() > 0 {
                            let reply = match request.trim() {
                                "\"Outputs\"" => json!({"Ok":{"Outputs":{}}}),
                                "\"Workspaces\"" => json!({"Ok":{"Workspaces":[]}}),
                                "\"Windows\"" => json!({"Ok":{"Windows":[]}}),
                                _ => json!({"Err":"Unsupported fixture request"}),
                            };
                            stream
                                .get_mut()
                                .write_all(format!("{reply}\n").as_bytes())
                                .await
                                .unwrap();
                            request.clear();
                        }
                    }
                });
                let environment = std::collections::BTreeMap::from([
                    ("NACRE_COMPOSITOR".into(), "niri".into()),
                    ("NIRI_SOCKET".into(), socket.to_str().unwrap().into()),
                ]);
                hub.borrow_mut().compositor =
                    Some(compositor_control::Control::new(paths.clone(), environment));
                let settings = settings::Service::load(hub.clone(), paths.clone(), None)
                    .await
                    .unwrap();
                hub.borrow_mut().settings = Some(settings);
                hub.borrow_mut().service("settings", "starting", 0).unwrap();
                hub.borrow_mut()
                    .service("notifications", "disabled", 5)
                    .unwrap();
                assert!(hub.borrow().notifications.is_none());
                let focus = Focus::new(paths);
                let state = State {
                    status: FocusStatus::Running,
                    total_ms: 60_000,
                    deadline_ms: wall_ms() - 5_000,
                    remaining_ms: 0,
                    previous_dnd: false,
                    owns_dnd: true,
                    daemon: true,
                };
                focus.save(&state).unwrap();
                let mut notices = hub.borrow().observe();
                let mut run = Box::pin(focus.clone().run(hub.clone()));
                // Virtual time forces expiry while the settings service is unavailable.
                assert!(tokio::time::timeout(Duration::from_secs(1), run.as_mut())
                    .await
                    .is_err());
                let saved =
                    State::parse(&crate::state::read_strict(&focus.path(), json!({})).unwrap())
                        .unwrap();
                assert_eq!(
                    saved, state,
                    "failed restoration must retain the persisted claim"
                );
                assert_eq!(focus.state.borrow().status, FocusStatus::Running);
                assert!(notifications::dnd(&hub).await.is_err());
                tokio::time::resume();
                hub.borrow_mut().service("settings", "running", 0).unwrap();
                tokio::time::timeout(Duration::from_secs(5), async {
                    loop {
                        tokio::select! {
                            result = run.as_mut() => panic!("focus stopped: {result:?}"),
                            notice = notices.recv() => { notice.unwrap(); }
                        }
                        if focus.state.borrow().status == FocusStatus::Done {
                            break;
                        }
                    }
                })
                .await
                .unwrap_or_else(|_| {
                    panic!(
                        "focus: {:?}; desktop: {:?}",
                        focus.state.borrow(),
                        hub.borrow().snapshot("settings.desktop")
                    )
                });
                assert!(!notifications::dnd(&hub).await.unwrap());
                let saved =
                    State::parse(&crate::state::read_strict(&focus.path(), json!({})).unwrap())
                        .unwrap();
                assert_eq!(saved.status, FocusStatus::Done);
                assert!(!saved.owns_dnd);
                compositor.abort();
                let _ = compositor.await;
            })
            .await;
    }

    #[tokio::test(start_paused = true)]
    async fn expired_session_completes_when_settings_remain_unavailable() {
        let logs = crate::daemon::test_logs::captured_logs();
        for initial_status in ["starting", "running", "disabled"] {
            let home = tempfile::tempdir().unwrap();
            let paths = Paths::new(home.path());
            std::fs::create_dir_all(&paths.state).unwrap();
            let hub = Hub::new(
                "focus-unavailable-test".into(),
                false,
                Duration::from_millis(16),
            )
            .shared();
            hub.borrow_mut()
                .service("settings", initial_status, 0)
                .unwrap();
            hub.borrow_mut()
                .service("notifications", "disabled", 5)
                .unwrap();
            let focus = Focus::new(paths);
            let state = State {
                status: FocusStatus::Running,
                total_ms: 60_000,
                deadline_ms: wall_ms() - 5_000,
                remaining_ms: 0,
                previous_dnd: false,
                owns_dnd: true,
                daemon: true,
            };
            focus.save(&state).unwrap();
            let mut run = Box::pin(focus.clone().run(hub.clone()));
            assert!(tokio::time::timeout(Duration::from_secs(1), run.as_mut())
                .await
                .is_err());
            if initial_status == "starting" {
                assert_eq!(
                    *focus.state.borrow(),
                    state,
                    "first failure must retain the claim"
                );
                // Neither unrelated startup signals nor a transient restart spends the retry.
                for (service, status) in [("network-watch", "starting"), ("settings", "restarting")]
                {
                    hub.borrow_mut().service(service, status, 1).unwrap();
                    assert!(tokio::time::timeout(Duration::from_secs(1), run.as_mut())
                        .await
                        .is_err());
                    assert_eq!(*focus.state.borrow(), state);
                }
                // The supervisor has exhausted its budget; no settings recovery follows.
                hub.borrow_mut().service("settings", "disabled", 5).unwrap();
            }
            assert!(tokio::time::timeout(Duration::from_secs(6), run.as_mut())
                .await
                .is_err());
            let saved = State::parse(&crate::state::read_strict(&focus.path(), json!({})).unwrap())
                .unwrap();
            assert_eq!(saved.status, FocusStatus::Done);
            assert!(!saved.owns_dnd);
            assert_eq!(hub.borrow().snapshot(TOPIC).unwrap()["status"], "done");
            assert!(notifications::dnd(&hub).await.is_err());
        }
        assert!(String::from_utf8_lossy(&logs.0.lock().unwrap())
            .contains("Focus completed without restoring Do Not Disturb after retry"));
    }
}
