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
    drop(crate::process::spawn_detached_quiet(program, args)?);
    Ok(())
}

fn launch_app(argv: &[String]) -> Result<()> {
    let mut command = vec!["systemd-run".into(), "--user".into(), "--scope".into(), "--collect".into(), "--quiet".into(), "env".into(), "-u".into(), "LD_LIBRARY_PATH".into(), "-u".into(), "QML_IMPORT_PATH".into(), "-u".into(), "QT_PLUGIN_PATH".into()];
    command.extend_from_slice(argv);
    launch(&command)
}

fn run_output(argv: &[String], input: Option<&str>, timeout: Duration) -> Result<std::process::Output> {
    let (program, args) = argv.split_first().ok_or("Empty Brain command")?;
    crate::process::run(program, args, input, timeout)
}

fn reject_linked_ancestors(path: &Path) -> Result<()> {
    if !path.is_absolute() { return Err("Brain state path must be absolute".into()) }
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
    compositor()?.query(name, Duration::from_secs(3), None).map_err(Into::into)
}

fn act(name: &str, args: &[Value], kwargs: Value) -> Result<()> {
    compositor()?.action(name, args, &kwargs).map(|_| ()).map_err(Into::into)
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
    let Ok(profile) = profile_path() else { return Default::default() };
    let marker = format!("--user-data-dir={}", profile.display()).into_bytes();
    let mut found = std::collections::BTreeSet::new();
    let Ok(entries) = fs::read_dir("/proc") else { return found };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name.to_str().and_then(|name| name.parse::<u32>().ok()) else { continue };
        let Ok(metadata) = entry.metadata() else { continue };
        if metadata.uid() != rustix::process::geteuid().as_raw() { continue }
        if fs::read(entry.path().join("cmdline")).ok().is_some_and(|args| args.split(|b| *b == 0).any(|arg| arg == marker)) {
            found.insert(pid);
        }
    }
    found
}

fn browser_running() -> bool { !profile_pids().is_empty() }

fn percent_encode(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) { result.push(byte as char); }
        else { result.push_str(&format!("%{byte:02X}")); }
    }
    result
}

fn file_uri(path: &Path) -> Result<String> {
    if !path.is_absolute() { return Err("Browser handoff path must be absolute".into()) }
    Ok(format!("file://{}", percent_encode_path(&path.to_string_lossy())))
}

fn percent_encode_path(value: &str) -> String {
    let mut result = String::new();
    for byte in value.bytes() {
        if byte == b'/' || byte.is_ascii_alphanumeric() || b"-_.~".contains(&byte) {
            result.push(byte as char);
        } else {
            result.push_str(&format!("%{byte:02X}"));
        }
    }
    result
}

fn is_brain_window(window: &Value, pids: &std::collections::BTreeSet<u32>) -> bool {
    let class = window.get("class").and_then(Value::as_str).unwrap_or_default();
    class == "nacre-brain" || (window.get("pid").and_then(Value::as_u64).is_some_and(|pid| pids.contains(&(pid as u32))) && (class == "chrome-127.0.0.1__-Default" || class.contains("nacre_brain_auth_open.html")))
}

fn preferred_window(candidates: &[Value], workspace: Option<i64>) -> Option<Value> {
    workspace
        .and_then(|workspace| candidates.iter().find(|w| w.pointer("/workspace/id").and_then(Value::as_i64) == Some(workspace)))
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
    if capabilities.pointer("/fullscreenState").and_then(Value::as_bool).unwrap_or(true) {
        clear();
    }
}

fn brain_window() -> Result<Option<Value>> {
    let windows = query("windows")?;
    let windows = windows.as_array().ok_or("Invalid compositor window list")?;
    let pids = profile_pids();
    let candidates: Vec<_> = windows.iter().filter(|w| is_brain_window(w, &pids)).cloned().collect();
    Ok(preferred_window(&candidates, profile_workspace()))
}

fn browser_arguments(browser: &Path, profile: &Path, app_uri: &str, handoff: bool) -> Vec<String> {
    vec![
        browser.to_string_lossy().into_owned(),
        "--no-first-run".into(),
        "--no-default-browser-check".into(),
        "--ozone-platform=wayland".into(),
        format!("--class={}", if handoff { "nacre-brain-auth" } else { "nacre-brain" }),
        format!("--user-data-dir={}", profile.display()),
        format!("--app={app_uri}"),
    ]
}

fn browser_command(path: &Path, handoff: bool) -> Result<Vec<String>> {
    let candidates = [Some(PathBuf::from("/opt/google/chrome/chrome")), which("chromium")];
    let browser = candidates.into_iter().flatten().find(|path| path.exists()).ok_or("Chrome or Chromium is required for the knowledge browser")?;
    Ok(browser_arguments(&browser, &profile_path()?, &file_uri(path)?, handoff))
}

fn which(program: &str) -> Option<PathBuf> {
    env::var_os("PATH")?.to_string_lossy().split(':').map(|part| Path::new(part).join(program)).find(|path| path.is_file())
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
    if health(&url) { return Ok(()) }
    let executable = env::current_exe()?;
    drop(crate::process::spawn_detached_quiet(
        executable.to_str().ok_or("Brain helper path is not UTF-8")?,
        &["brain-control".into(), "serve".into()],
    )?);
    for _ in 0..30 {
        thread::sleep(Duration::from_millis(100));
        if health(&url) { return Ok(()) }
    }
    Err("Nacre Brain server did not start".into())
}

fn health(url: &str) -> bool {
    use std::io::{Read, Write};
    use std::net::{TcpStream, ToSocketAddrs};
    let Some(authority) = url.strip_prefix("http://127.0.0.1:").and_then(|s| s.split('/').next()) else { return false };
    let Ok(port) = authority.parse::<u16>() else { return false };
    let Ok(mut addresses) = ("127.0.0.1", port).to_socket_addrs() else { return false };
    let Some(address) = addresses.next() else { return false };
    let Ok(mut stream) = TcpStream::connect_timeout(&address, Duration::from_secs(1)) else { return false };
    let _ = stream.set_read_timeout(Some(Duration::from_secs(1)));
    if write!(stream, "GET /api/health HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nConnection: close\r\n\r\n", port).is_err() { return false }
    let mut response = String::new();
    stream.take(4096).read_to_string(&mut response).is_ok() && response.starts_with("HTTP/1.1 200") && response.contains("\"ready\":true") && response.contains("\"authVersion\":1")
}

fn ensure_brain(focus: bool) -> Result<()> {
    let state = state()?;
    private_state_dir(&state)?;
    let lock = open_lock(&state.join("brain-launch.lock"))?;
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::LockExclusive)?;
    ensure_server()?;
    let (mut window, should_launch) = window_launch_decision(|| brain_window(), focus, browser_running)?;
    if should_launch {
        let previous = query("active_workspace").ok().and_then(|v| v.get("id").and_then(Value::as_i64));
        let open = brain_auth::refresh_bootstrap(&state, brain_web::port()?, false)?;
        launch(&browser_command(&open, false)?)?;
        for _ in 0..100 {
            window = brain_window()?;
            if window.is_some() { break }
            thread::sleep(Duration::from_millis(100));
        }
        if let Some(win) = &window { route_window(win)? }
        restore_background_workspace(focus, previous, |workspace| {
            let _ = act("switch_workspace", &[json!(workspace)], json!({"current_monitor":true}));
        });
    }
    if window.is_none() { return Ok(()) }
    if window.is_some() { bootstrap_existing_browser()? }
    if focus {
        if let Some(workspace) = profile_workspace() { let _ = run_output(&[home()?.join(".local/bin/nacre-shell").to_string_lossy().into_owned(), "workspace".into(), workspace.to_string()], None, Duration::from_secs(3)); }
        if let Some(address) = window.as_ref().and_then(|w| w.get("address")).and_then(Value::as_str) {
            let _ = run_output(&[home()?.join(".local/bin/nacre-shell").to_string_lossy().into_owned(), "focus".into(), address.into()], None, Duration::from_secs(3));
        }
    }
    Ok(())
}

fn route_window(window: &Value) -> Result<()> {
    let address = window.get("address").and_then(Value::as_str).ok_or("Brain window has no address")?;
    let mut compositor = compositor()?;
    if !compositor.valid_window_id(&json!(address)) { return Ok(()) }
    let caps = compositor.capabilities();
    clear_fullscreen_if_supported(&caps, || {
        let _ = compositor.action("fullscreen_state", &[json!(address)], &json!({"internal":0,"client":0}));
    });
    if let Some(workspace) = profile_workspace() {
        let _ = compositor.action("move_window", &[json!(address)], &json!({"workspace":workspace.to_string(),"follow":false}));
    }
    Ok(())
}

pub fn action(name: &str, value: &str) -> Result<()> {
    match name {
        "brain" | "open" => ensure_brain(true),
        "capture" => {
            let output = match run_output(&["zenity".into(), "--text-info".into(), "--editable".into(), "--title=Capture a thought".into(), "--width=580".into(), "--height=340".into()], None, Duration::from_secs(3600)) {
                Ok(output) if output.status.success() && output.stdout.len() <= 65_536 => output,
                _ => return Ok(()),
            };
            let text = String::from_utf8_lossy(&output.stdout);
            if !text.trim().is_empty() {
                let title = text.lines().next().unwrap_or_default().chars().take(90).collect::<String>();
                let args = ["note", "--kind", "personal", "--title", &title, "--source", "User quick capture in Nacre Brain", "--confidence", "reported"].into_iter().map(str::to_owned).collect::<Vec<_>>();
                let saved = crate::process::run("nacre-brain", &args, Some(&text), Duration::from_secs(30))?;
                if !saved.status.success() { return Err("Brain capture could not be saved".into()) }
            }
            Ok(())
        }
        "notes" => launch_app(&["env".into(), "NACRE_BRAIN_VIEW=notes".into(), "nacre-ai".into(), "brain".into()]),
        "tasks" | "new" | "resume" => launch_app(&[home()?.join(".local/bin/nacre-shell").to_string_lossy().into_owned(), name.into()]),
        "assign-note" => brain_web::assign_note(value),
        "open-note" => {
            let path = brain_web::note_path(value)?;
            let uri = format!("obsidian://open?path={}", percent_encode(&path.to_string_lossy()));
            launch_app(&["obsidian".into(), uri])
        }
        _ => Err("Unknown Brain action".into()),
    }
}

pub fn watch_brain() -> Result<()> {
    let state = state()?;
    private_state_dir(&state)?;
    let lock = open_lock(&state.join("brain-watch.lock"))?;
    if rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive).is_err() { return Ok(()) }
    loop {
        let _ = ensure_server();
        thread::sleep(Duration::from_secs(3));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(preferred_window(&candidates, Some(6)).and_then(|w| w["address"].as_str().map(str::to_owned)).as_deref(), Some("0x222"));
        assert_eq!(preferred_window(&candidates, Some(6)).and_then(|w| w["address"].as_str().map(str::to_owned)).as_deref(), Some("0x222"));
        assert!(!should_launch_browser(false, true, true));
        assert!(!should_launch_browser(false, false, true));
        assert!(should_launch_browser(true, false, true));
    }

    #[test]
    fn compositor_query_failure_propagates_without_authorizing_a_launch() {
        let called = std::cell::Cell::new(false);
        let decision = window_launch_decision(
            || Err("compositor unavailable".into()),
            true,
            || { called.set(true); false },
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
        assert_eq!(preferred.pointer("/workspace/id").and_then(Value::as_i64), Some(6));
    }

    #[test]
    fn browser_open_url_uses_dedicated_profile_and_private_handoff_class() {
        let open_uri = file_uri(Path::new("/tmp/Brain Home/open page.html")).expect("absolute app file URI");
        let handoff_uri = file_uri(Path::new("/tmp/handoff.html")).expect("absolute handoff URI");
        let open = browser_arguments(Path::new("/browser/chrome"), Path::new("/home/test/nacre/brain-browser"), &open_uri, false);
        let handoff = browser_arguments(Path::new("/browser/chrome"), Path::new("/home/test/nacre/brain-browser"), &handoff_uri, true);
        assert!(open.iter().any(|arg| arg == "--class=nacre-brain"));
        assert!(handoff.iter().any(|arg| arg == "--class=nacre-brain-auth"));
        assert!(open.iter().any(|arg| arg == "--user-data-dir=/home/test/nacre/brain-browser"));
        assert!(open.iter().any(|arg| arg == "--app=file:///tmp/Brain%20Home/open%20page.html"));
        assert!(handoff.iter().any(|arg| arg == "--app=file:///tmp/handoff.html"));
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
        clear_fullscreen_if_supported(&json!({"fullscreenState":true}), || calls.set(calls.get() + 1));
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
