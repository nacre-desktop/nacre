//! Subscription-scoped lyrics from the existing helper; no second player backend.
use super::hub::{failure, Notice, Shared};
use crate::Result;
use futures_util::future::LocalBoxFuture;
use nacre_protocol::{
    core_services::{DashboardMediaState, Entities, EntityId, Health, LyricLine},
    ErrorCode,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, rc::Rc};
type Loader = Rc<dyn Fn(Value) -> LocalBoxFuture<'static, Result<Option<Value>>>>;

pub const TOPIC: &str = "dashboard.media";
const SETTINGS: &str = "settings.store.dashboard";
fn empty() -> DashboardMediaState {
    DashboardMediaState {
        health: Health {
            available: false,
            stale: false,
            error: None,
        },
        key: String::new(),
        lines: Entities {
            items: BTreeMap::new(),
            order: vec![],
        },
        plain: String::new(),
        palette: BTreeMap::new(),
        lyrics_message: String::new(),
        cache_seconds: 0,
    }
}
fn publish(hub: &Shared, state: DashboardMediaState) -> Result<()> {
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}
pub fn initial(hub: &Shared) -> Result<()> {
    publish(hub, empty())
}
fn metadata(hub: &Shared) -> Option<Value> {
    let settings = hub.borrow().snapshot(SETTINGS)?;
    if !settings["error"].is_null() {
        return None;
    }
    let state = hub.borrow().snapshot(super::dashboard_player::TOPIC)?;
    if state["health"]["available"] != true {
        return None;
    }
    let player = &state["players"]["items"][state["active"].as_str()?];
    let (mut title, mut artist) = (
        player["title"].as_str()?.to_owned(),
        player["artist"].as_str()?.to_owned(),
    );
    // Match QML media.js lyricsMetadata's browser presentation cleanup.
    if regex::Regex::new("(?i)chrome|chromium|firefox|brave|vivaldi|browser")
        .ok()?
        .is_match(player["identity"].as_str().unwrap_or(""))
    {
        let cleaned = regex::Regex::new(r"(?i)\s*[\[(](?:official\s+)?(?:music\s+video|video|audio|lyric\s+video|lyrics|visualizer)[\])]\s*$").ok()?.replace(&title, "").trim().to_owned();
        artist = regex::Regex::new(r"(?i)\s+-\s+Topic$")
            .ok()?
            .replace(&artist, "")
            .trim()
            .to_owned();
        if let Some(parts) = regex::Regex::new(r"^(.+?)\s+[-–—]\s+(.+)$")
            .ok()?
            .captures(&cleaned)
        {
            if parts[1].to_lowercase() == artist.to_lowercase()
                || artist.is_empty() && cleaned != title
            {
                artist = parts[1].trim().into();
                title = parts[2].trim().into();
            } else {
                title = cleaned;
            }
        } else {
            title = cleaned;
        }
    }
    Some(
        json!({"title":title,"artist":artist,"album":player["album"],
        "duration":player["length_ms"].as_u64().unwrap_or(0) as f64 / 1000.0,
        "art":"","lyrics":settings["data"]["lyrics"].as_bool().unwrap_or(true),"palette":false,"mode":"dark"}),
    )
}
fn project(data: Value) -> DashboardMediaState {
    let mut state = empty();
    state.health.available = true;
    state.key = data["key"]
        .as_str()
        .unwrap_or("")
        .chars()
        .take(128)
        .collect();
    state.plain = data["plain"]
        .as_str()
        .unwrap_or("")
        .chars()
        .take(60000)
        .collect();
    state.lyrics_message = data["lyricsMessage"]
        .as_str()
        .unwrap_or("")
        .chars()
        .take(180)
        .collect();
    state.cache_seconds = data["cacheSeconds"].as_u64().unwrap_or(1800).min(86400) as u32;
    let mut lyric_bytes = 0;
    for (index, row) in data["lines"]
        .as_array()
        .into_iter()
        .flatten()
        .take(2000)
        .enumerate()
    {
        let Some(time) = row["time"].as_f64().filter(|t| t.is_finite() && *t >= 0.0) else {
            continue;
        };
        let id = EntityId(format!("l{index}"));
        let line = LyricLine {
            time,
            text: row["text"]
                .as_str()
                .unwrap_or("")
                .chars()
                .take(500)
                .collect(),
        };
        lyric_bytes += serde_json::to_vec(&line).expect("finite lyric line").len();
        // Leave room for plain lyrics, entity IDs and framing under MAX_FRAME.
        if lyric_bytes > 500_000 {
            break;
        }
        state.lines.order.push(id.clone());
        state.lines.items.insert(id, line);
    }
    state
}
// A private client leases player state when media is the only subscriber. It has
// no transport or polling task; its bounded delivery queue disappears on drop.
struct PlayerLease {
    hub: Shared,
    id: u64,
}
impl PlayerLease {
    fn new(hub: &Shared) -> Result<Self> {
        let (id, _) = hub.borrow_mut().add_client()?;
        let lease = Self {
            hub: hub.clone(),
            id,
        };
        hub.borrow_mut()
            .subscribe(id, &[super::dashboard_player::TOPIC.into()], false)?;
        Ok(lease)
    }
}
impl Drop for PlayerLease {
    fn drop(&mut self) {
        self.hub.borrow_mut().remove_client(self.id);
    }
}

pub async fn run(hub: Shared) -> Result<()> {
    run_inner(
        hub,
        Rc::new(|meta| {
            Box::pin(async move {
                Ok(
                    crate::process::offload(move || crate::dashboard_media::track_data(&meta).ok())
                        .await?,
                )
            })
        }),
    )
    .await
}
async fn run_inner(hub: Shared, load: Loader) -> Result<()> {
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    let mut notices = hub.borrow().observe();
    let failures = hub
        .borrow()
        .services
        .get("dashboard-media")
        .and_then(|s| s["failures"].as_u64())
        .unwrap_or(0) as usize;
    hub.borrow_mut()
        .service("dashboard-media", "running", failures)?;
    loop {
        while !*interest.borrow() {
            interest.changed().await?;
        }
        let mut lease = Some(PlayerLease::new(&hub)?);
        let mut attempted = None;
        while *interest.borrow() {
            let meta = metadata(&hub);
            if meta != attempted {
                initial(&hub)?; // Never show lyrics from the previous track.
                attempted = meta.clone();
                if let Some(meta) = meta {
                    let expected = meta.clone();
                    // Keep one in-flight job through interest/track changes. Its
                    // bounded HTTPS request may finish, but stale results cannot publish.
                    let pending = load(meta);
                    tokio::pin!(pending);
                    let data = loop {
                        tokio::select! {
                            result = &mut pending => break result?,
                            changed = interest.changed() => {
                                changed?;
                                if *interest.borrow() {
                                    if lease.is_none() {lease = Some(PlayerLease::new(&hub)?);}
                                } else {
                                    // Release MPRIS immediately, even if the bounded
                                    // helper request has not returned yet.
                                    lease.take();
                                    initial(&hub)?;
                                }
                            },
                        }
                    };
                    if *interest.borrow() && metadata(&hub).as_ref() == Some(&expected) {
                        let state = match data {
                            Some(data) => project(data),
                            None => {
                                let mut state = empty();
                                state.health.error = Some(failure(
                                    ErrorCode::Unavailable,
                                    "Media details unavailable",
                                    true,
                                ));
                                state
                            }
                        };
                        publish(&hub, state)?;
                    } else {
                        continue;
                    }
                }
            }
            tokio::select! {
                changed = interest.changed() => {changed?;},
                notice = notices.recv() => {
                    if !matches!(notice, Ok(Notice::Changed {ref topic,..}) if topic == SETTINGS || topic == super::dashboard_player::TOPIC) {continue;}
                },
            }
        }
        drop(lease);
        initial(&hub)?;
        // A closed page resets its key just as QML does. No idle retry timer.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn enrichment_contract_and_bounds() {
        let state = project(
            json!({"key":"track","lines":[{"time":2.5,"text":"Lyric"},{"time":-1,"text":"bad"}],"plain":"Text","cacheSeconds":864000}),
        );
        assert_eq!(state.lines.order.len(), 1);
        assert_eq!(state.cache_seconds, 86400);
        assert!(nacre_protocol::core_services::CoreSnapshot::Media(state).valid());
        let large = project(
            json!({"plain":"\u{0001}".repeat(60000),"lines":vec![json!({"time":1,"text":"🦀".repeat(500)});2000]}),
        );
        assert!(serde_json::to_vec(&large).unwrap().len() < nacre_protocol::MAX_FRAME - 1024);
    }
    #[tokio::test(flavor = "current_thread")]
    async fn interest_consent_changes_and_late_results() {
        tokio::task::LocalSet::new().run_until(async {
            let hub = super::super::hub::Hub::new("media-test".into(), false, std::time::Duration::from_millis(16)).shared();
            initial(&hub).unwrap();
            hub.borrow_mut().publish(SETTINGS, json!({"error":null,"data":{"lyrics":true}})).unwrap();
            hub.borrow_mut().publish(super::super::dashboard_player::TOPIC, json!({"health":{"available":true},"active":"p","players":{"items":{"p":{"title":"Song","artist":"Artist","album":"Album","identity":"Player","length_ms":1000}}}})).unwrap();
            let calls = Rc::new(std::cell::RefCell::new(Vec::new()));
            let release = Rc::new(tokio::sync::Notify::new());
            let load: Loader = {
                let calls = calls.clone(); let release = release.clone();
                Rc::new(move |meta| {
                    calls.borrow_mut().push(meta.clone());
                    let first = calls.borrow().len() == 1;
                    let release = release.clone();
                    Box::pin(async move {
                        if first {release.notified().await;}
                        Ok(Some(json!({"key":if meta["lyrics"] == true {"old"}else{"disabled"},"lines":[]})))
                    })
                })
            };
            let task = tokio::task::spawn_local(run_inner(hub.clone(), load));
            tokio::task::yield_now().await;
            assert!(calls.borrow().is_empty());
            let player_interest = hub.borrow_mut().watch_interest(super::super::dashboard_player::TOPIC);
            let (client, _) = hub.borrow_mut().add_client().unwrap();
            hub.borrow_mut().subscribe(client, &[TOPIC.into()], false).unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while calls.borrow().is_empty() {tokio::task::yield_now().await;}
            }).await.unwrap();
            assert!(*player_interest.borrow());
            hub.borrow_mut().remove_client(client);
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while *player_interest.borrow() {tokio::task::yield_now().await;}
            }).await.unwrap();
            assert_eq!(calls.borrow().len(), 1);
            let (client, _) = hub.borrow_mut().add_client().unwrap();
            hub.borrow_mut().subscribe(client, &[TOPIC.into()], false).unwrap();
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while !*player_interest.borrow() {tokio::task::yield_now().await;}
            }).await.unwrap();
            hub.borrow_mut().publish(SETTINGS, json!({"error":null,"data":{"lyrics":false}})).unwrap();
            release.notify_one();
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while hub.borrow().snapshot(TOPIC).unwrap()["key"] != "disabled" {tokio::task::yield_now().await;}
            }).await.unwrap();
            assert_eq!(calls.borrow().len(), 2);
            assert_eq!(calls.borrow()[1]["lyrics"], false);
            hub.borrow_mut().remove_client(client);
            tokio::time::timeout(std::time::Duration::from_secs(1), async {
                while *player_interest.borrow() {tokio::task::yield_now().await;}
            }).await.unwrap();
            task.abort(); let _ = task.await;
        }).await;
    }
}
