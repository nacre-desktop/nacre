//! Validated Niri desktop publication, preserving previous bytes on reload failure.
use super::{niri_config, niri_state::Error};
use serde_json::Value;
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};
type Result<T> = std::result::Result<T, Error>;
fn error(kind: &'static str, message: impl Into<String>) -> Error {
    Error {
        kind,
        message: message.into(),
    }
}
fn io_error(value: std::io::Error) -> Error {
    error("OSError", value.to_string())
}
fn update_host_scroll_factors(source: &str, mouse: f64, touchpad: f64) -> Result<String> {
    fn set_factor(document: &mut kdl::KdlDocument, device: &str, value: f64) -> Result<()> {
        if document.get(device).is_none() {
            let mut node = kdl::KdlNode::new(device);
            node.set_children(kdl::KdlDocument::new());
            document.nodes_mut().push(node);
        }
        let device = document
            .get_mut(device)
            .ok_or_else(|| error("ValueError", "Invalid generated input configuration"))?;
        if device.children().is_none() {
            device.set_children(kdl::KdlDocument::new());
        }
        let children = device
            .children_mut()
            .as_mut()
            .ok_or_else(|| error("ValueError", "Invalid generated input configuration"))?;
        if children.get("scroll-factor").is_none() {
            children
                .nodes_mut()
                .push(kdl::KdlNode::new("scroll-factor"));
        }
        let factor = children
            .get_mut("scroll-factor")
            .ok_or_else(|| error("ValueError", "Invalid generated input configuration"))?;
        factor.entries_mut().clear();
        factor.entries_mut().push(kdl::KdlEntry::new(value));
        Ok(())
    }
    let mut document: kdl::KdlDocument = source
        .parse()
        .map_err(|value: kdl::KdlError| error("ValueError", value.to_string()))?;
    let input = document
        .get_mut("input")
        .ok_or_else(|| error("ValueError", "Managed host input configuration is missing"))?;
    if input.children().is_none() {
        return Err(error(
            "ValueError",
            "Managed host input configuration is missing",
        ));
    }
    let input = input
        .children_mut()
        .as_mut()
        .ok_or_else(|| error("ValueError", "Managed host input configuration is missing"))?;
    set_factor(input, "mouse", mouse)?;
    set_factor(input, "touchpad", touchpad)?;
    Ok(document.to_string())
}
pub trait Runtime {
    fn monitors(&mut self, timeout: Duration) -> Result<Value>;
    fn validate(&mut self, path: &Path, timeout: Duration) -> Result<()>;
    fn reload(&mut self, timeout: Duration) -> Result<Value>;
    /// The running compositor advertises `cursor_shake` (stock niri does not).
    fn cursor_shake(&mut self) -> bool {
        false
    }
}
pub struct Configuration {
    pub home: PathBuf,
    pub config: PathBuf,
    pub loader: PathBuf,
    pub desktop: PathBuf,
    pub host: Option<PathBuf>,
}
impl Configuration {
    pub fn new(paths: &crate::paths::Paths, loader: Option<&Path>, desktop: Option<&Path>) -> Self {
        Self {
            home: paths.home.clone(),
            config: paths.config.clone(),
            loader: loader
                .map(Path::to_owned)
                .unwrap_or_else(|| paths.config.join("niri/config.kdl")),
            desktop: desktop
                .map(Path::to_owned)
                .unwrap_or_else(|| paths.config.join("niri/desktop.kdl")),
            host: None,
        }
    }
    pub fn active(
        paths: &crate::paths::Paths,
        env: &std::collections::BTreeMap<String, String>,
    ) -> Self {
        let explicit_loader = ["NACRE_NIRI_CONFIG", "NIRI_CONFIG"].iter().find_map(|key| {
            env.get(*key)
                .filter(|value| !value.is_empty())
                .map(|value| PathBuf::from(value.as_str()))
        });
        let mut loader = explicit_loader
            .clone()
            .unwrap_or_else(|| super::niri_bindings::loader(paths, env));
        let scoped_config = paths.config.parent().unwrap_or(&paths.config);
        if explicit_loader.is_none()
            && !loader.starts_with(&paths.home)
            && !loader.starts_with(scoped_config)
        {
            let mut scoped = env.clone();
            scoped.remove("NIRI_SOCKET");
            loader = super::niri_bindings::loader(paths, &scoped);
        }
        let session = paths.share.join("niri-on-pc/session/config");
        let files = super::niri_bindings::source_files(&loader, &paths.home);
        let included = |name: &str| {
            let generated = session.join(name);
            files
                .iter()
                .any(|path| path == &generated)
                .then_some(generated)
        };
        let standard_desktop = paths.config.join("niri/desktop.kdl");
        let explicit_desktop = env
            .get("NACRE_NIRI_DESKTOP_CONFIG")
            .filter(|value| !value.is_empty())
            .map(|value| PathBuf::from(value.as_str()));
        let desktop = explicit_desktop
            .or_else(|| included("desktop.kdl"))
            .or_else(|| {
                files
                    .iter()
                    .any(|path| path == &standard_desktop)
                    .then_some(standard_desktop)
            });
        let mut config = Self::new(paths, Some(&loader), desktop.as_deref());
        // The session builder owns these generated siblings. A user's custom includes
        // are never treated as generated settings targets.
        if config.desktop.parent() == Some(session.as_path()) {
            config.host = included("host.kdl");
        }
        config
    }
    pub fn ensure_loader(&self) -> Result<()> {
        if !self.loader.exists() {
            return Err(error("RuntimeError","Managed niri desktop include missing; regenerate the isolated session configuration"));
        }
        let files = super::niri_bindings::source_files(&self.loader, &self.home);
        if !files.iter().any(|path| path == &self.desktop)
            || self
                .host
                .as_ref()
                .is_some_and(|host| !files.iter().any(|path| path == host))
        {
            return Err(error("RuntimeError","Managed niri desktop include missing; regenerate the isolated session configuration"));
        }
        Ok(())
    }
    pub fn set(
        &self,
        preferences: &Value,
        timeout: Duration,
        runtime: &mut impl Runtime,
    ) -> Result<Value> {
        self.ensure_loader()?;
        let state = self.config.join("desktop.json");
        let mut values = if state.exists() {
            crate::json::parse_value(&fs::read_to_string(state).map_err(io_error)?)
                .map_err(|value| error("JSONDecodeError", value.to_string()))?
        } else {
            serde_json::json!({})
        };
        let values_map = values
            .as_object_mut()
            .ok_or_else(|| error("TypeError", "Desktop preferences must be an object"))?;
        let preferences = preferences
            .as_object()
            .ok_or_else(|| error("TypeError", "Desktop preferences must be an object"))?;
        let input = crate::software_input::preferences(&self.config)
            .map_err(|e| error("RuntimeError", e.to_string()))?;
        for key in ["keyboardLayouts", "touchpadEnabled", "touchpadDevices"] {
            if let Some(value) = input.get(key) {
                values_map.insert(key.into(), value.clone());
            }
        }
        values_map.extend(preferences.clone());
        let parent = self
            .desktop
            .parent()
            .ok_or_else(|| error("ValueError", "Invalid desktop configuration path"))?;
        let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
        let previous = if self.desktop.exists() {
            Some(fs::read(&self.desktop).map_err(io_error)?)
        } else {
            None
        };
        let desktop_permissions = fs::metadata(&self.desktop)
            .ok()
            .map(|metadata| metadata.permissions());
        if let Some(permissions) = &desktop_permissions {
            temporary
                .as_file()
                .set_permissions(permissions.clone())
                .map_err(io_error)?;
        }
        temporary
            .write_all(
                niri_config::desktop_with(
                    &values,
                    values.get("shakeCursor") == Some(&Value::Bool(true)) && runtime.cursor_shake(),
                )?
                .as_bytes(),
            )
            .map_err(io_error)?;
        let monitors = match values
            .get("compositorDisplays")
            .and_then(|value| value.get("niri"))
        {
            Some(value) if !value.is_null() => value.clone(),
            _ => runtime.monitors(timeout)?,
        };
        temporary
            .write_all(niri_config::monitors(&monitors)?.as_bytes())
            .map_err(io_error)?;
        temporary.flush().map_err(io_error)?;
        runtime.validate(temporary.path(), timeout)?;
        let (host_path, host_candidate, host_previous, host_permissions) =
            if let Some(path) = &self.host {
                let mut candidate = tempfile::NamedTempFile::new_in(
                    path.parent()
                        .ok_or_else(|| error("ValueError", "Invalid host configuration path"))?,
                )
                .map_err(io_error)?;
                let previous = fs::read(path).map_err(io_error)?;
                let permissions = fs::metadata(path)
                    .ok()
                    .map(|metadata| metadata.permissions());
                if let Some(permissions) = &permissions {
                    candidate
                        .as_file()
                        .set_permissions(permissions.clone())
                        .map_err(io_error)?;
                }
                let mouse = values
                    .get("mouseScrollSpeed")
                    .and_then(Value::as_f64)
                    .unwrap_or_else(crate::desktop_settings::mouse_scroll_speed_default);
                let touchpad = values
                    .get("scrollSpeed")
                    .and_then(Value::as_f64)
                    .unwrap_or_else(crate::desktop_settings::scroll_speed_default);
                candidate
                    .write_all(
                        update_host_scroll_factors(
                            std::str::from_utf8(&previous)
                                .map_err(|value| error("UnicodeDecodeError", value.to_string()))?,
                            mouse,
                            touchpad,
                        )?
                        .as_bytes(),
                    )
                    .map_err(io_error)?;
                candidate.flush().map_err(io_error)?;
                runtime.validate(candidate.path(), timeout)?;
                (
                    Some(path.clone()),
                    Some(candidate),
                    Some(previous),
                    permissions,
                )
            } else {
                (None, None, None, None)
            };
        temporary
            .persist(&self.desktop)
            .map_err(|value| io_error(value.error))?;
        if let (Some(path), Some(candidate)) = (&host_path, host_candidate) {
            if let Err(value) = candidate.persist(path) {
                restore(
                    &self.desktop,
                    previous.as_deref(),
                    desktop_permissions.as_ref(),
                )?;
                return Err(io_error(value.error));
            }
        }
        match runtime.reload(timeout) {
            Ok(value) => Ok(value),
            Err(failure) => {
                restore(
                    &self.desktop,
                    previous.as_deref(),
                    desktop_permissions.as_ref(),
                )?;
                if let Some(path) = host_path {
                    restore(&path, host_previous.as_deref(), host_permissions.as_ref())?;
                }
                Err(failure)
            }
        }
    }
}
fn restore(path: &Path, bytes: Option<&[u8]>, permissions: Option<&fs::Permissions>) -> Result<()> {
    match bytes {
        Some(bytes) => {
            let parent = path
                .parent()
                .ok_or_else(|| error("ValueError", "Invalid configuration path"))?;
            let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(io_error)?;
            temporary.write_all(bytes).map_err(io_error)?;
            temporary.flush().map_err(io_error)?;
            if let Some(permissions) = permissions {
                temporary
                    .as_file()
                    .set_permissions(permissions.clone())
                    .map_err(io_error)?;
            }
            temporary
                .persist(path)
                .map_err(|value| io_error(value.error))?;
        }
        None if path.exists() => fs::remove_file(path).map_err(io_error)?,
        None => (),
    }
    Ok(())
}
pub struct Native<'a> {
    pub backend: &'a mut super::niri_actions::Backend<super::niri_transport::Connection>,
}
impl Runtime for Native<'_> {
    fn cursor_shake(&mut self) -> bool {
        self.backend
            .transport
            .nacre_capabilities(Duration::from_millis(500))
            .cursor_shake
    }
    fn monitors(&mut self, timeout: Duration) -> Result<Value> {
        self.backend
            .query_bounded("monitors", timeout.as_secs_f64(), 4 * 1024 * 1024)
    }
    fn validate(&mut self, path: &Path, timeout: Duration) -> Result<()> {
        let output = crate::process::run(
            "niri",
            &[
                "validate".into(),
                "-c".into(),
                path.to_str()
                    .ok_or_else(|| error("ValueError", "Desktop configuration path is not UTF-8"))?
                    .into(),
            ],
            None,
            timeout,
        )
        .map_err(|value| error("RuntimeError", value.to_string()))?;
        if !output.status.success() {
            let details = if output.stderr.is_empty() {
                output.stdout
            } else {
                output.stderr
            };
            return Err(error(
                "RuntimeError",
                format!(
                    "Niri configuration validation failed: {}",
                    String::from_utf8_lossy(&details).trim()
                ),
            ));
        }
        Ok(())
    }
    fn reload(&mut self, timeout: Duration) -> Result<Value> {
        self.backend.execute(
            "reload",
            &[],
            &serde_json::json!({"timeout":timeout.as_secs_f64()}),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::os::unix::fs::PermissionsExt;
    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }
    fn decode(value: &str) -> crate::Result<Vec<u8>> {
        (0..value.len())
            .step_by(2)
            .map(|index| u8::from_str_radix(&value[index..index + 2], 16).map_err(Into::into))
            .collect()
    }
    fn file_values(root: &Path) -> crate::Result<Value> {
        fn visit(
            root: &Path,
            folder: &Path,
            result: &mut serde_json::Map<String, Value>,
        ) -> crate::Result<()> {
            for entry in fs::read_dir(folder)? {
                let path = entry?.path();
                if path.is_dir() {
                    visit(root, &path, result)?;
                } else {
                    result.insert(path.strip_prefix(root)?.to_string_lossy().into(),json!({"hex":hex(&fs::read(&path)?),"mode":format!("0o{:o}",fs::metadata(&path)?.permissions().mode()&0o777)}));
                }
            }
            Ok(())
        }
        let mut files = serde_json::Map::new();
        visit(root, root, &mut files)?;
        Ok(Value::Object(files))
    }
    struct Fixture {
        monitors: Value,
        input: Value,
        target: PathBuf,
        calls: Vec<Value>,
    }
    impl Runtime for Fixture {
        fn monitors(&mut self, timeout: Duration) -> Result<Value> {
            self.calls.push(
                json!({"kind":"query","name":"monitors","kwargs":{"timeout":timeout.as_secs()}}),
            );
            Ok(self.monitors.clone())
        }
        fn validate(&mut self, path: &Path, timeout: Duration) -> Result<()> {
            self.calls.push(json!({"kind":"validate","bytes":hex(&fs::read(path).map_err(io_error)?),"kwargs":{"check":true,"capture_output":true,"timeout":timeout.as_secs()}}));
            if self.input["validation_failure"] == true {
                Err(error("RuntimeError", "fixture validation failed"))
            } else {
                Ok(())
            }
        }
        fn reload(&mut self, timeout: Duration) -> Result<Value> {
            self.calls.push(json!({"kind":"reload","bytes":hex(&fs::read(&self.target).map_err(io_error)?),"kwargs":{"timeout":timeout.as_secs()}}));
            if self.input["reload_failure"] == true {
                Err(error("RuntimeError", "fixture reload failed"))
            } else {
                Ok(json!({"code":0,"stdout":"Handled","stderr":""}))
            }
        }
    }
    #[test]
    fn accepts_only_its_explicit_absolute_desktop_include() -> crate::Result<()> {
        let root = tempfile::tempdir()?;
        let loader = root.path().join("session.kdl");
        let target = root.path().join("desktop.kdl");
        let config = Configuration {
            home: root.path().into(),
            config: root.path().into(),
            loader: loader.clone(),
            desktop: target.clone(),
            host: None,
        };
        fs::write(
            &loader,
            format!(
                "include {}\n",
                serde_json::to_string(&target.to_string_lossy())?
            ),
        )?;
        assert!(config.ensure_loader().is_ok());
        fs::write(&loader, "include \"unrelated.kdl\"\n")?;
        assert!(config.ensure_loader().is_err());
        Ok(())
    }
    #[test]
    fn live_scroll_update_patches_managed_session_files_and_rolls_back() -> crate::Result<()> {
        let root = tempfile::tempdir()?;
        let paths = crate::paths::Paths::new(root.path());
        let session = paths.share.join("niri-on-pc/session/config");
        fs::create_dir_all(&session)?;
        let loader = session.join("config.kdl");
        let desktop = session.join("desktop.kdl");
        let host = session.join("host.kdl");
        fs::write(&loader, "include \"host.kdl\"\ninclude \"desktop.kdl\"\n")?;
        let original_desktop = "input { touchpad { scroll-factor 0.35; } }\n";
        let original_host = "layout { gaps 14; }\ninput {\n    mouse { scroll-factor 1.5; natural-scroll; }\n    touchpad { scroll-factor 0.35; tap; }\n}\noutput \"DP-1\" { scale 1.5; }\nbinds { Super+Q { close-window; }; }\n";
        fs::write(&desktop, original_desktop)?;
        fs::write(&host, original_host)?;
        fs::create_dir_all(&paths.config)?;
        fs::write(paths.config.join("desktop.json"), "{}\n")?;
        let config = Configuration {
            home: paths.home.clone(),
            config: paths.config.clone(),
            loader,
            desktop: desktop.clone(),
            host: Some(host.clone()),
        };
        let mut runtime = Fixture {
            monitors: serde_json::json!([]),
            input: serde_json::json!({"reload_failure":false}),
            target: desktop.clone(),
            calls: vec![],
        };
        let invalid = config.set(
            &serde_json::json!({"scrollSpeed":0.2,"mouseScrollSpeed":2.25}),
            Duration::from_secs(3),
            &mut runtime,
        );
        assert!(invalid.is_err());
        assert_eq!(fs::read(&desktop)?, original_desktop.as_bytes());
        assert_eq!(fs::read(&host)?, original_host.as_bytes());
        assert!(runtime.calls.is_empty());
        runtime.input["validation_failure"] = serde_json::json!(true);
        assert!(config
            .set(
                &serde_json::json!({"scrollSpeed":0.55}),
                Duration::from_secs(3),
                &mut runtime
            )
            .is_err());
        assert_eq!(fs::read(&desktop)?, original_desktop.as_bytes());
        assert_eq!(fs::read(&host)?, original_host.as_bytes());
        runtime.calls.clear();
        runtime.input["validation_failure"] = serde_json::json!(false);
        config.set(
            &serde_json::json!({"scrollSpeed":0.55,"mouseScrollSpeed":2.25}),
            Duration::from_secs(3),
            &mut runtime,
        )?;
        let desktop_text = fs::read_to_string(&desktop)?;
        let host_text = fs::read_to_string(&host)?;
        assert!(desktop_text.contains("scroll-factor 0.55"));
        assert!(desktop_text.contains("scroll-factor 2.25"));
        assert!(host_text.contains("scroll-factor 0.55"));
        assert!(host_text.contains("scroll-factor 2.25"));
        assert!(host_text.contains("output \"DP-1\""));
        assert!(host_text.contains("scale 1.5"));
        assert!(host_text.contains("Super+Q"));
        assert_eq!(
            runtime
                .calls
                .iter()
                .filter(|call| call["kind"] == "validate")
                .count(),
            2
        );
        assert_eq!(
            runtime
                .calls
                .iter()
                .filter(|call| call["kind"] == "reload")
                .count(),
            1
        );

        fs::write(&desktop, original_desktop)?;
        fs::write(&host, original_host)?;
        runtime.input["reload_failure"] = serde_json::json!(true);
        assert!(config
            .set(
                &serde_json::json!({"scrollSpeed":0.7,"mouseScrollSpeed":2.5}),
                Duration::from_secs(3),
                &mut runtime
            )
            .is_err());
        assert_eq!(fs::read(&desktop)?, original_desktop.as_bytes());
        assert_eq!(fs::read(&host)?, original_host.as_bytes());
        Ok(())
    }
    #[test]
    fn host_scroll_patch_preserves_non_scroll_settings() {
        let source = "layout { gaps 14; }\ninput {\n    mouse { natural-scroll; scroll-factor 1.5; }\n    touchpad { tap; scroll-factor 0.35; }\n}\noutput \"DP-1\" { scale 1.5; }\nbinds { Super+Q { close-window; }; }\n";
        let rendered = update_host_scroll_factors(source, 2.25, 0.55).unwrap();
        assert!(rendered.contains("gaps 14"));
        assert!(rendered.contains("natural-scroll"));
        assert!(rendered.contains("tap"));
        assert!(rendered.contains("output \"DP-1\""));
        assert!(rendered.contains("binds"));
        assert!(rendered.contains("scroll-factor 2.25"));
        assert!(rendered.contains("scroll-factor 0.55"));
    }
    #[test]
    fn active_configuration_uses_only_included_private_session_targets() -> crate::Result<()> {
        let root = tempfile::tempdir()?;
        let paths = crate::paths::Paths::new(root.path());
        let session = paths.share.join("niri-on-pc/session/config");
        fs::create_dir_all(&session)?;
        let loader = session.join("config.kdl");
        let desktop = session.join("desktop.kdl");
        let host = session.join("host.kdl");
        fs::write(&loader, "include \"host.kdl\"\ninclude \"desktop.kdl\"\n")?;
        fs::write(&desktop, "")?;
        fs::write(&host, "input {\n    touchpad { scroll-factor 0.35; }\n}\n")?;
        let config = Configuration::active(&paths, &std::collections::BTreeMap::new());
        assert_eq!(config.loader, loader);
        assert_eq!(config.desktop, desktop);
        assert_eq!(config.host.as_deref(), Some(host.as_path()));
        config.ensure_loader().unwrap();
        let original_desktop = fs::read(&desktop)?;
        let original_host = fs::read(&host)?;
        let invalid_target = session.join("custom.kdl");
        let invalid = Configuration::active(
            &paths,
            &std::collections::BTreeMap::from([
                (
                    "NACRE_NIRI_CONFIG".into(),
                    loader.to_string_lossy().into_owned(),
                ),
                (
                    "NACRE_NIRI_DESKTOP_CONFIG".into(),
                    invalid_target.to_string_lossy().into_owned(),
                ),
            ]),
        );
        assert_eq!(invalid.desktop, invalid_target);
        let mut runtime = Fixture {
            monitors: serde_json::json!([]),
            input: serde_json::json!({}),
            target: desktop.clone(),
            calls: vec![],
        };
        assert!(invalid
            .set(
                &serde_json::json!({"scrollSpeed":0.55}),
                Duration::from_secs(3),
                &mut runtime,
            )
            .is_err());
        assert_eq!(fs::read(&desktop)?, original_desktop);
        assert_eq!(fs::read(&host)?, original_host);
        assert!(!invalid_target.exists());
        assert!(runtime.calls.is_empty());
        Ok(())
    }
    #[test]
    fn frozen_python_desktop_file_bytes_modes_order_and_rollback() -> crate::Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-desktop-transaction-reference.json"
        ))?;
        let states: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-state-reference.json"
        ))?;
        let monitors = states["cases"]
            .as_array()
            .ok_or("Missing states")?
            .iter()
            .find(|case| case["request"]["name"] == "initial-u64-output-relative")
            .ok_or("Missing state")?["observations"]
            .as_array()
            .ok_or("Missing observations")?
            .last()
            .ok_or("Missing snapshot")?["snapshot"]["monitors"]
            .clone();
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let home = tempfile::tempdir()?;
            let paths = crate::paths::Paths::new(home.path());
            fs::create_dir_all(paths.config.join("niri"))?;
            for (name, file) in case["observed"]["initial_files"]
                .as_object()
                .ok_or("Missing input files")?
            {
                let path = home.path().join(name);
                fs::create_dir_all(path.parent().ok_or("Missing parent")?)?;
                fs::write(&path, decode(file["hex"].as_str().ok_or("Missing bytes")?)?)?;
                let mode = u32::from_str_radix(
                    file["mode"]
                        .as_str()
                        .ok_or("Missing mode")?
                        .trim_start_matches("0o"),
                    8,
                )?;
                fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
            }
            let config = Configuration::new(&paths, None, None);
            let mut runtime = Fixture {
                monitors: monitors.clone(),
                input: case["request"].clone(),
                target: config.desktop.clone(),
                calls: vec![],
            };
            let result = config.set(
                &case["request"]["preferences"],
                Duration::from_secs(3),
                &mut runtime,
            );
            let (result, exit) = match result {
                Ok(value) => (value, 0),
                Err(error) => (json!({"error":error.message,"type":error.kind}), 1),
            };
            assert_eq!(
                crate::json::encode(&result)? + "\n",
                case["observed"]["stdout"],
                "{}",
                case["request"]["name"]
            );
            assert_eq!(
                json!(exit),
                case["observed"]["exit"],
                "{}",
                case["request"]["name"]
            );
            assert_eq!(
                json!(runtime.calls),
                case["observed"]["calls"],
                "{}",
                case["request"]["name"]
            );
            assert_eq!(
                file_values(home.path())?,
                case["observed"]["files"],
                "{}",
                case["request"]["name"]
            );
        }
        Ok(())
    }
}
