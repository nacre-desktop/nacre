//! Newest saved captures from Pictures/Screenshots. Only regular, user-owned,
//! unlinked image files are listed; the daemon never deletes or rewrites them.
use super::{down, ok, publish, settle, Gate, QUIET};
use crate::{
    daemon::{
        hub::{failure, Shared},
        notifications::ImageCache,
    },
    paths::Paths,
    Result,
};
use nacre_protocol::{
    core_services::{Entities, EntityId},
    dashboard_extras::{Screenshot, ScreenshotsState, MAX_SCREENSHOTS},
    ErrorCode, Failure,
};
use rustix::fs::inotify::WatchFlags;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::UNIX_EPOCH,
};

pub const TOPIC: &str = "screenshots";
const SCAN_LIMIT: usize = 20_000;
const THUMB_SIDE: u32 = 256;
const THUMB_SOURCE_BYTES: usize = 32 * 1024 * 1024;

/// Private thumbnails of the listed captures, kept only while the topic has a
/// subscriber. A capture is decoded once per (mtime, size) identity, failures included.
#[derive(Default)]
pub struct Thumbs {
    cache: Option<ImageCache>,
    known: BTreeMap<PathBuf, ((u64, u64), Option<String>)>,
}
impl Thumbs {
    fn apply(&mut self, root: &Path, wanted: bool, state: &mut ScreenshotsState) {
        for shot in state.items.items.values_mut() {
            shot.thumb_path = None;
        }
        if !wanted {
            if let Some(cache) = self.cache.take() {
                let _ = cache.retain(&[]);
            }
            self.known.clear();
            return;
        }
        if self.cache.is_none() {
            self.cache = ImageCache::open(root.to_path_buf()).ok();
        }
        let Some(cache) = &self.cache else { return };
        let mut kept = BTreeMap::new();
        for shot in state.items.items.values_mut() {
            let path = PathBuf::from(&shot.path);
            let Some(meta) = regular(&path) else { continue };
            let identity = (
                meta.mtime() as u64 * 1000 + meta.mtime_nsec() as u64 / 1_000_000,
                meta.len(),
            );
            let thumb = match self.known.remove(&path) {
                Some((seen, thumb))
                    if seen == identity
                        && thumb.as_deref().is_none_or(|t| Path::new(t).exists()) =>
                {
                    thumb
                }
                _ => cache
                    .resolve_thumbnail(&path, THUMB_SOURCE_BYTES, THUMB_SIDE)
                    .ok(),
            };
            shot.thumb_path.clone_from(&thumb);
            kept.insert(path, (identity, thumb));
        }
        self.known = kept;
        let live: Vec<_> = self.known.values().filter_map(|(_, t)| t.clone()).collect();
        let _ = cache.retain(&live);
    }
}
pub type SharedThumbs = Arc<Mutex<Thumbs>>;

fn empty() -> Entities<Screenshot> {
    Entities {
        items: BTreeMap::new(),
        order: vec![],
    }
}
pub fn initial() -> ScreenshotsState {
    ScreenshotsState {
        health: down(false, None),
        items: empty(),
    }
}
fn entity(name: &str) -> EntityId {
    let digest = Sha256::digest(name.as_bytes());
    EntityId(format!(
        "s{}",
        digest[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

fn regular(path: &Path) -> Option<std::fs::Metadata> {
    let meta = std::fs::symlink_metadata(path).ok()?;
    (meta.is_file() && meta.uid() == nix::unistd::getuid().as_raw()).then_some(meta)
}
fn scan(folder: &Path) -> Result<ScreenshotsState> {
    match std::fs::symlink_metadata(folder) {
        Ok(meta) if meta.is_dir() => {}
        Ok(_) => return Err("Screenshot folder is linked or not a directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ScreenshotsState {
                health: ok(),
                items: empty(),
            })
        }
        Err(error) => return Err(error.into()),
    }
    let mut found = Vec::new();
    for entry in std::fs::read_dir(folder)?.take(SCAN_LIMIT).flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if !["png", "jpg", "webp"]
            .iter()
            .any(|e| name.ends_with(&format!(".{e}")))
        {
            continue;
        }
        let path = folder.join(name);
        let Some(meta) = regular(&path) else { continue };
        let created = meta
            .modified()?
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64);
        found.push((created, name.to_owned(), path));
    }
    found.sort_by(|a, b| b.cmp(a));
    let mut items = empty();
    for (created_ms, name, path) in found.into_iter().take(MAX_SCREENSHOTS) {
        let id = entity(&name);
        items.order.push(id.clone());
        items.items.insert(
            id,
            Screenshot {
                path: path.to_string_lossy().into_owned(),
                created_ms,
                thumb_path: None,
            },
        );
    }
    Ok(ScreenshotsState {
        health: ok(),
        items,
    })
}

pub async fn refresh(hub: &Shared, paths: &Paths, thumbs: &SharedThumbs, wanted: bool) {
    let folder = paths.screenshots();
    let state = match tokio::task::spawn_blocking(move || scan(&folder)).await {
        Ok(Ok(state)) => state,
        _ => {
            let mut state = hub
                .borrow()
                .snapshot(TOPIC)
                .and_then(|v| serde_json::from_value::<ScreenshotsState>(v).ok())
                .unwrap_or_else(initial);
            state.health = down(
                !state.items.items.is_empty(),
                Some("Screenshots are unavailable"),
            );
            state
        }
    };
    let (root, shared) = (paths.state.join("shell/screenshot-thumbs"), thumbs.clone());
    let state = tokio::task::spawn_blocking(move || {
        let mut state = state;
        shared
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .apply(&root, wanted, &mut state);
        state
    })
    .await
    .unwrap_or_else(|_| initial());
    if publish(hub, TOPIC, &state).is_err() {
        tracing::warn!("Screenshot state could not be published");
    }
}

pub async fn run(hub: Shared, paths: Paths, gate: Gate) -> Result<()> {
    let mut watch = super::watch::Watch::new(vec![(
        paths.screenshots(),
        WatchFlags::CLOSE_WRITE
            | WatchFlags::MOVED_TO
            | WatchFlags::MOVED_FROM
            | WatchFlags::DELETE
            | WatchFlags::CREATE,
    )])?;
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    let thumbs = SharedThumbs::default();
    loop {
        {
            let _slot = gate.acquire().await;
            let wanted = *interest.borrow();
            refresh(&hub, &paths, &thumbs, wanted).await;
        }
        // Any wake-up refreshes, so a cancelled settle never loses a change.
        tokio::select! {
            changed = settle(&mut watch, QUIET) => changed?,
            changed = interest.changed() => changed?,
        }
    }
}

pub async fn open(
    hub: &Shared,
    paths: &Paths,
    id: &EntityId,
) -> std::result::Result<Value, Failure> {
    let state = hub
        .borrow()
        .snapshot(TOPIC)
        .and_then(|v| serde_json::from_value::<ScreenshotsState>(v).ok());
    let path = state
        .and_then(|s| s.items.items.get(id).map(|s| PathBuf::from(&s.path)))
        .filter(|path| {
            path.parent() == Some(paths.screenshots().as_path()) && regular(path).is_some()
        })
        .ok_or_else(|| {
            failure(
                ErrorCode::StaleRevision,
                "Screenshot no longer exists",
                true,
            )
        })?;
    let mut child = tokio::process::Command::new("xdg-open")
        .arg(path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|_| {
            failure(
                ErrorCode::Unavailable,
                "No screenshot viewer is available",
                true,
            )
        })?;
    // Reap the viewer launcher without bounding the application it starts.
    tokio::task::spawn_local(async move {
        let _ = child.wait().await;
    });
    Ok(json!({"changed":false}))
}
