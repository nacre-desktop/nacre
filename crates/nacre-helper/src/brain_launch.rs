//! Browser and window lifecycle for the Brain web interface.
use crate::{brain_auth, brain_web, paths::Paths, Result};
use serde_json::{json, Value};
use std::{
    env, fs,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    thread,
    time::Duration,
};

fn home() -> Result<PathBuf> {
    env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "Missing home directory".into())
}

fn state() -> Result<PathBuf> {
    brain_web::state()
}

fn launch(argv: &[String]) -> Result<()> {
    let (program, args) = argv.split_first().ok_or("Empty Brain launch command")?;
    let mut child = crate::process::spawn_detached_quiet(program, args)
        .map_err(|error| launch_error_message(program, &error))?;
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

fn launch_error_message(program: &str, error: &dyn std::fmt::Display) -> String {
    format!("Could not start {program}: {error}")
}

pub(crate) fn launch_app(argv: &[String]) -> Result<()> {
    let mut command = vec![
        "systemd-run".into(),
        "--user".into(),
        "--scope".into(),
        "--collect".into(),
        "--quiet".into(),
        "env".into(),
        "-u".into(),
        "LD_LIBRARY_PATH".into(),
        "-u".into(),
        "QML_IMPORT_PATH".into(),
        "-u".into(),
        "QT_PLUGIN_PATH".into(),
    ];
    command.extend_from_slice(argv);
    launch(&command)
}

fn run_output(
    argv: &[String],
    input: Option<&str>,
    timeout: Duration,
) -> Result<std::process::Output> {
    let (program, args) = argv.split_first().ok_or("Empty Brain command")?;
    crate::process::run(program, args, input, timeout)
}

fn reject_linked_ancestors(path: &Path) -> Result<()> {
    if !path.is_absolute() {
        return Err("Brain state path must be absolute".into());
    }
    for ancestor in path.ancestors() {
        if fs::symlink_metadata(ancestor).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
            return Err("Refusing symlinked Brain state path".into());
        }
    }
    Ok(())
}

fn private_state_dir(path: &Path) -> Result<()> {
    reject_linked_ancestors(path)?;
    fs::create_dir_all(path)?;
    reject_linked_ancestors(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

fn open_lock(path: &Path) -> Result<fs::File> {
    reject_linked_ancestors(path)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_CLOEXEC)
        .open(path)?;
    if file.metadata()?.nlink() != 1 {
        return Err("Refusing linked Brain lock file".into());
    }
    Ok(file)
}

fn compositor() -> Result<crate::compositor::backend::Backend> {
    crate::compositor::backend::Backend::new(Paths::environment()?, env::vars().collect())
        .map_err(|error| error.into())
}

fn query(name: &str) -> Result<Value> {
    compositor()?
        .query(name, Duration::from_secs(3), None)
        .map_err(Into::into)
}

fn act(name: &str, args: &[Value], kwargs: Value) -> Result<()> {
    compositor()?
        .action(name, args, &kwargs)
        .map(|_| ())
        .map_err(Into::into)
}

fn profile_workspace() -> Option<i64> {
    let path = home().ok()?.join(".config/nacre/profile.json");
    let value: Value = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
    let workspace = value.get("brainWorkspace")?.as_i64()?;
    (1..=10).contains(&workspace).then_some(workspace)
}

fn profile_path() -> Result<PathBuf> {
    Ok(home()?.join(".local/share/nacre/brain-browser"))
}

fn profile_pids() -> std::collections::BTreeSet<u32> {
    let Ok(profile) = profile_path() else {
        return Default::default();
    };
    let marker = format!("--user-data-dir={}", profile.display()).into_bytes();
    let mut found = std::collections::BTreeSet::new();
    let Ok(entries) = fs::read_dir("/proc") else {
        return found;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|name| name.parse::<u32>().ok()) else {
            continue;
        };
        let Ok(metadata) = entry.metadata() else {
            continue;
        };
        if metadata.uid() != rustix::process::geteuid().as_raw() {
            continue;
        }
        if fs::read(entry.path().join("cmdline"))
            .ok()
            .is_some_and(|args| args.split(|b| *b == 0).any(|arg| arg == marker))
        {
            found.insert(pid);
        }
    }
    found
}

pub(crate) fn browser_running() -> bool {
    !profile_pids().is_empty()
}

fn percent_encode(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

fn is_brain_window(window: &Value, pids: &std::collections::BTreeSet<u32>) -> bool {
    let class = window
        .get("class")
        .and_then(Value::as_str)
        .unwrap_or_default();
    class == "nacre-brain"
        || (window
            .get("pid")
            .and_then(Value::as_u64)
            .is_some_and(|pid| pids.contains(&(pid as u32)))
            && (class == "chrome-127.0.0.1__-Default"
                || class == "nacre-brain-auth"
                || [
                    "chrome-",
                    "chromium-",
                    "brave-",
                    "vivaldi-",
                    "microsoft-edge-",
                    "msedge-",
                ]
                .iter()
                .any(|prefix| class.starts_with(prefix))))
}

fn preferred_window(candidates: &[Value], workspace: Option<i64>) -> Option<Value> {
    workspace
        .and_then(|workspace| {
            candidates
                .iter()
                .find(|w| w.pointer("/workspace/id").and_then(Value::as_i64) == Some(workspace))
        })
        .cloned()
        .or_else(|| candidates.first().cloned())
}

fn should_launch_browser(focus: bool, window_present: bool, profile_running: bool) -> bool {
    !window_present && (focus || !profile_running)
}

fn window_launch_decision(
    query_window: impl FnOnce() -> Result<Option<Value>>,
    focus: bool,
    profile_running: impl FnOnce() -> bool,
) -> Result<(Option<Value>, bool)> {
    let window = query_window()?;
    let launch = should_launch_browser(focus, window.is_some(), profile_running());
    Ok((window, launch))
}

fn restore_background_workspace(focus: bool, previous: Option<i64>, mut switch: impl FnMut(i64)) {
    if !focus {
        if let Some(workspace) = previous.filter(|workspace| *workspace > 0) {
            switch(workspace);
        }
    }
}

fn clear_fullscreen_if_supported(capabilities: &Value, clear: impl FnOnce()) {
    if capabilities
        .pointer("/fullscreenState")
        .and_then(Value::as_bool)
        .unwrap_or(true)
    {
        clear();
    }
}

fn brain_window() -> Result<Option<Value>> {
    let windows = query("windows")?;
    let windows = windows.as_array().ok_or("Invalid compositor window list")?;
    let pids = profile_pids();
    let candidates: Vec<_> = windows
        .iter()
        .filter(|w| is_brain_window(w, &pids))
        .cloned()
        .collect();
    Ok(preferred_window(&candidates, profile_workspace()))
}

fn browser_arguments(browser: &Path, profile: &Path, app_uri: &str, handoff: bool) -> Vec<String> {
    vec![
        browser.to_string_lossy().into_owned(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--ozone-platform=wayland".into(),
        format!(
            "--class={}",
            if handoff {
                "nacre-brain-auth"
            } else {
                "nacre-brain"
            }
        ),
        format!("--user-data-dir={}", profile.display()),
        format!("--app={app_uri}"),
    ]
}

fn known_browser(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    ([
        "brave-origin",
        "brave",
        "brave-browser",
        "chromium",
        "google-chrome-stable",
        "google-chrome",
        "vivaldi",
        "vivaldi-stable",
        "microsoft-edge",
        "brave-browser-stable",
    ]
    .contains(&name)
        || matches!(
            name,
            "chrome" | "chromium-browser" | "microsoft-edge-stable"
        ))
        && fs::metadata(path).is_ok_and(|metadata| metadata.permissions().mode() & 0o111 != 0)
}

fn in_path(program: &str, path: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let candidate = PathBuf::from(program);
        return (candidate.is_file() && known_browser(&candidate)).then_some(candidate);
    }
    path.split(':')
        .map(|part| Path::new(part).join(program))
        .find(|candidate| candidate.is_file() && known_browser(candidate))
}

fn select_browser(preferred: &[String], path: &str, chrome_path: &Path) -> Option<PathBuf> {
    preferred
        .iter()
        .find_map(|candidate| in_path(candidate, path))
        .or_else(|| {
            [
                "brave-origin",
                "brave",
                "brave-browser",
                "chromium",
                "google-chrome-stable",
            ]
            .into_iter()
            .find_map(|name| in_path(name, path))
        })
        .or_else(|| {
            (chrome_path.is_file() && known_browser(chrome_path)).then(|| chrome_path.to_path_buf())
        })
        .or_else(|| {
            ["vivaldi", "microsoft-edge"]
                .into_iter()
                .find_map(|name| in_path(name, path))
        })
}

fn desktop_exec(desktop_id: &str, home: &Path) -> Option<String> {
    let data_home = env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local/share"));
    let mut roots = vec![data_home.join("applications")];
    roots.extend(
        env::var_os("XDG_DATA_DIRS")
            .map(|dirs| {
                dirs.to_string_lossy()
                    .split(':')
                    .filter(|s| !s.is_empty())
                    .map(PathBuf::from)
                    .map(|p| p.join("applications"))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_else(|| {
                vec![
                    PathBuf::from("/usr/local/share/applications"),
                    PathBuf::from("/usr/share/applications"),
                ]
            }),
    );
    desktop_exec_in_roots(desktop_id, &roots)
}

fn desktop_exec_in_roots(desktop_id: &str, roots: &[PathBuf]) -> Option<String> {
    let file = roots
        .iter()
        .map(|root| root.join(desktop_id))
        .find(|path| path.is_file())?;
    let content = fs::read_to_string(file).ok()?;
    let exec = content
        .lines()
        .find_map(|line| line.strip_prefix("Exec="))?;
    exec.split_whitespace()
        .next()
        .map(|s| s.trim_matches('"').to_owned())
}

fn mime_default(content: &str) -> Option<String> {
    let mut in_defaults = false;
    for line in content.lines().map(str::trim) {
        if line.starts_with('[') && line.ends_with(']') {
            in_defaults = line == "[Default Applications]";
        } else if in_defaults {
            if let Some(value) = line.strip_prefix("x-scheme-handler/http=") {
                return value
                    .split(';')
                    .find(|entry| !entry.is_empty())
                    .map(str::to_owned);
            }
        }
    }
    None
}

fn browser_command(url: &str, handoff: bool) -> Result<Vec<String>> {
    let home = home()?;
    let mut preferred = Vec::new();
    if let Ok(output) = run_output(
        &[
            "xdg-settings".into(),
            "get".into(),
            "default-web-browser".into(),
        ],
        None,
        Duration::from_secs(2),
    ) {
        if output.status.success() {
            let desktop_id = String::from_utf8_lossy(&output.stdout).trim().to_owned();
            if let Some(exec) = desktop_exec(&desktop_id, &home) {
                preferred.push(exec);
            }
        }
    }
    if let Some(browser) = env::var("BROWSER")
        .ok()
        .and_then(|s| s.split_whitespace().next().map(str::to_owned))
    {
        preferred.push(browser);
    }
    let config_home = env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".config"));
    let mimeapps = fs::read_to_string(config_home.join("mimeapps.list"))
        .or_else(|_| fs::read_to_string(home.join(".local/share/applications/mimeapps.list")))
        .unwrap_or_default();
    if let Some(desktop_id) = mime_default(&mimeapps) {
        if let Some(exec) = desktop_exec(&desktop_id, &home) {
            preferred.push(exec);
        }
    }
    let browser = select_browser(&preferred, &env::var("PATH").unwrap_or_default(), Path::new("/opt/google/chrome/chrome"))
        .ok_or_else(|| "No supported Chromium-based browser is installed (tried the default browser, Brave, Chromium, Chrome, Vivaldi, and Edge)".to_owned())?;
    Ok(browser_arguments(&browser, &profile_path()?, url, handoff))
}

fn notify_launch_failure(message: &str) {
    let args = [
        "--app-name=Nacre Brain".to_owned(),
        "Nacre Brain".to_owned(),
        message.to_owned(),
    ];
    if let Err(error) = crate::process::run("notify-send", &args, None, Duration::from_secs(3)) {
        eprintln!("nacre-brain: notification failed: {error}");
    }
}

/// Retry authentication for a browser already open during a server upgrade.
pub fn migrate_existing_browser() -> Result<()> {
    if brain_window()?.is_some() {
        bootstrap_existing_browser()?;
    }
    Ok(())
}

pub fn bootstrap_existing_browser() -> Result<()> {
    let state = state()?;
    if !brain_auth::browser_ready(&state, brain_web::port()?)? {
        let handoff = brain_auth::refresh_bootstrap(&state, brain_web::port()?, true)?;
        launch(&browser_command(&handoff, true)?)?;
    }
    Ok(())
}

pub fn ensure_server() -> Result<()> {
    let port = brain_web::port()?;
    let url = format!("http://127.0.0.1:{}/api/health", port);
    if health(&url) {
        return Ok(());
    }
    let executable = env::current_exe()?;
    launch(&[
        executable
            .to_str()
            .ok_or("Brain helper path is not UTF-8")?
            .into(),
        "brain-control".into(),
        "serve".into(),
    ])?;
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        if health(&url) {
            return Ok(());
        }
    }
    Err("Nacre Brain server did not start".into())
}

fn health(url: &str) -> bool {
    state().is_ok_and(|state| health_at(url, &state))
}

fn health_at(url: &str, state: &Path) -> bool {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    let Some(authority) = url
        .strip_prefix("http://127.0.0.1:")
        .and_then(|s| s.split('/').next())
    else {
        return false;
    };
    let Ok(port) = authority.parse::<u16>() else {
        return false;
    };
    let Ok(mut addresses) = ("127.0.0.1", port).to_socket_addrs() else {
        return false;
    };
    let Some(address) = addresses.next() else {
        return false;
    };
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_secs(1)) else {
        return false;
    };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(1)));
    let Ok(nonce) = brain_auth::readiness_challenge() else {
        return false;
    };
    if write!(
        stream,
        "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nX-Nacre-Brain-Nonce: {nonce}\r\nConnection: close\r\n\r\n"
    )
    .is_err()
    {
        return false;
    }
    let mut response = String::new();
    if stream.take(4096).read_to_string(&mut response).is_err()
        || !response.starts_with("HTTP/1.1 200 ")
    {
        return false;
    }
    let Some((head, body)) = response.split_once("\r\n\r\n") else {
        return false;
    };
    let Ok(value) = serde_json::from_str::<Value>(body) else {
        return false;
    };
    if value["ready"] != true || value["authVersion"] != 1 {
        return false;
    }
    let proofs = head
        .lines()
        .filter_map(|line| line.split_once(':'))
        .filter(|(key, _)| key.eq_ignore_ascii_case("X-Nacre-Brain-Proof"))
        .map(|(_, value)| value.trim())
        .collect::<Vec<_>>();
    proofs.len() == 1
        && brain_auth::verify_readiness(state, port, &nonce, proofs[0]).unwrap_or(false)
}

fn ensure_brain(focus: bool) -> Result<()> {
    let state = state()?;
    private_state_dir(&state)?;
    let lock = open_lock(&state.join("brain-launch.lock"))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive)?;
    ensure_server()?;
    let (mut window, should_launch) = window_launch_decision(brain_window, focus, browser_running)?;
    let mut launched = false;
    if should_launch {
        let previous = query("active_workspace")
            .ok()
            .and_then(|v| v.get("id").and_then(Value::as_i64));
        let open = brain_auth::refresh_bootstrap(&state, brain_web::port()?, false)?;
        launch(&browser_command(&open, false)?)?;
        launched = true;
        for _ in 0..100 {
            window = brain_window()?;
            if window.is_some() {
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
        if let Some(win) = &window {
            route_window(win)?
        }
        restore_background_workspace(focus, previous, |workspace| {
            let _ = act(
                "switch_workspace",
                &[json!(workspace)],
                json!({"current_monitor":true}),
            );
        });
    }
    if window.is_some() && !launched {
        bootstrap_existing_browser()?
    }
    if focus {
        if let Some(workspace) = profile_workspace() {
            let _ = run_output(
                &[
                    home()?
                        .join(".local/bin/nacre-shell")
                        .to_string_lossy()
                        .into_owned(),
                    "workspace".into(),
                    workspace.to_string(),
                ],
                None,
                Duration::from_secs(3),
            );
        }
        if let Some(address) = window
            .as_ref()
            .and_then(|w| w.get("address"))
            .and_then(Value::as_str)
        {
            let _ = run_output(
                &[
                    home()?
                        .join(".local/bin/nacre-shell")
                        .to_string_lossy()
                        .into_owned(),
                    "focus".into(),
                    address.into(),
                ],
                None,
                Duration::from_secs(3),
            );
        }
    }
    Ok(())
}

fn route_window(window: &Value) -> Result<()> {
    let address = window
        .get("address")
        .and_then(Value::as_str)
        .ok_or("Brain window has no address")?;
    let mut compositor = compositor()?;
    if !compositor.valid_window_id(&json!(address)) {
        return Ok(());
    }
    let caps = compositor.capabilities();
    clear_fullscreen_if_supported(&caps, || {
        let _ = compositor.action(
            "fullscreen_state",
            &[json!(address)],
            &json!({"internal":0,"client":0}),
        );
    });
    if let Some(workspace) = profile_workspace() {
        let _ = compositor.action(
            "move_window",
            &[json!(address)],
            &json!({"workspace":workspace.to_string(),"follow":false}),
        );
    }
    Ok(())
}

/// Ask the daemon to run the action. Only an absent daemon or one without the Brain
/// methods falls back to the direct launcher; the daemon's own worker sets
/// NACRE_BRAIN_DIRECT so it never calls back.
/// Temporary duplicate: the direct path expires with the legacy non-daemon mode.
fn via_daemon(name: &str) -> Result<bool> {
    use crate::daemon::client::{connect, ConnectError};
    let method = match name {
        "brain" => "brain.open",
        _ => "brain.capture",
    };
    if env::var_os("NACRE_BRAIN_DIRECT").is_some() {
        return Ok(false);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    runtime.block_on(async {
        let mut client = match connect().await {
            Ok(client) => client,
            Err(ConnectError::Absent) => return Ok(false),
            Err(error) => return Err(error.into()),
        };
        if !client.methods.contains_key(method) {
            return Ok(false);
        }
        client.call(method, json!({}), None).await?;
        Ok(true)
    })
}

pub fn action(name: &str, value: &str) -> Result<()> {
    if matches!(name, "brain" | "capture") && via_daemon(name)? {
        return Ok(());
    }
    match name {
        "brain" => ensure_brain(true).map_err(|error| {
            eprintln!("nacre-brain: could not open Brain: {error}");
            notify_launch_failure(&format!("Could not open Brain: {error}"));
            error
        }),
        "capture" => {
            let output = match run_output(
                &[
                    "zenity".into(),
                    "--text-info".into(),
                    "--editable".into(),
                    "--title=Capture a thought".into(),
                    "--width=580".into(),
                    "--height=340".into(),
                ],
                None,
                Duration::from_secs(3600),
            ) {
                Ok(output) if output.status.success() && output.stdout.len() <= 65_536 => output,
                _ => return Ok(()),
            };
            let text = String::from_utf8_lossy(&output.stdout);
            if !text.trim().is_empty() {
                let title = text
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(90)
                    .collect::<String>();
                let args = [
                    "note",
                    "--kind",
                    "personal",
                    "--title",
                    &title,
                    "--source",
                    "User quick capture in Nacre Brain",
                    "--confidence",
                    "reported",
                ]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>();
                let saved = crate::process::run(
                    "nacre-brain",
                    &args,
                    Some(&text),
                    Duration::from_secs(30),
                )?;
                if !saved.status.success() {
                    return Err("Brain capture could not be saved".into());
                }
            }
            Ok(())
        }
        "notes" => launch_app(&[
            "env".into(),
            "NACRE_BRAIN_VIEW=notes".into(),
            "nacre-ai".into(),
            "brain".into(),
        ]),
        "tasks" | "new" | "resume" => launch_app(&[
            home()?
                .join(".local/bin/nacre-shell")
                .to_string_lossy()
                .into_owned(),
            name.into(),
        ]),
        "assign-note" => brain_web::assign_note(value),
        "open-note" => {
            let path = brain_web::note_path(value)?;
            let uri = format!(
                "obsidian://open?path={}",
                percent_encode(&path.to_string_lossy())
            );
            launch_app(&["obsidian".into(), uri])
        }
        _ => Err("Unknown Brain action".into()),
    }
}

pub fn watch_brain() -> Result<()> {
    let state = state()?;
    private_state_dir(&state)?;
    let lock = open_lock(&state.join("brain-watch.lock"))?;
    if rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).is_err() {
        return Ok(());
    }
    loop {
        let _ = ensure_server();
        thread::sleep(Duration::from_secs(3));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fake_ready_json_and_replayed_proofs_do_not_authorize_a_browser_handoff() {
        use std::io::{Read, Write};
        use std::net::TcpListener;
        let fixture = tempfile::tempdir().unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let auth = brain_auth::BrowserAuth::open(fixture.path(), port).unwrap();
        let stale_proof = auth.readiness_proof(&"0".repeat(64)).unwrap();
        let server = std::thread::spawn(move || {
            for proof in [
                String::new(),
                format!("X-Nacre-Brain-Proof: {stale_proof}\r\n"),
            ] {
                let (mut stream, _) = listener.accept().unwrap();
                let mut data = [0; 512];
                assert!(stream.read(&mut data).unwrap() > 0);
                write!(stream,"HTTP/1.1 200 OK\r\n{proof}Content-Length: 30\r\nConnection: close\r\n\r\n{{\"ready\":true,\"authVersion\":1}}").unwrap();
            }
        });
        let url = format!("http://127.0.0.1:{port}/api/health");
        assert!(!health_at(&url, fixture.path()));
        assert!(!health_at(&url, fixture.path()));
        server.join().unwrap();
    }
    #[test]
    fn authenticated_native_readiness_is_accepted() {
        let fixture = tempfile::tempdir().unwrap();
        let state = fixture.path().to_path_buf();
        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let url = format!("http://127.0.0.1:{port}/api/health");
        let server = std::thread::spawn(move || {
            crate::brain_http::serve(listener, &state, Duration::from_millis(300), |_| {
                crate::brain_http::Response::new(
                    200,
                    "application/json",
                    br#"{"ready":true,"authVersion":1}"#.to_vec(),
                )
            })
            .unwrap()
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            if health_at(&url, fixture.path()) {
                break;
            }
            assert!(std::time::Instant::now() < deadline);
            thread::sleep(Duration::from_millis(100));
        }
        server.join().unwrap();
    }

    #[test]
    fn browser_window_identification_reuses_profile_window_and_ignores_other_windows() {
        let pids = [123].into_iter().collect();
        let active = json!({"address":"0x111","pid":123,"class":"chrome-127.0.0.1__-Default","workspace":{"id":3}});
        let preferred = json!({"address":"0x222","pid":123,"class":"chrome-127.0.0.1__-Default","workspace":{"id":6}});
        let unrelated = json!({"address":"0x333","pid":456,"class":"chrome-127.0.0.1__-Default","workspace":{"id":6}});
        assert!(is_brain_window(&active, &pids));
        assert!(is_brain_window(&preferred, &pids));
        assert!(!is_brain_window(&unrelated, &pids));
        let candidates = [active, preferred.clone()];
        assert_eq!(
            preferred_window(&candidates, Some(6))
                .and_then(|w| w["address"].as_str().map(str::to_owned))
                .as_deref(),
            Some("0x222")
        );
        assert_eq!(
            preferred_window(&candidates, Some(6))
                .and_then(|w| w["address"].as_str().map(str::to_owned))
                .as_deref(),
            Some("0x222")
        );
        assert!(!should_launch_browser(false, true, true));
        assert!(!should_launch_browser(false, false, true));
        assert!(should_launch_browser(true, false, true));
    }

    #[test]
    fn browser_default_and_fallback_selection_use_fake_path() {
        let fixture = tempfile::tempdir().unwrap();
        let bin = fixture.path().join("bin");
        let apps = fixture.path().join(".local/share/applications");
        fs::create_dir_all(&bin).unwrap();
        fs::create_dir_all(&apps).unwrap();
        for name in ["brave-origin", "chromium"] {
            let executable = bin.join(name);
            fs::write(&executable, "").unwrap();
            fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::write(
            apps.join("brave-origin.desktop"),
            "[Desktop Entry]\nExec=brave-origin %U\n",
        )
        .unwrap();
        assert_eq!(
            desktop_exec_in_roots("brave-origin.desktop", std::slice::from_ref(&apps)).as_deref(),
            Some("brave-origin")
        );
        let mimeapps = "[Default Applications]\nx-scheme-handler/http=brave-origin.desktop;";
        let mime_id = mime_default(mimeapps).unwrap();
        let preferred = desktop_exec_in_roots(&mime_id, std::slice::from_ref(&apps)).unwrap();
        assert_eq!(
            select_browser(
                std::slice::from_ref(&preferred),
                bin.to_str().unwrap(),
                Path::new("/missing/chrome")
            )
            .unwrap(),
            bin.join("brave-origin")
        );
        assert_eq!(
            select_browser(&[], bin.to_str().unwrap(), Path::new("/missing/chrome")).unwrap(),
            bin.join("brave-origin")
        );
        assert_eq!(
            select_browser(
                &["chromium".into()],
                bin.to_str().unwrap(),
                Path::new("/missing/chrome")
            )
            .unwrap(),
            bin.join("chromium")
        );
        let not_executable = bin.join("brave-origin");
        fs::set_permissions(&not_executable, fs::Permissions::from_mode(0o644)).unwrap();
        fs::set_permissions(bin.join("chromium"), fs::Permissions::from_mode(0o644)).unwrap();
        assert!(select_browser(
            &[preferred],
            bin.to_str().unwrap(),
            Path::new("/missing/chrome")
        )
        .is_none());
        let missing = fixture
            .path()
            .join("missing-brave-origin")
            .to_string_lossy()
            .into_owned();
        let error = launch(std::slice::from_ref(&missing))
            .unwrap_err()
            .to_string();
        assert!(error.contains(&missing), "{error}");
        assert!(error.contains("os error 2"), "{error}");
    }

    #[test]
    fn browser_fallback_order_skips_non_chromium_defaults() {
        let fixture = tempfile::tempdir().unwrap();
        let bin = fixture.path().join("bin");
        fs::create_dir(&bin).unwrap();
        let chrome = fixture.path().join("chrome");
        let candidates = [
            bin.join("brave-origin"),
            bin.join("brave"),
            bin.join("brave-browser"),
            bin.join("chromium"),
            bin.join("google-chrome-stable"),
            chrome.clone(),
            bin.join("vivaldi"),
            bin.join("microsoft-edge"),
        ];
        let firefox = bin.join("firefox");
        for path in candidates.iter().chain(std::iter::once(&firefox)) {
            fs::write(path, "").unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
        }
        for candidate in &candidates {
            assert_eq!(
                select_browser(
                    &["firefox".into(), "missing-browser".into()],
                    bin.to_str().unwrap(),
                    &chrome
                ),
                Some(candidate.clone())
            );
            fs::remove_file(candidate).unwrap();
        }
        assert!(select_browser(&[], bin.to_str().unwrap(), &chrome).is_none());
    }

    #[test]
    fn browser_app_classes_are_scoped_to_profile_processes() {
        let pids = [123].into_iter().collect();
        for class in [
            "brave-127.0.0.1__-Default",
            "brave-origin-127.0.0.1__-Default",
            "chromium-127.0.0.1__-Default",
            "vivaldi-127.0.0.1__-Default",
            "microsoft-edge-127.0.0.1__-Default",
            "msedge-127.0.0.1__-Default",
            "chrome-127.0.0.1__-Default",
        ] {
            assert!(
                is_brain_window(&json!({"pid":123,"class":class}), &pids),
                "{class}"
            );
            assert!(
                !is_brain_window(&json!({"pid":456,"class":class}), &pids),
                "unrelated {class}"
            );
        }
        assert!(select_browser(&[], "/empty", Path::new("/missing/chrome")).is_none());
    }

    #[test]
    fn compositor_query_failure_propagates_without_authorizing_a_launch() {
        let called = std::cell::Cell::new(false);
        let decision = window_launch_decision(
            || Err("compositor unavailable".into()),
            true,
            || {
                called.set(true);
                false
            },
        );
        assert!(decision.is_err());
        assert!(!called.get());
    }

    #[test]
    fn background_launch_restores_previous_workspace_but_foreground_does_not() {
        let mut restored = Vec::new();
        restore_background_workspace(false, Some(4), |workspace| restored.push(workspace));
        restore_background_workspace(true, Some(6), |workspace| restored.push(workspace));
        restore_background_workspace(false, None, |workspace| restored.push(workspace));
        assert_eq!(restored, [4]);
    }

    #[test]
    fn existing_brain_profile_window_prefers_configured_workspace() {
        let pids = [123].into_iter().collect();
        let old = json!({"address":"0x111","pid":123,"class":"chrome-127.0.0.1__-Default","workspace":{"id":3}});
        let preferred = json!({"address":"0x222","pid":123,"class":"chrome-127.0.0.1__-Default","workspace":{"id":6}});
        assert!(is_brain_window(&old, &pids));
        assert!(is_brain_window(&preferred, &pids));
        assert_eq!(
            preferred.pointer("/workspace/id").and_then(Value::as_i64),
            Some(6)
        );
    }

    #[test]
    fn browser_open_url_uses_dedicated_profile_and_private_handoff_class() {
        let open_uri = "http://127.0.0.1:17843/#nacre-bootstrap=token";
        let handoff_uri = "http://127.0.0.1:17843/auth-complete#nacre-bootstrap=token";
        let open = browser_arguments(
            Path::new("/browser/chrome"),
            Path::new("/home/test/nacre/brain-browser"),
            open_uri,
            false,
        );
        let handoff = browser_arguments(
            Path::new("/browser/chrome"),
            Path::new("/home/test/nacre/brain-browser"),
            handoff_uri,
            true,
        );
        assert!(open.iter().any(|arg| arg == "--class=nacre-brain"));
        assert!(handoff.iter().any(|arg| arg == "--class=nacre-brain-auth"));
        assert!(open
            .iter()
            .any(|arg| arg == "--user-data-dir=/home/test/nacre/brain-browser"));
        assert!(open
            .iter()
            .any(|arg| arg == "--app=http://127.0.0.1:17843/#nacre-bootstrap=token"));
        assert!(handoff
            .iter()
            .any(|arg| arg == "--app=http://127.0.0.1:17843/auth-complete#nacre-bootstrap=token"));
    }

    #[test]
    fn obsidian_note_paths_are_uri_encoded() {
        assert_eq!(percent_encode("/vault/a note.md"), "%2Fvault%2Fa%20note.md");
        assert_eq!(percent_encode("a#b?c"), "a%23b%3Fc");
    }

    #[test]
    fn unavailable_fullscreen_capability_is_skipped_for_niri() {
        let capabilities = json!({"fullscreenState":false});
        let calls = std::cell::Cell::new(0);
        clear_fullscreen_if_supported(&capabilities, || calls.set(calls.get() + 1));
        assert_eq!(calls.get(), 0);
        clear_fullscreen_if_supported(&json!({"fullscreenState":true}), || {
            calls.set(calls.get() + 1)
        });
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn linked_state_ancestors_are_rejected() {
        let root = std::env::temp_dir().join(format!("brain-launch-{}", uuid::Uuid::new_v4()));
        let target = root.join("target");
        let linked = root.join("linked");
        fs::create_dir_all(&target).expect("fixture target");
        std::os::unix::fs::symlink(&target, &linked).expect("fixture symlink");
        assert!(reject_linked_ancestors(&linked.join("state")).is_err());
        fs::remove_dir_all(root).expect("cleanup fixture");
    }
}
