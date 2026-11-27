// SPDX-License-Identifier: GPL-3.0-only
//! A visible dashboard owns one bounded video child; the UI never loads libmpv.
use std::{
    io::{Read, Write},
    process::{Child, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, PartialEq)]
pub(super) struct Request {
    pub motion: nacre_protocol::wallpaper::WallpaperMotion,
    pub width: u32,
    pub height: u32,
}

type Frame = (u64, Arc<[u8]>);

pub(super) struct Preview {
    pub request: Request,
    frame: Arc<Mutex<Option<Frame>>>,
    /// Set by the reader after it notifies, cleared when the projection takes the frame.
    unread: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
    /// Set by Drop: a child the preview thread starts after that is killed at once.
    stopped: Arc<AtomicBool>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl Preview {
    pub fn start(request: Request, notify: Option<super::Notify>) -> Self {
        let mut preview = Self {
            request,
            frame: Default::default(),
            unread: Default::default(),
            child: Default::default(),
            stopped: Default::default(),
            reader: None,
        };
        // B0423: the child starts on the preview's own thread, not on the dashboard's resume path
        // (Bead's process command forces std's fork path: a page-table copy of the shell, ~16 ms
        // on every open while a video wallpaper plays). A failed attempt is kept until its
        // source/size/policy changes, rather than respawning on every dashboard projection.
        let request = serde_json::json!({
            "path": preview.request.motion.path,
            "file_identity": preview.request.motion.file_identity,
            "width": preview.request.width, "height": preview.request.height,
        });
        let (frame, unread, child, stopped) = (
            preview.frame.clone(),
            preview.unread.clone(),
            preview.child.clone(),
            preview.stopped.clone(),
        );
        preview.reader = std::thread::Builder::new()
            .name("dashboard-video".into())
            .spawn(move || {
                if outlast_hold(&stopped)
                    && let Ok(mut output) = launch(&request, &child, &stopped)
                {
                    pump(&mut output, &frame, &unread, &notify, FRAME_GAP, &stopped);
                }
                stop_child(&child);
                frame.lock().unwrap().take();
                if let Some(notify) = &notify {
                    notify(bead::wayland::UiEvent::Model(Default::default()));
                }
            })
            .ok();
        preview
    }
    pub fn frame(&self) -> Option<Frame> {
        self.unread.store(false, Ordering::Release);
        self.frame.lock().unwrap().clone()
    }
}
/// Start the renderer and send it the request; the caller pumps its frames.
fn launch(
    request: &serde_json::Value,
    slot: &Mutex<Option<Child>>,
    stopped: &AtomicBool,
) -> std::io::Result<std::process::ChildStdout> {
    let mut child = bead::services::process::command(
        std::env::current_exe()?.with_file_name("bead-wallpaper-video"),
    )
    .arg("--preview")
    .stdin(Stdio::piped())
    .stdout(Stdio::piped())
    .stderr(Stdio::null())
    .spawn()?;
    let input = child.stdin.take();
    let output = child.stdout.take();
    {
        // Under the slot's lock, so Drop's stop_child either finds this child or it sees `stopped`.
        let mut slot = slot.lock().unwrap();
        if stopped.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::other("preview dropped"));
        }
        *slot = Some(child);
    }
    input
        .ok_or_else(|| std::io::Error::other("preview stdin"))?
        .write_all(request.to_string().as_bytes())?;
    output.ok_or_else(|| std::io::Error::other("preview stdout"))
}
impl Drop for Preview {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Release);
        stop_child(&self.child);
        if let Some(reader) = self.reader.take() {
            reader.thread().unpark();
            let _ = reader.join();
        }
    }
}
/// The preview is capped at 15 fps (row 83 (b): each frame is a new image the UI decodes and
/// uploads; the sleep also back-pressures the decoder) and wakes the UI only when the previous
/// frame was consumed.
const FRAME_GAP: Duration = Duration::from_millis(66);
fn pump(
    output: &mut impl Read,
    frame: &Mutex<Option<Frame>>,
    unread: &AtomicBool,
    notify: &Option<super::Notify>,
    gap: Duration,
    stopped: &AtomicBool,
) {
    let mut sequence = 0;
    while let Ok(bytes) = read_frame(output) {
        if !outlast_hold(stopped) {
            break;
        }
        let started = Instant::now();
        sequence += 1;
        *frame.lock().unwrap() = Some((sequence, bytes));
        if let Some(notify) = notify
            && !unread.swap(true, Ordering::AcqRel)
        {
            notify(bead::wayland::UiEvent::Model(Default::default()));
        }
        if !park_until(started + gap, stopped) {
            break;
        }
    }
}
/// Row 83 (b): no new frame (and no child start) while a panel slides, a wallpaper fades or a
/// workspace switches, the same hold the desktop video takes. False once the preview is dropped.
fn outlast_hold(stopped: &AtomicBool) -> bool {
    while let Some(until) = crate::wallpaper_motion::held_until(Instant::now()) {
        if !park_until(until, stopped) {
            return false;
        }
    }
    !stopped.load(Ordering::Acquire)
}
/// Sleep until `deadline`; Drop unparks it, so closing never waits out a gap or a hold.
fn park_until(deadline: Instant, stopped: &AtomicBool) -> bool {
    while !stopped.load(Ordering::Acquire) {
        match deadline.checked_duration_since(Instant::now()) {
            Some(left) if !left.is_zero() => std::thread::park_timeout(left),
            _ => return true,
        }
    }
    false
}
fn stop_child(slot: &Mutex<Option<Child>>) {
    if let Some(mut child) = slot.lock().unwrap().take() {
        // Exact owned child, including a stalled writer or failed decoder.
        let _ = child.kill();
        let _ = child.wait();
    }
}
fn read_frame(reader: &mut impl Read) -> std::io::Result<Arc<[u8]>> {
    let mut header = [0; 4];
    reader.read_exact(&mut header)?;
    let size = u32::from_le_bytes(header) as usize;
    if size == 0 || size > 8 * 1024 * 1024 {
        return Err(std::io::Error::other("invalid preview frame length"));
    }
    let mut bytes = vec![0; size];
    reader.read_exact(&mut bytes)?;
    Ok(bytes.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    #[test]
    fn closing_dashboard_reaps_its_preview_even_with_a_blocked_reader() {
        let mut child = Command::new("sleep")
            .arg("60")
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let pid = child.id();
        let mut output = child.stdout.take().unwrap();
        let reader = std::thread::spawn(move || {
            let _ = read_frame(&mut output);
        });
        let path = std::env::current_exe().unwrap();
        let motion = nacre_protocol::wallpaper::WallpaperMotion {
            kind: nacre_protocol::wallpaper::MotionKind::Video,
            file_identity: nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
                &std::fs::metadata(&path).unwrap(),
            ),
            path: path.to_string_lossy().into_owned(),
        };
        let dashboard = super::super::Dashboard::new(
            super::super::Page::Today,
            Arc::new(
                bead_test::source::FakeSource::new(super::super::tests::fixture("calendar"))
                    .unwrap(),
            ),
            None,
            true,
        );
        *dashboard.video.borrow_mut() = Some(Preview {
            request: Request {
                motion,
                width: 1,
                height: 1,
            },
            frame: Default::default(),
            unread: Default::default(),
            child: Arc::new(Mutex::new(Some(child))),
            stopped: Default::default(),
            reader: Some(reader),
        });
        let stores = bead::core::Stores::default()
            .reconcile(&std::collections::BTreeMap::from([(
                "panel.open".into(),
                bead::core::Value::Bool(false),
            )]))
            .unwrap();
        dashboard.project_with_stores(&stores);
        assert!(dashboard.video.borrow().is_none());
        assert!(!std::path::Path::new(&format!("/proc/{pid}")).exists());
    }
    #[test]
    fn missing_panel_open_does_not_keep_a_preview() {
        let dashboard = super::super::Dashboard::new(
            super::super::Page::Today,
            Arc::new(
                bead_test::source::FakeSource::new(super::super::tests::fixture("calendar"))
                    .unwrap(),
            ),
            None,
            true,
        );
        *dashboard.video.borrow_mut() = Some(Preview {
            request: Request {
                motion: nacre_protocol::wallpaper::WallpaperMotion {
                    kind: nacre_protocol::wallpaper::MotionKind::Video,
                    file_identity: nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
                        &std::fs::metadata(std::env::current_exe().unwrap()).unwrap(),
                    ),
                    path: String::new(),
                },
                width: 1,
                height: 1,
            },
            frame: Default::default(),
            unread: Default::default(),
            child: Default::default(),
            stopped: Default::default(),
            reader: None,
        });
        dashboard.project_with_stores(&bead::core::Stores::default());
        assert!(dashboard.video.borrow().is_none());
    }
    #[test]
    fn detached_pearl_does_not_keep_a_preview() {
        let dashboard = super::super::Dashboard::new(
            super::super::Page::Today,
            Arc::new(
                bead_test::source::FakeSource::new(super::super::tests::fixture("calendar"))
                    .unwrap(),
            ),
            None,
            true,
        );
        *dashboard.video.borrow_mut() = Some(Preview {
            request: Request {
                motion: nacre_protocol::wallpaper::WallpaperMotion {
                    kind: nacre_protocol::wallpaper::MotionKind::Video,
                    file_identity: nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
                        &std::fs::metadata(std::env::current_exe().unwrap()).unwrap(),
                    ),
                    path: String::new(),
                },
                width: 1,
                height: 1,
            },
            frame: Default::default(),
            unread: Default::default(),
            child: Default::default(),
            stopped: Default::default(),
            reader: None,
        });
        let stores = bead::core::Stores::default()
            .reconcile(&std::collections::BTreeMap::from([
                ("panel.open".into(), bead::core::Value::Bool(true)),
                ("view.detached".into(), bead::core::Value::Bool(true)),
            ]))
            .unwrap();
        dashboard.project_with_stores(&stores);
        assert!(dashboard.video.borrow().is_none());
    }
    #[test]
    fn closing_today_keeps_the_shown_preview_frame_and_changes_nothing_on_screen()
    -> bead_test::Result<()> {
        use bead::core::{Value, source::Source};
        use bead::wayland::{SceneSource, UiEvent};
        use bead_test::source::FakeSource;
        use std::collections::BTreeMap;
        let dir = std::env::temp_dir().join(format!("bead-today-close-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let (poster, shown) = (dir.join("poster.png"), dir.join("frame.png"));
        image::RgbaImage::from_pixel(32, 32, image::Rgba([80, 130, 180, 255])).save(&poster)?;
        image::RgbaImage::from_pixel(4, 4, image::Rgba([200, 40, 40, 255])).save(&shown)?;
        let shown: Arc<[u8]> = std::fs::read(&shown)?.into();
        let identity = nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
            &std::fs::metadata(&poster)?,
        );
        let motion = nacre_protocol::wallpaper::WallpaperMotion {
            kind: nacre_protocol::wallpaper::MotionKind::Video,
            file_identity: identity.clone(),
            path: "/fixture/current.mp4".into(),
        };
        let pair = serde_json::json!({"revision":1,"palette_revision":1,"path":poster,
            "file_identity":identity,"presentation":"crop","motion":motion,
            "palette":{"mode":"dark","colours":{"background":"#000000","onSurface":"#ffffff","primary":"#112233"}}});
        let source =
            |data| -> bead_test::Result<Arc<dyn Source>> { Ok(Arc::new(FakeSource::new(data)?)) };
        let dashboard = super::super::Dashboard::new(
            super::super::Page::Today,
            source(super::super::tests::fixture("calendar"))?,
            None,
            true,
        )
        .with_overview(
            None,
            Some(source(
                serde_json::json!({"available":true,"stale":false,"revision":1,
                "current":pair,"outputs":{"fixture":pair}}),
            )?),
            None,
        )
        .with_wallpaper_settings(Some(source(serde_json::json!({
            "store":"wallpaper-picker","rev":1,"data":{"motionMode":"full"}
        }))?))
        .with_prefs(Some(source(serde_json::json!({}))?))
        .with_video_desktop(Some(crate::desktop::start_services()));
        dashboard.set_raster_scale(1.5);
        // The open hero (940x256 at scale 1.5) shows frame 7 of its live preview.
        *dashboard.video.borrow_mut() = Some(Preview {
            request: Request {
                motion,
                width: 1410,
                height: 384,
            },
            frame: Arc::new(Mutex::new(Some((7, shown.clone())))),
            unread: Default::default(),
            child: Default::default(),
            stopped: Default::default(),
            reader: None,
        });
        let root = super::super::tests::root();
        let mut h = bead_test::Harness::load(
            &root,
            std::path::Path::new("dashboard/today/main.luau"),
            &root.join("palette.json"),
            BTreeMap::new(),
            bead_test::Config {
                width: 1048,
                height: 714,
                scale: 1.5,
            },
        )?;
        h.session
            .stores
            .commit(BTreeMap::from([("panel.open".into(), Value::Bool(true))]))?;
        // As live, the poster that covered loading is decoded and held.
        dashboard.project_with_stores(&h.session.stores);
        let job = dashboard
            .poster_job
            .borrow_mut()
            .as_mut()
            .and_then(|job| job.thread.take());
        job.ok_or("poster job")?
            .join()
            .map_err(|_| "poster thread")?;
        let values = dashboard.project_with_stores(&h.session.stores);
        assert_eq!(
            values["dashboard.wallpaper_image"],
            Value::Text("today-wallpaper:7".into()),
            "the injected preview matches the open hero's request"
        );
        assert!(
            dashboard
                .poster
                .borrow()
                .as_ref()
                .is_some_and(|p| p.3.is_some())
        );
        let mut runtime = crate::reload::Runtime::new(h.session).with_dashboard_view(dashboard)?;
        runtime.services(Box::new(|_| {}));
        let open = runtime.stage(UiEvent::Reload)?.ok_or("open stages")?;
        runtime.session.commit(open.clone());
        let before = runtime.session.stores.values().clone();
        let closing = runtime
            .stage(UiEvent::PanelVisibility(false))?
            .ok_or("close stages")?;
        runtime.session.commit(closing.clone());
        let after = runtime.session.stores.values();
        let changed: Vec<_> = after
            .keys()
            .chain(before.keys())
            .filter(|id| before.get(*id) != after.get(*id))
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .map(|id| format!("{id}: {:?} -> {:?}", before.get(id), after.get(id)))
            .collect();
        let hero = |scene: &bead::render::Scene| {
            scene.primitives.iter().find_map(|p| match p {
                bead::render::Primitive::Image { path, .. }
                    if path.to_string_lossy().contains("today-wallpaper") =>
                {
                    let bytes = scene.asset_data.get(path);
                    Some((path.clone(), bytes.map(|b| (b.len(), **b == *shown))))
                }
                _ => None,
            })
        };
        let json = |scene: &bead::render::Scene| -> Vec<String> {
            let text = |p: &bead::render::Primitive| serde_json::to_string(p).unwrap_or_default();
            scene.primitives.iter().map(text).collect()
        };
        let (was, now) = (json(&open), json(&closing));
        let short = |p: Option<&String>| -> String {
            p.map_or("-".into(), |p| p.chars().take(200).collect())
        };
        let moved: Vec<_> = (0..was.len().max(now.len()))
            .filter(|i| was.get(*i) != now.get(*i))
            .map(|i| format!("#{i}: {} -> {}", short(was.get(i)), short(now.get(i))))
            .collect();
        println!(
            "DASH_CLOSE hero_open={:?} hero_close={:?} primitives={} changed_primitives={} changed_stores={changed:?}",
            hero(&open),
            hero(&closing),
            now.len(),
            moved.len()
        );
        assert_eq!(
            hero(&open),
            Some(("@today-wallpaper/video".into(), Some((shown.len(), true))))
        );
        assert_eq!(
            hero(&closing),
            hero(&open),
            "the close keeps the shown frame"
        );
        assert!(changed.iter().any(|c| c.starts_with("panel.open: ")));
        assert!(
            !changed.iter().any(|c| c.starts_with("dashboard.")),
            "the close changes no dashboard value: {changed:?}"
        );
        assert!(moved.is_empty(), "the close repaints nothing: {moved:#?}");
        // S2 (row 83): the reopen keeps that frame until the new preview's first one; the poster
        // in between is a new image (a structural miss and a hero re-bake on every video open).
        let reopened = runtime.stage(UiEvent::PanelVisibility(true))?;
        let reopened = reopened.unwrap_or_else(|| runtime.session.current_scene());
        runtime.session.commit(reopened.clone());
        assert_eq!(
            runtime.session.stores.get("dashboard.wallpaper_image"),
            Some(&Value::Text("today-wallpaper:7".into())),
            "the reopen keeps the shown frame's image key"
        );
        assert_eq!(
            hero(&reopened),
            hero(&open),
            "the reopen keeps the shown frame"
        );
        drop(runtime);
        std::fs::remove_dir_all(dir)?;
        Ok(())
    }
    #[test]
    fn unconsumed_frames_notify_once() {
        let wakes = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = wakes.clone();
        let notify: Option<super::super::Notify> = Some(Arc::new(move |_| {
            counter.fetch_add(1, Ordering::Relaxed);
        }));
        let (frame, unread) = (Mutex::default(), AtomicBool::new(false));
        let bytes = [[1, 0, 0, 0, 7], [1, 0, 0, 0, 8]].concat();
        pump(
            &mut &bytes[..],
            &frame,
            &unread,
            &notify,
            Duration::ZERO,
            &AtomicBool::new(false),
        );
        assert_eq!(wakes.load(Ordering::Relaxed), 1);
        assert_eq!(frame.lock().unwrap().as_ref().unwrap().0, 2);
        unread.store(false, Ordering::Release);
        pump(
            &mut &bytes[..],
            &frame,
            &unread,
            &notify,
            Duration::ZERO,
            &AtomicBool::new(false),
        );
        assert_eq!(wakes.load(Ordering::Relaxed), 2);
    }
    #[test]
    fn frame_reader_bounds_and_truncation() {
        assert!(read_frame(&mut &0u32.to_le_bytes()[..]).is_err());
        assert!(read_frame(&mut &u32::MAX.to_le_bytes()[..]).is_err());
        assert!(read_frame(&mut &[2, 0, 0, 0, 1][..]).is_err());
        assert_eq!(&*read_frame(&mut &[2, 0, 0, 0, 1, 2][..]).unwrap(), &[1, 2]);
    }
    fn missing_renderer() -> Request {
        let path = std::env::current_exe().unwrap();
        Request {
            motion: nacre_protocol::wallpaper::WallpaperMotion {
                kind: nacre_protocol::wallpaper::MotionKind::Video,
                file_identity: nacre_protocol::wallpaper::WallpaperFileIdentity::from_metadata(
                    &std::fs::metadata(&path).unwrap(),
                ),
                path: path.to_string_lossy().into_owned(),
            },
            width: 1,
            height: 1,
        }
    }
    /// Row 83 (b): a frame that arrives while a slide holds the video waits for the hold to end.
    #[test]
    fn a_held_preview_shows_no_new_frame_until_the_hold_ends() {
        let start = Instant::now();
        crate::wallpaper_motion::hold_for(Duration::from_millis(150));
        let (frame, unread) = (Mutex::default(), AtomicBool::new(false));
        let running = AtomicBool::new(false);
        pump(
            &mut &[1, 0, 0, 0, 7][..],
            &frame,
            &unread,
            &None,
            Duration::ZERO,
            &running,
        );
        assert!(
            start.elapsed() >= Duration::from_millis(150),
            "the frame was published inside the hold"
        );
        assert_eq!(frame.lock().unwrap().as_ref().unwrap().0, 1);
    }
    /// Closing the dashboard mid-slide must not wait out the hold on the UI thread.
    #[test]
    fn dropping_a_held_preview_does_not_wait_out_the_hold() {
        crate::wallpaper_motion::hold_for(Duration::from_millis(300));
        let preview = Preview::start(missing_renderer(), None);
        std::thread::sleep(Duration::from_millis(20));
        let start = Instant::now();
        drop(preview);
        assert!(
            start.elapsed() < Duration::from_millis(150),
            "Drop waited {:?} for the parked preview thread",
            start.elapsed()
        );
    }
    /// B0423: the child starts on the preview's own thread, so the dashboard's resume never pays its
    /// fork (in tests the renderer binary is missing, so the old caller-side spawn failed at once).
    #[test]
    fn the_preview_child_starts_on_the_preview_thread() {
        let preview = Preview::start(missing_renderer(), None);
        assert!(
            preview.reader.is_some(),
            "the spawn runs on the dashboard-video thread, not the caller's"
        );
        drop(preview);
    }
}
