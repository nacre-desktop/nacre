//! Serialized settings ownership. Disk work uses the daemon's bounded blocking pool.
use super::writer_lease::WriterLease;
use super::{
    hub::{failure, Shared},
    settings_validation as validation,
};
use crate::{paths::Paths, state, Result};
use nacre_protocol::{
    settings::{
        DesktopSettingsCall, SettingsGet, SettingsProviderWrite, SettingsPublish,
        SettingsReadError, SettingsSet, SettingsStore,
    },
    ErrorCode, Failure, Method, MAX_SAFE_INTEGER,
};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    path::PathBuf,
    rc::{Rc, Weak},
    sync::Arc,
    time::Duration,
};
use tokio::{
    sync::{Mutex, Notify},
    time::Instant,
};

const QUIET: Duration = Duration::from_millis(150);

struct Store {
    schema: &'static state::Schema,
    path: PathBuf,
    snapshot: SettingsStore,
    fingerprint: Option<[u8; 32]>,
    deadline: Option<Instant>,
}

pub struct Service {
    stores: Mutex<BTreeMap<String, Store>>,
    changed: Notify,
    durable_changed: Notify,
    hub: Weak<RefCell<super::hub::Hub>>,
    paths: Paths,
    writer_lease: Option<Arc<WriterLease>>,
    mutations: Cell<usize>,
    #[cfg(test)]
    defer_launcher_flush: Cell<bool>,
}

pub(crate) struct LauncherPreferencesAdmission {
    pub(crate) result: Value,
    revision: u64,
}

fn topic(id: &str) -> String {
    format!("settings.store.{id}")
}
fn unavailable() -> Failure {
    failure(
        ErrorCode::Unavailable,
        "Settings service is unavailable",
        true,
    )
}
fn denied() -> Failure {
    failure(
        ErrorCode::PermissionDenied,
        "Setting is not writable through this method",
        false,
    )
}
fn read_error(schema: &state::Schema, line: usize, message: &str) -> SettingsReadError {
    SettingsReadError {
        file: schema.paths[0].clone(),
        line,
        message: message.into(),
    }
}

impl Service {
    pub async fn load(
        hub: Shared,
        paths: Paths,
        writer_lease: Option<Arc<WriterLease>>,
    ) -> Result<Rc<Self>> {
        let service_paths = paths.clone();
        let stores = tokio::task::spawn_blocking(move || -> Result<BTreeMap<String, Store>> {
            let mut stores = BTreeMap::new();
            for schema in state::schemas()
                .iter()
                .filter(|s| s.settings.is_some() || s.id == "battery-alerts")
            {
                let path = if schema.id == "battery-alerts" {
                    paths.state.join("battery-alert-state.json")
                } else {
                    state::settings_path_in(&paths, &schema.id)?
                };
                let read = state::read_document(&path, schema);
                let (data, fingerprint, error) = match read {
                    Ok((data, fingerprint)) if validation::document(&schema.id, &data) => {
                        (data, fingerprint, None)
                    }
                    Ok(_) => (
                        schema.defaults.clone(),
                        None,
                        Some(read_error(schema, 0, "Invalid settings value")),
                    ),
                    Err((line, message)) => (
                        schema.defaults.clone(),
                        None,
                        Some(read_error(schema, line, message)),
                    ),
                };
                let snapshot = SettingsStore {
                    store: schema.id.clone(),
                    rev: 0,
                    exists: path.exists(),
                    data,
                    dirty: false,
                    error,
                };
                stores.insert(
                    schema.id.clone(),
                    Store {
                        schema,
                        path,
                        snapshot,
                        fingerprint,
                        deadline: None,
                    },
                );
            }
            Ok(stores)
        })
        .await??;
        let service = Rc::new(Self {
            stores: Mutex::new(stores),
            changed: Notify::new(),
            durable_changed: Notify::new(),
            hub: Rc::downgrade(&hub),
            paths: service_paths,
            writer_lease,
            mutations: Cell::new(0),
            #[cfg(test)]
            defer_launcher_flush: Cell::new(false),
        });
        for store in service.stores.lock().await.values() {
            service.publish(store)?;
        }
        hub.borrow_mut().publish(
            "settings.desktop",
            serde_json::json!({"available":false,"data":{},"monitors":[],"pending":false,"rev":0}),
        )?;
        Ok(service)
    }

    fn publish(&self, store: &Store) -> Result<()> {
        self.hub
            .upgrade()
            .ok_or("Settings hub ended")?
            .borrow_mut()
            .publish(
                &topic(&store.schema.id),
                serde_json::to_value(&store.snapshot)?,
            )?;
        Ok(())
    }

    pub async fn get(&self, request: SettingsGet) -> std::result::Result<Value, Failure> {
        if !nacre_protocol::settings::store_id(&request.store) {
            return Err(denied());
        }
        let stores = self.stores.lock().await;
        let store = stores.get(&request.store).ok_or_else(denied)?;
        serde_json::to_value(&store.snapshot).map_err(|_| unavailable())
    }

    pub async fn set(&self, request: SettingsSet) -> std::result::Result<Value, Failure> {
        request.validate()?;
        if request.store == "desktop" {
            if request.key.len() != 1
                || validation::descriptor("desktop", &request.key)
                    .is_none_or(|s| !validation::configured_value(s, &request.value))
            {
                return Err(denied());
            }
            let result = self
                .desktop(DesktopSettingsCall {
                    arguments: vec![
                        "set".into(),
                        request.key[0].clone(),
                        serde_json::to_string(&request.value).map_err(|_| denied())?,
                    ],
                    rev: Some(request.rev),
                })
                .await?;
            let accepted = self
                .get(SettingsGet {
                    store: "desktop".into(),
                })
                .await?;
            if accepted["rev"] != result["rev"] {
                return Err(failure(
                    ErrorCode::StaleRevision,
                    "Desktop settings changed before the operation could be confirmed",
                    true,
                ));
            }
            return Ok(accepted);
        }
        let mut stores = self.stores.lock().await;
        let store = stores.get_mut(&request.store).ok_or_else(denied)?;
        if store
            .schema
            .settings
            .as_ref()
            .is_none_or(|s| s.access != state::SettingsMode::Write)
        {
            return Err(denied());
        }
        if request.rev != store.snapshot.rev {
            return Err(failure(
                ErrorCode::StaleRevision,
                "Settings changed; refresh before writing",
                true,
            ));
        }
        if store.snapshot.error.is_some() {
            return Err(failure(
                ErrorCode::Unavailable,
                "Resolve the settings file error before writing",
                false,
            ));
        }
        let descriptor = validation::descriptor(&request.store, &request.key);
        let history_reset =
            request.store == "launcher" && validation::history_reset(&request.key, &request.value);
        if descriptor.is_none() && !history_reset {
            return Err(denied());
        }
        if descriptor.is_some_and(|descriptor| !validation::valid_value(descriptor, &request.value))
        {
            return Err(failure(
                ErrorCode::InvalidRequest,
                "Invalid settings value",
                false,
            ));
        }
        let next = store
            .snapshot
            .rev
            .checked_add(1)
            .filter(|rev| *rev <= MAX_SAFE_INTEGER)
            .ok_or_else(unavailable)?;
        let mut data = store.snapshot.data.clone();
        let mut node = &mut data;
        for key in &request.key[..request.key.len() - 1] {
            if !node.is_object() {
                return Err(denied());
            }
            let object = node.as_object_mut().ok_or_else(denied)?;
            node = object.entry(key).or_insert_with(|| serde_json::json!({}));
        }
        node.as_object_mut().ok_or_else(denied)?.insert(
            request.key.last().ok_or_else(denied)?.clone(),
            request.value,
        );
        let previous = store.snapshot.clone();
        let previous_deadline = store.deadline;
        store.snapshot.data = data;
        store.snapshot.rev = next;
        store.snapshot.dirty = true;
        store.deadline = Some(Instant::now() + QUIET);
        if self.publish(store).is_err() {
            store.snapshot = previous;
            store.deadline = previous_deadline;
            return Err(failure(
                ErrorCode::TooLarge,
                "Settings document exceeds publication limits",
                false,
            ));
        }
        self.changed.notify_one();
        self.durable_changed.notify_waiters();
        serde_json::to_value(&store.snapshot).map_err(|_| unavailable())
    }

    pub async fn publish_owned(
        &self,
        request: SettingsPublish,
    ) -> std::result::Result<Value, Failure> {
        request.validate()?;
        self.publish_document(request, false).await
    }

    pub async fn provider_write(
        &self,
        request: SettingsProviderWrite,
    ) -> std::result::Result<Value, Failure> {
        request.validate()?;
        self.publish_document(
            SettingsPublish {
                store: request.store,
                value: request.value,
                rev: request.rev,
            },
            true,
        )
        .await
    }

    async fn publish_document(
        &self,
        request: SettingsPublish,
        provider: bool,
    ) -> std::result::Result<Value, Failure> {
        let mut stores = self.stores.lock().await;
        let store = stores.get_mut(&request.store).ok_or_else(denied)?;
        if request.rev != store.snapshot.rev {
            return Err(failure(
                ErrorCode::StaleRevision,
                "Settings changed; refresh before writing",
                true,
            ));
        }
        if store.snapshot.error.is_some() {
            return Err(unavailable());
        }
        let incoming = request.value.as_object().ok_or_else(denied)?;
        if incoming.get("version").and_then(Value::as_u64) != Some(store.schema.version) {
            return Err(failure(
                ErrorCode::InvalidRequest,
                "Unsupported publisher document version",
                false,
            ));
        }
        let mut data = store.snapshot.data.clone();
        let object = data.as_object_mut().ok_or_else(denied)?;
        if provider {
            object.retain(|key, _| {
                incoming.contains_key(key) || !validation::provider_key(&request.store, key)
            });
        }
        for (key, value) in incoming {
            if key == "version" {
                object.insert(key.clone(), value.clone());
                continue;
            }
            let valid = if provider {
                object.get(key) == Some(value)
                    || validation::provider_change(&request.store, key, value, object.get(key))
            } else if request.store == "battery-alerts" {
                key == "severity" && value.as_u64().is_some_and(|n| n <= 2)
            } else {
                validation::descriptor(&request.store, std::slice::from_ref(key))
                    .is_some_and(|d| validation::valid_value(d, value))
            };
            if !valid {
                return Err(denied());
            }
            let mut value = value.clone();
            if provider && request.store == "colors" && key == "appThemes" {
                if let (Some(old), Some(next)) = (
                    object.get(key).and_then(Value::as_object),
                    value.as_object_mut(),
                ) {
                    for (key, value) in old {
                        if ![
                            "gtk3", "gtk4", "qt", "kde", "kitty", "rofi", "lock", "login",
                        ]
                        .contains(&key.as_str())
                        {
                            next.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
            object.insert(key.clone(), value);
        }
        if !validation::document(&request.store, &data) {
            return Err(denied());
        }
        let previous = store.snapshot.clone();
        let previous_deadline = store.deadline;
        store.snapshot.data = data;
        store.snapshot.rev = store
            .snapshot
            .rev
            .checked_add(1)
            .filter(|r| *r <= MAX_SAFE_INTEGER)
            .ok_or_else(unavailable)?;
        store.snapshot.dirty = true;
        store.deadline = Some(Instant::now() + QUIET);
        if self.publish(store).is_err() {
            store.snapshot = previous;
            store.deadline = previous_deadline;
            return Err(failure(
                ErrorCode::TooLarge,
                "Settings document exceeds publication limits",
                false,
            ));
        }
        self.changed.notify_one();
        self.durable_changed.notify_waiters();
        serde_json::to_value(&store.snapshot).map_err(|_| unavailable())
    }

    pub async fn desktop(
        &self,
        request: DesktopSettingsCall,
    ) -> std::result::Result<Value, Failure> {
        request.validate()?;
        let mut stores = self.stores.lock().await;
        let store = stores.get_mut("desktop").ok_or_else(unavailable)?;
        if request.rev.is_some_and(|rev| rev != store.snapshot.rev) {
            return Err(failure(
                ErrorCode::StaleRevision,
                "Settings changed; refresh before writing",
                true,
            ));
        }
        if request.arguments[0] != "state" && store.snapshot.error.is_some() {
            return Err(failure(
                ErrorCode::Unavailable,
                "Resolve the settings file error before writing",
                false,
            ));
        }
        if store.snapshot.rev >= MAX_SAFE_INTEGER {
            return Err(unavailable());
        }
        let control = self
            .hub
            .upgrade()
            .ok_or_else(unavailable)?
            .borrow()
            .compositor
            .clone()
            .ok_or_else(unavailable)?;
        let (result, written) = super::settings_controller::execute(
            control,
            self.paths.clone(),
            store.snapshot.data.clone(),
            store.snapshot.exists || store.snapshot.dirty,
            request.arguments,
        )
        .await
        .map_err(|_| {
            failure(
                ErrorCode::Unavailable,
                "Desktop settings controller is unavailable",
                false,
            )
        })?;
        if let Some(data) = written {
            store.snapshot.data = data;
            store.snapshot.rev += 1;
            store.snapshot.dirty = true;
            store.deadline = Some(Instant::now() + QUIET);
            self.publish(store).map_err(|_| unavailable())?;
            self.changed.notify_one();
            self.durable_changed.notify_waiters();
        }
        let mut result =
            result.map_err(|_| failure(ErrorCode::Io, "Desktop settings action failed", false))?;
        result["available"] = Value::Bool(true);
        result["rev"] = serde_json::json!(store.snapshot.rev);
        if let Some(error) = &store.snapshot.error {
            result["error"] = Value::String(error.message.clone());
        }
        self.hub
            .upgrade()
            .ok_or_else(unavailable)?
            .borrow_mut()
            .publish("settings.desktop", result.clone())?;
        Ok(result)
    }

    pub async fn launcher_preferences(
        &self,
        action: &str,
        value: &Value,
    ) -> std::result::Result<Value, Failure> {
        if action == "load" {
            let stores = self.stores.lock().await;
            let store = stores.get("launcher").ok_or_else(unavailable)?;
            return crate::launcher_preferences::normalized(&store.snapshot.data)
                .map_err(|e| failure(ErrorCode::Validation, &e.to_string(), false));
        }
        let admission = self.admit_launcher_preferences(action, value).await?;
        self.wait_launcher_preferences_admission(&admission).await?;
        Ok(admission.result)
    }

    /// Admit launcher frecency under the one settings owner without waiting for
    /// disk publication. The serial menu actor uses this before dispatch, then
    /// observes persistence in its bounded JoinSet.
    pub(crate) async fn admit_launcher_preferences(
        &self,
        action: &str,
        value: &Value,
    ) -> std::result::Result<LauncherPreferencesAdmission, Failure> {
        if action == "load" {
            return Err(failure(
                ErrorCode::InvalidRequest,
                "Launcher preference load is not a mutation",
                false,
            ));
        }
        let (result, revision) = {
            let mut stores = self.stores.lock().await;
            let store = stores.get_mut("launcher").ok_or_else(unavailable)?;
            let normalized = crate::launcher_preferences::normalized(&store.snapshot.data)
                .map_err(|e| failure(ErrorCode::Validation, &e.to_string(), false))?;
            if store.snapshot.error.is_some() {
                return Err(failure(
                    ErrorCode::Validation,
                    "Invalid launcher preferences; original file preserved",
                    false,
                ));
            }
            let result = crate::launcher_preferences::changed(normalized, action, value)
                .map_err(|e| failure(ErrorCode::Validation, &e.to_string(), false))?;
            let next = store
                .snapshot
                .rev
                .checked_add(1)
                .filter(|rev| *rev <= MAX_SAFE_INTEGER)
                .ok_or_else(unavailable)?;
            let previous = store.snapshot.clone();
            let deadline = store.deadline;
            for key in ["favorites", "hidden", "recent", "usage"] {
                store.snapshot.data[key] = result[key].clone();
            }
            store.snapshot.rev = next;
            store.snapshot.dirty = true;
            #[cfg(test)]
            let quiet = if self.defer_launcher_flush.get() {
                Duration::from_secs(3600)
            } else {
                QUIET
            };
            #[cfg(not(test))]
            let quiet = QUIET;
            store.deadline = Some(Instant::now() + quiet);
            if self.publish(store).is_err() {
                store.snapshot = previous;
                store.deadline = deadline;
                return Err(failure(
                    ErrorCode::TooLarge,
                    "Launcher preferences exceed publication limits",
                    false,
                ));
            }
            self.changed.notify_one();
            self.durable_changed.notify_waiters();
            (result, next)
        };
        Ok(LauncherPreferencesAdmission { result, revision })
    }

    pub(crate) async fn wait_launcher_preferences_admission(
        &self,
        admission: &LauncherPreferencesAdmission,
    ) -> std::result::Result<(), Failure> {
        self.wait_launcher_preferences(admission.revision, &admission.result)
            .await
    }

    async fn wait_launcher_preferences(
        &self,
        revision: u64,
        expected: &Value,
    ) -> std::result::Result<(), Failure> {
        let wait = async {
            loop {
                // Register before reading the state so a flush between the
                // snapshot check and await cannot be lost.
                let changed = self.durable_changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                let stores = self.stores.lock().await;
                let store = stores.get("launcher").ok_or_else(unavailable)?;
                if store.snapshot.error.is_some() {
                    return Err(failure(
                        ErrorCode::Io,
                        "Launcher preference could not be saved; original preserved",
                        false,
                    ));
                }
                let current = &store.snapshot;
                let matches = ["favorites", "hidden", "recent", "usage"]
                    .into_iter()
                    .all(|key| current.data.get(key) == expected.get(key));
                if current.rev >= revision && !matches {
                    return Err(failure(
                        ErrorCode::StaleRevision,
                        "Launcher preferences were superseded before durable confirmation",
                        false,
                    ));
                }
                if current.rev >= revision && !current.dirty {
                    return Ok(());
                }
                drop(stores);
                changed.await;
            }
        };
        tokio::time::timeout(Duration::from_secs(10), wait)
            .await
            .map_err(|_| {
                failure(
                    ErrorCode::Timeout,
                    "Launcher preference save timed out; it may have completed",
                    false,
                )
            })?
    }

    async fn refresh(&self) -> Result<()> {
        let mut stores = self.stores.lock().await;
        for store in stores.values_mut() {
            let path = store.path.clone();
            let schema = store.schema;
            let read =
                tokio::task::spawn_blocking(move || state::read_document(&path, schema)).await?;
            match read {
                Ok((data, fingerprint)) if validation::document(&schema.id, &data) => {
                    if fingerprint == store.fingerprint && store.snapshot.error.is_none() {
                        continue;
                    }
                    store.snapshot.rev = store
                        .snapshot
                        .rev
                        .checked_add(1)
                        .filter(|rev| *rev <= MAX_SAFE_INTEGER)
                        .ok_or("Settings revision exhausted")?;
                    store.snapshot.data = data;
                    store.snapshot.exists = fingerprint.is_some();
                    store.snapshot.dirty = false;
                    store.snapshot.error = None;
                    store.deadline = None;
                    store.fingerprint = fingerprint;
                }
                result => {
                    let (line, message) = result.err().unwrap_or((0, "Invalid settings value"));
                    store.snapshot.error = Some(read_error(schema, line, message));
                    store.snapshot.dirty = false;
                    store.deadline = None;
                    tracing::warn!(store = %schema.id, line, "Invalid hand edit; retaining last good settings");
                }
            }
            self.publish(store)?;
            self.durable_changed.notify_waiters();
        }
        Ok(())
    }

    async fn flush(&self) -> Result<()> {
        if let Some(lease) = &self.writer_lease {
            lease.ensure_valid()?;
        }
        let mut stores = self.stores.lock().await;
        for store in stores
            .values_mut()
            .filter(|s| s.deadline.is_some_and(|d| d <= Instant::now()))
        {
            let path = store.path.clone();
            let schema = store.schema;
            let data = store.snapshot.data.clone();
            let expected = store.fingerprint;
            let lease = self.writer_lease.clone();
            let result = tokio::task::spawn_blocking(move || -> Result<_> {
                let written = if let Some(lease) = lease.as_ref() {
                    if !lease.contains_path(&path) {
                        return Err("Settings path is outside the leased settings roots".into());
                    }
                    state::write_checked_pinned(lease, &path, schema, &data, expected)?;
                    state::read_document_pinned(lease, &path, schema)
                } else {
                    state::write_checked(&path, schema, &data, expected)?;
                    state::read_document(&path, schema)
                };
                written.map_err(|_| "Cannot verify saved settings".into())
            })
            .await?;
            store.deadline = None;
            match result {
                Ok((data, fingerprint)) => {
                    if !validation::document(&schema.id, &data) {
                        store.snapshot.error =
                            Some(read_error(schema, 0, "Invalid settings value"));
                    } else {
                        // A hand edit immediately after atomic publication also wins.
                        if data != store.snapshot.data {
                            store.snapshot.rev += 1;
                        }
                        store.snapshot.data = data;
                        store.fingerprint = fingerprint;
                        store.snapshot.exists = fingerprint.is_some();
                        store.snapshot.error = None;
                    }
                    store.snapshot.dirty = false;
                }
                Err(error) => {
                    store.snapshot.error = Some(read_error(
                        schema,
                        0,
                        if error.downcast_ref::<state::WriteConflict>().is_some() {
                            "Settings changed on disk; refresh before writing"
                        } else {
                            "Cannot safely save settings"
                        },
                    ));
                    store.snapshot.dirty = false;
                    tracing::error!(store = %schema.id, error = ?error, "Settings publication failed; original file preserved");
                }
            }
            self.publish(store)?;
            self.durable_changed.notify_waiters();
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn defer_launcher_flush_for_test(&self) {
        self.defer_launcher_flush.set(true);
    }

    #[cfg(test)]
    pub(crate) async fn flush_for_test(&self) -> Result<()> {
        if let Some(store) = self.stores.lock().await.get_mut("launcher") {
            store.deadline = Some(Instant::now());
        }
        self.flush().await
    }
}

pub fn methods() -> BTreeMap<String, Method> {
    [
        (
            "settings.get",
            include_str!("../../../../protocol/schema/settings-get.json"),
        ),
        (
            "settings.set",
            include_str!("../../../../protocol/schema/settings-set.json"),
        ),
        (
            "settings.publish",
            include_str!("../../../../protocol/schema/settings-publish.json"),
        ),
        (
            "settings.provider",
            include_str!("../../../../protocol/schema/settings-provider-write.json"),
        ),
        (
            "settings.desktop",
            include_str!("../../../../protocol/schema/desktop-settings-call.json"),
        ),
    ]
    .into_iter()
    .map(|(name, params)| {
        (
            name.into(),
            Method {
                v: 1,
                class: nacre_protocol::Capability::Desktop,
                timeout: 30000,
                params: serde_json::from_str(params).expect("checked-in settings schema"),
                result: if name == "settings.desktop" {
                    serde_json::json!({"type":"object"})
                } else {
                    serde_json::from_str(include_str!(
                        "../../../../protocol/schema/settings-store.json"
                    ))
                    .expect("checked-in store schema")
                },
            },
        )
    })
    .collect()
}

pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    if method != "settings.get"
        && !(method == "settings.desktop" && params["arguments"][0] == "state")
        && hub
            .borrow()
            .services
            .get("settings")
            .is_none_or(|service| service["status"] != "running")
    {
        return Err(unavailable());
    }
    let service = hub.borrow().settings.clone().ok_or_else(unavailable)?;
    let invalid = || failure(ErrorCode::InvalidRequest, "Invalid settings request", false);
    enum Mutation {
        Set(SettingsSet),
        Publish(SettingsPublish),
        Provider(SettingsProviderWrite),
        Desktop(DesktopSettingsCall),
    }
    let mutation = match method {
        "settings.get" => {
            return service
                .get(serde_json::from_value(params).map_err(|_| invalid())?)
                .await;
        }
        "settings.set" => {
            let request: SettingsSet = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            Mutation::Set(request)
        }
        "settings.publish" => {
            let request: SettingsPublish = serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            Mutation::Publish(request)
        }
        "settings.provider" => {
            let request: SettingsProviderWrite =
                serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            Mutation::Provider(request)
        }
        "settings.desktop" => {
            let request: DesktopSettingsCall =
                serde_json::from_value(params).map_err(|_| invalid())?;
            request.validate()?;
            Mutation::Desktop(request)
        }
        _ => {
            return Err(failure(
                ErrorCode::UnknownMethod,
                "Unknown settings method",
                false,
            ))
        }
    };
    if service.mutations.get() >= 64 {
        return Err(
            failure(ErrorCode::Busy, "Settings mutation queue is full", true).dispatch_state(false),
        );
    }
    service.mutations.set(service.mutations.get() + 1);
    struct Pending(Rc<Service>);
    impl Drop for Pending {
        fn drop(&mut self) {
            self.0.mutations.set(self.0.mutations.get() - 1);
        }
    }
    // Dropping a caller's JoinHandle detaches this bounded task. It keeps the
    // store lock until any physical effect and its cache publication finish.
    tokio::task::spawn_local(async move {
        let _pending = Pending(service.clone());
        match mutation {
            Mutation::Set(request) => service.set(request).await,
            Mutation::Publish(request) => service.publish_owned(request).await,
            Mutation::Provider(request) => service.provider_write(request).await,
            Mutation::Desktop(request) => service.desktop(request).await,
        }
    })
    .await
    .map_err(|_| {
        failure(ErrorCode::Internal, "Settings action failed", false).dispatch_state(true)
    })?
}

pub async fn run(
    hub: Shared,
    service: Rc<Service>,
    ready: Rc<RefCell<Option<tokio::sync::oneshot::Sender<()>>>>,
) -> Result<()> {
    let watcher = Watcher::new(&service).await?;
    service.refresh().await?;
    let failures = hub
        .borrow()
        .services
        .get("settings")
        .and_then(|s| s["failures"].as_u64())
        .unwrap_or(0) as usize;
    hub.borrow_mut().service("settings", "running", failures)?;
    if let Some(ready) = ready.borrow_mut().take() {
        if ready.send(()).is_err() {
            tracing::debug!("Settings readiness observer ended");
        }
    }
    loop {
        let deadline = service
            .stores
            .lock()
            .await
            .values()
            .filter_map(|s| s.deadline)
            .min();
        let timer = async {
            match deadline {
                Some(deadline) => tokio::time::sleep_until(deadline).await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            _ = service.changed.notified() => {},
            _ = timer => service.flush().await?,
            result = watcher.changed() => { result?; service.refresh().await?; watcher.rearm(&service).await?; },
        }
    }
}

struct Watcher {
    fd: tokio::io::unix::AsyncFd<rustix::fd::OwnedFd>,
}
impl Watcher {
    #[allow(unsafe_code)]
    async fn new(service: &Service) -> Result<Self> {
        let fd = rustix::fs::inotify::init(
            rustix::fs::inotify::CreateFlags::NONBLOCK | rustix::fs::inotify::CreateFlags::CLOEXEC,
        )?;
        let watcher = Self {
            // SAFETY: OwnedFd owns this nonblocking inotify descriptor. It is
            // never replaced, closed, or borrowed beyond the AsyncFd's lifetime.
            fd: unsafe {
                tokio::io::unix::AsyncFd::register_with_interest(fd, tokio::io::Interest::READABLE)?
            },
        };
        watcher.rearm(service).await?;
        Ok(watcher)
    }
    async fn rearm(&self, service: &Service) -> Result<()> {
        let stores = service.stores.lock().await;
        let directories: std::collections::BTreeSet<_> = stores
            .values()
            .filter_map(|s| s.path.parent())
            .filter_map(|p| p.ancestors().find(|p| p.is_dir()))
            .collect();
        for path in directories {
            rustix::fs::inotify::add_watch(
                self.fd.get_ref(),
                path,
                rustix::fs::inotify::WatchFlags::from_bits_retain(
                    0x8 | 0x80 | 0x100 | 0x200 | 0x400 | 0x800,
                ),
            )?;
        }
        Ok(())
    }
    async fn changed(&self) -> Result<()> {
        let mut buffer = [0u8; 8192];
        loop {
            let mut ready = self.fd.readable().await?;
            match ready.try_io(|fd| {
                rustix::io::read(fd.get_ref(), &mut buffer).map_err(std::io::Error::from)
            }) {
                Ok(result) => {
                    result?;
                    return Ok(());
                }
                Err(_) => continue,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    async fn fixture() -> (tempfile::TempDir, Paths, Shared, Rc<Service>) {
        let home = tempfile::tempdir().unwrap();
        let paths = Paths::new(home.path());
        std::fs::create_dir_all(&paths.config).unwrap();
        std::fs::create_dir_all(&paths.state).unwrap();
        let hub =
            super::super::hub::Hub::new("settings-test".into(), false, Duration::from_millis(16))
                .shared();
        let lease = Arc::new(WriterLease::acquire_for_paths(&paths.config, &paths.state).unwrap());
        let service = Service::load(hub.clone(), paths.clone(), Some(lease))
            .await
            .unwrap();
        (home, paths, hub, service)
    }
    fn change(rev: u64, value: bool) -> SettingsSet {
        SettingsSet {
            store: "power".into(),
            key: vec!["enabled".into()],
            value: json!(value),
            rev,
        }
    }
    #[tokio::test(flavor = "current_thread")]
    async fn providers_share_revision_preserve_unknown_fields_and_never_gain_other_stores() {
        let (_home, paths, _hub, service) = fixture().await;
        std::fs::write(
            paths.config.join("colors.json"),
            r#"{"version":1,"future":{"kept":true},"favorites":[]}"#,
        )
        .unwrap();
        service.refresh().await.unwrap();
        let before = service
            .get(SettingsGet {
                store: "colors".into(),
            })
            .await
            .unwrap();
        let request = |store: &str, value: Value, rev: u64| SettingsProviderWrite {
            store: store.into(),
            value,
            rev,
        };
        let result = service
            .provider_write(request(
                "colors",
                json!({"version":1,"favorites":[{"id":"fixture"}],"history":[]}),
                before["rev"].as_u64().unwrap(),
            ))
            .await
            .unwrap();
        assert_eq!(result["data"]["future"]["kept"], true);
        assert_eq!(
            service
                .provider_write(request(
                    "colors",
                    json!({"version":1,"favorites":[]}),
                    before["rev"].as_u64().unwrap()
                ))
                .await
                .unwrap_err()
                .code,
            ErrorCode::StaleRevision
        );
        let rev = result["rev"].as_u64().unwrap();
        for value in [
            json!({"version":1,"future":{"kept":false}}),
            json!({"version":2,"favorites":[]}),
            json!({"version":1,"favorites":"invalid"}),
            json!({"version":1,"run":["tool"]}),
        ] {
            assert!(service
                .provider_write(request("colors", value, rev))
                .await
                .is_err());
        }
        for store in [
            "desktop",
            "profile",
            "brain-auth",
            "software-input",
            "battery-alerts",
        ] {
            assert!(service
                .provider_write(request(store, json!({"version":1}), 0))
                .await
                .is_err());
        }
        let rollback = service
            .provider_write(request("colors", json!({"version":1,"favorites":[]}), rev))
            .await
            .unwrap();
        assert!(rollback["data"].get("history").is_none());
        assert_eq!(rollback["data"]["future"]["kept"], true);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn settings_failed_publication_never_acknowledges_success() {
        for external_conflict in [false, true] {
            let (_home, paths, _hub, service) = fixture().await;
            let accepted = service.set(change(0, true)).await.unwrap();
            let target = accepted["rev"].as_u64().unwrap();
            let expected = [crate::state::FieldExpectation::from_data(
                &accepted["data"],
                vec!["enabled".into()],
            )];
            if external_conflict {
                let path = state::settings_path_in(&paths, "power").unwrap();
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(path, r#"{"version":1,"enabled":false}"#).unwrap();
            } else {
                let mut stores = service.stores.lock().await;
                stores.get_mut("power").unwrap().path = paths.config.clone();
            }
            tokio::time::sleep(QUIET + Duration::from_millis(10)).await;
            service.flush().await.unwrap();
            let failed = service
                .get(SettingsGet {
                    store: "power".into(),
                })
                .await
                .unwrap();
            assert_eq!(failed["dirty"], false);
            assert!(!failed["error"].is_null());
            assert!(!crate::state::durable_snapshot_matches(
                &failed, target, &expected
            ));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn menu_preferences_reply_waits_for_flush() {
        let (_home, _paths, _hub, service) = fixture().await;
        let input = json!({"id":"fixture.desktop","enabled":true});
        let mut pending = Box::pin(service.launcher_preferences("favorite", &input));
        tokio::select! {
            result = &mut pending => panic!("preference returned before flush: {result:?}"),
            _ = tokio::time::sleep(Duration::from_millis(40)) => {},
        }
        let accepted = service
            .get(SettingsGet {
                store: "launcher".into(),
            })
            .await
            .unwrap();
        assert_eq!(accepted["dirty"], true);
        tokio::time::sleep(QUIET + Duration::from_millis(10)).await;
        service.flush().await.unwrap();
        let result = pending.await.unwrap();
        assert_eq!(result["favorites"], json!(["fixture.desktop"]));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn menu_preferences_superseded_returns_conflict() {
        let (_home, _paths, _hub, service) = fixture().await;
        let first_input = json!({"id":"fixture.desktop","enabled":true});
        let mut first = Box::pin(service.launcher_preferences("favorite", &first_input));
        tokio::select! {
            result = &mut first => panic!("first preference unexpectedly returned: {result:?}"),
            _ = tokio::time::sleep(Duration::from_millis(2)) => {},
        }
        let second_input = json!({"id":"fixture.desktop","enabled":false});
        let mut second = Box::pin(service.launcher_preferences("favorite", &second_input));
        tokio::select! {
            result = &mut second => panic!("second preference unexpectedly returned: {result:?}"),
            _ = tokio::time::sleep(Duration::from_millis(2)) => {},
        }
        assert_eq!(first.await.unwrap_err().code, ErrorCode::StaleRevision);
        tokio::time::sleep(QUIET + Duration::from_millis(10)).await;
        service.flush().await.unwrap();
        assert_eq!(second.await.unwrap()["favorites"], json!([]));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn menu_preferences_flush_failure_returns_error() {
        let (_home, paths, _hub, service) = fixture().await;
        {
            let mut stores = service.stores.lock().await;
            stores.get_mut("launcher").unwrap().path = paths.config.clone();
        }
        let input = json!({"id":"fixture.desktop","enabled":true});
        let mut pending = Box::pin(service.launcher_preferences("favorite", &input));
        tokio::select! {
            result = &mut pending => panic!("preference unexpectedly returned: {result:?}"),
            _ = tokio::time::sleep(Duration::from_millis(2)) => {},
        }
        tokio::time::sleep(QUIET + Duration::from_millis(10)).await;
        service.flush().await.unwrap();
        assert_eq!(pending.await.unwrap_err().code, ErrorCode::Io);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn menu_preferences_wait_releases_actor() {
        let (_home, _paths, _hub, service) = fixture().await;
        let input = json!({"id":"fixture.desktop","enabled":true});
        let mut pending = Box::pin(service.launcher_preferences("favorite", &input));
        tokio::select! {
            result = &mut pending => panic!("preference unexpectedly returned: {result:?}"),
            _ = tokio::time::sleep(Duration::from_millis(2)) => {},
        }
        let other = service.set(change(0, true)).await.unwrap();
        assert_eq!(other["dirty"], true);
        tokio::time::sleep(QUIET + Duration::from_millis(10)).await;
        service.flush().await.unwrap();
        assert!(pending.await.is_ok());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn menu_preferences_wait_observes_flush_completed_before_registration() {
        let (_home, _paths, _hub, service) = fixture().await;
        let expected = json!({
            "favorites":["fixture.desktop"],
            "hidden":[],
            "recent":[],
            "usage":{}
        });
        {
            let mut stores = service.stores.lock().await;
            let store = stores.get_mut("launcher").unwrap();
            store.snapshot.data = expected.clone();
            store.snapshot.rev = 1;
            store.snapshot.dirty = false;
            store.snapshot.error = None;
        }
        // This is the lost-wakeup boundary: flush has already published the
        // accepted revision before the waiter registers its notification.
        service
            .wait_launcher_preferences(1, &expected)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn existing_publishers_share_revisions_preserve_unknown_fields_and_refuse_other_stores() {
        let (_home, paths, _hub, service) = fixture().await;
        let request = |store: &str, value: Value, rev| SettingsPublish {
            store: store.into(),
            value,
            rev,
        };
        let result = service
            .publish_owned(request(
                "control-center",
                json!({"version":2,"tiles":["vpn"]}),
                0,
            ))
            .await
            .unwrap();
        assert_eq!(result["rev"], 1);
        let stale = service
            .publish_owned(request(
                "control-center",
                json!({"version":2,"tiles":[]}),
                0,
            ))
            .await
            .unwrap_err();
        assert_eq!(stale.code, ErrorCode::StaleRevision);
        for (id, value) in [
            ("desktop", json!({"version":3,"dnd":true})),
            ("battery-alerts", json!({"version":1,"severity":3})),
            ("battery-alerts", json!({"version":1,"severity":true})),
            ("battery-alerts", json!({"version":2,"severity":1})),
            ("control-center", json!({"version":2,"command":"arbitrary"})),
            ("dashboard-extras", json!({"version":1,"weatherPhotos":0})),
        ] {
            assert!(
                service.publish_owned(request(id, value, 0)).await.is_err(),
                "{id}"
            );
        }
        std::fs::write(
            paths.state.join("battery-alert-state.json"),
            r#"{"version":1,"severity":1,"future":"retained"}"#,
        )
        .unwrap();
        service.refresh().await.unwrap();
        let result = service
            .publish_owned(request(
                "battery-alerts",
                json!({"version":1,"severity":2}),
                1,
            ))
            .await
            .unwrap();
        assert_eq!(result["data"]["future"], "retained");
        assert!(service
            .set(SettingsSet {
                store: "battery-alerts".into(),
                key: vec!["severity".into()],
                value: json!(0),
                rev: 2
            })
            .await
            .is_err());
        service.publish_owned(request("dashboard-extras",json!({"version":1,"enabled":["calendar"],"order":["calendar"],"weatherPhotos":false}),0)).await.unwrap();
        tokio::time::sleep(QUIET + Duration::from_millis(20)).await;
        service.flush().await.unwrap();
        let bytes = std::fs::read(paths.state.join("battery-alert-state.json")).unwrap();
        let saved: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(saved["severity"], 2);
        assert_eq!(saved["future"], "retained");
    }

    #[tokio::test]
    async fn losing_a_caller_does_not_cancel_an_accepted_mutation() {
        tokio::task::LocalSet::new()
            .run_until(async {
                let (_home, _paths, hub, service) = fixture().await;
                hub.borrow_mut().settings = Some(service.clone());
                hub.borrow_mut().service("settings", "running", 0).unwrap();
                let held = service.stores.lock().await;
                let caller = tokio::task::spawn_local(dispatch(
                    hub.clone(),
                    "settings.set",
                    json!({"store":"power","key":["enabled"],"value":true,"rev":0}),
                ));
                tokio::task::yield_now().await;
                assert_eq!(service.mutations.get(), 1);
                caller.abort();
                assert!(caller.await.unwrap_err().is_cancelled());
                drop(held);
                tokio::task::yield_now().await;
                let value = service
                    .get(SettingsGet {
                        store: "power".into(),
                    })
                    .await
                    .unwrap();
                assert_eq!(value["data"]["enabled"], true);
                assert_eq!(value["rev"], 1);
                assert_eq!(service.mutations.get(), 0);
                service.mutations.set(64);
                let error = dispatch(
                    hub,
                    "settings.set",
                    json!({"store":"power","key":["enabled"],"value":false,"rev":1}),
                )
                .await
                .unwrap_err();
                assert_eq!(error.code, ErrorCode::Busy);
                assert_eq!(error.dispatched, Some(false));
                service.mutations.set(0);
            })
            .await;
    }

    #[tokio::test]
    async fn launcher_frecency_and_settings_share_a_single_revisioned_cache() {
        let (_home, paths, _hub, service) = fixture().await;
        let path = state::settings_path_in(&paths, "launcher").unwrap();
        std::fs::write(&path, "{\"favorites\":[],\"hidden\":[],\"unknown\":42}").unwrap();
        service.refresh().await.unwrap();
        service
            .set(SettingsSet {
                store: "launcher".into(),
                key: vec!["favorites".into()],
                value: json!(["fixture.desktop"]),
                rev: 1,
            })
            .await
            .unwrap();
        let input = json!({"id":"fixture.desktop","enabled":true});
        let mut pending = Box::pin(service.launcher_preferences("record", &input));
        tokio::select! {
            result = &mut pending => panic!("frecency returned before flush: {result:?}"),
            _ = tokio::time::sleep(Duration::from_millis(40)) => {},
        }
        assert_eq!(
            service
                .get(SettingsGet {
                    store: "launcher".into(),
                })
                .await
                .unwrap()["dirty"],
            true
        );
        tokio::time::sleep(QUIET + Duration::from_millis(10)).await;
        service.flush().await.unwrap();
        let result = pending.await.unwrap();
        assert_eq!(result["favorites"], json!(["fixture.desktop"]));
        assert_eq!(result["recent"], json!(["fixture.desktop"]));
        assert_eq!(result["usage"]["fixture.desktop"]["count"], 1);
        for (rev, key, value) in [(3, "recent", json!([])), (4, "usage", json!({}))] {
            service
                .set(SettingsSet {
                    store: "launcher".into(),
                    key: vec![key.into()],
                    value,
                    rev,
                })
                .await
                .unwrap();
        }
        assert!(service
            .set(SettingsSet {
                store: "launcher".into(),
                key: vec!["usage".into()],
                value: json!({"forged":{"count":999}}),
                rev: 5
            })
            .await
            .is_err());
        tokio::time::sleep(QUIET + Duration::from_millis(5)).await;
        service.flush().await.unwrap();
        let saved: Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        assert_eq!(saved["favorites"], json!(["fixture.desktop"]));
        assert_eq!(saved["recent"], json!([]));
        assert_eq!(saved["usage"], json!({}));
        assert_eq!(saved["unknown"], 42);
    }

    #[tokio::test]
    async fn concurrent_writers_use_memory_revision_and_one_quiet_publication() {
        let (_home, paths, _hub, service) = fixture().await;
        let (a, b) = tokio::join!(service.set(change(0, true)), service.set(change(0, false)));
        assert!(a.is_ok());
        assert_eq!(b.unwrap_err().code, ErrorCode::StaleRevision);
        assert!(!state::settings_path_in(&paths, "power").unwrap().exists());
        let newest = service.set(change(1, false)).await.unwrap();
        assert_eq!(newest["rev"], 2);
        tokio::time::sleep(QUIET + Duration::from_millis(5)).await;
        service.flush().await.unwrap();
        let read = service
            .get(SettingsGet {
                store: "power".into(),
            })
            .await
            .unwrap();
        assert_eq!(read["data"]["enabled"], false);
        assert_eq!(read["dirty"], false);
        assert_eq!(read["exists"], true);
        assert!(read.get("error").is_none());
    }

    #[tokio::test]
    async fn invalid_hand_edit_keeps_last_good_and_reports_line_without_clobbering() {
        let (_home, paths, _hub, service) = fixture().await;
        service.set(change(0, true)).await.unwrap();
        let path = state::settings_path_in(&paths, "power").unwrap();
        std::fs::write(&path, "{\n invalid\n}").unwrap();
        service.refresh().await.unwrap();
        let value = service
            .get(SettingsGet {
                store: "power".into(),
            })
            .await
            .unwrap();
        assert_eq!(value["data"]["enabled"], true);
        assert_eq!(value["error"]["line"], 2);
        assert_eq!(value["error"]["file"], "nacre/power.json");
        assert!(service.set(change(1, false)).await.is_err());
        tokio::time::sleep(QUIET + Duration::from_millis(5)).await;
        service.flush().await.unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\n invalid\n}");
        std::fs::write(&path, "{\"enabled\":false,\"unknown\":42}").unwrap();
        service.refresh().await.unwrap();
        let value = service
            .get(SettingsGet {
                store: "power".into(),
            })
            .await
            .unwrap();
        assert_eq!(value["data"]["enabled"], false);
        assert_eq!(value["data"]["unknown"], 42);
        assert!(value.get("error").is_none());
    }

    #[tokio::test]
    async fn unobserved_edit_wins_over_a_queued_write() {
        let (_home, paths, _hub, service) = fixture().await;
        service.set(change(0, true)).await.unwrap();
        let path = state::settings_path_in(&paths, "power").unwrap();
        let original = "{\"enabled\":false,\"unknown\":42}";
        std::fs::write(&path, original).unwrap();
        tokio::time::sleep(QUIET + Duration::from_millis(5)).await;
        service.flush().await.unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        service.refresh().await.unwrap();
        let value = service
            .get(SettingsGet {
                store: "power".into(),
            })
            .await
            .unwrap();
        assert_eq!(value["data"]["unknown"], 42);
    }

    #[tokio::test]
    async fn provider_read_only_and_unknown_keys_stay_protected() {
        let (_home, paths, _hub, service) = fixture().await;
        for store in [
            "brain-auth",
            "ai-settings",
            "scheme",
            "software-input",
            "../desktop",
        ] {
            let mut request = change(0, true);
            request.store = store.into();
            assert!(service.set(request).await.is_err(), "{store}");
        }
        let mut request = change(0, true);
        request.key = vec!["unregistered".into()];
        assert_eq!(
            service.set(request).await.unwrap_err().code,
            ErrorCode::PermissionDenied
        );
        request = change(0, true);
        request.value = json!(1);
        assert_eq!(
            service.set(request).await.unwrap_err().code,
            ErrorCode::InvalidRequest
        );
        let path = state::settings_path_in(&paths, "power").unwrap();
        let target = paths.home.join("untouched.json");
        std::fs::write(&target, "{\"enabled\":false}").unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        service.refresh().await.unwrap();
        assert!(service.set(change(0, true)).await.is_err());
        assert_eq!(
            std::fs::read_to_string(target).unwrap(),
            "{\"enabled\":false}"
        );
    }

    #[tokio::test]
    async fn hardlinks_and_future_versions_never_replace_an_original() {
        let (_home, paths, _hub, service) = fixture().await;
        let path = state::settings_path_in(&paths, "power").unwrap();
        let target = paths.home.join("original.json");
        std::fs::write(&target, "{\"enabled\":false}").unwrap();
        std::fs::hard_link(&target, &path).unwrap();
        service.refresh().await.unwrap();
        assert!(service.set(change(0, true)).await.is_err());
        assert_eq!(
            std::fs::read_to_string(&target).unwrap(),
            "{\"enabled\":false}"
        );
        std::fs::remove_file(&path).unwrap();
        std::fs::write(&path, "{\"version\":999,\"enabled\":false}").unwrap();
        service.refresh().await.unwrap();
        assert!(service.set(change(0, true)).await.is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"version\":999,\"enabled\":false}"
        );
    }

    #[tokio::test]
    async fn inotify_observes_atomic_replacement_without_a_polling_timer() {
        let (_home, paths, _hub, service) = fixture().await;
        let watcher = Watcher::new(&service).await.unwrap();
        let path = state::settings_path_in(&paths, "power").unwrap();
        let temporary = paths.config.join("replacement.json");
        std::fs::write(&temporary, "{\"enabled\":true}").unwrap();
        std::fs::rename(temporary, path).unwrap();
        tokio::time::timeout(Duration::from_secs(2), watcher.changed())
            .await
            .unwrap()
            .unwrap();
        service.refresh().await.unwrap();
        assert_eq!(
            service
                .get(SettingsGet {
                    store: "power".into()
                })
                .await
                .unwrap()["data"]["enabled"],
            true
        );
    }
}
