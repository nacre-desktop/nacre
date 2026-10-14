//! Neutral selection and typed dispatch shared by command clients and native users.
use super::{hyprland, hyprland_runtime, niri, niri_state::Error};
use crate::paths::Paths;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, time::Duration};
type Result<T> = std::result::Result<T, Error>;
pub fn select(environment: &BTreeMap<String, String>) -> Result<&'static str> {
    let explicit = environment
        .get("NACRE_COMPOSITOR")
        .map(|value| value.to_lowercase())
        .unwrap_or_default();
    if !explicit.is_empty() {
        return match explicit.as_str() {
            "hyprland" => Ok("hyprland"),
            "niri" => Ok("niri"),
            _ => Err(Error {
                kind: "ValueError",
                message: "Unknown NACRE_COMPOSITOR backend".into(),
            }),
        };
    }
    if environment
        .get("XDG_CURRENT_DESKTOP")
        .is_some_and(|value| value.to_lowercase().split(':').any(|value| value == "niri"))
    {
        return Ok("niri");
    }
    if environment
        .get("HYPRLAND_INSTANCE_SIGNATURE")
        .is_some_and(|value| !value.is_empty())
    {
        return Ok("hyprland");
    }
    Ok(
        if environment
            .get("NIRI_SOCKET")
            .is_some_and(|value| !value.is_empty())
        {
            "niri"
        } else {
            "hyprland"
        },
    )
}
pub enum Backend {
    Hyprland(hyprland_runtime::Backend),
    Niri(niri::Backend),
}
impl Backend {
    pub fn module_call(
        &mut self,
        operation: &str,
        args: &[Value],
        kwargs: &Value,
    ) -> Result<Value> {
        let argument = |index: usize, key: &str| {
            args.get(index)
                .or_else(|| kwargs.get(key))
                .cloned()
                .unwrap_or(Value::Null)
        };
        if operation == "session_context" {
            let environment: BTreeMap<String, String> =
                serde_json::from_value(argument(0, "environment")).map_err(|error| Error {
                    kind: "ValueError",
                    message: error.to_string(),
                })?;
            return Ok(json!(crate::niri_session_gate::compositor(&environment)));
        }
        let timeout = Duration::try_from_secs_f64(
            kwargs
                .get("timeout")
                .and_then(Value::as_f64)
                .unwrap_or(10.0),
        )
        .map_err(|error| Error {
            kind: "ValueError",
            message: error.to_string(),
        })?;
        if [
            "config_path",
            "binding_source_files",
            "ensure_desktop_loader",
            "night_light_configured_elsewhere",
        ]
        .contains(&operation)
        {
            if let Some(home) = args.first().and_then(Value::as_str) {
                let paths = match self {
                    Self::Hyprland(backend) if backend.paths.home == Path::new(home) => {
                        backend.paths.clone()
                    }
                    Self::Niri(backend) if backend.paths.home == Path::new(home) => {
                        backend.paths.clone()
                    }
                    _ => crate::paths::Paths::new(home),
                };
                match operation {
                    "config_path" => {
                        let kind = args.get(1).and_then(Value::as_str).ok_or_else(|| Error {
                            kind: "ValueError",
                            message: "Unknown compositor configuration".into(),
                        })?;
                        return match self {
                            Self::Hyprland(_) => {
                                if !["palette", "desktop", "monitor", "profile"].contains(&kind) {
                                    return Err(Error {
                                        kind: "ValueError",
                                        message: "Unknown compositor configuration".into(),
                                    });
                                }
                                Ok(json!(paths
                                    .config
                                    .join(format!("{kind}.lua"))
                                    .to_string_lossy()))
                            }
                            Self::Niri(backend) => {
                                let override_path = backend.config_path(kind)?;
                                let default = backend
                                    .paths
                                    .config
                                    .join("niri")
                                    .join(format!("{kind}.kdl"));
                                Ok(json!(if override_path != default {
                                    override_path
                                } else {
                                    paths.config.join("niri").join(format!("{kind}.kdl"))
                                }
                                .to_string_lossy()))
                            }
                        };
                    }
                    "binding_source_files" if self.name() == "hyprland" => {
                        return hyprland::binding_source_files(&paths.home, &paths.config).map(
                            |paths| {
                                json!(paths
                                    .iter()
                                    .map(|path| path.to_string_lossy().into_owned())
                                    .collect::<Vec<_>>())
                            },
                        )
                    }
                    "ensure_desktop_loader" if self.name() == "hyprland" => {
                        hyprland::ensure_desktop_loader(&paths.home)?;
                        return Ok(Value::Null);
                    }
                    "night_light_configured_elsewhere" => {
                        return Ok(json!(if self.name() == "hyprland" {
                            paths.home.join(".config/hypr/hyprsunset.conf").exists()
                        } else {
                            super::niri_night_light::configured_elsewhere(
                                paths.config.parent().unwrap_or(&paths.config),
                            )
                        }))
                    }
                    _ => {}
                }
            }
        }
        match operation {
            "native_niri_action" => match self {
                Self::Niri(backend) => {
                    let name = args.first().and_then(Value::as_str).ok_or_else(|| Error {
                        kind: "ValueError",
                        message: "Missing niri action".into(),
                    })?;
                    if name != "SetWindowWidth" {
                        return Err(Error {
                            kind: "ValueError",
                            message: "Unsupported native niri action".into(),
                        });
                    }
                    let values = args.get(1).ok_or_else(|| Error {
                        kind: "ValueError",
                        message: "Missing niri action values".into(),
                    })?;
                    let request: niri_ipc::Request = serde_json::from_value(
                        json!({"Action":{name:values}}),
                    )
                    .map_err(|error| Error {
                        kind: "ValueError",
                        message: error.to_string(),
                    })?;
                    let response = backend.actions.transport.request_value(
                        &serde_json::to_value(request).map_err(|error| Error {
                            kind: "ValueError",
                            message: error.to_string(),
                        })?,
                        timeout,
                        4 * 1024 * 1024,
                    )?;
                    if response != "Handled" {
                        return Err(Error {
                            kind: "ValueError",
                            message: "Unexpected niri action reply".into(),
                        });
                    }
                    Ok(Value::Null)
                }
                _ => Err(Error {
                    kind: "RuntimeError",
                    message: "Native niri action unavailable on this compositor".into(),
                }),
            },
            "environment_updates" => match self {
                Self::Hyprland(backend) => backend
                    .actions
                    .environment_updates(args.first().unwrap_or(&json!({}))),
                Self::Niri(backend) => {
                    if let Some(environment) = args.first().filter(|value| !value.is_null()) {
                        let environment = environment
                            .as_object()
                            .ok_or_else(|| Error {
                                kind: "ValueError",
                                message: "Invalid compositor environment".into(),
                            })?
                            .iter()
                            .map(|(key, value)| {
                                value
                                    .as_str()
                                    .map(|value| (key.clone(), value.to_owned()))
                                    .ok_or_else(|| Error {
                                        kind: "ValueError",
                                        message: "Invalid compositor environment".into(),
                                    })
                            })
                            .collect::<Result<BTreeMap<_, _>>>()?;
                        Ok(niri::Backend::environment_updates_from(&environment))
                    } else {
                        Ok(backend.environment_updates())
                    }
                }
            },
            "desktop_option_keys" => Ok(json!(if self.name() == "hyprland" {
                vec![
                    "animations",
                    "blur",
                    "shadow",
                    "followMouse",
                    "naturalScroll",
                    "gapsIn",
                    "gapsOut",
                    "borderSize",
                    "rounding",
                ]
            } else {
                vec![
                    "animations",
                    "blur",
                    "shadow",
                    "followMouse",
                    "naturalScroll",
                    "gapsIn",
                    "borderSize",
                    "rounding",
                ]
            })),
            "config_errors" => match self {
                Self::Hyprland(backend) => backend.actions.execute(operation, args, kwargs),
                Self::Niri(backend) => backend.config_errors(timeout).map(Value::from),
            },
            "night_light_command" => match self {
                Self::Hyprland(_) => Ok(json!(hyprland::night_light_command(
                    argument(0, "enabled") == true
                ))),
                Self::Niri(backend) => {
                    Ok(backend.night_light_command(argument(0, "enabled") == true))
                }
            },
            "night_light_configured_elsewhere" => match self {
                Self::Hyprland(backend) => Ok(json!(backend.night_light_configured_elsewhere())),
                Self::Niri(backend) => Ok(json!(super::niri_night_light::configured_elsewhere(
                    backend
                        .paths
                        .config
                        .parent()
                        .unwrap_or(&backend.paths.config)
                ))),
            },
            "logout" => match self {
                Self::Hyprland(backend) => backend.logout(kwargs["graceful"] == true, timeout),
                Self::Niri(backend) => backend.actions.execute(operation, args, kwargs),
            },
            "capabilities" => Ok(self.capabilities()),
            "backend_name" => Ok(json!(self.name())),
            "session_id" => Ok(json!(self.session_id())),
            "valid_window_id" => Ok(json!(self.valid_window_id(&argument(0, "value")))),
            "window_job_token" => match self {
                Self::Hyprland(_) => {
                    hyprland::window_job_token(&argument(0, "value")).map(Value::from)
                }
                Self::Niri(_) => {
                    super::niri_config::window_job_token(&argument(0, "value")).map(Value::from)
                }
            },
            "query" => self.query(
                argument(0, "name").as_str().ok_or_else(|| Error {
                    kind: "ValueError",
                    message: "Unknown compositor query".into(),
                })?,
                timeout,
                kwargs
                    .get("max_bytes")
                    .and_then(Value::as_u64)
                    .map(|value| value as usize),
            ),
            "query_result" => match self {
                Self::Niri(backend) => backend.query_result(
                    argument(0, "name").as_str().ok_or_else(|| Error {
                        kind: "ValueError",
                        message: "Unknown compositor query".into(),
                    })?,
                    timeout,
                    kwargs.get("check").is_none_or(|value| value != false),
                ),
                Self::Hyprland(backend) => {
                    let name = argument(0, "name");
                    let name = name.as_str().ok_or_else(|| Error {
                        kind: "ValueError",
                        message: "Unknown compositor query".into(),
                    })?;
                    backend.actions.query_result(
                        name,
                        timeout.as_secs_f64(),
                        kwargs.get("check").is_none_or(|value| value != false),
                    )
                }
            },
            "render_keyboard" => self
                .render_keyboard(&argument(0, "values"))
                .map(Value::from),
            "render_desktop_options" => self
                .render_desktop(&argument(0, "preferences"))
                .map(Value::from),
            "render_monitors" => self
                .render_monitors(&argument(0, "monitors"))
                .map(Value::from),
            "set_desktop_options" => self.set_desktop(&argument(0, "preferences"), timeout),
            "config_path" => self
                .config_path(argument(1, "kind").as_str().ok_or_else(|| Error {
                    kind: "ValueError",
                    message: "Unknown compositor configuration".into(),
                })?)
                .map(|path| json!(path.to_string_lossy())),
            "binding_source_files" => match self {
                Self::Hyprland(backend) => backend.binding_source_files(),
                Self::Niri(backend) => backend.binding_source_files(),
            }
            .map(|paths| {
                json!(paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>())
            }),
            "capture_bindings" => self.capture_bindings(
                Path::new(argument(0, "path").as_str().ok_or_else(|| Error {
                    kind: "ValueError",
                    message: "Invalid binding path".into(),
                })?),
                argument(1, "kind").as_str().unwrap_or("bindings"),
            ),
            "version_command" | "required_commands" | "diagnostic_commands" => match self {
                Self::Hyprland(backend) => backend.actions.execute(operation, args, kwargs),
                Self::Niri(backend) => Ok(match operation {
                    "version_command" => backend.version_command(),
                    "required_commands" => backend.required_commands(),
                    _ => backend.diagnostic_commands(),
                }),
            },
            "legacy_power_binding" => Ok(json!(if self.name() == "hyprland" {
                hyprland::LEGACY_POWER_BINDING
            } else {
                ""
            })),
            "palette_template_path" => Ok(json!(match self {
                Self::Hyprland(backend) => backend
                    .paths
                    .assets()
                    .join("compositor_backends/hyprland/palette.lua.in"),
                Self::Niri(backend) => backend
                    .paths
                    .assets()
                    .join("compositor_backends/niri/palette.kdl.in"),
            }
            .to_string_lossy())),
            "ensure_desktop_loader" => {
                match self {
                    Self::Hyprland(backend) => {
                        hyprland::ensure_desktop_loader(&backend.paths.home)?
                    }
                    Self::Niri(backend) => {
                        backend.configuration().ensure_loader()?;
                    }
                }
                Ok(Value::Null)
            }
            "night_light_available" => match self {
                Self::Hyprland(backend) => Ok(json!(backend.night_light_available())),
                Self::Niri(_) => {
                    use super::niri_night_light::Runtime;
                    super::niri_night_light::Native
                        .available()
                        .map(|value| json!(value))
                }
            },
            "night_light_external_socket" => match self {
                Self::Hyprland(backend) => backend
                    .night_light_external_socket(argument(0, "active") == true)
                    .map(|value| json!(value)),
                Self::Niri(backend) => backend
                    .night_light_external_socket(argument(0, "active") == true)
                    .map(|value| json!(value))
                    .map_err(|error| Error {
                        kind: "RuntimeError",
                        message: error.to_string(),
                    }),
            },
            "set_night_light_temperature" => match self {
                Self::Hyprland(backend) => backend.actions.execute(operation, args, kwargs),
                Self::Niri(_) => super::niri_night_light::temperature(
                    argument(0, "enabled") == true,
                    &mut super::niri_night_light::Native,
                ),
            },
            _ => self.action(operation, args, kwargs),
        }
    }
    pub fn new(paths: Paths, environment: BTreeMap<String, String>) -> Result<Self> {
        Ok(match select(&environment)? {
            "niri" => Self::Niri(niri::Backend::new(paths, environment)),
            _ => Self::Hyprland(hyprland_runtime::Backend::new(paths, environment)?),
        })
    }
    pub fn name(&self) -> &'static str {
        match self {
            Self::Hyprland(_) => "hyprland",
            Self::Niri(_) => "niri",
        }
    }
    pub fn capabilities(&self) -> Value {
        match self {
            Self::Hyprland(_) => hyprland::capabilities(),
            Self::Niri(_) => niri::capabilities(),
        }
    }
    pub fn valid_window_id(&self, value: &Value) -> bool {
        match self {
            Self::Hyprland(_) => hyprland::valid_window_id(value),
            Self::Niri(_) => super::niri_config::valid_window_id(value),
        }
    }
    pub fn session_id(&self) -> &str {
        match self {
            Self::Hyprland(backend) => backend.session_id(),
            Self::Niri(backend) => backend.session_id(),
        }
    }
    pub fn query(&mut self, name: &str, timeout: Duration, limit: Option<usize>) -> Result<Value> {
        match self {
            Self::Hyprland(backend) => backend.actions.query(name, timeout.as_secs_f64(), limit),
            Self::Niri(backend) => backend.query(name, timeout, limit),
        }
    }
    pub fn action(&mut self, name: &str, args: &[Value], kwargs: &Value) -> Result<Value> {
        match self {
            Self::Hyprland(backend) => backend.actions.execute(name, args, kwargs),
            Self::Niri(backend) => backend.actions.execute(name, args, kwargs),
        }
    }
    pub fn config_path(&self, kind: &str) -> Result<std::path::PathBuf> {
        match self {
            Self::Hyprland(backend) => backend.config_path(kind),
            Self::Niri(backend) => backend.config_path(kind),
        }
    }
    pub fn render_keyboard(&self, values: &Value) -> Result<String> {
        match self {
            Self::Hyprland(_) => hyprland::render_keyboard(values),
            Self::Niri(_) => super::niri_config::keyboard(values),
        }
    }
    pub fn render_desktop(&self, values: &Value) -> Result<String> {
        let paths = match self {
            Self::Hyprland(b) => &b.paths,
            Self::Niri(b) => &b.paths,
        };
        let values = crate::software_input::overlay(&paths.config, values).map_err(|e| Error {
            kind: "RuntimeError",
            message: e.to_string(),
        })?;
        let values = &values;
        match self {
            Self::Hyprland(_) => hyprland::render_desktop(values),
            Self::Niri(_) => super::niri_config::desktop(values),
        }
    }
    pub fn render_monitors(&self, values: &Value) -> Result<String> {
        match self {
            Self::Hyprland(_) => hyprland::render_monitors(values),
            Self::Niri(_) => super::niri_config::monitors(values),
        }
    }
    pub fn capture_bindings(&self, path: &Path, kind: &str) -> Result<Value> {
        match self {
            Self::Hyprland(backend) => backend.capture_bindings(path, kind),
            Self::Niri(backend) => backend.capture_bindings(path, kind),
        }
    }
    pub fn set_desktop(&mut self, values: &Value, timeout: Duration) -> Result<Value> {
        let paths = match self {
            Self::Hyprland(b) => &b.paths,
            Self::Niri(b) => &b.paths,
        };
        let values = crate::software_input::overlay(&paths.config, values).map_err(|e| Error {
            kind: "RuntimeError",
            message: e.to_string(),
        })?;
        let values = &values;
        match self {
            Self::Hyprland(backend) => backend.actions.execute(
                "set_desktop_options",
                std::slice::from_ref(values),
                &json!({"timeout":timeout.as_secs_f64()}),
            ),
            Self::Niri(backend) => backend.set_desktop_options(values, timeout),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_home_file_operations_preserve_caller_scope() -> crate::Result<()> {
        let home = tempfile::tempdir()?;
        for name in ["hyprland", "niri"] {
            let mut backend = Backend::new(
                Paths::new("/case/original"),
                BTreeMap::from([("NACRE_COMPOSITOR".into(), name.into())]),
            )?;
            let target = backend.module_call(
                "config_path",
                &[json!(home.path().to_string_lossy()), json!("desktop")],
                &json!({}),
            )?;
            assert_eq!(
                target,
                json!(home
                    .path()
                    .join(if name == "hyprland" {
                        ".config/nacre/desktop.lua"
                    } else {
                        ".config/nacre/niri/desktop.kdl"
                    })
                    .to_string_lossy())
            );
            let configured = home.path().join(if name == "hyprland" {
                ".config/hypr/hyprsunset.conf"
            } else {
                ".config/wlsunset/config"
            });
            std::fs::create_dir_all(configured.parent().ok_or("Missing parent")?)?;
            std::fs::write(configured, "fixture")?;
            assert_eq!(
                backend.module_call(
                    "night_light_configured_elsewhere",
                    &[json!(home.path().to_string_lossy())],
                    &json!({})
                )?,
                true
            );
        }
        Ok(())
    }
    #[test]
    fn selection_and_construction_are_lazy_and_preserve_precedence() -> crate::Result<()> {
        for (pairs, name) in [
            (vec![], "hyprland"),
            (vec![("NIRI_SOCKET", "/fixture")], "niri"),
            (
                vec![
                    ("NIRI_SOCKET", "/fixture"),
                    ("HYPRLAND_INSTANCE_SIGNATURE", "fixture"),
                ],
                "hyprland",
            ),
            (
                vec![
                    ("XDG_CURRENT_DESKTOP", "other:NIRI"),
                    ("HYPRLAND_INSTANCE_SIGNATURE", "fixture"),
                ],
                "niri",
            ),
        ] {
            let environment = pairs
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect();
            let backend = Backend::new(Paths::new("/case/home"), environment)?;
            assert_eq!(backend.name(), name);
        }
        assert!(select(&BTreeMap::from([("NACRE_COMPOSITOR".into(), "bad".into())])).is_err());
        Ok(())
    }
}
