//! AI usage on request. Codex quota is a provider connection, so nothing reads it
//! until `ai.usage.refresh`; Claude rows come from the local cache. Rows are the
//! existing `nacre-ai-usage read` report, run as a bounded child of this helper.
use super::{down, ok, publish};
use crate::daemon::hub::{failure, Shared};
use nacre_protocol::{
    core_services::{Entities, EntityId},
    dashboard_extras::{AiProvider, AiUsageState, MAX_USAGE_LINES},
    ErrorCode, Failure,
};
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::Path, time::Duration};
use tokio::io::AsyncReadExt;

pub const TOPIC: &str = "ai.usage";
const WORK: Duration = Duration::from_secs(12);

pub fn initial() -> AiUsageState {
    AiUsageState {
        health: down(false, None),
        providers: Entities {
            items: BTreeMap::new(),
            order: vec![],
        },
    }
}

async fn rows(helper: &Path, agent: &str) -> Option<Vec<String>> {
    let mut child = tokio::process::Command::new(helper)
        .args(["nacre-ai-usage", "read", "--agent", agent])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .process_group(0)
        .kill_on_drop(true)
        .spawn()
        .ok()?;
    let mut output = Vec::new();
    let mut stdout = child.stdout.take()?.take(64 * 1024);
    let read = tokio::time::timeout(WORK, async {
        stdout.read_to_end(&mut output).await?;
        child.wait().await
    })
    .await;
    if !matches!(read, Ok(Ok(status)) if status.success()) {
        return None;
    }
    let value: Value = serde_json::from_slice(&output).ok()?;
    // The first row is the "<provider> · <account>" header the dashboard omits.
    value["rows"].as_array().map(|rows| {
        rows.iter()
            .skip(1)
            .filter_map(Value::as_str)
            .map(|row| row.chars().filter(|c| !c.is_control()).take(256).collect())
            .take(MAX_USAGE_LINES)
            .collect()
    })
}

pub async fn refresh(hub: &Shared, helper: &Path) -> std::result::Result<Value, Failure> {
    let previous = hub
        .borrow()
        .snapshot(TOPIC)
        .and_then(|v| serde_json::from_value::<AiUsageState>(v).ok())
        .unwrap_or_else(initial);
    let mut state = AiUsageState {
        health: ok(),
        providers: previous.providers,
    };
    let now = chrono::Utc::now().timestamp_millis().max(0) as u64;
    let mut failed = false;
    for (key, name, agent) in [
        ("codex", "Codex", "codex"),
        ("claude", "Claude Code", "claude"),
    ] {
        let id = EntityId(key.into());
        match rows(helper, agent).await {
            Some(lines) => {
                if !state.providers.items.contains_key(&id) {
                    state.providers.order.push(id.clone());
                }
                state.providers.items.insert(
                    id,
                    AiProvider {
                        name: name.into(),
                        lines,
                        observed_ms: now,
                    },
                );
            }
            None => failed = true,
        }
    }
    if failed {
        state.health = down(
            !state.providers.items.is_empty(),
            Some("AI usage is unavailable"),
        );
    }
    publish(hub, TOPIC, &state)?;
    if failed && state.providers.items.is_empty() {
        return Err(failure(
            ErrorCode::Unavailable,
            "AI usage is unavailable",
            true,
        ));
    }
    Ok(json!({"changed":false}))
}
