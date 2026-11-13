//! N0314: the daemon's one PipeWire client. A thread runs the PipeWire main loop with the
//! registry (audio nodes, their devices, the `default` metadata) and the two capture streams;
//! it sends whole `audio` states, key-step outcomes and PCM windows to [`run`], which
//! publishes the `audio`, `audio.peak` and `audio.spectrum` topics and the audio OSD events.
//! Actions arrive as commands on the loop's channel. No child process and no polling: the
//! server pushes every change, and nothing is captured while no client subscribes.
use super::{
    audio_peak, audio_spectrum,
    hub::{failure, Shared},
    osd,
    pill_controls::{self, AudioStepOutcome},
};
use crate::Result;
use nacre_protocol::{
    core_services::{
        AudioKind, AudioNode, AudioReadback, AudioState, CoreSnapshot, Entities, EntityId, Health,
    },
    osd::{OsdEvent, OsdKind},
    ErrorCode, Failure,
};
use pipewire as pw;
use pw::{
    core::CoreRc,
    device::{Device, DeviceListener},
    metadata::{Metadata, MetadataListener},
    node::{Node, NodeListener},
    spa::{
        self,
        param::ParamType,
        pod::{Object, Pod, Property, Value, ValueArray},
        utils::SpaTypes,
    },
    stream::{StreamListener, StreamRc},
    types::ObjectType,
};
use std::{
    cell::RefCell,
    collections::{BTreeMap, HashMap},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    sync::{mpsc, oneshot},
    time::Instant,
};

/// The capture streams' own client names; they are not applications in the `audio` topic.
pub(super) const PEAK_CLIENT: &str = "nacre-peak";
pub(super) const SPECTRUM_CLIENT: &str = "nacre-spectrum";
/// Bursts of server events (a connect, a profile switch) become one `audio` publish.
const DEBOUNCE: Duration = Duration::from_millis(25);
/// The most application streams one kind publishes.
const MAX_STREAMS: usize = 64;
const TOPIC: &str = "audio";

pub(crate) enum Target {
    DefaultSink,
    DefaultSource,
    /// A device or stream by `node.name`.
    Node(String),
    /// `sink-input:<id>` (playback) or `source-output:<id>` (capture).
    Stream {
        playback: bool,
        id: u32,
    },
}
pub(crate) enum Action {
    /// Linear 0..1 on every channel.
    Volume(f64),
    Mute(bool),
    ToggleMute,
}
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capture {
    Peak,
    Spectrum,
}
pub(crate) type Reply = oneshot::Sender<std::result::Result<(), Failure>>;
pub(crate) enum Command {
    Apply {
        target: Target,
        action: Action,
        reply: Reply,
    },
    SetDefault {
        name: String,
        reply: Reply,
    },
    /// A volume key step on the default sink: `delta` on the perceptual scale, whole percent.
    Step {
        delta: f64,
        key_ms: Option<u64>,
        reply: Option<Reply>,
    },
    Capture {
        kind: Capture,
        on: bool,
    },
    Stop,
}
pub(crate) enum Event {
    State(AudioState),
    Step {
        result: std::result::Result<AudioStepOutcome, Failure>,
        key_ms: Option<u64>,
    },
    Peak(f64),
    /// One analysis hop of 16-bit mono PCM (`audio_spectrum::HOP` samples).
    Spectrum(Vec<u8>),
}

fn unavailable(message: &str) -> Failure {
    failure(ErrorCode::Unavailable, message, false).dispatch_state(false)
}
fn action_error() -> Failure {
    failure(
        ErrorCode::Io,
        "Audio action failed; it may already have completed",
        false,
    )
    .dispatch_state(true)
}

// ------------------------------------------------------------------ handle

/// The async side's view of the client: `Send`, cheap to clone, stored in the hub.
#[derive(Clone)]
pub struct Handle {
    send: Arc<dyn Fn(Command) -> bool + Send + Sync>,
    connected: Arc<AtomicBool>,
}
impl Handle {
    pub fn connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }
    pub(crate) fn send(&self, command: Command) -> bool {
        (self.send)(command)
    }
    async fn ask(&self, make: impl FnOnce(Reply) -> Command) -> std::result::Result<(), Failure> {
        let (reply, done) = oneshot::channel();
        if !self.connected() || !self.send(make(reply)) {
            return Err(unavailable("Audio is unavailable"));
        }
        match tokio::time::timeout(Duration::from_secs(2), done).await {
            Ok(Ok(result)) => result,
            _ => Err(unavailable("Audio is unavailable")),
        }
    }
    pub(crate) async fn apply(
        &self,
        target: Target,
        action: Action,
    ) -> std::result::Result<(), Failure> {
        self.ask(|reply| Command::Apply {
            target,
            action,
            reply,
        })
        .await
    }
    pub(crate) async fn set_default(&self, name: String) -> std::result::Result<(), Failure> {
        self.ask(|reply| Command::SetDefault { name, reply }).await
    }
    pub(crate) async fn step(
        &self,
        delta: f64,
        key_ms: Option<u64>,
    ) -> std::result::Result<(), Failure> {
        self.ask(|reply| Command::Step {
            delta,
            key_ms,
            reply: Some(reply),
        })
        .await
    }
}

#[cfg(test)]
impl Handle {
    /// Records every command as text and answers with `answer`; always connected.
    pub(crate) fn fake(
        answer: impl Fn(&Command) -> std::result::Result<(), Failure> + Send + Sync + 'static,
    ) -> (Self, Arc<std::sync::Mutex<Vec<String>>>) {
        let log = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = log.clone();
        let send = move |command: Command| {
            seen.lock().unwrap().push(describe(&command));
            let result = answer(&command);
            match command {
                Command::Apply { reply, .. } | Command::SetDefault { reply, .. } => {
                    let _ = reply.send(result);
                }
                Command::Step {
                    reply: Some(reply), ..
                } => {
                    let _ = reply.send(result);
                }
                _ => {}
            }
            true
        };
        let handle = Self {
            send: Arc::new(send),
            connected: Arc::new(AtomicBool::new(true)),
        };
        (handle, log)
    }
}
#[cfg(test)]
pub(crate) fn describe(command: &Command) -> String {
    let target = |target: &Target| match target {
        Target::DefaultSink => "sink".to_owned(),
        Target::DefaultSource => "source".to_owned(),
        Target::Node(name) => name.clone(),
        Target::Stream { playback, id } => {
            format!(
                "{}:{id}",
                if *playback {
                    "sink-input"
                } else {
                    "source-output"
                }
            )
        }
    };
    match command {
        Command::Apply {
            target: t,
            action: Action::Volume(value),
            ..
        } => format!("volume {} {value}", target(t)),
        Command::Apply {
            target: t,
            action: Action::Mute(muted),
            ..
        } => format!("mute {} {muted}", target(t)),
        Command::Apply {
            target: t,
            action: Action::ToggleMute,
            ..
        } => format!("toggle-mute {}", target(t)),
        Command::SetDefault { name, .. } => format!("default {name}"),
        Command::Step { delta, .. } => format!("step {delta}"),
        Command::Capture { kind, on } => format!(
            "capture {} {}",
            if *kind == Capture::Peak {
                "peak"
            } else {
                "spectrum"
            },
            if *on { "on" } else { "off" }
        ),
        Command::Stop => "stop".into(),
    }
}

/// Owns the client thread; dropping it ends the loop.
pub struct Client {
    handle: Handle,
    stop: Arc<AtomicBool>,
}
impl Drop for Client {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        self.handle.send(Command::Stop);
    }
}

pub(crate) fn spawn() -> Result<(Handle, mpsc::UnboundedReceiver<Event>, Client)> {
    let (events, receiver) = mpsc::unbounded_channel();
    let (sender, commands) = pw::channel::channel::<Command>();
    let connected = Arc::new(AtomicBool::new(false));
    let stop = Arc::new(AtomicBool::new(false));
    let handle = Handle {
        send: Arc::new(move |command| sender.send(command).is_ok()),
        connected: connected.clone(),
    };
    let thread_stop = stop.clone();
    std::thread::Builder::new()
        .name("nacre-audio".into())
        .stack_size(512 * 1024)
        .spawn(move || serve(commands, events, connected, thread_stop))?;
    Ok((handle.clone(), receiver, Client { handle, stop }))
}

// ------------------------------------------------------------------- graph

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Class {
    Sink,
    Source,
    Playback,
    Capture,
}
impl Class {
    fn parse(media_class: &str) -> Option<Self> {
        Some(match media_class {
            "Audio/Sink" => Self::Sink,
            "Audio/Source" => Self::Source,
            "Stream/Output/Audio" => Self::Playback,
            "Stream/Input/Audio" => Self::Capture,
            _ => return None,
        })
    }
}
/// One audio node as the registry and its `Props` param describe it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct Entry {
    pub(super) name: String,
    pub(super) description: String,
    pub(super) class: Option<Class>,
    pub(super) app: Option<String>,
    pub(super) media: Option<String>,
    pub(super) icon: Option<String>,
    /// `device.id` and `card.profile.device`: the route that owns a hardware volume.
    pub(super) device: Option<(u32, i32)>,
    /// Linear channel volumes; empty until the first `Props` arrives.
    pub(super) channels: Vec<f32>,
    pub(super) muted: Option<bool>,
}
#[derive(Default)]
pub(super) struct Graph {
    pub(super) nodes: BTreeMap<u32, Entry>,
    /// Device id -> `card.profile.device` -> route index, for routes that carry a volume.
    pub(super) devices: BTreeMap<u32, BTreeMap<i32, i32>>,
    pub(super) default_sink: Option<String>,
    pub(super) default_source: Option<String>,
}

fn round(value: f64, digits: i32) -> f64 {
    let scale = 10f64.powi(digits);
    (value * scale).round() / scale
}

impl Entry {
    fn own_stream(&self) -> bool {
        matches!(
            self.app.as_deref(),
            Some(PEAK_CLIENT) | Some(SPECTRUM_CLIENT)
        ) || matches!(self.name.as_str(), PEAK_CLIENT | SPECTRUM_CLIENT)
    }
    fn node(&self, kind: AudioKind, name: &str, icon: Option<&str>) -> AudioNode {
        let linear = round(
            self.channels
                .iter()
                .fold(0.0_f32, |top, v| top.max(*v))
                .into(),
            6,
        );
        let muted = self.muted.unwrap_or(false);
        AudioNode {
            name: name.chars().take(200).collect(),
            kind,
            volume: linear,
            muted,
            ready: true,
            readback: Some(AudioReadback {
                linear_volume: Some(linear),
                perceptual_volume: Some(round(linear.cbrt(), 4)),
                muted: Some(muted),
            }),
            icon: icon.map(|icon| icon.chars().take(128).collect()),
            target: None,
        }
    }
}

pub(super) fn unavailable_audio() -> AudioState {
    AudioState {
        health: Health {
            available: false,
            stale: false,
            error: None,
        },
        nodes: Entities {
            items: Default::default(),
            order: Vec::new(),
        },
        default_output: None,
        default_input: None,
    }
}

impl Graph {
    /// The id of `name` among `classes`.
    fn find(&self, name: &str, classes: &[Class]) -> Option<u32> {
        self.nodes
            .iter()
            .find(|(_, e)| e.name == name && e.class.is_some_and(|c| classes.contains(&c)))
            .map(|(id, _)| *id)
    }
    /// The `audio` topic: devices by `node.name`, then streams as `sink-input:<id>` and
    /// `source-output:<id>` (the ids pipewire-pulse reported), in registry order.
    pub(super) fn state(&self) -> AudioState {
        let mut state = unavailable_audio();
        let mut push = |id: EntityId, node: AudioNode| {
            if state.nodes.items.len() < 2048 && id.valid() && !state.nodes.items.contains_key(&id)
            {
                state.nodes.items.insert(id.clone(), node);
                state.nodes.order.push(id);
            }
        };
        for (class, kind) in [
            (Class::Sink, AudioKind::Output),
            (Class::Source, AudioKind::Input),
        ] {
            for entry in self.nodes.values() {
                if entry.class == Some(class) && !entry.channels.is_empty() {
                    let label = if entry.description.is_empty() {
                        &entry.name
                    } else {
                        &entry.description
                    };
                    push(
                        EntityId(entry.name.clone()),
                        entry.node(kind.clone(), label, None),
                    );
                }
            }
        }
        for (class, kind, prefix) in [
            (Class::Playback, AudioKind::Playback, "sink-input"),
            (Class::Capture, AudioKind::Capture, "source-output"),
        ] {
            for (id, entry) in self
                .nodes
                .iter()
                .filter(|(_, e)| {
                    e.class == Some(class) && !e.channels.is_empty() && !e.own_stream()
                })
                .take(MAX_STREAMS)
            {
                let name = entry
                    .app
                    .as_deref()
                    .or(entry.media.as_deref())
                    .unwrap_or_default();
                push(
                    EntityId(format!("{prefix}:{id}")),
                    entry.node(kind.clone(), name, entry.icon.as_deref()),
                );
            }
        }
        let listed = |name: &Option<String>, kind: AudioKind| {
            let id = EntityId(name.clone()?);
            (state.nodes.items.get(&id)?.kind == kind).then_some(id)
        };
        state.default_output = listed(&self.default_sink, AudioKind::Output);
        state.default_input = listed(&self.default_source, AudioKind::Input);
        state.health.available = !state.nodes.items.is_empty();
        state
    }
    fn resolve(&self, target: &Target) -> std::result::Result<u32, Failure> {
        let found = match target {
            Target::DefaultSink => self
                .default_sink
                .as_deref()
                .and_then(|name| self.find(name, &[Class::Sink])),
            Target::DefaultSource => self
                .default_source
                .as_deref()
                .and_then(|name| self.find(name, &[Class::Source])),
            Target::Node(name) => self.find(
                name,
                &[Class::Sink, Class::Source, Class::Playback, Class::Capture],
            ),
            Target::Stream { playback, id } => {
                let class = if *playback {
                    Class::Playback
                } else {
                    Class::Capture
                };
                self.nodes
                    .get(id)
                    .filter(|e| e.class == Some(class))
                    .map(|_| *id)
            }
        };
        found.ok_or_else(|| {
            unavailable(if matches!(target, Target::Stream { .. }) {
                "Audio stream is unavailable"
            } else {
                "Audio device is unavailable"
            })
        })
    }
}

/// Steps every channel by `delta` on the perceptual (cubic) scale `wpctl set-volume 5%+`
/// uses, snapped to whole percent and capped at 100 %; returns linear channel volumes.
pub(super) fn stepped_channel_volumes(current: &[f64], delta: f64) -> Option<Vec<f64>> {
    if current.is_empty()
        || current.len() > 64
        || !delta.is_finite()
        || current
            .iter()
            .any(|volume| !volume.is_finite() || !(0.0..=4.0).contains(volume))
    {
        return None;
    }
    Some(
        current
            .iter()
            .map(|volume| {
                let target = ((volume.cbrt() + delta).clamp(0.0, 1.0) * 100.0).round() / 100.0;
                (target.powi(3) * 1e6).round() / 1e6
            })
            .collect(),
    )
}

// ------------------------------------------------------------ client thread

/// Everything that lives for one connection: the graph plus the proxies that feed it.
struct Live {
    core: CoreRc,
    graph: Graph,
    nodes: HashMap<u32, (Node, NodeListener)>,
    devices: HashMap<u32, (Device, DeviceListener)>,
    metadata: Option<(u32, Metadata, MetadataListener)>,
    streams: [Option<(StreamRc, StreamListener<Vec<u8>>)>; 2],
}
type Events = mpsc::UnboundedSender<Event>;

fn serve(
    mut commands: pw::channel::Receiver<Command>,
    events: Events,
    connected: Arc<AtomicBool>,
    stop: Arc<AtomicBool>,
) {
    pw::init();
    let wanted = Rc::new(RefCell::new([false; 2]));
    let mut retry = 0u32;
    while !stop.load(Ordering::Relaxed) {
        let started = std::time::Instant::now();
        let (receiver, result) = connect(commands, &events, &connected, &wanted);
        commands = receiver;
        connected.store(false, Ordering::Relaxed);
        if let Err(error) = result {
            tracing::debug!(%error, "PipeWire is unavailable");
        }
        if events.send(Event::State(unavailable_audio())).is_err() || stop.load(Ordering::Relaxed) {
            break;
        }
        retry = if started.elapsed() > Duration::from_secs(10) {
            0
        } else {
            (retry + 1).min(4)
        };
        std::thread::sleep(Duration::from_millis(500 << retry));
    }
}

fn publish(live: &RefCell<Live>, events: &Events) {
    let _ = events.send(Event::State(live.borrow().graph.state()));
}

fn deserialize(pod: &Pod) -> Option<Object> {
    match spa::pod::deserialize::PodDeserializer::deserialize_from::<Value>(pod.as_bytes()) {
        Ok((_, Value::Object(object))) => Some(object),
        _ => None,
    }
}
/// `(channelVolumes, mute)` of a `Props` param.
fn volume_props(object: Object) -> (Option<Vec<f32>>, Option<bool>) {
    let (mut channels, mut muted) = (None, None);
    for property in object.properties {
        match (property.key, property.value) {
            (spa::sys::SPA_PROP_channelVolumes, Value::ValueArray(ValueArray::Float(v))) => {
                channels = Some(v);
            }
            (spa::sys::SPA_PROP_mute, Value::Bool(b)) => muted = Some(b),
            _ => {}
        }
    }
    (channels, muted)
}
/// `(card.profile.device, index, carries a volume)` of a `Route` param.
fn route_props(object: Object) -> Option<(i32, i32, bool)> {
    let (mut device, mut index, mut volume) = (None, None, false);
    for property in object.properties {
        match (property.key, property.value) {
            (spa::sys::SPA_PARAM_ROUTE_device, Value::Int(v)) => device = Some(v),
            (spa::sys::SPA_PARAM_ROUTE_index, Value::Int(v)) => index = Some(v),
            (spa::sys::SPA_PARAM_ROUTE_props, Value::Object(props)) => {
                volume = volume_props(props).0.is_some();
            }
            _ => {}
        }
    }
    Some((device?, index?, volume))
}
fn pod_bytes(object: Object) -> std::result::Result<Vec<u8>, Failure> {
    spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &Value::Object(object),
    )
    .map(|(cursor, _)| cursor.into_inner())
    .map_err(|_| action_error())
}
fn props_object(properties: Vec<Property>) -> Object {
    Object {
        type_: SpaTypes::ObjectParamProps.as_raw(),
        id: ParamType::Props.as_raw(),
        properties,
    }
}

/// Write channel volumes and/or mute to one node: through its device route when the active
/// profile has one for it (hardware volume, the way `wpctl` writes), else its own `Props`.
fn write(
    live: &RefCell<Live>,
    id: u32,
    channels: Option<Vec<f32>>,
    muted: Option<bool>,
) -> std::result::Result<(), Failure> {
    let guard = live.borrow();
    let entry = guard
        .graph
        .nodes
        .get(&id)
        .ok_or_else(|| unavailable("Audio device is unavailable"))?;
    let mut properties = Vec::new();
    if let Some(channels) = &channels {
        properties.push(Property::new(
            spa::sys::SPA_PROP_channelVolumes,
            Value::ValueArray(ValueArray::Float(channels.clone())),
        ));
    }
    if let Some(muted) = muted {
        properties.push(Property::new(spa::sys::SPA_PROP_mute, Value::Bool(muted)));
    }
    let route = entry.device.and_then(|(device, route_device)| {
        let index = *guard.graph.devices.get(&device)?.get(&route_device)?;
        guard
            .devices
            .get(&device)
            .map(|(proxy, _)| (proxy, index, route_device))
    });
    match route {
        Some((device, index, route_device)) => {
            let object = Object {
                type_: SpaTypes::ObjectParamRoute.as_raw(),
                id: ParamType::Route.as_raw(),
                properties: vec![
                    Property::new(spa::sys::SPA_PARAM_ROUTE_index, Value::Int(index)),
                    Property::new(spa::sys::SPA_PARAM_ROUTE_device, Value::Int(route_device)),
                    Property::new(
                        spa::sys::SPA_PARAM_ROUTE_props,
                        Value::Object(props_object(properties)),
                    ),
                    Property::new(spa::sys::SPA_PARAM_ROUTE_save, Value::Bool(true)),
                ],
            };
            let bytes = pod_bytes(object)?;
            device.set_param(
                ParamType::Route,
                0,
                Pod::from_bytes(&bytes).ok_or_else(action_error)?,
            );
        }
        None => {
            let (node, _) = guard.nodes.get(&id).ok_or_else(action_error)?;
            let bytes = pod_bytes(props_object(properties))?;
            node.set_param(
                ParamType::Props,
                0,
                Pod::from_bytes(&bytes).ok_or_else(action_error)?,
            );
        }
    }
    drop(guard);
    // Keep the next step consistent before the server echoes the new level.
    if let Some(entry) = live.borrow_mut().graph.nodes.get_mut(&id) {
        if let Some(channels) = channels {
            entry.channels = channels;
        }
        if let Some(muted) = muted {
            entry.muted = Some(muted);
        }
    }
    Ok(())
}

fn apply(
    live: &RefCell<Live>,
    target: &Target,
    action: &Action,
) -> std::result::Result<(), Failure> {
    let (id, count, muted) = {
        let guard = live.borrow();
        let id = guard.graph.resolve(target)?;
        let entry = &guard.graph.nodes[&id];
        (id, entry.channels.len(), entry.muted.unwrap_or(false))
    };
    match action {
        Action::Volume(_) | Action::Mute(_) | Action::ToggleMute if count == 0 => {
            Err(unavailable("Audio device is unavailable"))
        }
        Action::Volume(value) => write(live, id, Some(vec![*value as f32; count]), None),
        Action::Mute(muted) => write(live, id, None, Some(*muted)),
        Action::ToggleMute => write(live, id, None, Some(!muted)),
    }
}

/// One key step on the default sink. At 0 % or 100 % nothing is written, but the outcome is
/// still reported so the pill shows at the limits.
fn step(live: &RefCell<Live>, delta: f64) -> std::result::Result<AudioStepOutcome, Failure> {
    let (id, current, node_name, label, muted) = {
        let guard = live.borrow();
        let id = guard.graph.resolve(&Target::DefaultSink)?;
        let entry = &guard.graph.nodes[&id];
        (
            id,
            entry
                .channels
                .iter()
                .map(|v| f64::from(*v))
                .collect::<Vec<_>>(),
            entry.name.clone(),
            entry.description.clone(),
            entry.muted.unwrap_or(false),
        )
    };
    let next = stepped_channel_volumes(&current, delta)
        .ok_or_else(|| unavailable("Audio device is unavailable"))?;
    let linear = next.iter().copied().fold(0.0, f64::max);
    let outcome = AudioStepOutcome {
        node_name,
        label,
        linear,
        volume: round(linear.cbrt(), 3),
        muted,
    };
    if next
        .iter()
        .zip(&current)
        .any(|(a, b)| (a - b).abs() >= 1e-6)
    {
        write(
            live,
            id,
            Some(next.iter().map(|v| *v as f32).collect()),
            None,
        )?;
    }
    Ok(outcome)
}

fn set_default(live: &RefCell<Live>, name: &str) -> std::result::Result<(), Failure> {
    let guard = live.borrow();
    let class = guard
        .graph
        .nodes
        .values()
        .find(|e| e.name == name && matches!(e.class, Some(Class::Sink) | Some(Class::Source)))
        .and_then(|e| e.class)
        .ok_or_else(|| unavailable("Audio device is unavailable"))?;
    let (_, metadata, _) = guard
        .metadata
        .as_ref()
        .ok_or_else(|| unavailable("Audio devices are unavailable"))?;
    let key = if class == Class::Sink {
        "default.configured.audio.sink"
    } else {
        "default.configured.audio.source"
    };
    metadata.set_property(
        0,
        key,
        Some("Spa:String:JSON"),
        Some(&serde_json::json!({ "name": name }).to_string()),
    );
    Ok(())
}

/// A capture stream: the default source at 8 kHz for the peak meter, the default sink's
/// monitor at 48 kHz for the spectrum, both 16-bit mono. Whole windows go to the async side.
fn capture_stream(
    core: &CoreRc,
    kind: Capture,
    events: Events,
) -> std::result::Result<(StreamRc, StreamListener<Vec<u8>>), pw::Error> {
    let (name, rate, window) = match kind {
        Capture::Peak => (PEAK_CLIENT, 8000, audio_peak::WINDOW_BYTES),
        Capture::Spectrum => (SPECTRUM_CLIENT, 48000, audio_spectrum::HOP * 2),
    };
    let mut props = pw::properties::PropertiesBox::new();
    props.insert(*pw::keys::MEDIA_TYPE, "Audio");
    props.insert(*pw::keys::MEDIA_CATEGORY, "Capture");
    props.insert(*pw::keys::MEDIA_ROLE, "Music");
    props.insert(*pw::keys::NODE_NAME, name);
    props.insert(*pw::keys::APP_NAME, name);
    props.insert(*pw::keys::NODE_LATENCY, format!("{}/{rate}", window / 2));
    if kind == Capture::Spectrum {
        props.insert(*pw::keys::STREAM_CAPTURE_SINK, "true");
    }
    let stream = StreamRc::new(core.clone(), name, props)?;
    let listener = stream
        .add_local_listener_with_user_data(Vec::with_capacity(window * 2))
        .process(move |stream, pending: &mut Vec<u8>| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(data) = buffer.datas_mut().first_mut() else {
                return;
            };
            let (offset, size) = (data.chunk().offset() as usize, data.chunk().size() as usize);
            if let Some(bytes) = data.data() {
                let end = offset.saturating_add(size).min(bytes.len());
                pending.extend_from_slice(&bytes[offset.min(end)..end]);
            }
            while pending.len() >= window {
                let frame: Vec<u8> = pending.drain(..window).collect();
                let _ = events.send(match kind {
                    Capture::Peak => Event::Peak(audio_peak::peak(&frame)),
                    Capture::Spectrum => Event::Spectrum(frame),
                });
            }
        })
        .register()?;
    let mut info = spa::param::audio::AudioInfoRaw::new();
    info.set_format(spa::param::audio::AudioFormat::S16LE);
    info.set_rate(rate);
    info.set_channels(1);
    let bytes = pod_bytes(Object {
        type_: SpaTypes::ObjectParamFormat.as_raw(),
        id: ParamType::EnumFormat.as_raw(),
        properties: info.into(),
    })
    .map_err(|_| pw::Error::CreationFailed)?;
    let mut params = [Pod::from_bytes(&bytes).ok_or(pw::Error::CreationFailed)?];
    stream.connect(
        spa::utils::Direction::Input,
        None,
        pw::stream::StreamFlags::AUTOCONNECT | pw::stream::StreamFlags::MAP_BUFFERS,
        &mut params,
    )?;
    Ok((stream, listener))
}

fn set_capture(live: &RefCell<Live>, events: &Events, kind: Capture, on: bool) {
    let slot = kind as usize;
    if !on {
        live.borrow_mut().streams[slot] = None;
        return;
    }
    if live.borrow().streams[slot].is_some() {
        return;
    }
    let core = live.borrow().core.clone();
    match capture_stream(&core, kind, events.clone()) {
        Ok(stream) => live.borrow_mut().streams[slot] = Some(stream),
        Err(error) => tracing::warn!(%error, "Audio capture stream could not be created"),
    }
}

fn handle_command(
    live: &Rc<RefCell<Live>>,
    events: &Events,
    wanted: &RefCell<[bool; 2]>,
    command: Command,
) {
    match command {
        Command::Stop => {}
        Command::Capture { kind, on } => {
            wanted.borrow_mut()[kind as usize] = on;
            set_capture(live, events, kind, on);
        }
        Command::Apply {
            target,
            action,
            reply,
        } => {
            let _ = reply.send(apply(live, &target, &action));
        }
        Command::SetDefault { name, reply } => {
            let _ = reply.send(set_default(live, &name));
        }
        Command::Step {
            delta,
            key_ms,
            reply,
        } => {
            let result = step(live, delta);
            if let Some(reply) = reply {
                let _ = reply.send(result.clone().map(|_| ()));
            }
            let _ = events.send(Event::Step { result, key_ms });
        }
    }
}

fn parse<T: std::str::FromStr>(props: &spa::utils::dict::DictRef, key: &str) -> Option<T> {
    props.get(key)?.parse().ok()
}

/// One connection: runs the loop until the server goes away or `Stop` arrives.
fn connect(
    commands: pw::channel::Receiver<Command>,
    events: &Events,
    connected: &AtomicBool,
    wanted: &Rc<RefCell<[bool; 2]>>,
) -> (
    pw::channel::Receiver<Command>,
    std::result::Result<(), pw::Error>,
) {
    let setup: std::result::Result<_, pw::Error> = (|| {
        let mainloop = pw::main_loop::MainLoopRc::new(None)?;
        let context = pw::context::ContextRc::new(&mainloop, None)?;
        let core = context.connect_rc(None)?;
        let registry = core.get_registry_rc()?;
        Ok((mainloop, context, core, registry))
    })();
    let (mainloop, _context, core, registry) = match setup {
        Ok(setup) => setup,
        Err(error) => return (commands, Err(error)),
    };
    let live = Rc::new(RefCell::new(Live {
        core: core.clone(),
        graph: Graph::default(),
        nodes: HashMap::new(),
        devices: HashMap::new(),
        metadata: None,
        streams: [None, None],
    }));
    let weak_loop = mainloop.downgrade();
    let _core_listener = core
        .add_listener_local()
        .error(move |id, _, _, _| {
            if id == 0 {
                if let Some(mainloop) = weak_loop.upgrade() {
                    mainloop.quit();
                }
            }
        })
        .register();
    let (added, removed) = (live.clone(), live.clone());
    let (added_events, removed_events) = (events.clone(), events.clone());
    let registry_copy = registry.clone();
    let _registry_listener = registry
        .add_listener_local()
        .global(move |object| {
            let Some(props) = object.props.as_ref() else {
                return;
            };
            let id = object.id;
            let live = &added;
            match object.type_ {
                ObjectType::Node => {
                    let Some(class) = props.get("media.class").and_then(Class::parse) else {
                        return;
                    };
                    let Ok(node) = registry_copy.bind::<Node, _>(object) else {
                        return;
                    };
                    let text = |key: &str| props.get(key).map(str::to_owned);
                    let entry = Entry {
                        name: text("node.name").unwrap_or_default(),
                        description: text("node.description")
                            .or_else(|| text("node.nick"))
                            .unwrap_or_default(),
                        class: Some(class),
                        app: text("application.name"),
                        media: text("media.name"),
                        icon: text("application.icon_name"),
                        device: parse(props, "device.id").zip(parse(props, "card.profile.device")),
                        channels: Vec::new(),
                        muted: None,
                    };
                    let (weak, events) = (Rc::downgrade(live), added_events.clone());
                    let listener = node
                        .add_listener_local()
                        .param(move |_, kind, _, _, pod| {
                            let (Some(live), Some(pod)) = (weak.upgrade(), pod) else {
                                return;
                            };
                            if kind != ParamType::Props {
                                return;
                            }
                            let Some(object) = deserialize(pod) else {
                                return;
                            };
                            let (channels, muted) = volume_props(object);
                            {
                                let mut guard = live.borrow_mut();
                                let Some(entry) = guard.graph.nodes.get_mut(&id) else {
                                    return;
                                };
                                if let Some(channels) = channels {
                                    entry.channels = channels;
                                }
                                if let Some(muted) = muted {
                                    entry.muted = Some(muted);
                                }
                            }
                            publish(&live, &events);
                        })
                        .register();
                    node.subscribe_params(&[ParamType::Props]);
                    node.enum_params(0, Some(ParamType::Props), 0, u32::MAX);
                    let mut guard = live.borrow_mut();
                    guard.graph.nodes.insert(id, entry);
                    guard.nodes.insert(id, (node, listener));
                }
                ObjectType::Device => {
                    if props.get("media.class") != Some("Audio/Device") {
                        return;
                    }
                    let Ok(device) = registry_copy.bind::<Device, _>(object) else {
                        return;
                    };
                    let weak = Rc::downgrade(live);
                    let listener = device
                        .add_listener_local()
                        .param(move |_, kind, _, _, pod| {
                            let (Some(live), Some(pod)) = (weak.upgrade(), pod) else {
                                return;
                            };
                            if kind != ParamType::Route {
                                return;
                            }
                            let Some((route_device, index, volume)) =
                                deserialize(pod).and_then(route_props)
                            else {
                                return;
                            };
                            let mut guard = live.borrow_mut();
                            let routes = guard.graph.devices.entry(id).or_default();
                            if volume {
                                routes.insert(route_device, index);
                            } else {
                                routes.remove(&route_device);
                            }
                        })
                        .register();
                    device.subscribe_params(&[ParamType::Route]);
                    device.enum_params(0, Some(ParamType::Route), 0, u32::MAX);
                    let mut guard = live.borrow_mut();
                    guard.graph.devices.entry(id).or_default();
                    guard.devices.insert(id, (device, listener));
                    return;
                }
                ObjectType::Metadata => {
                    if props.get("metadata.name") != Some("default") {
                        return;
                    }
                    let Ok(metadata) = registry_copy.bind::<Metadata, _>(object) else {
                        return;
                    };
                    let (weak, events) = (Rc::downgrade(live), added_events.clone());
                    let listener = metadata
                        .add_listener_local()
                        .property(move |subject, key, _, value| {
                            let Some(live) = weak.upgrade() else { return 0 };
                            if subject != 0 {
                                return 0;
                            }
                            let name = value
                                .and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
                                .and_then(|v| v["name"].as_str().map(str::to_owned));
                            {
                                let mut guard = live.borrow_mut();
                                match key {
                                    Some("default.audio.sink") => guard.graph.default_sink = name,
                                    Some("default.audio.source") => {
                                        guard.graph.default_source = name;
                                    }
                                    None => {
                                        guard.graph.default_sink = None;
                                        guard.graph.default_source = None;
                                    }
                                    _ => return 0,
                                }
                            }
                            publish(&live, &events);
                            0
                        })
                        .register();
                    live.borrow_mut().metadata = Some((id, metadata, listener));
                }
                _ => return,
            }
            publish(live, &added_events);
        })
        .global_remove(move |id| {
            let changed = {
                let mut guard = removed.borrow_mut();
                let node = guard.graph.nodes.remove(&id).is_some();
                guard.nodes.remove(&id);
                guard.graph.devices.remove(&id);
                guard.devices.remove(&id);
                if guard.metadata.as_ref().is_some_and(|(m, _, _)| *m == id) {
                    guard.metadata = None;
                }
                node
            };
            if changed {
                publish(&removed, &removed_events);
            }
        })
        .register();
    let (command_live, command_events, command_wanted) =
        (live.clone(), events.clone(), wanted.clone());
    let weak_loop = mainloop.downgrade();
    let attached = commands.attach(mainloop.loop_(), move |command| {
        if matches!(command, Command::Stop) {
            if let Some(mainloop) = weak_loop.upgrade() {
                mainloop.quit();
            }
            return;
        }
        handle_command(&command_live, &command_events, &command_wanted, command);
    });
    for (slot, kind) in [Capture::Peak, Capture::Spectrum].into_iter().enumerate() {
        if wanted.borrow()[slot] {
            set_capture(&live, events, kind, true);
        }
    }
    connected.store(true, Ordering::Relaxed);
    publish(&live, events);
    mainloop.run();
    live.borrow_mut().streams = [None, None];
    (attached.deattach(), Ok(()))
}

// -------------------------------------------------------------- async side

/// The default device as the OSD compares it: identity plus level and mute.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct DefaultNode {
    pub(super) name: String,
    pub(super) description: String,
    /// Perceptual level, three decimals, as `wpctl get-volume` showed it.
    pub(super) volume: f64,
    pub(super) muted: bool,
}
fn default_node(state: &AudioState, id: Option<&EntityId>) -> Option<DefaultNode> {
    let id = id?;
    let node = state.nodes.items.get(id)?;
    let perceptual = node.readback.as_ref()?.perceptual_volume?;
    Some(DefaultNode {
        name: id.0.clone(),
        description: node.name.clone(),
        volume: round(perceptual, 3),
        muted: node.muted,
    })
}

/// A change of the default device is `switched` (when it has an OSD); the same device
/// with another volume or mute state is a `level` change. The first read is the baseline.
pub(super) fn audio_event(
    before: Option<&DefaultNode>,
    now: &DefaultNode,
    level: OsdKind,
    switched: Option<OsdKind>,
) -> Option<OsdEvent> {
    let before = before?;
    let kind = if before.name != now.name {
        switched?
    } else if before.muted != now.muted || (before.volume - now.volume).abs() > 0.0005 {
        level
    } else {
        return None;
    };
    let mut event = OsdEvent::new(kind, osd::now_ms());
    event.value = Some(now.volume);
    event.muted = Some(now.muted);
    event.label = Some(now.description.clone()).filter(|d| !d.is_empty());
    event.device = Some(now.name.clone());
    Some(event)
}

pub fn initial(hub: &Shared) -> Result<()> {
    publish_audio(hub, &unavailable_audio())
}
fn publish_audio(hub: &Shared, state: &AudioState) -> Result<()> {
    debug_assert!(CoreSnapshot::Audio(state.clone()).valid());
    hub.borrow_mut()
        .publish(TOPIC, serde_json::to_value(state)?)?;
    Ok(())
}

async fn due(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Publishes what the client thread reports and leases the captures to it while their
/// topics have subscribers. Runs for the daemon's life.
pub(crate) async fn run(
    hub: Shared,
    handle: Handle,
    mut events: mpsc::UnboundedReceiver<Event>,
    silence_after: Duration,
) -> Result<()> {
    let mut peak_interest = hub.borrow_mut().watch_interest(audio_peak::TOPIC);
    let mut spectrum_interest = hub.borrow_mut().watch_interest(audio_spectrum::TOPIC);
    hub.borrow_mut().service("audio", "running", 0)?;
    for (interest, kind) in [
        (&peak_interest, Capture::Peak),
        (&spectrum_interest, Capture::Spectrum),
    ] {
        if *interest.borrow() {
            handle.send(Command::Capture { kind, on: true });
        }
    }
    let (mut pending, mut publish_at) = (None::<AudioState>, None::<Instant>);
    let mut published = unavailable_audio();
    let (mut sink, mut source) = (None::<DefaultNode>, None::<DefaultNode>);
    let mut peak_level = None::<f64>;
    let mut analyzer = audio_spectrum::Analyzer::new();
    analyzer.set_onset(
        hub.borrow()
            .snapshot_ref(audio_spectrum::TOPIC)
            .and_then(|s| s["onset"].as_u64())
            .unwrap_or(0),
    );
    let mut elapsed = Duration::ZERO;
    let (mut idle_at, mut stalled) = (None::<Instant>, false);
    loop {
        tokio::select! {
            event = events.recv() => match event {
                None => return Err("Audio client ended".into()),
                Some(Event::State(state)) => {
                    pending = Some(state);
                    publish_at.get_or_insert(Instant::now() + DEBOUNCE);
                }
                Some(Event::Step { result, key_ms }) => match result {
                    Ok(outcome) => pill_controls::publish_audio_step(&hub, &outcome, key_ms)?,
                    Err(error) => tracing::debug!(code = ?error.code, "Audio step failed"),
                },
                Some(Event::Peak(level)) => {
                    if *peak_interest.borrow() && peak_level != Some(level) {
                        peak_level = Some(level);
                        audio_peak::publish(&hub, true, level)?;
                    }
                }
                Some(Event::Spectrum(bytes)) => {
                    if !*spectrum_interest.borrow() {
                        continue;
                    }
                    idle_at = Some(Instant::now() + silence_after);
                    stalled = false;
                    analyzer.ingest(&bytes, silence_after);
                    elapsed += Duration::from_secs_f64(1.0 / 60.0);
                    let period = hub.borrow().spectrum_period();
                    if elapsed >= period {
                        audio_spectrum::publish(&hub, analyzer.analyze(elapsed))?;
                        elapsed = Duration::ZERO;
                    }
                }
            },
            () = due(publish_at) => {
                publish_at = None;
                let Some(state) = pending.take() else { continue };
                if state != published {
                    publish_audio(&hub, &state)?;
                    published = state;
                }
                if let Some(now) = default_node(&published, published.default_output.as_ref()) {
                    if let Some(event) = audio_event(sink.as_ref(), &now, OsdKind::Volume, Some(OsdKind::Output))
                        .filter(|event| !osd::key_step_echo(event))
                    {
                        osd::report(&hub, event);
                    }
                    sink = Some(now);
                }
                if let Some(now) = default_node(&published, published.default_input.as_ref()) {
                    if let Some(event) = audio_event(source.as_ref(), &now, OsdKind::Microphone, None) {
                        osd::report(&hub, event);
                    }
                    source = Some(now);
                }
            }
            result = peak_interest.changed() => {
                result?;
                let on = *peak_interest.borrow();
                handle.send(Command::Capture { kind: Capture::Peak, on });
                if !on {
                    peak_level = None;
                    audio_peak::publish(&hub, false, 0.0)?;
                }
            }
            result = spectrum_interest.changed() => {
                result?;
                let on = *spectrum_interest.borrow();
                handle.send(Command::Capture { kind: Capture::Spectrum, on });
                if !on {
                    idle_at = None;
                    elapsed = Duration::ZERO;
                    audio_spectrum::publish(&hub, &analyzer.released())?;
                }
            }
            () = due(idle_at), if !stalled => {
                // A suspended monitor may stop sending PCM entirely: clear the display once,
                // then wait without a timer until audio resumes.
                stalled = true;
                audio_spectrum::publish(&hub, analyzer.stall(silence_after))?;
            }
        }
    }
}

#[cfg(test)]
mod tests;
