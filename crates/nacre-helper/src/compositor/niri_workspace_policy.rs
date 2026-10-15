//! Named primary / dynamic secondary policy. Only the event-stream owner reconciles
//! the managed include; compositor hotplug itself is never debounced.
use super::{
    niri_actions::Transport,
    niri_transport::Connection,
    niri_workspaces::{action, Native},
};
use crate::{files, paths::Paths, state, Result};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::PathBuf,
};

fn sets(desktop: &Value) -> BTreeMap<String, Vec<String>> {
    serde_json::from_value(desktop["niriWorkspaceSets"].clone()).unwrap_or_default()
}
pub fn names(profile: &Value) -> Vec<String> {
    (0..profile["workspaceCount"].as_u64().unwrap_or(6))
        .map(|i| {
            profile["workspaceNames"][i as usize]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| format!("nacre-{}", i + 1))
        })
        .collect()
}
fn primary(desktop: &Value, outputs: &BTreeSet<String>) -> Option<String> {
    desktop["workspacePrimaryOutput"]
        .as_str()
        .filter(|s| outputs.contains(*s))
        .map(str::to_owned)
        .or_else(|| {
            outputs
                .iter()
                .find(|s| s.starts_with("eDP") || s.starts_with("LVDS"))
                .cloned()
        })
        .or_else(|| outputs.first().cloned())
}
pub(crate) fn primary_names(default: &[String], desktop: &Value) -> Vec<String> {
    let known = sets(desktop);
    let preferred = desktop["workspacePrimaryOutput"]
        .as_str()
        .and_then(|s| known.get(s))
        .or_else(|| {
            known
                .iter()
                .find(|(s, _)| s.starts_with("eDP") || s.starts_with("LVDS"))
                .map(|(_, n)| n)
        })
        .or_else(|| known.values().next());
    preferred
        .filter(|n| n.len() == default.len())
        .cloned()
        .unwrap_or_else(|| default.to_vec())
}
/// With no live topology, anchor the primary to its configured connector, or the
/// laptop connector if known. Runtime resolves a fallback before any user action.
pub fn declarations(
    default: &[String],
    desktop: &Value,
    topology: Option<&BTreeSet<String>>,
) -> Result<String> {
    if desktop
        .get("niriWorkspaceMode")
        .is_some_and(|m| m != "primary-dynamic" && m != "fixed-all")
    {
        return Err("Unknown workspace mode; preferences preserved".into());
    }
    if let Some(value) = desktop.get("niriWorkspaceSets") {
        let checked: BTreeMap<String, Vec<String>> = serde_json::from_value(value.clone())?;
        if checked.values().any(|names| names.len() != default.len()) {
            return Err("Each output must have the configured workspace count".into());
        }
        render(&checked, default.len())?;
    }
    let known = sets(desktop);
    let mut assignments = if desktop["niriWorkspaceMode"] == "fixed-all" {
        known
    } else {
        let available = topology
            .cloned()
            .unwrap_or_else(|| known.keys().cloned().collect());
        let output = primary(desktop, &available).or_else(|| {
            desktop["workspacePrimaryOutput"]
                .as_str()
                .filter(|s| !s.is_empty())
                .map(str::to_owned)
        });
        let values = primary_names(default, desktop);
        if let Some(output) = output {
            BTreeMap::from([(output, values)])
        } else {
            BTreeMap::from([(String::new(), values)])
        }
    };
    if assignments.is_empty() {
        assignments.insert(String::new(), default.to_vec());
    }
    render(&assignments, default.len())
}
fn render(assignments: &BTreeMap<String, Vec<String>>, count: usize) -> Result<String> {
    let mut seen = BTreeSet::new();
    let mut text = String::new();
    for (output, names) in assignments {
        if output.len() > 256 || names.len() > count {
            return Err("Invalid workspace set".into());
        }
        for name in names {
            if name.is_empty() || name.len() > 200 || !seen.insert(name) {
                return Err(
                    "Persistent workspace names must be nonempty and unique across outputs".into(),
                );
            }
            text.push_str(&format!("workspace {}", crate::json::encode(name)?));
            if !output.is_empty() {
                text.push_str(&format!(
                    " {{\n    open-on-output {}\n}}",
                    crate::json::encode(output)?
                ));
            }
            text.push('\n');
        }
    }
    Ok(text)
}
fn native() -> Result<Native> {
    Ok(Native {
        connection: Connection::new(
            env::var_os("NIRI_SOCKET").ok_or("NIRI_SOCKET is unavailable")?,
        ),
    })
}
fn snapshot(io: &mut Native) -> Result<Value> {
    Ok(io.snapshot(2., 4 * 1024 * 1024)?)
}
fn validate_includes(
    loader: &std::path::Path,
    text: &str,
    next: &str,
    shortcuts: &str,
) -> Result<()> {
    use std::io::Write;
    let parent = loader.parent().ok_or("Missing managed config directory")?;
    let mut candidate = text.to_owned();
    let mut temporary = Vec::new();
    for (name, contents) in [
        ("workspaces.kdl", next),
        ("monitor-workspace-actions.kdl", shortcuts),
    ] {
        let mut file = tempfile::NamedTempFile::new_in(parent)?;
        file.write_all(contents.as_bytes())?;
        let replacement = format!(
            "include {}",
            crate::json::encode(&file.path().to_string_lossy())?
        );
        for include in [
            name.to_owned(),
            parent.join(name).to_string_lossy().into_owned(),
        ] {
            candidate = candidate.replace(
                &format!("include {}", crate::json::encode(&include)?),
                &replacement,
            );
        }
        temporary.push(file);
    }
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(candidate.as_bytes())?;
    let mut command = std::process::Command::new("niri");
    command.args(["validate", "--config"]).arg(file.path());
    let result = crate::process::capture(command, None, std::time::Duration::from_secs(5))?;
    if !result.status.success() {
        return Err(
            "Workspace configuration validation failed; original includes preserved".into(),
        );
    }
    Ok(())
}
fn key(row: &Value) -> &str {
    row["id"].as_str().unwrap_or("")
}
fn number(value: &str, prefix: &str) -> Result<u64> {
    Ok(value
        .strip_prefix(prefix)
        .ok_or("Invalid compositor identity")?
        .parse()?)
}
fn occupied(snapshot: &Value, id: &str) -> bool {
    snapshot["windows"].as_array().is_some_and(|rows| {
        rows.iter().any(|w| {
            w["workspaceId"] == id
                || w["workspace_id"]
                    .as_u64()
                    .is_some_and(|n| id == format!("niri-workspace:{n}"))
        })
    })
}

#[derive(Default)]
pub struct Session {
    origins: BTreeMap<String, (String, usize)>,
    views: BTreeMap<String, (String, Value)>,
    view_names: BTreeMap<String, String>,
    connected: BTreeSet<String>,
    initialized: bool,
    signature: Value,
    adoptions: BTreeMap<String, String>,
}
impl Session {
    /// Track before reconciliation, while disconnected outputs retain their last
    /// view. Stable opaque IDs survive niri's per-output index renumbering.
    fn observe(&mut self, value: &mut Value, desktop: &Value) -> BTreeSet<String> {
        // WorkspacesChanged is an atomic topology snapshot: every output has
        // at least its native spare. The independently queried Outputs metadata
        // can be ahead of queued focus events during a quick unplug/replug.
        let outputs: BTreeSet<String> = value["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|r| {
                r["outputId"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
            })
            .collect();
        let mut known = sets(desktop);
        if desktop["niriWorkspaceMode"] != "fixed-all" {
            if let Some(primary) = primary(desktop, &outputs) {
                let profile = Paths::environment()
                    .ok()
                    .and_then(|paths| fs::read(paths.config.join("profile.json")).ok())
                    .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                    .unwrap_or(json!({}));
                known = BTreeMap::from([(primary, primary_names(&names(&profile), desktop))]);
            }
        }
        for row in value["workspaces"].as_array_mut().unwrap() {
            let id = key(row).to_owned();
            let name = row["name"].as_str().unwrap_or("");
            let named_origin = known.iter().find_map(|(output, names)| {
                names
                    .iter()
                    .position(|n| n == name)
                    .map(|slot| (output.clone(), slot + 1))
            });
            let fixed = named_origin.is_some();
            let output = row["outputId"].as_str().unwrap_or("").to_owned();
            let origin = self
                .adoptions
                .get(&id)
                .map(|home| (home.clone(), row["index"].as_u64().unwrap_or(1) as usize))
                .or(named_origin)
                .unwrap_or_else(|| {
                    self.origins
                        .get(&id)
                        .cloned()
                        .unwrap_or((output.clone(), row["index"].as_u64().unwrap_or(1) as usize))
                });
            self.origins.insert(id.clone(), origin.clone());
            row["fixedSlot"] = json!(fixed);
            row["originOutput"] = json!(origin.0);
            row["originIndex"] = json!(origin.1);
            row["visiting"] = json!(origin.0 != output);
            if row["active"] == true
                && origin.0 == output
                && (!self.initialized || self.connected.contains(&output))
            {
                self.view_names.insert(
                    output.clone(),
                    row["name"].as_str().unwrap_or("").to_owned(),
                );
                self.views
                    .insert(output, (id, row["activeWindowId"].clone()));
            }
        }
        let live: BTreeSet<String> = value["workspaces"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| key(r).to_owned())
            .collect();
        self.origins.retain(|id, _| live.contains(id));
        self.adoptions.retain(|id, _| live.contains(id));
        outputs
    }
    pub fn accept(&mut self, value: &mut Value) -> Result<()> {
        if value["type"] != "snapshot" {
            return Ok(());
        }
        let paths = Paths::environment()?;
        // Cover ledger reads and publication as well as compositor requests.
        // Bring uses this same lock when adopting a visitor.
        let _lock = files::lock(&paths.state.join("monitor-workspaces.lock"))?;
        let desktop_path = paths.config.join("desktop.json");
        let mut desktop = state::read_strict(&desktop_path, json!({}))?;
        if let Ok(bytes) = fs::read(paths.state.join("workspace-session.json")) {
            if let Ok(ledger) = serde_json::from_slice::<Value>(&bytes) {
                if ledger["socket"].as_str() == env::var("NIRI_SOCKET").ok().as_deref() {
                    if !self.initialized {
                        self.origins =
                            serde_json::from_value(ledger["origins"].clone()).unwrap_or_default();
                        self.views =
                            serde_json::from_value(ledger["views"].clone()).unwrap_or_default();
                        self.view_names =
                            serde_json::from_value(ledger["viewNames"].clone()).unwrap_or_default();
                    }
                    self.adoptions =
                        serde_json::from_value(ledger["adoptions"].clone()).unwrap_or_default();
                }
            }
        }
        let outputs = self.observe(value, &desktop);
        let reconnected: Vec<_> = outputs.difference(&self.connected).cloned().collect();
        let changed = !self.initialized || outputs != self.connected;
        let policy = json!([
            desktop["niriWorkspaceMode"],
            desktop["workspacePrimaryOutput"],
            desktop["monitorWorkspaceActions"],
            desktop["niriWorkspaceSets"]
        ]);
        let signature = json!([
            policy,
            outputs,
            value["workspaces"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["visiting"] == true)
                .map(|r| (key(r), occupied(value, key(r))))
                .collect::<BTreeMap<_, _>>()
        ]);
        let reconcile = self.signature != signature;
        let loader = env::var_os("NACRE_NIRI_CONFIG")
            .or_else(|| env::var_os("NIRI_CONFIG"))
            .map(PathBuf::from);
        if let Some(loader) = loader {
            let fragment = loader
                .parent()
                .ok_or("Missing managed config directory")?
                .join("workspaces.kdl");
            // Never edit a private user loader or invent an include.
            let text = fs::read_to_string(&loader)?;
            let include = format!(
                "include {}",
                crate::json::encode(&fragment.to_string_lossy())?
            );
            if reconcile
                && text
                    .lines()
                    .any(|l| l.trim() == "include \"workspaces.kdl\"" || l.trim() == include)
            {
                let mut io = native()?;
                let current = snapshot(&mut io)?;
                let live_outputs: BTreeSet<String> = current["monitors"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|m| m["name"].as_str().map(str::to_owned))
                    .collect();
                if live_outputs != outputs {
                    // Remember the causal view, but never reconcile an already
                    // superseded topology against the compositor's live state.
                    self.connected = outputs;
                    self.initialized = true;
                    return Ok(());
                }

                let profile: Value =
                    serde_json::from_slice(&fs::read(paths.config.join("profile.json"))?)?;
                let default = names(&profile);
                declarations(&default, &desktop, Some(&outputs))?;
                let mut known = sets(&desktop);
                if desktop["niriWorkspaceMode"] == "fixed-all" {
                    let mut used: BTreeSet<String> = known.values().flatten().cloned().collect();
                    for output in &outputs {
                        if !known.contains_key(output) {
                            let primary_names = primary_names(&default, &desktop);
                            let values = if primary(&desktop, &outputs).as_ref() == Some(output)
                                && primary_names.iter().all(|name| !used.contains(name))
                            {
                                used.extend(primary_names.clone());
                                primary_names
                            } else {
                                (1..=default.len())
                                    .map(|slot| {
                                        let mut name = format!("{output}:{slot}");
                                        while used.contains(&name) {
                                            name.push('_');
                                        }
                                        used.insert(name.clone());
                                        name
                                    })
                                    .collect()
                            };
                            known.insert(output.clone(), values);
                        }
                    }
                    desktop["niriWorkspaceSets"] = json!(known);
                }
                let mut desired = if desktop["niriWorkspaceMode"] == "fixed-all" {
                    known.clone()
                } else {
                    primary(&desktop, &outputs)
                        .map(|p| BTreeMap::from([(p, primary_names(&default, &desktop))]))
                        .unwrap_or_default()
                };
                // A disconnected output contributes only occupied named visitors.
                for (output, names) in &mut desired {
                    if !outputs.contains(output) {
                        names.retain(|name| {
                            current["workspaces"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|r| r["name"] == *name && occupied(&current, key(r)))
                        });
                    }
                }
                let next = render(&desired, default.len())?;
                let bindings = loader
                    .parent()
                    .unwrap()
                    .join("monitor-workspace-actions.kdl");
                let shortcuts = if desktop["niriWorkspaceMode"] == "fixed-all"
                    && desktop["monitorWorkspaceActions"] == true
                {
                    include_str!("../../../../niri/monitor-workspace-actions.kdl")
                } else {
                    ""
                };
                let bindings_changed = text
                    .lines()
                    .any(|l| l.trim() == "include \"monitor-workspace-actions.kdl\"")
                    && fs::read_to_string(&bindings).ok().as_deref() != Some(shortcuts);
                if fs::read_to_string(&fragment)? != next || bindings_changed {
                    let previous = fs::read(&fragment)?;
                    let previous_bindings = fs::read(&bindings).unwrap_or_default();
                    validate_includes(&loader, &text, &next, shortcuts)?;
                    if fs::read(&fragment)? != previous || fs::read_to_string(&loader)? != text {
                        return Err(
                            "Managed configuration changed during workspace reconciliation".into(),
                        );
                    }
                    if bindings_changed {
                        files::atomic(&bindings, shortcuts.as_bytes())?;
                    }
                    files::atomic(&fragment, next.as_bytes())?;
                    if let Err(error) = action(
                        &mut io,
                        "LoadConfigFile",
                        json!({"path":loader.to_string_lossy()}),
                    ) {
                        files::atomic(&fragment, &previous)?;
                        if bindings_changed {
                            files::atomic(&bindings, &previous_bindings)?;
                        }
                        return Err(error);
                    }
                    // LoadConfigFile queues parsing on niri's watcher thread.
                    // Wait for native state, never a guessed hotplug delay.
                    let wanted: BTreeSet<&String> = desired.values().flatten().collect();
                    let removed: Vec<&str> = current["workspaces"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter_map(|r| r["name"].as_str())
                        .filter(|name| {
                            !wanted.iter().any(|n| *n == name)
                                && (known.values().flatten().any(|n| n == name)
                                    || default.iter().any(|n| n == name))
                        })
                        .collect();
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                    loop {
                        let now = snapshot(&mut io)?;
                        if desired.values().flatten().all(|name| {
                            now["workspaces"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|r| r["name"] == *name)
                        }) && removed.iter().all(|name| {
                            !now["workspaces"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|r| r["name"] == *name)
                        }) {
                            break;
                        }
                        if std::time::Instant::now() >= deadline {
                            files::atomic(&fragment, &previous)?;
                            if bindings_changed {
                                files::atomic(&bindings, &previous_bindings)?;
                            }
                            let _ = action(
                                &mut io,
                                "LoadConfigFile",
                                json!({"path":loader.to_string_lossy()}),
                            );
                            return Err(
                                "Named workspace reload did not settle; original include restored"
                                    .into(),
                            );
                        }
                        std::thread::sleep(std::time::Duration::from_millis(20));
                    }
                    // Removing a declaration unsets its name in niri. Explicitly
                    // unname any remaining empty visitor (including manual names).
                    for row in current["workspaces"].as_array().unwrap() {
                        if row["name"].is_string() && !occupied(&current, key(row)) {
                            if let Some((home, _)) = self.origins.get(key(row)) {
                                if !outputs.contains(home) {
                                    action(
                                        &mut io,
                                        "UnsetWorkspaceName",
                                        json!({"reference":{"Id":number(key(row), "niri-workspace:")?}}),
                                    )?;
                                }
                            }
                        }
                    }
                }
                if changed || self.signature.get(0) != Some(&policy) {
                    // Config reload creates missing names at the top. Put own slots
                    // in configured order without creating any additional spares.
                    let now = snapshot(&mut io)?;
                    for (output, names) in &desired {
                        if !outputs.contains(output) {
                            continue;
                        }
                        for (index, name) in names.iter().enumerate() {
                            if let Some(row) = now["workspaces"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .find(|r| r["name"] == *name)
                            {
                                let reference = json!({"Id":number(key(row),"niri-workspace:")?});
                                if row["outputId"] != *output {
                                    action(
                                        &mut io,
                                        "MoveWorkspaceToMonitor",
                                        json!({"output":output,"reference":reference}),
                                    )?;
                                }
                                action(
                                    &mut io,
                                    "MoveWorkspaceToIndex",
                                    json!({"index":index+1,"reference":reference}),
                                )?;
                            }
                        }
                    }
                }
                // Read/write through the versioned store: original bytes get a
                // pre-vN-to-v3 backup, and unrelated preferences are preserved.
                if sets(&desktop) != sets(&state::read_strict(&desktop_path, json!({}))?)
                    || fs::read(&desktop_path)
                        .ok()
                        .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
                        .is_some_and(|v| v["version"] != 3)
                {
                    state::update_as(&desktop_path, state::by_id("desktop")?, |mut v| {
                        if desktop["niriWorkspaceSets"].is_object() {
                            v["niriWorkspaceSets"] = desktop["niriWorkspaceSets"].clone();
                        }
                        Ok(v)
                    })?;
                }
                if changed && self.initialized {
                    let focus = current["activeWorkspace"]["id"].clone();
                    let focus_window = current["activeWorkspace"]["activeWindowId"].clone();
                    for output in &reconnected {
                        if let Some((workspace, window)) = self.views.get(output) {
                            let now = snapshot(&mut io)?;
                            if let Some(row) = now["workspaces"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .find(|r| {
                                    r["outputId"] == *output
                                        && (key(r) == workspace
                                            || self.view_names.get(output).is_some_and(|name| {
                                                !name.is_empty() && r["name"] == *name
                                            }))
                                })
                                .or_else(|| {
                                    // Dynamic empty workspaces disappear on unplug.
                                    // Reuse niri's existing spare, never create one.
                                    let index = self
                                        .view_names
                                        .get(output)
                                        .and_then(|s| s.parse::<u64>().ok())
                                        .unwrap_or(u64::MAX);
                                    now["workspaces"]
                                        .as_array()
                                        .unwrap()
                                        .iter()
                                        .filter(|r| {
                                            r["outputId"] == *output
                                                && r["name"].is_null()
                                                && !occupied(&now, key(r))
                                        })
                                        .min_by_key(|r| {
                                            r["index"].as_u64().unwrap_or(0).abs_diff(index)
                                        })
                                })
                            {
                                focus_view(&mut io, key(row), window)?;
                            }
                        }
                    }
                    if current["focusedMonitor"]["name"]
                        .as_str()
                        .is_some_and(|output| reconnected.iter().any(|name| name == output))
                    {
                        action(
                            &mut io,
                            "FocusMonitor",
                            json!({"output":current["focusedMonitor"]["name"]}),
                        )?;
                    } else if let Some(focus) = focus.as_str() {
                        focus_view(&mut io, focus, &focus_window)?;
                    }
                }
            }
        }
        self.signature = signature;
        self.connected = outputs;
        self.initialized = true;
        // The short-lived Bring command uses the same origin ledger, scoped to
        // this compositor socket so stale sessions cannot adopt windows.
        let ledger = json!({"socket":env::var("NIRI_SOCKET").ok(),"origins":self.origins,"adoptions":self.adoptions,"connected":self.connected,"views":self.views,"viewNames":self.view_names});
        let path = paths.state.join("workspace-session.json");
        if fs::read(&path)
            .ok()
            .and_then(|b| serde_json::from_slice::<Value>(&b).ok())
            .as_ref()
            != Some(&ledger)
        {
            files::atomic(&path, serde_json::to_string(&ledger)?.as_bytes())?;
        }
        Ok(())
    }
}
fn focus_view(io: &mut Native, workspace: &str, window: &Value) -> Result<()> {
    action(
        io,
        "FocusWorkspace",
        json!({"reference":{"Id":number(workspace,"niri-workspace:")?}}),
    )?;
    if let Some(id) = window.as_str() {
        let now = snapshot(io)?;
        if now["windows"].as_array().unwrap().iter().any(|w| {
            w["id"].as_u64() == number(id, "niri-window:").ok()
                && w["workspace_id"].as_u64() == number(workspace, "niri-workspace:").ok()
        }) {
            action(io, "FocusWindow", json!({"id":number(id,"niri-window:")?}))?;
        }
    }
    Ok(())
}

pub fn bring(paths: &Paths) -> Result<i32> {
    let _lock = files::lock(&paths.state.join("monitor-workspaces.lock"))?;
    let ledger: Value =
        serde_json::from_slice(&fs::read(paths.state.join("workspace-session.json"))?)?;
    if ledger["socket"].as_str() != env::var("NIRI_SOCKET").ok().as_deref() {
        return Err("Workspace session is stale".into());
    }
    let origins: BTreeMap<String, (String, usize)> =
        serde_json::from_value(ledger["origins"].clone())?;
    let desktop = state::read_strict(&paths.config.join("desktop.json"), json!({}))?;
    let mut io = native()?;
    let current = snapshot(&mut io)?;
    let output = current["focusedMonitor"]["name"]
        .as_str()
        .ok_or("No focused monitor")?;
    let rows = current["workspaces"]
        .as_array()
        .ok_or("Missing workspaces")?;
    if desktop["niriWorkspaceMode"] != "fixed-all"
        && origins
            .get(key(&current["activeWorkspace"]))
            .is_some_and(|(home, _)| home != output)
    {
        action(
            &mut io,
            "MoveWorkspaceToMonitor",
            json!({"output":output,"reference":{"Id":number(key(&current["activeWorkspace"]),"niri-workspace:")?}}),
        )?;
        let mut adopted = ledger.clone();
        if !adopted["adoptions"].is_object() {
            adopted["adoptions"] = json!({});
        }
        adopted["adoptions"][key(&current["activeWorkspace"])] = json!(output);
        adopted["origins"][key(&current["activeWorkspace"])] =
            json!([output, current["activeWorkspace"]["index"]]);
        files::atomic(
            &paths.state.join("workspace-session.json"),
            serde_json::to_string(&adopted)?.as_bytes(),
        )?;
    }
    let mut moves = Vec::new();
    for visitor in rows.iter().filter(|r| r["outputId"] == output) {
        let Some((home, slot)) = origins.get(key(visitor)) else {
            continue;
        };
        if home == output {
            continue;
        }
        let target = if desktop["niriWorkspaceMode"] == "fixed-all" {
            rows.iter()
                .find(|r| {
                    r["outputId"] == output
                        && origins
                            .get(key(r))
                            .is_some_and(|(home, n)| home == output && n == slot)
                })
                .ok_or("Local numbered workspace is unavailable")?
        } else {
            &current["activeWorkspace"]
        };
        if key(target) == key(visitor) {
            continue;
        }
        for window in current["windows"]
            .as_array()
            .ok_or("Missing windows")?
            .iter()
            .filter(|w| w["workspace_id"].as_u64() == number(key(visitor), "niri-workspace:").ok())
        {
            moves.push((
                window["id"].clone(),
                number(key(target), "niri-workspace:")?,
            ));
        }
    }
    for (window, target) in &moves {
        // Native moves create columns. Focus and place each at the far right
        // so existing columns keep their order, independent of active column.
        action(
            &mut io,
            "MoveWindowToWorkspace",
            json!({"window_id":window,"reference":{"Id":target},"focus":false}),
        )?;
        action(&mut io, "FocusWindow", json!({"id":window}))?;
        action(&mut io, "MoveColumnToLast", json!({}))?;
    }
    let focus = &current["activeWorkspace"]["activeWindowId"];
    if let Some(focus) = focus.as_str() {
        action(
            &mut io,
            "FocusWindow",
            json!({"id":number(focus,"niri-window:")?}),
        )?;
    } else {
        focus_view(&mut io, key(&current["activeWorkspace"]), focus)?;
    }
    let moved = moves.len();
    println!("{}", json!({"ok":true,"movedWindows":moved}));
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_primary_is_declared_by_default_and_configured_primary_falls_back_to_laptop(
    ) -> Result<()> {
        let names: Vec<String> = (1..=6).map(|n| format!("local-{n}")).collect();
        let desktop = json!({"workspacePrimaryOutput":"DP-1","niriWorkspaceSets":{"eDP-1":names,"DP-1":["d1","d2","d3","d4","d5","d6"]}});
        let docked = declarations(
            &names,
            &desktop,
            Some(&BTreeSet::from(["eDP-1".into(), "DP-1".into()])),
        )?;
        assert_eq!(docked.matches("workspace ").count(), 6);
        assert_eq!(docked.matches("open-on-output \"DP-1\"").count(), 6);
        let laptop = declarations(&names, &desktop, Some(&BTreeSet::from(["eDP-1".into()])))?;
        assert_eq!(laptop.matches("open-on-output \"eDP-1\"").count(), 6);
        assert!(laptop.contains("workspace \"d1\""));
        assert!(!laptop.contains("workspace \"local-1\""));
        let external: Vec<String> = (1..=6).map(|n| format!("external-{n}")).collect();
        let external_only = declarations(
            &names,
            &json!({"niriWorkspaceSets":{"DP-2":external}}),
            None,
        )?;
        assert!(external_only.contains("workspace \"external-1\""));
        assert!(!external_only.contains("workspace \"local-1\""));
        assert_eq!(external_only.matches("open-on-output \"DP-2\"").count(), 6);
        Ok(())
    }
    #[test]
    fn invalid_policy_and_partial_or_duplicate_saved_sets_are_refused() {
        let names = vec!["one".into(), "two".into()];
        for value in [
            json!({"niriWorkspaceMode":"future"}),
            json!({"niriWorkspaceSets":{"DP-1":["one"]}}),
            json!({"niriWorkspaceSets":{"DP-1":["one","two"],"eDP-1":["one","two"]}}),
            json!({"niriWorkspaceSets":[]}),
        ] {
            assert!(declarations(&names, &value, None).is_err());
        }
    }
    #[test]
    fn origins_and_saved_views_survive_reindex_and_reconnection_snapshots() {
        let desktop =
            json!({"niriWorkspaceMode":"fixed-all","niriWorkspaceSets":{"A":["a1"],"B":["b1"]}});
        let mut session = Session::default();
        let mut value = json!({"monitors":[{"name":"A"},{"name":"B"}],"workspaces":[{"id":"niri-workspace:9","index":1,"name":"b1","outputId":"B","active":true,"activeWindowId":"niri-window:4"}]});
        session.connected = session.observe(&mut value, &desktop);
        session.initialized = true;
        value["monitors"] = json!([{"name":"A"}]);
        value["workspaces"][0]["outputId"] = json!("A");
        value["workspaces"][0]["index"] = json!(7);
        session.connected = session.observe(&mut value, &desktop);
        assert_eq!(value["workspaces"][0]["originIndex"], 1);
        assert_eq!(value["workspaces"][0]["visiting"], true);
        value["monitors"] = json!([{"name":"A"},{"name":"B"}]);
        value["workspaces"][0]["outputId"] = json!("B");
        value["workspaces"][0]["activeWindowId"] = Value::Null;
        session.observe(&mut value, &desktop);
        assert_eq!(session.views["B"].1, "niri-window:4");
    }
}
