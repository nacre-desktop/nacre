//! Private nested proof of native hover, preserved focus and the real QML title.
use crate::{compositor::niri_transport::Connection, json, niri_hover_pointer::Pointer, Result};
use serde_json::{json as value, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    os::unix::{fs::PermissionsExt, net::UnixStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};
use wait_timeout::ChildExt;

#[derive(Default)]
struct Children(Vec<Child>);
impl Children {
    fn start(
        &mut self,
        command: &mut Command,
        environment: &BTreeMap<String, String>,
        log: &fs::File,
    ) -> Result<usize> {
        command
            .env_clear()
            .envs(environment)
            .stdin(Stdio::null())
            .stdout(log.try_clone()?)
            .stderr(log.try_clone()?);
        self.0.push(command.spawn()?);
        Ok(self.0.len() - 1)
    }
}
impl Drop for Children {
    fn drop(&mut self) {
        for child in self.0.iter_mut().rev() {
            match child.try_wait() {
                Ok(Some(_)) => {}
                Ok(None) => {
                    if let Err(error) = nix::sys::signal::kill(
                        nix::unistd::Pid::from_raw(child.id() as i32),
                        nix::sys::signal::Signal::SIGTERM,
                    ) {
                        eprintln!("Nested child termination failed: {error}");
                    }
                }
                Err(error) => eprintln!("Nested child status failed: {error}"),
            }
        }
        for child in &mut self.0 {
            match child.wait_timeout(Duration::from_secs(5)) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    if let Err(error) = child.kill().and_then(|()| child.wait().map(|_| ())) {
                        eprintln!("Nested child cleanup failed: {error}");
                    }
                }
                Err(error) => eprintln!("Nested child wait failed: {error}"),
            }
        }
    }
}

fn wait_created(
    runtime: &Path,
    predicate: impl Fn(&Path) -> bool,
    child: &mut Child,
) -> Result<PathBuf> {
    let pid =
        rustix::process::Pid::from_raw(child.id() as i32).ok_or("Invalid nested child PID")?;
    let process = rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty())?;
    let watch = rustix::fs::inotify::init(
        rustix::fs::inotify::CreateFlags::NONBLOCK | rustix::fs::inotify::CreateFlags::CLOEXEC,
    )?;
    rustix::fs::inotify::add_watch(
        &watch,
        runtime,
        rustix::fs::inotify::WatchFlags::CREATE | rustix::fs::inotify::WatchFlags::MOVED_TO,
    )?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        for entry in fs::read_dir(runtime)? {
            let path = entry?.path();
            if predicate(&path) {
                return Ok(path);
            }
        }
        if let Some(status) = child.try_wait()? {
            return Err(format!("Nested compositor exited: {status}").into());
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("Nested socket creation timed out")?;
        let timeout = rustix::event::Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: i64::from(remaining.subsec_nanos()),
        };
        let mut fds = [
            rustix::event::PollFd::new(&watch, rustix::event::PollFlags::IN),
            rustix::event::PollFd::new(&process, rustix::event::PollFlags::IN),
        ];
        match rustix::event::poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err("Nested socket creation timed out".into()),
            Ok(_) => {
                if !fds[1].revents().is_empty() {
                    let status = child.wait()?;
                    return Err(format!("Nested compositor exited: {status}").into());
                }
                let mut bytes = [0u8; 4096];
                match rustix::io::read(&watch, &mut bytes) {
                    Ok(_) => {}
                    Err(rustix::io::Errno::WOULDBLOCK | rustix::io::Errno::INTR) => {}
                    Err(error) => return Err(error.into()),
                }
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

struct Stream {
    socket: UnixStream,
    buffer: Vec<u8>,
    events: Vec<Value>,
}
impl Stream {
    fn subscribe(socket: &Path, request: &str) -> Result<Self> {
        let mut connection = UnixStream::connect(socket)?;
        connection.set_read_timeout(Some(Duration::from_secs(3)))?;
        connection.set_write_timeout(Some(Duration::from_secs(3)))?;
        connection.write_all((json::encode(&request)? + "\n").as_bytes())?;
        let mut reply = Vec::new();
        while reply.len() < 4096 {
            let mut byte = [0u8; 1];
            connection.read_exact(&mut byte)?;
            if byte[0] == b'\n' {
                break;
            }
            reply.push(byte[0]);
        }
        if serde_json::from_slice::<Value>(&reply)? != value!({"Ok":"Handled"}) {
            return Err("Nested stream handshake failed".into());
        }
        connection.set_nonblocking(true)?;
        Ok(Self {
            socket: connection,
            buffer: Vec::new(),
            events: Vec::new(),
        })
    }
    fn receive(&mut self) -> Result<()> {
        let mut bytes = [0u8; 65536];
        loop {
            match self.socket.read(&mut bytes) {
                Ok(0) => return Err("Nested event stream disconnected".into()),
                Ok(count) => self.buffer.extend_from_slice(&bytes[..count]),
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error.into()),
            }
            if self.buffer.len() > 4 * 1024 * 1024 {
                return Err("Nested event frame exceeded its limit".into());
            }
            while let Some(end) = self.buffer.iter().position(|byte| *byte == b'\n') {
                self.events
                    .push(serde_json::from_slice(&self.buffer[..end])?);
                self.buffer.drain(..=end);
                if self.events.len() > 8192 {
                    return Err("Nested event proof exceeded its limit".into());
                }
            }
        }
    }
}
fn drain(legacy: &mut Stream, hover: &mut Stream, duration: Duration) -> Result<()> {
    let deadline = Instant::now() + duration;
    loop {
        let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
            return Ok(());
        };
        let timeout = rustix::event::Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: i64::from(remaining.subsec_nanos()),
        };
        let mut fds = [
            rustix::event::PollFd::new(&legacy.socket, rustix::event::PollFlags::IN),
            rustix::event::PollFd::new(&hover.socket, rustix::event::PollFlags::IN),
        ];
        match rustix::event::poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Ok(()),
            Ok(_) => {
                let ready = [!fds[0].revents().is_empty(), !fds[1].revents().is_empty()];
                if ready[0] {
                    legacy.receive()?;
                }
                if ready[1] {
                    hover.receive()?;
                }
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn wait_event(legacy: &mut Stream, hover: &mut Stream, duration: Duration) -> Result<()> {
    let deadline = Instant::now() + duration;
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("Nested window readiness timed out")?;
        let timeout = rustix::event::Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: i64::from(remaining.subsec_nanos()),
        };
        let mut fds = [
            rustix::event::PollFd::new(&legacy.socket, rustix::event::PollFlags::IN),
            rustix::event::PollFd::new(&hover.socket, rustix::event::PollFlags::IN),
        ];
        match rustix::event::poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err("Nested window readiness timed out".into()),
            Ok(_) => {
                let ready = [!fds[0].revents().is_empty(), !fds[1].revents().is_empty()];
                if ready[0] {
                    legacy.receive()?;
                }
                if ready[1] {
                    hover.receive()?;
                }
                return Ok(());
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn bus_address(mut output: std::process::ChildStdout) -> Result<String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    let mut address = Vec::new();
    while address.len() < 4096 {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("Private bus startup timed out")?;
        let timeout = rustix::event::Timespec {
            tv_sec: remaining.as_secs() as i64,
            tv_nsec: i64::from(remaining.subsec_nanos()),
        };
        let mut fds = [rustix::event::PollFd::new(
            &output,
            rustix::event::PollFlags::IN,
        )];
        match rustix::event::poll(&mut fds, Some(&timeout)) {
            Ok(0) => return Err("Private bus startup timed out".into()),
            Ok(_) => {}
            Err(rustix::io::Errno::INTR) => continue,
            Err(error) => return Err(error.into()),
        }
        let mut byte = [0u8; 1];
        output.read_exact(&mut byte)?;
        if byte[0] == b'\n' {
            return Ok(String::from_utf8(address)?);
        }
        address.push(byte[0]);
    }
    Err("Private bus address exceeded its limit".into())
}

fn qml_tree(repo: &Path, destination: &Path) -> Result<()> {
    let assets: Value = serde_json::from_str(include_str!("../assets/niri-hover-probe.json"))?;
    for (relative, text) in assets.as_object().ok_or("Invalid probe assets")? {
        let path = destination.join(relative);
        fs::create_dir_all(path.parent().ok_or("Invalid probe path")?)?;
        fs::write(path, text.as_str().ok_or("Invalid probe text")?)?;
    }
    for (source, target) in [
        (
            "services/NacreCompositor.qml",
            "services/NacreCompositor.qml",
        ),
        ("utils/NacrePaths.qml", "utils/NacrePaths.qml"),
        ("widgets/NacreText.qml", "widgets/NacreText.qml"),
        ("widgets/NacreIcon.qml", "widgets/NacreIcon.qml"),
        (
            "modules/bar/components/NacreActiveTitle.qml",
            "modules/bar/components/NacreActiveTitle.qml",
        ),
        (
            "utils/scripts/window-title.js",
            "utils/scripts/window-title.js",
        ),
    ] {
        let target = destination.join(target);
        fs::create_dir_all(target.parent().ok_or("Invalid probe target")?)?;
        fs::copy(repo.join("nacre/shell").join(source), target)?;
    }
    crate::niri_session_stage::copy_tree(
        &repo.join("nacre/shell/services/compositor"),
        &destination.join("services/compositor"),
        &[],
    )?;
    fs::write(destination.join("services/qmldir"), "singleton NacreCompositor 1.0 NacreCompositor.qml\nsingleton NacreColours 1.0 NacreColours.qml\nsingleton NacreAudio 1.0 NacreAudio.qml\nsingleton NacreI18n 1.0 NacreI18n.qml\nsingleton ChatWindowTitle 1.0 ChatWindowTitle.qml\n")?;
    fs::write(
        destination.join("config/qmldir"),
        "singleton NacreAppearance 1.0 NacreAppearance.qml\n",
    )?;
    fs::write(
        destination.join("utils/qmldir"),
        "singleton NacreIcons 1.0 NacreIcons.qml\nsingleton NacrePaths 1.0 NacrePaths.qml\n",
    )?;
    fs::write(destination.join("widgets/qmldir"), "singleton NacreTokens 1.0 NacreTokens.qml\nNacreText 1.0 NacreText.qml\nNacreIcon 1.0 NacreIcon.qml\n")?;
    Ok(())
}

fn response(connection: &mut Connection, request: &str, key: &str) -> Result<Value> {
    let result =
        connection.request_value(&value!(request), Duration::from_secs(3), 4 * 1024 * 1024)?;
    result
        .get(key)
        .cloned()
        .ok_or_else(|| format!("Missing nested response: {key}").into())
}
fn action(connection: &mut Connection, action: niri_ipc::Action) -> Result<()> {
    connection.request(
        &niri_ipc::Request::Action(action),
        Duration::from_secs(3),
        4 * 1024 * 1024,
    )?;
    Ok(())
}

pub fn verify(
    repo: &Path,
    binary: &Path,
    output: &Path,
    environment: &BTreeMap<String, String>,
    helper: &Path,
) -> Result<Value> {
    let outer_runtime = environment
        .get("XDG_RUNTIME_DIR")
        .ok_or("Nested proof requires a Wayland runtime")?;
    let outer_display = PathBuf::from(
        environment
            .get("WAYLAND_DISPLAY")
            .ok_or("Nested proof requires a Wayland display")?,
    );
    let outer_display = if outer_display.is_absolute() {
        outer_display
    } else {
        Path::new(outer_runtime).join(outer_display)
    };
    let temporary = tempfile::Builder::new().prefix("nhover-").tempdir()?;
    let root = temporary.path();
    let runtime = root.join("runtime");
    fs::create_dir(&runtime)?;
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o700))?;
    let config = root.join("config.kdl");
    fs::write(&config, "input { keyboard { xkb { layout \"us\"; }; }; }\nlayout { gaps 10; default-column-width { proportion 0.5; }; }\nhotkey-overlay { skip-at-startup; }\n")?;
    let home = root.join("home");
    let tools = home.join(".local/share/nacre/shell/tools");
    fs::create_dir_all(&tools)?;
    fs::copy(helper, tools.join("nacre-helper"))?;
    let mut environment = environment.clone();
    for (name, path) in [
        ("HOME", home.clone()),
        ("XDG_RUNTIME_DIR", runtime.clone()),
        ("XDG_CONFIG_HOME", home.join(".config")),
        ("XDG_DATA_HOME", home.join(".local/share")),
        ("XDG_STATE_HOME", home.join(".local/state")),
        ("XDG_CACHE_HOME", home.join(".cache")),
        ("WAYLAND_DISPLAY", outer_display),
        ("NIRI_CONFIG", config.clone()),
    ] {
        environment.insert(name.into(), path.to_string_lossy().into_owned());
    }
    environment.insert("NACRE_COMPOSITOR".into(), "niri".into());
    environment.insert("QT_QPA_PLATFORM".into(), "wayland".into());
    for name in [
        "NIRI_SOCKET",
        "DISPLAY",
        "NACRE_QS_CONFIG",
        "QT_LOGGING_RULES",
        "HYPRLAND_INSTANCE_SIGNATURE",
        "NACRE_NIRI_HOVER_STREAM",
    ] {
        environment.remove(name);
    }
    fs::create_dir_all(output.parent().ok_or("Invalid proof destination")?)?;
    let log = fs::File::create(output.with_extension("log"))?;
    let mut children = Children::default();
    let bus_config = root.join("bus.conf");
    fs::write(&bus_config, "<busconfig><type>session</type><listen>unix:tmpdir=/tmp</listen><policy context=\"default\"><allow send_destination=\"*\"/><allow receive_sender=\"*\"/><allow own=\"*\"/></policy></busconfig>")?;
    let mut bus = Command::new("dbus-daemon")
        .arg("--nofork")
        .arg("--print-address=1")
        .arg(format!("--config-file={}", bus_config.display()))
        .env_clear()
        .envs(&environment)
        .stdout(Stdio::piped())
        .stderr(log.try_clone()?)
        .spawn()?;
    let stdout = bus.stdout.take().ok_or("Missing private bus output")?;
    children.0.push(bus);
    let address = bus_address(stdout)?;
    if address.trim().is_empty() {
        return Err("Private probe bus failed to start".into());
    }
    environment.insert("DBUS_SESSION_BUS_ADDRESS".into(), address.trim().into());
    let index = children.start(
        Command::new(binary).arg("--config").arg(&config),
        &environment,
        &log,
    )?;
    let pid = children.0[index].id().to_string();
    let socket = wait_created(
        &runtime,
        |path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            name.starts_with("niri.") && name.ends_with(&format!(".{pid}.sock"))
        },
        &mut children.0[index],
    )?;
    let display = wait_created(
        &runtime,
        |path| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            name.starts_with("wayland-") && !name.ends_with(".lock")
        },
        &mut children.0[index],
    )?;
    environment.insert("NIRI_SOCKET".into(), socket.to_string_lossy().into_owned());
    environment.insert(
        "WAYLAND_DISPLAY".into(),
        display.to_string_lossy().into_owned(),
    );
    let mut legacy = Stream::subscribe(&socket, "EventStream")?;
    let mut hover = Stream::subscribe(&socket, "NacreOverviewHoverStream")?;
    let mut connection = Connection::new(&socket);
    for title in ["Hover Alpha", "Hover Beta"] {
        children.start(
            Command::new("kitty").args([
                "--config",
                "NONE",
                "--class",
                "nacre-hover-probe",
                "--title",
                title,
                "--",
                "sleep",
                "120",
            ]),
            &environment,
            &log,
        )?;
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let windows = response(&mut connection, "Windows", "Windows")?;
        if windows.as_array().is_some_and(|windows| windows.len() == 2) {
            break;
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("Probe windows missing")?;
        wait_event(&mut legacy, &mut hover, remaining)?;
    }
    let qml = root.join("bar");
    qml_tree(repo, &qml)?;
    children.start(
        Command::new("quickshell").arg("-p").arg(&qml).arg("-n"),
        &environment,
        &log,
    )?;
    action(&mut connection, niri_ipc::Action::OpenOverview {})?;
    drain(&mut legacy, &mut hover, Duration::from_millis(100))?;
    let focus = response(&mut connection, "FocusedWindow", "FocusedWindow")?;
    let selection = response(&mut connection, "Workspaces", "Workspaces")?;
    let selection = selected(&selection)?;
    let outputs = response(&mut connection, "Outputs", "Outputs")?;
    let output_info = outputs
        .as_object()
        .and_then(|outputs| outputs.values().next())
        .ok_or("Nested output missing")?;
    let width = output_info["logical"]["width"]
        .as_u64()
        .filter(|width| *width >= 8 && *width <= u32::MAX as u64)
        .ok_or("Invalid nested width")? as u32;
    let height = output_info["logical"]["height"]
        .as_u64()
        .filter(|height| *height >= 8 && *height <= u32::MAX as u64)
        .ok_or("Invalid nested height")? as u32;
    let mut pointer = Pointer::connect(&display)?;
    let mut found = serde_json::Map::new();
    let mut latencies = Vec::new();
    'points: for y in (height / 8..height).step_by((height / 8) as usize) {
        for x in (width / 8..width).step_by((width / 8) as usize) {
            let started = Instant::now();
            pointer.move_to(x, y, width, height)?;
            drain(&mut legacy, &mut hover, Duration::from_millis(20))?;
            let under = response(&mut connection, "WindowUnderCursor", "WindowUnderCursor")?;
            if !under.is_null() {
                let id = under["id"]
                    .as_u64()
                    .ok_or("Invalid hovered ID")?
                    .to_string();
                if !found.contains_key(&id) {
                    found.insert(id, value!([x, y, under["title"]]));
                    latencies.push(started.elapsed().as_secs_f64() * 1000.0);
                }
            }
            if found.len() == 2 {
                break 'points;
            }
        }
    }
    if found.len() != 2 {
        return Err("Could not hit both preview windows".into());
    }
    let mut captions = serde_json::Map::new();
    for (id, point) in &found {
        pointer.move_to(
            point[0].as_u64().ok_or("Invalid probe x")? as u32,
            point[1].as_u64().ok_or("Invalid probe y")? as u32,
            width,
            height,
        )?;
        let deadline = Instant::now() + Duration::from_millis(1500);
        loop {
            let mut command = Command::new("quickshell");
            command
                .arg("-p")
                .arg(&qml)
                .args(["ipc", "call", "windowTitle-default", "state"])
                .env_clear()
                .envs(&environment);
            let caption = crate::process::capture(command, None, Duration::from_secs(3))?;
            if caption.status.success() {
                let state: Value = serde_json::from_slice(&caption.stdout)?;
                if state["title"] == point[2] {
                    captions.insert(id.clone(), state["title"].clone());
                    break;
                }
            }
            if Instant::now() >= deadline {
                return Err("QML title does not follow hover".into());
            }
            drain(&mut legacy, &mut hover, Duration::from_millis(50))?;
        }
    }
    if response(&mut connection, "FocusedWindow", "FocusedWindow")? != focus {
        return Err("Hover changed keyboard focus".into());
    }
    if selected(&response(&mut connection, "Workspaces", "Workspaces")?)? != selection {
        return Err("Hover changed selected windows".into());
    }
    drain(&mut legacy, &mut hover, Duration::from_millis(100))?;
    let ids: Vec<Value> = hover
        .events
        .iter()
        .filter_map(|event| {
            event
                .get("NacreOverviewHoverChanged")
                .and_then(|change| change.get("id"))
                .cloned()
        })
        .collect();
    if found
        .keys()
        .any(|id| !ids.contains(&value!(id.parse::<u64>().unwrap_or(u64::MAX))))
    {
        return Err("Missing hover events".into());
    }
    if legacy
        .events
        .iter()
        .any(|event| event.get("NacreOverviewHoverChanged").is_some())
    {
        return Err("Hover leaked into legacy stream".into());
    }
    action(&mut connection, niri_ipc::Action::CloseOverview {})?;
    drain(&mut legacy, &mut hover, Duration::from_millis(100))?;
    if hover.events.iter().rev().find_map(|event| {
        event
            .get("NacreOverviewHoverChanged")
            .and_then(|change| change.get("id"))
    }) != Some(&Value::Null)
    {
        return Err("Hover was not cleared on overview close".into());
    }
    let report = value!({"sourceRevision":crate::niri_session_runtime::HOVER_SOURCE,"windows":found,"focusPreserved":true,"legacyStreamUnaffected":true,"hoverClearedOnClose":true,"probeRoundtripMs":latencies,"nestedVerified":true,"qmlCaptionVerified":true,"qmlCaptions":captions,"binarySha256":crate::niri_hover_install::digest(binary)?,"patchSha256":crate::niri_hover_install::digest(&repo.join("tools/patches/niri-26.04-overview-hover.patch"))?});
    fs::write(output, json::pretty(&report)?)?;
    println!("{}", json::encode(&report)?);
    Ok(report)
}
fn selected(workspaces: &Value) -> Result<BTreeMap<String, Value>> {
    workspaces
        .as_array()
        .ok_or("Invalid nested workspaces")?
        .iter()
        .map(|workspace| {
            Ok((
                workspace["id"]
                    .as_u64()
                    .ok_or("Invalid workspace ID")?
                    .to_string(),
                workspace["active_window_id"].clone(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nested_child_exit_wakes_socket_wait_without_readiness_polling() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let mut child = Command::new("/bin/sh")
            .args(["-c", "exit 7"])
            .env_clear()
            .spawn()?;
        let result = wait_created(temporary.path(), |_| false, &mut child);
        assert!(result
            .expect_err("Unexpected readiness")
            .to_string()
            .starts_with("Nested compositor exited:"));
        assert_eq!(child.wait()?.code(), Some(7));
        Ok(())
    }
}
