//! Fakes only, except the ignored live test: the graph is built by hand and `run` is fed
//! events over the same channel the client thread uses.
use super::*;
use crate::daemon::hub::{Hub, Notice};
use serde_json::json;
use tokio::task::LocalSet;

fn entry(name: &str, class: Class, channels: &[f32], muted: bool) -> Entry {
    Entry {
        name: name.into(),
        class: Some(class),
        channels: channels.to_vec(),
        muted: Some(muted),
        ..Default::default()
    }
}
fn graph() -> Graph {
    let mut graph = Graph::default();
    let mut speakers = entry("alsa_output.speakers", Class::Sink, &[0.125, 0.125], false);
    speakers.description = "Speakers".into();
    speakers.device = Some((30, 2));
    graph.nodes.insert(40, speakers);
    graph
        .nodes
        .insert(41, entry("alsa_input.mic", Class::Source, &[1.0], true));
    // No `Props` yet: not listed until its volume is known.
    graph
        .nodes
        .insert(42, entry("bluez_output.buds", Class::Sink, &[], false));
    graph
        .nodes
        .insert(43, entry("bad name", Class::Sink, &[0.5], false));
    let mut player = entry("firefox", Class::Playback, &[0.125, 0.064], true);
    player.app = Some("Fixture player".into());
    player.media = Some("Track".into());
    player.icon = Some("fixture-icon".into());
    graph.nodes.insert(7, player);
    let mut titled = entry("mpv", Class::Playback, &[1.0], false);
    titled.media = Some("Only a stream title".into());
    graph.nodes.insert(8, titled);
    let mut recorder = entry("rec", Class::Capture, &[0.0], false);
    recorder.app = Some("Fixture recorder".into());
    graph.nodes.insert(3, recorder);
    let mut own = entry(PEAK_CLIENT, Class::Capture, &[1.0], false);
    own.app = Some(PEAK_CLIENT.into());
    graph.nodes.insert(9, own);
    graph.devices.insert(30, BTreeMap::from([(2, 4)]));
    graph.default_sink = Some("alsa_output.speakers".into());
    graph.default_source = Some("alsa_input.mic".into());
    graph
}

#[test]
fn the_topic_lists_devices_then_streams_minus_our_own_captures() {
    let graph = graph();
    let state = graph.state();
    assert!(CoreSnapshot::Audio(state.clone()).valid());
    assert!(state.health.available);
    let ids: Vec<&str> = state.nodes.order.iter().map(|id| id.0.as_str()).collect();
    assert_eq!(
        ids,
        [
            "alsa_output.speakers",
            "alsa_input.mic",
            "sink-input:7",
            "sink-input:8",
            "source-output:3"
        ]
    );
    let node = |id: &str| &state.nodes.items[&EntityId(id.into())];
    let speakers = node("alsa_output.speakers");
    assert_eq!(
        (speakers.name.as_str(), &speakers.kind),
        ("Speakers", &AudioKind::Output)
    );
    let reading = speakers.readback.as_ref().unwrap();
    assert_eq!(
        (
            reading.linear_volume,
            reading.perceptual_volume,
            reading.muted
        ),
        (Some(0.125), Some(0.5), Some(false))
    );
    assert_eq!(node("alsa_input.mic").name, "alsa_input.mic");
    assert!(node("alsa_input.mic").muted);
    let player = node("sink-input:7");
    assert_eq!(player.kind, AudioKind::Playback);
    assert_eq!(player.name, "Fixture player");
    assert_eq!(player.icon.as_deref(), Some("fixture-icon"));
    assert!(player.muted && player.ready);
    assert_eq!(player.volume, 0.125);
    assert_eq!(node("sink-input:8").name, "Only a stream title");
    assert_eq!(node("source-output:3").kind, AudioKind::Capture);
    assert_eq!(
        state.default_output.as_ref().unwrap().0,
        "alsa_output.speakers"
    );
    assert_eq!(state.default_input.as_ref().unwrap().0, "alsa_input.mic");
    // A default that is not a listed device of that kind is no default.
    let mut graph = graph;
    graph.default_sink = Some("firefox".into());
    graph.default_source = Some("bluez_output.buds".into());
    let state = graph.state();
    assert_eq!((state.default_output, state.default_input), (None, None));
    assert!(!Graph::default().state().health.available);
    assert_eq!(Class::parse("Audio/Source/Virtual"), Some(Class::Source));
}

#[test]
fn targets_resolve_to_one_node_of_the_right_class() {
    let graph = graph();
    assert_eq!(graph.resolve(&Target::DefaultSink).unwrap(), 40);
    assert_eq!(graph.resolve(&Target::DefaultSource).unwrap(), 41);
    assert_eq!(
        graph.resolve(&Target::Node("firefox".into())).unwrap(),
        7,
        "a stream by node name"
    );
    assert_eq!(
        graph
            .resolve(&Target::Stream {
                playback: true,
                id: 7
            })
            .unwrap(),
        7
    );
    for (target, message) in [
        (Target::Node("nope".into()), "Audio device is unavailable"),
        (
            Target::Stream {
                playback: false,
                id: 7,
            },
            "Audio stream is unavailable",
        ),
        (
            Target::Stream {
                playback: true,
                id: 99,
            },
            "Audio stream is unavailable",
        ),
    ] {
        let error = graph.resolve(&target).unwrap_err();
        assert_eq!(
            (error.code, error.message.as_str()),
            (ErrorCode::Unavailable, message)
        );
    }
    let mut graph = graph;
    graph.default_sink = Some("firefox".into());
    assert!(graph.resolve(&Target::DefaultSink).is_err());
}

fn perceptual(volumes: &[f64]) -> Vec<i64> {
    volumes
        .iter()
        .map(|v| (v.cbrt() * 100.0).round() as i64)
        .collect()
}

#[test]
fn steps_move_every_channel_by_whole_percent_on_the_perceptual_scale() {
    let cubes = |levels: &[f64]| levels.iter().map(|v| v.powi(3)).collect::<Vec<_>>();
    let eight = cubes(&[0.10, 0.20, 0.30, 0.40, 0.50, 0.60, 0.70, 0.80]);
    let stepped = stepped_channel_volumes(&eight, 0.05).unwrap();
    assert_eq!(perceptual(&stepped), vec![15, 25, 35, 45, 55, 65, 75, 85]);
    let stepped = stepped_channel_volumes(&stepped, 0.05).unwrap();
    assert_eq!(perceptual(&stepped), vec![20, 30, 40, 50, 60, 70, 80, 90]);
    assert_eq!(
        perceptual(&stepped_channel_volumes(&[0.0, 0.125], -0.05).unwrap()),
        vec![0, 45]
    );
    assert_eq!(stepped_channel_volumes(&[1.0], 0.05).unwrap(), vec![1.0]);
    assert!(stepped_channel_volumes(&[], 0.05).is_none());
    assert!(stepped_channel_volumes(&[f64::NAN], 0.05).is_none());
    assert!(stepped_channel_volumes(&[0.5], f64::INFINITY).is_none());
}

#[test]
fn audio_changes_are_levels_or_device_switches_and_the_first_read_is_silent() {
    let node = |name: &str, volume: f64, muted: bool| DefaultNode {
        name: name.into(),
        description: "Speakers".into(),
        volume,
        muted,
    };
    let speakers = node("alsa_output.speakers", 0.5, false);
    let event = |before: Option<&DefaultNode>, now: &DefaultNode, switched: Option<OsdKind>| {
        audio_event(before, now, OsdKind::Volume, switched)
    };
    assert!(event(None, &speakers, Some(OsdKind::Output)).is_none());
    assert!(event(Some(&speakers), &speakers, Some(OsdKind::Output)).is_none());
    let louder = node("alsa_output.speakers", 0.6, false);
    let e = event(Some(&speakers), &louder, Some(OsdKind::Output)).unwrap();
    assert_eq!(
        (
            e.kind,
            e.value,
            e.muted,
            e.label.as_deref(),
            e.device.as_deref()
        ),
        (
            OsdKind::Volume,
            Some(0.6),
            Some(false),
            Some("Speakers"),
            Some("alsa_output.speakers")
        )
    );
    let muted = node("alsa_output.speakers", 0.6, true);
    assert_eq!(
        event(Some(&louder), &muted, None).unwrap().muted,
        Some(true)
    );
    let buds = node("bluez_output.buds", 0.2, false);
    assert_eq!(
        event(Some(&speakers), &buds, Some(OsdKind::Output))
            .unwrap()
            .kind,
        OsdKind::Output
    );
    assert!(
        event(Some(&speakers), &buds, None).is_none(),
        "a new default input is not an OSD"
    );
}

// ----------------------------------------------------------------- run

struct Fixture {
    hub: Shared,
    commands: Arc<std::sync::Mutex<Vec<String>>>,
    events: mpsc::UnboundedSender<Event>,
    notices: tokio::sync::broadcast::Receiver<Notice>,
}
impl Fixture {
    fn start() -> (Self, tokio::task::JoinHandle<Result<()>>) {
        let hub = Hub::new("audio".into(), false, Duration::from_millis(1)).shared();
        initial(&hub).unwrap();
        audio_peak::initial(&hub).unwrap();
        audio_spectrum::initial(&hub).unwrap();
        let (handle, commands) = Handle::fake(|_| Ok(()));
        let (events, receiver) = mpsc::unbounded_channel();
        let notices = hub.borrow().observe();
        let task = tokio::task::spawn_local(run(
            hub.clone(),
            handle,
            receiver,
            Duration::from_millis(60),
        ));
        (
            Self {
                hub,
                commands,
                events,
                notices,
            },
            task,
        )
    }
    fn osd_events(&mut self) -> Vec<OsdEvent> {
        let mut out = Vec::new();
        while let Ok(notice) = self.notices.try_recv() {
            if let Notice::Event { topic, data, .. } = notice {
                if topic == "osd" {
                    out.push(serde_json::from_value(data).unwrap());
                }
            }
        }
        out
    }
    async fn next_osd(&mut self) -> OsdEvent {
        for _ in 0..200 {
            if let Some(event) = self.osd_events().into_iter().next() {
                return event;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("no OSD event");
    }
    async fn no_osd(&mut self) {
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(self.osd_events(), vec![]);
    }
    async fn until(&self, topic: &str, mut wanted: impl FnMut(&serde_json::Value) -> bool) {
        for _ in 0..500 {
            if wanted(&self.hub.borrow().snapshot(topic).unwrap()) {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("{topic} never became what was wanted");
    }
    fn commands(&self) -> Vec<String> {
        self.commands.lock().unwrap().clone()
    }
    fn subscribe(&self, topic: &str) -> u64 {
        let (client, _) = self.hub.borrow_mut().add_client().unwrap();
        self.hub
            .borrow_mut()
            .subscribe(client, &[topic.into()], false)
            .unwrap();
        client
    }
}

#[tokio::test(flavor = "current_thread")]
async fn states_become_the_topic_and_the_audio_osd_events() {
    LocalSet::new()
        .run_until(async {
            let (mut f, task) = Fixture::start();
            let mut graph = graph();
            f.events.send(Event::State(graph.state())).unwrap();
            f.until("audio", |data| data["health"]["available"] == true)
                .await;
            let state: AudioState =
                serde_json::from_value(f.hub.borrow().snapshot("audio").unwrap()).unwrap();
            assert_eq!(state, graph.state(), "the topic is the state as sent");
            f.no_osd().await;
            // A level change on the default output: one `volume` event.
            graph.nodes.get_mut(&40).unwrap().channels = vec![0.216, 0.216];
            f.events.send(Event::State(graph.state())).unwrap();
            let e = f.next_osd().await;
            assert_eq!(
                (
                    e.kind,
                    e.value,
                    e.muted,
                    e.label.as_deref(),
                    e.device.as_deref()
                ),
                (
                    OsdKind::Volume,
                    Some(0.6),
                    Some(false),
                    Some("Speakers"),
                    Some("alsa_output.speakers")
                )
            );
            f.until("audio", |data| {
                data["nodes"]["items"]["alsa_output.speakers"]["volume"] == 0.216
            })
            .await;
            // The same state again publishes nothing.
            f.events.send(Event::State(graph.state())).unwrap();
            f.no_osd().await;
            graph.nodes.get_mut(&40).unwrap().muted = Some(true);
            f.events.send(Event::State(graph.state())).unwrap();
            assert_eq!(f.next_osd().await.muted, Some(true));
            graph.nodes.get_mut(&41).unwrap().channels = vec![0.027];
            f.events.send(Event::State(graph.state())).unwrap();
            let e = f.next_osd().await;
            assert_eq!((e.kind, e.value), (OsdKind::Microphone, Some(0.3)));
            // A new default output with another level is one `output` event.
            let mut buds = entry("bluez_output.buds", Class::Sink, &[0.008, 0.008], false);
            buds.description = "Buds".into();
            graph.nodes.insert(42, buds);
            graph.default_sink = Some("bluez_output.buds".into());
            f.events.send(Event::State(graph.state())).unwrap();
            let e = f.next_osd().await;
            assert_eq!(
                (e.kind, e.label.as_deref(), e.value),
                (OsdKind::Output, Some("Buds"), Some(0.2))
            );
            f.no_osd().await;
            // A new default input is not an OSD.
            graph.default_source = None;
            f.events.send(Event::State(graph.state())).unwrap();
            f.until("audio", |data| data["default_input"].is_null())
                .await;
            f.no_osd().await;
            // A key step reports its outcome at once; the server's echo of it is not a second OSD.
            let outcome = AudioStepOutcome {
                node_name: "bluez_output.buds".into(),
                label: "Buds".into(),
                linear: 0.25_f64.powi(3),
                volume: 0.25,
                muted: false,
            };
            f.events
                .send(Event::Step {
                    result: Ok(outcome),
                    key_ms: Some(1),
                })
                .unwrap();
            let e = f.next_osd().await;
            assert_eq!(
                (e.kind, e.value, e.label.as_deref()),
                (OsdKind::Volume, Some(0.25), Some("Buds"))
            );
            graph.nodes.get_mut(&42).unwrap().channels = vec![0.015625; 2];
            f.events.send(Event::State(graph.state())).unwrap();
            f.until("audio", |data| {
                data["nodes"]["items"]["bluez_output.buds"]["volume"] == 0.015625
            })
            .await;
            f.no_osd().await;
            // The client went away: unavailable, and the next device is a baseline again.
            f.events.send(Event::State(unavailable_audio())).unwrap();
            f.until("audio", |data| data["health"]["available"] == false)
                .await;
            assert!(f.commands().is_empty(), "nothing was asked of the client");
            task.abort();
        })
        .await;
}

fn pcm(amplitude: f32) -> Vec<u8> {
    (0..audio_spectrum::HOP)
        .flat_map(|i| {
            let phase = i as f32 * 1000.0 * std::f32::consts::TAU / audio_spectrum::RATE as f32;
            ((phase.sin() * amplitude * 32767.0) as i16).to_le_bytes()
        })
        .collect()
}

#[tokio::test(flavor = "current_thread")]
async fn captures_are_leased_to_the_client_only_while_subscribed() {
    LocalSet::new()
        .run_until(async {
            let (f, task) = Fixture::start();
            tokio::time::sleep(Duration::from_millis(50)).await;
            assert!(f.commands().is_empty(), "no lease, no capture");
            // Peak: the level publishes on change only and the lease ends with the last client.
            let peak = f.subscribe(audio_peak::TOPIC);
            f.until(audio_peak::TOPIC, |_| f.commands() == ["capture peak on"])
                .await;
            f.events.send(Event::Peak(0.5)).unwrap();
            f.until(audio_peak::TOPIC, |data| {
                data["health"]["available"] == true && data["level"] == 0.5
            })
            .await;
            let seq = f.hub.borrow().topic_seq(audio_peak::TOPIC).unwrap();
            f.events.send(Event::Peak(0.5)).unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert_eq!(f.hub.borrow().topic_seq(audio_peak::TOPIC).unwrap(), seq);
            f.hub.borrow_mut().remove_client(peak);
            f.until(audio_peak::TOPIC, |data| {
                data["health"]["available"] == false && data["level"] == 0.0
            })
            .await;
            assert_eq!(f.commands(), ["capture peak on", "capture peak off"]);
            f.events.send(Event::Peak(0.7)).unwrap();
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert_eq!(
                f.hub.borrow().snapshot(audio_peak::TOPIC).unwrap()["level"],
                0.0,
                "data after the release is dropped"
            );
            // Spectrum: analysed per hop, cleared when the monitor stops, onset kept on release.
            let spectrum = f.subscribe(audio_spectrum::TOPIC);
            f.until(audio_spectrum::TOPIC, |_| f.commands().len() == 3)
                .await;
            assert_eq!(f.commands()[2], "capture spectrum on");
            for _ in 0..3 {
                f.events.send(Event::Spectrum(pcm(0.5))).unwrap();
            }
            f.until(audio_spectrum::TOPIC, |data| {
                data["health"]["available"] == true && data["paused"] == false
            })
            .await;
            let state = f.hub.borrow().snapshot(audio_spectrum::TOPIC).unwrap();
            assert!((state["level"].as_f64().unwrap() - 0.354).abs() < 0.01);
            assert_eq!(state["onset"], 1);
            // Silence for longer than `silence_after` clears the display once.
            f.until(audio_spectrum::TOPIC, |data| {
                data["paused"] == true && data["level"] == 0.0
            })
            .await;
            f.events.send(Event::Spectrum(pcm(0.5))).unwrap();
            f.until(audio_spectrum::TOPIC, |data| data["paused"] == false)
                .await;
            f.hub.borrow_mut().remove_client(spectrum);
            f.until(audio_spectrum::TOPIC, |data| {
                data["health"]["available"] == false
            })
            .await;
            assert_eq!(
                f.hub.borrow().snapshot(audio_spectrum::TOPIC).unwrap()["onset"],
                1
            );
            assert_eq!(
                f.commands().last().map(String::as_str),
                Some("capture spectrum off")
            );
            task.abort();
        })
        .await;
}

/// Against a real server (`PIPEWIRE_RUNTIME_DIR`/`XDG_RUNTIME_DIR` with WirePlumber and at
/// least one `Audio/Sink`): the graph arrives, writes echo back, a step reports, captures run.
#[tokio::test(flavor = "current_thread")]
#[ignore = "needs a private PipeWire server"]
async fn live_pipewire_roundtrip() {
    let (handle, mut events, _client) = spawn().unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut state = unavailable_audio();
    while Instant::now() < deadline && state.default_output.is_none() {
        if let Ok(Some(Event::State(next))) =
            tokio::time::timeout(Duration::from_secs(1), events.recv()).await
        {
            state = next;
        }
    }
    let sink = state.default_output.clone().expect("a default sink");
    assert!(handle.connected());
    eprintln!("LIVE sinks: {}", json!(state));
    // Absolute volume, read back from the server.
    handle
        .apply(Target::DefaultSink, Action::Volume(0.125))
        .await
        .unwrap();
    let mut seen = false;
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline && !seen {
        if let Ok(Some(Event::State(next))) =
            tokio::time::timeout(Duration::from_secs(1), events.recv()).await
        {
            seen = next.nodes.items[&sink].volume == 0.125;
        }
    }
    assert!(seen, "the write echoed back");
    // A step from 50 % perceptual goes to 55 %.
    handle.step(0.05, Some(1)).await.unwrap();
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut outcome = None;
    while Instant::now() < deadline && outcome.is_none() {
        if let Ok(Some(Event::Step { result, .. })) =
            tokio::time::timeout(Duration::from_secs(1), events.recv()).await
        {
            outcome = Some(result.unwrap());
        }
    }
    assert_eq!(outcome.unwrap().volume, 0.55);
    assert!(handle
        .apply(Target::Node("nope".into()), Action::Mute(true))
        .await
        .is_err());
    handle.set_default(sink.0.clone()).await.unwrap();
    // Captures deliver windows.
    for (kind, name) in [(Capture::Peak, "peak"), (Capture::Spectrum, "spectrum")] {
        handle.send(Command::Capture { kind, on: true });
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got = false;
        while Instant::now() < deadline && !got {
            match tokio::time::timeout(Duration::from_secs(1), events.recv()).await {
                Ok(Some(Event::Peak(_))) => got = kind == Capture::Peak,
                Ok(Some(Event::Spectrum(bytes))) => {
                    got = kind == Capture::Spectrum && bytes.len() == audio_spectrum::HOP * 2;
                }
                _ => {}
            }
        }
        assert!(got, "{name} capture delivered a window");
        handle.send(Command::Capture { kind, on: false });
    }
}
