//! Complete native Niri boundary, sharing one lazy request connection per caller.
use super::{
    niri_actions, niri_config, niri_desktop, niri_events, niri_night_light, niri_state::Error,
    niri_transport,
};
use crate::paths::Paths;
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
type Result<T> = std::result::Result<T, Error>;
fn failure(kind: &'static str, message: impl Into<String>) -> Error {
    Error {
        kind,
        message: message.into(),
    }
}
pub fn capabilities() -> Value {
    json!({"stableWorkspaceIndices":false,"globalWorkspaces":false,"perOutputWorkspaces":true,"scrollingLayout":true,"floatingPlacement":true,"pinWindow":false,"windowCapture":false,"nativeWindowScreenshot":true,"shapedBackgroundBlur":false,"cursorPosition":false,"nativePanelDrag":false,"windowHide":false,"windowTrash":false,"scratchpad":false,"windowGroups":true,"windowForceKill":false,"pseudoTiling":false,"layoutSplit":false,"keymaps":false,"nonConsumingShortcuts":false,"bindingIntrospection":false,"fullscreenState":false,"shortcutRelease":false,"desktopOptions":["animations","blur","shadow","followMouse","naturalScroll","gapsIn","borderSize","rounding"],"monitorConfiguration":true,"monitorMirroring":false,"nightLightController":true})
}
pub struct Backend {
    pub actions: niri_actions::Backend<niri_transport::Connection>,
    pub paths: Paths,
    environment: BTreeMap<String, String>,
}
impl Backend {
    pub fn new(paths: Paths, environment: BTreeMap<String, String>) -> Self {
        let socket = environment
            .get("NIRI_SOCKET")
            .map(String::as_str)
            .unwrap_or("");
        Self {
            actions: niri_actions::Backend {
                transport: niri_transport::Connection::new(socket),
            },
            paths,
            environment,
        }
    }
    pub fn session_id(&self) -> &str {
        self.environment
            .get("NIRI_SOCKET")
            .map(String::as_str)
            .unwrap_or("")
    }
    pub fn version_command(&self) -> Value {
        json!(["niri", "--version"])
    }
    pub fn required_commands(&self) -> Value {
        json!(["niri"])
    }
    pub fn diagnostic_commands(&self) -> Value {
        json!({"runningVersion":["niri","msg","--json","version"],"installedVersion":["niri","--version"],"allMonitors":["niri","msg","--json","outputs"],"configErrors":["niri","validate"]})
    }
    pub fn environment_updates(&self) -> Value {
        Self::environment_updates_from(&self.environment)
    }
    pub fn environment_updates_from(environment: &BTreeMap<String, String>) -> Value {
        let mut result = Map::new();
        for name in [
            "NIRI_SOCKET",
            "WAYLAND_DISPLAY",
            "XDG_RUNTIME_DIR",
            "NACRE_COMPOSITOR",
        ] {
            if let Some(value) = environment.get(name).filter(|value| !value.is_empty()) {
                result.insert(name.into(), json!(value));
            }
        }
        Value::Object(result)
    }
    pub fn config_path(&self, kind: &str) -> Result<PathBuf> {
        niri_config::config_path(
            &self.paths.config,
            kind,
            self.environment
                .get("NACRE_NIRI_DESKTOP_CONFIG")
                .filter(|value| !value.is_empty())
                .map(Path::new),
        )
    }
    pub fn configuration(&self) -> niri_desktop::Configuration {
        niri_desktop::Configuration::new(
            &self.paths,
            self.environment
                .get("NACRE_NIRI_CONFIG")
                .filter(|value| !value.is_empty())
                .or_else(|| self.environment.get("NIRI_CONFIG"))
                .map(Path::new),
            self.environment
                .get("NACRE_NIRI_DESKTOP_CONFIG")
                .filter(|value| !value.is_empty())
                .map(Path::new),
        )
    }
    pub fn night_light_command(&self, enabled: bool) -> Value {
        json!(niri_night_light::command(
            enabled,
            self.environment
                .get("WAYLAND_DISPLAY")
                .map(String::as_str)
                .unwrap_or(""),
            self.environment
                .get("XDG_RUNTIME_DIR")
                .map(String::as_str)
                .unwrap_or("")
        ))
    }
    pub fn binding_source_files(&self) -> Result<Vec<PathBuf>> {
        let default = self.paths.config.join("niri");
        let parent = self
            .environment
            .get("NACRE_NIRI_CONFIG")
            .filter(|value| !value.is_empty())
            .and_then(|value| Path::new(value).parent())
            .unwrap_or(&default);
        let entries = match fs::read_dir(parent) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(error) => return Err(failure("OSError", error.to_string())),
        };
        let mut paths = Vec::new();
        for entry in entries {
            let path = entry
                .map_err(|error| failure("OSError", error.to_string()))?
                .path();
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".kdl") && name.contains("bindings"))
            {
                paths.push(path);
            }
        }
        paths.sort();
        Ok(paths)
    }
    pub fn capture_bindings(&self, path: &Path, kind: &str) -> Result<Value> {
        niri_config::bindings(
            &fs::read_to_string(path).map_err(|error| failure("OSError", error.to_string()))?,
            kind,
        )
    }
    pub fn query(
        &mut self,
        name: &str,
        timeout: Duration,
        max_bytes: Option<usize>,
    ) -> Result<Value> {
        if name == "bindings" {
            let mut records = Vec::new();
            for path in self.binding_source_files()? {
                let captured = self.capture_bindings(&path, "bindings")?;
                records.extend(
                    captured
                        .as_array()
                        .ok_or_else(|| failure("ValueError", "Malformed binding records"))?
                        .clone(),
                );
            }
            return niri_config::binding_query(&Value::Array(records));
        }
        self.actions.query_bounded(
            name,
            timeout.as_secs_f64(),
            max_bytes.unwrap_or(4 * 1024 * 1024),
        )
    }
    pub fn query_result(&mut self, name: &str, timeout: Duration, check: bool) -> Result<Value> {
        match self.query(name, timeout, None) {
            Ok(value) => Ok(
                json!({"code":0,"stdout":crate::json::encode(&value).map_err(|error|failure("ValueError",error.to_string()))?,"stderr":""}),
            ),
            Err(error) if !check => Ok(json!({"code":1,"stdout":"","stderr":error.message})),
            Err(error) => Err(error),
        }
    }
    pub fn events(&self) -> Result<niri_events::Events<niri_transport::Connection>> {
        if self.session_id().is_empty() {
            return Err(failure("RuntimeError", "NIRI_SOCKET is unavailable"));
        }
        let hover = self
            .environment
            .get("NACRE_NIRI_HOVER_STREAM")
            .is_none_or(|value| value != "0");
        let socket = niri_events::EventSocket::connect(Path::new(self.session_id()), hover)?;
        niri_events::Events::new(socket, niri_transport::Connection::new(self.session_id()))
    }
    pub fn set_desktop_options(&mut self, preferences: &Value, timeout: Duration) -> Result<Value> {
        self.configuration().set(
            preferences,
            timeout,
            &mut niri_desktop::Native {
                backend: &mut self.actions,
            },
        )
    }
    pub fn config_errors(&self, timeout: Duration) -> Result<String> {
        let output = crate::process::run("niri", &["validate".into()], None, timeout)
            .map_err(|error| failure("RuntimeError", error.to_string()))?;
        if output.status.success() {
            return Ok(String::new());
        }
        let bytes = if output.stderr.is_empty() {
            output.stdout
        } else {
            output.stderr
        };
        Ok(String::from_utf8(bytes)
            .map_err(|error| failure("UnicodeDecodeError", error.to_string()))?
            .trim()
            .into())
    }
    pub fn night_light_external_socket(&self, active: bool) -> crate::Result<bool> {
        let own = if active {
            Some(crate::systemd::status(niri_night_light::SERVICE)?.pid)
        } else {
            None
        };
        niri_night_light::external_controller(
            Path::new("/proc"),
            own,
            self.environment
                .get("WAYLAND_DISPLAY")
                .map(String::as_str)
                .unwrap_or(""),
            self.environment
                .get("XDG_RUNTIME_DIR")
                .map(String::as_str)
                .unwrap_or(""),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_loader_fallback_keeps_explicit_override() {
        let mut environment =
            BTreeMap::from([("NIRI_CONFIG".into(), "/session/config.kdl".into())]);
        let backend = Backend::new(Paths::new("/case/home"), environment.clone());
        assert_eq!(
            backend.configuration().loader,
            Path::new("/session/config.kdl")
        );
        environment.insert("NACRE_NIRI_CONFIG".into(), "/explicit/config.kdl".into());
        let backend = Backend::new(Paths::new("/case/home"), environment);
        assert_eq!(
            backend.configuration().loader,
            Path::new("/explicit/config.kdl")
        );
    }
    #[test]
    fn frozen_backend_metadata_and_lazy_construction() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-metadata-reference.json"
        ))?;
        for case in reference["cases"].as_array().ok_or("Missing metadata")? {
            let environment = case["request"]
                .get("environment")
                .and_then(Value::as_object)
                .map(|environment| {
                    environment
                        .iter()
                        .map(|(key, value)| {
                            Ok((
                                key.clone(),
                                value.as_str().ok_or("Malformed environment")?.into(),
                            ))
                        })
                        .collect::<crate::Result<BTreeMap<String, String>>>()
                })
                .transpose()?
                .unwrap_or_default();
            let backend = Backend::new(Paths::new("/case/home"), environment);
            assert!(!backend.actions.transport.connected());
            let value = match case["request"]["method"].as_str() {
                Some("session_id") => json!(backend.session_id()),
                Some("version_command") => backend.version_command(),
                Some("required_commands") => backend.required_commands(),
                Some("diagnostic_commands") => backend.diagnostic_commands(),
                Some("environment_updates") => backend.environment_updates(),
                Some("capabilities") => capabilities(),
                _ => return Err("Unknown metadata operation".into()),
            };
            assert_eq!(
                crate::json::encode(&value)? + "\n",
                case["observed"]["stdout"],
                "{}",
                case["request"]["name"]
            );
        }
        Ok(())
    }
    #[test]
    fn binding_query_uses_configuration_files_without_opening_ipc() -> crate::Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        fs::create_dir_all(paths.config.join("niri"))?;
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-bindings-reference.json"
        ))?;
        let case = &reference["cases"][1];
        fs::write(
            paths.config.join("niri/bindings.kdl"),
            case["request"]["text"].as_str().ok_or("Missing bindings")?,
        )?;
        let mut backend = Backend::new(paths, BTreeMap::new());
        let value = backend.query("bindings", Duration::from_secs(3), None)?;
        assert_eq!(
            crate::json::encode(&value)? + "\n",
            case["observed"]["stdout"]
        );
        assert!(!backend.actions.transport.connected());
        assert!(
            matches!(backend.events(),Err(error) if error.message=="NIRI_SOCKET is unavailable")
        );
        Ok(())
    }
}
