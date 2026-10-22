//! Desktop actions used by control.sh and lightweight shell services.
use crate::{compositor::backend::Backend, i18n::tr, paths::Paths, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    process::{Command, Stdio},
    time::Duration,
};

fn run(program: &str, args: &[&str]) -> Result<String> {
    let arguments = args
        .iter()
        .map(|value| (*value).to_owned())
        .collect::<Vec<_>>();
    let output = crate::process::run(program, &arguments, None, Duration::from_secs(10))?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr)
            .trim()
            .to_owned()
            .into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
fn launch(args: &[String]) -> Result<()> {
    Command::new("uwsm")
        .args(["app", "--"])
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    Ok(())
}
fn launch_args(name: &str, paths: &Paths) -> Option<Vec<String>> {
    let home = paths.home.to_string_lossy();
    let shell = format!("{home}/.local/share/nacre/shell");
    let local = format!("{home}/.local/bin");
    Some(match name {
        "power" => vec![format!("{local}/nacre-shell"), "session".into()],
        "files" => vec![format!("{local}/nacre-app"), "files".into()],
        "terminal" => vec![format!("{local}/nacre-app"), "terminal".into()],
        "notifications" => vec![format!("{local}/nacre-settings"), "notifications".into()],
        "updates" => vec![
            "kitty".into(),
            "--class".into(),
            "nacre-control".into(),
            "--title".into(),
            "System updates".into(),
            "--".into(),
            "bash".into(),
            format!("{shell}/tools/updates.sh"),
        ],
        "tasks" => vec![
            "kitty".into(),
            "--class".into(),
            "nacre-ai-dashboard".into(),
            "--title".into(),
            "Nacre AI".into(),
            "--".into(),
            "nacre-ai".into(),
            "dashboard".into(),
        ],
        "new" | "resume" => vec![
            "kitty".into(),
            "--class".into(),
            "nacre-ai-task".into(),
            "--title".into(),
            if name == "new" {
                "New chat"
            } else {
                "Resume task"
            }
            .into(),
            "--".into(),
            "nacre-ai".into(),
            "window".into(),
            "--worker-command".into(),
            if name == "new" { "new" } else { "latest" }.into(),
        ],
        _ => return None,
    })
}
fn direct_args(name: &str) -> Option<Vec<String>> {
    Some(match name {
        "network" => vec!["nm-connection-editor".into()],
        "bluetooth" => vec!["blueman-manager".into()],
        "audio" => vec!["pavucontrol".into()],
        "play" => vec!["playerctl".into(), "play-pause".into()],
        "next" => vec!["playerctl".into(), "next".into()],
        "previous" => vec!["playerctl".into(), "previous".into()],
        "mute" => vec![
            "wpctl".into(),
            "set-mute".into(),
            "@DEFAULT_AUDIO_SINK@".into(),
            "toggle".into(),
        ],
        "lock" => vec!["loginctl".into(), "lock-session".into()],
        _ => return None,
    })
}
fn volume_percent(value: &str) -> Result<String> {
    Ok(value.parse::<i32>()?.clamp(0, 100).to_string() + "%")
}
fn workspace_number(value: &str) -> Result<u32> {
    let number = value.parse::<u32>()?;
    if !(1..=10).contains(&number) {
        return Err(tr(
            "helper.desktop-actions.invalid-workspace",
            "Invalid workspace",
        )
        .into());
    }
    Ok(number)
}
fn ai_workspace(paths: &Paths) -> Option<u64> {
    std::fs::read(paths.config.join("profile.json"))
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
        .and_then(|data| data["aiWorkspace"].as_u64())
        .filter(|value| (1..=10).contains(value))
}
fn valid_window_group(ids: &Value, valid: impl Fn(&Value) -> bool) -> Result<()> {
    if !ids.as_array().is_some_and(|items| items.iter().all(valid)) {
        return Err(tr(
            "helper.desktop-actions.invalid-window-group",
            "Invalid window group",
        )
        .into());
    }
    Ok(())
}
fn app_window_rows(ids: &Value, clients: &Value) -> Vec<Value> {
    let mut windows = clients
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|window| ids.as_array().unwrap().contains(&window["address"]))
        .collect::<Vec<_>>();
    windows.sort_by(|a, b| {
        a["workspace"]["id"]
            .as_i64()
            .cmp(&b["workspace"]["id"].as_i64())
            .then_with(|| a["title"].as_str().cmp(&b["title"].as_str()))
    });
    windows
}
fn compositor_windows_with(query: impl FnOnce() -> Result<Value>) -> Result<Value> {
    match query() {
        Ok(windows) => Ok(windows),
        Err(error)
            if error
                .downcast_ref::<crate::compositor::niri_state::Error>()
                .is_some_and(|error| error.kind == "RuntimeError") =>
        {
            Ok(json!([]))
        }
        Err(error) => Err(error),
    }
}
fn compositor_windows(backend: &mut Backend) -> Result<Value> {
    compositor_windows_with(|| {
        Ok(backend.query("windows", Duration::from_secs(10), Some(4 * 1024 * 1024))?)
    })
}
fn focus_address(
    address: &Value,
    valid: bool,
    dispatch: impl FnOnce(&Value) -> Result<()>,
) -> Result<()> {
    if !valid {
        return Err(tr("helper.desktop-actions.invalid-window", "Invalid window").into());
    }
    dispatch(address)
}
fn pick(title: &str, choices: &Value) -> Option<String> {
    let encoded = serde_json::to_string(choices).ok()?;
    let mut command = Command::new("nacre-menu");
    command.args(["pick", "--json", "--title", title]);
    let output =
        crate::process::capture(command, Some(encoded.as_bytes()), Duration::from_secs(300))
            .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}
trait AiRuntime {
    fn windows(&mut self) -> Result<Value>;
    fn focus(&mut self, address: &Value) -> Result<()>;
    fn switch_workspace(&mut self, workspace: u64) -> Result<()>;
    fn launch(&mut self, args: &[String]) -> Result<()>;
}
struct NativeAiRuntime {
    backend: Backend,
}
impl AiRuntime for NativeAiRuntime {
    fn windows(&mut self) -> Result<Value> {
        compositor_windows(&mut self.backend)
    }
    fn focus(&mut self, address: &Value) -> Result<()> {
        self.backend
            .action("focus_window", &[address.clone()], &json!({"timeout":10}))?;
        Ok(())
    }
    fn switch_workspace(&mut self, workspace: u64) -> Result<()> {
        self.backend.action(
            "switch_workspace",
            &[json!(workspace)],
            &json!({"current_monitor":true,"timeout":10}),
        )?;
        Ok(())
    }
    fn launch(&mut self, args: &[String]) -> Result<()> {
        launch(args)
    }
}
fn ai_launch_with(name: &str, paths: &Paths, runtime: &mut impl AiRuntime) -> Result<()> {
    if name == "tasks" {
        if let Some(window) = runtime
            .windows()?
            .as_array()
            .and_then(|rows| rows.iter().find(|row| row["class"] == "nacre-ai-dashboard"))
        {
            runtime.focus(&window["address"])?;
            return Ok(());
        }
    }
    if let Some(workspace) = ai_workspace(paths) {
        runtime.switch_workspace(workspace)?;
    }
    let arguments = launch_args(name, paths).ok_or("Unknown AI action")?;
    runtime.launch(&arguments)
}
fn ai_launch(name: &str, paths: &Paths) -> Result<()> {
    let backend = Backend::new(paths.clone(), std::env::vars().collect())?;
    ai_launch_with(name, paths, &mut NativeAiRuntime { backend })
}
fn action(name: &str, value: &str, paths: &Paths) -> Result<()> {
    let home = paths.home.to_string_lossy();
    let shell = format!("{home}/.local/share/nacre/shell");
    if ["power", "files", "terminal", "notifications", "updates"].contains(&name) {
        return launch(&launch_args(name, paths).ok_or("Unknown desktop action")?);
    }
    if ["tasks", "new", "resume"].contains(&name) {
        return ai_launch(name, paths);
    }
    if name == "volume" {
        let volume = volume_percent(value)?;
        return run("wpctl", &["set-volume", "@DEFAULT_AUDIO_SINK@", &volume]).map(|_| ());
    }
    if name == "brightness" {
        let amount = (value.parse::<i32>()?.clamp(0, 100) as f64 / 100.0)
            .max(0.01)
            .to_string();
        return run(
            &format!("{shell}/bin/qs"),
            &["-c", "nacre", "ipc", "call", "brightness", "set", &amount],
        )
        .map(|_| ());
    }
    if name == "workspace" {
        let number = workspace_number(value)?;
        let mut backend = Backend::new(paths.clone(), std::env::vars().collect())?;
        backend.action(
            "switch_workspace",
            &[json!(number)],
            &json!({"current_monitor":true,"timeout":10}),
        )?;
        return Ok(());
    }
    if name == "focus" {
        let mut backend = Backend::new(paths.clone(), std::env::vars().collect())?;
        let address = json!(value);
        let valid = backend.valid_window_id(&address);
        focus_address(&address, valid, |address| {
            Ok(backend.action("focus_window", &[address.clone()], &json!({"timeout":10}))?)
        })?;
        return Ok(());
    }
    if name == "app-windows" {
        let ids: Value = serde_json::from_str(value)?;
        let mut backend = Backend::new(paths.clone(), std::env::vars().collect())?;
        valid_window_group(&ids, |id| backend.valid_window_id(id))?;
        let clients = compositor_windows(&mut backend)?;
        let windows = app_window_rows(&ids, &clients);
        if windows.len() == 1 {
            let address = windows[0]["address"].clone();
            let valid = backend.valid_window_id(&address);
            focus_address(&address, valid, |id| {
                Ok(backend.action("focus_window", &[id.clone()], &json!({"timeout":10}))?)
            })?;
            return Ok(());
        }
        if windows.is_empty() {
            return Ok(());
        }
        let labels=windows.iter().map(|window|json!({"label":format!("{}  {}",window["workspace"]["id"].as_i64().unwrap_or(0),window["title"].as_str().unwrap_or("").replace('\n'," ").chars().take(120).collect::<String>()),"value":windows.iter().position(|row|row==window).unwrap().to_string()})).collect::<Vec<_>>();
        if let Some(selected) = pick("Choose window", &json!(labels)) {
            let choice = selected.parse::<usize>();
            if let Ok(index) = choice {
                if let Some(window) = windows.get(index) {
                    backend.action(
                        "focus_window",
                        &[window["address"].clone()],
                        &json!({"timeout":10}),
                    )?;
                }
            }
        }
        return Ok(());
    }
    if name == "wifi" {
        let mut command = Command::new("nmcli");
        command.args([
            "-t",
            "-f",
            "IN-USE,SSID,SIGNAL,SECURITY",
            "device",
            "wifi",
            "list",
            "--rescan",
            "no",
        ]);
        let output = crate::process::capture(command, None, Duration::from_secs(10))?;
        if !output.status.success() {
            return Err("Could not list Wi-Fi networks".into());
        }
        let mut seen = BTreeSet::new();
        let mut rows = Vec::new();
        for line in String::from_utf8_lossy(&output.stdout).lines() {
            let mut fields = Vec::new();
            let mut field = String::new();
            let mut escaped = false;
            for ch in line.chars() {
                if escaped {
                    field.push(ch);
                    escaped = false
                } else if ch == '\\' {
                    escaped = true
                } else if ch == ':' {
                    fields.push(std::mem::take(&mut field))
                } else {
                    field.push(ch)
                }
            }
            fields.push(field);
            if fields.len() < 4 {
                continue;
            }
            let ssid = fields[1].clone();
            if ssid.is_empty() || !seen.insert(ssid.clone()) {
                continue;
            }
            rows.push(json!({"label":format!("{}{}   {}%{}",if fields[0]=="*"{"● "}else{"  "},ssid,fields[2],if !fields[3].is_empty()&&fields[3]!="--"{"  "}else{""}),"value":ssid}));
        }
        if rows.is_empty() {
            return launch(&[
                "kitty".into(),
                "--class".into(),
                "nacre-control".into(),
                "--title".into(),
                "Wi-Fi connections".into(),
                "--".into(),
                "nmtui-connect".into(),
            ]);
        }
        if let Some(selected) = pick("Wi-Fi", &json!(rows)) {
            if rows.iter().any(|row| row["value"] == selected) {
                return launch(&[
                    "kitty".into(),
                    "--class".into(),
                    "nacre-control".into(),
                    "--title".into(),
                    "Connect Wi-Fi".into(),
                    "--".into(),
                    "nmcli".into(),
                    "--ask".into(),
                    "device".into(),
                    "wifi".into(),
                    "connect".into(),
                    selected,
                ]);
            }
        }
        return Ok(());
    }
    let direct = direct_args(name).ok_or_else(|| {
        tr(
            "helper.desktop-actions.unknown-desktop-action",
            "Unknown desktop action",
        )
    })?;
    let _ = run(
        &direct[0],
        &direct[1..].iter().map(String::as_str).collect::<Vec<_>>(),
    )?;
    Ok(())
}
pub fn run_cli(args: &[String]) -> Result<i32> {
    let name = args.first().map(String::as_str).unwrap_or("");
    let value = args.get(1).map(String::as_str).unwrap_or("");
    action(name, value, &Paths::environment()?)?;
    Ok(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    struct AiFixture {
        windows: Value,
        events: Vec<String>,
    }
    impl AiRuntime for AiFixture {
        fn windows(&mut self) -> Result<Value> {
            self.events.push("windows".into());
            Ok(self.windows.clone())
        }
        fn focus(&mut self, address: &Value) -> Result<()> {
            self.events.push(format!("focus:{address}"));
            Ok(())
        }
        fn switch_workspace(&mut self, workspace: u64) -> Result<()> {
            self.events.push(format!("workspace:{workspace}"));
            Ok(())
        }
        fn launch(&mut self, args: &[String]) -> Result<()> {
            self.events.push(format!("launch:{}", args[0]));
            Ok(())
        }
    }
    #[test]
    fn update_and_lock_routes_do_not_enter_brain() {
        let paths = Paths::new("/fixture/home");
        assert_eq!(
            launch_args("updates", &paths).unwrap().last().unwrap(),
            "/fixture/home/.local/share/nacre/shell/tools/updates.sh"
        );
        assert_eq!(
            direct_args("lock").unwrap(),
            vec!["loginctl", "lock-session"]
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        );
    }
    #[test]
    fn volume_is_clamped_and_invalid_focus_workspace_is_rejected() {
        assert_eq!(volume_percent("500").unwrap(), "100%");
        assert!(workspace_number("99").is_err());
        let called = Cell::new(false);
        let invalid = focus_address(&json!("; injected"), false, |_| {
            called.set(true);
            Ok(())
        });
        assert!(invalid.is_err());
        assert!(!called.get());
    }
    #[test]
    fn ai_actions_stay_on_current_workspace_without_assignment() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::new(root.path());
        assert_eq!(ai_workspace(&paths), None);
        let mut runtime = AiFixture {
            windows: json!([]),
            events: Vec::new(),
        };
        for command in ["new", "resume", "tasks"] {
            ai_launch_with(command, &paths, &mut runtime).unwrap();
        }
        assert_eq!(
            runtime
                .events
                .iter()
                .filter(|event| event.starts_with("launch:"))
                .count(),
            3
        );
        assert_eq!(
            runtime
                .events
                .iter()
                .filter(|event| *event == "windows")
                .count(),
            1
        );
        assert!(!runtime
            .events
            .iter()
            .any(|event| event.starts_with("workspace:")));
    }
    #[test]
    fn explicit_ai_workspace_is_read_from_profile() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::new(root.path());
        std::fs::create_dir_all(&paths.config).unwrap();
        std::fs::write(
            paths.config.join("profile.json"),
            r#"{"workspaceCount":10,"aiWorkspace":9}"#,
        )
        .unwrap();
        assert_eq!(ai_workspace(&paths), Some(9));
        let mut runtime = AiFixture {
            windows: json!([]),
            events: Vec::new(),
        };
        ai_launch_with("new", &paths, &mut runtime).unwrap();
        assert_eq!(runtime.events, vec!["workspace:9", "launch:kitty"]);
    }
    #[test]
    fn unknown_action_cannot_become_a_shell_command() {
        assert!(action("shell", "anything", &Paths::new("/tmp/nacre-test")).is_err());
    }
    #[test]
    fn window_query_falls_back_only_for_runtime_failure() {
        let fallback = compositor_windows_with(|| {
            Err(crate::compositor::niri_state::Error {
                kind: "RuntimeError",
                message: "offline".into(),
            }
            .into())
        })
        .unwrap();
        assert_eq!(fallback, json!([]));
        let malformed = compositor_windows_with(|| {
            Err(crate::compositor::niri_state::Error {
                kind: "ValueError",
                message: "bad response".into(),
            }
            .into())
        });
        assert_eq!(malformed.unwrap_err().to_string(), "bad response");
        let failed = focus_address(&json!("0x123"), true, |_| {
            Err(crate::compositor::niri_state::Error {
                kind: "RuntimeError",
                message: "offline".into(),
            }
            .into())
        });
        assert_eq!(failed.unwrap_err().to_string(), "offline");
    }
    #[test]
    fn backend_owned_opaque_window_is_selected_and_focused() {
        let ids = json!(["window-42"]);
        let clients = json!([{"address":"window-42","workspace":{"id":2},"title":"Editor"}]);
        valid_window_group(&ids, |id| id == "window-42").unwrap();
        let windows = app_window_rows(&ids, &clients);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0]["address"], "window-42");
        let selected = Cell::new(false);
        focus_address(&windows[0]["address"], true, |id| {
            selected.set(id == "window-42");
            Ok(())
        })
        .unwrap();
        assert!(selected.get());
        let ordered = app_window_rows(
            &json!(["b", "a"]),
            &json!([{"address":"a","workspace":{"id":2},"title":"Z"},{"address":"b","workspace":{"id":1},"title":"A"}]),
        );
        assert_eq!(ordered[0]["address"], "b");
    }
    #[test]
    fn existing_ai_dashboard_is_focused_without_switching_or_relaunching() {
        let root = tempfile::tempdir().unwrap();
        let paths = Paths::new(root.path());
        let mut runtime = AiFixture {
            windows: json!([{"class":"nacre-ai-dashboard","address":"window-42"}]),
            events: Vec::new(),
        };
        ai_launch_with("tasks", &paths, &mut runtime).unwrap();
        assert_eq!(runtime.events, vec!["windows", "focus:\"window-42\""]);
    }
}
