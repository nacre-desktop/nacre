//! Existing saved-chat and terminal handoff boundaries; no desktop-adapter port.
use super::*;

pub(super) fn handle(command: &Value, shared: &Arc<Mutex<Shared>>) {
    let result = match command["action"].as_str() {
        Some("load") => load(command, shared),
        Some("workspace") => workspace(shared),
        _ => Ok(()),
    };
    if result.is_err() {
        if let Ok(mut state) = shared.lock() {
            state.error = "Could not complete that chat action.".into();
            state.update();
        }
    }
}

fn records_path() -> Result<PathBuf> {
    Ok(home()?.join(".local/state/nacre/shell/extras/chats.json"))
}

fn records(path: &Path) -> Result<Value> {
    if path.exists() {
        // Check links/size before asking the state layer to decode its version.
        read_private(path)?;
    }
    let value = crate::state::read_as(path, Some(crate::state::by_id("chat-list")?), json!({}))?;
    if !value.is_object() {
        return Err("Invalid saved chat records".into());
    }
    Ok(value)
}

fn loaded_metadata(item: &Value) -> Result<Value> {
    let agent = item["agent"].as_str().unwrap_or("codex");
    if !matches!(agent, "codex" | "claude") {
        return Err("Invalid saved chat provider".into());
    }
    let id = item["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or("Missing chat ID")?;
    let title = item["title"].as_str().ok_or("Missing chat title")?;
    let cwd = item["cwd"]
        .as_str()
        .filter(|cwd| !cwd.is_empty())
        .or_else(|| item["project"]["path"].as_str())
        .ok_or("Missing chat directory")?;
    if !Path::new(cwd).is_absolute() {
        return Err("Chat directory must be absolute".into());
    }
    let account = item.get("account").and_then(Value::as_str).unwrap_or("");
    // Account validation uses the shared core without loading credentials.
    ai_core::effective_account(agent, Some(account))?;
    Ok(json!({"agent":agent,"account":account,"id":id,"cwd":cwd,"title":title,"started":true}))
}

fn load(command: &Value, shared: &Arc<Mutex<Shared>>) -> Result<()> {
    let key = command["key"].as_str().ok_or("Missing saved chat key")?;
    let records = records(&records_path()?)?;
    let meta = loaded_metadata(records.get(key).ok_or("Saved chat no longer exists")?)?;
    let old_peer = {
        let mut state = shared.lock().map_err(|_| "Chat state lock poisoned")?;
        if state.busy {
            return Ok(());
        }
        state.generation = state.generation.wrapping_add(1);
        state.meta["in_workspace"] = json!(true);
        state.peer.take()
    };
    // Exit the app-server before another provider can own this conversation.
    if let Some(peer) = old_peer {
        peer.close();
    }
    {
        let mut state = shared.lock().map_err(|_| "Chat state lock poisoned")?;
        let root = state
            .state
            .parent()
            .ok_or("Missing channel state directory")?
            .to_path_buf();
        state.state = state_path_for(&root, &meta);
        state.meta = meta;
        state.messages.clear();
        state.pending.clear();
        state.pending_rpc = None;
        state.question = None;
        state.async_questions.clear();
        state.provider_pid = None;
        state.status.clear();
        state.error.clear();
        state.save()?;
        state.history();
        state.update();
    }
    let shared = Arc::clone(shared);
    let generation = shared
        .lock()
        .map_err(|_| "Chat state lock poisoned")?
        .generation;
    thread::spawn(move || {
        // Claude's CLI restores through its existing SDK boundary at send time.
        let codex = shared
            .lock()
            .is_ok_and(|state| state.meta["agent"] == "codex");
        if codex {
            if let Err(error) = ensure_peer(&shared) {
                if let Ok(mut state) = shared.lock() {
                    if state.generation == generation {
                        state.finish(&friendly_error(&error));
                    }
                }
            }
        } else {
            let _ = restore_claude_history(&shared);
        }
    });
    Ok(())
}

fn workspace(shared: &Arc<Mutex<Shared>>) -> Result<()> {
    let path = records_path()?;
    let (meta, generation, connect) = {
        let state = shared.lock().map_err(|_| "Chat state lock poisoned")?;
        if state.busy || state.meta["id"].as_str().is_none_or(str::is_empty) {
            return Ok(());
        }
        (
            state.meta.clone(),
            state.generation,
            state.connect_lock.clone(),
        )
    };
    let key = Uuid::new_v4().simple().to_string()[..24].to_owned();
    private_dir(path.parent().ok_or("Missing saved chat directory")?)?;
    records(&path)?;
    let schema = crate::state::by_id("chat-list")?;
    crate::state::update_as(&path, schema, |mut records| {
        if !records.is_object() {
            return Err("Invalid saved chat records".into());
        }
        records[&key] = json!({"id":meta["id"],"title":meta["title"],"agent":meta["agent"],"account":meta["account"],"cwd":meta["cwd"],"state":"saved","project":{"id":"general-chat","path":meta["cwd"],"chat_directory":true}});
        Ok(records)
    })?;
    let peer = {
        let mut state = shared.lock().map_err(|_| "Chat state lock poisoned")?;
        if state.generation != generation || state.busy {
            return Err("Chat changed during handoff".into());
        }
        state.generation = state.generation.wrapping_add(1);
        // Fence background reconnects before releasing the manager lock to
        // wait for the old peer. The terminal launches only after it exits.
        state.meta["in_workspace"] = json!(true);
        state.peer.take()
    };
    if let Some(peer) = peer {
        peer.close();
    }
    // A resume handshake may still own a child that has not reached peer
    // publication. Its generation check closes that child before releasing
    // this lock, so the terminal cannot race it for the same conversation.
    let _connection = connect.lock().map_err(|_| "Chat connect lock poisoned")?;
    {
        let mut state = shared.lock().map_err(|_| "Chat state lock poisoned")?;
        state.provider_pid = None;
        state.meta["in_workspace"] = json!(true);
        state.save()?;
        state.status = "Continue this chat in the workspace, or start a new chat here.".into();
        state.update();
    }
    let helper = match std::env::var_os("NACRE_HELPER_BINARY") {
        Some(path) => std::path::PathBuf::from(path),
        None => std::env::current_exe()?,
    };
    let mut child = Command::new(helper)
        .args(["desktop-extras", "resume"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    if let Some(mut input) = child.stdin.take() {
        input.write_all(serde_json::to_string(&json!({"key":key}))?.as_bytes())?;
    }
    use wait_timeout::ChildExt;
    match child.wait_timeout(Duration::from_secs(30))? {
        Some(status) if status.success() => {}
        _ => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("Could not open chat in workspace".into());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loaded_chat_keeps_identity_provider_and_directory() {
        let item = json!({"id":"one","title":"Same title","agent":"claude","account":"","project":{"path":"/fixture/one"}});
        let meta = loaded_metadata(&item).unwrap();
        assert_eq!(meta["id"], "one");
        assert_eq!(meta["agent"], "claude");
        assert_eq!(meta["cwd"], "/fixture/one");
        assert_eq!(meta["started"], true);
        assert!(
            loaded_metadata(&json!({"id":"two","title":"Same title","cwd":"relative"})).is_err()
        );
        assert!(loaded_metadata(
            &json!({"id":"two","title":"Same title","cwd":"/fixture","account":"../other"})
        )
        .is_err());
    }
}
