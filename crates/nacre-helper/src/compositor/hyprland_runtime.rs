//! Installed Hyprland backend assets and lifecycle, using the common native encoder.
use super::{hyprland, hyprland_events, hyprland_transport, niri_state::Error};
use crate::paths::Paths;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::net::UnixStream,
    path::{Path, PathBuf},
    time::Duration,
};
type Result<T> = std::result::Result<T, Error>;
fn failure(message: impl Into<String>) -> Error {
    Error {
        kind: "RuntimeError",
        message: message.into(),
    }
}
pub struct Backend {
    pub actions: hyprland::Backend<hyprland_transport::Socket>,
    pub paths: Paths,
    environment: BTreeMap<String, String>,
}
fn executable(name: &str) -> Option<PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|root| root.join(name))
        .find(|path| {
            fs::metadata(path).is_ok_and(|metadata| {
                metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
            })
        })
}
impl Backend {
    pub fn new(paths: Paths, environment: BTreeMap<String, String>) -> Result<Self> {
        let runtime = environment
            .get("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", nix::unistd::getuid())));
        let signature = environment
            .get("HYPRLAND_INSTANCE_SIGNATURE")
            .map(String::as_str)
            .unwrap_or("");
        let socket = if signature.is_empty() {
            hyprland_transport::Socket {
                folder: runtime.join("hypr"),
            }
        } else {
            hyprland_transport::Socket::new(&runtime, signature)
                .map_err(|error| failure(error.to_string()))?
        };
        Ok(Self {
            actions: hyprland::Backend { transport: socket },
            paths,
            environment,
        })
    }
    pub fn session_id(&self) -> &str {
        self.environment
            .get("HYPRLAND_INSTANCE_SIGNATURE")
            .map(String::as_str)
            .unwrap_or("")
    }
    pub fn events(&mut self) -> Result<hyprland_events::Events> {
        hyprland_events::Events::connect(
            &self.actions.transport.folder.join(".socket2.sock"),
            &mut self.actions,
        )
    }
    pub fn config_path(&self, kind: &str) -> Result<PathBuf> {
        if !["palette", "desktop", "monitor", "profile"].contains(&kind) {
            return Err(Error {
                kind: "ValueError",
                message: "Unknown compositor configuration".into(),
            });
        }
        Ok(self.paths.config.join(format!("{kind}.lua")))
    }
    pub fn binding_source_files(&self) -> Result<Vec<PathBuf>> {
        hyprland::binding_source_files(&self.paths.home, &self.paths.config)
    }
    pub fn capture_bindings(&self, path: &Path, kind: &str) -> Result<Value> {
        hyprland::capture_bindings(
            path,
            kind,
            executable("lua").as_deref(),
            &self
                .paths
                .assets()
                .join("compositor_backends/hyprland/bindings.lua"),
        )
    }
    pub fn night_light_available(&self) -> bool {
        executable("hyprsunset").is_some()
    }
    pub fn night_light_configured_elsewhere(&self) -> bool {
        self.paths
            .home
            .join(".config/hypr/hyprsunset.conf")
            .exists()
    }
    pub fn night_light_external_socket(&self, active: bool) -> Result<bool> {
        if active
            || self.session_id().is_empty()
            || self
                .environment
                .get("XDG_RUNTIME_DIR")
                .is_none_or(|value| value.is_empty())
        {
            return Ok(false);
        }
        let path = self.actions.transport.folder.join(".hyprsunset.sock");
        if !path.exists() {
            return Ok(false);
        }
        match UnixStream::connect(path) {
            Ok(socket) => {
                socket
                    .set_read_timeout(Some(Duration::from_millis(150)))
                    .map_err(|error| failure(error.to_string()))?;
                Ok(true)
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                ) =>
            {
                Ok(false)
            }
            Err(error) => {
                eprintln!("nacre-helper: external night-light socket probe failed: {error}");
                Ok(true)
            }
        }
    }
    pub fn logout(&mut self, graceful: bool, timeout: Duration) -> Result<Value> {
        if graceful {
            if let Some(program) = executable("hyprshutdown") {
                let output = crate::process::checked(
                    program
                        .to_str()
                        .ok_or_else(|| failure("Shutdown command path is not UTF-8"))?,
                    &[],
                    None,
                    timeout,
                )
                .map_err(|error| failure(error.to_string()))?;
                return Ok(
                    json!({"code":output.status.code().unwrap_or(1),"stdout":String::from_utf8(output.stdout).map_err(|error|failure(error.to_string()))?,"stderr":String::from_utf8(output.stderr).map_err(|error|failure(error.to_string()))?}),
                );
            }
        }
        self.actions
            .execute("logout", &[], &json!({"timeout":timeout.as_secs_f64()}))
    }
}
