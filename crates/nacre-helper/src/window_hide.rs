//! Compatible private hide/restore ledger; compositor success controls record lifetime.
use crate::{i18n, Result};
use nix::fcntl::{Flock, FlockArg};
use serde_json::{json, Value};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    time::SystemTime,
};
pub const HIDDEN: &str = "special:hidden";
pub const QUERY_TIMEOUT_SECONDS: u64 = 5;
pub const MAX_QUERY_BYTES: usize = 4 * 1024 * 1024;
pub fn address(value: &Value) -> bool {
    crate::compositor::hyprland::valid_window_id(value)
}
pub trait Runtime {
    fn valid_window_id(&mut self, value: &Value) -> Result<bool> {
        Ok(address(value))
    }
    fn query(&mut self, name: &str) -> Result<Value>;
    fn service_action(&mut self, _name: &str, _window: &str, _kwargs: Value) -> Result<Value> {
        Err("Window service action unavailable".into())
    }
    fn dispatch(&mut self, method: &str, window: &str, values: Value) -> Result<()>;
    fn notify(&mut self, message: &str) -> Result<()>;
}
pub struct Native {
    pub backend: crate::compositor::backend::Backend,
}
impl Runtime for Native {
    fn valid_window_id(&mut self, value: &Value) -> Result<bool> {
        Ok(self.backend.valid_window_id(value))
    }
    fn query(&mut self, name: &str) -> Result<Value> {
        Ok(self.backend.query(
            name,
            std::time::Duration::from_secs(QUERY_TIMEOUT_SECONDS),
            Some(MAX_QUERY_BYTES),
        )?)
    }
    fn dispatch(&mut self, method: &str, window: &str, values: Value) -> Result<()> {
        dispatch(self, method, window, values)
    }
    fn service_action(&mut self, name: &str, window: &str, kwargs: Value) -> Result<Value> {
        Ok(self.backend.action(name, &[json!(window)], &kwargs)?)
    }
    fn notify(&mut self, message: &str) -> Result<()> {
        let connection = zbus::blocking::connection::Builder::session()?
            .method_timeout(std::time::Duration::from_secs(2))
            .build()?;
        let proxy = zbus::blocking::Proxy::new(
            &connection,
            "org.freedesktop.Notifications",
            "/org/freedesktop/Notifications",
            "org.freedesktop.Notifications",
        )?;
        let _: u32 = proxy.call(
            "Notify",
            &(
                "Nacre Windows",
                0u32,
                "nacre-notifications",
                message,
                "",
                Vec::<String>::new(),
                std::collections::HashMap::<String, zbus::zvariant::Value>::new(),
                2000i32,
            ),
        )?;
        Ok(())
    }
}
pub fn dispatch(
    runtime: &mut impl Runtime,
    method: &str,
    window: &str,
    values: Value,
) -> Result<()> {
    if !runtime.valid_window_id(&json!(window))? {
        return Err("Invalid window address".into());
    }
    let (name, kwargs) = match method {
        "window.move" => (
            "move_window",
            json!({"workspace":values["workspace"],"follow":values["follow"],"timeout":5}),
        ),
        "focus" => ("focus_window", json!({"timeout":5})),
        _ => return Err("Unknown window action".into()),
    };
    let result = runtime.service_action(name, window, kwargs)?;
    if result["stdout"].as_str().unwrap_or("").trim() != "ok" {
        return Err("The compositor did not accept the window action".into());
    }
    Ok(())
}
pub fn run(args: &[String]) -> Result<i32> {
    let mut name = "hide";
    let mut window = None;
    let mut quiet = false;
    let mut selected = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "hide" | "restore-last" | "restore-current" if !selected => {
                name = &args[index];
                selected = true;
            }
            "-h" | "--help" => {
                println!("Usage: window-hide [hide|restore-last|restore-current] [--window ADDRESS] [--quiet]");
                return Ok(0);
            }
            "--quiet" => quiet = true,
            "--window" => {
                index += 1;
                window = args.get(index).map(String::as_str);
                if window.is_none() {
                    eprintln!("window-hide: --window requires an address");
                    return Ok(2);
                }
            }
            _ => {
                eprintln!("window-hide: invalid argument {}", args[index]);
                return Ok(2);
            }
        }
        index += 1;
    }
    let result = (|| -> Result<()> {
        let paths = crate::paths::Paths::environment()?;
        let folder = paths
            .cache
            .parent()
            .ok_or("Invalid cache root")?
            .join("hypr-minimize");
        let backend = crate::compositor::backend::Backend::new(paths, std::env::vars().collect())?;
        action(name, &folder, window, quiet, &mut Native { backend })
    })();
    match result {
        Ok(()) => Ok(0),
        Err(error) => {
            eprintln!("window-hide: {error}");
            Ok(1)
        }
    }
}
fn announce(runtime: &mut impl Runtime, quiet: bool, message: &str) {
    if !quiet {
        if let Err(error) = runtime.notify(message) {
            eprintln!("nacre-helper: window feedback failed: {error}");
        }
    }
}
pub fn write_record(path: &Path, workspace: i64) -> Result<()> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("Invalid window record path")?)?;
    writeln!(temporary, "{workspace}")?;
    temporary.persist(path)?;
    Ok(())
}
pub fn records(folder: &Path, clients: &Value) -> Result<Vec<(SystemTime, PathBuf, i64)>> {
    records_with_validator(folder, clients, |value| Ok(address(value)))
}
fn records_with_validator(
    folder: &Path,
    clients: &Value,
    mut valid: impl FnMut(&Value) -> Result<bool>,
) -> Result<Vec<(SystemTime, PathBuf, i64)>> {
    let rows = clients
        .as_array()
        .ok_or("Malformed compositor query response")?;
    let mut result = Vec::new();
    let integer = regex::Regex::new(r"^-?[0-9]+$")?;
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if !valid(&json!(name))? {
            continue;
        }
        let metadata = fs::symlink_metadata(&path)?;
        if metadata.file_type().is_symlink() || !metadata.is_file() || metadata.len() > 128 {
            continue;
        }
        let text = fs::read_to_string(&path)?;
        let text = text.trim();
        if !integer.is_match(text) {
            continue;
        }
        let workspace = text.parse::<i64>()?;
        let client = rows.iter().find(|client| client["address"] == name);
        if client.is_none_or(|client| client["workspace"]["name"] != HIDDEN) {
            fs::remove_file(&path)?;
            continue;
        }
        result.push((metadata.modified()?, path, workspace));
    }
    result.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.file_name().cmp(&a.1.file_name()))
    });
    Ok(result)
}
pub fn destination(workspace: i64, runtime: &mut impl Runtime) -> Result<String> {
    if workspace > 0 {
        return Ok(workspace.to_string());
    }
    let workspaces = runtime.query("workspaces")?;
    for item in workspaces
        .as_array()
        .ok_or("Malformed compositor query response")?
    {
        if item["id"] != workspace {
            continue;
        }
        if let Some(name) = item["name"]
            .as_str()
            .filter(|name| !name.is_empty() && !name.chars().any(|char| (char as u32) < 32))
        {
            return Ok(if name.starts_with("special:") {
                name.into()
            } else {
                format!("name:{name}")
            });
        }
    }
    Err("The original workspace is unavailable".into())
}
pub fn action(
    name: &str,
    folder: &Path,
    window: Option<&str>,
    quiet: bool,
    runtime: &mut impl Runtime,
) -> Result<()> {
    if let Some(window) = window {
        if !runtime.valid_window_id(&json!(window))? {
            return Err("Invalid window address".into());
        }
    }
    if !folder.exists() {
        if let Some(parent) = folder.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::DirBuilder::new().mode(0o700).create(folder)?;
    }
    let guard = OpenOptions::new()
        .create(true)
        .append(true)
        .custom_flags(nix::libc::O_NOFOLLOW)
        .open(folder.join(".lock"))?;
    guard.set_permissions(fs::Permissions::from_mode(0o600))?;
    let _guard = Flock::lock(guard, FlockArg::LockExclusive).map_err(|(_, error)| error)?;
    if name == "hide" {
        let client = if let Some(window) = window {
            let clients = runtime.query("windows")?;
            clients
                .as_array()
                .ok_or("Malformed compositor query response")?
                .iter()
                .find(|client| client["address"] == window)
                .cloned()
                .unwrap_or(json!({}))
        } else {
            runtime.query("active_window")?
        };
        let id = &client["address"];
        let workspace = &client["workspace"];
        let number = workspace["id"].as_i64();
        let number = if runtime.valid_window_id(id)? {
            number
        } else {
            None
        };
        let Some(number) = number else {
            announce(
                runtime,
                quiet,
                &i18n::tr(
                    "windows.notification.no-active-window-to-hide",
                    "No active window to hide",
                ),
            );
            return Ok(());
        };
        if workspace["name"] == HIDDEN {
            return Ok(());
        }
        let id = id.as_str().ok_or("Invalid window address")?;
        let path = folder.join(id);
        if path.is_symlink() {
            return Err("The window record is a symlink".into());
        }
        write_record(&path, number)?;
        if let Err(error) = runtime.dispatch(
            "window.move",
            id,
            json!({"workspace":HIDDEN,"follow":false}),
        ) {
            fs::remove_file(path)?;
            return Err(error);
        }
        announce(
            runtime,
            quiet,
            &i18n::tr("windows.notification.window-hidden", "Window hidden"),
        );
        return Ok(());
    }
    let clients = runtime.query("windows")?;
    let mut candidates =
        records_with_validator(folder, &clients, |id| runtime.valid_window_id(id))?;
    if let Some(window) = window {
        candidates.retain(|row| row.1.file_name().is_some_and(|name| name == window));
    }
    if name == "restore-current" {
        let current = runtime.query("active_workspace")?["id"].as_i64();
        candidates.retain(|row| Some(row.2) == current);
    } else {
        candidates.truncate(1);
    }
    for (_, path, workspace) in &candidates {
        let id = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Invalid window address")?;
        let target = destination(*workspace, runtime)?;
        runtime.dispatch(
            "window.move",
            id,
            json!({"workspace":target,"follow":false}),
        )?;
        fs::remove_file(path)?;
        if name == "restore-last" {
            runtime.dispatch("focus", id, json!({}))?;
        }
    }
    let message = if candidates.is_empty() {
        i18n::tr(
            "windows.notification.no-hidden-windows-to-restore",
            "No hidden windows to restore",
        )
    } else if name == "restore-last" {
        "Window restored".into()
    } else {
        "Window restored to current workspace".into()
    };
    announce(runtime, quiet, &message);
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        input: Value,
        calls: Vec<Value>,
        queries: Vec<Value>,
        notifications: Vec<Value>,
    }
    impl Runtime for Fixture {
        fn query(&mut self, name: &str) -> Result<Value> {
            self.queries.push(json!(name));
            Ok(match name {
                "active_window" => self.input["clients"]
                    .as_array()
                    .and_then(|rows| rows.first())
                    .cloned()
                    .unwrap_or(json!({})),
                "active_workspace" => json!({"id":2}),
                "windows" => self.input["clients"].clone(),
                _ => json!([]),
            })
        }
        fn dispatch(&mut self, method: &str, window: &str, values: Value) -> Result<()> {
            self.calls
                .push(json!({"method":method,"window":window,"values":values}));
            if self.input["failure"] == true {
                Err("fixture dispatch failed".into())
            } else {
                Ok(())
            }
        }
        fn notify(&mut self, message: &str) -> Result<()> {
            self.notifications.push(json!(message));
            Ok(())
        }
    }
    #[test]
    fn lock_symlink_never_changes_its_target() -> Result<()> {
        let home = tempfile::tempdir()?;
        let folder = home.path().join("ledger");
        fs::create_dir(&folder)?;
        let target = home.path().join("private");
        fs::write(&target, "retain\n")?;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o644))?;
        std::os::unix::fs::symlink(&target, folder.join(".lock"))?;
        let mut fixture = Fixture {
            input: json!({}),
            calls: vec![],
            queries: vec![],
            notifications: vec![],
        };
        assert!(action("hide", &folder, None, true, &mut fixture).is_err());
        assert_eq!(fs::read_to_string(&target)?, "retain\n");
        assert_eq!(fs::metadata(&target)?.permissions().mode() & 0o777, 0o644);
        assert!(fixture.queries.is_empty());
        assert!(fixture.calls.is_empty());
        Ok(())
    }
    #[test]
    fn frozen_current_python_calls_records_modes_and_rollback() -> Result<()> {
        let reference: Value =
            serde_json::from_str(include_str!("../tests/fixtures/window-hide-reference.json"))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let home = tempfile::tempdir()?;
            let folder = home.path().join(".cache/hypr-minimize");
            let input = &case["request"];
            if let Some(records) = input["records"]
                .as_object()
                .filter(|records| !records.is_empty())
            {
                fs::create_dir_all(&folder)?;
                for (name, text) in records {
                    fs::write(
                        folder.join(name),
                        text.as_str().ok_or("Missing record bytes")?,
                    )?;
                }
            }
            let mut fixture = Fixture {
                input: input.clone(),
                calls: vec![],
                queries: vec![],
                notifications: vec![],
            };
            let error=action(input["action"].as_str().ok_or("Missing action")?,&folder,input["window"].as_str(),false,&mut fixture).err().map(|error|json!({"error":error.to_string(),"type":if error.to_string()=="fixture dispatch failed"{"RuntimeError"}else{"ValueError"}})).unwrap_or(Value::Null);
            assert_eq!(error, case["observed"]["error"], "{}", input["name"]);
            assert_eq!(
                json!(fixture.calls),
                case["observed"]["calls"],
                "{}",
                input["name"]
            );
            assert_eq!(
                json!(fixture.queries),
                case["observed"]["queries"],
                "{}",
                input["name"]
            );
            assert_eq!(
                json!(fixture.notifications),
                case["observed"]["notifications"],
                "{}",
                input["name"]
            );
            let mut files = serde_json::Map::new();
            for entry in walkdir::WalkDir::new(home.path()) {
                let entry = entry?;
                if entry.file_type().is_file() {
                    files.insert(entry.path().strip_prefix(home.path())?.to_string_lossy().into(),json!({"hex":fs::read(entry.path())?.iter().map(|byte|format!("{byte:02x}")).collect::<String>(),"mode":format!("0o{:o}",entry.metadata()?.permissions().mode()&0o777)}));
                }
            }
            assert_eq!(
                Value::Object(files),
                case["observed"]["files"],
                "{}",
                input["name"]
            );
        }
        Ok(())
    }
}
