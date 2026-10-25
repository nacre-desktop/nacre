//! Policy ports are checked exactly; the behaviours run against a fake engine, a fake
//! clock and fake UPower, power-profiles-daemon and notification services on a private bus.
use super::*;
use crate::daemon::{hub::Hub, settings};
use nacre_protocol::settings::SettingsSet;
use std::{
    cell::Cell,
    io::{BufRead, BufReader},
    process::Child,
    sync::{Arc, Mutex},
};
use tokio::{sync::Notify, task::LocalSet};

fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

/// Polls a condition; the bound only stops a hang, no test waits for it to elapse.
async fn eventually(mut check: impl FnMut() -> bool) {
    for _ in 0..1500 {
        if check() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("condition not reached");
}
async fn quiet() {
    tokio::time::sleep(Duration::from_millis(300)).await;
}

fn home() -> (tempfile::TempDir, Paths) {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths::new(home.path());
    std::fs::create_dir_all(paths.state.join("wallpaper")).unwrap();
    std::fs::create_dir_all(&paths.config).unwrap();
    (home, paths)
}

async fn hub_with_settings(paths: &Paths) -> (Shared, Rc<settings::Service>) {
    let hub = Hub::new("fixture".into(), false, Duration::from_millis(1)).shared();
    let service = settings::Service::load(hub.clone(), paths.clone(), None)
        .await
        .unwrap();
    hub.borrow_mut().settings = Some(service.clone());
    (hub, service)
}

// ------------------------------------------------------------- pure policy

#[test]
fn rotation_choice_matches_the_qml_script() {
    let all = strings(&["/a", "/b", "/c"]);
    assert_eq!(rotation_next(&all, "/a", false, &[], 0.0).0.unwrap(), "/b");
    assert_eq!(rotation_next(&all, "/c", false, &[], 0.0).0.unwrap(), "/a");
    assert_eq!(
        rotation_next(&all, "/gone", false, &[], 0.0).0.unwrap(),
        "/a"
    );
    assert_eq!(
        rotation_next(&all[..1], "/a", true, &[], 0.0),
        (None, vec![])
    );
    // The shuffled bag never offers the current image and drains before refilling.
    let (first, bag) = rotation_next(&all, "/a", true, &[], 0.0);
    assert_eq!(
        (first.as_deref(), bag.clone()),
        (Some("/b"), strings(&["/c"]))
    );
    let (second, bag) = rotation_next(&all, "/b", true, &bag, 0.99);
    assert_eq!((second.as_deref(), bag.clone()), (Some("/c"), vec![]));
    let (third, bag) = rotation_next(&all, "/c", true, &bag, 0.99);
    assert_eq!((third.as_deref(), bag), (Some("/b"), strings(&["/a"])));
    // Stale bag members and the current image are dropped.
    let (choice, _) = rotation_next(&all, "/a", true, &strings(&["/a", "/zzz", "/c"]), 0.0);
    assert_eq!(choice.as_deref(), Some("/c"));
    let entries = vec![
        json!({"path":"/s","dynamic":false}),
        json!({"path":"/d","dynamic":true}),
        json!({"path":"/s","dynamic":false}),
    ];
    assert_eq!(rotation_pool(&entries, "all"), strings(&["/s", "/d"]));
    assert_eq!(rotation_pool(&entries, "dynamic"), strings(&["/d"]));
    assert_eq!(rotation_pool(&entries, "static"), strings(&["/s"]));
}

#[test]
fn wallpaper_history_records_only_committed_selection_and_traverses_like_qml() {
    let pool = strings(&["/a", "/b", "/c"]);
    let mut history = WallpaperHistory::default();
    history.committed(Some("/a"));
    history.committed(Some("/b"));
    history.committed(Some("/c"));
    assert_eq!(
        history
            .entries
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["/a", "/b"]
    );

    let target = history.previous(&pool, "/c").unwrap();
    assert_eq!(target, "/b");
    // A failed helper selection leaves both the current path and history untouched.
    history.selection_failed(&target);
    assert_eq!(history.current.as_deref(), Some("/c"));
    assert_eq!(
        history
            .entries
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["/a", "/b"]
    );

    let target = history.previous(&pool, "/c").unwrap();
    history.committed(Some(&target));
    assert_eq!(history.current.as_deref(), Some("/b"));
    assert_eq!(
        history
            .entries
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["/a"]
    );
    let target = history.previous(&pool, "/b").unwrap();
    assert_eq!(target, "/a");
    history.committed(Some(&target));
    assert!(
        history.entries.is_empty(),
        "walking backwards removes traversed entries"
    );
}

#[test]
fn wallpaper_history_filters_pool_wraps_without_history_and_is_bounded() {
    let mut history = WallpaperHistory::default();
    history.committed(Some("/a"));
    history.committed(Some("/b"));
    history.committed(Some("/c"));
    history.committed(Some("/d"));
    let filtered_pool = strings(&["/a", "/c", "/d"]);
    let target = history.previous(&filtered_pool, "/d").unwrap();
    assert_eq!(target, "/c", "invalid pool entries are skipped");
    history.committed(Some(&target));
    assert_eq!(
        history
            .entries
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["/a"]
    );

    let mut empty = WallpaperHistory::default();
    empty.committed(Some("/b"));
    assert!(
        empty.previous(&strings(&["/b"]), "/b").is_none(),
        "singleton pools are unavailable"
    );
    let target = empty.previous(&strings(&["/a", "/b", "/c"]), "/b").unwrap();
    assert_eq!(
        target, "/a",
        "empty history wraps to the preceding pool entry"
    );
    empty.selection_failed(&target);
    assert!(
        empty.entries.is_empty(),
        "failed fallback does not create history"
    );
    empty.committed(Some(&target));
    assert_eq!(
        empty.entries.iter().map(String::as_str).collect::<Vec<_>>(),
        ["/b"]
    );

    let mut bounded = WallpaperHistory::default();
    bounded.committed(Some("/0"));
    for index in 1..=101 {
        bounded.committed(Some(&format!("/{index}")));
    }
    assert_eq!(bounded.entries.len(), 100);
    assert_eq!(bounded.entries.front().map(String::as_str), Some("/1"));
    assert_eq!(bounded.entries.back().map(String::as_str), Some("/100"));
}

#[test]
fn power_profile_follows_the_enabled_source_setting() {
    let data = json!({"enabled":true,"ac":"balanced","battery":"power-saver"});
    assert_eq!(desired_profile(Some(&data), false), "balanced");
    assert_eq!(desired_profile(Some(&data), true), "power-saver");
    assert_eq!(desired_profile(Some(&json!({"ac":"balanced"})), false), "");
    assert_eq!(
        desired_profile(Some(&json!({"enabled":"yes","ac":"balanced"})), false),
        ""
    );
    assert_eq!(desired_profile(None, false), "");
}

#[test]
fn battery_alert_policy_is_the_qml_state_machine() {
    let sample = |fraction: f64, discharging: bool| Sample {
        ready: true,
        present: true,
        discharging,
        fraction,
    };
    let mut policy = AlertPolicy::default();
    assert!(
        policy.request(&sample(0.1, true)).is_none(),
        "needs restored state"
    );
    policy.initialize(0);
    assert!(policy
        .request(&Sample {
            ready: false,
            ..sample(0.1, true)
        })
        .is_none());
    assert!(policy.request(&sample(0.21, true)).is_none());
    assert!(policy.request(&sample(f64::NAN, true)).is_none());
    assert!(policy.request(&sample(1.5, true)).is_none());
    let low = policy.request(&sample(0.20, true)).unwrap();
    assert_eq!((low.level, low.percent), (1, 20));
    // Undelivered requests repeat; delivered ones are once per threshold.
    assert_eq!(policy.request(&sample(0.19, true)).unwrap().level, 1);
    policy.delivered(&low);
    assert!(policy.request(&sample(0.19, true)).is_none());
    let critical = policy.request(&sample(0.15, true)).unwrap();
    assert_eq!((critical.level, critical.percent), (2, 15));
    policy.delivered(&critical);
    assert!(policy.request(&sample(0.05, true)).is_none());
    // Charging, or more than 20 %, re-arms and voids in-flight requests.
    let stale = Request {
        epoch: policy.epoch,
        ..critical
    };
    assert!(policy.request(&sample(0.05, false)).is_none());
    assert!(std::mem::take(&mut policy.rearmed));
    policy.delivered(&stale);
    assert_eq!(policy.severity, 0);
    assert_eq!(policy.request(&sample(0.14, true)).unwrap().level, 2);
    // A removed battery re-arms; a missing reading does not.
    policy.initialize(2);
    assert!(policy.request(&sample(0.1, true)).is_none());
    assert!(policy
        .request(&Sample {
            present: false,
            ..sample(0.1, true)
        })
        .is_none());
    assert_eq!(policy.severity, 0);
}

#[test]
fn startup_authorities_are_explicit_and_default_to_legacy() {
    let parse = |values: &[&str]| crate::daemon::authorities(&strings(values));
    assert_eq!(parse(&[]).unwrap(), (false, false));
    assert_eq!(parse(&["--notifications=daemon"]).unwrap(), (true, false));
    assert_eq!(parse(&["--behaviours=daemon"]).unwrap(), (false, true));
    assert_eq!(
        parse(&["--behaviours=legacy", "--notifications=daemon"]).unwrap(),
        (true, false)
    );
    for bad in [
        &["--behaviours=on"][..],
        &["--behaviours=daemon", "--behaviours=legacy"],
        &["daemon"],
    ] {
        assert!(parse(bad).is_err());
    }
}

// ------------------------------------------------------ wallpaper rotation

struct Fixture {
    _home: tempfile::TempDir,
    paths: Paths,
    dir: PathBuf,
    clock: Rc<Cell<i64>>,
    wake: Rc<Notify>,
    /// Target of the rotation's live sleep; `None` while it is not waiting on the clock.
    armed: Rc<Cell<Option<i64>>>,
}
/// Clears `armed` when the polled sleep future completes or is dropped.
struct Armed(Rc<Cell<Option<i64>>>);
impl Drop for Armed {
    fn drop(&mut self) {
        self.0.set(None);
    }
}
const T0: i64 = 1_800_000_000_000;
const ENTRY: &str =
    r#"{"path":"PATH","poster":"PATH","name":"n","dynamic":false,"animated":false}"#;

impl Fixture {
    fn new(prefs: Value) -> Self {
        let (home, paths) = home();
        let dir = home.path().join("engine");
        std::fs::create_dir_all(&dir).unwrap();
        let script = format!(
            "#!/bin/sh\ndir='{}'\nst='{}'\ncase \"$2\" in\n  catalog) touch \"$dir/catalog-started\"; cat \"$dir/catalog.json\" ;;\n  select) echo \"$3\" >> \"$dir/selects\"; [ -f \"$dir/fail\" ] && exit 1; echo \"$3\" > \"$st/last.txt\"; printf '{{\"path\":\"%s\",\"poster\":\"%s\",\"appliedAtMs\":%s}}' \"$3\" \"$3\" \"$(cat \"$dir/now\")\" > \"$st/media.json\"; echo '{}' | sed \"s|PATH|$3|g\" ;;\n  session) touch \"$dir/session-started\"; seen=0; while :; do [ -f \"$dir/session-end\" ] && exit 0; total=$(wc -l < \"$dir/session\"); [ \"$total\" -gt \"$seen\" ] && tail -n +$((seen+1)) \"$dir/session\" && seen=$total; sleep 0.05; done ;;\nesac\n",
            dir.display(),
            paths.state.join("wallpaper").display(),
            ENTRY
        );
        let engine = dir.join("engine");
        std::fs::write(&engine, script).unwrap();
        std::fs::set_permissions(&engine, std::os::unix::fs::PermissionsExt::from_mode(0o755))
            .unwrap();
        std::fs::write(
            dir.join("session"),
            "{\"locked\":false,\"sleeping\":false,\"path\":\"\"}\n",
        )
        .unwrap();
        let fixture = Self {
            _home: home,
            paths,
            dir,
            clock: Rc::new(Cell::new(T0)),
            wake: Rc::new(Notify::new()),
            armed: Rc::new(Cell::new(None)),
        };
        std::fs::write(fixture.dir.join("now"), T0.to_string()).unwrap();
        fixture.catalog(&["/w/a.png", "/w/b.png", "/w/c.png"]);
        fixture.current("/w/a.png", T0);
        let mut document = json!({"version":1,"rotationAnchorMs":T0});
        document
            .as_object_mut()
            .unwrap()
            .extend(prefs.as_object().unwrap().clone());
        std::fs::write(
            state::settings_path_in(&fixture.paths, "wallpaper-picker").unwrap(),
            document.to_string(),
        )
        .unwrap();
        fixture
    }
    fn catalog(&self, paths: &[&str]) {
        let entries: Vec<Value> = paths
            .iter()
            .map(|path| serde_json::from_str(&ENTRY.replace("PATH", path)).unwrap())
            .collect();
        std::fs::write(
            self.dir.join("catalog.json"),
            json!({"entries":entries}).to_string(),
        )
        .unwrap();
    }
    /// What a successful `wallpaper-media select` leaves behind.
    fn current(&self, path: &str, at: i64) {
        let state = self.paths.state.join("wallpaper");
        std::fs::write(state.join("last.txt"), path).unwrap();
        std::fs::write(
            state.join("media.json"),
            json!({"path":path,"poster":path,"appliedAtMs":at}).to_string(),
        )
        .unwrap();
    }
    fn selects(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.join("selects"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }
    fn advance(&self, to: i64) {
        std::fs::write(self.dir.join("now"), (T0 + to).to_string()).unwrap();
        self.clock.set(T0 + to);
        self.wake.notify_waiters();
    }
    /// Waits until the rotation idles on the clock at `at`: every change before it, including
    /// the engine call that follows a wake-up, is finished, so the clock may move again.
    async fn parked(&self, at: i64) {
        eventually(|| self.armed.get() == Some(T0 + at)).await;
    }
    /// Waits until the rotation stopped waiting on the clock (paused).
    async fn unarmed(&self) {
        eventually(|| self.armed.get().is_none()).await;
    }
    fn env(&self) -> Env {
        let (clock, wake, now) = (self.clock.clone(), self.wake.clone(), self.clock.clone());
        let armed = self.armed.clone();
        Env {
            engine: self.dir.join("engine"),
            now_ms: Rc::new(move || now.get()),
            sleep: Rc::new(move |target| {
                let (clock, wake) = (clock.clone(), wake.clone());
                let armed = armed.clone();
                Box::pin(async move {
                    // Set on the first poll: `select!` builds the future even while paused.
                    armed.set(Some(target));
                    let _guard = Armed(armed);
                    loop {
                        let notified = wake.notified();
                        if clock.get() >= target {
                            return;
                        }
                        notified.await;
                    }
                })
            }),
            random: Rc::new(|| 0.0),
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn rotation_follows_interval_failure_retry_lock_and_library() {
    let fixture =
        Fixture::new(json!({"rotationEnabled":true,"rotationMinutes":5,"rotationShuffle":false}));
    let (hub, _settings) = hub_with_settings(&fixture.paths).await;
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(rotation(
                hub.clone(),
                fixture.paths.clone(),
                fixture.env(),
            ));
            fixture.parked(300_000).await;
            assert!(fixture.selects().is_empty());
            fixture.advance(300_000 - 1);
            quiet().await;
            assert!(
                fixture.selects().is_empty(),
                "five minutes after the last change, not before"
            );
            fixture.advance(300_000);
            eventually(|| fixture.selects() == ["/w/b.png"]).await;
            // The next interval counts from the change.
            fixture.parked(600_000).await;
            fixture.advance(599_999);
            quiet().await;
            assert_eq!(fixture.selects().len(), 1);
            // A failed change is retried after 60 s, not at the next interval.
            std::fs::write(fixture.dir.join("fail"), "").unwrap();
            fixture.advance(600_000);
            eventually(|| fixture.selects().len() == 2).await;
            fixture.parked(600_000 + 60_000).await;
            fixture.advance(600_000 + 59_999);
            quiet().await;
            assert_eq!(fixture.selects().len(), 2);
            std::fs::remove_file(fixture.dir.join("fail")).unwrap();
            fixture.advance(600_000 + 60_000);
            eventually(|| fixture.selects().len() == 3).await;
            assert_eq!(fixture.selects()[2], "/w/c.png");
            fixture.parked(960_000).await;
            // Locked sessions do not rotate; unlocking resumes at once.
            let session = fixture.dir.join("session");
            let append = |line: &str| {
                use std::io::Write;
                let mut file = std::fs::OpenOptions::new()
                    .append(true)
                    .open(&session)
                    .unwrap();
                writeln!(file, "{line}").unwrap();
            };
            append("{\"locked\":true,\"sleeping\":false,\"path\":\"\"}");
            fixture.unarmed().await;
            fixture.advance(2_000_000);
            quiet().await;
            assert_eq!(fixture.selects().len(), 3);
            append("{\"locked\":false,\"sleeping\":false,\"path\":\"\"}");
            eventually(|| fixture.selects().len() == 4).await;
            assert_eq!(fixture.selects()[3], "/w/a.png", "sequential order wraps");
            fixture.parked(2_300_000).await;
            // Fewer than two wallpapers pause until the library changes.
            fixture.catalog(&["/w/a.png"]);
            hub.borrow_mut()
                .publish("wallpaper.watch", json!({"generation":2}))
                .unwrap();
            quiet().await;
            fixture.advance(2_300_000);
            quiet().await;
            assert_eq!(fixture.selects().len(), 4);
            fixture.catalog(&["/w/a.png", "/w/b.png"]);
            hub.borrow_mut()
                .publish("wallpaper.watch", json!({"generation":1}))
                .unwrap();
            eventually(|| fixture.selects().len() == 5).await;
            task.abort();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn disabled_rotation_does_nothing() {
    let fixture = Fixture::new(json!({"rotationEnabled":false,"rotationMinutes":5}));
    let (hub, _settings) = hub_with_settings(&fixture.paths).await;
    LocalSet::new()
        .run_until(async {
            let task =
                tokio::task::spawn_local(rotation(hub, fixture.paths.clone(), fixture.env()));
            fixture.advance(10_000_000);
            quiet().await;
            assert!(fixture.selects().is_empty());
            task.abort();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn manual_navigation_readiness_tracks_active_and_unavailable_wallpaper() {
    let fixture = Fixture::new(json!({"rotationEnabled":false,"rotationMinutes":5}));
    let (hub, _settings) = hub_with_settings(&fixture.paths).await;
    hub.borrow_mut()
        .wallpaper_history
        .committed(Some("/fixture/current.png"));
    hub.borrow_mut()
        .wallpaper_history
        .set_selection_stable(true);
    let pair = json!({
        "revision":1,"palette_revision":1,"path":"/fixture/current.png",
        "file_identity":{"device":"1","inode":"4","size":4,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},
        "presentation":"crop",
        "palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}}
    });
    hub.borrow_mut()
        .publish(
            super::super::active_wallpaper::TOPIC,
            json!({"available":false,"stale":false,"previous_available":false,"revision":0,
                "current":null,"outputs":{}}),
        )
        .unwrap();
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(rotation(
                hub.clone(),
                fixture.paths.clone(),
                fixture.env(),
            ));
            quiet().await;
            assert!(!fixture.dir.join("session-started").exists());
            assert!(!fixture.dir.join("catalog-started").exists());

            hub.borrow_mut()
                .publish(
                    super::super::active_wallpaper::TOPIC,
                    json!({"available":true,"stale":false,"previous_available":false,"revision":1,
                        "current":pair.clone(),"outputs":{"fixture":pair.clone()}}),
                )
                .unwrap();
            eventually(|| fixture.dir.join("session-started").exists()).await;
            eventually(|| fixture.dir.join("catalog-started").exists()).await;
            eventually(|| {
                hub.borrow()
                    .snapshot(super::super::active_wallpaper::TOPIC)
                    .is_some_and(|state| state["previous_available"] == true)
            })
            .await;

            hub.borrow_mut()
                .publish(
                    super::super::active_wallpaper::TOPIC,
                    json!({"available":false,"stale":true,"previous_available":false,"revision":1,
                        "current":pair.clone(),"outputs":{"fixture":pair.clone()}}),
                )
                .unwrap();
            eventually(|| {
                hub.borrow()
                    .snapshot(super::super::active_wallpaper::TOPIC)
                    .is_some_and(|state| state["previous_available"] != true)
            })
            .await;
            task.abort();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn ended_session_stream_clears_rotation_readiness() {
    let fixture = Fixture::new(json!({"rotationEnabled":false,"rotationMinutes":5}));
    let (hub, _settings) = hub_with_settings(&fixture.paths).await;
    hub.borrow_mut()
        .wallpaper_history
        .committed(Some("/fixture/current.png"));
    hub.borrow_mut()
        .wallpaper_history
        .set_selection_stable(true);
    let pair = json!({
        "revision":1,"palette_revision":1,"path":"/fixture/current.png",
        "file_identity":{"device":"1","inode":"4","size":4,"mtime_sec":1,"mtime_nsec":0,"ctime_sec":1,"ctime_nsec":0},
        "presentation":"crop",
        "palette":{"mode":"dark","colours":{"background":"#102030","onSurface":"#f0f0f0","primary":"#22aa88"}}
    });
    hub.borrow_mut()
        .publish(
            super::super::active_wallpaper::TOPIC,
            json!({"available":true,"stale":false,"previous_available":false,"revision":1,
                "current":pair.clone(),"outputs":{"fixture":pair}}),
        )
        .unwrap();
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(rotation(
                hub.clone(),
                fixture.paths.clone(),
                fixture.env(),
            ));
            eventually(|| fixture.dir.join("session-started").exists()).await;
            eventually(|| {
                hub.borrow()
                    .snapshot(super::super::active_wallpaper::TOPIC)
                    .is_some_and(|state| state["previous_available"] == true)
            })
            .await;

            std::fs::write(fixture.dir.join("session-end"), "").unwrap();
            eventually(|| task.is_finished()).await;
            assert!(!hub
                .borrow()
                .snapshot(super::super::active_wallpaper::TOPIC)
                .unwrap()["previous_available"]
                .as_bool()
                .unwrap_or(false));
            assert!(task.await.unwrap().is_err());
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

#[derive(Default)]
struct Battery {
    on_battery: bool,
    state: u32,
    percentage: f64,
}
struct Upower(Arc<Mutex<Battery>>);
#[zbus::interface(name = "org.freedesktop.UPower")]
impl Upower {
    #[zbus(property)]
    fn on_battery(&self) -> bool {
        self.0.lock().unwrap().on_battery
    }
}
struct Device(Arc<Mutex<Battery>>);
#[zbus::interface(name = "org.freedesktop.UPower.Device")]
impl Device {
    #[zbus(property, name = "Type")]
    fn kind(&self) -> u32 {
        2
    }
    #[zbus(property)]
    fn state(&self) -> u32 {
        self.0.lock().unwrap().state
    }
    #[zbus(property)]
    fn percentage(&self) -> f64 {
        self.0.lock().unwrap().percentage
    }
    #[zbus(property)]
    fn is_present(&self) -> bool {
        true
    }
    #[zbus(property)]
    fn power_supply(&self) -> bool {
        true
    }
}
struct FakeUpower {
    state: Arc<Mutex<Battery>>,
    connection: zbus::Connection,
}
impl FakeUpower {
    async fn new(bus: &PrivateBus, on_battery: bool, state: u32, percentage: f64) -> Self {
        let state = Arc::new(Mutex::new(Battery {
            on_battery,
            state,
            percentage,
        }));
        let connection = zbus::connection::Builder::address(bus.1.as_str())
            .unwrap()
            .name(UPOWER)
            .unwrap()
            .serve_at(UPOWER_PATH, Upower(state.clone()))
            .unwrap()
            .serve_at(DISPLAY_DEVICE, Device(state.clone()))
            .unwrap()
            .build()
            .await
            .unwrap();
        Self { state, connection }
    }
    /// Change the battery and emit the property signals the real service sends.
    async fn set(&self, on_battery: bool, state: u32, percentage: f64) {
        *self.state.lock().unwrap() = Battery {
            on_battery,
            state,
            percentage,
        };
        let server = self.connection.object_server();
        let root = server.interface::<_, Upower>(UPOWER_PATH).await.unwrap();
        root.get()
            .await
            .on_battery_changed(root.signal_emitter())
            .await
            .unwrap();
        let device = server.interface::<_, Device>(DISPLAY_DEVICE).await.unwrap();
        let (guard, emitter) = (device.get().await, device.signal_emitter());
        guard.state_changed(emitter).await.unwrap();
        guard.percentage_changed(emitter).await.unwrap();
    }
}

struct Profiles {
    active: Arc<Mutex<String>>,
    sets: Arc<Mutex<Vec<String>>>,
    performance: bool,
}
#[zbus::interface(name = "org.freedesktop.UPower.PowerProfiles")]
impl Profiles {
    #[zbus(property)]
    fn active_profile(&self) -> String {
        self.active.lock().unwrap().clone()
    }
    #[zbus(property)]
    fn set_active_profile(&mut self, profile: String) {
        self.sets.lock().unwrap().push(profile.clone());
        *self.active.lock().unwrap() = profile;
    }
    #[zbus(property)]
    fn profiles(&self) -> Vec<HashMap<String, Variant<'static>>> {
        let mut names = vec!["power-saver", "balanced"];
        if self.performance {
            names.push("performance");
        }
        names
            .into_iter()
            .map(|name| HashMap::from([("Profile".to_owned(), Variant::from(name))]))
            .collect()
    }
}

struct Power {
    _home: tempfile::TempDir,
    _bus: PrivateBus,
    hub: Shared,
    service: Rc<settings::Service>,
    sets: Arc<Mutex<Vec<String>>>,
    upower: FakeUpower,
    _profiles: zbus::Connection,
    client: zbus::Connection,
}
async fn power_fixture(performance: bool, prefs: Value) -> Power {
    let (home, paths) = home();
    let bus = private_bus(home.path());
    std::fs::write(
        state::settings_path_in(&paths, "power").unwrap(),
        prefs.to_string(),
    )
    .unwrap();
    let (hub, service) = hub_with_settings(&paths).await;
    let upower = FakeUpower::new(&bus, false, 2, 80.0).await;
    let sets = Arc::new(Mutex::new(Vec::new()));
    let profiles = Profiles {
        active: Arc::new(Mutex::new("power-saver".into())),
        sets: sets.clone(),
        performance,
    };
    let _profiles = connect_with(&bus, PROFILE_SERVICES[0], profiles).await;
    let client = connect(&bus, None).await;
    Power {
        _home: home,
        _bus: bus,
        hub,
        service,
        sets,
        upower,
        _profiles,
        client,
    }
}
async fn connect_with(
    bus: &PrivateBus,
    (name, path): (&str, &str),
    interface: impl zbus::object_server::Interface,
) -> zbus::Connection {
    zbus::connection::Builder::address(bus.1.as_str())
        .unwrap()
        .name(name.to_owned())
        .unwrap()
        .serve_at(path.to_owned(), interface)
        .unwrap()
        .build()
        .await
        .unwrap()
}
async fn change_power(service: &settings::Service, key: &str, value: Value) {
    let rev = service
        .get(SettingsGet {
            store: "power".into(),
        })
        .await
        .unwrap()["rev"]
        .as_u64()
        .unwrap();
    service
        .set(SettingsSet {
            store: "power".into(),
            key: vec![key.into()],
            value,
            rev,
        })
        .await
        .unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn power_profile_applies_once_per_source_and_setting_change() {
    let Power {
        hub,
        service,
        sets,
        upower,
        client,
        ..
    } = &power_fixture(
        false,
        json!({"version":1,"enabled":true,"ac":"balanced","battery":"power-saver"}),
    )
    .await;
    let (hub, client) = (hub.clone(), client.clone());
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(power_profile(hub, client));
            eventually(|| *sets.lock().unwrap() == ["balanced"]).await;
            // Unrelated bus traffic does not reapply.
            upower.set(false, 2, 70.0).await;
            quiet().await;
            assert_eq!(sets.lock().unwrap().len(), 1);
            upower.set(true, 2, 70.0).await;
            eventually(|| *sets.lock().unwrap() == ["balanced", "power-saver"]).await;
            // Performance needs a daemon that offers it; nothing is attempted otherwise.
            change_power(service, "battery", json!("performance")).await;
            quiet().await;
            assert_eq!(sets.lock().unwrap().len(), 2);
            change_power(service, "battery", json!("balanced")).await;
            eventually(|| sets.lock().unwrap().len() == 3).await;
            // Disabling stops automation; re-enabling applies afresh.
            change_power(service, "enabled", json!(false)).await;
            quiet().await;
            assert_eq!(sets.lock().unwrap().len(), 3);
            task.abort();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn power_profile_offers_performance_when_the_daemon_has_it() {
    let power = power_fixture(
        true,
        json!({"version":1,"enabled":true,"ac":"performance","battery":"power-saver"}),
    )
    .await;
    let (hub, client, sets) = (power.hub.clone(), power.client.clone(), power.sets.clone());
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(power_profile(hub, client));
            eventually(|| *sets.lock().unwrap() == ["performance"]).await;
            task.abort();
        })
        .await;
}

/// App, icon, summary, body and urgency of every notice the fake service received.
type Seen = Arc<Mutex<Vec<(String, String, String, String, u8)>>>;
struct Notifier {
    seen: Seen,
    failures: Arc<Mutex<usize>>,
}
#[zbus::interface(name = "org.freedesktop.Notifications")]
impl Notifier {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app: String,
        _replaces: u32,
        icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        hints: HashMap<String, zbus::zvariant::OwnedValue>,
        _timeout: i32,
    ) -> zbus::fdo::Result<u32> {
        let urgency = hints
            .get("urgency")
            .and_then(|v| u8::try_from(v.clone()).ok())
            .unwrap_or(0);
        self.seen
            .lock()
            .unwrap()
            .push((app, icon, summary, body, urgency));
        let mut failures = self.failures.lock().unwrap();
        if *failures > 0 {
            *failures -= 1;
            return Err(zbus::fdo::Error::Failed("fixture failure".into()));
        }
        Ok(1)
    }
}

struct Alerts {
    _home: tempfile::TempDir,
    _bus: PrivateBus,
    hub: Shared,
    service: Rc<settings::Service>,
    paths: Paths,
    upower: FakeUpower,
    seen: Seen,
    _server: zbus::Connection,
    system: zbus::Connection,
    session: zbus::Connection,
}
impl Alerts {
    async fn new(
        ledger: Option<u8>,
        failures: usize,
        on_battery: bool,
        state: u32,
        percentage: f64,
    ) -> Self {
        let (home, paths) = home();
        let bus = private_bus(home.path());
        if let Some(level) = ledger {
            std::fs::write(
                paths.state.join("battery-alert-state.json"),
                json!({"version":1,"severity":level}).to_string(),
            )
            .unwrap();
        }
        let (hub, service) = hub_with_settings(&paths).await;
        let upower = FakeUpower::new(&bus, on_battery, state, percentage).await;
        let seen = Arc::new(Mutex::new(Vec::new()));
        let server = connect_with(
            &bus,
            (NOTIFICATIONS, "/org/freedesktop/Notifications"),
            Notifier {
                seen: seen.clone(),
                failures: Arc::new(Mutex::new(failures)),
            },
        )
        .await;
        let (system, session) = (connect(&bus, None).await, connect(&bus, None).await);
        Self {
            _home: home,
            _bus: bus,
            hub,
            service,
            paths,
            upower,
            seen,
            _server: server,
            system,
            session,
        }
    }
    fn start(&self) -> tokio::task::JoinHandle<Result<()>> {
        tokio::task::spawn_local(battery_alerts(
            self.hub.clone(),
            self.paths.clone(),
            self.system.clone(),
            self.session.clone(),
            Timing {
                settle: Duration::from_millis(20),
                retry: Duration::from_millis(100),
                watchdog: Duration::from_secs(2),
            },
        ))
    }
    fn bodies(&self) -> Vec<(String, u8)> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .map(|n| (n.3.clone(), n.4))
            .collect()
    }
    async fn wait_ledger(&self, expected: u64) {
        for _ in 0..250 {
            if self.ledger().await == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("ledger did not reach {expected}");
    }
    async fn ledger(&self) -> u64 {
        self.service
            .get(SettingsGet {
                store: "battery-alerts".into(),
            })
            .await
            .unwrap()["data"]["severity"]
            .as_u64()
            .unwrap_or(0)
    }
}

#[tokio::test(flavor = "current_thread")]
async fn battery_notices_fire_once_per_threshold_and_rearm_on_charge() {
    let alerts = Alerts::new(None, 0, true, 2, 50.0).await;
    LocalSet::new()
        .run_until(async {
            let task = alerts.start();
            quiet().await;
            assert!(alerts.bodies().is_empty());
            alerts.upower.set(true, 2, 19.6).await;
            eventually(|| alerts.bodies().len() == 1).await;
            assert_eq!(alerts.bodies()[0], ("Remaining: 20%".to_owned(), 1));
            let first = alerts.seen.lock().unwrap()[0].clone();
            assert_eq!(
                (first.0.as_str(), first.1.as_str(), first.2.as_str()),
                ("Nacre Battery", "nacre-notifications", "Battery Low")
            );
            alerts.wait_ledger(1).await;
            alerts.upower.set(true, 2, 18.0).await;
            quiet().await;
            assert_eq!(alerts.bodies().len(), 1, "same threshold: no repeat");
            alerts.upower.set(true, 2, 14.0).await;
            eventually(|| alerts.bodies().len() == 2).await;
            assert_eq!(alerts.bodies()[1], ("Remaining: 14%".to_owned(), 2));
            alerts.upower.set(true, 2, 5.0).await;
            quiet().await;
            assert_eq!(alerts.bodies().len(), 2);
            // Charging re-arms and clears the ledger.
            alerts.upower.set(false, 1, 5.0).await;
            alerts.wait_ledger(0).await;
            alerts.upower.set(true, 2, 19.0).await;
            eventually(|| alerts.bodies().len() == 3).await;
            assert_eq!(alerts.bodies()[2], ("Remaining: 19%".to_owned(), 1));
            task.abort();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn battery_notices_survive_a_restart_through_the_ledger() {
    let alerts = Alerts::new(Some(1), 0, true, 2, 50.0).await;
    LocalSet::new()
        .run_until(async {
            let task = alerts.start();
            alerts.upower.set(true, 2, 19.0).await;
            quiet().await;
            assert!(
                alerts.bodies().is_empty(),
                "the low notice was already delivered"
            );
            alerts.upower.set(true, 2, 12.0).await;
            eventually(|| alerts.bodies().len() == 1).await;
            assert_eq!(alerts.bodies()[0].1, 2);
            task.abort();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn battery_delivery_retries_then_gives_up_after_three_failures() {
    let alerts = Alerts::new(None, 2, true, 2, 50.0).await;
    LocalSet::new()
        .run_until(async {
            let task = alerts.start();
            alerts.upower.set(true, 2, 18.0).await;
            eventually(|| alerts.bodies().len() == 3).await;
            quiet().await;
            assert_eq!(
                alerts.bodies().len(),
                3,
                "delivered on the third attempt, then quiet"
            );
            task.abort();
        })
        .await;
    let alerts = Alerts::new(None, 99, true, 2, 50.0).await;
    LocalSet::new()
        .run_until(async {
            let task = alerts.start();
            alerts.upower.set(true, 2, 18.0).await;
            eventually(|| alerts.bodies().len() == 3).await;
            quiet().await;
            assert_eq!(alerts.bodies().len(), 3, "three failures stop the retries");
            // A new reading restarts evaluation; the counter only resets on re-arm or success.
            alerts.upower.set(true, 2, 17.0).await;
            quiet().await;
            assert_eq!(alerts.bodies().len(), 4);
            task.abort();
        })
        .await;
}

#[tokio::test(flavor = "current_thread")]
async fn missing_upower_is_not_ready_and_never_notifies() {
    let (home, paths) = home();
    let bus = private_bus(home.path());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let _server = connect_with(
        &bus,
        (NOTIFICATIONS, "/org/freedesktop/Notifications"),
        Notifier {
            seen: seen.clone(),
            failures: Arc::new(Mutex::new(0)),
        },
    )
    .await;
    let (hub, _service) = hub_with_settings(&paths).await;
    let (system, session) = (connect(&bus, None).await, connect(&bus, None).await);
    assert!(!read_sample(&system).await.ready);
    LocalSet::new()
        .run_until(async {
            let task = tokio::task::spawn_local(battery_alerts(
                hub,
                paths,
                system,
                session,
                Timing::default(),
            ));
            quiet().await;
            assert!(seen.lock().unwrap().is_empty());
            task.abort();
        })
        .await;
}
