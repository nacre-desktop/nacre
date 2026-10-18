//! Original settings validation and geometry, frozen before the controller port.
use nacre_helper::desktop_settings as settings;
use serde_json::{json, Value};
#[test]
fn frozen_validation_defaults_and_preset_scope() {
    let reference: Value = serde_json::from_str(include_str!(
        "fixtures/desktop-settings-validation-reference.json"
    ))
    .unwrap();
    for (key, value) in reference["defaults"].as_object().unwrap() {
        assert_eq!(&settings::defaults()[key], value, "{key}");
    }
    assert_eq!(json!(settings::INPUT_KEYS), reference["inputKeys"]);
    assert_eq!(json!(settings::preset_keys()), reference["presetKeys"]);
    for case in reference["validation"].as_array().unwrap() {
        let actual = match settings::validate(case["key"].as_str().unwrap(), &case["value"], false)
        {
            Ok(value) => json!({"result":value}),
            Err(error) => json!({"error":error.to_string(),"type":"ValueError"}),
        };
        let mut expected = case.clone();
        expected.as_object_mut().unwrap().remove("key");
        expected.as_object_mut().unwrap().remove("value");
        assert_eq!(actual, expected, "{}: {}", case["key"], case["value"]);
    }
    assert_eq!(
        json!({"result":settings::validate("language",&json!("pseudo"),true).unwrap()}),
        reference["developerPseudo"]
    );
}
#[test]
fn frozen_additive_niri_preferences() {
    let reference: Value = serde_json::from_str(include_str!(
        "fixtures/desktop-settings-niri-reference.json"
    ))
    .unwrap();
    for (key, value) in reference["defaults"].as_object().unwrap() {
        assert_eq!(&settings::defaults()[key], value);
        assert!(!settings::preset_keys().contains(key));
    }
    for case in reference["validation"].as_array().unwrap() {
        let actual = match settings::validate(case["key"].as_str().unwrap(), &case["value"], false)
        {
            Ok(value) => json!({"result":value}),
            Err(error) => json!({"error":error.to_string(),"type":"ValueError"}),
        };
        let mut expected = case.clone();
        expected.as_object_mut().unwrap().remove("key");
        expected.as_object_mut().unwrap().remove("value");
        assert_eq!(actual, expected, "{}: {}", case["key"], case["value"]);
    }
}
#[test]
fn frozen_scaled_rotated_and_mirrored_geometry() {
    let reference: Value = serde_json::from_str(include_str!(
        "fixtures/desktop-settings-validation-reference.json"
    ))
    .unwrap();
    for case in reference["plans"].as_array().unwrap() {
        let actual = match settings::display_plan(
            case["monitors"].as_array().unwrap(),
            case["primary"].as_str().unwrap(),
            case["mode"].as_str().unwrap(),
        ) {
            Ok(value) => json!({"result":value}),
            Err(error) => json!({"error":error.to_string(),"type":"ValueError"}),
        };
        let mut expected = case.clone();
        for key in ["monitors", "primary", "mode"] {
            expected.as_object_mut().unwrap().remove(key);
        }
        assert_eq!(actual, expected);
    }
}

#[test]
fn frozen_cli_help_errors_and_exit_codes() {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/desktop-settings-cli-reference.json")).unwrap();
    for case in reference["cases"].as_array().unwrap() {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .arg("desktop-settings")
            .args(
                case["args"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.as_str().unwrap()),
            )
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            case["stdout"],
            "{}",
            case["args"]
        );
        assert_eq!(
            String::from_utf8(output.stderr).unwrap(),
            case["stderr"],
            "{}",
            case["args"]
        );
        assert_eq!(
            output.status.code(),
            case["exit"].as_i64().map(|v| v as i32)
        );
    }
}

use nacre_helper::{
    desktop_settings::{Effect, Runtime, Settings},
    paths::Paths,
    Result,
};
struct GuardRuntime {
    config: String,
    fail_arm: bool,
    fail_query: bool,
    applied: usize,
}
impl Runtime for GuardRuntime {
    fn compositor(&mut self, operation: &str, _args: &[Value], _kwargs: &Value) -> Result<Value> {
        Ok(match operation {
            "backend_name" => json!("hyprland"),
            "capabilities" => json!({"monitorConfiguration":true,"monitorMirroring":true}),
            "desktop_option_keys" => json!([]),
            "config_path" => json!(self.config),
            "query" => {
                if self.fail_query {
                    return Err("fixture monitor failure".into());
                }
                json!([
    {"name":"DP-1","width":1920,"height":1080,"scale":1,"x":0,"y":0,"transform":0},
    {"name":"DP-2","width":1920,"height":1080,"scale":1,"x":1920,"y":0,"transform":0}])
            }
            "render_desktop_options" | "render_monitors" => json!(""),
            "configure_monitors" => {
                self.applied += 1;
                Value::Null
            }
            _ => return Err(format!("Unexpected guard operation {operation}").into()),
        })
    }
    fn effect(&mut self, effect: Effect<'_>) -> Result<()> {
        if matches!(effect, Effect::Rollback { .. }) && self.fail_arm {
            return Err("Automatic display rollback could not be armed".into());
        }
        Ok(())
    }
    fn token(&mut self) -> String {
        "fixed-token".into()
    }
}
fn guarded(home: &std::path::Path) -> Settings<GuardRuntime> {
    let paths = Paths::new(home);
    let mut settings = Settings::new(
        paths,
        GuardRuntime {
            config: home.join("desktop.lua").to_string_lossy().into_owned(),
            fail_arm: false,
            fail_query: false,
            applied: 0,
        },
    )
    .unwrap();
    std::fs::create_dir_all(settings.state.parent().unwrap()).unwrap();
    std::fs::write(&settings.state, "{\"dnd\": false}").unwrap();
    settings.environment.clear();
    settings
}
#[test]
fn unarmed_rollback_prevents_output_changes_and_preserves_state() {
    let home = tempfile::tempdir().unwrap();
    let mut settings = guarded(home.path());
    settings.runtime.fail_arm = true;
    let before = std::fs::read(&settings.state).unwrap();
    let result = settings
        .execute("display", &["extend-right".into(), "DP-1".into()])
        .unwrap();
    assert_eq!(
        result["error"],
        "Automatic display rollback could not be armed"
    );
    assert_eq!(settings.runtime.applied, 0);
    assert!(!settings.pending.exists());
    assert_eq!(std::fs::read(&settings.state).unwrap(), before);
}
#[test]
fn wrong_rollback_token_never_applies_or_deletes_another_transaction() {
    let home = tempfile::tempdir().unwrap();
    let mut settings = guarded(home.path());
    std::fs::write(
        &settings.pending,
        "{\"token\":\"correct\",\"previous\":[],\"monitors\":[]}",
    )
    .unwrap();
    let result = settings.execute("revert", &["wrong".into()]).unwrap();
    assert!(result.get("error").is_none());
    assert_eq!(settings.runtime.applied, 0);
    assert!(settings.pending.exists());
}
#[test]
fn linked_locks_and_state_never_write_through_to_a_target() {
    for lock in [false, true] {
        let home = tempfile::tempdir().unwrap();
        let mut settings = guarded(home.path());
        let target = home.path().join("untouched");
        std::fs::write(&target, "private").unwrap();
        let link = if lock {
            settings.state.with_extension("lock")
        } else {
            settings.state.clone()
        };
        if link.exists() {
            std::fs::remove_file(&link).unwrap();
        }
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(settings
            .execute("set", &["dnd".into(), "true".into()])
            .is_err());
        assert_eq!(std::fs::read_to_string(target).unwrap(), "private");
    }
}
#[test]
fn unavailable_monitors_use_the_existing_json_error_ui() {
    let home = tempfile::tempdir().unwrap();
    let mut settings = guarded(home.path());
    settings.runtime.fail_query = true;
    let result = settings.execute("state", &[]).unwrap();
    assert_eq!(result["error"], "fixture monitor failure");
    assert_eq!(result["data"]["dnd"], false);
}
#[test]
fn newer_preferences_are_preserved_without_downgrade() {
    let home = tempfile::tempdir().unwrap();
    let mut settings = guarded(home.path());
    let original = b"{\"version\":999,\"dnd\":false}";
    std::fs::write(&settings.state, original).unwrap();
    assert!(settings.execute("state", &[]).is_err());
    assert_eq!(std::fs::read(&settings.state).unwrap(), original);
}

struct DenyManager {
    captured: std::sync::Arc<std::sync::Mutex<Vec<(String, zbus::zvariant::OwnedValue)>>>,
}
#[zbus::interface(name = "org.freedesktop.systemd1.Manager")]
impl DenyManager {
    fn subscribe(&self) {}
    fn start_transient_unit(
        &self,
        name: &str,
        mode: &str,
        properties: Vec<(String, zbus::zvariant::OwnedValue)>,
        _auxiliary: Vec<(String, Vec<(String, zbus::zvariant::OwnedValue)>)>,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedObjectPath> {
        assert!(name.starts_with("nacre-display-revert-") && name.ends_with(".service"));
        assert_eq!(mode, "fail");
        *self.captured.lock().unwrap() = properties;
        Err(zbus::fdo::Error::AccessDenied("fixture refusal".into()))
    }
}
struct Daemon(std::process::Child);
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
#[test]
fn direct_user_manager_call_binds_private_context_and_exports_no_credentials() {
    use std::{
        io::{BufRead, BufReader, Write},
        process::{Command, Stdio},
        sync::{Arc, Mutex},
    };
    let home = tempfile::tempdir().unwrap();
    let mut daemon = Daemon(
        Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address"])
            .arg(format!(
                "--address=unix:path={}",
                home.path().join("bus").display()
            ))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut address = String::new();
    BufReader::new(daemon.0.stdout.take().unwrap())
        .read_line(&mut address)
        .unwrap();
    let captured = Arc::new(Mutex::new(Vec::new()));
    let _server = zbus::blocking::connection::Builder::address(address.trim())
        .unwrap()
        .name("org.freedesktop.systemd1")
        .unwrap()
        .serve_at(
            "/org/freedesktop/systemd1",
            DenyManager {
                captured: captured.clone(),
            },
        )
        .unwrap()
        .build()
        .unwrap();
    let binary = std::env::var_os("NACRE_DESKTOP_TEST")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/debug/examples/desktop-settings-test")
        });
    let mut child = Command::new(binary)
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .env("XDG_CONFIG_HOME", home.path().join("config"))
        .env("XDG_STATE_HOME", home.path().join("state"))
        .env("XDG_CACHE_HOME", home.path().join("cache"))
        .env("XDG_DATA_HOME", home.path().join("data"))
        .env("XDG_RUNTIME_DIR", home.path().join("runtime"))
        .env("HYPRLAND_INSTANCE_SIGNATURE", "private-instance")
        .env("NACRE_COMPOSITOR", "hyprland")
        .env("DBUS_SESSION_BUS_ADDRESS", address.trim())
        .env("OPENAI_API_KEY", "synthetic-do-not-export")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"{\"operation\":\"native-rollback\"}\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    let response: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        response["error"],
        "Automatic display rollback could not be armed"
    );
    let properties = captured.lock().unwrap();
    let property = |name: &str| {
        properties
            .iter()
            .find(|(key, _)| key == name)
            .unwrap()
            .1
            .try_clone()
            .unwrap()
    };
    let environment: Vec<String> = property("Environment").try_into().unwrap();
    assert!(environment.contains(&"NACRE_COMPOSITOR=hyprland".into()));
    assert!(environment.contains(&"HYPRLAND_INSTANCE_SIGNATURE=private-instance".into()));
    assert!(environment.contains(&format!(
        "XDG_RUNTIME_DIR={}",
        home.path().join("runtime").display()
    )));
    assert!(environment.contains(&format!(
        "XDG_CONFIG_HOME={}",
        home.path().join("config").display()
    )));
    assert!(
        !environment
            .iter()
            .any(|value| value.contains("OPENAI_API_KEY")
                || value.contains("synthetic-do-not-export"))
    );
    let commands: Vec<(String, Vec<String>, bool)> = property("ExecStart").try_into().unwrap();
    assert_eq!(commands.len(), 1);
    assert_eq!(
        &commands[0].1[1..],
        &[
            "desktop-settings",
            "rollback-after",
            "0123456789abcdef0123456789abcdef"
        ]
    );
    assert!(!commands[0].2);
    assert!(!String::from_utf8_lossy(&output.stdout).contains("synthetic-do-not-export"));
}
