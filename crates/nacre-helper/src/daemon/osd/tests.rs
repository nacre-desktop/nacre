//! Fakes only: scripts for `wpctl`/`pactl`, a temp LED tree and night-light record, and
//! a private session bus with fake power-profiles-daemon and UPower keyboard backlight.
use super::*;
use crate::daemon::hub::{Hub, Notice};
use serde_json::json;
use std::{
    io::{BufRead, BufReader, Write},
    os::unix::fs::PermissionsExt,
    process::Child,
};
use tokio::task::LocalSet;

struct Fixture {
    dir: tempfile::TempDir,
    hub: Shared,
    notices: tokio::sync::broadcast::Receiver<Notice>,
}
impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for (n, vol, name, desc) in [
            ("sink", "0.50", "alsa_output.speakers", "Speakers"),
            ("source", "0.80", "alsa_input.mic", "Built-in microphone"),
        ] {
            std::fs::write(root.join(format!("{n}.vol")), vol).unwrap();
            std::fs::write(root.join(format!("{n}.name")), name).unwrap();
            std::fs::write(root.join(format!("{n}.desc")), desc).unwrap();
        }
        std::fs::write(root.join("events"), "").unwrap();
        let d = root.display();
        script(
            &root.join("wpctl"),
            &format!(
                "d='{d}'\necho x >> $d/wpctl.log\ncase \"$2\" in @DEFAULT_AUDIO_SINK@) n=sink;; @DEFAULT_AUDIO_SOURCE@) n=source;; *) exit 1;; esac\ncase \"$1\" in\n get-volume) printf 'Volume: %s' \"$(cat $d/$n.vol)\"; [ -f $d/$n.muted ] && printf ' [MUTED]'; echo;;\n inspect) echo 'id 9, type PipeWire:Interface:Node'; echo \"  * node.description = \\\"$(cat $d/$n.desc)\\\"\"; echo \"    node.name = \\\"$(cat $d/$n.name)\\\"\";;\nesac\n"
            ),
        );
        script(
            &root.join("pactl"),
            &format!("touch '{d}/pactl-started'\nexec tail -n 0 -F '{d}/events'\n"),
        );
        std::fs::create_dir_all(root.join("leds/input3::capslock")).unwrap();
        std::fs::write(root.join("leds/input3::capslock/brightness"), "0\n").unwrap();
        let hub = Hub::new("fixture".into(), false, Duration::from_millis(1)).shared();
        initial(&hub).unwrap();
        let notices = hub.borrow().observe();
        Self { dir, hub, notices }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
    fn env(&self) -> Env {
        Env {
            wpctl: self.path("wpctl").into(),
            pactl: self.path("pactl").into(),
            leds: self.path("leds"),
            night: vec![self.path("night.json")],
            tick: Duration::from_millis(20),
            retry: Duration::from_millis(30),
        }
    }
    fn set(&self, name: &str, value: &str) {
        std::fs::write(self.path(name), value).unwrap();
    }
    fn pactl(&self, line: &str) {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(self.path("events"))
            .unwrap();
        writeln!(file, "{line}").unwrap();
    }
    /// Events published so far.
    fn events(&mut self) -> Vec<OsdEvent> {
        let mut out = Vec::new();
        while let Ok(notice) = self.notices.try_recv() {
            if let Notice::Event { topic, name, data } = notice {
                assert_eq!((topic.as_str(), name.as_str()), (TOPIC, EVENT));
                out.push(serde_json::from_value(data).unwrap());
            }
        }
        out
    }
    async fn next(&mut self) -> OsdEvent {
        for _ in 0..1500 {
            if let Some(event) = self.events().into_iter().next() {
                return event;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("no OSD event");
    }
    /// Waits until the session is past its silent baseline. The baseline makes four `wpctl`
    /// calls; any further call comes from the event loop, which only starts after it. Events
    /// sent before the fake `pactl` opens its tail are lost, so the sink event is repeated.
    async fn running(&self) {
        let calls =
            || std::fs::read_to_string(self.path("wpctl.log")).map_or(0, |log| log.lines().count());
        for _ in 0..1500 {
            if calls() > 4 {
                return;
            }
            if self.path("pactl-started").exists() {
                self.pactl("Event 'change' on sink #5");
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("the session never left its baseline");
    }
    async fn none(&mut self) {
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(self.events(), vec![]);
    }
}
fn script(path: &Path, body: &str) {
    std::fs::write(path, format!("#!/bin/sh\n{body}")).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}
async fn baseline() {
    tokio::time::sleep(Duration::from_millis(400)).await;
}

#[test]
fn audio_changes_are_levels_or_device_switches_and_the_first_read_is_silent() {
    let node = |name: &str, volume: f64, muted: bool| Node {
        name: name.into(),
        description: "Desc".into(),
        volume,
        muted,
    };
    let (a, b) = (node("a", 0.5, false), node("b", 0.5, false));
    fn event(before: Option<&Node>, now: &Node, switched: Option<OsdKind>) -> Option<OsdEvent> {
        audio_event(before, now, OsdKind::Volume, switched)
    }
    assert!(event(None, &a, Some(OsdKind::Output)).is_none());
    assert!(event(Some(&a), &a, Some(OsdKind::Output)).is_none());
    assert!(event(Some(&a), &node("a", 0.5004, false), None).is_none());
    let louder = event(Some(&a), &node("a", 0.6, false), None).unwrap();
    assert_eq!((louder.kind, louder.value), (OsdKind::Volume, Some(0.6)));
    assert_eq!(louder.muted, Some(false));
    assert_eq!(
        event(Some(&a), &node("a", 0.5, true), None).unwrap().muted,
        Some(true)
    );
    let switched = event(Some(&a), &b, Some(OsdKind::Output)).unwrap();
    assert_eq!(
        (switched.kind, switched.device.as_deref()),
        (OsdKind::Output, Some("b"))
    );
    assert_eq!(switched.label.as_deref(), Some("Desc"));
    assert!(
        event(Some(&a), &b, None).is_none(),
        "an input switch has no OSD"
    );
}

#[test]
fn volume_inside_a_burst_never_replaces_an_output_change() {
    let mut pending = Pending::default();
    let of = |kind| OsdEvent::new(kind, 1);
    pending.add(of(OsdKind::Output));
    pending.add(of(OsdKind::Volume));
    assert_eq!(pending.event.as_ref().unwrap().kind, OsdKind::Output);
    pending.add(of(OsdKind::Capslock));
    assert_eq!(pending.event.as_ref().unwrap().kind, OsdKind::Capslock);
}

#[test]
fn method_reports_become_events_and_bad_or_repeated_ones_do_not() {
    let mut fixture = Fixture::new();
    let hub = fixture.hub.clone();
    brightness(
        &hub,
        &json!({"changed":true,"output":"eDP-1","level_percent":75}),
    );
    brightness(
        &hub,
        &json!({"changed":true,"output":"eDP-1","level_percent":75}),
    );
    brightness(&hub, &json!({"changed":true,"level_percent":75}));
    brightness(
        &hub,
        &json!({"changed":true,"output":"eDP-1","level_percent":50}),
    );
    keyboard(&hub, 1, 2);
    keyboard(&hub, 0, 0);
    keyboard(&hub, -1, 2);
    let events = fixture.events();
    assert_eq!(events.len(), 3, "{events:?}");
    assert_eq!(
        (events[0].kind, events[0].value, events[0].device.as_deref()),
        (OsdKind::Brightness, Some(0.75), Some("eDP-1"))
    );
    assert_eq!(events[1].value, Some(0.5));
    assert_eq!(
        (events[2].kind, events[2].value, events[2].max),
        (OsdKind::Keyboard, Some(0.5), Some(2))
    );
}

#[tokio::test(flavor = "current_thread")]
async fn audio_defaults_report_levels_mute_microphone_and_output_changes() {
    let mut f = Fixture::new();
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(session(f.hub.clone(), f.env(), None));
            f.running().await;
            f.none().await;
            f.set("sink.vol", "0.60");
            f.pactl("Event 'change' on sink #5");
            let e = f.next().await;
            assert_eq!(
                (e.kind, e.value, e.muted),
                (OsdKind::Volume, Some(0.6), Some(false))
            );
            assert_eq!(e.label.as_deref(), Some("Speakers"));
            assert_eq!(e.device.as_deref(), Some("alsa_output.speakers"));
            // Unrelated events and unchanged state are silent.
            f.pactl("Event 'change' on sink-input #8");
            f.pactl("Event 'change' on sink #5");
            f.none().await;
            f.set("sink.muted", "");
            f.pactl("Event 'change' on sink #5");
            assert_eq!(f.next().await.muted, Some(true));
            f.set("source.vol", "0.30");
            f.pactl("Event 'change' on source #6");
            let e = f.next().await;
            assert_eq!((e.kind, e.value), (OsdKind::Microphone, Some(0.3)));
            // A new default output plus the volume signal it causes is one `output` event.
            f.set("sink.name", "bluez_output.buds");
            f.set("sink.desc", "Buds");
            f.set("sink.vol", "0.20");
            f.pactl("Event 'change' on server");
            f.pactl("Event 'change' on sink #9");
            let e = f.next().await;
            assert_eq!(
                (e.kind, e.label.as_deref(), e.value),
                (OsdKind::Output, Some("Buds"), Some(0.2))
            );
            f.none().await;
            // A new default input is not an OSD.
            f.set("source.name", "alsa_input.other");
            f.pactl("Event 'change' on server");
            f.none().await;
            task.abort();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn capslock_and_night_light_report_only_changes_after_the_baseline() {
    let mut f = Fixture::new();
    f.set("leds/input3::capslock/brightness", "1\n");
    f.set(
        "night.json",
        &json!({"owner":"unit","enabled":true}).to_string(),
    );
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(session(f.hub.clone(), f.env(), None));
            f.running().await;
            f.none().await;
            f.set("leds/input3::capslock/brightness", "0\n");
            let e = f.next().await;
            assert_eq!((e.kind, e.value), (OsdKind::Capslock, Some(0.0)));
            // An enabled record without a running owner is off.
            f.set(
                "night.json",
                &json!({"owner":"","enabled":true}).to_string(),
            );
            let e = f.next().await;
            assert_eq!((e.kind, e.value), (OsdKind::NightLight, Some(0.0)));
            f.set(
                "night.json",
                &json!({"owner":"unit","enabled":true}).to_string(),
            );
            assert_eq!(f.next().await.value, Some(1.0));
            task.abort();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn nothing_runs_until_a_client_subscribes_and_it_stops_after_the_last_one() {
    let f = Fixture::new();
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(serve(f.hub.clone(), f.env(), false));
            baseline().await;
            assert!(!f.path("pactl-started").exists());
            let (client, _) = f.hub.borrow_mut().add_client().unwrap();
            f.hub
                .borrow_mut()
                .subscribe(client, &[TOPIC.into()], false)
                .unwrap();
            for _ in 0..100 {
                if f.path("pactl-started").exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(f.path("pactl-started").exists());
            f.hub.borrow_mut().remove_client(client);
            std::fs::remove_file(f.path("pactl-started")).unwrap();
            baseline().await;
            let (client, _) = f.hub.borrow_mut().add_client().unwrap();
            f.hub
                .borrow_mut()
                .subscribe(client, &[TOPIC.into()], false)
                .unwrap();
            for _ in 0..100 {
                if f.path("pactl-started").exists() {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            assert!(
                f.path("pactl-started").exists(),
                "a later subscriber starts it again"
            );
            task.abort();
        })
        .await;
}

// ------------------------------------------------------------- private bus

struct PrivateBus(Child, String);
impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
fn private_bus(home: &Path) -> PrivateBus {
    let mut child = std::process::Command::new("dbus-daemon")
        .args([
            "--session",
            "--nofork",
            "--print-address",
            &format!("--address=unix:path={}", home.join("bus").display()),
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .stdin(Stdio::null())
        .spawn()
        .unwrap();
    let mut address = String::new();
    BufReader::new(child.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    PrivateBus(child, address.trim().to_owned())
}
async fn connect(bus: &PrivateBus, name: Option<&str>) -> zbus::Connection {
    let builder = zbus::connection::Builder::address(bus.1.as_str()).unwrap();
    match name {
        Some(name) => builder.name(name.to_owned()).unwrap(),
        None => builder,
    }
    .build()
    .await
    .unwrap()
}

struct Profiles(String);
#[zbus::interface(name = "org.freedesktop.UPower.PowerProfiles")]
impl Profiles {
    #[zbus(property)]
    fn active_profile(&self) -> String {
        self.0.clone()
    }
    #[zbus(property)]
    fn set_active_profile(&mut self, profile: String) {
        self.0 = profile;
    }
}
struct Backlight;
#[zbus::interface(name = "org.freedesktop.UPower.KbdBacklight")]
impl Backlight {
    fn get_max_brightness(&self) -> i32 {
        2
    }
}

#[tokio::test(flavor = "current_thread")]
async fn power_profile_and_keyboard_backlight_follow_the_system_bus() {
    let mut f = Fixture::new();
    let bus = private_bus(f.dir.path());
    let profiles = zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .name(PROFILE_SERVICES[0].0)
        .unwrap()
        .serve_at(PROFILE_SERVICES[0].1, Profiles("balanced".into()))
        .unwrap()
        .build()
        .await
        .unwrap();
    let upower = zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .name(UPOWER)
        .unwrap()
        .serve_at(KEYBOARD_PATH, Backlight)
        .unwrap()
        .build()
        .await
        .unwrap();
    let client = connect(&bus, None).await;
    LocalSet::new()
        .run_until(async {
            let task =
                tokio::task::spawn_local(session(f.hub.clone(), f.env(), Some(client.clone())));
            f.running().await;
            f.none().await;
            let service = proxy(
                &client,
                PROFILE_SERVICES[0].0,
                PROFILE_SERVICES[0].1,
                PROFILE_SERVICES[0].0,
            )
            .await
            .unwrap();
            service
                .set_property("ActiveProfile", "performance")
                .await
                .unwrap();
            let e = f.next().await;
            assert_eq!(
                (e.kind, e.label.as_deref()),
                (OsdKind::PowerProfile, Some("performance"))
            );
            // Unknown profile names are never reported.
            service
                .set_property("ActiveProfile", "turbo")
                .await
                .unwrap();
            f.none().await;
            upower
                .emit_signal(
                    None::<&str>,
                    KEYBOARD_PATH,
                    KEYBOARD_IFACE,
                    "BrightnessChanged",
                    &(1i32,),
                )
                .await
                .unwrap();
            let e = f.next().await;
            assert_eq!(
                (e.kind, e.value, e.max),
                (OsdKind::Keyboard, Some(0.5), Some(2))
            );
            // The method report for the same key press adds nothing.
            keyboard(&f.hub, 1, 2);
            f.none().await;
            task.abort();
        })
        .await;
    drop(profiles);
}

#[tokio::test(flavor = "current_thread")]
async fn a_failing_pactl_degrades_only_audio_and_is_retried() {
    let mut f = Fixture::new();
    // pactl is present but no audio server answers: it exits at once, every time.
    let d = f.dir.path().display().to_string();
    script(
        &f.path("pactl"),
        &format!("echo x >> '{d}/pactl-runs'\nexit 1\n"),
    );
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(serve(f.hub.clone(), f.env(), false));
            baseline().await;
            let (client, _) = f.hub.borrow_mut().add_client().unwrap();
            f.hub
                .borrow_mut()
                .subscribe(client, &[TOPIC.into()], false)
                .unwrap();
            // Well past the five failures that used to disable the service.
            for _ in 0..200 {
                let runs = std::fs::read_to_string(f.path("pactl-runs")).unwrap_or_default();
                if runs.lines().count() >= 7 {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            let runs = std::fs::read_to_string(f.path("pactl-runs")).unwrap_or_default();
            assert!(runs.lines().count() >= 7, "audio is retried: {runs:?}");
            assert!(!task.is_finished(), "the osd service keeps running");
            // Other producers are unaffected.
            f.set("leds/input3::capslock/brightness", "1\n");
            let e = f.next().await;
            assert_eq!((e.kind, e.value), (OsdKind::Capslock, Some(1.0)));
            task.abort();
        })
        .await;
}
