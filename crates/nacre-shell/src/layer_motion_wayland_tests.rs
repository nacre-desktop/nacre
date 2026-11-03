// SPDX-License-Identifier: GPL-3.0-only
//! Wire-level Bead motion test through the production prompt panel factory.
//! The minimal fake-compositor dispatch structure follows Bead's private MIT wire
//! harness at commit e44930898b223c4abe679455bb5e9c0c2f85198c; see the fixture
//! README and bundled MIT license for protocol/support attribution.
use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    fs,
    os::unix::{fs::PermissionsExt, net::UnixListener},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use bead::wayland::UiEvent;
use wayland_protocols::wp::viewporter::server::{
    wp_viewport::WpViewport,
    wp_viewporter::{self, WpViewporter},
};
use wayland_protocols::xdg::xdg_output::zv1::server::{
    zxdg_output_manager_v1::{self, ZxdgOutputManagerV1},
    zxdg_output_v1::{self, ZxdgOutputV1},
};
use wayland_protocols_wlr::layer_shell::v1::server::{
    zwlr_layer_shell_v1::{self, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, ZwlrLayerSurfaceV1},
};
use wayland_server::{
    Client, DataInit, Dispatch, Display, DisplayHandle, GlobalDispatch, New, Resource,
    backend::{ClientData as WaylandClientData, DisconnectReason},
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_output, wl_region, wl_shm, wl_shm_pool,
        wl_surface,
    },
};

mod motion_protocol {
    use wayland_server;
    use wayland_server::protocol::*;
    pub mod __interfaces {
        use wayland_server::backend as wayland_backend;
        use wayland_server::protocol::__interfaces::*;
        wayland_scanner::generate_interfaces!(
            "tests/fixtures/bead-layer-motion/bead-layer-motion-v1.xml"
        );
    }
    use self::__interfaces::*;
    wayland_scanner::generate_server_code!(
        "tests/fixtures/bead-layer-motion/bead-layer-motion-v1.xml"
    );
}

use motion_protocol::bead_layer_motion_manager_v1::BeadLayerMotionManagerV1;
use motion_protocol::bead_layer_motion_v1::{BeadLayerMotionV1, Request as MotionRequest};

const OUTPUT_WIDTH: i32 = 1920;
const OUTPUT_HEIGHT: i32 = 1200;
const FRAME_PERIOD: Duration = Duration::from_nanos(8_333_333);
const CHILD_RUNTIME: Duration = Duration::from_millis(1_400);
const HARNESS_TIMEOUT: Duration = Duration::from_secs(8);
const IDENT: &str = "0123456789abcdef0123456789abcdef";
const PROMPT_NAMESPACE: &str = "nacre-prompt-layer-motion-wire-test";

#[derive(Debug, Default)]
struct Counts {
    panel_attaches: usize,
    dismiss_attaches: usize,
    prompt_controllers: usize,
    prompt_controllers_at_done: Option<usize>,
    animate_requests: usize,
    completions: usize,
    first_panel_commit_armed: Option<bool>,
    other_attaches: usize,
    wire_events: Vec<String>,
    prompt_attaches_at_done: Option<usize>,
    prompt_set_sizes: Vec<(u32, u32)>,
    prompt_set_sizes_at_done: Option<usize>,
    prompt_configures: Vec<(u32, u32)>,
    prompt_configures_at_done: Option<usize>,
    prompt_buffer_sizes: Vec<(i32, i32)>,
    prompt_buffer_sizes_at_done: Option<usize>,
    output_resize_sent: bool,
}

#[derive(Clone, Copy)]
struct PlaneState {
    id: u32,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    clip_x: f64,
    clip_y: f64,
    clip_width: f64,
    clip_height: f64,
    opacity: f64,
}

struct ControllerData {
    surface_id: wayland_server::backend::ObjectId,
}

struct PendingMotion {
    resource: BeadLayerMotionV1,
    surface_id: wayland_server::backend::ObjectId,
    serial: u32,
    duration: Duration,
}

struct ActiveMotion {
    resource: BeadLayerMotionV1,
    surface_id: wayland_server::backend::ObjectId,
    serial: u32,
    ends: Instant,
    planes: Vec<PlaneState>,
}

#[derive(Default)]
struct SurfaceData {
    buffer: Option<wl_buffer::WlBuffer>,
    callbacks: Vec<wl_callback::WlCallback>,
}

struct ShmBufferData {
    width: i32,
    height: i32,
}

struct LayerRole {
    namespace: String,
    resource: ZwlrLayerSurfaceV1,
    requested_size: (u32, u32),
    configured_size: Option<(u32, u32)>,
}

struct SurfaceRecord {
    surface: wl_surface::WlSurface,
    data: Arc<Mutex<SurfaceData>>,
}

struct Compositor {
    counts: Counts,
    start: Instant,
    next_serial: u32,
    surfaces: Vec<SurfaceRecord>,
    layers: HashMap<wayland_server::backend::ObjectId, LayerRole>,
    xdg_outputs: Vec<(wl_output::WlOutput, ZxdgOutputV1)>,
    planes: HashMap<wayland_server::backend::ObjectId, BTreeMap<u32, PlaneState>>,
    pending_motion: Option<PendingMotion>,
    active_motion: Option<ActiveMotion>,
    resize_after_motion_done: bool,
}

#[derive(Debug)]
struct PeerData;
impl WaylandClientData for PeerData {
    fn disconnected(&self, _: wayland_server::backend::ClientId, _: DisconnectReason) {}
}

struct PrivateDir(PathBuf);
impl PrivateDir {
    fn create(label: &str) -> std::io::Result<Self> {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "nacre-prompt-motion-{label}-{}-{nonce}",
            std::process::id()
        ));
        fs::create_dir(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        Ok(Self(path))
    }
}
impl Drop for PrivateDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ChildGuard(Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}

fn make_prompt_spec(helper: PathBuf) -> Result<bead::wayland::PanelSpec, Box<dyn Error>> {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let palette = workspace.join("shell/palette.json");
    let colors = bead::render::Palette::read(&palette)?;
    let expires = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64() + 60.;
    crate::prompt::panel(
        palette,
        colors,
        helper,
        IDENT.into(),
        crate::prompt::Request {
            input: false,
            title: "Private compositor request".into(),
            initial: String::new(),
            choices: vec![crate::prompt::Choice {
                label: "Continue".into(),
                value: "continue".into(),
                description: String::new(),
                icon: "chevron_right".into(),
                disabled: false,
            }],
            expires,
        },
        false,
    )
}

fn run_prompt_client() -> Result<(), Box<dyn Error>> {
    let helper =
        PathBuf::from(std::env::var_os("NACRE_TEST_HELPER").ok_or("missing private helper")?);
    let spec = make_prompt_spec(helper)?;
    let factory = spec.factory.clone();
    let fallback = spec.fallback.clone();
    let panels =
        bead::wayland::Panels::new("prompt".into(), BTreeMap::from([("prompt".into(), spec)]))?;
    let options = bead::wayland::Options {
        namespace: PROMPT_NAMESPACE.into(),
        panels: Some(panels),
        app: Some(vec![serde_json::Value::String(IDENT.into())]),
        panel_output: Some("NACRE-TEST-0".into()),
        seconds: Some(CHILD_RUNTIME.as_secs_f64()),
        renderer: bead::render::RendererKind::Cpu,
        interactive: true,
        keyboard_focus: true,
        ..bead::wayland::Options::default()
    };
    let renderer = bead::render::renderer(bead::render::RendererKind::Cpu)?;
    bead::wayland::run_runtime(options, fallback, renderer, None)?;

    // Verify the same production factory after the cold wire arm has completed;
    // this must not prewarm the scene or raster cache before run_runtime starts.
    let mut probe =
        factory(true).unwrap_or_else(|error| panic!("production prompt factory failed: {error}"));
    probe.viewport(OUTPUT_WIDTH as u32, OUTPUT_HEIGHT as u32);
    let prepared = probe
        .stage(UiEvent::Reload)?
        .ok_or("prompt factory produced no scene")?;
    let motion = prepared
        .motion
        .as_ref()
        .ok_or("prompt scene has no motion")?;
    assert!(
        motion.open,
        "production prompt factory must produce an open scene"
    );
    assert!(
        !motion.reveal,
        "production prompt opening must use the plain-motion plan"
    );
    probe.commit();
    Ok(())
}

#[test]
fn layer_motion_client_child_entry() {
    if std::env::var_os("NACRE_LAYER_MOTION_CHILD").is_none() {
        return;
    }
    run_prompt_client().expect("production prompt client completes against private compositor");
}

#[test]
fn real_prompt_wire_sequence_and_stock_compositor_fallback() {
    run_wire_case(true, false).expect("advertised motion wire path");
    run_wire_case(true, true).expect("advertised motion output-resize path");
    run_wire_case(false, false).expect("stock Wayland fallback path");
}

fn run_wire_case(
    advertise_motion: bool,
    resize_after_motion_done: bool,
) -> Result<(), Box<dyn Error>> {
    let label = match (advertise_motion, resize_after_motion_done) {
        (true, true) => "motion-resize",
        (true, false) => "motion",
        (false, false) => "fallback",
        (false, true) => unreachable!("fallback does not resize the output"),
    };
    let private = PrivateDir::create(label)?;
    let home = private.0.join("home");
    let runtime_dir = private.0.join("runtime");
    for path in [
        &home,
        &runtime_dir,
        &private.0.join("cache"),
        &private.0.join("config"),
        &private.0.join("data"),
        &private.0.join("state"),
    ] {
        fs::create_dir_all(path)?;
    }
    fs::set_permissions(&runtime_dir, fs::Permissions::from_mode(0o700))?;
    let helper = private.0.join("fake-nacre-helper");
    fs::write(
        &helper,
        "#!/bin/sh\n[ \"${1:-}\" = menu-cli ] && [ \"${2:-}\" = respond ] || exit 2\ncat >/dev/null\nexit 0\n",
    )?;
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700))?;

    let socket = runtime_dir.join("wayland-0");
    let listener = UnixListener::bind(&socket)?;
    listener.set_nonblocking(true)?;
    let exe = std::env::current_exe()?;
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let stdout_path = private.0.join("client.stdout");
    let stderr_path = private.0.join("client.stderr");
    let child = Command::new(exe)
        .arg("--exact")
        .arg("layer_motion_wayland_tests::layer_motion_client_child_entry")
        .arg("--nocapture")
        .current_dir(root)
        .env("NACRE_LAYER_MOTION_CHILD", "1")
        .env("NACRE_TEST_HELPER", &helper)
        .env("HOME", &home)
        .env("XDG_RUNTIME_DIR", &runtime_dir)
        .env("XDG_CACHE_HOME", private.0.join("cache"))
        .env("XDG_CONFIG_HOME", private.0.join("config"))
        .env("XDG_DATA_HOME", private.0.join("data"))
        .env("XDG_STATE_HOME", private.0.join("state"))
        .env("WAYLAND_DISPLAY", "wayland-0")
        .env("RUST_TEST_THREADS", "1")
        .env_remove("WAYLAND_SOCKET")
        .env_remove("DISPLAY")
        .env_remove("DBUS_SESSION_BUS_ADDRESS")
        .stdout(Stdio::from(fs::File::create(&stdout_path)?))
        .stderr(Stdio::from(fs::File::create(&stderr_path)?))
        .spawn()?;
    let mut child = ChildGuard(child);
    let deadline = Instant::now() + HARNESS_TIMEOUT;
    let stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if let Some(status) = child.0.try_wait()? {
                    return Err(child_failure(label, status, &stdout_path, &stderr_path).into());
                }
                if Instant::now() >= deadline {
                    return Err(format!(
                        "{label}: Wayland child did not connect\n{}",
                        read_logs(&stdout_path, &stderr_path)
                    )
                    .into());
                }
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => return Err(error.into()),
        }
    };

    let mut display = Display::<Compositor>::new()?;
    let mut handle = display.handle();
    let mut compositor = Compositor::new(resize_after_motion_done);
    handle.create_global::<Compositor, wl_compositor::WlCompositor, _>(6, ());
    handle.create_global::<Compositor, wl_shm::WlShm, _>(1, ());
    handle.create_global::<Compositor, wl_output::WlOutput, _>(4, ());
    handle.create_global::<Compositor, ZwlrLayerShellV1, _>(4, ());
    handle.create_global::<Compositor, WpViewporter, _>(1, ());
    handle.create_global::<Compositor, ZxdgOutputManagerV1, _>(3, ());
    if advertise_motion {
        handle.create_global::<Compositor, BeadLayerMotionManagerV1, _>(1, ());
    }
    handle.insert_client(stream, Arc::new(PeerData))?;

    let mut next_frame = Instant::now() + FRAME_PERIOD;
    loop {
        display.dispatch_clients(&mut compositor)?;
        let now = Instant::now();
        if now >= next_frame {
            compositor.vblank(now);
            next_frame += FRAME_PERIOD;
        }
        display.flush_clients()?;
        if let Some(status) = child.0.try_wait()? {
            if !status.success() {
                return Err(child_failure(label, status, &stdout_path, &stderr_path).into());
            }
            break;
        }
        if now >= deadline {
            return Err(format!(
                "{label}: Wayland child exceeded {HARNESS_TIMEOUT:?}\n{}",
                read_logs(&stdout_path, &stderr_path)
            )
            .into());
        }
        thread::sleep(Duration::from_millis(1));
    }
    let counts = compositor.counts;
    eprintln!(
        "{label}: compositor counts: {counts:#?}\n{}",
        read_logs(&stdout_path, &stderr_path)
    );
    assert!(
        counts.dismiss_attaches > 0,
        "production prompt keeps its separate dismiss-buffer attachment"
    );
    assert_eq!(
        counts.other_attaches, 0,
        "only the prompt and its dismiss surface attach buffers"
    );
    if advertise_motion {
        let attaches_at_done = counts
            .prompt_attaches_at_done
            .expect("prompt motion completed");
        assert_eq!(
            attaches_at_done, 1,
            "one prompt atlas attachment for the slide"
        );
        if resize_after_motion_done {
            assert!(
                counts.output_resize_sent,
                "mock sent the logical output resize"
            );
            let set_sizes_at_done = counts.prompt_set_sizes_at_done.unwrap();
            let configures_at_done = counts.prompt_configures_at_done.unwrap();
            let buffers_at_done = counts.prompt_buffer_sizes_at_done.unwrap();
            let initial_size = *counts
                .prompt_set_sizes
                .first()
                .expect("initial prompt size");
            let resized_size = *counts
                .prompt_set_sizes
                .last()
                .expect("prompt size after logical output resize");
            let initial_configure = *counts
                .prompt_configures
                .first()
                .expect("initial prompt configure");
            let resized_configure = *counts
                .prompt_configures
                .last()
                .expect("prompt configure after logical output resize");
            assert!(
                counts.prompt_set_sizes.len() > set_sizes_at_done,
                "output resize causes a new prompt size request"
            );
            assert_ne!(
                resized_size, initial_size,
                "output resize changes the prompt layout size"
            );
            assert!(
                counts.prompt_configures.len() > configures_at_done,
                "output resize receives a prompt configure"
            );
            assert_ne!(
                resized_configure, initial_configure,
                "output resize changes the prompt configure"
            );
            assert!(
                counts.panel_attaches > attaches_at_done,
                "a replacement prompt buffer follows the viewport resize"
            );
            // The output resize permits one paint after its manual Configure, one
            // after the worker applies Relayout, and one after the final requested
            // allocation Configure. More commits would be animation-tick redraws.
            assert!(
                counts.panel_attaches <= attaches_at_done + 3,
                "output resize paints only its allocation, layout, and final-size updates"
            );
            assert!(
                counts.prompt_buffer_sizes.len() > buffers_at_done,
                "output resize commits a new prompt buffer"
            );
            assert_eq!(
                counts.prompt_buffer_sizes.last(),
                Some(&(resized_configure.0 as i32, resized_configure.1 as i32)),
                "replacement buffer matches the resized prompt allocation"
            );
        } else {
            assert_eq!(
                counts.panel_attaches, 1,
                "unchanged viewport uses one prompt atlas attachment"
            );
        }
        assert_eq!(
            counts.animate_requests, 1,
            "one compositor animation request"
        );
        assert_eq!(
            counts.first_panel_commit_armed,
            Some(true),
            "animation is armed by the first prompt buffer commit"
        );
        assert_eq!(
            counts.completions, 1,
            "compositor sends one motion completion"
        );
        let at_done = counts.prompt_controllers_at_done.unwrap();
        assert!(
            counts.prompt_controllers > at_done,
            "client receives completion and restores its reusable controller"
        );
    } else {
        assert_eq!(
            counts.animate_requests, 0,
            "stock compositor does not receive motion requests"
        );
        assert!(
            counts.panel_attaches > 1,
            "stock compositor retains the repaint fallback"
        );
    }
    let stderr = fs::read_to_string(&stderr_path)?;
    assert!(
        !stderr.contains("UI_ERROR"),
        "{label}: production prompt runtime logged a UI error: {stderr}"
    );
    Ok(())
}

fn read_logs(stdout: &Path, stderr: &Path) -> String {
    format!(
        "child stdout:\n{}\nchild stderr:\n{}",
        fs::read_to_string(stdout).unwrap_or_default(),
        fs::read_to_string(stderr).unwrap_or_default()
    )
}

fn child_failure(label: &str, status: ExitStatus, stdout: &Path, stderr: &Path) -> String {
    format!(
        "{label}: child exited {status}\n{}",
        read_logs(stdout, stderr)
    )
}

impl Compositor {
    fn new(resize_after_motion_done: bool) -> Self {
        Self {
            counts: Counts::default(),
            start: Instant::now(),
            next_serial: 1,
            surfaces: Vec::new(),
            layers: HashMap::new(),
            xdg_outputs: Vec::new(),
            planes: HashMap::new(),
            pending_motion: None,
            active_motion: None,
            resize_after_motion_done,
        }
    }
    fn serial(&mut self) -> u32 {
        let serial = self.next_serial;
        self.next_serial = self.next_serial.wrapping_add(1).max(1);
        serial
    }
    fn vblank(&mut self, now: Instant) {
        let timestamp = self.start.elapsed().as_millis() as u32;
        for surface in &self.surfaces {
            let callbacks =
                std::mem::take(&mut surface.data.lock().expect("surface lock").callbacks);
            for callback in callbacks {
                callback.done(timestamp);
            }
        }
        if self
            .active_motion
            .as_ref()
            .is_some_and(|motion| now >= motion.ends)
        {
            let motion = self.active_motion.take().expect("checked active motion");
            if !self
                .surfaces
                .iter()
                .any(|surface| surface.surface.id() == motion.surface_id)
            {
                return;
            }
            for plane in motion.planes {
                motion.resource.plane_state(
                    plane.id,
                    plane.x,
                    plane.y,
                    plane.width,
                    plane.height,
                    plane.clip_x,
                    plane.clip_y,
                    plane.clip_width,
                    plane.clip_height,
                    plane.opacity,
                );
            }
            if self
                .layers
                .get(&motion.surface_id)
                .is_some_and(|role| role.namespace == PROMPT_NAMESPACE)
            {
                self.counts.prompt_controllers_at_done = Some(self.counts.prompt_controllers);
                self.counts.prompt_attaches_at_done = Some(self.counts.panel_attaches);
                self.counts.prompt_set_sizes_at_done = Some(self.counts.prompt_set_sizes.len());
                self.counts.prompt_configures_at_done = Some(self.counts.prompt_configures.len());
                self.counts.prompt_buffer_sizes_at_done =
                    Some(self.counts.prompt_buffer_sizes.len());
            }
            self.counts.wire_events.push(format!(
                "{:.3}ms motion_done serial={} surface={:?}",
                self.start.elapsed().as_secs_f64() * 1000.,
                motion.serial,
                motion.surface_id,
            ));
            motion.resource.done(motion.serial);
            self.counts.completions += 1;
            if self.resize_after_motion_done && !self.counts.output_resize_sent {
                for (output, xdg_output) in &self.xdg_outputs {
                    xdg_output.logical_size(960, 900);
                    output.mode(
                        wl_output::Mode::Preferred,
                        OUTPUT_WIDTH,
                        OUTPUT_HEIGHT,
                        120_000,
                    );
                    output.mode(wl_output::Mode::Current, 960, 900, 120_000);
                    output.done();
                }
                self.counts.output_resize_sent = true;
                self.counts.wire_events.push(format!(
                    "{:.3}ms output_logical_size_changed 1920x1200->960x900",
                    self.start.elapsed().as_secs_f64() * 1000.,
                ));
                let prompt_configure = self.layers.iter().find_map(|(surface_id, role)| {
                    (role.namespace == PROMPT_NAMESPACE).then(|| {
                        let requested_width = if role.requested_size.0 == 0 {
                            OUTPUT_WIDTH as u32
                        } else {
                            role.requested_size.0
                        };
                        let requested_height = if role.requested_size.1 == 0 {
                            OUTPUT_HEIGHT as u32
                        } else {
                            role.requested_size.1
                        };
                        (
                            surface_id.clone(),
                            role.resource.clone(),
                            requested_width.min(960),
                            requested_height.min(900),
                        )
                    })
                });
                if let Some((surface_id, layer, width, height)) = prompt_configure {
                    let serial = self.serial();
                    self.counts.prompt_configures.push((width, height));
                    self.counts.wire_events.push(format!(
                        "{:.3}ms output_resize_configure serial={serial} surface={surface_id:?} configured={width}x{height}",
                        self.start.elapsed().as_secs_f64() * 1000.,
                    ));
                    layer.configure(serial, width, height);
                }
            }
        }
    }
}

macro_rules! bind_unit_global {
    ($interface:ty) => {
        impl GlobalDispatch<$interface, ()> for Compositor {
            fn bind(
                _: &mut Self,
                _: &DisplayHandle,
                _: &Client,
                resource: New<$interface>,
                _: &(),
                init: &mut DataInit<'_, Self>,
            ) {
                init.init(resource, ());
            }
        }
    };
}

macro_rules! ignore_unit_requests {
    ($interface:ty, $request:ty) => {
        impl Dispatch<$interface, ()> for Compositor {
            fn request(
                _: &mut Self,
                _: &Client,
                _: &$interface,
                _: $request,
                _: &(),
                _: &DisplayHandle,
                _: &mut DataInit<'_, Self>,
            ) {
            }
        }
    };
}

bind_unit_global!(wl_compositor::WlCompositor);
bind_unit_global!(ZwlrLayerShellV1);
bind_unit_global!(WpViewporter);
bind_unit_global!(ZxdgOutputManagerV1);
bind_unit_global!(BeadLayerMotionManagerV1);
ignore_unit_requests!(wl_output::WlOutput, wl_output::Request);
ignore_unit_requests!(wl_region::WlRegion, wl_region::Request);
ignore_unit_requests!(wl_callback::WlCallback, wl_callback::Request);
ignore_unit_requests!(
    WpViewport,
    wayland_protocols::wp::viewporter::server::wp_viewport::Request
);
ignore_unit_requests!(ZxdgOutputV1, zxdg_output_v1::Request);

impl Dispatch<wl_compositor::WlCompositor, ()> for Compositor {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &wl_compositor::WlCompositor,
        request: wl_compositor::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_compositor::Request::CreateSurface { id } => {
                let data = Arc::new(Mutex::new(SurfaceData::default()));
                let surface = init.init(id, data.clone());
                state.surfaces.push(SurfaceRecord { surface, data });
            }
            wl_compositor::Request::CreateRegion { id } => {
                init.init(id, ());
            }
            _ => {}
        }
    }
}

impl GlobalDispatch<wl_output::WlOutput, ()> for Compositor {
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<wl_output::WlOutput>,
        _: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        let output = init.init(resource, ());
        output.geometry(
            0,
            0,
            300,
            200,
            wl_output::Subpixel::Unknown,
            "Nacre".into(),
            "wire-test".into(),
            wl_output::Transform::Normal,
        );
        output.mode(
            wl_output::Mode::Current,
            OUTPUT_WIDTH,
            OUTPUT_HEIGHT,
            120_000,
        );
        output.scale(1);
        output.name("NACRE-TEST-0".into());
        output.description("private prompt layer-motion test output".into());
    }
}
impl Dispatch<ZxdgOutputManagerV1, ()> for Compositor {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &ZxdgOutputManagerV1,
        request: zxdg_output_manager_v1::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let zxdg_output_manager_v1::Request::GetXdgOutput { id, output } = request {
            let xdg_output = init.init(id, ());
            xdg_output.logical_position(0, 0);
            xdg_output.logical_size(OUTPUT_WIDTH, OUTPUT_HEIGHT);
            xdg_output.name("NACRE-TEST-0".into());
            xdg_output.description("private prompt layer-motion test output".into());
            // xdg-output v3 replaces zxdg_output.done with wl_output.done.
            output.done();
            state.xdg_outputs.push((output, xdg_output));
        }
    }
}
impl GlobalDispatch<wl_shm::WlShm, ()> for Compositor {
    fn bind(
        _: &mut Self,
        _: &DisplayHandle,
        _: &Client,
        resource: New<wl_shm::WlShm>,
        _: &(),
        init: &mut DataInit<'_, Self>,
    ) {
        let shm = init.init(resource, ());
        shm.format(wl_shm::Format::Argb8888);
        shm.format(wl_shm::Format::Xrgb8888);
    }
}
impl Dispatch<wl_shm::WlShm, ()> for Compositor {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &wl_shm::WlShm,
        request: wl_shm::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let wl_shm::Request::CreatePool { id, .. } = request {
            init.init(id, ());
        }
    }
}
impl Dispatch<wl_shm_pool::WlShmPool, ()> for Compositor {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &wl_shm_pool::WlShmPool,
        request: wl_shm_pool::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let wl_shm_pool::Request::CreateBuffer {
            id, width, height, ..
        } = request
        {
            init.init(id, ShmBufferData { width, height });
        }
    }
}
impl Dispatch<wl_buffer::WlBuffer, ShmBufferData> for Compositor {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &wl_buffer::WlBuffer,
        _: wl_buffer::Request,
        _: &ShmBufferData,
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
    }
}
impl Dispatch<wl_surface::WlSurface, Arc<Mutex<SurfaceData>>> for Compositor {
    fn request(
        state: &mut Self,
        _: &Client,
        surface: &wl_surface::WlSurface,
        request: wl_surface::Request,
        data: &Arc<Mutex<SurfaceData>>,
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        match request {
            wl_surface::Request::Attach { buffer, .. } => {
                if buffer.is_some() {
                    let surface_id = surface.id();
                    let namespace = state
                        .layers
                        .get(&surface_id)
                        .map(|role| role.namespace.clone());
                    match namespace.as_deref() {
                        Some("bead-panel-dismiss") => state.counts.dismiss_attaches += 1,
                        Some(PROMPT_NAMESPACE) => state.counts.panel_attaches += 1,
                        _ => state.counts.other_attaches += 1,
                    }
                    if namespace.as_deref() == Some(PROMPT_NAMESPACE) {
                        let pending = state
                            .pending_motion
                            .as_ref()
                            .is_some_and(|motion| motion.surface_id == surface_id);
                        let active = state
                            .active_motion
                            .as_ref()
                            .is_some_and(|motion| motion.surface_id == surface_id);
                        let buffer_id = buffer.as_ref().map(Resource::id);
                        let buffer_info = buffer
                            .as_ref()
                            .and_then(|buffer| buffer.data::<ShmBufferData>())
                            .map(|info| {
                                state
                                    .counts
                                    .prompt_buffer_sizes
                                    .push((info.width, info.height));
                                format!("{}x{}", info.width, info.height)
                            });
                        state.counts.wire_events.push(format!(
                            "{:.3}ms prompt_attach#{} surface={surface_id:?} buffer={buffer_id:?} info={buffer_info:?} pending_motion={pending} active_motion={active} completions={}",
                            state.start.elapsed().as_secs_f64() * 1000.,
                            state.counts.panel_attaches,
                            state.counts.completions,
                        ));
                    }
                }
                data.lock().expect("surface lock").buffer = buffer;
            }
            wl_surface::Request::Frame { callback } => {
                let callback = init.init(callback, ());
                data.lock().expect("surface lock").callbacks.push(callback);
            }
            wl_surface::Request::Commit => {
                let surface_id = surface.id();
                let configure = state.layers.get_mut(&surface_id).and_then(|role| {
                    if role.configured_size == Some(role.requested_size) {
                        return None;
                    }
                    role.configured_size = Some(role.requested_size);
                    Some((
                        role.resource.clone(),
                        if role.requested_size.0 == 0 {
                            OUTPUT_WIDTH as u32
                        } else {
                            role.requested_size.0
                        },
                        if role.requested_size.1 == 0 {
                            OUTPUT_HEIGHT as u32
                        } else {
                            role.requested_size.1
                        },
                    ))
                });
                if let Some((layer, width, height)) = configure {
                    let serial = state.serial();
                    if state
                        .layers
                        .get(&surface_id)
                        .is_some_and(|role| role.namespace == PROMPT_NAMESPACE)
                    {
                        state.counts.prompt_configures.push((width, height));
                    }
                    state.counts.wire_events.push(format!(
                        "{:.3}ms layer_configure_send serial={serial} surface={surface_id:?} configured={width}x{height}",
                        state.start.elapsed().as_secs_f64() * 1000.,
                    ));
                    layer.configure(serial, width, height);
                }
                let buffer = data.lock().expect("surface lock").buffer.take();
                if let Some(buffer) = buffer {
                    if state
                        .layers
                        .get(&surface_id)
                        .is_some_and(|role| role.namespace == PROMPT_NAMESPACE)
                        && state.counts.first_panel_commit_armed.is_none()
                    {
                        state.counts.first_panel_commit_armed = Some(
                            state
                                .pending_motion
                                .as_ref()
                                .is_some_and(|pending| pending.surface_id == surface_id),
                        );
                    }
                    buffer.release();
                    if state
                        .pending_motion
                        .as_ref()
                        .is_some_and(|pending| pending.surface_id == surface_id)
                    {
                        let pending = state.pending_motion.take().expect("checked pending motion");
                        let planes = state
                            .planes
                            .get(&surface_id)
                            .map(|planes| planes.values().copied().collect())
                            .unwrap_or_default();
                        state.active_motion = Some(ActiveMotion {
                            resource: pending.resource,
                            surface_id: pending.surface_id,
                            serial: pending.serial,
                            ends: Instant::now() + pending.duration,
                            planes,
                        });
                    }
                }
            }
            _ => {}
        }
        let _ = surface;
    }
}

impl Dispatch<ZwlrLayerShellV1, ()> for Compositor {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &ZwlrLayerShellV1,
        request: zwlr_layer_shell_v1::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let zwlr_layer_shell_v1::Request::GetLayerSurface {
            id,
            surface,
            namespace,
            ..
        } = request
        {
            let surface_id = surface.id();
            assert!(
                state
                    .surfaces
                    .iter()
                    .any(|record| record.surface.id() == surface_id)
            );
            let layer = init.init(id, surface_id.clone());
            state.layers.insert(
                surface_id,
                LayerRole {
                    namespace,
                    resource: layer,
                    requested_size: (0, 0),
                    configured_size: None,
                },
            );
        }
    }
}
impl Dispatch<ZwlrLayerSurfaceV1, wayland_server::backend::ObjectId> for Compositor {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &ZwlrLayerSurfaceV1,
        request: zwlr_layer_surface_v1::Request,
        surface_id: &wayland_server::backend::ObjectId,
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwlr_layer_surface_v1::Request::SetSize { width, height } => {
                let namespace = state
                    .layers
                    .get(surface_id)
                    .map(|role| role.namespace.clone())
                    .unwrap_or_else(|| "unassigned".into());
                state.counts.wire_events.push(format!(
                    "{:.3}ms layer_set_size surface={surface_id:?} namespace={} requested={width}x{height}",
                    state.start.elapsed().as_secs_f64() * 1000.,
                    namespace,
                ));
                if namespace == PROMPT_NAMESPACE {
                    state.counts.prompt_set_sizes.push((width, height));
                }
                if let Some(role) = state.layers.get_mut(surface_id) {
                    role.requested_size = (width, height);
                }
            }
            zwlr_layer_surface_v1::Request::AckConfigure { serial } => {
                state.counts.wire_events.push(format!(
                    "{:.3}ms layer_ack_configure surface={surface_id:?} serial={serial}",
                    state.start.elapsed().as_secs_f64() * 1000.,
                ));
            }
            _ => {}
        }
    }
}

impl Dispatch<WpViewporter, ()> for Compositor {
    fn request(
        _: &mut Self,
        _: &Client,
        _: &WpViewporter,
        request: wp_viewporter::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let wp_viewporter::Request::GetViewport { id, .. } = request {
            init.init(id, ());
        }
    }
}
impl Dispatch<BeadLayerMotionManagerV1, ()> for Compositor {
    fn request(
        state: &mut Self,
        _: &Client,
        _: &BeadLayerMotionManagerV1,
        request: motion_protocol::bead_layer_motion_manager_v1::Request,
        _: &(),
        _: &DisplayHandle,
        init: &mut DataInit<'_, Self>,
    ) {
        if let motion_protocol::bead_layer_motion_manager_v1::Request::GetMotion { id, surface } =
            request
        {
            let surface_id = surface.id();
            if state
                .layers
                .get(&surface_id)
                .is_some_and(|role| role.namespace == PROMPT_NAMESPACE)
            {
                state.counts.prompt_controllers += 1;
            }
            init.init(id, ControllerData { surface_id });
        }
    }
}
impl Dispatch<BeadLayerMotionV1, ControllerData> for Compositor {
    fn request(
        state: &mut Self,
        _: &Client,
        resource: &BeadLayerMotionV1,
        request: MotionRequest,
        data: &ControllerData,
        _: &DisplayHandle,
        _: &mut DataInit<'_, Self>,
    ) {
        match request {
            MotionRequest::AddPlane {
                plane,
                from_x,
                from_y,
                from_width,
                from_height,
                clip_x,
                clip_y,
                clip_width,
                clip_height,
                from_opacity,
                ..
            } => {
                state
                    .planes
                    .entry(data.surface_id.clone())
                    .or_default()
                    .insert(
                        plane,
                        PlaneState {
                            id: plane,
                            x: from_x,
                            y: from_y,
                            width: from_width,
                            height: from_height,
                            clip_x,
                            clip_y,
                            clip_width,
                            clip_height,
                            opacity: from_opacity,
                        },
                    );
            }
            MotionRequest::SetPlaneTo {
                plane,
                x,
                y,
                width,
                height,
                clip_x,
                clip_y,
                clip_width,
                clip_height,
                opacity,
            } => {
                state
                    .planes
                    .entry(data.surface_id.clone())
                    .or_default()
                    .insert(
                        plane,
                        PlaneState {
                            id: plane,
                            x,
                            y,
                            width,
                            height,
                            clip_x,
                            clip_y,
                            clip_width,
                            clip_height,
                            opacity,
                        },
                    );
            }
            MotionRequest::Animate {
                serial,
                duration_ms,
                ..
            } => {
                state.counts.animate_requests += 1;
                state.counts.wire_events.push(format!(
                    "{:.3}ms animate_request serial={serial} duration_ms={duration_ms} surface={:?}",
                    state.start.elapsed().as_secs_f64() * 1000.,
                    data.surface_id,
                ));
                state.pending_motion = Some(PendingMotion {
                    resource: resource.clone(),
                    surface_id: data.surface_id.clone(),
                    serial,
                    duration: Duration::from_millis(duration_ms.into()),
                });
            }
            _ => {}
        }
    }
}
