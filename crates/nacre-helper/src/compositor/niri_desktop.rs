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
pub trait Runtime {
    fn monitors(&mut self, timeout: Duration) -> Result<Value>;
    fn validate(&mut self, path: &Path, timeout: Duration) -> Result<()>;
    fn reload(&mut self, timeout: Duration) -> Result<Value>;
}
pub struct Configuration {
    pub config: PathBuf,
    pub loader: PathBuf,
    pub desktop: PathBuf,
}
impl Configuration {
    pub fn new(paths: &crate::paths::Paths, loader: Option<&Path>, desktop: Option<&Path>) -> Self {
        Self {
            config: paths.config.clone(),
            loader: loader
                .map(Path::to_owned)
                .unwrap_or_else(|| paths.config.join("niri/config.kdl")),
            desktop: desktop
                .map(Path::to_owned)
                .unwrap_or_else(|| paths.config.join("niri/desktop.kdl")),
        }
    }
    pub fn ensure_loader(&self) -> Result<()> {
        if !self.loader.exists() {
            return Err(error("RuntimeError","Managed niri desktop include missing; regenerate the isolated session configuration"));
        }
        let contents = fs::read_to_string(&self.loader).map_err(io_error)?;
        let absolute_include = format!(
            "include {}",
            serde_json::to_string(&self.desktop.to_string_lossy())
                .map_err(|value| error("ValueError", value.to_string()))?
        );
        if !contents.contains("include \"desktop.kdl\"") && !contents.contains(&absolute_include) {
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
        temporary
            .write_all(niri_config::desktop(&values)?.as_bytes())
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
        temporary
            .persist(&self.desktop)
            .map_err(|value| io_error(value.error))?;
        match runtime.reload(timeout) {
            Ok(value) => Ok(value),
            Err(failure) => {
                match previous {
                    Some(bytes) => fs::write(&self.desktop, bytes).map_err(io_error)?,
                    None => fs::remove_file(&self.desktop).map_err(io_error)?,
                }
                Err(failure)
            }
        }
    }
}
pub struct Native<'a> {
    pub backend: &'a mut super::niri_actions::Backend<super::niri_transport::Connection>,
}
impl Runtime for Native<'_> {
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
            config: root.path().into(),
            loader: loader.clone(),
            desktop: target.clone(),
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
