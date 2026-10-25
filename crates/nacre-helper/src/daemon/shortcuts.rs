//! N0051: the `shortcuts` topic, the key bindings of the running compositor with the Nacre menu
//! route each opens. It is the list the QML Settings "Shortcuts" page showed (`desktop-extras
//! keys`): the compositor is asked once it is up, and again on `shortcuts.refresh`; a config
//! reload is watched. A failed read keeps the last good list, marked stale.
use super::{
    behaviours::changed,
    hub::{failure, Hub, Shared},
};
use crate::{desktop_extras, paths::Paths, Result};
use nacre_protocol::{
    core_services::{Entities, EntityId, Health},
    pill_controls::PillEmpty,
    settings_asks::{Shortcut, ShortcutsState, MAX_SHORTCUTS, SHORTCUTS_TOPIC},
    ErrorCode, Failure, Method,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, sync::Arc, time::Duration};
use tokio::sync::Notify;

const READ_LIMIT: Duration = Duration::from_secs(45);
/// `shortcuts.refresh`: read again. A request made while a read runs is not lost.
static REFRESH: Notify = Notify::const_new();

#[derive(Clone)]
pub struct Env {
    /// The compositor's bindings: `[{key, description, submap, menu}]`.
    pub read: Arc<dyn Fn() -> Result<Value> + Send + Sync>,
    pub files: Arc<dyn Fn() -> Result<Vec<std::path::PathBuf>> + Send + Sync>,
}
impl Env {
    fn production() -> Self {
        Self {
            read: Arc::new(|| desktop_extras::bindings(&Paths::environment()?)),
            files: Arc::new(|| desktop_extras::binding_files(&Paths::environment()?)),
        }
    }
}

fn clean(text: &str, max: usize) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}
fn health(available: bool, stale: bool) -> Health {
    Health {
        available,
        stale,
        error: (!available || stale)
            .then(|| Failure::new(ErrorCode::Unavailable, "Shortcuts are unavailable", true)),
    }
}
fn empty() -> Entities<Shortcut> {
    Entities {
        items: BTreeMap::new(),
        order: Vec::new(),
    }
}
/// The bounded, display-safe projection of the helper's rows. Order is the helper's.
fn items(rows: &Value) -> Entities<Shortcut> {
    let mut all = empty();
    for row in rows.as_array().into_iter().flatten().take(MAX_SHORTCUTS) {
        let text = |name: &str, max: usize| clean(row[name].as_str().unwrap_or(""), max);
        let key = text("key", 64);
        if key.is_empty() {
            continue;
        }
        let some = |value: String| (!value.is_empty()).then_some(value);
        let id = EntityId(format!("k{}", all.order.len()));
        all.order.push(id.clone());
        all.items.insert(
            id,
            Shortcut {
                key,
                description: text("description", 256),
                submap: some(text("submap", 64)),
                menu: some(text("menu", 128)),
            },
        );
    }
    all
}

pub fn initial(hub: &Shared) -> Result<()> {
    let state = ShortcutsState {
        health: health(false, false),
        items: empty(),
    };
    hub.borrow_mut()
        .publish(SHORTCUTS_TOPIC, serde_json::to_value(&state)?)?;
    Ok(())
}
fn compositor_up(hub: &Shared) -> bool {
    hub.borrow()
        .snapshot_ref("compositor")
        .is_some_and(|state| state["available"] == true)
}
fn publish(hub: &Shared, read: Option<Value>) -> Result<()> {
    let last = hub
        .borrow()
        .snapshot(SHORTCUTS_TOPIC)
        .and_then(|value| serde_json::from_value::<ShortcutsState>(value).ok());
    let state = match read {
        Some(rows) => ShortcutsState {
            health: health(true, false),
            items: items(&rows),
        },
        None => {
            let kept = last.map_or_else(empty, |state| state.items);
            ShortcutsState {
                health: health(!kept.items.is_empty(), !kept.items.is_empty()),
                items: kept,
            }
        }
    };
    debug_assert!(state.validate().is_ok());
    hub.borrow_mut()
        .publish(SHORTCUTS_TOPIC, serde_json::to_value(&state)?)?;
    Ok(())
}

pub async fn run(hub: Shared) -> Result<()> {
    run_with(hub, Env::production()).await
}
async fn run_with(hub: Shared, env: Env) -> Result<()> {
    let mut notices = hub.borrow().observe();
    let mut watcher = Watcher::new()?;
    hub.borrow_mut().service("shortcuts", "running", 0)?;
    let (mut want, mut was_up) = (true, false);
    loop {
        let up = compositor_up(&hub);
        want |= up && !was_up;
        was_up = up;
        if want && up {
            want = false;
            let files = env.files.clone();
            if let Ok(Ok(files)) =
                tokio::task::spawn_blocking(move || files().map_err(|e| e.to_string())).await
            {
                watcher.rearm(&files)?;
            }
            let read = env.read.clone();
            let outcome = tokio::time::timeout(
                READ_LIMIT,
                tokio::task::spawn_blocking(move || read().map_err(|error| error.to_string())),
            )
            .await;
            publish(
                &hub,
                match outcome {
                    Ok(Ok(Ok(rows))) => Some(rows),
                    _ => None,
                },
            )?;
        }
        tokio::select! {
            result = changed(&mut notices, &["compositor"]) => result?,
            () = REFRESH.notified() => want = true,
            result = watcher.changed() => { result?; want = true; },
        }
    }
}

pub fn handles(method: &str) -> bool {
    method == "shortcuts.refresh"
}
/// Advertised while the service runs.
pub fn available_methods(hub: &Hub) -> BTreeMap<String, Method> {
    let mut methods = nacre_protocol::settings_asks::methods();
    methods.retain(|name, _| handles(name));
    if hub.safe || !hub.services.contains_key("shortcuts") {
        methods.clear();
    }
    methods
}
pub fn dispatch(hub: &Hub, params: Value) -> std::result::Result<Value, Failure> {
    serde_json::from_value::<PillEmpty>(params).map_err(|_| {
        failure(ErrorCode::Validation, "Invalid shortcuts request", false).dispatch_state(false)
    })?;
    if hub.safe || !hub.services.contains_key("shortcuts") {
        return Err(
            failure(ErrorCode::Unavailable, "Shortcuts are unavailable", true)
                .dispatch_state(false),
        );
    }
    REFRESH.notify_one();
    Ok(json!({"requested":true}))
}

/// Watch containing directories as well as symlink targets: atomic replacement
/// and N0078's in-place writes must both survive without a polling timer.
struct Watcher {
    fd: tokio::io::unix::AsyncFd<rustix::fd::OwnedFd>,
    directories: BTreeMap<std::path::PathBuf, i32>,
}
impl Watcher {
    #[allow(unsafe_code)]
    fn new() -> Result<Self> {
        use rustix::fs::inotify::{init, CreateFlags};
        let fd = init(CreateFlags::NONBLOCK | CreateFlags::CLOEXEC)?;
        Ok(Self {
            // SAFETY: this nonblocking descriptor is owned for the AsyncFd lifetime.
            fd: unsafe {
                tokio::io::unix::AsyncFd::register_with_interest(fd, tokio::io::Interest::READABLE)?
            },
            directories: BTreeMap::new(),
        })
    }
    fn rearm(&mut self, files: &[std::path::PathBuf]) -> Result<()> {
        use rustix::fs::inotify::{add_watch, remove_watch, WatchFlags};
        let wanted: std::collections::BTreeSet<_> = files
            .iter()
            .flat_map(|p| [Some(p.clone()), std::fs::canonicalize(p).ok()])
            .flatten()
            .filter_map(|p| {
                p.parent()
                    .and_then(|p| p.ancestors().find(|p| p.is_dir()))
                    .map(std::path::Path::to_path_buf)
            })
            .collect();
        self.directories.retain(|path, wd| {
            if wanted.contains(path) {
                true
            } else {
                let _ = remove_watch(self.fd.get_ref(), *wd);
                false
            }
        });
        for path in wanted {
            let wd = add_watch(
                self.fd.get_ref(),
                &path,
                WatchFlags::CLOSE_WRITE
                    | WatchFlags::MOVED_TO
                    | WatchFlags::MOVED_FROM
                    | WatchFlags::CREATE
                    | WatchFlags::DELETE
                    | WatchFlags::DELETE_SELF
                    | WatchFlags::MOVE_SELF,
            )?;
            self.directories.insert(path, wd);
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
mod tests;
