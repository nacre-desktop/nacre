//! Monitor-relative tile sends and reversible exchanges of persistent workspaces.
use crate::{
    compositor::{niri_actions::Transport, niri_transport::Connection},
    files,
    paths::Paths,
    Result,
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

const MAX_BYTES: usize = 4 * 1024 * 1024;
const TIMEOUT: f64 = 2.0;
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Mode {
    Send(usize),
    Swap,
    Rotate,
}
#[derive(Clone, Debug)]
pub struct Layout {
    pub outputs: Vec<String>,
    pub slots: BTreeMap<String, Vec<u64>>,
    pub active: BTreeMap<String, u64>,
    pub focused: String,
    pub names: BTreeMap<u64, String>,
}
#[derive(Clone, Debug)]
pub struct Plan {
    pub before: Layout,
    pub after: Layout,
    pub send: Option<u64>,
}
struct Native {
    connection: Connection,
}
impl Transport for Native {
    fn snapshot(
        &mut self,
        timeout: f64,
        max_bytes: usize,
    ) -> std::result::Result<Value, crate::compositor::niri_state::Error> {
        let timeout = Duration::from_secs_f64(timeout);
        let outputs = self.connection.response("Outputs", timeout, max_bytes)?;
        let rows = self.connection.response("Workspaces", timeout, max_bytes)?;
        let bad = || crate::compositor::niri_state::Error {
            kind: "ValueError",
            message: "Malformed native monitor/workspace state".into(),
        };
        let monitors: Vec<Value> = outputs.as_object().ok_or_else(bad)?.iter().filter(|(_, output)| output["logical"].is_object()).map(|(name, output)| json!({"name":name,"x":output["logical"]["x"],"y":output["logical"]["y"]})).collect();
        let mut workspaces = Vec::new();
        for row in rows.as_array().ok_or_else(bad)? {
            let id = row["id"].as_u64().ok_or_else(bad)?;
            workspaces.push(json!({"id":format!("niri-workspace:{id}"),"index":row["idx"],"name":row["name"],"outputId":row["output"],"active":row["is_active"],"focused":row["is_focused"],"activeWindowId":row["active_window_id"].as_u64().map(|id|format!("niri-window:{id}"))}));
        }
        let selected = workspaces
            .iter()
            .find(|row| row["focused"] == true)
            .cloned()
            .unwrap_or(Value::Null);
        Ok(
            json!({"monitors":monitors,"workspaces":workspaces,"focusedMonitor":{"name":selected["outputId"]},"activeWorkspace":selected}),
        )
    }
    fn request(
        &mut self,
        value: Value,
        timeout: f64,
        max_bytes: usize,
    ) -> std::result::Result<Value, crate::compositor::niri_state::Error> {
        self.connection
            .request_value(&value, Duration::from_secs_f64(timeout), max_bytes)
    }
}
fn id(value: &Value) -> Result<u64> {
    value
        .as_str()
        .and_then(|s| s.strip_prefix("niri-workspace:"))
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| "Invalid workspace identity".into())
}
fn layout(snapshot: &Value, count: usize) -> Result<Layout> {
    let monitors = snapshot["monitors"]
        .as_array()
        .ok_or("Missing monitor state")?;
    let mut monitors: Vec<_> = monitors.iter().filter(|m| m["name"].is_string()).collect();
    monitors.sort_by(|a, b| {
        a["x"]
            .as_i64()
            .unwrap_or(0)
            .cmp(&b["x"].as_i64().unwrap_or(0))
            .then(
                a["y"]
                    .as_i64()
                    .unwrap_or(0)
                    .cmp(&b["y"].as_i64().unwrap_or(0)),
            )
            .then(a["name"].as_str().cmp(&b["name"].as_str()))
    });
    let outputs: Vec<String> = monitors
        .iter()
        .map(|m| m["name"].as_str().unwrap().to_owned())
        .collect();
    if outputs.len() < 2 {
        return Err("Another connected monitor is required".into());
    }
    let rows = snapshot["workspaces"]
        .as_array()
        .ok_or("Missing workspace state")?;
    let focused = snapshot["focusedMonitor"]["name"]
        .as_str()
        .ok_or("No focused monitor")?
        .to_owned();
    if !outputs.contains(&focused) {
        return Err("Focused monitor is disconnected".into());
    }
    let mut slots = BTreeMap::new();
    let mut active = BTreeMap::new();
    let mut names = BTreeMap::new();
    for output in &outputs {
        let mut named: Vec<_> = rows
            .iter()
            .filter(|r| {
                r["outputId"] == *output && r["name"].as_str().is_some_and(|s| !s.is_empty())
            })
            .collect();
        named.sort_by_key(|r| r["index"].as_u64().unwrap_or(u64::MAX));
        if named.len() != count {
            return Err(format!("{output} must have exactly {count} persistent workspaces").into());
        }
        let mut ids = Vec::new();
        for row in named {
            let key = id(&row["id"])?;
            ids.push(key);
            names.insert(key, row["name"].as_str().unwrap().to_owned());
        }
        let selected = rows
            .iter()
            .find(|r| r["outputId"] == *output && r["active"] == true)
            .ok_or("Monitor has no active workspace")?;
        let selected = id(&selected["id"])?;
        slots.insert(output.clone(), ids);
        active.insert(output.clone(), selected);
    }
    Ok(Layout {
        outputs,
        slots,
        active,
        focused,
        names,
    })
}
pub fn plan(snapshot: &Value, count: usize, mode: Mode, direction: i32) -> Result<Plan> {
    if !(1..=10).contains(&count) || ![-1, 1].contains(&direction) {
        return Err("Invalid workspace count or direction".into());
    }
    let before = layout(snapshot, count)?;
    let mut after = before.clone();
    let index = before
        .outputs
        .iter()
        .position(|o| o == &before.focused)
        .ok_or("Missing source monitor")?;
    let next =
        |index: usize| (index as i32 + direction).rem_euclid(before.outputs.len() as i32) as usize;
    let target = &before.outputs[next(index)];
    let send = match mode {
        Mode::Send(number) => {
            if number == 0 || number > count {
                return Err("Workspace number is outside the configured set".into());
            }
            let selected = snapshot["activeWorkspace"]["activeWindowId"]
                .as_str()
                .filter(|s| !s.is_empty());
            if selected.is_none() {
                return Err("There is no selected tile to send".into());
            }
            Some(before.slots[target][number - 1])
        }
        Mode::Swap => {
            let source_id = before.active[&before.focused];
            let target_id = before.active[target];
            let source_slot = before.slots[&before.focused]
                .iter()
                .position(|id| *id == source_id)
                .ok_or("Select a configured workspace first")?;
            let target_slot = before.slots[target]
                .iter()
                .position(|id| *id == target_id)
                .ok_or("The other monitor must select a configured workspace first")?;
            after.slots.get_mut(&before.focused).unwrap()[source_slot] = target_id;
            after.slots.get_mut(target).unwrap()[target_slot] = source_id;
            after.active.insert(before.focused.clone(), target_id);
            after.active.insert(target.clone(), source_id);
            None
        }
        Mode::Rotate => {
            if before
                .outputs
                .iter()
                .any(|output| !before.slots[output].contains(&before.active[output]))
            {
                return Err(
                    "Select configured workspaces on all monitors before rotating sets".into(),
                );
            }
            for (index, output) in before.outputs.iter().enumerate() {
                let target = before.outputs[next(index)].clone();
                after
                    .slots
                    .insert(target.clone(), before.slots[output].clone());
                after.active.insert(target, before.active[output]);
            }
            None
        }
    };
    Ok(Plan {
        before,
        after,
        send,
    })
}
fn action(io: &mut impl Transport, name: &str, params: Value) -> Result<()> {
    let response = io.request(json!({"Action":{name:params}}), TIMEOUT, MAX_BYTES)?;
    if response != "Handled" {
        return Err("Compositor did not acknowledge the workspace action".into());
    }
    Ok(())
}
fn snapshot(io: &mut impl Transport) -> Result<Value> {
    Ok(io.snapshot(TIMEOUT, MAX_BYTES)?)
}
fn assign(io: &mut impl Transport, wanted: &Layout) -> Result<()> {
    // Resolve from actual state, including a request whose acknowledgement was lost.
    let current = snapshot(io)?;
    let rows = current["workspaces"]
        .as_array()
        .ok_or("Missing workspace state")?;
    let connected: BTreeSet<_> = current["monitors"]
        .as_array()
        .ok_or("Missing monitors")?
        .iter()
        .filter_map(|r| r["name"].as_str())
        .collect();
    if wanted
        .outputs
        .iter()
        .any(|o| !connected.contains(o.as_str()))
    {
        return Err("Monitor disconnected; automatic compositor recovery preserved".into());
    }
    for (output, ids) in &wanted.slots {
        for key in ids {
            let row = rows
                .iter()
                .find(|r| id(&r["id"]).ok() == Some(*key))
                .ok_or("Workspace disappeared")?;
            if row["outputId"] != *output {
                action(
                    io,
                    "MoveWorkspaceToMonitor",
                    json!({"reference":{"Id":key},"output":output}),
                )?;
            }
        }
    }
    for (output, ids) in &wanted.slots {
        for (index, key) in ids.iter().enumerate() {
            action(
                io,
                "MoveWorkspaceToIndex",
                json!({"reference":{"Id":key},"index":index+1}),
            )?;
        }
        if output != &wanted.focused {
            action(
                io,
                "FocusWorkspace",
                json!({"reference":{"Id":wanted.active[output]}}),
            )?;
        }
    }
    action(
        io,
        "FocusWorkspace",
        json!({"reference":{"Id":wanted.active[&wanted.focused]}}),
    )?;
    let observed = snapshot(io)?;
    let observed = layout(&observed, wanted.slots[&wanted.focused].len())?;
    if observed.outputs != wanted.outputs
        || observed.slots != wanted.slots
        || observed.active != wanted.active
        || observed.focused != wanted.focused
    {
        return Err("Workspace layout changed during the operation".into());
    }
    Ok(())
}
pub fn apply(
    io: &mut impl Transport,
    plan: &Plan,
    mut persist: impl FnMut(&Layout) -> Result<()>,
) -> Result<()> {
    if let Some(target) = plan.send {
        return action(
            io,
            "MoveWindowToWorkspace",
            json!({"window_id":null,"reference":{"Id":target},"focus":false}),
        );
    }
    let result = assign(io, &plan.after).and_then(|_| persist(&plan.after));
    if let Err(error) = result {
        return match assign(io, &plan.before) {
            Ok(()) => Err(format!("Workspace exchange cancelled and restored: {error}").into()),
            Err(recovery) => Err(format!(
                "Workspace exchange failed: {error}; recovery could not complete: {recovery}"
            )
            .into()),
        };
    }
    Ok(())
}
fn ownership(layout: &Layout) -> Value {
    Value::Object(
        layout
            .slots
            .iter()
            .map(|(output, ids)| {
                (
                    output.clone(),
                    json!(ids
                        .iter()
                        .map(|id| layout.names[id].clone())
                        .collect::<Vec<_>>()),
                )
            })
            .collect(),
    )
}
struct Preferences {
    paths: Paths,
    fragment: PathBuf,
    loader: PathBuf,
}
impl Preferences {
    fn check(&self) -> Result<()> {
        if !self.fragment.is_absolute() || !self.fragment.is_file() {
            return Err("Managed workspace configuration is missing".into());
        }
        let text = fs::read_to_string(&self.loader)?;
        let absolute = format!(
            "include {}",
            crate::json::encode(&self.fragment.to_string_lossy())?
        );
        let relative = format!(
            "include {}",
            crate::json::encode(&self.fragment.file_name().unwrap().to_string_lossy())?
        );
        let included = |expected: &str| {
            text.lines()
                .any(|line| line.trim().trim_end_matches(';') == expected)
        };
        if !included(&absolute)
            && !(self.fragment.parent() == self.loader.parent() && included(&relative))
        {
            return Err(
                "Active Niri configuration does not include the managed workspace file".into(),
            );
        }
        Ok(())
    }
    fn save(&self, layout: &Layout) -> Result<()> {
        self.check()?;
        let desktop_path = self.paths.config.join("desktop.json");
        let schema = crate::state::schema(&desktop_path)
            .ok_or("Desktop preferences schema is unavailable")?;
        let previous = fs::read(&self.fragment)?;
        let replacement = ownership(layout);
        let desktop: Value = crate::state::read_strict(&desktop_path, json!({}))?;
        let old_mapping = desktop.get("niriWorkspaceOutputs").cloned();
        let mut combined = old_mapping.clone().unwrap_or(json!({}));
        let map = combined
            .as_object_mut()
            .ok_or("Invalid saved workspace outputs")?;
        for (name, value) in replacement.as_object().unwrap() {
            map.insert(name.clone(), value.clone());
        }
        let names = vec![String::new(); layout.slots[&layout.focused].len()];
        let text = crate::niri_session_runtime::workspace_declarations(
            &names,
            &json!({"niriWorkspaceOutputs":combined}),
        )?;
        // Validate the actual loader with only this include replaced before publishing.
        let mut fragment = tempfile::NamedTempFile::new_in(self.fragment.parent().unwrap())?;
        use std::io::Write;
        fragment.write_all(text.as_bytes())?;
        let loader_text = fs::read_to_string(&self.loader)?;
        let candidate_include = format!(
            "include {}",
            crate::json::encode(&fragment.path().to_string_lossy())?
        );
        let absolute = format!(
            "include {}",
            crate::json::encode(&self.fragment.to_string_lossy())?
        );
        let relative = format!(
            "include {}",
            crate::json::encode(&self.fragment.file_name().unwrap().to_string_lossy())?
        );
        let candidate = loader_text
            .replace(&absolute, &candidate_include)
            .replace(&relative, &candidate_include);
        let mut loader = tempfile::NamedTempFile::new_in(self.loader.parent().unwrap())?;
        loader.write_all(candidate.as_bytes())?;
        let mut command = std::process::Command::new("niri");
        command.args(["validate", "--config"]).arg(loader.path());
        let validated = crate::process::capture(command, None, Duration::from_secs(5))?;
        if !validated.status.success() {
            return Err(
                "Workspace configuration validation failed; original settings preserved".into(),
            );
        }
        if fs::read(&self.fragment)? != previous {
            return Err("Workspace file changed during the exchange".into());
        }
        files::atomic(&self.fragment, text.as_bytes())?;
        if let Err(error) = crate::state::update_as(&desktop_path, schema, |mut value| {
            if value.get("niriWorkspaceOutputs").cloned() != old_mapping {
                return Err("Workspace preferences changed during the exchange".into());
            }
            value
                .as_object_mut()
                .ok_or("Invalid desktop preferences")?
                .insert("niriWorkspaceOutputs".into(), combined.clone());
            Ok(value)
        }) {
            files::atomic(&self.fragment, &previous)?;
            return Err(error);
        }
        Ok(())
    }
}
pub fn run(args: &[String]) -> Result<i32> {
    let mut args = args.to_vec();
    let file = if let Some(index) = args.iter().position(|s| s == "--workspace-file") {
        if index + 1 >= args.len() {
            return Err("Missing workspace file".into());
        }
        let path = PathBuf::from(args.remove(index + 1));
        args.remove(index);
        Some(path)
    } else {
        None
    };
    let loader_override = if let Some(index) = args.iter().position(|s| s == "--loader") {
        if index + 1 >= args.len() {
            return Err("Missing Niri loader path".into());
        }
        let path = PathBuf::from(args.remove(index + 1));
        args.remove(index);
        Some(path)
    } else {
        None
    };
    let dry = args.first().is_some_and(|s| s == "plan");
    if dry {
        args.remove(0);
    }
    let direction = match args.get(1).map(String::as_str) {
        Some("next") => 1,
        Some("previous") => -1,
        _ => return Err("Expected next or previous monitor direction".into()),
    };
    let mode = match args.as_slice() {
        [mode, _, number] if mode == "send" => Mode::Send(number.parse()?),
        [mode, _] if mode == "swap" => Mode::Swap,
        [mode, _] if mode == "rotate" => Mode::Rotate,
        _ => {
            return Err(
                "Expected send DIRECTION WORKSPACE, swap DIRECTION or rotate DIRECTION".into(),
            )
        }
    };
    let paths = Paths::environment()?;
    let profile: Value = serde_json::from_slice(&fs::read(paths.config.join("profile.json"))?)?;
    let count = profile["workspaceCount"]
        .as_u64()
        .ok_or("Missing configured workspace count")? as usize;
    let socket = env::var_os("NIRI_SOCKET")
        .filter(|s| !s.is_empty())
        .ok_or("NIRI_SOCKET is unavailable")?;
    let mut io = Native {
        connection: Connection::new(socket),
    };
    let _lock = if dry {
        None
    } else {
        Some(files::lock(&paths.state.join("monitor-workspaces.lock"))?)
    };
    let current = snapshot(&mut io)?;
    let selected = plan(&current, count, mode, direction)?;
    if dry {
        println!(
            "{}",
            json!({"outputs":selected.before.outputs,"source":selected.before.focused,"sendWorkspaceId":selected.send,"workspaceSets":ownership(&selected.after)})
        );
        return Ok(0);
    }
    let loader = loader_override
        .or_else(|| env::var_os("NACRE_NIRI_CONFIG").map(PathBuf::from))
        .or_else(|| env::var_os("NIRI_CONFIG").map(PathBuf::from))
        .unwrap_or_else(|| paths.config.join("niri/config.kdl"));
    let fragment = file.unwrap_or_else(|| {
        loader
            .parent()
            .unwrap_or(Path::new("."))
            .join("workspaces.kdl")
    });
    let preferences = Preferences {
        paths,
        fragment,
        loader,
    };
    if selected.send.is_none() {
        preferences.check()?;
    }
    apply(&mut io, &selected, |wanted| preferences.save(wanted))?;
    println!(
        "{}",
        json!({"ok":true,"source":selected.before.focused,"outputs":selected.before.outputs})
    );
    Ok(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture {
        state: Value,
        calls: Vec<Value>,
        fail: Option<(usize, bool)>,
        disconnected: bool,
    }
    impl Fixture {
        fn new(outputs: usize) -> Self {
            let monitors: Vec<_> = (0..outputs)
                .rev()
                .map(|i| json!({"name":format!("screen-{i}"),"x":i*200,"y":0}))
                .collect();
            let workspaces:Vec<_>=(0..outputs).flat_map(|output|(0..6).map(move|index|json!({"id":format!("niri-workspace:{}",output*100+index+1),"index":index+1,"name":format!("set-{output}-{index}"),"outputId":format!("screen-{output}"),"active":index==output.min(5),"activeWindowId":"niri-window:123"}))).collect();
            let mut me = Self {
                state: json!({"monitors":monitors,"workspaces":workspaces,"focusedMonitor":{"name":"screen-0"},"activeWindow":null}),
                calls: vec![],
                fail: None,
                disconnected: false,
            };
            me.selected();
            me
        }
        fn selected(&mut self) {
            let output = self.state["focusedMonitor"]["name"].clone();
            self.state["activeWorkspace"] = self.state["workspaces"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["outputId"] == output && r["active"] == true)
                .cloned()
                .unwrap_or(Value::Null);
        }
        fn normalize(&mut self, output: &Value) {
            let rows = self.state["workspaces"].as_array_mut().unwrap();
            let mut indices: Vec<_> = rows
                .iter()
                .enumerate()
                .filter(|(_, r)| r["outputId"] == *output)
                .map(|(i, r)| (i, r["index"].as_u64().unwrap()))
                .collect();
            indices.sort_by_key(|(_, index)| *index);
            for (index, (row, _)) in indices.iter().enumerate() {
                rows[*row]["index"] = json!(index + 1);
            }
        }
        fn handle(&mut self, request: &Value) {
            let (name, args) = request["Action"]
                .as_object()
                .unwrap()
                .iter()
                .next()
                .unwrap();
            if name == "MoveWindowToWorkspace" {
                return;
            }
            let key = args["reference"]["Id"].as_u64().unwrap();
            let pos = self.state["workspaces"]
                .as_array()
                .unwrap()
                .iter()
                .position(|r| id(&r["id"]).unwrap() == key)
                .unwrap();
            let output = self.state["workspaces"][pos]["outputId"].clone();
            match name.as_str() {
                "MoveWorkspaceToMonitor" => {
                    let target = args["output"].clone();
                    let index = self.state["workspaces"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .filter(|r| r["outputId"] == target)
                        .map(|r| r["index"].as_u64().unwrap())
                        .max()
                        .unwrap_or(0)
                        + 1;
                    self.state["workspaces"][pos]["outputId"] = target.clone();
                    self.state["workspaces"][pos]["index"] = json!(index);
                    self.state["workspaces"][pos]["active"] = json!(false);
                    self.normalize(&output);
                    self.normalize(&target);
                }
                "MoveWorkspaceToIndex" => {
                    let wanted = args["index"].as_u64().unwrap() as usize;
                    let rows = self.state["workspaces"].as_array_mut().unwrap();
                    let mut others: Vec<_> = rows
                        .iter()
                        .enumerate()
                        .filter(|(i, r)| *i != pos && r["outputId"] == output)
                        .map(|(i, r)| (i, r["index"].as_u64().unwrap()))
                        .collect();
                    others.sort_by_key(|(_, index)| *index);
                    others.insert((wanted - 1).min(others.len()), (pos, 0));
                    for (index, (row, _)) in others.iter().enumerate() {
                        rows[*row]["index"] = json!(index + 1);
                    }
                }
                "FocusWorkspace" => {
                    for row in self.state["workspaces"].as_array_mut().unwrap() {
                        if row["outputId"] == output {
                            row["active"] = json!(id(&row["id"]).unwrap() == key);
                        }
                    }
                    self.state["focusedMonitor"]["name"] = output;
                }
                other => panic!("unexpected action {other}"),
            }
            self.selected();
        }
    }
    impl Transport for Fixture {
        fn snapshot(
            &mut self,
            _: f64,
            _: usize,
        ) -> std::result::Result<Value, crate::compositor::niri_state::Error> {
            Ok(self.state.clone())
        }
        fn request(
            &mut self,
            value: Value,
            _: f64,
            _: usize,
        ) -> std::result::Result<Value, crate::compositor::niri_state::Error> {
            self.calls.push(value.clone());
            let fail = self.fail.filter(|(n, _)| *n == self.calls.len());
            if fail.is_none_or(|(_, applied)| applied) {
                self.handle(&value);
            }
            if self.disconnected && fail.is_some() {
                self.state["monitors"].as_array_mut().unwrap().pop();
            }
            if fail.is_some() {
                self.fail = None;
                return Err(crate::compositor::niri_state::Error {
                    kind: "OSError",
                    message: "injected failure".into(),
                });
            }
            Ok(json!("Handled"))
        }
    }
    #[test]
    fn sends_to_another_monitors_numbered_workspace_without_following_even_in_overview(
    ) -> Result<()> {
        let mut io = Fixture::new(3);
        let original = io.state.clone();
        let p = plan(&io.state, 6, Mode::Send(4), 1)?;
        apply(&mut io, &p, |_| panic!("send must not save ownership"))?;
        assert_eq!(
            io.calls,
            vec![
                json!({"Action":{"MoveWindowToWorkspace":{"window_id":null,"reference":{"Id":104},"focus":false}}})
            ]
        );
        assert_eq!(io.state, original);
        assert_eq!(plan(&io.state, 6, Mode::Send(6), -1)?.send, Some(206));
        Ok(())
    }
    #[test]
    fn swaps_only_active_workspaces_preserves_slots_and_round_trips() -> Result<()> {
        let mut io = Fixture::new(3);
        let original = layout(&io.state, 6)?;
        let p = plan(&io.state, 6, Mode::Swap, 1)?;
        assert_eq!(p.after.slots["screen-0"], vec![102, 2, 3, 4, 5, 6]);
        assert_eq!(p.after.slots["screen-1"], vec![101, 1, 103, 104, 105, 106]);
        assert_eq!(p.after.slots["screen-2"], original.slots["screen-2"]);
        let mut saved = None;
        apply(&mut io, &p, |desired| {
            saved = Some(ownership(desired));
            Ok(())
        })?;
        assert_eq!(saved.unwrap()["screen-0"][0], "set-1-1");
        assert_eq!(layout(&io.state, 6)?.slots, p.after.slots);
        let p = plan(&io.state, 6, Mode::Swap, 1)?;
        apply(&mut io, &p, |_| Ok(()))?;
        assert_eq!(layout(&io.state, 6)?.slots, original.slots);
        Ok(())
    }
    #[test]
    fn rotates_all_sets_in_layout_order_with_a_reverse_operation() -> Result<()> {
        let mut io = Fixture::new(3);
        let original = layout(&io.state, 6)?;
        let p = plan(&io.state, 6, Mode::Rotate, 1)?;
        assert_eq!(p.before.outputs, vec!["screen-0", "screen-1", "screen-2"]);
        assert_eq!(p.after.slots["screen-0"], original.slots["screen-2"]);
        apply(&mut io, &p, |_| Ok(()))?;
        assert_eq!(layout(&io.state, 6)?.active, p.after.active);
        let p = plan(&io.state, 6, Mode::Rotate, -1)?;
        apply(&mut io, &p, |_| Ok(()))?;
        let after = layout(&io.state, 6)?;
        assert_eq!(after.slots, original.slots);
        assert_eq!(after.active, original.active);
        assert_eq!(after.focused, original.focused);
        Ok(())
    }
    #[test]
    fn restores_original_ownership_order_and_selection_after_partial_or_lost_reply_failures(
    ) -> Result<()> {
        for applied in [false, true] {
            for failure in [1, 2, 7, 15] {
                let mut io = Fixture::new(2);
                let original = layout(&io.state, 6)?;
                let p = plan(&io.state, 6, Mode::Rotate, 1)?;
                io.fail = Some((failure, applied));
                assert!(apply(&mut io, &p, |_| Ok(())).is_err());
                let recovered = layout(&io.state, 6)?;
                assert_eq!(
                    recovered.slots, original.slots,
                    "failure {failure} applied {applied}"
                );
                assert_eq!(recovered.active, original.active);
            }
        }
        Ok(())
    }
    #[test]
    fn persistence_failure_reverts_compositor_and_disconnect_never_discards_workspace_identities(
    ) -> Result<()> {
        let mut io = Fixture::new(2);
        let original = layout(&io.state, 6)?;
        let p = plan(&io.state, 6, Mode::Swap, 1)?;
        assert!(apply(&mut io, &p, |_| Err("disk failure".into())).is_err());
        assert_eq!(layout(&io.state, 6)?.slots, original.slots);
        let mut io = Fixture::new(2);
        io.fail = Some((1, true));
        io.disconnected = true;
        assert!(apply(&mut io, &p, |_| Ok(()))
            .unwrap_err()
            .to_string()
            .contains("disconnected"));
        assert_eq!(io.state["workspaces"].as_array().unwrap().len(), 12);
        Ok(())
    }
    #[test]
    fn rejects_missing_output_slots_empty_selection_and_invalid_arguments_before_any_action(
    ) -> Result<()> {
        assert!(plan(&Fixture::new(1).state, 6, Mode::Swap, 1).is_err());
        let mut io = Fixture::new(2);
        io.state["activeWorkspace"]["activeWindowId"] = Value::Null;
        assert!(plan(&io.state, 6, Mode::Send(2), 1).is_err());
        assert!(plan(&io.state, 6, Mode::Swap, 0).is_err());
        assert!(plan(&io.state, 6, Mode::Send(7), 1).is_err());
        io.state["workspaces"].as_array_mut().unwrap().pop();
        assert!(plan(&io.state, 6, Mode::Rotate, 1).is_err());
        assert!(io.calls.is_empty());
        Ok(())
    }
    #[test]
    fn native_unnamed_spares_are_excluded_from_the_six_persistent_slots() -> Result<()> {
        let mut io = Fixture::new(2);
        io.state["workspaces"].as_array_mut().unwrap().push(json!({"id":"niri-workspace:999","index":7,"name":null,"outputId":"screen-0","active":false}));
        let selected = plan(&io.state, 6, Mode::Rotate, 1)?;
        assert_eq!(selected.before.slots["screen-0"].len(), 6);
        apply(&mut io, &selected, |_| Ok(()))?;
        assert_eq!(layout(&io.state, 6)?.slots, selected.after.slots);
        Ok(())
    }
    #[test]
    fn monitor_order_uses_positions_and_wraps_for_stacked_or_negative_coordinates() -> Result<()> {
        let mut io = Fixture::new(3);
        for monitor in io.state["monitors"].as_array_mut().unwrap() {
            let (x, y) = match monitor["name"].as_str().unwrap() {
                "screen-0" => (0, 800),
                "screen-1" => (0, 0),
                _ => (-800, 0),
            };
            monitor["x"] = json!(x);
            monitor["y"] = json!(y);
        }
        let forward = plan(&io.state, 6, Mode::Send(2), 1)?;
        assert_eq!(
            forward.before.outputs,
            vec!["screen-2", "screen-1", "screen-0"]
        );
        assert_eq!(forward.send, Some(202));
        assert_eq!(plan(&io.state, 6, Mode::Send(2), -1)?.send, Some(102));
        Ok(())
    }
    #[test]
    fn opaque_workspace_ids_keep_all_u64_bits() -> Result<()> {
        let mut io = Fixture::new(2);
        io.state["workspaces"][11]["id"] = json!(format!("niri-workspace:{}", u64::MAX));
        let p = plan(&io.state, 6, Mode::Send(6), 1)?;
        apply(&mut io, &p, |_| Ok(()))?;
        assert_eq!(
            io.calls[0]["Action"]["MoveWindowToWorkspace"]["reference"]["Id"].as_u64(),
            Some(u64::MAX)
        );
        Ok(())
    }
}
