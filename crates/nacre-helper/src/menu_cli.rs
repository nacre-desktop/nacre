//! Existing menu routes and private selector sessions, waiting on kernel events.
use crate::signals::Signals;
use crate::{files, i18n, json, menu, paths::Paths, process, Result};
use clap::Parser;
use nix::{
    fcntl::{Flock, FlockArg},
    sys::signal::Signal,
};
use regex::Regex;
use rustix::{
    event::{poll, PollFd, PollFlags, Timespec},
    fs::inotify,
};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::{self, Read},
    os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt},
    path::PathBuf,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

fn error(id: &str, fallback: &str) -> crate::Error {
    i18n::tr(&format!("helper.menu-cli.{id}"), fallback).into()
}
fn now() -> Result<f64> {
    Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64())
}
fn root(paths: &Paths) -> PathBuf {
    paths.cache.join("menu/requests")
}
fn count(text: &str) -> usize {
    text.chars().count()
}
fn truncate(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

pub fn session_path(paths: &Paths, ident: &str) -> Result<PathBuf> {
    if !Regex::new(r"\A[0-9a-f]{32}\z")?.is_match(ident) {
        return Err(error(
            "invalid-selector-session",
            "Invalid selector session",
        ));
    }
    let path = root(paths).join(ident);
    let available = fs::symlink_metadata(&path).is_ok_and(|metadata| {
        !metadata.file_type().is_symlink()
            && metadata.is_dir()
            && metadata.uid() == rustix::process::getuid().as_raw()
            && metadata.mode() & 0o077 == 0
    });
    if !available {
        return Err(error(
            "selector-session-is-unavailable",
            "Selector session is unavailable",
        ));
    }
    Ok(path)
}

pub fn choices(text: &str, json_input: bool) -> Result<Value> {
    if count(text) > 1_048_576 {
        return Err(error(
            "selection-list-is-too-large",
            "Selection list is too large",
        ));
    }
    let raw = if json_input {
        serde_json::from_str(text)?
    } else {
        Value::Array(text.split(['\n','\r','\x0b','\x0c','\x1c','\x1d','\x1e','\u{85}','\u{2028}','\u{2029}']).filter_map(|line| {
            let parts: Vec<_> = line.split('\t').collect();
            (!parts[0].is_empty()).then(|| json!({"label":parts[0],"value":parts.get(1).copied().unwrap_or(parts[0]),"description":parts.get(2).copied().unwrap_or("")}))
        }).collect())
    };
    let rows = raw
        .as_array()
        .filter(|rows| rows.len() <= 2000)
        .ok_or_else(|| {
            error(
                "use-a-list-of-at-most-2000-choices",
                "Use a list of at most 2000 choices",
            )
        })?;
    let mut result = Vec::new();
    for row in rows {
        let label = row.get("label").and_then(Value::as_str);
        let value = row
            .get("value")
            .or_else(|| row.get("label"))
            .and_then(Value::as_str);
        let (Some(label), Some(value)) = (label, value) else {
            return Err(error(
                "each-choice-needs-a-label-and-string-value",
                "Each choice needs a label and string value",
            ));
        };
        if !row.is_object() {
            return Err(error(
                "each-choice-needs-a-label-and-string-value",
                "Each choice needs a label and string value",
            ));
        }
        let description = row
            .get("description")
            .map(json::python_str)
            .unwrap_or_default();
        let icon = row
            .get("icon")
            .map(json::python_str)
            .unwrap_or_else(|| "chevron_right".into());
        if label.is_empty()
            || count(label) > 200
            || count(value) > 16384
            || [label, value, description.as_str(), icon.as_str()]
                .iter()
                .any(|text| text.contains('\0'))
        {
            return Err(error(
                "invalid-selection-label-or-value",
                "Invalid selection label or value",
            ));
        }
        result.push(json!({"label":label,"value":value,"description":description,"icon":icon,"disabled":row.get("disabled")==Some(&Value::Bool(true))}));
    }
    Ok(Value::Array(result))
}

pub fn load_request(paths: &Paths, ident: &str) -> Result<Value> {
    let path = session_path(paths, ident)?.join("request.json");
    let metadata = fs::symlink_metadata(&path)?;
    if metadata.file_type().is_symlink()
        || metadata.len() > 1_048_576
        || metadata.uid() != rustix::process::getuid().as_raw()
        || metadata.mode() & 0o077 != 0
    {
        return Err(error(
            "invalid-selector-request",
            "Invalid selector request",
        ));
    }
    let mut data: Value = serde_json::from_slice(&fs::read(path)?)?;
    let version = data
        .get("version")
        .is_some_and(|value| value == &Value::Bool(true) || value.as_f64() == Some(1.0));
    let expires = data
        .get("expires")
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite());
    let current_time = now()?;
    if !data.is_object()
        || !version
        || data["id"] != ident
        || !matches!(data["kind"].as_str(), Some("pick" | "input"))
        || !expires.is_some_and(|expiry| expiry > current_time)
    {
        return Err(error(
            "selector-request-expired-or-has-an-invalid-format",
            "Selector request expired or has an invalid format",
        ));
    }
    if !data
        .get("title")
        .map_or(Some(""), Value::as_str)
        .is_some_and(|title| count(title) <= 200)
    {
        return Err(error("invalid-selector-title", "Invalid selector title"));
    }
    if !data
        .get("initial")
        .map_or(Some(""), Value::as_str)
        .is_some_and(|initial| count(initial) <= 16384)
    {
        return Err(error("invalid-initial-input", "Invalid initial input"));
    }
    if data["kind"] == "pick" {
        data["choices"] = choices(
            &serde_json::to_string(data.get("choices").unwrap_or(&json!([])))?,
            true,
        )?;
    }
    Ok(data)
}

pub fn respond(paths: &Paths, ident: &str, value: &Value) -> Result<()> {
    let request = load_request(paths, ident)?;
    let cancelled = value.get("cancelled").and_then(Value::as_bool);
    if !value.is_object() || cancelled.is_none() {
        return Err(error(
            "invalid-selector-response",
            "Invalid selector response",
        ));
    }
    let answer = value
        .get("value")
        .map_or(Some(""), Value::as_str)
        .filter(|answer| count(answer) <= 16384 && !answer.contains('\0'))
        .ok_or_else(|| error("invalid-selector-value", "Invalid selector value"))?;
    if cancelled == Some(false)
        && request["kind"] == "pick"
        && !request["choices"].as_array().is_some_and(|rows| {
            rows.iter()
                .any(|row| row["value"] == answer && row["disabled"] != true)
        })
    {
        return Err(error(
            "that-choice-is-not-part-of-this-selector",
            "That choice is not part of this selector",
        ));
    }
    files::json_write(
        &session_path(paths, ident)?.join("response.json"),
        &json!({"cancelled":cancelled.unwrap(),"value":answer,"error":truncate(&value.get("error").map(json::python_str).unwrap_or_default(),200)}),
    )
}

fn ipc(paths: &Paths, target: &str, args: &[&str]) -> Result<()> {
    let mut arguments = ["-c", "nacre", "ipc", "call", target]
        .map(str::to_owned)
        .to_vec();
    arguments.extend(args.iter().map(|arg| (*arg).to_owned()));
    process::checked(
        paths
            .share
            .join("shell/bin/qs")
            .to_str()
            .ok_or("Invalid Quickshell path")?,
        &arguments,
        None,
        Duration::from_secs(5),
    )?;
    Ok(())
}

fn request(
    paths: &Paths,
    kind: &str,
    title: &str,
    rows: Option<Value>,
    initial: &str,
    timeout: u64,
) -> Result<i32> {
    let directory = root(paths);
    fs::create_dir_all(&directory)?;
    fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))?;
    let guard = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .open(directory.join("selector.lock"))?;
    fs::set_permissions(
        directory.join("selector.lock"),
        fs::Permissions::from_mode(0o600),
    )?;
    let _guard = Flock::lock(guard, FlockArg::LockExclusiveNonblock).map_err(|(_, _)| {
        error(
            "another-selector-is-already-open",
            "Another selector is already open",
        )
    })?;
    let ident = uuid::Uuid::new_v4().simple().to_string();
    let path = directory.join(&ident);
    fs::DirBuilder::new().mode(0o700).create(&path)?;
    files::json_write(
        &path.join("request.json"),
        &json!({"version":1,"id":ident,"kind":kind,"title":truncate(title,200),"choices":rows.unwrap_or_else(||json!([])),"initial":truncate(initial,16384),"expires":now()?+timeout as f64}),
    )?;
    let result = (|| -> Result<i32> {
        let signals = Signals::new()?;
        let watcher =
            inotify::init(inotify::CreateFlags::CLOEXEC | inotify::CreateFlags::NONBLOCK)?;
        inotify::add_watch(
            &watcher,
            &path,
            inotify::WatchFlags::MOVED_TO
                | inotify::WatchFlags::CLOSE_WRITE
                | inotify::WatchFlags::CREATE,
        )?;
        ipc(paths, "menuPrompt", &["open", &ident])?;
        let deadline = Instant::now() + Duration::from_secs(timeout);
        loop {
            let response = path.join("response.json");
            if response.is_file() {
                let result: Value = serde_json::from_slice(&fs::read(response)?)?;
                if result["cancelled"] == true {
                    if let Some(message) = result
                        .get("error")
                        .filter(|value| value.as_str().is_some_and(|text| !text.is_empty()))
                    {
                        eprintln!("{}", json::python_str(message));
                    }
                    return Ok(1);
                }
                println!(
                    "{}",
                    result
                        .get("value")
                        .map(json::python_str)
                        .unwrap_or_default()
                );
                return Ok(0);
            }
            let Some(remaining) = deadline.checked_duration_since(Instant::now()) else {
                return Err(error("selector-timed-out", "Selector timed out"));
            };
            let timeout = Timespec {
                tv_sec: remaining.as_secs().try_into()?,
                tv_nsec: remaining.subsec_nanos().into(),
            };
            let mut descriptors = [
                PollFd::new(&watcher, PollFlags::IN),
                PollFd::new(&signals.fd, PollFlags::IN),
            ];
            poll(&mut descriptors, Some(&timeout))?;
            if descriptors[1].revents().contains(PollFlags::IN) {
                if let Some(signal) = signals.fd.read_signal()? {
                    return Ok(if signal.ssi_signo == Signal::SIGINT as u32 {
                        130
                    } else {
                        1
                    });
                }
            }
            if descriptors[0].revents().contains(PollFlags::IN) {
                let mut bytes = [0_u8; 4096];
                rustix::io::read(&watcher, &mut bytes[..])?;
            }
        }
    })();
    if let Err(error) = ipc(paths, "menuPrompt", &["cancel", &ident]) {
        eprintln!("Selector cancel: {error}");
    }
    for name in [
        "request.json",
        "request.next",
        "response.json",
        "response.next",
    ] {
        match fs::remove_file(path.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    fs::remove_dir(path)?;
    result
}

#[derive(Parser)]
#[command(
    name = "menu-cli.py",
    infer_long_args = true,
    args_override_self = true
)]
struct Arguments {
    #[arg(default_value = "home", allow_negative_numbers = true)]
    route: String,
    #[arg(default_value = "", allow_negative_numbers = true)]
    value: String,
    #[arg(long, default_value = "Choose an item")]
    title: String,
    #[arg(long)]
    json: bool,
    #[arg(long, default_value = "")]
    initial: String,
    #[arg(long,default_value="300",allow_negative_numbers=true,value_parser=crate::arguments::integer)]
    timeout: i64,
}
const HELP: &str = "usage: menu-cli.py [-h] [--title TITLE] [--json] [--initial INITIAL]\n                   [--timeout TIMEOUT]\n                   [route] [value]\n\nOpen a Nacre menu route, or ask its shared window for a selection/text value.\n\npositional arguments:\n  route\n  value\n\noptions:\n  -h, --help         show this help message and exit\n  --title TITLE\n  --json\n  --initial INITIAL\n  --timeout TIMEOUT\n";
pub fn run(arguments: &[String]) -> Result<i32> {
    let parsed = match Arguments::try_parse_from(
        std::iter::once("menu-cli.py").chain(arguments.iter().map(String::as_str)),
    ) {
        Ok(parsed) => parsed,
        Err(error) if error.kind() == clap::error::ErrorKind::DisplayHelp => {
            print!("{HELP}");
            return Ok(0);
        }
        Err(error) => {
            error.print()?;
            return Ok(2);
        }
    };
    let result = (|| -> Result<i32> {
        let paths = Paths::environment()?;
        let route = parsed.route.as_str();
        let value = parsed.value.as_str();
        let title = parsed.title;
        let initial = parsed.initial;
        let json_input = parsed.json;
        let timeout = parsed.timeout;
        match route {
            "respond" => {
                use io::BufRead;
                let mut bytes = Vec::new();
                io::stdin()
                    .lock()
                    .take(4 * 65537)
                    .read_until(b'\n', &mut bytes)?;
                let line = truncate(std::str::from_utf8(&bytes)?, 65537);
                respond(&paths, value, &serde_json::from_str::<Value>(&line)?)?;
            }
            "read-request" => println!("{}", json::encode(&load_request(&paths, value)?)?),
            "webapp-launch" => menu::launch_webapp(value)?,
            "pick" | "input" => {
                if !(1..=3600).contains(&timeout) {
                    return Err(error(
                        "timeout-must-be-between-1-and-3600-seconds",
                        "Timeout must be between 1 and 3600 seconds",
                    ));
                }
                let rows = if route == "pick" {
                    let mut text = String::new();
                    io::stdin().take(4 * 1_048_577).read_to_string(&mut text)?;
                    Some(choices(&truncate(&text, 1_048_577), json_input)?)
                } else {
                    None
                };
                return request(&paths, route, &title, rows, &initial, timeout as u64);
            }
            _ => ipc(&paths, "nacre", &["menu", route, value])?,
        }
        Ok(0)
    })();
    match result {
        Ok(code) => Ok(code),
        Err(error) => {
            eprintln!("{error}");
            Ok(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tabbed_choices_and_validation_match_existing_spec() -> Result<()> {
        let rows = choices("Same\tone\tFirst\nSame\ttwo\tSecond\n", false)?;
        assert_eq!(rows[0]["value"], "one");
        assert_eq!(rows[1]["value"], "two");
        assert_eq!(rows[1]["description"], "Second");
        assert!(choices(&"x".repeat(1_048_577), false).is_err());
        for value in [
            json!({}),
            json!([{"label":"One","value":3}]),
            json!([{"label":"","value":""}]),
            json!([{"label":"One","value":"bad\u{0}"}]),
        ] {
            assert!(choices(&serde_json::to_string(&value)?, true).is_err());
        }
        assert!(choices(
            &serde_json::to_string(&vec![json!({"label":"One"}); 2001])?,
            true
        )
        .is_err());
        Ok(())
    }
    #[test]
    fn owned_selector_can_only_choose_enabled_members() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let ident = "a".repeat(32);
        let session = root(&paths).join(&ident);
        fs::create_dir_all(&session)?;
        fs::set_permissions(&session, fs::Permissions::from_mode(0o700))?;
        files::json_write(
            &session.join("request.json"),
            &json!({"version":1,"id":ident,"kind":"pick","expires":now()?+60.0,"choices":[{"label":"One","value":"1"},{"label":"Two","value":"2","disabled":true}]}),
        )?;
        respond(&paths, &ident, &json!({"cancelled":false,"value":"1"}))?;
        let response = session.join("response.json");
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(&response)?)?["value"],
            "1"
        );
        assert_eq!(fs::metadata(&response)?.mode() & 0o777, 0o600);
        assert!(respond(&paths, &ident, &json!({"cancelled":false,"value":"2"})).is_err());
        assert!(respond(
            &paths,
            &ident,
            &json!({"cancelled":false,"value":"unknown"})
        )
        .is_err());
        assert!(respond(&paths, &ident, &json!({"cancelled":1,"value":"1"})).is_err());
        respond(&paths, &ident, &json!({"cancelled":true,"value":""}))?;
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(response)?)?["cancelled"],
            true
        );
        Ok(())
    }
    #[test]
    fn paths_permissions_symlinks_and_expiry_are_rejected() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        for ident in [
            "/tmp/elsewhere",
            "../escape",
            "bad",
            "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
        ] {
            assert!(session_path(&paths, ident).is_err());
        }
        let ident = "b".repeat(32);
        let session = root(&paths).join(&ident);
        fs::create_dir_all(&session)?;
        fs::set_permissions(&session, fs::Permissions::from_mode(0o755))?;
        assert!(session_path(&paths, &ident).is_err());
        fs::set_permissions(&session, fs::Permissions::from_mode(0o700))?;
        files::json_write(
            &session.join("request.json"),
            &json!({"version":1,"id":ident,"kind":"input","expires":0}),
        )?;
        assert!(load_request(&paths, &ident).is_err());
        std::os::unix::fs::symlink(&session, root(&paths).join("c".repeat(32)))?;
        assert!(session_path(&paths, &"c".repeat(32)).is_err());
        Ok(())
    }
}
