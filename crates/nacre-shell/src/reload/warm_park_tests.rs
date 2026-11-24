// SPDX-License-Identifier: GPL-3.0-only
//! N0231: a closed launcher or dashboard parks in Bead's warm cache and reopens from it,
//! driven through a real panel worker as Bead's panel host drives it.
use super::*;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const SCALE: f32 = 1.5;
const SEARCH_ROW: &str = "Zedfix";

fn shell() -> (PathBuf, PathBuf, bead::render::Palette) {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../shell");
    let palette = root.join("palette.json");
    let colors = bead::render::Palette::read(&palette).unwrap();
    (root, palette, colors)
}

fn texts(scene: &Scene) -> Vec<String> {
    scene
        .primitives
        .iter()
        .filter_map(|primitive| match primitive {
            bead::render::Primitive::Text { text, .. } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

fn shows(scene: &Scene, text: &str) -> bool {
    texts(scene).iter().any(|shown| shown.contains(text))
}

/// A panel worker as Bead's CPU panel host drives it: it stages, presents and acknowledges each
/// transaction, sends missing CPU resources to the worker, and parks and resumes it (warm.rs).
struct Host {
    source: Box<dyn SceneSource>,
    events: mpsc::Receiver<UiEvent>,
    factories: Arc<AtomicUsize>,
    renderer: bead::render::tree::TreeCpuRenderer,
    resource_jobs: usize,
    shown: Option<Scene>,
    resumed: Option<Scene>,
}

impl Host {
    fn new(
        fallback: Scene,
        factory: impl FnOnce() -> Result<Runtime, Box<dyn Error>> + Send + 'static,
    ) -> Self {
        let factories = Arc::new(AtomicUsize::new(0));
        let calls = factories.clone();
        let mut source: Box<dyn SceneSource> = Box::new(bead::wayland::worker::Source::new(
            Box::new(move || {
                calls.fetch_add(1, Ordering::SeqCst);
                factory()
                    .map(|runtime| Box::new(runtime) as Box<dyn SceneSource>)
                    .map_err(|error| error.to_string())
            }),
            fallback,
        ));
        let (sender, events) = mpsc::channel();
        source.services(Box::new(move |event| {
            let _ = sender.send(event);
        }));
        source.viewport(1920, 1200);
        source.scales(vec![SCALE]);
        let mut host = Self {
            source,
            events,
            factories,
            renderer: Default::default(),
            resource_jobs: 0,
            shown: None,
            resumed: None,
        };
        host.stage(UiEvent::Reload);
        host
    }

    fn stage(&mut self, event: UiEvent) {
        self.forward(event);
        self.settle();
    }

    fn forward(&mut self, event: UiEvent) {
        let (accepted, resumed, failed) = match &event {
            UiEvent::Worker(packet) => (packet.is_accepted(), packet.resumed(), packet.has_error()),
            _ => (false, false, false),
        };
        let prepared = matches!(&event, UiEvent::ResourcesPrepared(_));
        let Some(scene) = self.source.stage(event).expect("panel transaction") else {
            return;
        };
        // The real presenter retries its frame once resources are prepared; this host does not.
        if prepared {
            return;
        }
        assert!(!failed, "the view failed: {:?}", texts(&scene));
        if !accepted {
            self.present(&scene);
        }
        // Ready: the Ack, behind any resource job the frame sent. Accepted: the commit.
        self.source.commit();
        if accepted {
            if resumed {
                self.resumed = Some(scene.clone());
            }
            self.shown = Some(scene);
        }
    }

    /// The CPU presenter's first attempt at a Ready frame: an opening slide plans its layer, then
    /// the frame prepares. Missing resources become one PrepareCpu/PrepareImages job on the
    /// worker, as rendering.rs sends them (B0392 lets such a worker park).
    fn present(&mut self, scene: &Scene) {
        use bead::render::Renderer;
        let size = |length: f32| (length * SCALE).ceil().max(1.) as u32;
        let presented = if scene.motion.as_ref().is_some_and(|motion| motion.open) {
            bead::render::layer_motion::LayerMotionPlan::for_slide(
                scene,
                true,
                size(scene.bounds.w),
                size(scene.bounds.h),
                SCALE,
                bead::render::PixelFormat::Rgba,
                &mut self.renderer,
            )
            .map(|_| ())
        } else {
            Ok(())
        }
        .and_then(|()| self.renderer.prepare_on_ui(scene, SCALE));
        let Err(error) = presented else {
            return;
        };
        if let Some(pending) = error.downcast_ref::<bead::render::tree::CpuResourcesPending>() {
            self.source.prepare_cpu(pending.request.clone()).unwrap();
        } else if let Some(pending) = error.downcast_ref::<bead::render::tree::ResourcesPending>() {
            self.source.prepare_images(pending.images.clone()).unwrap();
        } else {
            panic!("CPU presentation failed: {error}");
        }
        self.resource_jobs += 1;
    }

    /// Forward worker packets, prepared resources and service hints until the worker is dormant.
    fn settle(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            match self.events.recv_timeout(Duration::from_millis(250)) {
                Ok(
                    event
                    @ (UiEvent::Worker(_) | UiEvent::ResourcesPrepared(_) | UiEvent::Model(_)),
                ) => self.forward(event),
                Ok(_) => {}
                Err(mpsc::RecvTimeoutError::Timeout) if self.source.dormant_ready() => return,
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    assert!(
                        Instant::now() < deadline,
                        "the panel worker never settled ({} resource jobs)",
                        self.resource_jobs
                    )
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => panic!("the panel worker stopped"),
            }
        }
    }

    /// Bead parks a settled closed worker that declares a bound within its budget; parked, it
    /// keeps payload-free model hints and evicts on any other event. Returns its admission charge.
    fn close_and_park(&mut self) -> usize {
        self.stage(UiEvent::PanelVisibility(false));
        assert!(self.source.dormant_ready(), "a settled close is dormant");
        let charge = self
            .source
            .warm_retained_bytes()
            .expect("the view opts in to warm parking");
        // Parking tears down the surfaces, their renderer and the host's copies of the scene.
        self.renderer = Default::default();
        self.shown = None;
        let quiet = Instant::now() + Duration::from_millis(500);
        while let Ok(event) = self
            .events
            .recv_timeout(quiet.saturating_duration_since(Instant::now()))
        {
            assert!(
                matches!(&event, UiEvent::Model(values) if values.is_empty()),
                "a parked panel got an event that evicts it: {event:?}"
            );
        }
        charge
    }

    /// Bead's reopen of a parked view: its visibility first, then the resume, whose frame shows.
    fn reopen(&mut self) -> Scene {
        self.resumed = None;
        self.forward(UiEvent::PanelVisibility(true));
        assert!(
            self.source.resume().expect("resume job").is_none(),
            "cached pixels wait for the resumed transaction"
        );
        self.settle();
        self.resumed.clone().expect("the resume was accepted")
    }
}

/// What the bar's trigger opens the launcher with (`nacre launcher QUERY`).
fn launch(query: &str) -> BTreeMap<String, bead::core::Value> {
    crate::key_routes::Launch {
        query: query.into(),
        route: None,
        toggle: false,
        target_view: None,
    }
    .model()
}

/// The launcher Runtime as launcher_panel::spec builds it, on a fixture menu.
fn launcher(memory: crate::launcher::Memory) -> Host {
    let (root, palette, colors) = shell();
    let fallback = crate::launcher_panel::spec(
        colors.get("surface").unwrap(),
        palette.clone(),
        colors.clone(),
        Default::default(),
        false,
        memory.clone(),
        None,
    )
    .fallback;
    let cold = fallback.clone();
    Host::new(fallback, move || {
        let row = |id: &str, name: &str| serde_json::json!({"id": id, "name": name});
        let menu = crate::launcher::Fixture::new(serde_json::json!({"queries": {
            "": crate::launcher::fixture_reply("", vec![row("home-app", "Homely")]),
            "zzz": crate::launcher::fixture_reply("zzz", vec![row("search-app", SEARCH_ROW)]),
        }}))
        .map_err(|error| error.to_string())?;
        let session =
            bead::ui::Session::new(root, "ui/main.luau".into(), palette, colors.clone(), cold);
        Runtime::panel_session(session, true)
            .with_frame(
                crate::placement::Attachment::Attached(bead::core::Edge::Bottom),
                &colors,
            )?
            .with_launcher(Some(Arc::new(menu)))?
            .with_launcher_quick(crate::launcher_quick::Quick::new(None, Vec::new()))?
            .with_launcher_memory(memory)
    })
}

/// Today as dashboard::spec builds it; each page lease reads the fixtures, `updates` its count.
fn dashboard(updates: Arc<AtomicU64>) -> Host {
    use crate::dashboard::{Choice, Dashboard, Factory, Page};
    let (root, palette, colors) = shell();
    let choice = Choice {
        client: None,
        fixtures: None,
        prefs: None,
    };
    let fallback = crate::dashboard::spec(
        Page::Today,
        Arc::new(palette.clone()),
        Arc::new(colors.clone()),
        choice,
        Default::default(),
        false,
    )
    .unwrap()
    .fallback;
    let cold = fallback.clone();
    Host::new(fallback, move || {
        let fixtures = root.join("tests/fixtures/dashboard");
        let pages: Factory = Arc::new(move |page| {
            let lease = |name: &str| -> Result<Arc<dyn bead::core::source::Source>, String> {
                let bytes = std::fs::read(fixtures.join(format!("{name}.json")))
                    .map_err(|error| error.to_string())?;
                let mut data: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
                if name == "updates" {
                    data["count"] = updates.load(Ordering::SeqCst).into();
                }
                let source =
                    bead_test::source::FakeSource::new(data).map_err(|error| error.to_string())?;
                Ok(Arc::new(source))
            };
            Ok(
                Dashboard::new(page, lease("calendar")?, Some(lease("weather")?), true)
                    .with_overview(Some(lease("system")?), None, Some(lease("updates")?)),
            )
        });
        let initial = pages(Page::Today)?;
        let session = bead::ui::Session::new(
            root,
            "dashboard/today/main.luau".into(),
            palette,
            colors.clone(),
            cold,
        );
        Runtime::panel_session(session, true)
            .with_frame(
                crate::placement::Attachment::Attached(bead::core::Edge::Top),
                &colors,
            )?
            .with_dashboard_factory(initial, pages)
    })
}

#[test]
fn only_the_launcher_and_the_dashboard_on_today_opt_in_to_warm_parking() {
    use crate::dashboard::{Dashboard, Page};
    let (root, palette, colors) = shell();
    let fallback = crate::sound::fallback(colors.get("surface").unwrap());
    let runtime = || {
        let session = bead::ui::Session::new(
            root.clone(),
            "missing.luau".into(),
            palette.clone(),
            colors.clone(),
            fallback.clone(),
        );
        Runtime::new(session)
    };
    assert_eq!(runtime().warm_retained_bytes(), None);
    for (page, bound) in [(Page::Today, Some(WARM_PANEL_BYTES)), (Page::Media, None)] {
        let source = bead_test::source::FakeSource::new(serde_json::json!({})).unwrap();
        let view = Dashboard::new(page, Arc::new(source), None, true);
        let runtime = runtime().with_dashboard_view(view).unwrap();
        assert_eq!(runtime.warm_retained_bytes(), bound, "{}", page.name());
    }
}

#[test]
fn a_parked_launcher_reopens_fresh_without_a_factory_call() {
    crate::i18n::init();
    let memory = crate::launcher::Memory::default();
    *memory.lock().unwrap() = launch("");
    let mut host = launcher(memory.clone());
    // A search, as typing or `nacre launcher zzz` leaves it.
    host.stage(UiEvent::Model(launch("zzz")));
    let searched = host.shown.clone().expect("a searched frame");
    assert!(shows(&searched, SEARCH_ROW), "{:?}", texts(&searched));
    host.close_and_park();
    // The bar's trigger writes the open's state before it shows the view (open_launcher).
    *memory.lock().unwrap() = launch("");
    let first = host.reopen();
    let state = host.source.saved_state();
    assert_eq!(state["launcher.search.text"], "", "{state}");
    assert_ne!(state["launcher.selection"], "search-app", "{state}");
    assert!(
        !shows(&first, SEARCH_ROW),
        "no stale row on the first reopened frame: {:?}",
        texts(&first)
    );
    assert_eq!(host.factories.load(Ordering::SeqCst), 1, "no factory call");
    eprintln!("launcher resource_jobs={}", host.resource_jobs);
}

#[test]
fn a_parked_dashboard_reopens_on_fresh_data_without_a_factory_call() {
    crate::i18n::init();
    let count = |n| crate::i18n::tr_n("bead.dashboard.updates-count", n, &[]);
    let updates = Arc::new(AtomicU64::new(12));
    let mut host = dashboard(updates.clone());
    let opened = host.shown.clone().expect("an open frame");
    assert!(shows(&opened, &count(12)), "{:?}", texts(&opened));
    host.close_and_park();
    // The parked page holds no daemon lease; the daemon's data moves on meanwhile.
    updates.store(3, Ordering::SeqCst);
    let first = host.reopen();
    assert!(
        shows(&first, &count(3)) && !shows(&first, &count(12)),
        "the first reopened frame is projected from fresh leases: {:?}",
        texts(&first)
    );
    assert_eq!(host.factories.load(Ordering::SeqCst), 1, "no factory call");
    eprintln!("dashboard resource_jobs={}", host.resource_jobs);
}

/// In-use heap (glibc arenas and mmapped chunks) and Bead's charged cache bytes.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn heap() -> (usize, usize) {
    // SAFETY: allocator trim and by-value accounting only, in an isolated child process.
    let info = unsafe {
        nix::libc::malloc_trim(0);
        nix::libc::mallinfo2()
    };
    (
        info.uordblks + info.hblkhd,
        bead::core::cache::report().used_bytes,
    )
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
fn parked_panels_keep_less_than_their_charge_and_both_fit_the_budget() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "reload::warm_park_tests::parked_panel_memory_child",
            "--ignored",
            "--nocapture",
        ])
        .env("NACRE_WARM_PARK_CHILD", "1")
        .output()
        .unwrap();
    // Shown in passing runs too: libtest captures print!, not the stderr handle.
    use std::io::Write;
    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in stdout
        .lines()
        .filter_map(|line| line.find("WARM_").map(|at| &line[at..]))
    {
        let _ = writeln!(std::io::stderr(), "{line}");
    }
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[cfg(all(target_os = "linux", target_env = "gnu"))]
#[test]
#[ignore = "private child of the parked-panel memory test"]
fn parked_panel_memory_child() {
    assert_eq!(std::env::var("NACRE_WARM_PARK_CHILD").as_deref(), Ok("1"));
    crate::i18n::init();
    let mib = |bytes: usize| bytes as f64 / bead::core::cache::MIB as f64;
    let mut charges = 0;
    for view in ["launcher", "dashboard-today"] {
        let mut host = if view == "launcher" {
            launcher(Arc::new(std::sync::Mutex::new(launch(""))))
        } else {
            dashboard(Arc::new(AtomicU64::new(12)))
        };
        let charge = host.close_and_park();
        let jobs = host.resource_jobs;
        let parked = heap();
        // Dropping the parked worker is the cold retire; its reaper frees the session.
        drop(host);
        std::thread::sleep(Duration::from_secs(2));
        let released = heap();
        let kept = parked.0.saturating_sub(released.0);
        println!(
            "WARM_PARKED view={view} kept_heap_mib={:.2} kept_cache_mib={:.2} charge_mib={:.2} bound_mib={:.0} resource_jobs={jobs}",
            mib(kept),
            mib(parked.1.saturating_sub(released.1)),
            mib(charge),
            mib(WARM_PANEL_BYTES),
        );
        // The charge less the worker's 2 MiB minimum stack is the heap it may hold.
        assert!(
            kept <= charge - 2 * bead::core::cache::MIB,
            "{view} keeps {:.2} MiB, more than its charge allows: raise WARM_PANEL_BYTES",
            mib(kept)
        );
        charges += charge;
    }
    println!(
        "WARM_BUDGET charges_mib={:.2} budget_mib={:.0}",
        mib(charges),
        mib(WARM_SOURCE_BUDGET)
    );
    assert!(
        charges + 4096 <= WARM_SOURCE_BUDGET,
        "the launcher and the dashboard must park together: raise WARM_SOURCE_BUDGET"
    );
}
