//! Fakes only: a temp LED tree and night-light record, and a private session bus with fake
//! power-profiles-daemon and UPower keyboard backlight. Audio OSD kinds are `audio`'s.
use super::*;
use crate::daemon::hub::{Hub, Notice};
use serde_json::json;
use std::{
    io::{BufRead, BufReader},
    process::{Child, Stdio},
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
            leds: self.path("leds"),
            night: vec![self.path("night.json")],
            tick: Duration::from_millis(500),
        }
    }
    fn set(&self, name: &str, value: &str) {
        std::fs::write(self.path(name), value).unwrap();
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
    /// Waits until the session is past its silent baseline (LED, night light, profile reads).
    async fn running(&self) {
        baseline().await;
    }
    async fn none(&mut self) {
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert_eq!(self.events(), vec![]);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn the_fallback_tick_follows_the_caps_lock_source() {
    let tick = Duration::from_millis(500);
    let period = |compositor, led, watched| {
        cap_ticker(compositor, led, watched, tick).map(|ticker| ticker.period())
    };
    // NN0036: the compositor reports Caps Lock and inotify covers night light.
    assert_eq!(period(true, true, true), None);
    assert_eq!(period(true, false, true), None);
    // N0321: a known LED is polled by its read job; without one a slower tick looks for one.
    assert_eq!(period(false, true, true), None);
    assert_eq!(period(false, false, true), Some(tick * REDISCOVER));
    // Unwatched night-light records keep the full tick.
    assert_eq!(period(true, false, false), Some(tick));
    assert_eq!(period(false, false, false), Some(tick));
}

/// N0321: the read job keeps polling an unchanged LED on its own thread and returns only
/// with a change, or with no reading once the session dropped its guard.
#[test]
fn a_known_led_is_polled_by_its_job_until_it_changes_or_the_session_ends() {
    let f = Fixture::new();
    let known = Some(f.path("leds/input3::capslock/brightness"));
    let tick = Duration::from_millis(10);
    let alive = std::sync::Arc::new(());
    let job = {
        let (leds, known, alive) = (
            f.path("leds"),
            known.clone(),
            std::sync::Arc::downgrade(&alive),
        );
        std::thread::spawn(move || watch_caps(leds, known, Some(false), tick, alive))
    };
    std::thread::sleep(tick * 10);
    assert!(!job.is_finished(), "an unchanged LED ended the poll");
    f.set("leds/input3::capslock/brightness", "1\n");
    assert_eq!(job.join().unwrap(), (known.clone(), Some(true)));
    let job = {
        let (leds, known, alive) = (
            f.path("leds"),
            known.clone(),
            std::sync::Arc::downgrade(&alive),
        );
        std::thread::spawn(move || watch_caps(leds, known, Some(true), tick, alive))
    };
    drop(alive);
    assert_eq!(job.join().unwrap(), (known, None));
}

#[test]
fn capslock_prefers_the_lowest_input_led() {
    let root = tempfile::tempdir().unwrap();
    for name in [
        "input12::capslock",
        "platform::capslock",
        "input3::capslock",
        "input2::numlock",
    ] {
        std::fs::create_dir(root.path().join(name)).unwrap();
    }
    assert_eq!(
        find_capslock(root.path()),
        Some(root.path().join("input3::capslock/brightness"))
    );
}

/// sysfs sends no inotify events for LED devices, so an unplugged keyboard's LED is replaced by
/// the next read itself, here by a keyboard plugged in while the first one was still there.
#[tokio::test(flavor = "current_thread")]
async fn an_unplugged_capslock_led_is_replaced_by_rediscovery() {
    let mut f = Fixture::new();
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(session(f.hub.clone(), f.env(), None));
            f.running().await;
            std::fs::create_dir_all(f.path("leds/input12::capslock")).unwrap();
            f.set("leds/input12::capslock/brightness", "1\n");
            f.none().await;
            std::fs::remove_dir_all(f.path("leds/input3::capslock")).unwrap();
            let e = f.next().await;
            assert_eq!((e.kind, e.value), (OsdKind::Capslock, Some(1.0)));
            task.abort();
        })
        .await;
}

/// NN0036: with nacre-niri the compositor reports Caps Lock and the LED is not read; when its
/// stream ends the LED takes over again from a silent baseline.
#[tokio::test(flavor = "current_thread")]
async fn compositor_reported_capslock_replaces_the_led() {
    let mut f = Fixture::new();
    capslock(&f.hub, Some(false));
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(session(f.hub.clone(), f.env(), None));
            f.running().await;
            f.set("leds/input3::capslock/brightness", "1\n");
            tokio::time::sleep(Duration::from_millis(700)).await;
            assert_eq!(f.events(), vec![]);
            capslock(&f.hub, Some(true));
            let e = f.next().await;
            assert_eq!((e.kind, e.value), (OsdKind::Capslock, Some(1.0)));
            capslock(&f.hub, None);
            f.none().await;
            f.set("leds/input3::capslock/brightness", "0\n");
            let e = f.next().await;
            assert_eq!((e.kind, e.value), (OsdKind::Capslock, Some(0.0)));
            task.abort();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn nightlight_state_watch_wakes_on_atomic_record_replace() {
    tokio::task::LocalSet::new()
        .run_until(async {
            let root = tempfile::tempdir().unwrap();
            let state = root.path().join("state");
            std::fs::create_dir_all(&state).unwrap();
            let record = state.join("control-tools.json");
            std::fs::write(&record, "{\"enabled\":false,\"owner\":\"\"}").unwrap();
            let mut watcher = Some(watch_inputs(std::slice::from_ref(&record)).unwrap());
            let changed = tokio::task::spawn_local(async move { next_watch(&mut watcher).await });
            tokio::task::yield_now().await;
            let temporary = state.join("control-tools.json.tmp");
            std::fs::write(&temporary, "{\"enabled\":true,\"owner\":\"night-light\"}").unwrap();
            std::fs::rename(temporary, &record).unwrap();
            let names = tokio::time::timeout(Duration::from_secs(2), changed)
                .await
                .unwrap()
                .unwrap()
                .unwrap();
            assert!(names.iter().any(|name| {
                record
                    .file_name()
                    .is_some_and(|file| file == name.as_os_str())
            }));
        })
        .await;
}
async fn baseline() {
    tokio::time::sleep(Duration::from_millis(400)).await;
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
    let mut f = Fixture::new();
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(serve(f.hub.clone(), f.env(), false));
            baseline().await;
            // Nobody subscribes: a LED change is not even watched.
            f.set("leds/input3::capslock/brightness", "1\n");
            f.none().await;
            let (client, _) = f.hub.borrow_mut().add_client().unwrap();
            f.hub
                .borrow_mut()
                .subscribe(client, &[TOPIC.into()], false)
                .unwrap();
            baseline().await;
            f.set("leds/input3::capslock/brightness", "0\n");
            assert_eq!(f.next().await.kind, OsdKind::Capslock);
            f.hub.borrow_mut().remove_client(client);
            baseline().await;
            f.set("leds/input3::capslock/brightness", "1\n");
            f.none().await;
            let (client, _) = f.hub.borrow_mut().add_client().unwrap();
            f.hub
                .borrow_mut()
                .subscribe(client, &[TOPIC.into()], false)
                .unwrap();
            baseline().await;
            f.set("leds/input3::capslock/brightness", "0\n");
            assert_eq!(
                f.next().await.kind,
                OsdKind::Capslock,
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
