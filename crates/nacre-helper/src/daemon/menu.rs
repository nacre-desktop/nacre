//! One launcher owner: indexed application folders, existing stores and shared ranking.
use super::hub::{failure, Shared};
use crate::{launcher_preferences, paths::Paths, Result};
use nacre_menu::{
    calculator,
    context::{array, Context},
    desktop, model, sources, text,
};
use nacre_protocol::{
    menu::{
        MenuActivate, MenuActivated, MenuConversion, MenuItem, MenuQuery, MenuQueryResult,
        MenuUiActivate, MenuView, UI_ACTIVATE,
    },
    Capability, ErrorCode, Failure, Method,
};
use rustix::fs::inotify;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    fs,
    io::Read,
    mem::MaybeUninit,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{
    io::unix::AsyncFd,
    sync::{mpsc, oneshot},
};

const BASE: &str = include_str!("../../../../nacre/shell-tools/launcher-menu.json");
const DEFAULTS: &str = include_str!("../../../nacre-menu/data/defaults.json");
const WATCH_MASK: u32 = 0x8 | 0x40 | 0x80 | 0x100 | 0x200 | 0x400 | 0x800;
type Response = std::result::Result<Value, Failure>;
struct Request {
    method: String,
    params: Value,
    reply: oneshot::Sender<Response>,
}
#[derive(Clone)]
pub struct Requests(mpsc::Sender<Request>);
impl Requests {
    async fn call(&self, method: &str, params: Value) -> Response {
        let (reply, result) = oneshot::channel();
        self.0
            .try_send(Request {
                method: method.into(),
                params,
                reply,
            })
            .map_err(|e| {
                failure(
                    if matches!(e, mpsc::error::TrySendError::Full(_)) {
                        ErrorCode::Busy
                    } else {
                        ErrorCode::Unavailable
                    },
                    "Launcher request queue is unavailable",
                    true,
                )
            })?;
        result
            .await
            .map_err(|_| failure(ErrorCode::Unavailable, "Launcher service ended", true))?
    }
}
pub fn methods() -> BTreeMap<String, Method> {
    let mut methods = BTreeMap::new();
    for (name, schema, result) in [
        (
            "menu.query",
            include_str!("../../../../protocol/schema/menu-query.json"),
            include_str!("../../../../protocol/schema/menu-query-result.json"),
        ),
        (
            "menu.activate",
            include_str!("../../../../protocol/schema/menu-activate.json"),
            include_str!("../../../../protocol/schema/menu-activated.json"),
        ),
        (
            "menu.preferences",
            "{\"type\":\"object\"}",
            "{\"type\":\"object\"}",
        ),
    ] {
        methods.insert(
            name.into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout: 10000,
                params: serde_json::from_str(schema).expect("committed launcher schema"),
                result: serde_json::from_str(result).expect("committed launcher schema"),
            },
        );
    }
    methods
}
pub async fn dispatch(hub: Shared, method: &str, params: Value, timeout: Option<u32>) -> Response {
    let requests = hub
        .borrow()
        .menu
        .clone()
        .ok_or_else(|| failure(ErrorCode::Unavailable, "Launcher service is indexing", true))?;
    let result = requests.call(method, params).await?;
    // Existing view-owned actions retain their confirmation/polkit UI. The client
    // supplied only an opaque ticket; this row was selected and checked by the owner.
    if method == "menu.activate" && result.get("uiRow").is_some() {
        let params = json!({"row":result["uiRow"],"catalog_rev":result["catalog_rev"]});
        serde_json::from_value::<MenuUiActivate>(params.clone())
            .map_err(|_| ())
            .and_then(|selected| selected.validate().map_err(|_| ()))
            .map_err(|_| {
                failure(
                    ErrorCode::Internal,
                    "Invalid selected launcher action",
                    false,
                )
                .dispatch_state(false)
            })?;
        let (id, receiver) = hub.borrow_mut().forward(UI_ACTIVATE, params, timeout)?;
        let _route = super::session::Route { hub, id };
        return match receiver.await.map_err(|_| {
            failure(ErrorCode::Unavailable, "UI provider disconnected", true).dispatch_state(true)
        })? {
            nacre_protocol::Reply::Ok { ok } => {
                let answer: MenuActivated = serde_json::from_value(ok).map_err(|_| {
                    failure(
                        ErrorCode::Internal,
                        "Invalid launcher UI acknowledgement",
                        false,
                    )
                    .dispatch_state(true)
                })?;
                serde_json::to_value(answer)
                    .map_err(|_| failure(ErrorCode::Internal, "Invalid launcher UI result", false))
            }
            nacre_protocol::Reply::Err { err } => Err(err.dispatch_state(true)),
        };
    }
    Ok(result)
}
fn clock() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
fn read_text(path: &Path) -> Result<String> {
    let file = match fs::File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
        Err(e) => return Err(e.into()),
    };
    let mut bytes = Vec::new();
    file.take(1048577).read_to_end(&mut bytes)?;
    if bytes.len() > 1048576 {
        return Err("Launcher input exceeds its size limit".into());
    }
    Ok(String::from_utf8(bytes)?)
}
fn local_icon(value: &str) -> bool {
    value.starts_with('/') || !value.contains(':')
}
fn presentation(row: &Value) -> Value {
    // The full application record and search fields stay in the engine. The
    // view already receives the common display fields in MenuItem.
    Value::Object(
        row.as_object()
            .into_iter()
            .flat_map(|row| row.iter())
            .filter(|(key, _)| {
                !matches!(
                    key.as_str(),
                    "app"
                        | "aliases"
                        | "name"
                        | "kind"
                        | "description"
                        | "breadcrumb"
                        | "icon"
                        | "emoji"
                        | "brand"
                        | "status"
                        | "checked"
                        | "reason"
                        | "disabled"
                ) && !(key.as_str() == "label" && row["label"] == row["name"])
                    && !(key.as_str() == "id" && row["kind"] == "app")
            })
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}
fn read_json(path: &Path) -> Result<Value> {
    let s = read_text(path)?;
    if s.is_empty() {
        Ok(json!({}))
    } else {
        Ok(serde_json::from_str(&s)?)
    }
}
fn locale() -> String {
    ["LC_MESSAGES", "LANG"]
        .iter()
        .filter_map(|key| std::env::var(key).ok())
        .find(|s| !s.is_empty())
        .unwrap_or_else(|| "en_US".into())
}
fn ordering_locale(locale: &str) -> String {
    let base = locale.split(['.', '@']).next().unwrap_or("en");
    if matches!(base, "C" | "POSIX") {
        "en-US".into()
    } else {
        base.replace('_', "-")
    }
}
fn roots(paths: &Paths) -> Vec<PathBuf> {
    let home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| paths.home.join(".local/share"));
    let mut roots = vec![home.join("applications")];
    let data = std::env::var_os("XDG_DATA_DIRS")
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    roots.extend(
        std::env::split_paths(&data)
            .filter(|p| p.is_absolute())
            .map(|p| p.join("applications")),
    );
    let mut seen = BTreeSet::new();
    roots.retain(|p| seen.insert(p.clone()));
    roots
}
struct Loaded {
    context: Context,
    entries: BTreeMap<String, desktop::Entry>,
    directories: BTreeSet<PathBuf>,
    warnings: usize,
    fingerprint: String,
    auxiliary: String,
}
fn language_tag(value: &str) -> bool {
    let mut parts = value.split(['-', '_']);
    parts.next().is_some_and(|base| {
        (2..=3).contains(&base.len()) && base.bytes().all(|byte| byte.is_ascii_lowercase())
    }) && parts
        .all(|part| !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_alphanumeric()))
}
fn translations(paths: &Paths, language: &str) -> Result<Value> {
    if let Some(parent) = paths.assets().parent() {
        for name in ["source/i18n", "shell/i18n"] {
            let directory = parent.join(name);
            if directory.join("en.json").is_file() {
                return translations_in(&directory, language);
            }
        }
    }
    Ok(json!({}))
}
fn translations_in(directory: &Path, language: &str) -> Result<Value> {
    let mut messages = read_json(&directory.join("en.json"))?;
    if language != "pseudo" && language != "en" && language_tag(language) {
        let path = directory.join(format!("{language}.json"));
        if path.is_file() {
            match read_json(&path) {
                Ok(Value::Object(translated)) => messages
                    .as_object_mut()
                    .ok_or("Invalid English translation catalog")?
                    .extend(translated),
                _ => tracing::warn!("Launcher translation catalog is invalid; using English"),
            }
        }
    }
    Ok(messages)
}
fn load(paths: &Paths, roots: &[PathBuf], preferences: Value) -> Result<Loaded> {
    let settings = read_json(&paths.config.join("desktop.json"))?;
    let requested = std::env::var("NACRE_LANGUAGE")
        .ok()
        .filter(|value| !value.is_empty())
        .or_else(|| settings["language"].as_str().map(str::to_owned))
        .unwrap_or_else(|| "system".into());
    let language = if matches!(requested.as_str(), "system" | "pseudo") {
        locale()
    } else if language_tag(&requested) {
        requested.clone()
    } else {
        "en".into()
    };
    let message_language = if requested == "pseudo" {
        "pseudo"
    } else if requested == "system" {
        language.split('_').next().unwrap_or("en")
    } else if language_tag(&requested) {
        &requested
    } else {
        "en"
    };
    let translations = translations(paths, message_language)?;
    let entries = desktop::scan(roots, &language)?;
    let mut warnings: usize = entries
        .values()
        .map(|e| {
            e.invalid_lines.len() + usize::from(!local_icon(e.data["icon"].as_str().unwrap_or("")))
        })
        .sum();
    let catalog =
        nacre_menu::config::merge(BASE, &read_text(&paths.config.join("launcher-menu.json"))?)
            .map_err(|e| -> crate::Error { e.into() })?;
    let catalog = nacre_i18n::localize(&catalog, &translations, message_language);
    let mut context = Context::new(
        catalog,
        serde_json::from_str(DEFAULTS)?,
        &ordering_locale(&language),
        clock(),
    )?;
    context.preferences = preferences;
    context.all_apps = desktop::applications(&entries, &context.ordering);
    context.visible_apps = context
        .all_apps
        .iter()
        .filter(|e| !array(&context.preferences["hidden"]).contains(&e["id"]))
        .cloned()
        .collect();
    context.settings = settings;
    context.source_options =
        sources::decode(&read_text(&paths.config.join("launcher-sources.json"))?);
    context.wallpapers =
        json!({"appearancePreferences":read_json(&paths.config.join("wallpaper-picker.json"))?});
    context.translations = translations;
    context.language = message_language.into();
    context.legacy_actions = nacre_menu::browser::default_actions(&context);
    if warnings > 0 {
        tracing::warn!(
            count = warnings,
            "Launcher entries contain invalid lines or nonlocal icons"
        );
    }
    if !text::field(&context.catalog, "error").is_empty() {
        warnings += 1;
    }
    let mut directories = BTreeSet::new();
    for root in roots {
        watch_directories(root, &mut BTreeSet::new(), &mut directories, 0)?;
    }
    directories.insert(paths.config.clone());
    directories.insert(paths.state.join("menu"));
    let auxiliary = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            read_text(&paths.config.join("launcher-workflows.json"))?,
            read_text(&paths.config.join("software-library.json"))?,
        ))?)
    );
    let fingerprint = fingerprint(&context, &auxiliary)?;
    Ok(Loaded {
        context,
        entries,
        directories,
        warnings,
        fingerprint,
        auxiliary,
    })
}
fn fingerprint(context: &Context, auxiliary: &str) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            &context.catalog,
            &context.all_apps,
            &context.preferences,
            &context.settings,
            &context.source_options,
            &context.wallpapers,
            &context.translations,
            auxiliary,
        ))?)
    ))
}

fn watch_directories(
    path: &Path,
    seen: &mut BTreeSet<PathBuf>,
    output: &mut BTreeSet<PathBuf>,
    depth: usize,
) -> Result<()> {
    if depth > 32 || output.len() > 16384 {
        return Err("Application watch tree exceeds its limit".into());
    }
    let real = match fs::canonicalize(path) {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if !seen.insert(real) {
        return Ok(());
    }
    output.insert(path.into());
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if !entry.file_name().to_string_lossy().starts_with('.') && entry.path().is_dir() {
            watch_directories(&entry.path(), seen, output, depth + 1)?;
        }
    }
    Ok(())
}
// The descriptor is owned by AsyncFd for its entire registration. No duplicate
// descriptor or independent reader is exposed; only its owner drains readiness.
#[allow(unsafe_code)]
fn watch_fd(fd: rustix::fd::OwnedFd) -> Result<AsyncFd<rustix::fd::OwnedFd>> {
    // SAFETY: OwnedFd remains registered and open until AsyncFd is dropped.
    Ok(unsafe { AsyncFd::register(fd) }.map_err(|_| "Cannot register application watches")?)
}
struct Watches {
    fd: AsyncFd<rustix::fd::OwnedFd>,
    watched: BTreeMap<i32, PathBuf>,
    targets: BTreeSet<PathBuf>,
    config: Option<PathBuf>,
    menu_state: Option<PathBuf>,
}
impl Watches {
    fn new() -> Result<Self> {
        Ok(Self {
            fd: watch_fd(inotify::init(
                inotify::CreateFlags::NONBLOCK | inotify::CreateFlags::CLOEXEC,
            )?)?,
            watched: BTreeMap::new(),
            targets: BTreeSet::new(),
            config: None,
            menu_state: None,
        })
    }
    fn add(&mut self, paths: impl IntoIterator<Item = PathBuf>) -> Result<()> {
        self.targets = paths.into_iter().collect();
        let mut wanted = BTreeSet::new();
        for path in &self.targets {
            if let Some(existing) = path.ancestors().find(|p| p.is_dir()) {
                wanted.insert(existing.to_path_buf());
                if existing == path {
                    if let Some(parent) = path.parent() {
                        wanted.insert(parent.to_path_buf());
                    }
                }
            }
        }
        let obsolete: Vec<_> = self
            .watched
            .iter()
            .filter(|(_, path)| !wanted.contains(*path))
            .map(|(wd, _)| *wd)
            .collect();
        for wd in obsolete {
            match inotify::remove_watch(self.fd.get_ref(), wd) {
                Ok(()) | Err(rustix::io::Errno::INVAL) => {}
                Err(e) => return Err(e.into()),
            }
            self.watched.remove(&wd);
        }
        let known: BTreeSet<_> = self.watched.values().cloned().collect();
        for path in wanted.difference(&known) {
            match inotify::add_watch(
                self.fd.get_ref(),
                path,
                inotify::WatchFlags::from_bits_retain(WATCH_MASK),
            ) {
                Ok(wd) => {
                    self.watched.insert(wd, path.clone());
                }
                Err(rustix::io::Errno::NOENT) => {} // Raced directory removal; its parent remains watched.
                Err(e) => return Err(e.into()),
            }
        }
        Ok(())
    }
    async fn changed(&mut self) -> Result<()> {
        use std::os::unix::ffi::OsStrExt;
        loop {
            let mut guard = self.fd.readable().await?;
            let result = guard.try_io(|fd| {
                let mut bytes = [MaybeUninit::<u8>::uninit(); 65536];
                let mut reader = inotify::Reader::new(fd.get_ref(), &mut bytes);
                let mut dirty = false;
                loop {
                    let event = reader.next().map_err(std::io::Error::from)?;
                    let mask = event.events().bits();
                    let wd = event.wd();
                    let name = event
                        .file_name()
                        .map_or(b"".as_slice(), |name| name.to_bytes());
                    if let Some(path) = self.watched.get(&wd) {
                        let relevant = if mask & 0xc00 != 0 {
                            true
                        } else if self.config.as_deref() == Some(path.as_path()) {
                            [
                                b"launcher.json".as_slice(),
                                b"launcher-menu.json",
                                b"launcher-sources.json",
                                b"launcher-workflows.json",
                                b"desktop.json",
                                b"wallpaper-picker.json",
                                b"software-library.json",
                            ]
                            .contains(&name)
                        } else if self.menu_state.as_deref() == Some(path.as_path()) {
                            // Reading workflow state takes a registry lock. Its
                            // CLOSE_WRITE is not a data change and must not feed
                            // another provider refresh back into this watcher.
                            name.ends_with(b".json") || mask & 0x40000000 != 0
                        } else if self.targets.contains(path) {
                            name.ends_with(b".desktop") || mask & 0x40000000 != 0
                        } else {
                            self.targets.iter().any(|target| {
                                target
                                    .strip_prefix(path)
                                    .ok()
                                    .and_then(|rest| rest.components().next())
                                    .is_some_and(|part| part.as_os_str().as_bytes() == name)
                            })
                        };
                        if relevant {
                            let root = if self.config.as_deref() == Some(path.as_path()) {
                                "config"
                            } else if self.menu_state.as_deref() == Some(path.as_path()) {
                                "menu-state"
                            } else if self.targets.contains(path) {
                                "applications"
                            } else {
                                "ancestor"
                            };
                            let entry = if mask & 0x40000000 != 0 {
                                "directory"
                            } else if name.ends_with(b".desktop") {
                                "desktop"
                            } else if name.ends_with(b".json") {
                                "json"
                            } else if name.ends_with(b".lock") {
                                "lock"
                            } else {
                                "other"
                            };
                            tracing::debug!(root, entry, mask, "Launcher watch changed");
                        }
                        dirty |= relevant;
                    }
                    if mask & 0x4000 != 0 {
                        dirty = true;
                    } // IN_Q_OVERFLOW: a complete rescan is required.
                    if mask & 0x8c00 != 0 {
                        self.watched.remove(&wd);
                    }
                    if reader.is_buffer_empty() {
                        break;
                    }
                }
                Ok(dirty)
            });
            match result {
                Ok(Ok(true)) => return Ok(()),
                Ok(Ok(false)) | Err(_) => continue,
                Ok(Err(e)) => return Err(e.into()),
            }
        }
    }
}
struct Ticket {
    row: Vec<u8>,
    bytes: usize,
}
struct Engine {
    loaded: Loaded,
    rev: u64,
    providers_rev: u64,
    tickets: BTreeMap<String, Ticket>,
    order: VecDeque<String>,
    bytes: usize,
    secret: String,
}
impl Engine {
    fn replace(&mut self, mut loaded: Loaded) -> Result<()> {
        if self.loaded.fingerprint == loaded.fingerprint {
            // Keep app records shared with the prepared generation after a no-op
            // rescan; paths and diagnostics may still need refreshing.
            for (id, entry) in &mut loaded.entries {
                if let Some(previous) = self.loaded.entries.get(id) {
                    if entry.data == previous.data {
                        entry.data = previous.data.clone();
                    }
                }
            }
            self.loaded.entries = loaded.entries;
            self.loaded.directories = loaded.directories;
            self.loaded.warnings = loaded.warnings;
            return Ok(());
        }
        self.rev = self
            .rev
            .checked_add(1)
            .filter(|v| *v <= nacre_protocol::MAX_SAFE_INTEGER)
            .ok_or("Launcher revision exhausted")?;
        self.loaded = loaded;
        self.tickets.clear();
        self.order.clear();
        self.bytes = 0;
        Ok(())
    }
    async fn prepare_queries(&mut self) -> Result<()> {
        if self.loaded.context.queries_prepared() {
            return Ok(());
        }
        // Move the owned generation into the bounded blocking pool. Other daemon
        // services (including UI routing) remain responsive while it is indexed.
        let vacant = Context::new(json!({"rows":[]}), json!({}), "en-US", clock())?;
        let context = std::mem::replace(&mut self.loaded.context, vacant);
        self.loaded.context = tokio::task::spawn_blocking(move || {
            context.prepare_queries()?;
            Ok::<_, crate::Error>(context)
        })
        .await??;
        Ok(())
    }
    fn query(&mut self, request: MenuQuery) -> Response {
        request.validate()?;
        let route = request.route.clone().unwrap_or_else(|| {
            match request.mode.as_str() {
                "apps" => "apps",
                "search" => "home",
                "actions" => "actions",
                _ => "home",
            }
            .into()
        });
        let route = model::resolve(&route, self.loaded.context.rows());
        self.loaded.context.now = clock();
        let scoped = model::scope(&request.query);
        let conversion = self.calculation_result(&scoped)?;
        let rows = if let Some(result) = &conversion {
            if result.ok {
                let value = result
                    .value
                    .as_deref()
                    .filter(|v| !v.is_empty())
                    .ok_or_else(|| {
                        failure(
                            ErrorCode::Internal,
                            "Invalid calculator provider result",
                            false,
                        )
                    })?;
                let description = result
                    .description
                    .clone()
                    .filter(|v| !v.is_empty())
                    .unwrap_or_else(|| {
                        self.loaded.context.tr(
                            "launcher.app-browser.description.enter-to-copy-the-result",
                            "Enter to copy the result",
                            json!({}),
                        )
                    });
                vec![
                    json!({"id":"calculation","kind":"calculation","name":value,"label":value,"description":description,"icon":"calculate","operation":"copy","value":value,"disabled":false}),
                ]
            } else {
                Vec::new()
            }
        } else {
            self.loaded
                .context
                .indexed_content(
                    &route,
                    &request.query,
                    request.filter.as_deref().unwrap_or("all"),
                    request.category.as_deref().unwrap_or("all"),
                )
                .map_err(|_| failure(ErrorCode::Internal, "Launcher ranking failed", false))?
        };
        let total = rows.len().min(u32::MAX as usize) as u32;
        let mut items = BTreeMap::new();
        let mut order = Vec::new();
        for row in rows
            .into_iter()
            .take(request.limit.unwrap_or(2048) as usize)
        {
            let id = model::stable(&row);
            if text::field(&row, "id").is_empty() || items.contains_key(&id) {
                return Err(failure(
                    ErrorCode::Validation,
                    "Launcher result IDs are missing or duplicated",
                    false,
                ));
            }
            let encoded = serde_json::to_vec(&row).map_err(|_| {
                failure(ErrorCode::Internal, "Launcher item encoding failed", false)
            })?;
            let mut digest = Sha256::new();
            digest.update(self.secret.as_bytes());
            digest.update(self.rev.to_le_bytes());
            digest.update(&encoded);
            let token = format!("{:x}", digest.finalize());
            let string = |field| row.get(field).and_then(Value::as_str).map(str::to_owned);
            let item = MenuItem {
                name: row["name"]
                    .as_str()
                    .or_else(|| row["label"].as_str())
                    .unwrap_or("")
                    .into(),
                kind: row["kind"].as_str().unwrap_or("action").into(),
                description: string("description"),
                breadcrumb: string("breadcrumb"),
                icon: string("icon").filter(|icon| local_icon(icon)),
                emoji: string("emoji"),
                brand: string("brand"),
                status: string("status"),
                checked: row["checked"].as_bool(),
                reason: string("reason"),
                disabled: row["disabled"] == true,
                activation: Some(token.clone()),
                presentation: (request.presentation == Some(true)).then(|| presentation(&row)),
            };
            if !self.tickets.contains_key(&token) {
                let bytes = encoded.len();
                while !self.order.is_empty()
                    && (self.order.len() >= 4096 || self.bytes + bytes > 8 * 1024 * 1024)
                {
                    let old = self.order.pop_front().unwrap();
                    if let Some(ticket) = self.tickets.remove(&old) {
                        self.bytes -= ticket.bytes;
                    }
                }
                self.bytes += bytes;
                self.order.push_back(token.clone());
                self.tickets.insert(
                    token,
                    Ticket {
                        row: encoded,
                        bytes,
                    },
                );
            }
            order.push(id.clone());
            items.insert(id, item);
        }
        let message = self.loaded.context.previews["providerMessages"]
            [scoped["text"].as_str().unwrap_or("")]
        .as_str()
        .map(str::to_owned);
        let result = MenuQueryResult {
            query: request.query,
            mode: request.mode,
            route,
            catalog_rev: self.rev,
            items,
            order,
            total: Some(total),
            conversion,
            message,
        };
        result.validate()?;
        serde_json::to_value(result).map_err(|_| {
            failure(
                ErrorCode::Internal,
                "Launcher result encoding failed",
                false,
            )
        })
    }
    fn calculation_result(
        &self,
        scoped: &Value,
    ) -> std::result::Result<Option<MenuConversion>, Failure> {
        if scoped["kind"] != "calculator"
            || self.loaded.context.source_options["calculator"] == false
        {
            return Ok(None);
        }
        let term = scoped["text"].as_str().unwrap_or("");
        let basic = calculator::evaluate(term);
        if basic.ok || !conversion_query(term) {
            return Ok(Some(MenuConversion {
                ok: basic.ok,
                value: basic.value,
                description: None,
                error: Some(basic.error),
            }));
        }
        if self.loaded.context.previews["conversionQuery"] != term {
            return Ok(Some(MenuConversion {
                ok: false,
                value: None,
                description: None,
                error: Some(self.loaded.context.tr(
                    "launcher.app-browser.error.converting",
                    "Converting…",
                    json!({}),
                )),
            }));
        }
        serde_json::from_value(self.loaded.context.previews["conversion"].clone())
            .map(Some)
            .map_err(|_| {
                failure(
                    ErrorCode::Internal,
                    "Invalid calculator provider result",
                    false,
                )
            })
    }
    fn activate(&self, request: MenuActivate) -> Response {
        request.validate()?;
        let stale = || {
            failure(
                ErrorCode::StaleRevision,
                "Catalog changed; query again",
                true,
            )
        };
        if request.catalog_rev != self.rev {
            return Err(stale());
        }
        let row: Value =
            serde_json::from_slice(&self.tickets.get(&request.activation).ok_or_else(stale)?.row)
                .map_err(|_| {
                failure(
                    ErrorCode::Internal,
                    "Invalid saved launcher activation",
                    false,
                )
            })?;
        if model::stable(&row) != request.id {
            return Err(failure(
                ErrorCode::InvalidRequest,
                "Launcher activation does not match its item",
                false,
            ));
        }
        if row["disabled"] == true {
            return Err(failure(
                ErrorCode::Unavailable,
                "Launcher item is unavailable",
                false,
            ));
        }
        if row["kind"] == "group" {
            return Ok(json!(MenuActivated {
                keep_open: true,
                view: Some(MenuView {
                    route: Some(text::field(&row, "id")),
                    ..MenuView::default()
                })
            }));
        }
        if row["operation"] == "app" {
            let entry = self.loaded.entries.get(&request.id).ok_or_else(stale)?;
            if array(&self.loaded.context.preferences["hidden"]).contains(&json!(request.id))
                || entry.data["noDisplay"] == true
            {
                return Err(failure(
                    ErrorCode::Unavailable,
                    "Application is hidden",
                    false,
                ));
            }
            return Ok(json!({"application":entry.data}));
        }
        Ok(json!({"uiRow":row,"catalog_rev":self.rev}))
    }
    fn status(&self) -> Value {
        json!({"available":true,"catalog_rev":self.rev,"providers_rev":self.providers_rev,"applications":self.loaded.entries.len(),"warnings":self.loaded.warnings,"error":self.loaded.context.catalog["error"]})
    }
}
struct Registration(Shared);
impl Drop for Registration {
    fn drop(&mut self) {
        let mut hub = self.0.borrow_mut();
        hub.menu = None;
        if let Some(mut status) = hub.snapshot("menu") {
            status["available"] = json!(false);
            if hub.publish("menu", status).is_err() {
                tracing::warn!("Cannot publish launcher disconnect");
            }
        }
    }
}
pub async fn run(hub: Shared, paths: Paths) -> Result<()> {
    let (requests, mut incoming) = mpsc::channel::<Request>(16);
    hub.borrow_mut().menu = Some(Requests(requests));
    let _registration = Registration(hub.clone());
    // Subscribe before any indexing await so initial compositor publications cannot be missed.
    let mut changes = hub.borrow().observe();
    hub.borrow_mut().publish(
        "menu",
        json!({"available":false,"catalog_rev":0,"applications":0}),
    )?;
    // Workflow reads take registry locks. Create their directory up front so
    // a delayed initial provider read does not look like a new catalog root.
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(paths.state.join("menu"))?;
    let roots = roots(&paths);
    let mut watches = Watches::new()?;
    watches.config = Some(paths.config.clone());
    watches.menu_state = Some(paths.state.join("menu"));
    watches.add(
        roots
            .iter()
            .cloned()
            .chain([paths.config.clone(), paths.state.join("menu")]),
    )?;
    let load_paths = paths.clone();
    let load_roots = roots.clone();
    let loaded = tokio::task::spawn_blocking(move || {
        let preferences =
            launcher_preferences::initialize(&load_paths.config.join("launcher.json"))?;
        load(&load_paths, &load_roots, preferences)
    })
    .await??;
    watches.add(
        loaded
            .directories
            .iter()
            .cloned()
            .chain(roots.iter().cloned()),
    )?;
    let mut engine = Engine {
        loaded,
        rev: 1,
        providers_rev: 0,
        tickets: BTreeMap::new(),
        order: VecDeque::new(),
        bytes: 0,
        secret: uuid::Uuid::new_v4().to_string(),
    };
    if let Some(snapshot) = hub.borrow().snapshot("compositor") {
        compositor_inputs(&mut engine.loaded.context, &snapshot);
    }
    engine.prepare_queries().await?;
    hub.borrow_mut().publish("menu", engine.status())?;
    let pool = hub.borrow().providers.clone();
    let mut jobs = tokio::task::JoinSet::new();
    let mut refreshing = true;
    let mut refresh_again = false;
    let initial_pool = pool.clone();
    jobs.spawn_local(async move {
        (
            None,
            "load".to_owned(),
            String::new(),
            initial_pool.refresh().await,
        )
    });
    loop {
        tokio::select! {
            result=pool.reap_idle()=>{
                result?;
                engine.loaded.context.retire_provider_cache();
                // Query responses and retired-worker buffers are already dropped.
                // Return unused allocator pages once, on the existing idle event.
                #[cfg(target_env = "gnu")]
                #[allow(unsafe_code)]
                // SAFETY: malloc_trim takes no pointers and is thread-safe. A zero
                // return means no unused pages were available, not an error.
                unsafe { nix::libc::malloc_trim(0); }
            },
            request=incoming.recv()=>{
                let Some(request)=request else {return Ok(());};
                if request.reply.is_closed(){continue;}
                if request.method=="menu.query" {
                    if let Ok(query)=serde_json::from_value::<MenuQuery>(request.params.clone()) {
                        if let Err(error)=query.validate(){
                            if request.reply.send(Err(error)).is_err(){tracing::debug!("Launcher caller ended");}
                            continue;
                        }
                        if let Some((action,term))=provider_request(&engine.loaded.context,&query) {
                            if jobs.len()>=16 {
                                if request.reply.send(Err(failure(ErrorCode::Busy,"Launcher provider request limit reached",true))).is_err(){tracing::debug!("Launcher caller ended");}
                            }else{
                                let pool=pool.clone();
                                jobs.spawn_local(async move {let result=pool.request(&action,&term).await;(Some(request),action,term,result)});
                            }
                            continue;
                        }
                    }
                }
                if request.method=="menu.activate" {
                    if let Ok(activation)=serde_json::from_value::<MenuActivate>(request.params.clone()) {
                        if let Ok(result)=engine.activate(activation.clone()) {
                            if let Some(application)=result.get("application") {
                                if jobs.len()>=16 {
                                    if request.reply.send(Err(failure(ErrorCode::Busy,"Launcher activation queue is full",true))).is_err(){tracing::debug!("Launcher caller ended");}
                                }else{
                                    // Retain existing history: a selected app is recorded before launch.
                                    let recorded=preference(&paths,&mut engine,json!({"action":"record","value":{"id":activation.id,"enabled":true}}),false).await;
                                    if let Err(error)=recorded {if request.reply.send(Err(error)).is_err(){tracing::debug!("Launcher caller ended");}continue;}
                                    engine.prepare_queries().await?;
                                    hub.borrow_mut().publish("menu",engine.status())?;
                                    let encoded=serde_json::to_string(application)?;let pool=pool.clone();
                                    jobs.spawn_local(async move{let result=pool.request("launch",&encoded).await;(Some(request),"launch".into(),String::new(),result)});
                                }
                                continue;
                            }
                        }
                    }
                }
                let result=match request.method.as_str() {
                    "menu.query"=>match serde_json::from_value::<MenuQuery>(request.params){Ok(q)=>engine.query(q),Err(_)=>Err(failure(ErrorCode::InvalidRequest,"Invalid launcher query",false))},
                    "menu.activate"=>match serde_json::from_value::<MenuActivate>(request.params){Ok(q)=>engine.activate(q),Err(_)=>Err(failure(ErrorCode::InvalidRequest,"Invalid launcher activation",false))},
                    "menu.preferences"=>preference(&paths,&mut engine,request.params,false).await,
                    "helper.preferences"=>preference(&paths,&mut engine,request.params,true).await,
                    _=>Err(failure(ErrorCode::UnknownMethod,"Unknown launcher method",false)),
                };
                if matches!(request.method.as_str(), "menu.preferences"|"helper.preferences") && result.is_ok(){engine.prepare_queries().await?;hub.borrow_mut().publish("menu",engine.status())?;}
                if request.reply.send(result).is_err(){tracing::debug!("Launcher caller ended before its reply");}
            },
            job=jobs.join_next(),if !jobs.is_empty()=>{
                let (request,action,term,result)=job.ok_or("Launcher provider task disappeared")??;
                if let Some(request)=request {
                    if request.reply.is_closed(){continue;}
                    let result=if action=="launch" {result.map_err(|mut error|{if error.code!=ErrorCode::Busy{error.retry=false;error=error.dispatch_state(true);}error})}else{match result {
                        Ok(value)=>{apply_provider(&mut engine.loaded.context,&action,&term,&value);match serde_json::from_value::<MenuQuery>(request.params){Ok(q)=>engine.query(q),Err(_)=>Err(failure(ErrorCode::InvalidRequest,"Invalid launcher query",false))}},
                        Err(error)=>Err(error),
                    }};
                    if request.reply.send(result).is_err(){tracing::debug!("Launcher provider caller ended");}
                } else {
                    refreshing = false;
                    match result {
                        Ok(value)=>{apply_provider(&mut engine.loaded.context,&action,&term,&value);engine.prepare_queries().await?;engine.providers_rev=engine.providers_rev.checked_add(1).filter(|v|*v<=nacre_protocol::MAX_SAFE_INTEGER).ok_or("Launcher provider revision exhausted")?;hub.borrow_mut().service("menu","running",0)?;hub.borrow_mut().publish("menu.providers",json!({"available":true,"error":""}))?;hub.borrow_mut().publish("menu",engine.status())?;},
                        Err(error)=>{tracing::warn!(code=?error.code,"Launcher provider unavailable");hub.borrow_mut().service("menu","degraded",0)?;hub.borrow_mut().publish("menu.providers",json!({"available":false,"error":error.message}))?;},
                    }
                    if refresh_again {
                        refresh_again = false;
                        refreshing = true;
                        let pool = pool.clone();
                        jobs.spawn_local(async move {(None, "load".into(), String::new(), pool.refresh().await)});
                    }
                }
            },
            change=changes.recv()=>{
                match change {
                    Ok(super::hub::Notice::Changed{topic,..}) if topic=="compositor"=>{
                        let snapshot=hub.borrow().snapshot("compositor");
                        if let Some(snapshot)=snapshot {compositor_inputs(&mut engine.loaded.context,&snapshot);engine.prepare_queries().await?;}
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_))=>{
                        tracing::warn!("Launcher missed state updates; resynchronizing from current compositor data");
                        let snapshot=hub.borrow().snapshot("compositor");
                        if let Some(snapshot)=snapshot {compositor_inputs(&mut engine.loaded.context,&snapshot);engine.prepare_queries().await?;}
                    },
                    Err(tokio::sync::broadcast::error::RecvError::Closed)=>return Err("Launcher state observer ended".into()),
                    _=>{},
                }
            },
            changed=watches.changed()=>{
                changed?;tracing::debug!("Launcher inputs changed; refreshing index");tokio::time::sleep(Duration::from_millis(150)).await;
                let load_paths=paths.clone();let load_roots=roots.clone();
                match tokio::task::spawn_blocking(move||{
                    let preferences=launcher_preferences::load(&load_paths.config.join("launcher.json"))?;
                    load(&load_paths,&load_roots,preferences)
                }).await? {
                    Ok(mut loaded)=>{
                        watches.add(loaded.directories.iter().cloned().chain(roots.iter().cloned()))?;
                        loaded.context.snapshot=engine.loaded.context.snapshot.clone();loaded.context.library=engine.loaded.context.library.clone();loaded.context.workflows=engine.loaded.context.workflows.clone();loaded.context.compositor=engine.loaded.context.compositor.clone();
                        let changed = engine.loaded.fingerprint != loaded.fingerprint;
                        engine.replace(loaded)?;engine.prepare_queries().await?;hub.borrow_mut().publish("menu",engine.status())?;
                        if changed {
                            if refreshing {refresh_again=true;} else {
                                refreshing=true;
                                let pool=pool.clone();jobs.spawn_local(async move{(None,"load".into(),String::new(),pool.refresh().await)});
                            }
                        }
                    },
                    Err(_)=>{tracing::warn!("Launcher input rejected; retaining the last valid index");hub.borrow_mut().publish("menu",json!({"available":true,"catalog_rev":engine.rev,"applications":engine.loaded.entries.len(),"error":"Launcher input rejected; last valid index retained"}))?;},
                }
            }
        }
    }
}
async fn preference(paths: &Paths, engine: &mut Engine, params: Value, legacy: bool) -> Response {
    if params
        .as_object()
        .is_none_or(|p| p.keys().any(|key| key != "action" && key != "value"))
    {
        return Err(failure(
            ErrorCode::Validation,
            "Invalid launcher preference parameters",
            false,
        ));
    }
    let action = params["action"].as_str().ok_or_else(|| {
        failure(
            ErrorCode::Validation,
            "Expected launcher preference action",
            false,
        )
    })?;
    if action == "load" {
        return Ok(engine.loaded.context.preferences.clone());
    }
    if !["favorite", "hide", "record"].contains(&action) {
        return Err(failure(
            ErrorCode::Validation,
            "Invalid launcher preference action",
            false,
        ));
    }
    let path = paths.config.join("launcher.json");
    let action = action.to_owned();
    let value = params["value"].clone();
    let result = tokio::task::spawn_blocking(move || {
        // The daemon retained its last good preferences after an invalid hand
        // edit. Do not let the legacy corruption fallback overwrite that file.
        crate::state::read_strict(&path, json!({}))?;
        launcher_preferences::change(&path, &action, &value)
    })
    .await
    .map_err(|_| {
        failure(
            ErrorCode::Internal,
            "Launcher preference task failed",
            false,
        )
    })?
    .map_err(|error| {
        let message = if legacy {
            error.to_string()
        } else {
            "Launcher preference was not saved".into()
        };
        failure(ErrorCode::Validation, &message, false)
    })?;
    if engine.loaded.context.preferences == result {
        return Ok(result);
    }
    let hidden_changed = engine.loaded.context.preferences["hidden"] != result["hidden"];
    if hidden_changed {
        engine.loaded.context.inputs_changed();
    }
    engine.loaded.context.preferences = result.clone();
    if hidden_changed {
        engine.loaded.context.visible_apps = engine
            .loaded
            .context
            .all_apps
            .iter()
            .filter(|e| !array(&result["hidden"]).contains(&e["id"]))
            .cloned()
            .collect();
    }
    engine.loaded.fingerprint = fingerprint(&engine.loaded.context, &engine.loaded.auxiliary)
        .map_err(|_| {
            failure(
                ErrorCode::Internal,
                "Launcher preference revision failed",
                false,
            )
        })?;
    engine.rev = engine
        .rev
        .checked_add(1)
        .filter(|v| *v <= nacre_protocol::MAX_SAFE_INTEGER)
        .ok_or_else(|| failure(ErrorCode::Internal, "Launcher revision exhausted", false))?;
    engine.tickets.clear();
    engine.order.clear();
    engine.bytes = 0;
    Ok(result)
}

// Only these compositor fields affect launcher results. Focus/geometry changes
// must not rebuild the installed-app/software/font index on the next keypress.
fn compositor_inputs(context: &mut Context, snapshot: &Value) {
    let windows:Vec<_>=array(&snapshot["windowOrder"]).iter().filter_map(|id|id.as_str().and_then(|id|snapshot["windows"].get(id))).map(|row|json!({"windowId":row["windowId"],"title":row["title"],"wmClass":row["wmClass"],"workspace":{"name":row["workspace"]["name"]}})).collect();
    let next = json!({"windows":windows});
    if context.compositor != next {
        context.inputs_changed();
        context.compositor = next;
    }
}
// The existing view requests conversion only for lowercase whole words to/in.
fn conversion_query(term: &str) -> bool {
    term.split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .any(|word| matches!(word, "to" | "in"))
}
fn provider_request(context: &Context, request: &MenuQuery) -> Option<(String, String)> {
    let scope = model::scope(&request.query);
    let kind = scope["kind"].as_str().unwrap_or("all");
    let term = text::field(&scope, "text");
    let enabled = |key| context.source_options[key] != false;
    let action = match kind {
        "files" if enabled("files") => "files",
        "calculator" if enabled("calculator") && conversion_query(&term) => "convert",
        "all" if request.route.as_deref() == Some("software.packages") && !term.is_empty() => {
            "packages"
        }
        // Emoji search is local but scans the full CLDR catalog in a worker.
        "all" if !term.is_empty() && term.chars().count() <= 160 => "emoji",
        _ => return None,
    };
    Some((action.into(), term))
}
fn apply_provider(context: &mut Context, action: &str, term: &str, value: &Value) {
    if let Some(message) = value.get("message").and_then(Value::as_str) {
        if !context.previews["providerMessages"].is_object() {
            context.previews["providerMessages"] = json!({});
        }
        if context.previews["providerMessages"]
            .as_object()
            .is_some_and(|m| m.len() >= 64)
        {
            context.previews["providerMessages"] = json!({});
        }
        context.previews["providerMessages"][term] = json!(message);
    }
    match action {
        "load" => {
            let snapshot_changed = [
                "capabilities",
                "packages",
                "fonts",
                "stayAwake",
                "recording",
                "reminders",
                "webapps",
            ]
            .iter()
            .any(|key| context.snapshot[*key] != value["snapshot"][*key]);
            if snapshot_changed
                || context.library != value["library"]
                || context.workflows != value["workflows"]
            {
                context.inputs_changed();
            }
            context.snapshot = value["snapshot"].clone();
            context.library = value["library"].clone();
            context.workflows = value["workflows"].clone();
            context.recording_active =
                context.snapshot["recording"] == true || context.workflows["recording"] == true;
        }
        "files" => {
            context.file_query = term.into();
            context.files = array(&value["rows"]).to_vec();
        }
        "packages" => {
            context.package_query = term.into();
            context.packages = array(&value["rows"]).to_vec();
        }
        "emoji" => {
            context.previews["emojiQuery"] = json!(term);
            context.previews["emojiMatches"] = value["entries"].clone();
        }
        "convert" => {
            context.previews["conversionQuery"] = json!(term);
            context.previews["conversion"] = value.clone();
        }
        _ => {}
    }
}

/// Existing CLI/serve callers use the sole writer when a daemon exists. An
/// absent default endpoint retains standalone parity; an explicit endpoint fails closed.
pub fn preferences_client(arguments: &[String], input: Option<Value>) -> Result<Option<Value>> {
    let explicit = std::env::var_os("NACRE_DAEMON_SOCKET").is_some();
    if !explicit && std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|p| !Path::new(&p).is_absolute())
    {
        return Ok(None);
    }
    let endpoint = super::socket::path()?;
    if let Err(error) = super::socket::validate_endpoint(&endpoint) {
        if !explicit
            && error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
        {
            return Ok(None);
        }
        return Err(error);
    }
    let [action] = arguments else {
        return Err("Unsupported helper operation".into());
    };
    if !["load", "favorite", "hide", "record"].contains(&action.as_str())
        || (action == "load" && input.is_some())
    {
        return Err("Unsupported helper operation".into());
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    runtime.block_on(async {
        let mut client = match super::client::connect().await {
            Ok(client) => client,
            Err(super::client::ConnectError::Absent) if !explicit => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !client.methods.contains_key("menu.preferences") {
            let status = client.call("daemon.status", json!({}), Some(1000)).await?;
            if explicit || status["safe"] == true {
                return Err("Launcher preferences service is unavailable".into());
            }
            return Ok(None);
        }
        client
            .call(
                "menu.preferences",
                json!({"action":action,"value":input}),
                Some(10000),
            )
            .await
            .map(Some)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qml_presentation_keeps_context_fields_without_shipping_the_application_index() -> Result<()>
    {
        let mut state = engine()?;
        state.loaded.context.all_apps = (0..2000).map(|index| std::sync::Arc::new(json!({
            "id":format!("fixture-{index:04}.desktop"),"name":format!("Application {index:04}"),
            "comment":"Public desktop entry used to exercise the two-thousand application view.",
            "icon":"application-x-executable","keywords":["query-only","search-only"],
            "command":["fixture","literal argument"],"runInTerminal":false
        }))).collect();
        state.loaded.context.visible_apps = state.loaded.context.all_apps.clone();
        state.loaded.context.inputs_changed();
        let value = state.query(serde_json::from_value(
            json!({"query":"","route":"apps","presentation":true}),
        )?)?;
        assert_eq!(array(&value["order"]).len(), 2000);
        for row in value["items"]
            .as_object()
            .ok_or("Missing result rows")?
            .values()
        {
            assert_eq!(row["presentation"], json!({"operation":"app"}));
        }
        let message = nacre_protocol::Message::Ret {
            id: nacre_protocol::Id::Integer(1),
            reply: nacre_protocol::Reply::Ok { ok: value },
        };
        assert!(nacre_protocol::framing::encode(&message)?.len() <= nacre_protocol::MAX_FRAME + 1);
        let view = presentation(
            &json!({"id":"file-1","kind":"file","name":"Public file","label":"Public file","file":true,"filePath":"/public/fixture.txt","operation":"open-file","value":"/public/fixture.txt","app":{"command":["never ship"]},"aliases":["search only"]}),
        );
        assert_eq!(
            view,
            json!({"id":"file-1","file":true,"filePath":"/public/fixture.txt","operation":"open-file","value":"/public/fixture.txt"})
        );
        Ok(())
    }

    #[test]
    fn catalog_translations_follow_ui_languages_and_reject_path_like_language_tags() -> Result<()> {
        let home = tempfile::tempdir()?;
        let directory = home.path().join("i18n");
        fs::create_dir_all(&directory)?;
        fs::write(directory.join("en.json"), br#"{"fixture":"English label"}"#)?;
        fs::write(
            directory.join("tr.json"),
            br#"{"fixture":"Translated label"}"#,
        )?;
        assert_eq!(
            translations_in(&directory, "tr")?["fixture"],
            "Translated label"
        );
        assert_eq!(
            translations_in(&directory, "missing")?["fixture"],
            "English label"
        );
        for language in ["../tr", "tr/../../secret", "/private/fixture", "tr\0"] {
            assert!(!language_tag(language));
        }
        fs::write(directory.join("tr.json"), b"{")?;
        assert_eq!(
            translations_in(&directory, "tr")?["fixture"],
            "English label"
        );
        Ok(())
    }

    #[test]
    fn selected_action_type_accepts_the_frozen_enabled_non_app_provider_rows() -> Result<()> {
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(
            include_bytes!("../../../nacre-menu/tests/fixtures/providers.json.gz").as_slice(),
        )
        .read_to_end(&mut bytes)?;
        let reference: Value = serde_json::from_slice(&bytes)?;
        fn visit(value: &Value, count: &mut usize, failures: &mut Vec<String>) {
            match value {
                Value::Array(values) => {
                    for value in values {
                        visit(value, count, failures);
                    }
                }
                Value::Object(values) => {
                    if values.contains_key("operation")
                        && values.contains_key("kind")
                        && value["kind"] != "group"
                        && value["operation"] != "app"
                        && value["disabled"] != true
                    {
                        *count += 1;
                        let result = serde_json::from_value::<MenuUiActivate>(
                            json!({"row":value,"catalog_rev":1}),
                        )
                        .map_err(|error| error.to_string())
                        .and_then(|selected| {
                            selected.validate().map_err(|error| error.to_string())
                        });
                        if let Err(error) = result {
                            failures.push(format!("{}: {error}", text::field(value, "id")));
                        }
                    }
                    for value in values.values() {
                        visit(value, count, failures);
                    }
                }
                _ => {}
            }
        }
        let mut count = 0;
        let mut failures = Vec::new();
        for case in array(&reference["cases"]) {
            visit(&case["result"], &mut count, &mut failures);
        }
        assert!(count > 100, "No representative selected rows checked");
        assert!(
            failures.is_empty(),
            "Rejected frozen rows: {:?}",
            &failures[..failures.len().min(12)]
        );
        Ok(())
    }
    fn engine() -> Result<Engine> {
        let catalog =
            nacre_menu::config::merge(BASE, "").map_err(|e| -> crate::Error { e.into() })?;
        let mut context = Context::new(
            catalog,
            serde_json::from_str(DEFAULTS)?,
            "en-US",
            1720000000.0,
        )?;
        context.all_apps = vec![std::sync::Arc::new(
            json!({"id":"fixture","name":"Fixture app","comment":"Synthetic","genericName":"","icon":"fixture","categories":[],"keywords":[],"command":["fixture","$HOME;$(touch forbidden)"],"runInTerminal":false,"workingDirectory":""}),
        )];
        context.visible_apps = context.all_apps.clone();
        context.preferences = json!({"favorites":[],"hidden":[],"recent":[],"usage":{}});
        Ok(Engine {
            loaded: Loaded {
                context,
                entries: BTreeMap::from([("fixture".into(),desktop::parse("fixture","[Desktop Entry]\nType=Application\nName=Fixture app\nExec=fixture '$HOME;$(touch forbidden)'\n","en_US"))]),
                directories: BTreeSet::new(),
                warnings: 0,
                fingerprint: "fixture".into(),
                auxiliary: String::new(),
            },
            rev: 7,
            providers_rev: 0,
            tickets: BTreeMap::new(),
            order: VecDeque::new(),
            bytes: 0,
            secret: "fixture-secret".into(),
        })
    }
    #[test]
    fn native_arithmetic_query_issues_only_the_calculated_literal_copy_ticket() -> Result<()> {
        let mut state = engine()?;
        let request: MenuQuery =
            serde_json::from_value(json!({"query":"= 1 + 1","mode":"search"}))?;
        assert!(provider_request(&state.loaded.context, &request).is_none());
        let answer = state.query(request)?;
        assert_eq!(
            answer["conversion"],
            json!({"ok":true,"value":"2","error":""})
        );
        assert_eq!(answer["order"], json!(["nacre-menu:calculation"]));
        let result = state.activate(serde_json::from_value(json!({
            "id":"nacre-menu:calculation",
            "activation":answer["items"]["nacre-menu:calculation"]["activation"],
            "catalog_rev":state.rev,
            "row":{"operation":"command","arguments":["forged-command"]}
        }))?)?;
        let selected: MenuUiActivate = serde_json::from_value(
            json!({"row":result["uiRow"],"catalog_rev":result["catalog_rev"]}),
        )?;
        selected.validate()?;
        assert_eq!(selected.row.operation, "copy");
        assert_eq!(
            selected.row.value,
            Some(nacre_protocol::menu::MenuActionValue::Text("2".into()))
        );
        let failed = state.query(serde_json::from_value(
            json!({"query":"= 1 / 0","mode":"search"}),
        )?)?;
        assert!(array(&failed["order"]).is_empty());
        assert_eq!(failed["conversion"]["ok"], false);
        assert_eq!(
            failed["conversion"]["error"],
            "The result is undefined or outside the numeric range"
        );
        Ok(())
    }
    #[test]
    fn native_calculator_retains_conversion_word_gate_and_matching_query() -> Result<()> {
        let mut state = engine()?;
        for query in [
            "= 1+1",
            "= sin(1)",
            "= 2 IN m",
            "= to_value(2)",
            "= invalid",
        ] {
            let request: MenuQuery =
                serde_json::from_value(json!({"query":query,"mode":"search"}))?;
            assert!(
                provider_request(&state.loaded.context, &request).is_none(),
                "{query}"
            );
        }
        for query in ["= 1 kg to g", "= 2 m in cm"] {
            let request: MenuQuery =
                serde_json::from_value(json!({"query":query,"mode":"search"}))?;
            assert_eq!(
                provider_request(&state.loaded.context, &request).map(|p| p.0),
                Some("convert".into())
            );
        }
        apply_provider(
            &mut state.loaded.context,
            "convert",
            "1 kg to g",
            &json!({"ok":true,"value":"1000 g","description":"Mass"}),
        );
        let converted = state.query(serde_json::from_value(
            json!({"query":"= 1 kg to g","mode":"search"}),
        )?)?;
        assert_eq!(
            converted["items"]["nacre-menu:calculation"]["name"],
            "1000 g"
        );
        assert_eq!(
            converted["items"]["nacre-menu:calculation"]["description"],
            "Mass"
        );
        let pending = state.query(serde_json::from_value(
            json!({"query":"= 2 kg to g","mode":"search"}),
        )?)?;
        assert_eq!(pending["conversion"]["ok"], false);
        assert_eq!(pending["conversion"]["error"], "Converting…");
        assert!(array(&pending["order"]).is_empty());
        state.loaded.context.source_options["calculator"] = json!(false);
        let request: MenuQuery = serde_json::from_value(json!({"query":"= 1+1","mode":"search"}))?;
        assert!(provider_request(&state.loaded.context, &request).is_none());
        let disabled = state.query(request)?;
        assert!(disabled.get("conversion").is_none());
        assert_eq!(array(&disabled["order"]).len(), 1);
        assert_eq!(
            disabled["items"][disabled["order"][0].as_str().unwrap()]["disabled"],
            true
        );
        Ok(())
    }
    #[test]
    fn no_op_rescans_preserve_shared_app_records() -> Result<()> {
        let mut state = engine()?;
        let record = state.loaded.entries["fixture"].data.clone();
        state.loaded.context.all_apps = vec![record.clone()];
        state.loaded.context.visible_apps = vec![record.clone()];
        let mut reloaded = engine()?.loaded;
        reloaded.entries.get_mut("fixture").unwrap().path = "refreshed.desktop".into();
        state.replace(reloaded)?;
        assert!(std::sync::Arc::ptr_eq(
            &record,
            &state.loaded.entries["fixture"].data
        ));
        assert_eq!(
            state.loaded.entries["fixture"].path,
            PathBuf::from("refreshed.desktop")
        );
        assert_eq!(state.rev, 7);
        Ok(())
    }
    #[test]
    fn activation_requires_issued_ticket_matching_id_revision_and_availability() -> Result<()> {
        let mut engine = engine()?;
        let query = serde_json::from_value(json!({"query":"","mode":"apps","route":"apps"}))?;
        let answer = engine.query(query)?;
        assert_eq!(answer["order"], json!(["fixture"]));
        let ticket = answer["items"]["fixture"]["activation"]
            .as_str()
            .unwrap()
            .to_owned();
        let request = MenuActivate {
            id: "fixture".into(),
            activation: ticket.clone(),
            catalog_rev: 7,
        };
        let result = engine.activate(request.clone())?;
        assert_eq!(
            result["application"]["command"],
            json!(["fixture", "$HOME;$(touch forbidden)"])
        );
        let mut wrong = request.clone();
        wrong.id = "other".into();
        assert_eq!(
            engine.activate(wrong).unwrap_err().code,
            ErrorCode::InvalidRequest
        );
        let mut wrong = request.clone();
        wrong.activation = "unissued".into();
        assert_eq!(
            engine.activate(wrong).unwrap_err().code,
            ErrorCode::StaleRevision
        );
        let mut wrong = request.clone();
        wrong.catalog_rev = 6;
        assert_eq!(
            engine.activate(wrong).unwrap_err().code,
            ErrorCode::StaleRevision
        );
        let saved = engine.tickets.get_mut(&ticket).unwrap();
        let mut row: Value = serde_json::from_slice(&saved.row)?;
        row["disabled"] = json!(true);
        saved.row = serde_json::to_vec(&row)?;
        assert_eq!(
            engine.activate(request).unwrap_err().code,
            ErrorCode::Unavailable
        );
        Ok(())
    }
    #[test]
    fn activation_ignores_supplied_rows_and_confirmation_claims() -> Result<()> {
        let mut engine = engine()?;
        let query = engine.query(serde_json::from_value(
            json!({"query":"","mode":"search","route":"power"}),
        )?)?;
        let id = "nacre-menu:power.reboot";
        let token = query["items"][id]["activation"]
            .as_str()
            .ok_or("Missing confirmation fixture ticket")?;
        let original: Value = serde_json::from_slice(&engine.tickets[token].row)?;
        let request: MenuActivate = serde_json::from_value(json!({
            "id":id,"activation":token,"catalog_rev":engine.rev,
            "confirmed":true,"row":{"operation":"command","arguments":["fixture-forged"]}
        }))?;
        let answer = engine.activate(request)?;
        assert_eq!(answer["uiRow"], original);
        assert_eq!(answer["uiRow"]["operation"], "power");
        assert_eq!(answer["uiRow"]["value"], "reboot");
        assert!(answer["uiRow"].get("confirmed").is_none());
        serde_json::from_value::<MenuUiActivate>(
            json!({"row":answer["uiRow"],"catalog_rev":engine.rev}),
        )?
        .validate()?;
        Ok(())
    }
    #[test]
    fn native_query_maps_keep_order_and_source_gates_and_navigation() -> Result<()> {
        let mut engine = engine()?;
        let result = engine.query(serde_json::from_value(
            json!({"query":"","mode":"search","route":"settings"}),
        )?)?;
        let order = array(&result["order"]);
        assert!(!order.is_empty());
        let groups = engine.query(serde_json::from_value(
            json!({"query":"apps","mode":"search"}),
        )?)?;
        let activation = groups["items"]["nacre-menu:apps"]["activation"]
            .as_str()
            .ok_or("Missing apps group")?;
        let navigation = engine.activate(MenuActivate {
            id: "nacre-menu:apps".into(),
            activation: activation.into(),
            catalog_rev: engine.rev,
        })?;
        assert_eq!(navigation["view"]["route"], "apps");
        for id in order {
            assert!(result["items"].get(id.as_str().unwrap()).is_some());
        }
        engine.loaded.context.inputs_changed();
        engine.loaded.context.source_options["files"] = json!(false);
        let request: MenuQuery =
            serde_json::from_value(json!({"query":": fixture","mode":"search"}))?;
        assert!(provider_request(&engine.loaded.context, &request).is_none());
        let result = engine.query(request)?;
        assert_eq!(array(&result["order"]).len(), 1);
        assert_eq!(
            result["items"][result["order"][0].as_str().unwrap()]["disabled"],
            true
        );
        assert!(engine
            .query(serde_json::from_value(json!({"limit":0}))?)
            .is_err());
        engine.loaded.context.inputs_changed();
        std::sync::Arc::make_mut(&mut engine.loaded.context.visible_apps[0])["icon"] =
            json!("https://invalid.example/icon.png");
        let result = engine.query(serde_json::from_value(
            json!({"query":"","mode":"apps","route":"apps"}),
        )?)?;
        assert!(
            result["items"]["fixture"].get("icon").is_none(),
            "Remote icons must never reach an image renderer"
        );
        engine.loaded.context.inputs_changed();
        std::sync::Arc::make_mut(&mut engine.loaded.context.visible_apps[0])["id"] = json!("apps");
        std::sync::Arc::make_mut(&mut engine.loaded.context.visible_apps[0])["name"] =
            json!("All apps");
        let result = engine.query(serde_json::from_value(
            json!({"query":"apps","mode":"search"}),
        )?)?;
        assert!(result["items"].get("apps").is_some());
        assert!(
            result["items"].get("nacre-menu:apps").is_some(),
            "An application and group with the same raw ID must both survive"
        );
        Ok(())
    }
    #[tokio::test(flavor = "current_thread")]
    async fn inotify_handles_creation_and_closed_writes_without_polling() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let mut watches = Watches::new()?;
        watches.add([temp.path().into()])?;
        watches.menu_state = Some(temp.path().to_path_buf());
        fs::write(temp.path().join("recording-audio.lock"), "")?;
        assert!(
            tokio::time::timeout(Duration::from_millis(30), watches.changed())
                .await
                .is_err(),
            "Registry locks must not trigger a scan"
        );
        fs::write(temp.path().join("fixture.json"), "{}")?;
        tokio::time::timeout(Duration::from_secs(2), watches.changed()).await??;
        Ok(())
    }
    #[tokio::test(flavor = "current_thread")]
    async fn application_watch_ignores_mime_cache_but_keeps_desktop_writes() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let mut watches = Watches::new()?;
        watches.add([temp.path().to_path_buf()])?;
        fs::write(temp.path().join("mimeinfo.cache"), "fixture")?;
        assert!(
            tokio::time::timeout(Duration::from_millis(30), watches.changed())
                .await
                .is_err()
        );
        fs::write(temp.path().join("fixture.desktop"), "[Desktop Entry]")?;
        tokio::time::timeout(Duration::from_secs(2), watches.changed()).await??;
        Ok(())
    }
    #[test]
    fn window_titles_refresh_cached_queries_without_geometry_invalidations() -> Result<()> {
        let mut engine = engine()?;
        let mut snapshot = json!({"windowOrder":["1"],"windows":{"1":{"windowId":1,"title":"First fixture caption","wmClass":"fixture","workspace":{"name":"Fixture"},"isFocused":false}}});
        compositor_inputs(&mut engine.loaded.context, &snapshot);
        engine.query(serde_json::from_value(
            json!({"query":"caption","mode":"apps"}),
        )?)?;
        let original = engine.loaded.context.compositor.clone();
        snapshot["windows"]["1"]["isFocused"] = json!(true);
        compositor_inputs(&mut engine.loaded.context, &snapshot);
        assert_eq!(original, engine.loaded.context.compositor);
        snapshot["windows"]["1"]["title"] = json!("Updated unique caption");
        compositor_inputs(&mut engine.loaded.context, &snapshot);
        let result = engine.query(serde_json::from_value(
            json!({"query":"updated unique caption","mode":"apps"}),
        )?)?;
        assert!(result["items"]
            .as_object()
            .unwrap()
            .values()
            .any(|row| row["name"] == "Updated unique caption"));
        Ok(())
    }
}
