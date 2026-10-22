//! Newest saved captures from Pictures/Screenshots. Only regular, user-owned,
//! unlinked image files are listed; the daemon never deletes or rewrites them.
use super::{down, ok, publish, settle, Gate, QUIET};
use crate::{
    daemon::hub::{failure, Shared},
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
    time::UNIX_EPOCH,
};

pub const TOPIC: &str = "screenshots";
const SCAN_LIMIT: usize = 20_000;

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
            },
        );
    }
    Ok(ScreenshotsState {
        health: ok(),
        items,
    })
}

pub async fn refresh(hub: &Shared, paths: &Paths) {
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
    loop {
        {
            let _slot = gate.acquire().await;
            refresh(&hub, &paths).await;
        }
        settle(&mut watch, QUIET).await?;
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
