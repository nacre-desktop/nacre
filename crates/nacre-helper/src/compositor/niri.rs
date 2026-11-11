//! Complete native Niri boundary, sharing one lazy request connection per caller.
use super::{
    niri_actions, niri_bindings, niri_config, niri_desktop, niri_events, niri_night_light,
    niri_state::Error, niri_transport,
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
fn is_edge_event(value: &Value) -> bool {
    matches!(
        value["type"].as_str(),
        Some("edge-approach" | "edge-hover" | "edge-triggered" | "edge-left")
    )
}
fn failure(kind: &'static str, message: impl Into<String>) -> Error {
    Error {
        kind,
        message: message.into(),
    }
}
pub fn capabilities() -> Value {
    json!({"stableWorkspaceIndices":false,"globalWorkspaces":false,"perOutputWorkspaces":true,"scrollingLayout":true,"floatingPlacement":true,"pinWindow":false,"windowCapture":false,"nativeWindowScreenshot":true,"shapedBackgroundBlur":false,"cursorPosition":false,"nativePanelDrag":false,"windowHide":false,"windowTrash":false,"scratchpad":false,"windowGroups":true,"windowForceKill":false,"pseudoTiling":false,"layoutSplit":false,"keymaps":false,"nonConsumingShortcuts":false,"bindingIntrospection":false,"fullscreenState":false,"shortcutRelease":false,"desktopOptions":["animations","blur","shadow","followMouse","naturalScroll","gapsIn","borderSize","rounding"],"monitorConfiguration":true,"monitorMirroring":false,"nightLightController":true})
}
pub fn capabilities_with_native(native: niri_transport::NacreCapabilities) -> Value {
    let mut result = capabilities();
    result["pinWindow"] = json!(native.window_pinning);
    result["windowHide"] = json!(native.scratchpad);
    result["windowForceKill"] = json!(false);
    if native.force_close {
        // This scoped feature does not imply generic per-window process kill.
        result["windowForceCloseWayland"] = json!(true);
    }
    if native.window_restore {
        result["windowRestore"] = json!(true);
    }
    if native.monitor_workspace_actions {
        result["monitorWorkspaceActions"] = json!(true);
    }
    if native.launch_workspace {
        result["launchWorkspace"] = json!(true);
    }
    result
}
pub fn capabilities_for_socket(socket: &Path) -> Value {
    capabilities_with_native(niri_transport::nacre_capabilities(socket))
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
        niri_desktop::Configuration::active(&self.paths, &self.environment)
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
        let root = niri_bindings::loader(&self.paths, &self.environment);
        Ok(niri_bindings::source_files(&root, &self.paths.home))
    }
    pub fn collection_action(&mut self, action: &str, arguments: Value) -> Result<()> {
        self.actions.transport.collection_action(action, arguments)
    }
    pub fn collections(&mut self) -> Result<Vec<Value>> {
        self.actions.transport.collections()
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
            let (_, records) = niri_bindings::inventory(
                &niri_bindings::loader(&self.paths, &self.environment),
                &self.paths.home,
            )?;
            return niri_config::binding_query(&records);
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
    pub fn events(&mut self) -> Result<niri_events::Events<niri_transport::Connection>> {
        if self.session_id().is_empty() {
            return Err(failure("RuntimeError", "NIRI_SOCKET is unavailable"));
        }
        if let Some(source) = crate::daemon::niri_events::Source::connect(
            &self.environment,
            Path::new(self.session_id()),
        )
        .map_err(|error| failure("ConnectionError", error.to_string()))?
        {
            return Ok(niri_events::Events::from_daemon(
                source,
                niri_transport::Connection::new(self.session_id()),
            ));
        }
        let hover = self
            .environment
            .get("NACRE_NIRI_HOVER_STREAM")
            .is_none_or(|value| value != "0");
        let native_capabilities = self
            .actions
            .transport
            .nacre_capabilities(Duration::from_millis(500));
        // Startup capability probes can precede ConfigLoaded. The stream's own
        // negotiation handles collection support and falls back for older peers.
        let socket =
            niri_events::EventSocket::connect_collections(Path::new(self.session_id()), hover)?;
        niri_events::Events::new_with_capabilities(
            socket,
            niri_transport::Connection::new(self.session_id()),
            native_capabilities,
        )
        .map(|events| events.with_edge_trigger_cues(native_capabilities.edge_trigger_cues))
    }
    pub fn edge_events(&mut self) -> Result<()> {
        let path = PathBuf::from(self.session_id());
        if self.session_id().is_empty() {
            return Err(failure("RuntimeError", "NIRI_SOCKET is unavailable"));
        }
        let environment = self.environment.clone();
        if let Some(mut daemon) = crate::daemon::niri_events::Source::connect(&environment, &path)
            .map_err(|error| failure("ConnectionError", error.to_string()))?
        {
            loop {
                let value = daemon
                    .next_value()
                    .map_err(|error| failure("ConnectionError", error.to_string()))?;
                if value["type"] == "snapshot" {
                    let next = value
                        .get("edgeTriggersActive")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    println!("{{\"type\":\"edge-source\",\"active\":{next}}}");
                    std::io::Write::flush(&mut std::io::stdout())
                        .map_err(|error| failure("OSError", error.to_string()))?;
                    if !next {
                        return Ok(());
                    }
                } else if is_edge_event(&value) {
                    println!(
                        "{}",
                        crate::json::encode(&value)
                            .map_err(|error| { failure("ValueError", error.to_string()) })?
                    );
                    std::io::Write::flush(&mut std::io::stdout())
                        .map_err(|error| failure("OSError", error.to_string()))?;
                }
            }
        }
        let capabilities = self
            .actions
            .transport
            .nacre_capabilities(Duration::from_millis(500));
        if !capabilities.edge_triggers || !capabilities.edge_trigger_cues {
            println!("{{\"type\":\"edge-source\",\"active\":false}}");
            std::io::Write::flush(&mut std::io::stdout())
                .map_err(|error| failure("OSError", error.to_string()))?;
            return Ok(());
        }
        let mut events = niri_events::EventSocket::connect_edge_triggers(&path)?;
        println!("{{\"type\":\"edge-source\",\"active\":true}}");
        std::io::Write::flush(&mut std::io::stdout())
            .map_err(|error| failure("OSError", error.to_string()))?;
        loop {
            let Some(event) = niri_events::edge_trigger(&events.next_event()?)? else {
                continue;
            };
            println!(
                "{}",
                crate::json::encode(&event)
                    .map_err(|error| failure("ValueError", error.to_string()))?
            );
            std::io::Write::flush(&mut std::io::stdout())
                .map_err(|error| failure("OSError", error.to_string()))?;
        }
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
    pub fn capabilities(&mut self) -> Value {
        capabilities_with_native(
            self.actions
                .transport
                .nacre_capabilities(Duration::from_millis(500)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_runtime_capabilities_follow_optional_ipc_features() -> crate::Result<()> {
        use std::{
            io::{BufRead, BufReader, Write},
            os::unix::net::UnixListener,
            thread,
        };
        let directory = tempfile::tempdir()?;
        let socket = directory.path().join("niri.sock");
        let listener = UnixListener::bind(&socket)?;
        let peer = thread::spawn(move || -> std::io::Result<()> {
            let (stream, _) = listener.accept()?;
            let mut stream = BufReader::new(stream);
            let mut request = String::new();
            stream.read_line(&mut request)?;
            assert_eq!(request, "\"NacreCapabilities\"\n");
            writeln!(stream.get_mut(), "{{\"Ok\":{{\"NacreCapabilities\":{{\"workspace_collections\":false,\"proportional_x11_widths\":false,\"overview_hover\":false,\"scratchpad\":true,\"window_pinning\":true,\"edge_triggers\":false,\"force_close\":true,\"monitor_workspace_actions\":false,\"window_restore\":true}}}}}}")
        });
        let capabilities = capabilities_for_socket(&socket);
        assert_eq!(capabilities["windowForceKill"], false);
        assert_eq!(capabilities["windowForceCloseWayland"], true);
        assert_eq!(capabilities["windowRestore"], true);
        assert_eq!(capabilities["pinWindow"], true);
        assert_eq!(capabilities["windowHide"], true);
        assert!(capabilities.get("monitorWorkspaceActions").is_none());
        peer.join()
            .map_err(|_| "Niri capability fixture panicked")??;
        Ok(())
    }
    #[test]
    fn collection_stream_is_negotiated_when_startup_probe_precedes_config_loaded(
    ) -> crate::Result<()> {
        use std::{
            io::{BufRead, BufReader, Write},
            os::unix::net::UnixListener,
            thread,
        };

        let directory = tempfile::tempdir()?;
        let socket = directory.path().join("niri.sock");
        let listener = UnixListener::bind(&socket)?;
        let peer = thread::spawn(move || -> std::io::Result<()> {
            // The startup capability probe runs before ConfigLoaded and reports
            // collections disabled.
            let (stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut probe = BufReader::new(stream);
            let mut request = String::new();
            probe.read_line(&mut request)?;
            assert_eq!(request, "\"NacreCapabilities\"\n");
            writeln!(
                probe.get_mut(),
                "{}",
                serde_json::json!({"Ok":{"NacreCapabilities":{
                    "workspace_collections":false,"proportional_x11_widths":false,"overview_hover":false,
                    "scratchpad":false,"window_pinning":false,"edge_triggers":false,"edge_trigger_cues":false,
                    "force_close":false,"monitor_workspace_actions":false,"window_restore":false
                }}})
            )?;

            // The richer stream is still requested. Simulate a peer that lacks
            // the combined hover variant but supports collection events.
            let (stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut combined = BufReader::new(stream);
            request.clear();
            combined.read_line(&mut request)?;
            assert_eq!(
                request.trim(),
                "\"NacreOverviewHoverStreamWithCollections\""
            );
            writeln!(combined.get_mut(), "{{\"Err\":\"unsupported\"}}")?;
            drop(combined);

            let (stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut collections = BufReader::new(stream);
            request.clear();
            collections.read_line(&mut request)?;
            assert_eq!(request.trim(), "\"EventStreamWithCollections\"");
            writeln!(collections.get_mut(), "{{\"Ok\":\"Handled\"}}")?;
            for event in [
                serde_json::json!({"CollectionsChanged":{"collections":[
                    {"id":17,"idx":1,"name":"work","output":"DP-1","is_active":true,"workspace_ids":[4]},
                    {"id":28,"idx":1,"name":"mail","output":"eDP-1","is_active":true,"workspace_ids":[8]}
                ]}}),
                serde_json::json!({"WorkspacesChanged":{"workspaces":[]}}),
                serde_json::json!({"WindowsChanged":{"windows":[]}}),
                serde_json::json!({"ConfigLoaded":{"failed":false}}),
                serde_json::json!({"ConfigLoaded":{"failed":false}}),
            ] {
                writeln!(collections.get_mut(), "{event}")?;
            }
            drop(collections);

            // Events refreshes outputs on workspace changes and reads edge-cue
            // capabilities, outputs and native capabilities on each reload.
            let (stream, _) = listener.accept()?;
            stream.set_read_timeout(Some(Duration::from_secs(2)))?;
            let mut queries = BufReader::new(stream);
            for reply in [
                ("Outputs", serde_json::json!({})),
                ("Outputs", serde_json::json!({})),
                (
                    "NacreCapabilities",
                    serde_json::json!({
                        "workspace_collections":false,"proportional_x11_widths":false,"overview_hover":false,
                        "scratchpad":false,"window_pinning":false,"edge_triggers":false,"edge_trigger_cues":false,
                        "force_close":false,"monitor_workspace_actions":false,"window_restore":false
                    }),
                ),
                ("Outputs", serde_json::json!({})),
                (
                    "NacreCapabilities",
                    serde_json::json!({
                        "workspace_collections":false,"proportional_x11_widths":false,"overview_hover":false,
                        "scratchpad":false,"window_pinning":false,"edge_triggers":false,"edge_trigger_cues":false,
                        "force_close":false,"monitor_workspace_actions":false,"window_restore":false
                    }),
                ),
                (
                    "NacreCapabilities",
                    serde_json::json!({
                        "workspace_collections":true,"proportional_x11_widths":false,"overview_hover":false,
                        "scratchpad":false,"window_pinning":false,"edge_triggers":false,"edge_trigger_cues":false,
                        "force_close":false,"monitor_workspace_actions":false,"window_restore":false
                    }),
                ),
                ("Outputs", serde_json::json!({})),
                (
                    "NacreCapabilities",
                    serde_json::json!({
                        "workspace_collections":true,"proportional_x11_widths":false,"overview_hover":false,
                        "scratchpad":false,"window_pinning":false,"edge_triggers":false,"edge_trigger_cues":false,
                        "force_close":false,"monitor_workspace_actions":false,"window_restore":false
                    }),
                ),
            ] {
                request.clear();
                queries.read_line(&mut request)?;
                let (name, value) = reply;
                assert_eq!(request.trim(), serde_json::to_string(name).unwrap());
                let mut response = serde_json::json!({"Ok":{}});
                response["Ok"][name] = value;
                writeln!(queries.get_mut(), "{response}")?;
            }
            Ok(())
        });

        let environment =
            BTreeMap::from([("NIRI_SOCKET".into(), socket.to_string_lossy().into_owned())]);
        let mut backend = Backend::new(Paths::new(directory.path()), environment);
        let mut events = backend.events()?;
        let initial_snapshot = events.next_value()?;
        assert!(initial_snapshot["capabilities"]
            .get("workspaceCollections")
            .is_none());
        assert!(initial_snapshot.get("collections").is_none());
        let _queued_change = events.next_value()?;
        let disabled_snapshot = events.next_value()?;
        assert!(disabled_snapshot["capabilities"]
            .get("workspaceCollections")
            .is_none());
        assert!(disabled_snapshot.get("collections").is_none());
        let _disabled_change = events.next_value()?;
        let restored_snapshot = events.next_value()?;
        assert_eq!(
            restored_snapshot["capabilities"]["workspaceCollections"],
            true
        );
        assert_eq!(
            restored_snapshot["collections"].as_array().unwrap().len(),
            2
        );
        assert_eq!(restored_snapshot["collections"][0]["output"], "DP-1");
        assert_eq!(restored_snapshot["collections"][1]["output"], "eDP-1");
        peer.join()
            .map_err(|_| "Niri late collection fixture panicked")??;
        Ok(())
    }
    #[test]
    fn stock_capabilities_keep_the_existing_schema() {
        assert_eq!(
            capabilities_with_native(niri_transport::NacreCapabilities::default()),
            capabilities()
        );
    }
    #[test]
    fn stock_capability_bits_match_implemented_paths() {
        let capabilities = capabilities();
        let mut advertised = capabilities
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(name, value)| (value == true).then_some(name.as_str()))
            .collect::<Vec<_>>();
        advertised.sort_unstable();
        let mut implemented = vec![
            "perOutputWorkspaces", "scrollingLayout", "floatingPlacement",
            "nativeWindowScreenshot", "windowGroups", "monitorConfiguration",
            "nightLightController",
        ];
        implemented.sort_unstable();
        assert_eq!(advertised, implemented);
    }
    #[test]
    fn nacre_niri_probe_adds_only_native_supported_capabilities() {
        let capabilities = capabilities_with_native(niri_transport::NacreCapabilities {
            scratchpad: true,
            window_pinning: true,
            force_close: true,
            monitor_workspace_actions: true,
            window_restore: true,
            launch_workspace: true,
            ..Default::default()
        });
        let mut advertised = capabilities
            .as_object()
            .unwrap()
            .iter()
            .filter_map(|(name, value)| (value == true).then_some(name.as_str()))
            .collect::<Vec<_>>();
        advertised.sort_unstable();
        let mut implemented = vec![
            "perOutputWorkspaces", "scrollingLayout", "floatingPlacement",
            "nativeWindowScreenshot", "windowGroups", "monitorConfiguration",
            "nightLightController", "pinWindow", "windowHide",
            "windowForceCloseWayland", "windowRestore", "monitorWorkspaceActions",
            "launchWorkspace",
        ];
        implemented.sort_unstable();
        assert_eq!(advertised, implemented);
    }
    #[test]
    fn daemon_edge_lane_forwards_trigger_and_matching_exit_events() {
        assert!(is_edge_event(&json!({"type":"edge-triggered"})));
        assert!(is_edge_event(&json!({"type":"edge-left"})));
        assert!(is_edge_event(&json!({"type":"edge-approach"})));
        assert!(is_edge_event(&json!({"type":"edge-hover"})));
        assert!(!is_edge_event(&json!({"type":"windows-changed"})));
    }
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
        fs::write(
            paths.config.join("niri/config.kdl"),
            "binds { Super+Q { close-window; }; }\n",
        )?;
        let mut backend = Backend::new(paths, BTreeMap::new());
        let value = backend.query("bindings", Duration::from_secs(3), None)?;
        assert_eq!(value[0]["key"], "Q");
        assert!(!backend.actions.transport.connected());
        assert!(
            matches!(backend.events(),Err(error) if error.message=="NIRI_SOCKET is unavailable")
        );
        Ok(())
    }
}
