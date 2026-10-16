//! One inotify reader, preserving each legacy engine's debounced JSON lines.
use super::{client, hub::Shared};
use crate::{json, paths::Paths, Result};
use nacre_protocol::{
    framing, wallpaper::WallpaperWatch, Id, Message, Mirror, Update, MAX_SAFE_INTEGER,
};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    cell::RefCell, collections::BTreeMap, io::Write, os::unix::ffi::OsStrExt, path::Path, rc::Rc,
    time::Duration,
};

pub const TOPIC: &str = "wallpaper.watch";
pub type Generations = Rc<RefCell<BTreeMap<String, u64>>>;

// Raw paths intentionally retain symlink identity: changing an alias must not
// be mistaken for changing its target. Length prefixes delimit arbitrary Unix
// path bytes, and the private topic contains no personal path strings.
fn source(paths: &Paths, root: &Path, scheme: &Path, engine: &Path) -> Result<String> {
    let cwd = std::env::current_dir()?;
    let mut digest = Sha256::new();
    for path in [root, scheme, &paths.config, engine] {
        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };
        let bytes = absolute.as_os_str().as_bytes();
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
    }
    Ok(format!("{:x}", digest.finalize()))
}

pub fn publish(
    hub: &Shared,
    generations: &Generations,
    available: bool,
    error: Option<&str>,
) -> Result<()> {
    let state = WallpaperWatch {
        available,
        generations: generations.borrow().clone(),
        error: error.map(str::to_owned),
    };
    state.validate()?;
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

pub async fn run(hub: Shared, paths: Paths, generations: Generations) -> Result<()> {
    let result = observe(hub.clone(), paths, generations.clone()).await;
    if result.is_err()
        && publish(
            &hub,
            &generations,
            false,
            Some("Wallpaper monitoring is unavailable"),
        )
        .is_err()
    {
        tracing::error!("Cannot publish wallpaper monitor failure");
    }
    result
}

async fn observe(hub: Shared, paths: Paths, generations: Generations) -> Result<()> {
    // Native callers may name the installed helper or the running binary. The
    // legacy default names its bridge. Share directory subscriptions but retain
    // independent counters/deadlines, including when a path is a symlink.
    let mut engines = Vec::new();
    for path in [
        std::env::current_exe()?,
        paths.tools().join("nacre-helper"),
        paths.share.join("shell/bin/nacre_shell"),
    ] {
        if !engines.contains(&path) {
            engines.push(path);
        }
    }
    let root = paths.home.join("Pictures/Wallpapers");
    let scheme = paths.state.join("scheme.json");
    let keys = engines
        .iter()
        .map(|engine| source(&paths, &root, &scheme, engine))
        .collect::<Result<Vec<_>>>()?;
    for key in &keys {
        generations.borrow_mut().entry(key.clone()).or_insert(0);
    }
    crate::wallpaper_watch::observe(&paths, engines, |changed| {
        if let Some(changed) = changed {
            let mut generations = generations.borrow_mut();
            for (index, key) in keys.iter().enumerate() {
                if changed & (1 << index) != 0 {
                    let generation = generations
                        .get_mut(key)
                        .ok_or("Wallpaper source disappeared")?;
                    *generation = generation
                        .checked_add(1)
                        .filter(|n| *n <= MAX_SAFE_INTEGER)
                        .ok_or("Wallpaper generation exhausted")?;
                }
            }
        } else {
            let failures = hub
                .borrow()
                .services
                .get("wallpaper-watch")
                .and_then(|v| v["failures"].as_u64())
                .unwrap_or(0) as usize;
            hub.borrow_mut()
                .service("wallpaper-watch", "running", failures)?;
        }
        publish(&hub, &generations, true, None)
    })
    .await
}

fn changed(last: Option<u64>, current: u64) -> Result<u64> {
    match last {
        None => Ok(0), // Unlike network-state watch, wallpaper-watch has no initial line.
        Some(last) => current
            .checked_sub(last)
            .ok_or_else(|| "Wallpaper generation went backwards".into()),
    }
}

async fn watch_messages(
    mut connection: client::Client,
    key: &str,
    explicit: bool,
) -> Result<Option<i32>> {
    let mut mirror = Mirror::default();
    mirror.receive(&connection.welcome);
    connection.methods.clear();
    connection.welcome = Message::Ping { id: Id::Integer(0) };
    framing::write(
        &mut connection.writer,
        &Message::Sub {
            topics: vec![TOPIC.into()],
        },
    )
    .await?;
    let mut last = None;
    loop {
        let read = connection.reader.next();
        let message = if last.is_some() {
            read.await?
        } else {
            tokio::time::timeout(Duration::from_secs(10), read).await??
        }
        .ok_or("Daemon wallpaper stream ended")?;
        if let Message::Error { err } = &message {
            return Err(err.clone().into());
        }
        match mirror.receive(&message) {
            Update::Resync(topic) => {
                framing::write(
                    &mut connection.writer,
                    &Message::Resync {
                        topics: vec![topic],
                    },
                )
                .await?
            }
            Update::Changed(topic) if topic == TOPIC => {
                let state: WallpaperWatch = serde_json::from_value(mirror.topics[TOPIC].1.clone())?;
                state.validate()?;
                if last.is_none() && !state.available && state.error.is_none() {
                    continue;
                }
                if !state.available {
                    return Err("Daemon wallpaper monitoring is unavailable".into());
                }
                let Some(current) = state.generations.get(key).copied() else {
                    if last.is_none() && !explicit {
                        return Ok(None);
                    }
                    return Err("Daemon wallpaper source is unavailable".into());
                };
                for _ in 0..changed(last, current)? {
                    writeln!(
                        std::io::stdout(),
                        "{}",
                        json::dumps(&json!({"changed":true}), false)?
                    )?;
                }
                std::io::stdout().flush()?;
                last = Some(current);
            }
            _ => {}
        }
    }
}

pub fn watch(paths: &Paths, root: &Path, scheme: &Path, engine: &Path) -> Result<Option<i32>> {
    let explicit = std::env::var_os("NACRE_DAEMON_SOCKET").is_some();
    if !explicit && std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|p| !Path::new(&p).is_absolute())
    {
        return Ok(None);
    }
    let key = source(paths, root, scheme, engine)?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .max_blocking_threads(2)
        .build()?;
    runtime.block_on(async {
        let mut connection = match client::connect().await {
            Ok(connection) => connection,
            Err(client::ConnectError::Absent) if !explicit => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !matches!(&connection.welcome, Message::Welcome { topics, .. } if topics.get(TOPIC) == Some(&1)) {
            let status = connection.call("daemon.status", json!({}), Some(1000)).await?;
            if !explicit && status["safe"] != true { return Ok(None); }
            return Err("Daemon wallpaper watch is unavailable".into());
        }
        tokio::select! {
            result = watch_messages(connection, &key, explicit) => result,
            closed = client::output_closed() => { closed?; Ok(Some(0)) },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn initial_snapshot_is_silent_and_coalesced_changes_keep_the_legacy_line_count() {
        assert_eq!(changed(None, 5).unwrap(), 0);
        assert_eq!(changed(Some(5), 8).unwrap(), 3);
        assert_eq!(changed(Some(8), 8).unwrap(), 0);
        assert!(changed(Some(8), 7).is_err());
    }
    #[test]
    fn watch_identity_includes_each_path_and_preserves_engine_aliases() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let root = home.path().join("pictures");
        let scheme = home.path().join("scheme.json");
        let engine = home.path().join("helper");
        std::fs::write(&engine, "fixture")?;
        let alias = home.path().join("alias");
        std::os::unix::fs::symlink(&engine, &alias)?;
        let key = source(&paths, &root, &scheme, &engine)?;
        assert_ne!(key, source(&paths, &root, &scheme, &alias)?);
        assert_ne!(key, source(&paths, &root.join("nested"), &scheme, &engine)?);
        assert_ne!(
            key,
            source(&paths, &root, &scheme.with_extension("other"), &engine)?
        );
        Ok(())
    }
}
