//! Read-only topic adapter. The existing compositor reducers and transports own semantics.
use super::{compositor_control::Control, hub::Shared};
use crate::{
    compositor::{backend, hyprland, hyprland_runtime, niri_events, niri_state},
    Result,
};
use serde_json::{json, Map, Value};
use std::{
    cell::RefCell, collections::BTreeMap, io::Cursor, path::PathBuf, rc::Rc, time::Duration,
};
use tokio::{
    io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, BufReader},
    net::UnixStream,
    sync::oneshot,
};
const MAXIMUM: usize = 4 * 1024 * 1024;
pub type Ready = Rc<RefCell<Option<oneshot::Sender<()>>>>;

fn id(value: &Value) -> Option<String> {
    match value {
        Value::String(id) if !id.is_empty() => Some(id.clone()),
        Value::Number(id) => Some(id.to_string()),
        _ => None,
    }
}
fn collection(rows: &Value, keys: &[&str]) -> Result<(Map<String, Value>, Vec<String>)> {
    let mut map = Map::new();
    let mut order = Vec::new();
    for row in rows.as_array().ok_or("Invalid compositor collection")? {
        let key = keys
            .iter()
            .find_map(|key| id(&row[*key]))
            .ok_or("Missing compositor entity ID")?;
        if map.insert(key.clone(), row.clone()).is_some() {
            return Err("Duplicate compositor entity ID".into());
        }
        order.push(key);
    }
    Ok((map, order))
}
/// Project a complete legacy snapshot into stable-key collections, hiding unresolved references.
pub fn project(snapshot: &Value, backend: &str, keyboard: &Value) -> Result<Value> {
    let (mut outputs, output_order) = collection(&snapshot["monitors"], &["name", "id"])?;
    let mut native_outputs = BTreeMap::new();
    for (key, row) in &mut outputs {
        if let Some(native) = id(&row["id"]) {
            native_outputs.insert(native, key.clone());
        }
        if backend == "hyprland" {
            row["nativeId"] = row["id"].clone();
            row["id"] = json!(key);
            row["outputId"] = json!(key);
            let scale = row["scale"]
                .as_f64()
                .filter(|s| s.is_finite() && *s > 0.0)
                .unwrap_or(1.0);
            let (w, h) = (
                row["width"].as_f64().unwrap_or(0.0),
                row["height"].as_f64().unwrap_or(0.0),
            );
            let rotated = row["transform"].as_u64().is_some_and(|t| t % 2 == 1);
            row["logicalWidth"] = json!(if rotated { h / scale } else { w / scale });
            row["logicalHeight"] = json!(if rotated { w / scale } else { h / scale });
        }
    }
    let (mut workspaces, mut workspace_order) =
        collection(&snapshot["workspaces"], &["id", "workspaceId"])?;
    for (key, row) in &mut workspaces {
        if backend == "hyprland" {
            row["nativeId"] = row["id"].clone();
            row["id"] = json!(key);
            row["workspaceId"] = json!(key);
            row["outputId"] = row["monitor"].clone();
        }
    }
    workspaces
        .retain(|_, row| id(&row["outputId"]).is_none_or(|output| outputs.contains_key(&output)));
    workspace_order.retain(|id| workspaces.contains_key(id));
    let (mut windows, mut window_order) =
        collection(&snapshot["windows"], &["windowId", "id", "address"])?;
    for row in windows.values_mut() {
        if backend == "hyprland" {
            row["workspaceId"] = id(&row["workspaceId"])
                .map(Value::String)
                .unwrap_or(Value::Null);
            row["outputId"] = id(&row["outputId"])
                .and_then(|native| native_outputs.get(&native).cloned())
                .map(Value::String)
                .unwrap_or(Value::Null);
            row["nativeMonitorId"] = row["monitor"].clone();
            row["monitor"] = row["outputId"].clone();
        }
    }
    windows.retain(|_, row| {
        id(&row["workspaceId"]).is_none_or(|workspace| workspaces.contains_key(&workspace))
            && id(&row["outputId"]).is_none_or(|output| outputs.contains_key(&output))
    });
    window_order.retain(|id| windows.contains_key(id));
    for row in workspaces.values_mut() {
        if id(&row["activeWindowId"]).is_some_and(|id| !windows.contains_key(&id)) {
            row["activeWindowId"] = Value::Null;
        }
    }
    for row in windows.values_mut() {
        if let Some(workspace) = id(&row["workspaceId"]) {
            row["workspace"] = workspaces.get(&workspace).cloned().unwrap_or(Value::Null);
        }
    }
    for row in outputs.values_mut() {
        if let Some(workspace) = id(&row["activeWorkspace"]["id"]) {
            row["activeWorkspace"] = workspaces.get(&workspace).cloned().unwrap_or(Value::Null);
        }
    }
    let window = id(&snapshot["activeWindow"]["windowId"])
        .or_else(|| id(&snapshot["activeWindow"]["id"]))
        .filter(|id| windows.contains_key(id));
    let workspace = id(&snapshot["activeWorkspace"]["id"])
        .filter(|id| workspaces.contains_key(id))
        .or_else(|| {
            window
                .as_ref()
                .and_then(|window| id(&windows[window]["workspaceId"]))
        });
    let output = id(&snapshot["focusedMonitor"]["name"])
        .filter(|id| outputs.contains_key(id))
        .or_else(|| {
            outputs
                .iter()
                .find(|(_, row)| row["focused"] == true)
                .map(|(id, _)| id.clone())
        });
    let capabilities = if backend == "hyprland" {
        hyprland::capabilities()
    } else {
        niri_state::capabilities()
    };
    Ok(
        json!({"backend":backend,"available":true,"error":"","outputs":outputs,"outputOrder":output_order,"workspaces":workspaces,"workspaceOrder":workspace_order,"windows":windows,"windowOrder":window_order,"focus":{"windowId":window,"workspaceId":workspace,"outputId":output},"keyboard":keyboard,"capabilities":capabilities,"overviewOpen":snapshot.get("overviewOpen").cloned().unwrap_or(json!(false)),"overviewHoverSupported":snapshot.get("overviewHoverSupported").cloned().unwrap_or(json!(false)),"overviewHoveredWindowId":snapshot.get("overviewHoveredWindowId").cloned().unwrap_or(Value::Null),"configFailed":snapshot.get("configFailed").cloned().unwrap_or(json!(false))}),
    )
}
fn publish(
    hub: &Shared,
    snapshot: &Value,
    backend: &str,
    keyboard: &Value,
    ready: &Ready,
) -> Result<()> {
    hub.borrow_mut()
        .publish("compositor", project(snapshot, backend, keyboard)?)?;
    let failures = hub
        .borrow()
        .services
        .get("compositor")
        .and_then(|s| s["failures"].as_u64())
        .unwrap_or(0) as usize;
    hub.borrow_mut()
        .service("compositor", "running", failures)?;
    if let Some(ready) = ready.borrow_mut().take() {
        if ready.send(()).is_err() {
            tracing::debug!("Daemon readiness waiter ended");
        }
    }
    Ok(())
}
async fn line<R: AsyncBufRead + Unpin>(
    reader: &mut R,
    partial: &mut Vec<u8>,
) -> Result<Option<Vec<u8>>> {
    loop {
        let bytes = reader.fill_buf().await?;
        if bytes.is_empty() {
            if partial.is_empty() {
                return Ok(None);
            }
            return Err("Incomplete compositor event".into());
        }
        let newline = bytes.iter().position(|byte| *byte == b'\n');
        let count = newline.map_or(bytes.len(), |n| n + 1);
        if partial.len() + count > MAXIMUM {
            return Err("Compositor event exceeds its limit".into());
        }
        partial.extend_from_slice(&bytes[..count]);
        reader.consume(count);
        if newline.is_some() {
            return Ok(Some(std::mem::take(partial)));
        }
    }
}
async fn outputs(control: &Control) -> Result<Map<String, Value>> {
    control
        .with(|backend| {
            let backend::Backend::Niri(backend) = backend else {
                return Err("Expected niri request transport".into());
            };
            let value =
                backend
                    .actions
                    .transport
                    .response("Outputs", Duration::from_secs(10), MAXIMUM)?;
            let outputs = value
                .as_object()
                .cloned()
                .ok_or("Invalid compositor output reply")?;
            Ok(outputs)
        })
        .await
}
fn schedule(due: &mut Option<tokio::time::Instant>, last: tokio::time::Instant, title_only: bool) {
    let now = tokio::time::Instant::now();
    let target = if title_only {
        (last + Duration::from_millis(100)).max(now)
    } else {
        now
    };
    *due = Some(due.map_or(target, |old| old.min(target)));
}
async fn wait_due(due: Option<tokio::time::Instant>) {
    if let Some(due) = due {
        tokio::time::sleep_until(due).await;
    } else {
        std::future::pending::<()>().await;
    }
}
fn niri_keyboard(event: &Value, keyboard: &mut Value) -> Result<bool> {
    let mut changed = false;
    if let Some(payload) = event.get("KeyboardLayoutsChanged") {
        let layouts = payload
            .get("keyboard_layouts")
            .ok_or("Missing keyboard layout snapshot")?;
        let names = layouts["names"]
            .as_array()
            .filter(|names| names.iter().all(Value::is_string))
            .ok_or("Invalid keyboard layouts")?;
        let current = layouts["current_idx"]
            .as_u64()
            .ok_or("Invalid keyboard layout index")?;
        keyboard["names"] = json!(names);
        keyboard["currentIndex"] = json!(current);
        changed = true;
    }
    if let Some(payload) = event.get("KeyboardLayoutSwitched") {
        keyboard["currentIndex"] = json!(payload["idx"]
            .as_u64()
            .ok_or("Invalid keyboard layout index")?);
        changed = true;
    }
    if changed {
        let active = keyboard["currentIndex"]
            .as_u64()
            .and_then(|idx| keyboard["names"].get(idx as usize))
            .cloned();
        if active.is_none() {
            keyboard["currentIndex"] = Value::Null;
        }
        keyboard["activeName"] = active.unwrap_or(json!(""));
    }
    Ok(changed)
}
fn title_event(event: &Value) -> bool {
    event["type"] == "window-title"
        && event
            .get("changes")
            .and_then(Value::as_array)
            .is_none_or(|changes| changes.len() == 1)
}
fn queue_event(events: &mut Vec<Value>, event: Value) -> Result<bool> {
    // Semantic deltas contain only a kind, IDs and small scalar flags. This
    // bounds the pre-publication batch to 256 KiB even for malformed IPC input.
    if serde_json::to_vec(&event)?.len() > 1024 {
        return Err("Niri semantic event exceeds its bound".into());
    }
    if title_event(&event) {
        if let Some(previous) = events
            .iter_mut()
            .find(|previous| title_event(previous) && previous["windowId"] == event["windowId"])
        {
            *previous = event;
            return Ok(true);
        }
        if events.len() == nacre_protocol::MAX_EVENTS {
            if let Some(index) = events.iter().position(title_event) {
                events.remove(index);
            } else {
                // State still contains every latest title; intermediate title
                // notifications may coalesce behind the pending semantic burst.
                return Ok(true);
            }
        }
    } else if events.len() == nacre_protocol::MAX_EVENTS {
        return Ok(false);
    }
    events.push(event);
    Ok(true)
}
/// Add the policy ledger's `originOutput`/`originIndex`/`visiting` to niri workspace rows.
/// Additive and optional: absent without a ledger for this socket or an entry for the row.
fn annotate_origins(snapshot: &mut Value, ledger: &Value, socket: &str) {
    if ledger["socket"].as_str() != Some(socket) {
        return;
    }
    let Some(rows) = snapshot["workspaces"].as_array_mut() else {
        return;
    };
    for row in rows {
        let origin = id(&row["id"]).and_then(|id| ledger["origins"].get(&id));
        if let (Some(output), Some(index)) = (
            origin.and_then(|o| o[0].as_str()),
            origin.and_then(|o| o[1].as_u64()),
        ) {
            row["visiting"] = json!(row["outputId"]
                .as_str()
                .is_some_and(|owner| owner != output));
            row["originOutput"] = json!(output);
            row["originIndex"] = json!(index);
        }
    }
}
fn flush_niri(
    hub: &Shared,
    state: &niri_state::State,
    keyboard: &Value,
    ready: &Ready,
    connection: &str,
    socket: &str,
    events: &mut Vec<Value>,
) -> Result<()> {
    let mut snapshot = state.snapshot()?;
    if let Some(ledger) = crate::paths::Paths::environment()
        .ok()
        .and_then(|paths| std::fs::read(paths.state.join("workspace-session.json")).ok())
        .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
    {
        annotate_origins(&mut snapshot, &ledger, socket);
    }
    publish(hub, &snapshot, "niri", keyboard, ready)?;
    let mut hub = hub.borrow_mut();
    hub.publish(
        super::niri_events::TOPIC,
        json!({"backend":"niri","available":true,"connectionId":connection}),
    )?;
    for event in events.drain(..) {
        hub.event_after(super::niri_events::TOPIC, "niri", event, Some("compositor"))?;
    }
    Ok(())
}
async fn niri(
    hub: Shared,
    control: Control,
    environment: BTreeMap<String, String>,
    ready: Ready,
) -> Result<()> {
    let path = PathBuf::from(
        environment
            .get("NIRI_SOCKET")
            .filter(|s| !s.is_empty())
            .ok_or("Missing compositor socket")?,
    );
    let connection_id = super::niri_events::identity(&path)?;
    let socket = path.to_string_lossy().into_owned();
    let hover = environment
        .get("NACRE_NIRI_HOVER_STREAM")
        .is_none_or(|value| value != "0");
    let event_path = path.clone();
    let reader = tokio::task::spawn_blocking(move || {
        niri_events::EventSocket::connect(&event_path, hover)
            .map(niri_events::EventSocket::into_stream)
    })
    .await??;
    let prefix = reader.buffer().to_vec();
    let stream = reader.into_inner();
    stream.set_nonblocking(true)?;
    let mut reader = BufReader::new(Cursor::new(prefix).chain(UnixStream::from_std(stream)?));
    let initial = outputs(&control).await?;
    let mut state = niri_state::State::new(initial);
    let mut partial = Vec::new();
    let mut keyboard = json!({"names":[],"currentIndex":null,"activeName":""});
    let mut events = Vec::new();
    let mut due = None;
    let mut last = tokio::time::Instant::now() - Duration::from_millis(100);
    loop {
        tokio::select! {
            bytes=line(&mut reader,&mut partial)=>{
                let event:Value=serde_json::from_slice(&bytes?.ok_or("Compositor event stream ended")?)?;
                let keyboard_changed=niri_keyboard(&event,&mut keyboard)?;
                let normalized=state.apply(&event)?;
                if hub.borrow().services.contains_key("niri-widths") {
                    hub.borrow_mut().niri_width_source.update(&state.windows,&event)?;
                }
                let refresh=event.get("WorkspacesChanged").is_some()||event.get("ConfigLoaded").is_some();
                let mut outputs_changed=false;
                if refresh {let updated=outputs(&control).await?;outputs_changed=state.outputs!=updated;state.outputs=updated;}
                if state.ready.contains("window") && state.ready.contains("workspace") && (normalized["type"]!="other" || keyboard_changed || refresh) {
                    if normalized["type"]!="other" {
                        let mut next=vec![normalized.clone()];
                        if let Some(changes)=normalized.get("changes").and_then(Value::as_array) {
                            for kind in changes.iter().skip(1) {
                                next.push(json!({"type":kind,"windowId":normalized["windowId"]}));
                            }
                        }
                        if outputs_changed {next.push(json!({"type":"outputs-changed"}));}
                        for event in next {
                            if !queue_event(&mut events,event.clone())? {
                                flush_niri(&hub,&state,&keyboard,&ready,&connection_id,&socket,&mut events)?;
                                last=tokio::time::Instant::now();
                                if !queue_event(&mut events,event)? {return Err("Compositor event batch exceeded its bound".into());}
                            }
                        }
                    }
                    let title_only=title_event(&normalized);
                    schedule(&mut due,last,title_only);
                }
            },
            _=wait_due(due)=>{
                flush_niri(&hub,&state,&keyboard,&ready,&connection_id,&socket,&mut events)?;
                last=tokio::time::Instant::now();due=None;
            },
        }
    }
}
fn devices(backend: &mut hyprland_runtime::Backend) -> Result<Value> {
    use crate::compositor::hyprland::Transport;
    let reply = backend
        .actions
        .transport
        .run(vec!["devices".into(), "-j".into()], 10.0, true)?;
    Ok(serde_json::from_str(
        reply["stdout"]
            .as_str()
            .ok_or("Missing compositor input reply")?,
    )?)
}
fn hypr_keyboard(devices: &Value) -> Value {
    let mut keyboard = json!({"names":[],"currentIndex":null,"activeName":""});
    if let Some(device) = devices["keyboards"].as_array().and_then(|devices| {
        devices
            .iter()
            .find(|d| d["main"] == true)
            .or_else(|| devices.first())
    }) {
        keyboard["names"] = json!(device["layout"]
            .as_str()
            .unwrap_or("")
            .split(',')
            .filter(|name| !name.is_empty())
            .collect::<Vec<_>>());
        keyboard["activeName"] = json!(device["active_keymap"].as_str().unwrap_or(""));
    }
    keyboard
}
async fn hyprland(hub: Shared, control: Control, ready: Ready) -> Result<()> {
    let (events, initial, initial_keyboard) = control
        .with(move |backend| -> Result<_> {
            let backend::Backend::Hyprland(backend) = backend else {
                return Err("Expected Hyprland request transport".into());
            };
            let mut events = backend.events()?;
            let initial = events
                .next_value()?
                .ok_or("Missing initial compositor snapshot")?;
            let keyboard = hypr_keyboard(&devices(backend)?);
            Ok((events.into_stream(), initial, keyboard))
        })
        .await?;
    let prefix = events.buffer().to_vec();
    let stream = events.into_inner();
    stream.set_nonblocking(true)?;
    let mut reader = BufReader::new(Cursor::new(prefix).chain(UnixStream::from_std(stream)?));
    let mut partial = Vec::new();
    let mut keyboard = initial_keyboard;
    publish(&hub, &initial, "hyprland", &keyboard, &ready)?;
    let mut due = None;
    let mut last = tokio::time::Instant::now();
    let mut keyboard_changed = false;
    let mut outputs_pending = 0;
    loop {
        tokio::select! {
            bytes=line(&mut reader,&mut partial)=>{
                let bytes=bytes?.ok_or("Compositor event stream ended")?;
                let raw=std::str::from_utf8(&bytes)?.trim_end();let event=hyprland::parse_event(raw);
                if event["type"]=="outputs-changed" {
                    outputs_pending+=1;
                    if outputs_pending>nacre_protocol::MAX_EVENTS {return Err("Compositor output event burst exceeds its limit".into());}
                }
                let layout=raw.starts_with("activelayout>>");
                if event["type"]!="other" || layout {keyboard_changed|=layout;schedule(&mut due,last,event["type"]=="window-title");}
            },
            _=wait_due(due)=>{
                let updated=control.with(move|backend|->Result<_>{
                    let backend::Backend::Hyprland(backend)=backend else {return Err("Expected Hyprland request transport".into());};
                    let snapshot=json!({"type":"snapshot","monitors":backend.actions.query("monitors",10.0,None)?,"workspaces":backend.actions.query("workspaces",10.0,None)?,"windows":backend.actions.query("windows",10.0,None)?,"activeWindow":backend.actions.query("active_window",10.0,None)?});
                    let keyboard=if keyboard_changed{Some(hypr_keyboard(&devices(backend)?))}else{None};
                    Ok((snapshot,keyboard))
                }).await?;
                if let Some(updated)=updated.1 {keyboard=updated;}
                publish(&hub,&updated.0,"hyprland",&keyboard,&ready)?;
                for _ in 0..outputs_pending {hub.borrow_mut().event("compositor","outputs-changed",json!({}))?;}
                outputs_pending=0;
                keyboard_changed=false;last=tokio::time::Instant::now();due=None;
            },
        }
    }
}

pub async fn run(
    hub: Shared,
    control: Control,
    environment: BTreeMap<String, String>,
    ready: Ready,
) -> Result<()> {
    let result = match backend::select(&environment)? {
        "niri" => niri(hub.clone(), control, environment, ready).await,
        _ => hyprland(hub.clone(), control, ready).await,
    };
    if result.is_err() {
        let mut last = hub
            .borrow()
            .snapshot("compositor")
            .unwrap_or_else(|| json!({}));
        last["available"] = json!(false);
        last["error"] = json!("Compositor connection failed");
        if let Err(error) = hub.borrow_mut().publish("compositor", last) {
            tracing::error!(code=?error.code,"Cannot publish compositor disconnect");
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn niri_origins_come_from_the_ledger_for_this_socket_only() {
        let ledger = json!({"socket":"/run/niri.sock","origins":{"niri-workspace:1":["A",2],"niri-workspace:2":["B",1]}});
        let rows = || json!({"workspaces":[{"id":"niri-workspace:1","outputId":"A"},{"id":"niri-workspace:2","outputId":"A"},{"id":"niri-workspace:3","outputId":"A"}]});
        let mut snapshot = rows();
        annotate_origins(&mut snapshot, &ledger, "/run/niri.sock");
        let w = &snapshot["workspaces"];
        assert_eq!(
            (
                &w[0]["visiting"],
                &w[0]["originOutput"],
                &w[0]["originIndex"]
            ),
            (&json!(false), &json!("A"), &json!(2))
        );
        assert_eq!(
            (&w[1]["visiting"], &w[1]["originOutput"]),
            (&json!(true), &json!("B"))
        );
        assert!(w[2].get("visiting").is_none());
        let mut other = rows();
        annotate_origins(&mut other, &ledger, "/run/other.sock");
        assert_eq!(other, rows());
        let projected = project(
            &{
                let mut s = snapshot.clone();
                s["monitors"] = json!([{"name":"A"}]);
                s["windows"] = json!([]);
                s
            },
            "niri",
            &json!({}),
        )
        .unwrap();
        assert_eq!(
            projected["workspaces"]["niri-workspace:2"]["originOutput"],
            "B"
        );
    }
    #[test]
    fn semantic_batches_bound_bytes_and_count_and_coalesce_only_titles() -> Result<()> {
        let mut events = Vec::new();
        for version in 0..300 {
            assert!(queue_event(
                &mut events,
                json!({"type":"window-title","windowId":"w1","version":version})
            )?);
        }
        assert_eq!(events.len(), 1);
        assert_eq!(events[0]["version"], 299);
        for id in 0..255 {
            assert!(queue_event(
                &mut events,
                json!({"type":"window-close","windowId":id})
            )?);
        }
        assert_eq!(events.len(), 256);
        assert!(!queue_event(
            &mut events,
            json!({"type":"window-open","windowId":"next"})
        )?);
        assert!(queue_event(
            &mut events,
            json!({"type":"configuration-reloaded","failed":"x".repeat(1024)})
        )
        .is_err());
        Ok(())
    }
    fn consistent(data: &Value) {
        for row in data["workspaces"].as_object().unwrap().values() {
            if let Some(output) = id(&row["outputId"]) {
                assert!(data["outputs"].get(output).is_some());
            }
            if let Some(window) = id(&row["activeWindowId"]) {
                assert!(data["windows"].get(window).is_some());
            }
        }
        for row in data["windows"].as_object().unwrap().values() {
            if let Some(workspace) = id(&row["workspaceId"]) {
                assert!(data["workspaces"].get(workspace).is_some());
            }
            if let Some(output) = id(&row["outputId"]) {
                assert!(data["outputs"].get(output).is_some());
            }
        }
        for (field, collection) in [
            ("windowId", "windows"),
            ("workspaceId", "workspaces"),
            ("outputId", "outputs"),
        ] {
            if let Some(key) = id(&data["focus"][field]) {
                assert!(data[collection].get(key).is_some());
            }
        }
    }
    #[test]
    fn all_frozen_niri_snapshots_project_without_inventing_or_dangling_entities() -> Result<()> {
        let reference: Value = serde_json::from_str(include_str!(
            "../../tests/fixtures/niri-state-reference.json"
        ))?;
        let mut count = 0;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            for observation in case["observations"]
                .as_array()
                .ok_or("Missing observations")?
            {
                let snapshot = &observation["snapshot"];
                let data = project(
                    snapshot,
                    "niri",
                    &json!({"names":[],"currentIndex":null,"activeName":""}),
                )?;
                consistent(&data);
                for original in snapshot["windows"].as_array().ok_or("Missing windows")? {
                    if let Some(window) =
                        data["windows"].get(original["windowId"].as_str().ok_or("Missing ID")?)
                    {
                        for field in [
                            "title",
                            "appId",
                            "pid",
                            "geometry",
                            "geometryKnown",
                            "floating",
                            "urgent",
                        ] {
                            assert_eq!(window[field], original[field], "{field}");
                        }
                    }
                }
                count += 1;
            }
        }
        assert!(count > 20);
        Ok(())
    }
    #[test]
    fn hyprland_ids_rotation_and_workspace_references_share_the_same_contract() -> Result<()> {
        let data = project(
            &json!({"monitors":[{"id":7,"name":"output-A","width":1920,"height":1080,"scale":2,"transform":1,"focused":true}],"workspaces":[{"id":1,"monitor":"output-A"}],"windows":[{"id":"0x1","windowId":"0x1","workspaceId":1,"outputId":7,"monitor":7,"title":"Example"},{"id":"0x2","windowId":"0x2","workspaceId":99,"outputId":7}],"activeWindow":{"windowId":"0x1"}}),
            "hyprland",
            &json!({}),
        )?;
        consistent(&data);
        assert_eq!(data["outputs"]["output-A"]["logicalWidth"], 540.0);
        assert_eq!(data["outputs"]["output-A"]["logicalHeight"], 960.0);
        assert_eq!(data["windows"]["0x1"]["workspaceId"], "1");
        assert_eq!(data["windows"]["0x1"]["monitor"], "output-A");
        assert_eq!(data["windows"]["0x1"]["workspace"]["id"], "1");
        assert!(data["windows"].get("0x2").is_none());
        Ok(())
    }
    #[test]
    fn keyboard_events_are_validated_and_a_non_title_update_is_not_delayed() -> Result<()> {
        let mut keyboard = json!({"names":[],"currentIndex":null,"activeName":""});
        niri_keyboard(
            &json!({"KeyboardLayoutsChanged":{"keyboard_layouts":{"names":["One","Two"],"current_idx":0}}}),
            &mut keyboard,
        )?;
        niri_keyboard(&json!({"KeyboardLayoutSwitched":{"idx":1}}), &mut keyboard)?;
        assert_eq!(keyboard["activeName"], "Two");
        assert!(niri_keyboard(
            &json!({"KeyboardLayoutSwitched":{"idx":"invalid"}}),
            &mut keyboard
        )
        .is_err());
        let last = tokio::time::Instant::now();
        let mut due = None;
        schedule(&mut due, last, true);
        assert!(due.unwrap() >= last + Duration::from_millis(100));
        schedule(&mut due, last, false);
        assert!(due.unwrap() < last + Duration::from_millis(100));
        Ok(())
    }
}
