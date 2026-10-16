//! One NetworkManager signal source; one-release clients retain `changed` lines.
use super::{client, hub::Shared};
use crate::Result;
use nacre_protocol::{
    framing, network::NetworkWatch, Id, Message, Mirror, Update, MAX_SAFE_INTEGER,
};
use serde_json::json;
use std::{cell::Cell, io::Write, path::Path, rc::Rc, time::Duration};

pub const TOPIC: &str = "network.watch";

pub fn publish(hub: &Shared, generation: u64, available: bool, error: Option<&str>) -> Result<()> {
    let state = NetworkWatch {
        available,
        generation,
        error: error.map(str::to_owned),
    };
    state.validate()?;
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

pub async fn run(hub: Shared, generation: Rc<Cell<u64>>) -> Result<()> {
    let result = observe(hub.clone(), generation.clone()).await;
    if result.is_err()
        && publish(
            &hub,
            generation.get(),
            false,
            Some("Network monitoring is unavailable"),
        )
        .is_err()
    {
        tracing::error!("Cannot publish network monitor failure");
    }
    result
}

async fn observe(hub: Shared, generation: Rc<Cell<u64>>) -> Result<()> {
    let connection = tokio::time::timeout(
        Duration::from_secs(5),
        zbus::connection::Builder::system()?
            .method_timeout(Duration::from_secs(5))
            .build(),
    )
    .await??;
    let mut running = false;
    crate::network_state::watch_events(&connection, || {
        let next = generation
            .get()
            .checked_add(1)
            .filter(|n| *n <= MAX_SAFE_INTEGER)
            .ok_or("Network event generation exhausted")?;
        generation.set(next);
        publish(&hub, next, true, None)?;
        // The lifecycle state changes once per reader, not once per signal.
        if !running {
            let failures = hub
                .borrow()
                .services
                .get("network-watch")
                .and_then(|v| v["failures"].as_u64())
                .unwrap_or(0) as usize;
            hub.borrow_mut()
                .service("network-watch", "running", failures)?;
            running = true;
        }
        Ok(true)
    })
    .await?;
    Err("Network monitor ended".into())
}

fn changed(last: Option<u64>, state: &NetworkWatch) -> Result<u64> {
    state.validate()?;
    if !state.available {
        return Err("Daemon network monitoring is unavailable".into());
    }
    match last {
        Some(last) => state
            .generation
            .checked_sub(last)
            .ok_or_else(|| "Network event generation went backwards".into()),
        None => Ok(1),
    }
}

async fn watch_messages(mut connection: client::Client) -> Result<i32> {
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
    let mut ready = false;
    loop {
        let read = connection.reader.next();
        let message = if ready {
            read.await?
        } else {
            tokio::time::timeout(Duration::from_secs(10), read).await??
        }
        .ok_or("Daemon network stream ended")?;
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
                .await?;
            }
            Update::Changed(topic) if topic == TOPIC => {
                let state: NetworkWatch = serde_json::from_value(mirror.topics[TOPIC].1.clone())?;
                state.validate()?;
                if !ready && !state.available && state.error.is_none() {
                    continue;
                }
                let count = changed(last, &state)?;
                // A coalesced snapshot may cover several signals. Preserve each
                // legacy line without sending one wire frame per source event.
                for _ in 0..count {
                    writeln!(std::io::stdout(), "changed")?;
                }
                std::io::stdout().flush()?;
                last = Some(state.generation);
                ready = true;
            }
            _ => {}
        }
    }
}

pub fn watch() -> Result<Option<i32>> {
    let explicit = std::env::var_os("NACRE_DAEMON_SOCKET").is_some();
    if !explicit && std::env::var_os("XDG_RUNTIME_DIR").is_none_or(|p| !Path::new(&p).is_absolute())
    {
        return Ok(None);
    }
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
            return Err("Daemon network watch is unavailable".into());
        }
        tokio::select! {
            result = watch_messages(connection) => result.map(Some),
            closed = client::output_closed() => { closed?; Ok(Some(0)) },
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn coalesced_changes_preserve_line_count_but_initial_state_only_refreshes_once() {
        let mut state = NetworkWatch {
            available: true,
            generation: 8,
            error: None,
        };
        assert_eq!(changed(None, &state).unwrap(), 1);
        assert_eq!(changed(Some(5), &state).unwrap(), 3);
        assert_eq!(changed(Some(8), &state).unwrap(), 0);
        assert!(changed(Some(9), &state).is_err());
        state.available = false;
        assert!(changed(Some(8), &state).is_err());
        state.available = true;
        state.generation = MAX_SAFE_INTEGER + 1;
        assert!(changed(None, &state).is_err());
    }
}
