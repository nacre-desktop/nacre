//! `dashboard.player`: MPRIS players projected from D-Bus signals, only while a
//! client subscribes. Nothing polls; artwork is cached into a private local cache.
use super::{
    hub::{failure, Shared},
    notifications::ImageCache,
};
use crate::{paths::Paths, Result};
use futures_util::StreamExt;
use nacre_protocol::{
    core_services::{
        CoreCall, CoreSnapshot, Entities, EntityId, Health, Player, PlayerState, PlayerStatus,
    },
    Capability, ErrorCode, Failure, Method, UI_CALL_TIMEOUT_MS,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{mpsc, oneshot, Semaphore};
use zbus::zvariant::{OwnedValue, Value as Variant};

pub const TOPIC: &str = "dashboard.player";
const PREFIX: &str = "org.mpris.MediaPlayer2.";
const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const ROOT: &str = "org.mpris.MediaPlayer2";
const MAX_PLAYERS: usize = 16;
const ART_URL_BYTES: usize = 4096;
const ART_BYTES: u64 = 4 * 1024 * 1024;
const COALESCE: Duration = Duration::from_millis(75);
const RETRY: Duration = Duration::from_secs(5);

pub fn methods() -> BTreeMap<String, Method> {
    let schema: Value = serde_json::from_str(include_str!(
        "../../../../protocol/schema/core-service-call.json"
    ))
    .expect("core call schema");
    [
        "player.play_pause",
        "player.next",
        "player.previous",
        "player.seek",
    ]
    .into_iter()
    .map(|method| {
        let variant = schema["oneOf"]
            .as_array()
            .expect("core call variants")
            .iter()
            .find(|variant| variant["properties"]["m"]["const"] == method)
            .expect("player call variant");
        (
            method.into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout: UI_CALL_TIMEOUT_MS,
                params: json!({"$schema":schema["$schema"],"$defs":schema["$defs"],
                        "$ref":variant["properties"]["p"]["$ref"]}),
                result: serde_json::from_str(include_str!(
                    "../../../../protocol/schema/core-service-result.json"
                ))
                .expect("core result schema"),
            },
        )
    })
    .collect()
}

/// What one player reports on the bus, before projection.
#[derive(Clone, Debug, Default, PartialEq)]
struct Raw {
    identity: String,
    status: Option<PlayerStatus>,
    title: String,
    artist: String,
    album: String,
    art_url: Option<String>,
    track: Option<String>,
    position_us: Option<i64>,
    length_us: Option<i64>,
    can_control: bool,
    can_play: bool,
    can_pause: bool,
    can_next: bool,
    can_previous: bool,
    can_seek: bool,
}

fn peel<'a>(mut value: &'a Variant<'a>) -> &'a Variant<'a> {
    while let Variant::Value(inner) = value {
        value = inner;
    }
    value
}
fn text(value: &Variant<'_>) -> Option<String> {
    match peel(value) {
        Variant::Str(s) => Some(s.as_str().to_owned()),
        _ => None,
    }
}
fn number(value: &Variant<'_>) -> Option<i64> {
    match peel(value) {
        Variant::I64(n) => Some(*n),
        Variant::U64(n) => i64::try_from(*n).ok(),
        Variant::I32(n) => Some(i64::from(*n)),
        Variant::U32(n) => Some(i64::from(*n)),
        _ => None,
    }
}
fn flag(props: &HashMap<String, OwnedValue>, name: &str) -> bool {
    matches!(props.get(name).map(|v| peel(v)), Some(Variant::Bool(true)))
}
fn clip(value: &str, limit: usize) -> String {
    value.chars().take(limit).collect()
}

impl Raw {
    fn read(identity: String, props: &HashMap<String, OwnedValue>) -> Self {
        let mut raw = Raw {
            identity: clip(&identity, 256),
            status: props
                .get("PlaybackStatus")
                .and_then(|v| text(v))
                .and_then(|s| match s.as_str() {
                    "Playing" => Some(PlayerStatus::Playing),
                    "Paused" => Some(PlayerStatus::Paused),
                    "Stopped" => Some(PlayerStatus::Stopped),
                    _ => None,
                }),
            // A missing Position is unknown, and so are negative placeholders.
            position_us: props
                .get("Position")
                .and_then(|v| number(v))
                .filter(|n| *n >= 0),
            can_control: flag(props, "CanControl"),
            can_play: flag(props, "CanPlay"),
            can_pause: flag(props, "CanPause"),
            can_next: flag(props, "CanGoNext"),
            can_previous: flag(props, "CanGoPrevious"),
            can_seek: flag(props, "CanSeek"),
            ..Raw::default()
        };
        if let Some(Variant::Dict(metadata)) = props.get("Metadata").map(|v| peel(v)) {
            for (key, value) in metadata.iter() {
                let Some(key) = text(key) else { continue };
                match key.as_str() {
                    "xesam:title" => raw.title = clip(&text(value).unwrap_or_default(), 512),
                    "xesam:album" => raw.album = clip(&text(value).unwrap_or_default(), 512),
                    "xesam:artist" => {
                        raw.artist = match peel(value) {
                            Variant::Array(items) => clip(
                                &items.iter().filter_map(text).collect::<Vec<_>>().join(", "),
                                512,
                            ),
                            other => clip(&text(other).unwrap_or_default(), 512),
                        };
                    }
                    "mpris:artUrl" => {
                        raw.art_url = text(value).filter(|url| {
                            !url.is_empty()
                                && (url.starts_with("data:") || url.len() <= ART_URL_BYTES)
                        })
                    }
                    "mpris:length" => raw.length_us = number(value).filter(|n| *n > 0),
                    "mpris:trackid" => {
                        raw.track = match peel(value) {
                            Variant::ObjectPath(path) => Some(path.as_str().to_owned()),
                            other => text(other),
                        }
                        .filter(|t| zbus::zvariant::ObjectPath::try_from(t.as_str()).is_ok());
                    }
                    _ => {}
                }
            }
        }
        raw
    }
    fn playing(&self) -> bool {
        self.status == Some(PlayerStatus::Playing)
    }
    fn can_toggle(&self) -> bool {
        self.can_control
            && if self.playing() {
                self.can_pause
            } else {
                self.can_play
            }
    }
    fn can_seek(&self) -> bool {
        self.can_control
            && self.can_seek
            && self.track.is_some()
            && self.position_us.is_some()
            && self.length_us.is_some()
    }
}

struct Tracked {
    id: EntityId,
    owner: String,
    raw: Raw,
    /// (source URL, private cached path once ready)
    art: Option<(String, Option<String>)>,
    read_ms: u64,
}

#[derive(Default)]
struct Model {
    next: u64,
    players: BTreeMap<String, Tracked>,
    /// Last player that started playing; kept while it still plays.
    recent: Option<EntityId>,
}

impl Model {
    fn upsert(&mut self, name: &str, owner: String, raw: Raw, now_ms: u64) -> bool {
        if !self.players.contains_key(name) {
            if self.players.len() >= MAX_PLAYERS {
                return false;
            }
            self.next += 1;
            self.players.insert(
                name.into(),
                Tracked {
                    id: EntityId(format!("player-{}", self.next)),
                    owner: owner.clone(),
                    raw: Raw::default(),
                    art: None,
                    read_ms: 0,
                },
            );
        }
        let entry = self.players.get_mut(name).expect("inserted player");
        if raw.playing() && !entry.raw.playing() {
            self.recent = Some(entry.id.clone());
        }
        if entry.raw.art_url != raw.art_url {
            entry.art = None;
        }
        entry.owner = owner;
        entry.raw = raw;
        entry.read_ms = now_ms;
        true
    }
    fn remove(&mut self, name: &str) {
        if let Some(gone) = self.players.remove(name) {
            if self.recent.as_ref() == Some(&gone.id) {
                self.recent = None;
            }
        }
    }
    fn active(&self) -> Option<&Tracked> {
        let recent = self
            .recent
            .as_ref()
            .and_then(|id| self.players.values().find(|t| &t.id == id));
        recent
            .filter(|t| t.raw.playing())
            .or_else(|| self.players.values().find(|t| t.raw.playing()))
            .or(recent)
            .or_else(|| self.players.values().next())
    }
    fn state(&self, health: Health) -> PlayerState {
        let mut rows: Vec<_> = self.players.values().collect();
        rows.sort_by_key(|t| {
            t.id.0
                .strip_prefix("player-")
                .and_then(|n| n.parse::<u64>().ok())
        });
        let mut items = BTreeMap::new();
        let mut order = Vec::new();
        for t in rows {
            let r = &t.raw;
            let length_ms = r.length_us.map(|us| (us / 1000).max(1) as u64);
            let position_ms = r
                .position_us
                .map(|us| (us / 1000) as u64)
                .map(|ms| length_ms.map_or(ms, |l| ms.min(l)));
            items.insert(
                t.id.clone(),
                Player {
                    identity: r.identity.clone(),
                    status: r.status.unwrap_or(PlayerStatus::Stopped),
                    title: r.title.clone(),
                    artist: r.artist.clone(),
                    album: r.album.clone(),
                    art_path: t.art.as_ref().and_then(|(_, path)| path.clone()),
                    position_ms,
                    position_updated_ms: position_ms.map(|_| t.read_ms),
                    length_ms,
                    can_control: r.can_control,
                    can_play_pause: r.can_toggle(),
                    can_next: r.can_control && r.can_next,
                    can_previous: r.can_control && r.can_previous,
                    can_seek: r.can_seek() && length_ms.is_some(),
                },
            );
            order.push(t.id.clone());
        }
        PlayerState {
            health,
            active: self.active().map(|t| t.id.clone()),
            players: Entities { items, order },
        }
    }
}

fn health(available: bool, stale: bool, error: Option<Failure>) -> Health {
    Health {
        available,
        stale,
        error,
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}
fn publish(hub: &Shared, state: PlayerState) -> Result<()> {
    if !CoreSnapshot::Player(state.clone()).valid() {
        return Err("Invalid player state".into());
    }
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}
/// Nothing is retained while nobody watches: the topic returns to its starting state.
fn publish_idle(hub: &Shared) -> Result<()> {
    publish(hub, Model::default().state(health(false, false, None)))
}
pub fn initial(hub: &Shared) -> Result<()> {
    publish_idle(hub)
}

enum Command {
    Call(
        CoreCall,
        oneshot::Sender<std::result::Result<Value, Failure>>,
    ),
}
#[derive(Clone)]
pub struct Requests(mpsc::Sender<Command>);

pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    let invalid = || failure(ErrorCode::Validation, "Invalid player parameters", false);
    let call: CoreCall =
        serde_json::from_value(json!({"m":method,"p":params})).map_err(|_| invalid())?;
    if !call.valid() {
        return Err(invalid());
    }
    let unavailable = || failure(ErrorCode::Unavailable, "Players are unavailable", true);
    let requests = hub.borrow().player.clone().ok_or_else(unavailable)?;
    let (reply, receive) = oneshot::channel();
    requests
        .0
        .try_send(Command::Call(call, reply))
        .map_err(|_| failure(ErrorCode::Busy, "Player queue is full", true))?;
    receive.await.map_err(|_| unavailable())?
}

async fn properties(
    connection: &zbus::Connection,
    destination: &str,
    interface: &str,
) -> zbus::Result<HashMap<String, OwnedValue>> {
    connection
        .call_method(
            Some(destination),
            PATH,
            Some("org.freedesktop.DBus.Properties"),
            "GetAll",
            &(interface,),
        )
        .await?
        .body()
        .deserialize()
}

async fn read_player(connection: &zbus::Connection, owner: &str) -> zbus::Result<Raw> {
    let identity = match properties(connection, owner, ROOT).await {
        Ok(root) => root
            .get("Identity")
            .and_then(|v| text(v))
            .unwrap_or_default(),
        Err(_) => String::new(),
    };
    let player = properties(connection, owner, PLAYER).await?;
    Ok(Raw::read(identity, &player))
}

/// Art arrives as a local file, a data URI or an https URL; only a re-encoded
/// private PNG path is ever published, and failures leave the art unknown.
fn fetch_art(cache: &ImageCache, url: &str) -> Option<String> {
    if let Some(rest) = url.strip_prefix("data:") {
        use base64::Engine;
        let (head, body) = rest.split_once(',')?;
        if !head.ends_with(";base64") || body.len() as u64 > ART_BYTES * 4 / 3 + 8 {
            return None;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(body)
            .ok()?;
        return cache.resolve_bytes(bytes).ok();
    }
    if url.starts_with("https://") {
        let agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(8)))
            .https_only(true)
            .build()
            .new_agent();
        let mut response = agent
            .get(url)
            .header("User-Agent", "Nacre/1.0")
            .call()
            .ok()?;
        let bytes = response
            .body_mut()
            .with_config()
            .limit(ART_BYTES + 1)
            .read_to_vec()
            .ok()?;
        if bytes.len() as u64 > ART_BYTES {
            return None;
        }
        return cache.resolve_bytes(bytes).ok();
    }
    cache.resolve_path(url).ok()
}

struct Session {
    hub: Shared,
    cache: Arc<ImageCache>,
    connection: zbus::Connection,
    model: Model,
    art_gate: Arc<Semaphore>,
    art_done: mpsc::Sender<(String, String, Option<String>)>,
}

impl Session {
    fn publish(&self) -> Result<()> {
        publish(&self.hub, self.model.state(health(true, false, None)))
    }
    async fn refresh(&mut self, name: &str, owner: String) {
        let Ok(raw) = read_player(&self.connection, &owner).await else {
            // A vanished or misbehaving player is simply not listed.
            self.model.remove(name);
            return;
        };
        if !self.model.upsert(name, owner, raw, now_ms()) {
            return;
        }
        let tracked = self.model.players.get_mut(name).expect("tracked player");
        let Some(url) = tracked
            .raw
            .art_url
            .clone()
            .filter(|_| tracked.art.is_none())
        else {
            return;
        };
        tracked.art = Some((url.clone(), None));
        let (cache, gate, done, name) = (
            self.cache.clone(),
            self.art_gate.clone(),
            self.art_done.clone(),
            name.to_owned(),
        );
        tokio::spawn(async move {
            let Ok(_permit) = gate.acquire_owned().await else {
                return;
            };
            let source = url.clone();
            let path = tokio::task::spawn_blocking(move || fetch_art(&cache, &source))
                .await
                .ok()
                .flatten();
            let _ = done.send((name, url, path)).await;
        });
    }
    async fn retain_art(&self) {
        let keep: Vec<String> = self
            .model
            .players
            .values()
            .filter_map(|t| t.art.as_ref().and_then(|(_, p)| p.clone()))
            .collect();
        let cache = self.cache.clone();
        let _ = tokio::task::spawn_blocking(move || cache.retain(&keep)).await;
    }
    async fn call(&mut self, call: CoreCall) -> std::result::Result<Value, Failure> {
        let stale = || {
            failure(
                ErrorCode::StaleRevision,
                "Player changed; refresh state",
                true,
            )
        };
        let (id, member) = match &call {
            CoreCall::PlayerPlayPause(p) => (&p.player, "PlayPause"),
            CoreCall::PlayerNext(p) => (&p.player, "Next"),
            CoreCall::PlayerPrevious(p) => (&p.player, "Previous"),
            CoreCall::PlayerSeek(p) => (&p.player, "SetPosition"),
            _ => return Err(failure(ErrorCode::UnknownMethod, "Unknown method", false)),
        };
        let tracked = self
            .model
            .players
            .values()
            .find(|t| &t.id == id)
            .ok_or_else(stale)?;
        let r = &tracked.raw;
        let allowed = match &call {
            CoreCall::PlayerPlayPause(_) => r.can_toggle(),
            CoreCall::PlayerNext(_) => r.can_control && r.can_next,
            CoreCall::PlayerPrevious(_) => r.can_control && r.can_previous,
            _ => r.can_seek(),
        };
        if !allowed {
            return Err(failure(
                ErrorCode::PermissionDenied,
                "The player does not allow this action",
                false,
            ));
        }
        let owner = Some(tracked.owner.as_str());
        let result = if let CoreCall::PlayerSeek(p) = &call {
            let micros = i64::try_from(p.position_ms)
                .ok()
                .and_then(|ms| ms.checked_mul(1000))
                .filter(|us| Some(*us) <= r.length_us)
                .ok_or_else(|| {
                    failure(ErrorCode::Validation, "Position is beyond the track", false)
                })?;
            let track = zbus::zvariant::ObjectPath::try_from(r.track.clone().unwrap_or_default())
                .map_err(|_| stale())?;
            self.connection
                .call_method(owner, PATH, Some(PLAYER), member, &(track, micros))
                .await
        } else {
            self.connection
                .call_method(owner, PATH, Some(PLAYER), member, &())
                .await
        };
        result.map_err(|_| {
            failure(
                ErrorCode::Unavailable,
                "The player did not accept the request",
                true,
            )
            .dispatch_state(true)
        })?;
        Ok(json!({"changed":true}))
    }
}

/// One subscription lifetime: connect, discover, follow signals until interest ends.
async fn session(
    hub: &Shared,
    cache: &Arc<ImageCache>,
    interest: &mut tokio::sync::watch::Receiver<bool>,
    commands: &mut mpsc::Receiver<Command>,
) -> Result<()> {
    let connection = zbus::connection::Builder::session()?
        .method_timeout(Duration::from_secs(2))
        .build()
        .await?;
    let dbus = zbus::fdo::DBusProxy::new(&connection).await?;
    let mut owners = dbus.receive_name_owner_changed().await?;
    let rule = zbus::MatchRule::builder()
        .msg_type(zbus::message::Type::Signal)
        .path(PATH)?
        .build();
    let mut signals = zbus::MessageStream::for_match_rule(rule, &connection, Some(64)).await?;
    let (art_done, mut art_results) = mpsc::channel(MAX_PLAYERS * 2);
    let mut s = Session {
        hub: hub.clone(),
        cache: cache.clone(),
        connection: connection.clone(),
        model: Model::default(),
        art_gate: Arc::new(Semaphore::new(1)),
        art_done,
    };
    for name in dbus.list_names().await? {
        if name.as_str().starts_with(PREFIX) {
            if let Ok(owner) = dbus
                .get_name_owner(zbus::names::BusName::try_from(name.as_str())?)
                .await
            {
                s.refresh(name.as_str(), owner.to_string()).await;
            }
        }
    }
    s.publish()?;
    let mut dirty: Vec<String> = Vec::new();
    let mut deadline: Option<Instant> = None;
    loop {
        let wait = deadline.map_or(Duration::ZERO, |at| {
            at.saturating_duration_since(Instant::now())
        });
        tokio::select! {
            changed = interest.changed() => {
                changed?;
                if !*interest.borrow() { return Ok(()); }
            }
            Some(Command::Call(call, reply)) = commands.recv() => {
                let _ = reply.send(s.call(call).await);
            }
            Some(change) = owners.next() => {
                let args = change.args()?;
                let name = args.name().as_str().to_owned();
                if name.starts_with(PREFIX) {
                    match args.new_owner().as_ref() {
                        Some(owner) => s.refresh(&name, owner.to_string()).await,
                        None => s.model.remove(&name),
                    }
                    s.retain_art().await;
                    s.publish()?;
                }
            }
            Some(message) = signals.next() => {
                let Ok(message) = message else { continue };
                let Some(sender) = message.header().sender().map(|v| v.to_string()) else { continue };
                if let Some(name) = s.model.players.iter().find(|(_, t)| t.owner == sender).map(|(n, _)| n.clone()) {
                    if !dirty.contains(&name) { dirty.push(name); }
                    deadline.get_or_insert_with(|| Instant::now() + COALESCE);
                }
            }
            Some((name, url, path)) = art_results.recv() => {
                if let Some(t) = s.model.players.get_mut(&name) {
                    if t.art.as_ref().is_some_and(|(current, _)| *current == url) {
                        t.art = Some((url, path));
                        s.retain_art().await;
                        s.publish()?;
                    }
                }
            }
            _ = tokio::time::sleep(wait), if deadline.is_some() => {
                deadline = None;
                for name in std::mem::take(&mut dirty) {
                    if let Some(owner) = s.model.players.get(&name).map(|t| t.owner.clone()) {
                        s.refresh(&name, owner).await;
                    }
                }
                s.retain_art().await;
                s.publish()?;
            }
        }
    }
}

pub async fn run(hub: Shared, paths: Paths) -> Result<()> {
    let cache = Arc::new(ImageCache::open(paths.state.join("shell/player-art"))?);
    let (requests, mut commands) = mpsc::channel(16);
    hub.borrow_mut().player = Some(Requests(requests));
    let mut interest = hub.borrow_mut().watch_interest(TOPIC);
    let clear = |cache: Arc<ImageCache>| async move {
        let _ = tokio::task::spawn_blocking(move || cache.retain(&[])).await;
    };
    clear(cache.clone()).await;
    hub.borrow_mut().service("dashboard-player", "running", 0)?;
    loop {
        publish_idle(&hub)?;
        while !*interest.borrow() {
            tokio::select! {
                changed = interest.changed() => changed?,
                Some(Command::Call(_, reply)) = commands.recv() => {
                    let _ = reply.send(Err(failure(ErrorCode::Unavailable, "Players are unavailable", true)));
                }
            }
        }
        while *interest.borrow() {
            if session(&hub, &cache, &mut interest, &mut commands)
                .await
                .is_err()
                && *interest.borrow()
            {
                // Keep a visible failure and retry quietly; never spin.
                let state = hub
                    .borrow()
                    .snapshot(TOPIC)
                    .and_then(|v| serde_json::from_value::<PlayerState>(v).ok())
                    .map(|mut s| {
                        s.health = health(
                            false,
                            !s.players.order.is_empty(),
                            Some(failure(
                                ErrorCode::Unavailable,
                                "Media players are unavailable",
                                true,
                            )),
                        );
                        s
                    });
                if let Some(state) = state {
                    publish(&hub, state)?;
                }
                tokio::select! {
                    _ = tokio::time::sleep(RETRY) => {}
                    changed = interest.changed() => changed?,
                }
            }
        }
        clear(cache.clone()).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::hub::Hub;
    use std::sync::Mutex;
    use zbus::zvariant::{ObjectPath, Value};

    fn props(status: &str, extra: &[(&str, Value<'_>)]) -> HashMap<String, OwnedValue> {
        let mut map: HashMap<String, OwnedValue> = HashMap::new();
        let mut put = |name: &str, value: Value<'_>| {
            map.insert(
                name.into(),
                OwnedValue::try_from(value).expect("owned value"),
            );
        };
        put("PlaybackStatus", Value::from(status));
        for flag in ["CanControl", "CanPlay", "CanPause", "CanGoNext", "CanSeek"] {
            put(flag, Value::from(true));
        }
        put("CanGoPrevious", Value::from(false));
        for (name, value) in extra {
            put(name, value.try_clone().expect("plain value"));
        }
        map
    }
    fn metadata(entries: Vec<(&str, Value<'_>)>) -> Value<'static> {
        let mut dict = zbus::zvariant::Dict::new(
            &zbus::zvariant::Signature::Str,
            &zbus::zvariant::Signature::Variant,
        );
        for (key, value) in entries {
            dict.append(
                Value::from(key).try_to_owned().unwrap().into(),
                Value::Value(Box::new(value)).try_to_owned().unwrap().into(),
            )
            .unwrap();
        }
        Value::Dict(dict).try_to_owned().unwrap().into()
    }

    #[test]
    fn properties_become_bounded_display_values_and_unknowns_stay_omitted() {
        let long = "x".repeat(900);
        let meta = metadata(vec![
            ("xesam:title", Value::from(long.as_str())),
            ("xesam:artist", Value::from(vec!["A", "B"])),
            ("mpris:length", Value::from(200_000_000i64)),
            (
                "mpris:trackid",
                Value::ObjectPath(ObjectPath::try_from("/track/1").unwrap()),
            ),
            ("mpris:artUrl", Value::from("https://example.invalid/a.png")),
        ]);
        let raw = Raw::read(
            "Fixture".into(),
            &props(
                "Playing",
                &[
                    ("Metadata", meta),
                    ("Position", Value::from(250_000_000i64)),
                ],
            ),
        );
        let mut model = Model::default();
        assert!(model.upsert("org.mpris.MediaPlayer2.fixture", ":1.9".into(), raw, 1234));
        let state = model.state(health(true, false, None));
        assert!(CoreSnapshot::Player(state.clone()).valid());
        let player = &state.players.items[&state.players.order[0]];
        assert_eq!(player.title.len(), 512);
        assert_eq!(player.artist, "A, B");
        assert_eq!(player.length_ms, Some(200_000));
        assert_eq!(
            player.position_ms,
            Some(200_000),
            "position never exceeds the length"
        );
        assert_eq!(player.position_updated_ms, Some(1234));
        assert!(
            player.can_seek && player.can_play_pause && player.can_next && !player.can_previous
        );
        assert_eq!(
            player.art_path, None,
            "no remote URL ever crosses the protocol"
        );
        let none = Raw::read(
            "Quiet".into(),
            &props("Stopped", &[("Position", Value::from(-1i64))]),
        );
        assert_eq!((none.position_us, none.length_us), (None, None));
        let mut quiet = Model::default();
        quiet.upsert("org.mpris.MediaPlayer2.quiet", ":1.2".into(), none, 9);
        let state = quiet.state(health(true, false, None));
        let player = &state.players.items[&state.players.order[0]];
        assert!(player.position_ms.is_none() && player.length_ms.is_none() && !player.can_seek);
    }

    #[test]
    fn active_player_prefers_the_latest_playing_one_and_ids_are_never_reused() {
        let mut model = Model::default();
        let raw = |status: &str| Raw::read("P".into(), &props(status, &[]));
        model.upsert("a", ":1.1".into(), raw("Paused"), 1);
        model.upsert("b", ":1.2".into(), raw("Paused"), 1);
        assert_eq!(model.active().unwrap().id, EntityId("player-1".into()));
        model.upsert("b", ":1.2".into(), raw("Playing"), 2);
        model.upsert("a", ":1.1".into(), raw("Playing"), 3);
        assert_eq!(
            model.active().unwrap().id,
            EntityId("player-1".into()),
            "latest to start playing wins"
        );
        model.upsert("a", ":1.1".into(), raw("Paused"), 4);
        assert_eq!(model.active().unwrap().id, EntityId("player-2".into()));
        model.remove("b");
        model.upsert("c", ":1.3".into(), raw("Paused"), 5);
        assert!(model.players["c"].id == EntityId("player-3".into()));
        for index in 0..MAX_PLAYERS * 2 {
            model.upsert(&format!("n{index}"), ":1.4".into(), raw("Paused"), 6);
        }
        assert_eq!(model.players.len(), MAX_PLAYERS);
    }

    struct FakeState {
        status: String,
        title: String,
        art: String,
        position: i64,
        can_next: bool,
        calls: Vec<String>,
    }
    struct FakeRoot(String);
    #[zbus::interface(name = "org.mpris.MediaPlayer2")]
    impl FakeRoot {
        #[zbus(property)]
        fn identity(&self) -> String {
            self.0.clone()
        }
    }
    struct FakePlayer(Arc<Mutex<FakeState>>);
    #[zbus::interface(name = "org.mpris.MediaPlayer2.Player")]
    impl FakePlayer {
        fn play_pause(&self) {
            self.0.lock().unwrap().calls.push("PlayPause".into());
        }
        fn next(&self) {
            self.0.lock().unwrap().calls.push("Next".into());
        }
        fn previous(&self) {
            self.0.lock().unwrap().calls.push("Previous".into());
        }
        fn set_position(&self, track: ObjectPath<'_>, position: i64) {
            self.0
                .lock()
                .unwrap()
                .calls
                .push(format!("SetPosition {track} {position}"));
        }
        #[zbus(property)]
        fn playback_status(&self) -> String {
            self.0.lock().unwrap().status.clone()
        }
        #[zbus(property)]
        fn metadata(&self) -> HashMap<String, OwnedValue> {
            let state = self.0.lock().unwrap();
            let mut map = HashMap::new();
            let mut put = |k: &str, v: Value<'_>| {
                map.insert(k.to_owned(), OwnedValue::try_from(v).unwrap());
            };
            put("xesam:title", Value::from(state.title.as_str()));
            put("xesam:artist", Value::from(vec!["Fixture artist"]));
            put("mpris:length", Value::from(100_000_000i64));
            put(
                "mpris:trackid",
                Value::ObjectPath(ObjectPath::try_from("/fixture/track").unwrap()),
            );
            if !state.art.is_empty() {
                put("mpris:artUrl", Value::from(state.art.as_str()));
            }
            map
        }
        #[zbus(property)]
        fn position(&self) -> i64 {
            self.0.lock().unwrap().position
        }
        #[zbus(property)]
        fn can_control(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_play(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_pause(&self) -> bool {
            true
        }
        #[zbus(property)]
        fn can_go_next(&self) -> bool {
            self.0.lock().unwrap().can_next
        }
        #[zbus(property)]
        fn can_go_previous(&self) -> bool {
            false
        }
        #[zbus(property)]
        fn can_seek(&self) -> bool {
            true
        }
    }
    async fn fake(name: &str, state: Arc<Mutex<FakeState>>) -> zbus::Result<zbus::Connection> {
        zbus::connection::Builder::session()?
            .name(format!("{PREFIX}{name}"))?
            .serve_at(PATH, FakeRoot(format!("Fake {name}")))?
            .serve_at(PATH, FakePlayer(state))?
            .build()
            .await
    }
    fn fake_state(status: &str, title: &str, art: &str, can_next: bool) -> Arc<Mutex<FakeState>> {
        Arc::new(Mutex::new(FakeState {
            status: status.into(),
            title: title.into(),
            art: art.into(),
            position: 30_000_000,
            can_next,
            calls: Vec::new(),
        }))
    }
    async fn wait_for(
        hub: &Shared,
        what: &str,
        check: impl Fn(&PlayerState) -> bool,
    ) -> PlayerState {
        for _ in 0..250 {
            let state = hub
                .borrow()
                .snapshot(TOPIC)
                .and_then(|v| serde_json::from_value::<PlayerState>(v).ok());
            if let Some(state) = state.filter(|s| check(s)) {
                return state;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("Timed out waiting for {what}");
    }

    #[test]
    fn fake_players_run_only_inside_a_private_bus() {
        let executable = std::env::current_exe().expect("test executable path");
        let status = std::process::Command::new("dbus-run-session")
            .arg("--")
            .arg(executable)
            .args([
                "--exact",
                "daemon::dashboard_player::tests::private_bus_contract",
                "--nocapture",
            ])
            .env("NACRE_PRIVATE_PLAYER_BUS", "1")
            .status()
            .expect("dbus-run-session is required for the private player bus fixture");
        assert!(status.success(), "private player bus fixture failed");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn private_bus_contract() -> Result<()> {
        if std::env::var("NACRE_PRIVATE_PLAYER_BUS").as_deref() != Ok("1") {
            return Ok(());
        }
        let home = tempfile::tempdir()?;
        std::fs::set_permissions(
            home.path(),
            std::os::unix::fs::PermissionsExt::from_mode(0o700),
        )?;
        let art = home.path().join("cover.png");
        image::RgbaImage::new(4, 4).save(&art)?;
        let paths = Paths::new(home.path());
        let local = tokio::task::LocalSet::new();
        local
            .run_until(async {
                let hub = Hub::new("boot".into(), false, Duration::from_millis(16)).shared();
                initial(&hub)?;
                let idle = hub.borrow().snapshot(TOPIC).unwrap();
                assert_eq!(
                    idle["health"]["available"], false,
                    "starting state before any client"
                );
                let mut service = tokio::task::spawn_local(run(hub.clone(), paths.clone()));
                let (client, _wake) = hub.borrow_mut().add_client()?;
                let quiet = fake_state("Paused", "Quiet", "", false);
                let loud = fake_state(
                    "Playing",
                    "Loud",
                    &format!("file://{}", art.display()),
                    true,
                );
                let _quiet = fake("quiet", quiet.clone()).await?;
                let loud_bus = fake("loud", loud.clone()).await?;
                // No client yet: nothing connected, nothing published but the starting state.
                tokio::time::sleep(Duration::from_millis(200)).await;
                assert_eq!(hub.borrow().snapshot(TOPIC).unwrap(), idle);
                if service.is_finished() {
                    (&mut service).await??;
                }
                assert!(hub.borrow().methods().contains_key("player.seek"));
                hub.borrow_mut().subscribe(client, &[TOPIC.into()], false)?;
                let state = wait_for(&hub, "both players with art", |s| {
                    s.players.order.len() == 2
                        && s.players.items.values().any(|p| p.art_path.is_some())
                })
                .await;
                assert!(CoreSnapshot::Player(state.clone()).valid());
                let active = state.active.clone().unwrap();
                let current = &state.players.items[&active];
                assert_eq!(
                    (current.title.as_str(), current.status),
                    ("Loud", PlayerStatus::Playing)
                );
                assert_eq!(current.identity, "Fake loud");
                assert_eq!(current.length_ms, Some(100_000));
                let cached = current.art_path.clone().unwrap();
                assert!(cached.starts_with(paths.state.join("shell/player-art").to_str().unwrap()));
                assert_ne!(
                    cached,
                    art.to_str().unwrap(),
                    "published art is a private copy"
                );
                let quiet_id = state
                    .players
                    .items
                    .iter()
                    .find(|(_, p)| p.title == "Quiet")
                    .map(|(id, _)| id.clone())
                    .unwrap();

                // Controls reach the right player and honor the advertised abilities.
                let call = |method: &'static str, params: serde_json::Value| {
                    dispatch(hub.clone(), method, params)
                };
                call("player.play_pause", json!({"player":active.0})).await?;
                call("player.next", json!({"player":active.0})).await?;
                call(
                    "player.seek",
                    json!({"player":active.0,"position_ms":45000}),
                )
                .await?;
                assert_eq!(
                    loud.lock().unwrap().calls,
                    ["PlayPause", "Next", "SetPosition /fixture/track 45000000"]
                );
                let denied = call("player.next", json!({"player":quiet_id.0}))
                    .await
                    .unwrap_err();
                assert_eq!(denied.code, ErrorCode::PermissionDenied);
                let beyond = call(
                    "player.seek",
                    json!({"player":active.0,"position_ms":100001}),
                )
                .await
                .unwrap_err();
                assert_eq!(beyond.code, ErrorCode::Validation);
                let stale = call("player.next", json!({"player":"player-99"}))
                    .await
                    .unwrap_err();
                assert_eq!(stale.code, ErrorCode::StaleRevision);
                let invalid = call("player.seek", json!({"player":"../x","position_ms":1}))
                    .await
                    .unwrap_err();
                assert_eq!(invalid.code, ErrorCode::Validation);
                assert!(quiet.lock().unwrap().calls.is_empty());

                // Property signals update the topic without any polling.
                {
                    let mut state = loud.lock().unwrap();
                    state.status = "Paused".into();
                    state.title = "Changed".into();
                    state.art.clear();
                }
                {
                    let interface = loud_bus
                        .object_server()
                        .interface::<_, FakePlayer>(PATH)
                        .await?;
                    let emitter = interface.signal_emitter();
                    interface
                        .get()
                        .await
                        .playback_status_changed(emitter)
                        .await?;
                    interface.get().await.metadata_changed(emitter).await?;
                }
                let state = wait_for(&hub, "paused change", |s| {
                    s.players
                        .items
                        .values()
                        .any(|p| p.title == "Changed" && p.status == PlayerStatus::Paused)
                })
                .await;
                assert!(state.players.items.values().all(|p| p.art_path.is_none()));
                assert!(
                    std::fs::read_dir(paths.state.join("shell/player-art"))?
                        .filter_map(|e| e.ok())
                        .all(|e| e.file_name().to_string_lossy().starts_with('.')),
                    "unreferenced art is removed from the cache"
                );

                // A vanishing player leaves the list; its ID is not reused.
                drop(loud_bus);
                let state = wait_for(&hub, "player removal", |s| s.players.order.len() == 1).await;
                assert_eq!(state.active, Some(quiet_id.clone()));

                // Last unsubscribe returns the topic to its starting state.
                hub.borrow_mut().unsubscribe(client, &[TOPIC.into()]);
                let state = wait_for(&hub, "idle reset", |s| {
                    !s.health.available && s.players.order.is_empty()
                })
                .await;
                assert!(state.active.is_none() && !state.health.stale);
                let unavailable = call("player.next", json!({"player":quiet_id.0}))
                    .await
                    .unwrap_err();
                assert_eq!(unavailable.code, ErrorCode::Unavailable);
                service.abort();
                Ok::<(), crate::Error>(())
            })
            .await
    }
}
