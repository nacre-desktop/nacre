//! Clipboard topic over the native cliphist index (N0026). Previews are the
//! single-line labels the dashboard already shows; content never leaves cliphist.
use super::{down, ok, publish, settle, Gate, QUIET};
use crate::{
    daemon::hub::{failure, Shared},
    desktop_extras,
    paths::Paths,
    Result,
};
use nacre_protocol::{
    core_services::{Entities, EntityId},
    dashboard_extras::{ClipEntry, ClipKind, ClipboardState, MAX_CLIPBOARD_ENTRIES},
    ErrorCode, Failure,
};
use rustix::fs::inotify::WatchFlags;
use serde_json::{json, Value};
use std::{collections::BTreeMap, time::Duration};

pub const TOPIC: &str = "clipboard";
const WORK: Duration = Duration::from_secs(10);

fn empty() -> Entities<ClipEntry> {
    Entities {
        items: BTreeMap::new(),
        order: vec![],
    }
}
pub fn initial() -> ClipboardState {
    ClipboardState {
        health: down(false, None),
        entries: empty(),
    }
}
fn entity(id: &str) -> EntityId {
    EntityId(format!("c{id}"))
}

fn cliphist_directory(paths: &Paths) -> std::path::PathBuf {
    std::env::var_os("CLIPHIST_DB_PATH")
        .map(std::path::PathBuf::from)
        .and_then(|db| db.parent().map(std::path::Path::to_path_buf))
        .or_else(|| {
            std::env::var_os("XDG_CACHE_HOME")
                .map(std::path::PathBuf::from)
                .filter(|p| p.is_absolute())
                .map(|p| p.join("cliphist"))
        })
        .unwrap_or_else(|| paths.home.join(".cache/cliphist"))
}

pub async fn refresh(hub: &Shared, paths: &Paths) {
    let work = paths.clone();
    let read = tokio::time::timeout(
        WORK,
        tokio::task::spawn_blocking(move || {
            desktop_extras::clip_index(&work, MAX_CLIPBOARD_ENTRIES)
        }),
    )
    .await;
    let state = match read {
        Ok(Ok(Ok(rows))) => {
            let mut entries = empty();
            for row in rows {
                let id = entity(&row.id);
                entries.order.push(id.clone());
                entries.items.insert(
                    id,
                    ClipEntry {
                        kind: if row.image {
                            ClipKind::Image
                        } else {
                            ClipKind::Text
                        },
                        preview: row.label,
                        created_ms: row.created.saturating_mul(1000),
                    },
                );
            }
            ClipboardState {
                health: ok(),
                entries,
            }
        }
        _ => {
            // Keep the last good rows, visibly stale.
            let mut state = hub
                .borrow()
                .snapshot(TOPIC)
                .and_then(|v| serde_json::from_value::<ClipboardState>(v).ok())
                .unwrap_or_else(initial);
            state.health = down(
                !state.entries.items.is_empty(),
                Some("Clipboard history is unavailable"),
            );
            state
        }
    };
    if publish(hub, TOPIC, &state).is_err() {
        tracing::warn!("Clipboard state could not be published");
    }
}

pub async fn run(hub: Shared, paths: Paths, gate: Gate) -> Result<()> {
    let mut watch = super::watch::Watch::new(vec![(
        cliphist_directory(&paths),
        WatchFlags::CLOSE_WRITE | WatchFlags::MOVED_TO,
    )])?;
    loop {
        {
            let _slot = gate.acquire().await;
            refresh(&hub, &paths).await;
        }
        settle(&mut watch, QUIET).await?;
    }
}

pub async fn copy(
    hub: &Shared,
    paths: &Paths,
    id: &EntityId,
) -> std::result::Result<Value, Failure> {
    let known = hub
        .borrow()
        .snapshot(TOPIC)
        .and_then(|v| serde_json::from_value::<ClipboardState>(v).ok())
        .is_some_and(|s| s.entries.items.contains_key(id));
    let digits =
        id.0.strip_prefix('c')
            .filter(|d| !d.is_empty() && d.bytes().all(|c| c.is_ascii_digit()));
    let (true, Some(digits)) = (known, digits) else {
        return Err(failure(
            ErrorCode::StaleRevision,
            "Clipboard entry no longer exists",
            true,
        ));
    };
    let (work, digits) = (paths.clone(), digits.to_owned());
    match tokio::time::timeout(
        WORK,
        tokio::task::spawn_blocking(move || desktop_extras::clip_copy(&work, &digits)),
    )
    .await
    {
        Ok(Ok(Ok(()))) => Ok(json!({"changed":true})),
        _ => Err(failure(ErrorCode::Io, "Clipboard copy failed", true)),
    }
}

pub async fn clear(hub: &Shared, paths: &Paths) -> std::result::Result<Value, Failure> {
    let work = paths.clone();
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(move || desktop_extras::clip_clear(&work)),
    )
    .await;
    refresh(hub, paths).await;
    match result {
        Ok(Ok(Ok(changed))) => Ok(json!({"changed":changed})),
        _ => Err(failure(
            ErrorCode::Io,
            "Clipboard history could not be cleared",
            true,
        )),
    }
}

/// N0051: clear the whole history, pinned clips included (the Privacy page; `clear` keeps pins).
pub async fn clear_history(hub: &Shared, paths: &Paths) -> std::result::Result<Value, Failure> {
    let work = paths.clone();
    let result = tokio::time::timeout(
        Duration::from_secs(30),
        tokio::task::spawn_blocking(move || desktop_extras::clip_wipe(&work)),
    )
    .await;
    refresh(hub, paths).await;
    match result {
        Ok(Ok(Ok(changed))) => Ok(json!({"changed":changed})),
        _ => Err(failure(
            ErrorCode::Io,
            "Clipboard history could not be cleared",
            true,
        )),
    }
}
