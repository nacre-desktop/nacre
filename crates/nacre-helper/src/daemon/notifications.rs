//! Managed notification producer. Starting this service is startup authority,
//! separate from client routing; clients cannot turn ownership on.
mod bus;
mod model;
mod storage;

use super::hub::{failure, Notice, Shared};
use crate::{paths::Paths, Result};
use futures_util::StreamExt;
use model::{Input, Model, NativeSignal, Outcome, Policy};
use nacre_protocol::{
    core_services::{CoreCall, Health, HistoryStorage, NotificationClearScope},
    settings::SettingsGet,
    ErrorCode, Failure, Method, MAX_SAFE_INTEGER,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use storage::{ImageCache, Store};
use tokio::{
    sync::{mpsc, oneshot, Notify, OwnedSemaphorePermit, Semaphore},
    task::JoinHandle,
};

pub const TOPIC: &str = "notifications";
const DEADLINE: Duration = Duration::from_secs(3);
const FLUSH: Duration = Duration::from_millis(150);
const SAVE_RETRY: Duration = Duration::from_secs(1);

enum Command {
    Notify(bus::Notify),
    Close(u32),
    Inhibit(String, String),
    Uninhibit(String, u32),
    Core(u64, CoreCall),
}
enum Response {
    NativeId(u32),
    Unit,
    Core(Value),
}
struct Request {
    command: Command,
    deadline: Instant,
    reply: oneshot::Sender<std::result::Result<Response, Failure>>,
    _slot: OwnedSemaphorePermit,
    _bytes: OwnedSemaphorePermit,
}

#[derive(Clone)]
struct Ingress {
    sender: mpsc::Sender<Request>,
    slots: Arc<Semaphore>,
    bytes: Arc<Semaphore>,
    ready: Arc<AtomicBool>,
    writable: Arc<AtomicBool>,
    inhibited: Arc<AtomicBool>,
    disconnected: Arc<Mutex<BTreeSet<u64>>>,
    clear_ui: Arc<AtomicBool>,
    wake: Arc<Notify>,
    stop: Arc<Notify>,
}

impl Ingress {
    async fn call(
        &self,
        command: Command,
        bytes: usize,
        timeout: Option<u32>,
    ) -> std::result::Result<Response, Failure> {
        if !self.ready.load(Ordering::Acquire) || self.sender.is_closed() {
            return Err(bus::unavailable());
        }
        if bytes > 4 * 1024 * 1024 + 128 * 1024 {
            return Err(failure(
                ErrorCode::TooLarge,
                "Notification request exceeds its limit",
                false,
            ));
        }
        let slot = self.slots.clone().try_acquire_owned().map_err(|_| busy())?;
        let bytes = self
            .bytes
            .clone()
            .try_acquire_many_owned(bytes.max(1) as u32)
            .map_err(|_| busy())?;
        let duration = timeout.map_or(DEADLINE, |ms| {
            Duration::from_millis(u64::from(ms)).min(DEADLINE)
        });
        let (reply, receive) = oneshot::channel();
        self.sender
            .try_send(Request {
                command,
                deadline: Instant::now() + duration,
                reply,
                _slot: slot,
                _bytes: bytes,
            })
            .map_err(|_| busy())?;
        tokio::time::timeout(duration, receive)
            .await
            .map_err(|_| timeout_error())?
            .map_err(|_| bus::unavailable())?
    }
    fn storage_writable(&self) -> bool {
        self.writable.load(Ordering::Acquire)
    }
    fn inhibited(&self) -> bool {
        self.inhibited.load(Ordering::Acquire)
    }
}
fn busy() -> Failure {
    failure(ErrorCode::Busy, "Notification queue is full", true)
}
fn timeout_error() -> Failure {
    failure(ErrorCode::Timeout, "Notification deadline reached", true)
}

#[derive(Clone)]
pub struct Requests(Ingress);
impl Requests {
    pub fn stop(&self) {
        self.0.ready.store(false, Ordering::Release);
        self.0.stop.notify_one();
    }
    pub fn available(&self) -> bool {
        self.0.ready.load(Ordering::Acquire) && !self.0.sender.is_closed()
    }
    pub fn methods(&self) -> BTreeMap<String, Method> {
        if self.available() {
            nacre_protocol::notifications::method_descriptors()
        } else {
            BTreeMap::new()
        }
    }
    pub fn disconnected(&self, id: u64) {
        if let Ok(mut pending) = self.0.disconnected.lock() {
            if pending.len() < 256 {
                pending.insert(id);
            } else {
                self.0.clear_ui.store(true, Ordering::Release);
            }
        } else {
            self.0.clear_ui.store(true, Ordering::Release);
        }
        self.0.wake.notify_one();
    }
}

pub async fn dispatch(
    hub: Shared,
    client: u64,
    method: &str,
    params: Value,
    timeout: Option<u32>,
) -> std::result::Result<Value, Failure> {
    let bytes = serde_json::to_vec(&params)
        .map_err(|_| {
            failure(
                ErrorCode::Validation,
                "Invalid notification parameters",
                false,
            )
        })?
        .len();
    if bytes > 128 * 1024 {
        return Err(failure(
            ErrorCode::TooLarge,
            "Notification parameters exceed their limit",
            false,
        ));
    }
    let call: CoreCall = serde_json::from_value(json!({"m":method,"p":params})).map_err(|_| {
        failure(
            ErrorCode::Validation,
            "Invalid notification parameters",
            false,
        )
    })?;
    if !call.valid() {
        return Err(failure(
            ErrorCode::Validation,
            "Invalid notification parameters",
            false,
        ));
    }
    let requests = hub
        .borrow()
        .notifications
        .clone()
        .ok_or_else(bus::unavailable)?;
    match requests
        .0
        .call(Command::Core(client, call), bytes, timeout)
        .await?
    {
        Response::Core(value) => Ok(value),
        _ => Err(failure(
            ErrorCode::Internal,
            "Notification processing failed",
            false,
        )),
    }
}

fn channel() -> (Requests, mpsc::Receiver<Request>) {
    let (sender, receive) = mpsc::channel(64);
    (
        Requests(Ingress {
            sender,
            slots: Arc::new(Semaphore::new(64)),
            bytes: Arc::new(Semaphore::new(16 * 1024 * 1024)),
            ready: Arc::new(AtomicBool::new(false)),
            writable: Arc::new(AtomicBool::new(false)),
            inhibited: Arc::new(AtomicBool::new(false)),
            disconnected: Arc::new(Mutex::new(BTreeSet::new())),
            clear_ui: Arc::new(AtomicBool::new(false)),
            wake: Arc::new(Notify::new()),
            stop: Arc::new(Notify::new()),
        }),
        receive,
    )
}

fn policy(paths: &Paths, raw: &Value) -> Result<Policy> {
    let assets = paths.assets();
    let qml = if assets.file_name().is_some_and(|name| name == "shell-tools") {
        assets
            .parent()
            .ok_or("Missing notification configuration")?
            .join("shell/config/NacreNotifications.qml")
    } else {
        assets
            .parent()
            .ok_or("Missing notification configuration")?
            .join("config/NacreNotifications.qml")
    };
    let qml = std::fs::read_to_string(qml)?;
    let timeout =
        regex::Regex::new(r"(?m)^\s*property\s+int\s+defaultExpireTimeout\s*:\s*([0-9]+)\s*$")?
            .captures(&qml)
            .and_then(|c| c[1].parse::<u32>().ok())
            .filter(|n| *n > 0 && *n <= i32::MAX as u32)
            .ok_or("Notification timeout policy is unavailable")?;
    let expire = regex::Regex::new(r"(?m)^\s*property\s+bool\s+expire\s*:\s*(true|false)\s*$")?
        .captures(&qml)
        .map(|c| &c[1] == "true")
        .ok_or("Notification expiry policy is unavailable")?;
    let strings = |key: &str| -> Result<Vec<String>> {
        raw[key]
            .as_array()
            .ok_or("Notification retention policy is unavailable")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Notification retention policy is unavailable".into())
            })
            .collect()
    };
    Ok(Policy {
        expire,
        default_timeout_ms: timeout,
        feedback_apps: strings("feedbackApps")?.into_iter().collect(),
        feedback_summaries: strings("feedbackSummaries")?.into_iter().collect(),
        feedback_patterns: strings("feedbackPatterns")?,
    })
}

async fn desktop_state(hub: &Shared) -> std::result::Result<Value, Failure> {
    if hub
        .borrow()
        .services
        .get("settings")
        .is_none_or(|v| v["status"] != "running")
    {
        return Err(bus::unavailable());
    }
    let settings = hub.borrow().settings.clone().ok_or_else(bus::unavailable)?;
    let state = settings
        .get(SettingsGet {
            store: "desktop".into(),
        })
        .await?;
    if !state["error"].is_null() {
        return Err(bus::unavailable());
    }
    Ok(state)
}

fn read_dnd(state: &Value) -> std::result::Result<bool, Failure> {
    // DesktopSettings defines DND as `data.dnd === true`; a missing key in a
    // successfully read, validated store is the established false default.
    match state["data"].get("dnd") {
        None => Ok(false),
        Some(value) => value.as_bool().ok_or_else(bus::unavailable),
    }
}

pub(super) async fn dnd(hub: &Shared) -> std::result::Result<bool, Failure> {
    read_dnd(&desktop_state(hub).await?)
}

async fn bounded_dnd(hub: &Shared) -> std::result::Result<bool, Failure> {
    tokio::time::timeout(DEADLINE, dnd(hub))
        .await
        .map_err(|_| timeout_error())?
}

async fn set_dnd(
    hub: &Shared,
    enabled: bool,
    deadline: Instant,
) -> std::result::Result<bool, Failure> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(timeout_error());
    }
    tokio::time::timeout(remaining, set_dnd_inner(hub, enabled))
        .await
        .map_err(|_| timeout_error())?
}

pub(super) async fn set_dnd_inner(
    hub: &Shared,
    enabled: bool,
) -> std::result::Result<bool, Failure> {
    let state = desktop_state(hub).await?;
    let before = read_dnd(&state)?;
    if before == enabled {
        return Ok(false);
    }
    let result = super::settings::dispatch(
        hub.clone(),
        "settings.desktop",
        json!({"arguments":["set","dnd",enabled.to_string()],"rev":state["rev"]}),
    )
    .await?;
    if !result["error"].is_null() || dnd(hub).await? != enabled {
        return Err(failure(
            ErrorCode::Unavailable,
            "Notification settings readback failed",
            true,
        ));
    }
    Ok(true)
}

fn wall_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

fn publish(
    hub: &Shared,
    model: &Model,
    storage: &HistoryStorage,
    ingress: &Ingress,
    available: bool,
    changed: bool,
) -> Result<u64> {
    let mut state = model.snapshot(storage.clone());
    if !available {
        state.popup_order = Some(Vec::new());
    }
    state.health = Health {
        available,
        stale: !available,
        error: (!available).then(bus::unavailable),
    };
    ingress
        .inhibited
        .store(state.inhibited.unwrap_or(false), Ordering::Release);
    ingress
        .writable
        .store(storage.ready && storage.writable, Ordering::Release);
    let value = serde_json::to_value(state)?;
    if changed {
        hub.borrow_mut().publish_transition(TOPIC, value)?;
    } else {
        hub.borrow_mut().publish(TOPIC, value)?;
    }
    Ok(hub.borrow().topic_seq(TOPIC).unwrap_or(0))
}

type SaveTask = JoinHandle<(Store, std::result::Result<(), Failure>)>;

fn public_storage(store: &Store) -> HistoryStorage {
    let mut health = store.health();
    if health.error.is_some() {
        health.writable = false;
    }
    health
}

fn store_can_write(store: &Option<Store>) -> bool {
    store.as_ref().is_some_and(|store| {
        let health = store.health();
        health.ready && health.writable
    })
}

fn start_save(store: &mut Option<Store>, task: &mut Option<SaveTask>, rows: Value) {
    if task.is_some() {
        return;
    }
    let Some(mut store) = store.take() else {
        return;
    };
    *task = Some(tokio::task::spawn_blocking(move || {
        let saved = store.save(&rows);
        (store, saved)
    }));
}

fn retry_at(dirty: &mut Option<Instant>, delay: Duration) {
    *dirty = Some(Instant::now() + delay);
}

struct ReadinessGuard {
    hub: Shared,
    ingress: Ingress,
}

impl Drop for ReadinessGuard {
    fn drop(&mut self) {
        self.ingress.ready.store(false, Ordering::Release);
        if let Ok(mut hub) = self.hub.try_borrow_mut() {
            if let Some(mut state) = hub.snapshot(TOPIC) {
                state["health"] =
                    json!({"available":false,"stale":true,"error":bus::unavailable()});
                state["popup_order"] = json!([]);
                let _ = hub.publish(TOPIC, state);
            }
            hub.notifications = None;
        }
    }
}

type ImageGate = Arc<Semaphore>;

/// A timed-out blocking job retains its slot until it actually exits. Neither
/// decoding nor cache cleanup can accumulate detached workers after a deadline.
async fn image_work<T: Send + 'static>(
    gate: ImageGate,
    deadline: Instant,
    work: impl FnOnce() -> std::result::Result<T, Failure> + Send + 'static,
) -> std::result::Result<T, Failure> {
    let remaining = deadline.saturating_duration_since(Instant::now());
    if remaining.is_zero() {
        return Err(timeout_error());
    }
    let permit = gate.try_acquire_owned().map_err(|_| busy())?;
    tokio::time::timeout(
        remaining,
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            work()
        }),
    )
    .await
    .map_err(|_| timeout_error())?
    .map_err(|_| {
        failure(
            ErrorCode::Internal,
            "Notification image worker failed",
            false,
        )
    })?
}

async fn retain_cache(cache: Arc<ImageCache>, gate: ImageGate, references: Vec<String>) -> bool {
    image_work(gate, Instant::now() + DEADLINE, move || {
        cache.retain(&references)
    })
    .await
    .is_ok()
}

async fn resolve_before(
    request: bus::Notify,
    cache: Arc<ImageCache>,
    gate: ImageGate,
    deadline: Instant,
) -> std::result::Result<Input, Failure> {
    let resolved = image_work(gate, deadline, move || resolve(request, &cache)).await?;
    if Instant::now() >= deadline {
        return Err(timeout_error());
    }
    Ok(resolved)
}

async fn finish(
    hub: &Shared,
    model: &Model,
    storage: &HistoryStorage,
    ingress: &Ingress,
    bus: &bus::Owned,
    outcome: Outcome,
    deadline: Instant,
) -> std::result::Result<(), Failure> {
    let seq = publish(
        hub,
        model,
        storage,
        ingress,
        ingress.ready.load(Ordering::Acquire),
        outcome.changed,
    )
    .map_err(|_| {
        failure(
            ErrorCode::Internal,
            "Notification publication failed",
            false,
        )
    })?;
    let mut signal_error = None;
    for signal in outcome.signals {
        let remaining = deadline.saturating_duration_since(Instant::now());
        let sent = match signal {
            NativeSignal::ActionInvoked {
                destination,
                native_id,
                key,
            } => {
                tokio::time::timeout(
                    remaining,
                    bus.connection.emit_signal(
                        Some(destination.as_str()),
                        bus::PATH,
                        bus::NAME,
                        "ActionInvoked",
                        &(native_id, key),
                    ),
                )
                .await
            }
            NativeSignal::NotificationClosed {
                destination,
                native_id,
                reason,
            } => {
                tokio::time::timeout(
                    remaining,
                    bus.connection.emit_signal(
                        Some(destination.as_str()),
                        bus::PATH,
                        bus::NAME,
                        "NotificationClosed",
                        &(native_id, reason),
                    ),
                )
                .await
            }
        };
        if !matches!(sent, Ok(Ok(()))) {
            signal_error = Some(failure(
                ErrorCode::Io,
                "Notification signal could not be submitted",
                true,
            ));
        }
    }
    for (name, mut data) in outcome.events {
        if name == "action" && signal_error.is_some() {
            continue;
        }
        data["state_seq"] = json!(seq);
        let event: nacre_protocol::notifications::NotificationEvent =
            serde_json::from_value(json!({"name":name,"data":data}))
                .map_err(|_| failure(ErrorCode::Internal, "Invalid notification event", false))?;
        if !event.valid() {
            return Err(failure(
                ErrorCode::Internal,
                "Invalid notification event",
                false,
            ));
        }
        hub.borrow_mut().event(TOPIC, &name, data)?;
    }
    signal_error.map_or(Ok(()), Err)
}

fn resolve(request: bus::Notify, cache: &ImageCache) -> std::result::Result<Input, Failure> {
    let mut icon = request.icon;
    if icon.starts_with('/') || icon.starts_with("file:") {
        icon = cache.resolve_path(&icon)?;
    } else if icon.contains('/') || icon.contains(':') {
        return Err(failure(
            ErrorCode::Validation,
            "Unsupported notification icon",
            false,
        ));
    }
    let image_path = match request.image {
        Some(bus::Image::Path(path)) => Some(cache.resolve_path(&path)?),
        Some(bus::Image::Data(data)) => Some(cache.resolve_data(data)?),
        None => None,
    };
    Ok(Input {
        sender: request.sender,
        replaces: request.replaces,
        timeout: request.timeout,
        desktop_entry: request.desktop_entry,
        image_path,
        notice: bead_services::notifications::Notice {
            id: 0,
            app: request.app_name,
            icon,
            summary: request.summary,
            body: request.body,
            actions: request.actions,
            urgency: request.urgency,
            transient: request.transient,
            resident: request.resident,
            popup: false,
            expanded: false,
            timeout_ms: 0,
            time_unix_ms: 0,
            deadline: None,
            remaining: None,
            hovered: false,
        },
    })
}

fn destructive(call: &CoreCall) -> bool {
    matches!(call,CoreCall::NotificationDismiss(p) if p.forget)
        || matches!(call,CoreCall::NotificationClear(p) if p.scope!=NotificationClearScope::Popups)
}

/// Separate startup authority. The normal daemon never calls this function.
pub async fn run(hub: Shared, paths: Paths) -> Result<()> {
    let mut notices = hub.borrow().observe();
    let raw_policy = crate::notification_history::policy(&paths)?;
    let timing = policy(&paths, &raw_policy)?;
    let history = paths.state.join("shell/notifications/history.json");
    let mut store =
        Some(tokio::task::spawn_blocking(move || Store::open(history, raw_policy)).await??);
    let mut storage = public_storage(store.as_ref().expect("opened store"));
    let cache = Arc::new(ImageCache::open(
        paths.state.join("shell/notifications/images"),
    )?);
    let image_gate = Arc::new(Semaphore::new(1));
    let (requests, mut receive) = channel();
    hub.borrow_mut().notifications = Some(requests.clone());
    let ingress = requests.0;
    let _readiness = ReadinessGuard {
        hub: hub.clone(),
        ingress: ingress.clone(),
    };
    let boot = hub.borrow().boot.clone();
    let mut generation = 0u64;
    let mut rows = store.as_ref().expect("opened store").rows();
    let mut history_dirty = store.as_ref().is_some_and(Store::needs_save);
    let mut dirty = history_dirty.then(|| Instant::now() + FLUSH);
    let mut save_task: Option<SaveTask> = None;
    loop {
        // Re-read the authoritative setting on every ownership reacquisition.
        let current_dnd = loop {
            match bounded_dnd(&hub).await {
                Ok(value) => break value,
                Err(_) => {
                    ingress.ready.store(false, Ordering::Release);
                    tokio::select! {
                        _=ingress.stop.notified()=>{
                            hub.borrow_mut().notifications=None;
                            hub.borrow_mut().service("notifications","stopped",0)?;
                            return Ok(());
                        },
                        _=notices.recv()=>{}
                    }
                }
            }
        };
        generation = generation
            .checked_add(1)
            .filter(|n| *n <= MAX_SAFE_INTEGER)
            .ok_or("Notification generations exhausted")?;
        let loaded_count = rows.len();
        let mut model = Model::new(&boot, generation, rows, current_dnd, timing.clone());
        if model.closed_count() != loaded_count {
            history_dirty = true;
            dirty.get_or_insert_with(|| Instant::now() + FLUSH);
        }
        let initial_refs = model.image_references();
        let mut cache_dirty =
            !retain_cache(Arc::clone(&cache), Arc::clone(&image_gate), initial_refs).await;
        if cache_dirty {
            retry_at(&mut dirty, SAVE_RETRY);
        }
        let connection = match zbus::connection::Builder::session()?
            .max_queued(64)
            .build()
            .await
        {
            Ok(connection) => connection,
            Err(_) => {
                hub.borrow_mut()
                    .service("notifications", "unavailable", 0)?;
                tokio::select! {_=ingress.stop.notified()=>{hub.borrow_mut().notifications=None;return Ok(());},_=tokio::time::sleep(Duration::from_secs(1))=>{}}
                rows = model
                    .persisted_rows()?
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                continue;
            }
        };
        let mut bus = match bus::Owned::start(connection, ingress.clone()).await {
            Ok(bus) => bus,
            Err(_) => {
                ingress.ready.store(false, Ordering::Release);
                publish(&hub, &model, &storage, &ingress, false, true)?;
                hub.borrow_mut().service("notifications", "refused", 0)?;
                hub.borrow_mut().notifications = None;
                return Ok(());
            }
        };
        let dbus = zbus::fdo::DBusProxy::new(&bus.connection).await?;
        let mut owners = dbus.receive_name_owner_changed().await?;
        ingress.ready.store(true, Ordering::Release);
        publish(&hub, &model, &storage, &ingress, true, true)?;
        hub.borrow_mut().service("notifications", "running", 0)?;
        let mut ui_clients = BTreeSet::new();
        let mut stopping = false;
        loop {
            let deadline = model.next_deadline().into_iter().chain(dirty).min();
            let timer = async {
                if let Some(deadline) = deadline {
                    tokio::time::sleep_until(deadline.into()).await;
                } else {
                    std::future::pending::<()>().await;
                }
            };
            tokio::select! {
                _=bus.connection.closed()=>break,
                _=ingress.stop.notified()=>{stopping=true;break;},
                _=timer=>{
                    let old_refs=model.image_references();
                    let outcome=model.tick(Instant::now(),wall_ms());
                    if outcome.persist {history_dirty=true;dirty.get_or_insert_with(||Instant::now()+FLUSH);}
                    let _=finish(&hub,&model,&storage,&ingress,&bus,outcome,Instant::now()+DEADLINE).await;
                    let references=model.image_references();
                    if references!=old_refs {cache_dirty=true;dirty.get_or_insert_with(||Instant::now()+FLUSH);}
                    if dirty.is_some_and(|deadline|deadline<=Instant::now()) {
                        if cache_dirty {
                            cache_dirty= !retain_cache(Arc::clone(&cache),Arc::clone(&image_gate),references.clone()).await;
                            if cache_dirty {retry_at(&mut dirty,SAVE_RETRY);}
                        }
                        if history_dirty && save_task.is_none() && store_can_write(&store) {
                            let rows=model.persisted_rows()?;
                            history_dirty=false;
                            if !cache_dirty {dirty=None;}
                            start_save(&mut store,&mut save_task,rows);
                        } else if (save_task.is_some() && history_dirty) || cache_dirty {
                            retry_at(&mut dirty,SAVE_RETRY);
                        } else {
                            dirty=None;
                        }
                    }
                },
                completed=async {
                    if let Some(task)=save_task.as_mut() {Some(task.await)} else {std::future::pending().await}
                }, if save_task.is_some()=>{
                    save_task=None;
                    match completed {
                        Some(Ok((saved,result)))=>{
                            storage=public_storage(&saved);
                            let failed=result.is_err();
                            store=Some(saved);
                            if failed {history_dirty=true;retry_at(&mut dirty,SAVE_RETRY);}
                        },
                        _=>{
                            store=None;
                            storage=HistoryStorage{ready:false,writable:false,error:Some(failure(ErrorCode::Unavailable,"Notification history worker failed",true))};
                        }
                    }
                    publish(&hub,&model,&storage,&ingress,ingress.ready.load(Ordering::Acquire),false)?;
                },
                _=ingress.wake.notified()=>{
                    let mut ids=ingress.disconnected.lock().map(|mut ids|std::mem::take(&mut *ids)).unwrap_or_default();
                    if ingress.clear_ui.swap(false,Ordering::AcqRel) {ids.extend(ui_clients.iter().copied());}
                    for id in ids {if ui_clients.remove(&id) {let outcome=model.disconnect(id,Instant::now(),wall_ms());
                        if outcome.persist {history_dirty=true;dirty.get_or_insert_with(||Instant::now()+FLUSH);}
                        let _=finish(&hub,&model,&storage,&ingress,&bus,outcome,Instant::now()+DEADLINE).await;}}
                },
                changed=notices.recv()=>{
                    if matches!(changed,Ok(Notice::Changed{ref topic,..}) if topic=="settings.store.desktop" || topic=="daemon") || matches!(changed,Err(tokio::sync::broadcast::error::RecvError::Lagged(_))) {
                        match bounded_dnd(&hub).await {
                            Ok(value)=>{ingress.ready.store(true,Ordering::Release);let outcome=model.set_dnd(value,Instant::now(),wall_ms());
                                let _=finish(&hub,&model,&storage,&ingress,&bus,outcome,Instant::now()+DEADLINE).await;},
                            Err(_)=>{ingress.ready.store(false,Ordering::Release);publish(&hub,&model,&storage,&ingress,false,true)?;}
                        }
                    }
                },
                Some(change)=owners.next()=>{
                    if let Ok(args)=change.args() {if args.new_owner().as_ref().is_none() {
                        let outcome=model.owner_left(args.name().as_str(),Instant::now(),wall_ms());
                        let _=finish(&hub,&model,&storage,&ingress,&bus,outcome,Instant::now()+DEADLINE).await;
                    }}
                },
                Some(request)=receive.recv()=>{
                    if request.deadline<=Instant::now() {let _=request.reply.send(Err(timeout_error()));continue;}
                    if !ingress.ready.load(Ordering::Acquire) {let _=request.reply.send(Err(bus::unavailable()));continue;}
                    let old_refs=model.image_references();
                    // Resolution can create a cache file even if model admission
                    // subsequently fails. Reclaim it after the bounded worker ends.
                    let resolved_images=matches!(&request.command, Command::Notify(input)
                        if input.image.is_some() || input.icon.starts_with('/') || input.icon.starts_with("file:"));
                    let mut persist=false;
                    let result=match request.command {
                        Command::Notify(input)=>match resolve_before(input,Arc::clone(&cache),Arc::clone(&image_gate),request.deadline).await {
                            Ok(input) if Instant::now()<request.deadline=>model.notify(input,Instant::now(),wall_ms()).map(|outcome| {let id=outcome.native_id.unwrap_or(0);(outcome,Response::NativeId(id))}),
                            Ok(_)=>Err(timeout_error()),Err(error)=>Err(error)},
                        Command::Close(id)=>Ok((model.close_native(id,3,Instant::now(),wall_ms()),Response::Unit)),
                        Command::Inhibit(owner,reason)=>{
                            let name=match zbus::names::BusName::try_from(owner.as_str()) {Ok(name)=>name,Err(_)=>{let _=request.reply.send(Err(failure(ErrorCode::Validation,"Invalid inhibition owner",false)));continue;}};
                            let remaining=request.deadline.saturating_duration_since(Instant::now());
                            if remaining.is_zero(){let _=request.reply.send(Err(timeout_error()));continue;}
                            let live=match tokio::time::timeout(remaining,dbus.name_has_owner(name)).await {
                                Ok(Ok(value))=>value,
                                Ok(Err(_))=>false,
                                Err(_)=>{let _=request.reply.send(Err(timeout_error()));continue;}
                            };
                            if live {model.inhibit_native(owner,reason,Instant::now(),wall_ms()).map(|o|{let id=o.native_id.unwrap_or(0);(o,Response::NativeId(id))})} else {Err(bus::unavailable())}
                        },
                        Command::Uninhibit(owner,cookie)=>model.uninhibit_native(&owner,cookie,Instant::now(),wall_ms()).map(|o|(o,Response::Unit)),
                        Command::Core(client,call)=>{
                            if !hub.borrow().has_client(client) {Err(bus::unavailable())}
                            else if matches!(&call,CoreCall::NotificationHover(_) | CoreCall::NotificationInhibited(_)) && !hub.borrow().is_ui_provider(client) {
                                Err(failure(ErrorCode::PermissionDenied,"Notification UI provider required",false))
                            } else if matches!(&call,CoreCall::NotificationHover(_) | CoreCall::NotificationInhibited(_))
                                && !ui_clients.contains(&client) && ui_clients.len()>=128 {
                                Err(busy())
                            } else if destructive(&call) && (!storage.ready || !storage.writable) {
                                Err(failure(ErrorCode::PermissionDenied,"Notification history is not writable",false))
                            } else {match call {
                                CoreCall::NotificationDnd(p)=>match set_dnd(&hub,p.enabled,request.deadline).await {Ok(changed)=>{let outcome=model.set_dnd(p.enabled,Instant::now(),wall_ms());Ok((outcome,Response::Core(json!({"changed":changed}))))},Err(e)=>Err(e)},
                                call=>{
                                    let ui_policy=matches!(&call,CoreCall::NotificationHover(_) | CoreCall::NotificationInhibited(_));
                                    model.control(call,client,Instant::now(),wall_ms()).map(|outcome|{
                                        if ui_policy && outcome.changed {ui_clients.insert(client);}
                                        let changed=outcome.changed;(outcome,Response::Core(json!({"changed":changed})))
                                    })
                                }
                            }}
                        },
                    };
                    let result=match result {Ok((outcome,response))=>{persist=outcome.persist;
                        finish(&hub,&model,&storage,&ingress,&bus,outcome,request.deadline).await.map(|_|response)},Err(error)=>Err(error)};
                    if persist {history_dirty=true;dirty.get_or_insert_with(||Instant::now()+FLUSH);}
                    if resolved_images || model.image_references()!=old_refs {cache_dirty=true;dirty.get_or_insert_with(||Instant::now()+FLUSH);}
                    let _=request.reply.send(result);
                },
            }
        }
        ingress.ready.store(false, Ordering::Release);
        rows = model
            .persisted_rows()?
            .as_array()
            .cloned()
            .unwrap_or_default();
        if let Some(task) = save_task.take() {
            match task.await {
                Ok((saved, result)) => {
                    storage = public_storage(&saved);
                    store = Some(saved);
                    if result.is_err() {
                        history_dirty = true;
                    }
                }
                Err(_) => {
                    history_dirty = true;
                    store = None;
                    storage = HistoryStorage {
                        ready: false,
                        writable: false,
                        error: Some(failure(
                            ErrorCode::Unavailable,
                            "Notification history worker failed",
                            true,
                        )),
                    };
                }
            }
        }
        let mut flush_error = None;
        if history_dirty && store_can_write(&store) {
            let mut current = store.take().expect("writable history store");
            let final_rows = Value::Array(rows.clone());
            match tokio::task::spawn_blocking(move || {
                let result = current.save(&final_rows);
                (current, result)
            })
            .await
            {
                Ok((saved, result)) => {
                    storage = public_storage(&saved);
                    store = Some(saved);
                    if let Err(error) = result {
                        flush_error = Some(error);
                    }
                    history_dirty = false;
                }
                Err(_) => {
                    store = None;
                    storage = HistoryStorage {
                        ready: false,
                        writable: false,
                        error: Some(failure(
                            ErrorCode::Unavailable,
                            "Notification history worker failed",
                            true,
                        )),
                    };
                    flush_error = Some(failure(
                        ErrorCode::Unavailable,
                        "Notification history worker failed during shutdown",
                        true,
                    ));
                }
            }
        }
        if history_dirty && flush_error.is_none() && store.is_none() {
            flush_error = Some(failure(
                ErrorCode::Unavailable,
                "Notification history worker failed during shutdown",
                true,
            ));
        }
        let references = model.image_references();
        let _ = retain_cache(Arc::clone(&cache), Arc::clone(&image_gate), references).await;
        publish(&hub, &model, &storage, &ingress, false, true)?;
        bus.stop().await;
        if let Some(error) = flush_error {
            return Err(error.into());
        }
        if stopping {
            hub.borrow_mut().notifications = None;
            hub.borrow_mut().service("notifications", "stopped", 0)?;
            return Ok(());
        }
        hub.borrow_mut()
            .service("notifications", "unavailable", 0)?;
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn timed_out_image_work_keeps_its_capacity_until_the_worker_exits() {
        let gate = Arc::new(Semaphore::new(1));
        let (started, ready) = oneshot::channel();
        let (release, wait) = std::sync::mpsc::channel();
        let task = tokio::spawn(image_work(
            gate.clone(),
            Instant::now() + Duration::from_millis(100),
            move || {
                let _ = started.send(());
                let _ = wait.recv_timeout(Duration::from_secs(2));
                Ok(())
            },
        ));
        ready.await.unwrap();
        assert_eq!(task.await.unwrap().unwrap_err().code, ErrorCode::Timeout);
        assert_eq!(gate.available_permits(), 0);
        let rejected = image_work(gate.clone(), Instant::now() + DEADLINE, || Ok(())).await;
        assert_eq!(rejected.unwrap_err().code, ErrorCode::Busy);
        release.send(()).unwrap();
        let permit = tokio::time::timeout(Duration::from_secs(1), gate.acquire_owned())
            .await
            .unwrap()
            .unwrap();
        drop(permit);
    }
}
