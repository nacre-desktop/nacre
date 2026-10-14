//! Existing user-selected menu actions; native APIs and actual external tools.
use crate::{
    compositor::{CaptureWindow, Compositor},
    files,
    i18n::tr,
    paths::Paths,
    process, systemd, Result,
};
use serde_json::json;
use std::{
    fs,
    os::unix::{
        fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
        process::ExitStatusExt,
    },
    path::{Path, PathBuf},
    process::Output,
    time::{Duration, Instant},
};

type ToolRunner<'a> = dyn FnMut(&str, &[String], Option<&[u8]>, Duration) -> Result<Output> + 'a;

pub fn run_terminal_tool(name: &str) -> Result<i32> {
    let command: Vec<String> = if name == "logs" {
        vec![
            "journalctl".into(),
            "--user".into(),
            "-u".into(),
            "nacre-shell.service".into(),
            "--since".into(),
            "30 minutes ago".into(),
            "--no-pager".into(),
        ]
    } else {
        vec![
            "bash".into(),
            Paths::environment()?
                .assets()
                .join("updates.sh")
                .to_str()
                .ok_or("Update script path is not UTF-8")?
                .into(),
        ]
    };
    let status = std::process::Command::new(&command[0])
        .args(&command[1..])
        .status()?;
    Ok(status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(1)))
}

fn screenshot_path(paths: &Paths, date: &str) -> Result<PathBuf> {
    let folder = paths.screenshots();
    let parent = folder.parent().ok_or("Missing screenshot parent")?;
    for directory in [parent, folder.as_path()] {
        match fs::symlink_metadata(directory) {
            Ok(metadata) if !metadata.is_dir() || metadata.file_type().is_symlink() => {
                return Err("Screenshot destination is linked or not a directory".into())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::DirBuilder::new()
                    .mode(if directory == folder { 0o700 } else { 0o755 })
                    .create(directory)?;
            }
            Err(error) => return Err(error.into()),
        }
    }
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o700))?;
    let path = folder.join(format!("screenshot_{date}.png"));
    // Reserve the filename privately before either native capture or grim writes.
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    Ok(path)
}
fn capture_message() -> String {
    tr(
        "helper.menu-backend.window-saved-and-copied",
        "Window saved and copied",
    )
}
fn focus_error() -> crate::Error {
    tr(
        "helper.menu-backend.focus-the-window-you-want-to-capture-first",
        "Focus the window you want to capture first",
    )
    .into()
}
fn checked_tool(
    run: &mut ToolRunner<'_>,
    name: &str,
    args: &[String],
    input: Option<&[u8]>,
) -> Result<Output> {
    let output = run(name, args, input, Duration::from_secs(5))?;
    if !output.status.success() {
        return Err(format!("{name} failed: {}", output.status).into());
    }
    Ok(output)
}

fn window_with(
    paths: &Paths,
    date: &str,
    window: CaptureWindow,
    run: &mut ToolRunner<'_>,
    native: impl FnOnce(&Path, u64) -> Result<()>,
) -> Result<String> {
    match window {
        CaptureWindow::Niri(id) => {
            let id = id.ok_or_else(focus_error)?;
            let path = screenshot_path(paths, date)?;
            native(&path, id)?;
        }
        CaptureWindow::Hyprland(current) => {
            let position = current["at"]
                .as_array()
                .filter(|v| v.len() == 2)
                .ok_or_else(focus_error)?;
            let size = current["size"]
                .as_array()
                .filter(|v| v.len() == 2)
                .ok_or_else(focus_error)?;
            let mut coordinates = Vec::new();
            for item in position.iter().chain(size.iter()) {
                // bool and floating values are not Python integers.
                coordinates.push(item.as_i64().ok_or_else(focus_error)?);
            }
            if coordinates[2..].iter().any(|v| *v <= 0 || *v > 32768) {
                return Err(focus_error());
            }
            let path = screenshot_path(paths, date)?;
            let name = path
                .to_str()
                .ok_or("Screenshot path is not UTF-8")?
                .to_owned();
            let geometry = format!(
                "{},{} {}x{}",
                coordinates[0], coordinates[1], coordinates[2], coordinates[3]
            );
            checked_tool(run, "grim", &["-g".into(), geometry, name], None)?;
            checked_tool(
                run,
                "wl-copy",
                &["--type".into(), "image/png".into()],
                Some(&fs::read(path)?),
            )?;
        }
    }
    Ok(capture_message())
}

fn await_capture(
    path: &Path,
    capture: impl FnOnce() -> Result<()>,
    timeout: Duration,
) -> Result<()> {
    use rustix::{
        event::{poll, PollFd, PollFlags, Timespec},
        fs::inotify,
    };
    use std::{mem::MaybeUninit, os::unix::ffi::OsStrExt};
    let watcher = inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK)?;
    inotify::add_watch(
        &watcher,
        path.parent().ok_or("Missing screenshot folder")?,
        inotify::WatchFlags::CLOSE_WRITE | inotify::WatchFlags::MOVED_TO,
    )?;
    capture()?;
    let deadline = Instant::now() + timeout;
    let mut bytes = [MaybeUninit::uninit(); 4096];
    let mut reader = inotify::Reader::new(&watcher, &mut bytes);
    loop {
        while !reader.is_buffer_empty() {
            let event = reader.next()?;
            if event.events().contains(inotify::ReadFlags::QUEUE_OVERFLOW) {
                return Err("Window capture event queue overflowed".into());
            }
            if event
                .file_name()
                .is_some_and(|name| Some(name.to_bytes()) == path.file_name().map(|n| n.as_bytes()))
                && path.is_file()
            {
                return Ok(());
            }
        }
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("Window capture did not finish")?;
        let timeout = Timespec {
            tv_sec: remaining.as_secs().try_into()?,
            tv_nsec: remaining.subsec_nanos().into(),
        };
        let mut descriptors = [PollFd::new(&watcher, PollFlags::IN)];
        match poll(&mut descriptors, Some(&timeout)) {
            Ok(0) => return Err("Window capture did not finish".into()),
            Ok(_) => {
                let event = reader.next()?;
                if event.events().contains(inotify::ReadFlags::QUEUE_OVERFLOW) {
                    return Err("Window capture event queue overflowed".into());
                }
                if event.file_name().is_some_and(|name| {
                    Some(name.to_bytes()) == path.file_name().map(|n| n.as_bytes())
                }) && path.is_file()
                {
                    return Ok(());
                }
            }
            Err(rustix::io::Errno::INTR) => {}
            Err(error) => return Err(error.into()),
        }
    }
}

fn screenshot_with(
    paths: &Paths,
    action: &str,
    date: &str,
    run: &mut ToolRunner<'_>,
) -> Result<String> {
    if action == "capture-area" {
        let selected = run("slurp", &[], None, Duration::from_secs(180))?;
        let geometry = String::from_utf8(selected.stdout)?;
        let geometry = geometry
            .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
        if !selected.status.success() || geometry.is_empty() {
            return Ok(String::new());
        }
        let image = checked_tool(
            run,
            "grim",
            &["-g".into(), geometry.into(), "-".into()],
            None,
        )?
        .stdout;
        checked_tool(
            run,
            "wl-copy",
            &["--type".into(), "image/png".into()],
            Some(&image),
        )?;
        let path = screenshot_path(paths, date)?;
        let image = checked_tool(
            run,
            "wl-paste",
            &["--type".into(), "image/png".into()],
            None,
        )?
        .stdout;
        fs::write(path, image)?;
        Ok("Selection saved and copied".into())
    } else {
        let path = screenshot_path(paths, date)?;
        checked_tool(
            run,
            "grim",
            &[path.to_str().ok_or("Screenshot path is not UTF-8")?.into()],
            None,
        )?;
        checked_tool(
            run,
            "wl-copy",
            &["--type".into(), "image/png".into()],
            Some(&fs::read(path)?),
        )?;
        Ok(tr(
            "helper.screenshot.saved-to-pictures-screenshots-and-copied-to-clipboard",
            "Saved to Pictures/Screenshots and copied to clipboard",
        ))
    }
}

pub fn capture(action: &str) -> Result<String> {
    let paths = Paths::environment()?;
    let date = chrono::Local::now().format("%Y%m%d_%H%M%S_%6f").to_string();
    if action == "capture-window" {
        let mut compositor = Compositor::connect()?;
        let window = compositor.active_window()?;
        window_with(
            &paths,
            &date,
            window,
            &mut process::run_bytes,
            |path, id| {
                await_capture(
                    path,
                    || compositor.screenshot_window(path, id),
                    Duration::from_secs(5),
                )
            },
        )
    } else {
        screenshot_with(&paths, action, &date, &mut process::run_bytes)
    }
}

pub fn support_report() -> Result<String> {
    let report = crate::diagnostics_native::report(false)?;
    copy_report(&report, &mut process::run_bytes)
}
fn copy_report(report: &str, run: &mut ToolRunner<'_>) -> Result<String> {
    checked_tool(
        run,
        "wl-copy",
        &["--type".into(), "text/plain;charset=utf-8".into()],
        Some(report.as_bytes()),
    )?;
    Ok("Support report copied".into())
}

#[cfg(test)]
mod report_tests {
    use super::*;
    #[test]
    fn support_report_copies_only_redacted_markdown_and_surfaces_failure() -> Result<()> {
        for code in [0, 7] {
            let result = copy_report(
                "# Redacted fixture\n",
                &mut |program, args, input, timeout| {
                    assert_eq!(program, "wl-copy");
                    assert_eq!(args, ["--type", "text/plain;charset=utf-8"]);
                    assert_eq!(input, Some(b"# Redacted fixture\n".as_slice()));
                    assert_eq!(timeout, Duration::from_secs(5));
                    Ok(Output {
                        status: std::process::ExitStatus::from_raw(code * 256),
                        stdout: vec![],
                        stderr: vec![],
                    })
                },
            );
            if code == 0 {
                assert_eq!(result?, "Support report copied");
            } else {
                assert!(result.is_err());
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;
    use serde_json::Value;
    use std::{cell::RefCell, os::unix::process::ExitStatusExt};

    #[test]
    fn frozen_python_capture_calls_files_and_results() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/menu-capture-reference.json"
        ))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing capture cases")?
        {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            let action = case["action"].as_str().ok_or("Missing capture action")?;
            let calls = RefCell::new(Vec::new());
            let normalize =
                |value: &str| value.replace(&home.path().to_string_lossy().into_owned(), "{home}");
            let mut run = |program: &str,
                           args: &[String],
                           input: Option<&[u8]>,
                           timeout: Duration| {
                assert_eq!(
                    timeout,
                    Duration::from_secs(if program == "slurp" { 180 } else { 5 })
                );
                let capture_output = program == "slurp"
                    || program == "wl-paste"
                    || (program == "grim" && args.last().is_some_and(|a| a == "-"));
                calls.borrow_mut().push(json!({"argv":std::iter::once(program.to_owned()).chain(args.iter().map(|a| normalize(a))).collect::<Vec<_>>(), "input":input.unwrap_or_default().iter().map(|b| format!("{b:02x}")).collect::<String>(), "check":program != "slurp", "capture_output":capture_output, "text":program == "slurp"}));
                if program == "grim" && args.last().is_some_and(|a| a != "-") {
                    fs::write(args.last().ok_or("Missing capture path")?, b"fixture png")?;
                }
                let stdout = if program == "slurp" {
                    case["selection"]
                        .as_str()
                        .ok_or("Missing selection")?
                        .as_bytes()
                        .to_vec()
                } else if program == "grim" || program == "wl-paste" {
                    b"fixture png".to_vec()
                } else {
                    vec![]
                };
                Ok(Output {
                    status: std::process::ExitStatus::from_raw(if program == "slurp" {
                        case["code"].as_i64().ok_or("Missing status")? as i32 * 256
                    } else {
                        0
                    }),
                    stdout,
                    stderr: vec![],
                })
            };
            let result = if action == "capture-window" {
                calls
                    .borrow_mut()
                    .push(json!({"query":"active_window","timeout":5}));
                let window = if case["native"] == true {
                    CaptureWindow::Niri(case["current"]["windowId"].as_str().map(|_| 42))
                } else {
                    CaptureWindow::Hyprland(case["current"].clone())
                };
                window_with(
                    &paths,
                    "20240102_030405_678901",
                    window,
                    &mut run,
                    |path, id| {
                        calls.borrow_mut().push(json!({"path":normalize(path.to_str().ok_or("Invalid capture path")?), "window":format!("niri:{id}"), "timeout":5}));
                        fs::write(path, b"fixture png")?;
                        Ok(())
                    },
                )
            } else {
                screenshot_with(&paths, action, "20240102_030405_678901", &mut run)
            };
            let outcome = match result {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":"ValueError","message":error.to_string()}),
            };
            assert_eq!(outcome, case["outcome"], "{action}");
            assert_eq!(json!(calls.into_inner()), case["calls"], "{action}");
            let mut files = serde_json::Map::new();
            for item in walkdir::WalkDir::new(home.path()) {
                let item = item?;
                if item.file_type().is_file() {
                    files.insert(item.path().strip_prefix(home.path())?.to_str().ok_or("Invalid fixture path")?.into(), json!({"bytes":fs::read(item.path())?.iter().map(|b| format!("{b:02x}")).collect::<String>(), "mode":format!("0o{:o}", fs::metadata(item.path())?.permissions().mode() & 0o777)}));
                }
            }
            let mut expected = case["files"].clone();
            for file in expected
                .as_object_mut()
                .ok_or("Missing reference files")?
                .values_mut()
            {
                assert_eq!(file["mode"], "0o644", "Frozen Python permission changed");
                file["mode"] = json!("0o600");
            }
            assert_eq!(Value::Object(files), expected, "{action}");
            if paths.screenshots().exists() {
                assert_eq!(
                    fs::metadata(paths.screenshots())?.permissions().mode() & 0o777,
                    0o700
                );
            }
        }
        Ok(())
    }
    #[test]
    fn capture_completion_uses_close_or_rename_events_and_a_deadline() -> Result<()> {
        let home = tempfile::tempdir()?;
        let path = home.path().join("capture.png");
        await_capture(
            &path,
            || {
                fs::write(&path, b"finished")?;
                Ok(())
            },
            Duration::from_secs(1),
        )?;
        fs::remove_file(&path)?;
        await_capture(
            &path,
            || {
                let temporary = home.path().join("capture.next");
                fs::write(&temporary, b"finished")?;
                fs::rename(temporary, &path)?;
                Ok(())
            },
            Duration::from_secs(1),
        )?;
        fs::remove_file(&path)?;
        let error = await_capture(
            &path,
            || {
                fs::write(home.path().join("unrelated.png"), b"unrelated")?;
                Ok(())
            },
            Duration::from_millis(25),
        )
        .expect_err("Unrelated completion must not finish capture");
        assert_eq!(error.to_string(), "Window capture did not finish");
        Ok(())
    }
    #[test]
    fn screenshots_repair_leaf_modes_and_refuse_links_or_collisions() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(paths.screenshots())?;
        fs::set_permissions(paths.screenshots(), fs::Permissions::from_mode(0o755))?;
        let path = screenshot_path(&paths, "fixture")?;
        assert_eq!(fs::metadata(&path)?.permissions().mode() & 0o777, 0o600);
        assert_eq!(
            fs::metadata(paths.screenshots())?.permissions().mode() & 0o777,
            0o700
        );
        fs::write(&path, b"keep existing screenshot")?;
        assert!(screenshot_path(&paths, "fixture").is_err());
        assert_eq!(fs::read(&path)?, b"keep existing screenshot");
        fs::remove_file(path)?;
        fs::remove_dir(paths.screenshots())?;
        let other = tempfile::tempdir()?;
        std::os::unix::fs::symlink(other.path(), paths.screenshots())?;
        assert!(screenshot_path(&paths, "fixture").is_err());
        assert_eq!(fs::read_dir(other.path())?.count(), 0);
        Ok(())
    }
}

fn which(name: &str) -> bool {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|root| {
        let path = root.join(name);
        path.is_file() && fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    })
}

// Python shlex.split(comments=False): these words are argv, never shell code.
fn editor_words(value: &str) -> Result<Vec<String>> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quote = None;
    let mut started = false;
    let mut chars = value.chars();
    while let Some(c) = chars.next() {
        match quote {
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    word.push(c);
                }
            }
            Some('"') => match c {
                '"' => quote = None,
                '\\' => {
                    let next = chars.next().ok_or("No escaped character")?;
                    if next != '"' && next != '\\' {
                        word.push('\\');
                    }
                    word.push(next);
                }
                c => word.push(c),
            },
            _ => match c {
                ' ' | '\t' | '\r' | '\n' => {
                    if started {
                        words.push(std::mem::take(&mut word));
                        started = false;
                    }
                }
                '\'' | '"' => {
                    quote = Some(c);
                    started = true;
                }
                '\\' => {
                    word.push(chars.next().ok_or("No escaped character")?);
                    started = true;
                }
                c => {
                    word.push(c);
                    started = true;
                }
            },
        }
    }
    if quote.is_some() {
        return Err("No closing quotation".into());
    }
    if started {
        words.push(word);
    }
    Ok(words)
}

fn configuration_command(
    paths: &Paths,
    route: &str,
    editor: &str,
    mut find: impl FnMut(&str) -> bool,
) -> Result<Vec<String>> {
    let name = match route {
        "defaults" => "apps.json",
        "shortcuts" => "shortcuts.lua",
        _ => {
            return Err(tr(
                "helper.menu-backend.unknown-configuration-route",
                "Unknown configuration route",
            )
            .into())
        }
    };
    let path = paths.config.join(name);
    if !path.exists() {
        let content = if route == "defaults" {
            let defaults = json!({"terminal":["kitty"],"browser":["google-chrome-stable"],"files":["nacre-shell","dolphin"],"mail":["google-chrome-stable","--app=https://mail.google.com/"],"emoji":["nacre-menu","actions.emoji"]});
            serde_json::to_string_pretty(&defaults)? + "\n"
        } else {
            "-- Personal Hyprland shortcut overrides.\n".into()
        };
        files::atomic(&path, content.as_bytes())?;
    }
    let preferred = editor_words(editor)?;
    let mut command = if preferred.first().is_some_and(|name| find(name)) {
        preferred
    } else {
        ["nvim","nano","vim"].into_iter().find(|name|find(name)).map(|name|vec![name.to_string()])
            .ok_or_else(||tr("helper.menu-backend.install-a-text-editor-such-as-neovim-to-edit-configuration","Install a text editor such as Neovim to edit configuration"))?
    };
    command.push(
        path.to_str()
            .ok_or("Configuration path is not UTF-8")?
            .into(),
    );
    Ok(command)
}

pub fn edit_config(route: &str) -> Result<i32> {
    let editor = match std::env::var("EDITOR") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => String::new(),
        Err(error) => return Err(error.into()),
    };
    let command = configuration_command(&Paths::environment()?, route, &editor, which)?;
    let status = std::process::Command::new(&command[0])
        .args(&command[1..])
        .status()?;
    Ok(status
        .code()
        .unwrap_or_else(|| -status.signal().unwrap_or(1)))
}

pub fn read_screen(kind: &str) -> Result<String> {
    screen_with(
        &Paths::environment()?,
        kind,
        &mut process::run_bytes,
        |cache| {
            let temporary = tempfile::Builder::new().suffix(".png").tempfile_in(cache)?;
            let (file, path) = temporary.keep()?;
            drop(file);
            Ok(path)
        },
    )
}

trait Units {
    fn owner(&mut self, name: &str) -> Result<String>;
    fn start(&mut self, name: &str, args: &[String]) -> Result<()>;
    fn stop(&mut self, name: &str, owner: &str) -> Result<()>;
    fn finish_recording(&mut self, name: &str, owner: &str) -> Result<()>;
    fn active(&mut self, name: &str) -> Result<bool> {
        Ok(!self.owner(name)?.is_empty())
    }
    fn wait_owner(&mut self, name: &str) -> Result<String> {
        self.owner(name)
    }
}
struct NativeUnits;
impl Units for NativeUnits {
    fn owner(&mut self, name: &str) -> Result<String> {
        Ok(systemd::status(name)?.owner())
    }
    fn active(&mut self, name: &str) -> Result<bool> {
        Ok(systemd::status(name)?.active)
    }
    fn wait_owner(&mut self, name: &str) -> Result<String> {
        systemd::wait_owner(name)
    }
    fn start(&mut self, name: &str, args: &[String]) -> Result<()> {
        systemd::start(name, args)
    }
    fn stop(&mut self, name: &str, owner: &str) -> Result<()> {
        if systemd::status(name)?.owner() != owner {
            return Err("Stay awake belongs to another controller".into());
        }
        systemd::stop(name)
    }
    fn finish_recording(&mut self, name: &str, owner: &str) -> Result<()> {
        systemd::interrupt_and_wait(name, owner)
    }
}
fn record(paths: &Paths, name: &str) -> Result<serde_json::Value> {
    let path = paths.state.join(format!("menu/{name}.json"));
    match fs::read(&path) {
        Ok(bytes) => match serde_json::from_slice(&bytes) {
            Ok(value) => Ok(value),
            Err(error) => {
                eprintln!("Menu state {name}: {error}");
                Ok(json!({}))
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(json!({})),
        Err(error) => {
            eprintln!("Menu state {name}: {error}");
            Ok(json!({}))
        }
    }
}
pub fn record_stop() -> Result<String> {
    record_stop_with(&Paths::environment()?, &mut NativeUnits)
}
pub fn record_start() -> Result<String> {
    record_start_with(
        &Paths::environment()?,
        &mut NativeUnits,
        &mut process::run_bytes,
        || chrono::Local::now().format("%Y%m%d_%H%M%S_%6f").to_string(),
    )
}
fn record_start_with(
    paths: &Paths,
    units: &mut impl Units,
    run: &mut impl FnMut(&str, &[String], Option<&[u8]>, Duration) -> Result<Output>,
    stamp: impl FnOnce() -> String,
) -> Result<String> {
    let _guard = crate::menu_registry::registry_lock(paths, "recording")?;
    if units.active("nacre-screen-recording.service")? {
        return Err(tr(
            "helper.menu-backend.a-nacre-recording-is-already-running",
            "A Nacre recording is already running",
        )
        .into());
    }
    let selected = run("slurp", &[], None, Duration::from_secs(180))?;
    let geometry = String::from_utf8(selected.stdout)?
        .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
        .to_string();
    if !selected.status.success() || geometry.is_empty() {
        return Ok(String::new());
    }
    let folder = paths.recordings();
    fs::create_dir_all(&folder)?;
    let path = folder.join(format!("recording_{}.mp4", stamp()));
    units.start(
        "nacre-screen-recording.service",
        &[
            "wf-recorder".into(),
            "-g".into(),
            geometry,
            "-f".into(),
            path.to_str().ok_or("Recording path is not UTF-8")?.into(),
        ],
    )?;
    let owner = units.wait_owner("nacre-screen-recording.service")?;
    if owner.is_empty() {
        return Err(tr(
            "helper.menu-backend.recording-could-not-start",
            "Recording could not start",
        )
        .into());
    }
    files::json_write(
        &paths.state.join("menu/recording.json"),
        &json!({"owner":owner,"path":path}),
    )?;
    crate::menu_registry::invalidate(paths)?;
    Ok(tr(
        "helper.menu-backend.recording-started-use-actions-capture-stop-recording",
        "Recording started; use Actions › Capture › Stop recording",
    ))
}
fn record_stop_with(paths: &Paths, units: &mut impl Units) -> Result<String> {
    let _guard = crate::menu_registry::registry_lock(paths, "recording")?;
    let data = record(paths, "recording")?;
    let owner = units.owner("nacre-screen-recording.service")?;
    if owner.is_empty() {
        return Ok(tr(
            "helper.menu-backend.no-nacre-recording-is-running",
            "No Nacre recording is running",
        ));
    }
    if !data.is_object() || data["owner"] != owner {
        return Err(tr(
            "helper.menu-backend.this-recording-belongs-to-another-controller",
            "This recording belongs to another controller",
        )
        .into());
    }
    units
        .finish_recording("nacre-screen-recording.service", &owner)
        .map_err(|error| -> crate::Error {
            if error.to_string() == "Recording is still finishing; try again shortly" {
                tr(
                    "helper.menu-backend.recording-is-still-finishing-try-again-shortly",
                    "Recording is still finishing; try again shortly",
                )
                .into()
            } else {
                error
            }
        })?;
    crate::menu_registry::invalidate(paths)?;
    Ok(tr(
        "helper.menu-backend.recording-saved-in-videos-recordings",
        "Recording saved in Videos/Recordings",
    ))
}
pub fn stay_awake() -> Result<String> {
    stay_awake_with(&Paths::environment()?, &mut NativeUnits)
}
fn stay_awake_with(paths: &Paths, units: &mut impl Units) -> Result<String> {
    let _guard = crate::menu_registry::registry_lock(paths, "stay-awake")?;
    let owner = units.owner("nacre-stay-awake.service")?;
    let data = record(paths, "stay-awake")?;
    let message = if !owner.is_empty() {
        if !data.is_object() || data["owner"] != owner {
            return Err(tr(
                "helper.menu-backend.stay-awake-belongs-to-another-controller",
                "Stay awake belongs to another controller",
            )
            .into());
        }
        units.stop("nacre-stay-awake.service", &owner)?;
        "Normal idle behavior restored"
    } else {
        units.start(
            "nacre-stay-awake.service",
            &[
                "systemd-inhibit".into(),
                "--what=idle".into(),
                "--mode=block".into(),
                "--who=Nacre".into(),
                "--why=Nacre stay awake".into(),
                "sleep".into(),
                "infinity".into(),
            ],
        )?;
        let owner = units.wait_owner("nacre-stay-awake.service")?;
        if owner.is_empty() {
            return Err(tr(
                "helper.menu-backend.stay-awake-could-not-start",
                "Stay awake could not start",
            )
            .into());
        }
        files::json_write(
            &paths.state.join("menu/stay-awake.json"),
            &json!({"owner":owner}),
        )?;
        "Stay awake enabled until you turn it off or log out"
    };
    crate::menu_registry::invalidate(paths)?;
    Ok(message.into())
}

fn screen_with(
    paths: &Paths,
    kind: &str,
    run: &mut impl FnMut(&str, &[String], Option<&[u8]>, Duration) -> Result<Output>,
    create: impl FnOnce(&Path) -> Result<PathBuf>,
) -> Result<String> {
    let selected = run("slurp", &[], None, Duration::from_secs(180))?;
    let geometry = String::from_utf8(selected.stdout)?;
    let trim = |value: &str| {
        value
            .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
            .to_string()
    };
    let geometry = trim(&geometry);
    if !selected.status.success() || geometry.is_empty() {
        return Ok(String::new());
    }
    let cache = paths.cache.join("menu");
    fs::create_dir_all(cache.parent().ok_or("Missing cache parent")?)?;
    match fs::DirBuilder::new().mode(0o700).create(&cache) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && cache.is_dir() => {}
        Err(error) => return Err(error.into()),
    };
    let temporary = create(&cache)?;
    let result = (|| -> Result<String> {
        let name = temporary
            .to_str()
            .ok_or("Capture path is not UTF-8")?
            .to_owned();
        let captured = run(
            "grim",
            &["-g".into(), geometry, name.clone()],
            None,
            Duration::from_secs(5),
        )?;
        if !captured.status.success() {
            return Err("Screen capture failed".into());
        }
        let (program, args) = if kind == "text" {
            ("tesseract", vec![name, "stdout".into()])
        } else {
            ("zbarimg", vec!["--quiet".into(), "--raw".into(), name])
        };
        let recognized = run(program, &args, None, Duration::from_secs(30))?;
        let text = trim(&String::from_utf8(recognized.stdout)?);
        if text.is_empty() {
            return Err(if kind == "text" {
                "No text found"
            } else {
                "No QR code found"
            }
            .into());
        }
        let copied = run(
            "wl-copy",
            &["--type".into(), "text/plain;charset=utf-8".into()],
            Some(text.as_bytes()),
            Duration::from_secs(5),
        )?;
        if !copied.status.success() {
            return Err("Clipboard copy failed".into());
        }
        Ok(if kind == "text" {
            "Text copied"
        } else {
            "QR content copied"
        }
        .into())
    })();
    match fs::remove_file(&temporary) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    #[test]
    fn current_python_record_start_waits_before_publishing_owned_state() -> Result<()> {
        use std::cell::RefCell;
        struct Fake<'a> {
            case: &'a Value,
            calls: &'a RefCell<Vec<Value>>,
            home: &'a Path,
        }
        impl Units for Fake<'_> {
            fn owner(&mut self, _: &str) -> Result<String> {
                Ok(self.case["owner"].as_str().ok_or("Missing owner")?.into())
            }
            fn active(&mut self, _: &str) -> Result<bool> {
                Ok(self.case["busy"].as_bool().ok_or("Missing active state")?)
            }
            fn start(&mut self, name: &str, args: &[String]) -> Result<()> {
                let mut argv = vec![
                    "systemd-run".to_string(),
                    "--user".into(),
                    "--collect".into(),
                    format!("--unit={}", name.trim_end_matches(".service")),
                ];
                argv.extend(
                    args.iter().map(|arg| {
                        arg.replace(&self.home.to_string_lossy().into_owned(), "{home}")
                    }),
                );
                self.calls
                    .borrow_mut()
                    .push(json!({"argv":argv,"timeout":5,"check":true}));
                Ok(())
            }
            fn stop(&mut self, _: &str, _: &str) -> Result<()> {
                Err("Unexpected stop".into())
            }
            fn finish_recording(&mut self, _: &str, _: &str) -> Result<()> {
                Err("Unexpected signal".into())
            }
        }
        let reference: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/menu-record-start-reference.json"
        )))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing record-start cases")?
        {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            let calls = RefCell::new(Vec::new());
            let mut units = Fake {
                case,
                calls: &calls,
                home: home.path(),
            };
            let result = record_start_with(
                &paths,
                &mut units,
                &mut |program, args, input, timeout| {
                    assert_eq!(program, "slurp");
                    assert!(args.is_empty());
                    assert!(input.is_none());
                    assert_eq!(timeout, Duration::from_secs(180));
                    calls
                        .borrow_mut()
                        .push(json!({"argv":["slurp"],"timeout":180,"check":false}));
                    Ok(Output {
                        status: std::process::ExitStatus::from_raw(
                            (case["code"].as_i64().ok_or("Missing code")? as i32) << 8,
                        ),
                        stdout: case["selection"]
                            .as_str()
                            .ok_or("Missing selection")?
                            .as_bytes()
                            .to_vec(),
                        stderr: vec![],
                    })
                },
                || "20240102_030405_678901".into(),
            );
            let outcome = match result {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":"ValueError","message":error.to_string()}),
            };
            assert_eq!(outcome, case["outcome"]);
            assert_eq!(json!(*calls.borrow()), case["calls"]);
            let mut observed = serde_json::Map::new();
            for item in fs::read_dir(paths.state.join("menu"))? {
                let path = item?.path();
                observed.insert(format!(".local/state/nacre/menu/{}",path.file_name().ok_or("Missing filename")?.to_string_lossy()),json!({"text":fs::read_to_string(&path)?.replace(&home.path().to_string_lossy().into_owned(),"{home}"),"mode":format!("0o{:o}",fs::metadata(path)?.permissions().mode()&0o777)}));
            }
            assert_eq!(Value::Object(observed), case["files"]);
        }
        let stamp = chrono::NaiveDateTime::parse_from_str(
            "2024-01-02 03:04:05.678901",
            "%Y-%m-%d %H:%M:%S%.f",
        )?;
        assert_eq!(
            stamp.format("%Y%m%d_%H%M%S_%6f").to_string(),
            "20240102_030405_678901"
        );
        Ok(())
    }
    #[test]
    fn current_python_owned_unit_guards_and_state_files() -> Result<()> {
        struct FakeUnits<'a> {
            owner: String,
            active: std::vec::IntoIter<bool>,
            calls: &'a mut Vec<Value>,
        }
        impl Units for FakeUnits<'_> {
            fn owner(&mut self, _: &str) -> Result<String> {
                Ok(self.owner.clone())
            }
            fn start(&mut self, name: &str, args: &[String]) -> Result<()> {
                let mut argv = vec![
                    "systemd-run".to_string(),
                    "--user".into(),
                    "--collect".into(),
                    format!("--unit={}", name.trim_end_matches(".service")),
                ];
                argv.extend_from_slice(args);
                self.calls
                    .push(json!({"argv":argv,"timeout":5,"check":true}));
                self.owner = "8:created".into();
                Ok(())
            }
            fn stop(&mut self, name: &str, _: &str) -> Result<()> {
                self.calls.push(
                    json!({"argv":["systemctl","--user","stop",name],"timeout":5,"check":true}),
                );
                Ok(())
            }
            fn finish_recording(&mut self, name: &str, _: &str) -> Result<()> {
                self.calls.push(json!({"argv":["systemctl","--user","kill","--signal=SIGINT",name],"timeout":5,"check":true}));
                if self.active.any(|active| !active) {
                    Ok(())
                } else {
                    Err("Recording is still finishing; try again shortly".into())
                }
            }
        }
        let reference: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/menu-unit-reference.json"
        )))?;
        for case in reference["cases"].as_array().ok_or("Missing unit cases")? {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            let name = if case["operation"] == "record_stop" {
                "recording"
            } else {
                "stay-awake"
            };
            if case["record"]
                .as_object()
                .is_some_and(|record| !record.is_empty())
            {
                fs::create_dir_all(paths.state.join("menu"))?;
                files::json_write(
                    &paths.state.join(format!("menu/{name}.json")),
                    &case["record"],
                )?;
            }
            let mut calls = vec![];
            let mut units = FakeUnits {
                owner: case["owner"].as_str().ok_or("Missing owner")?.into(),
                active: case["active"]
                    .as_array()
                    .ok_or("Missing states")?
                    .iter()
                    .map(|value| value.as_bool().expect("fixture state"))
                    .collect::<Vec<_>>()
                    .into_iter(),
                calls: &mut calls,
            };
            let result = if name == "recording" {
                record_stop_with(&paths, &mut units)
            } else {
                stay_awake_with(&paths, &mut units)
            };
            let outcome = match result {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":"ValueError","message":error.to_string()}),
            };
            assert_eq!(outcome, case["outcome"]);
            assert_eq!(json!(calls), case["calls"]);
            let mut observed = serde_json::Map::new();
            let folder = paths.state.join("menu");
            for entry in fs::read_dir(folder)? {
                let path = entry?.path();
                observed.insert(format!(".local/state/nacre/menu/{}",path.file_name().ok_or("Missing filename")?.to_string_lossy()),json!({"text":fs::read_to_string(&path)?,"mode":format!("0o{:o}",fs::metadata(path)?.permissions().mode()&0o777)}));
            }
            assert_eq!(Value::Object(observed), case["files"]);
        }
        Ok(())
    }
    #[test]
    fn current_python_screen_reader_results_arguments_and_cleanup() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/menu-screen-reference.json"
        )))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing screen cases")?
        {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            let mut calls = Vec::new();
            let normalize =
                |value: &str| value.replace(&home.path().to_string_lossy().into_owned(), "{home}");
            let result = screen_with(
                &paths,
                case["kind"].as_str().ok_or("Missing capture kind")?,
                &mut |program, args, input, timeout| {
                    let argv: Vec<_> = std::iter::once(program.to_owned())
                        .chain(args.iter().map(|arg| normalize(arg)))
                        .collect();
                    let status = if program == "slurp" {
                        case["code"].as_i64().ok_or("Missing selection code")? as i32
                    } else {
                        0
                    };
                    if program == "grim" || program == "wl-copy" {
                        assert_eq!(timeout, Duration::from_secs(5));
                        let input = input
                            .unwrap_or_default()
                            .iter()
                            .map(|byte| format!("{byte:02x}"))
                            .collect::<String>();
                        calls.push(json!({"argv":argv,"input":input,"check":true}));
                        if program == "grim" {
                            fs::write(args.last().ok_or("Missing capture file")?, b"fixture png")?;
                        }
                    } else {
                        calls.push(json!({"argv":argv,"timeout":timeout.as_secs(),"check":false}));
                    }
                    let stdout = if program == "slurp" {
                        case["selection"].as_str().ok_or("Missing selection")?
                    } else if program == "tesseract" || program == "zbarimg" {
                        case["text"].as_str().ok_or("Missing recognition text")?
                    } else {
                        ""
                    };
                    Ok(Output {
                        status: std::process::ExitStatus::from_raw(status << 8),
                        stdout: stdout.as_bytes().to_vec(),
                        stderr: vec![],
                    })
                },
                |cache| {
                    let path = cache.join("fixture.png");
                    let file = fs::OpenOptions::new()
                        .create_new(true)
                        .write(true)
                        .open(&path)?;
                    file.set_permissions(fs::Permissions::from_mode(0o600))?;
                    Ok(path)
                },
            );
            let outcome = match result {
                Ok(value) => json!({"value":value}),
                Err(error) => json!({"error":"ValueError","message":error.to_string()}),
            };
            assert_eq!(outcome, case["outcome"]);
            assert_eq!(json!(calls), case["calls"]);
            assert_eq!(case["files"], json!({}));
            let cache = paths.cache.join("menu");
            if cache.exists() {
                assert_eq!(fs::read_dir(cache)?.count(), 0);
            }
        }
        Ok(())
    }
    #[test]
    fn current_python_configuration_files_and_editor_arguments() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/menu-config-reference.json"
        )))?;
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing config cases")?
        {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            if let Some(seed) = case["seed"].as_str() {
                fs::create_dir_all(&paths.config)?;
                let path = paths.config.join("apps.json");
                fs::write(&path, seed)?;
                fs::set_permissions(path, fs::Permissions::from_mode(0o640))?;
            }
            let result = configuration_command(
                &paths,
                case["route"].as_str().ok_or("Missing route")?,
                case["editor"].as_str().ok_or("Missing editor")?,
                |name| case["tools"][name].is_string(),
            );
            let (outcome, calls) = match result {
                Ok(command) => (
                    json!({"value":7}),
                    json!([command
                        .iter()
                        .map(|value| value
                            .replace(&home.path().to_string_lossy().into_owned(), "{home}"))
                        .collect::<Vec<_>>()]),
                ),
                Err(error) => (
                    json!({"error":"ValueError","message":error.to_string()}),
                    json!([]),
                ),
            };
            assert_eq!(outcome, case["outcome"]);
            assert_eq!(calls, case["calls"]);
            let mut observed = serde_json::Map::new();
            for name in ["apps.json", "shortcuts.lua"] {
                let path = paths.config.join(name);
                if path.exists() {
                    observed.insert(format!(".config/nacre/{name}"),json!({"text":fs::read_to_string(&path)?,"mode":format!("0o{:o}",fs::metadata(path)?.permissions().mode()&0o777)}));
                }
            }
            assert_eq!(Value::Object(observed), case["files"]);
        }
        Ok(())
    }
}
