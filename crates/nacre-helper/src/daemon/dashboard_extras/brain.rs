//! Recent Brain notes and open tasks. The vault is read only; a change below the
//! vault or its first two directory levels refreshes the topic after a quiet period.
use super::{down, ok, publish, settle, Gate};
use crate::daemon::hub::Shared;
use crate::{dashboard_data, Result};
use nacre_protocol::{
    core_services::{Entities, EntityId},
    dashboard_extras::{BrainNote, BrainRecentState, BrainTask, MAX_BRAIN_NOTES, MAX_BRAIN_TASKS},
};
use rustix::fs::inotify::WatchFlags;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::PathBuf, time::Duration};

pub const TOPIC: &str = "brain.recent";
const QUIET: Duration = Duration::from_secs(2);

fn empty<T>() -> Entities<T> {
    Entities {
        items: BTreeMap::new(),
        order: vec![],
    }
}
pub fn initial() -> BrainRecentState {
    BrainRecentState {
        health: down(false, None),
        notes: empty(),
        tasks: empty(),
    }
}
fn entity(prefix: &str, text: &str) -> EntityId {
    let digest = Sha256::digest(text.as_bytes());
    EntityId(format!(
        "{prefix}{}",
        digest[..8]
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    ))
}

fn load(root: &std::path::Path) -> Result<BrainRecentState> {
    let value = dashboard_data::brain_context_at(root)?;
    let (mut notes, mut tasks) = (empty::<BrainNote>(), empty::<BrainTask>());
    for row in value["recent"]
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_BRAIN_NOTES)
    {
        let (title, path) = (
            row["title"].as_str().unwrap_or(""),
            row["path"].as_str().unwrap_or(""),
        );
        let id = entity("n", path);
        if !notes.items.contains_key(&id) {
            notes.order.push(id.clone());
            notes.items.insert(
                id,
                BrainNote {
                    title: title.into(),
                    path: path.into(),
                },
            );
        }
    }
    for row in value["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .take(MAX_BRAIN_TASKS)
    {
        let (text, path) = (
            row["text"].as_str().unwrap_or(""),
            row["path"].as_str().unwrap_or(""),
        );
        let id = entity("t", &format!("{path}\n{text}"));
        if !tasks.items.contains_key(&id) {
            tasks.order.push(id.clone());
            tasks.items.insert(
                id,
                BrainTask {
                    text: text.into(),
                    path: path.into(),
                },
            );
        }
    }
    Ok(BrainRecentState {
        health: ok(),
        notes,
        tasks,
    })
}

fn directories(root: &std::path::Path) -> Vec<(PathBuf, WatchFlags)> {
    let flags = WatchFlags::CLOSE_WRITE
        | WatchFlags::MOVED_TO
        | WatchFlags::MOVED_FROM
        | WatchFlags::DELETE
        | WatchFlags::CREATE;
    let mut dirs = walkdir::WalkDir::new(root)
        .max_depth(2)
        .into_iter()
        .filter_entry(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_dir())
        .take(64)
        .map(|e| (e.into_path(), flags))
        .collect::<Vec<_>>();
    // A missing vault is watched through its parent until it appears.
    if dirs.is_empty() {
        dirs.push((root.to_path_buf(), flags));
    }
    dirs
}

async fn refresh(hub: &Shared) -> Vec<(PathBuf, WatchFlags)> {
    let read = tokio::task::spawn_blocking(move || {
        let root = crate::brain_data::configured_root_path()?;
        Ok::<_, crate::Error>((load(&root)?, directories(&root)))
    })
    .await;
    match read {
        Ok(Ok((state, dirs))) => {
            if publish(hub, TOPIC, &state).is_err() {
                tracing::warn!("Brain state could not be published");
            }
            dirs
        }
        _ => {
            let mut state = hub
                .borrow()
                .snapshot(TOPIC)
                .and_then(|v| serde_json::from_value::<BrainRecentState>(v).ok())
                .unwrap_or_else(initial);
            state.health = down(
                !state.notes.items.is_empty(),
                Some("Brain vault is unavailable"),
            );
            if publish(hub, TOPIC, &state).is_err() {
                tracing::warn!("Brain state could not be published");
            }
            vec![]
        }
    }
}

pub async fn run(hub: Shared, gate: Gate) -> Result<()> {
    let mut watch = super::watch::Watch::new(vec![])?;
    loop {
        {
            let _slot = gate.acquire().await;
            let dirs = refresh(&hub).await;
            watch.set(dirs);
        }
        settle(&mut watch, QUIET).await?;
    }
}
