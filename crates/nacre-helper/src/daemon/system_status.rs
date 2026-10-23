//! N0051: `system.status`, the read-only System page status. It is what the QML Maintenance page
//! showed (`maintenance state`, the same collector) plus the update-cache age it already carries
//! and the AUR review policy provenance (`aur-review policy`). Nothing is changed and nothing the
//! collector reports is relayed unbounded: every string is cleaned and cut, every list capped.
use super::hub::{failure, Hub, Shared};
use crate::{aur_review, diagnostics_native, paths::Paths};
use nacre_protocol::{
    pill_controls::PillEmpty,
    settings_asks::{AurProvenance, ResourceSample, SystemStatus},
    ErrorCode, Failure, Method,
};
use serde_json::Value;
use std::{collections::BTreeMap, time::Duration};

const LIMIT: Duration = Duration::from_secs(25);

fn clean(text: &str, max: usize) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(max)
        .collect::<String>()
        .trim()
        .to_owned()
}
fn strings(value: &Value, cap: usize, max: usize) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .take(cap)
        .map(|s| clean(s, max))
        .collect()
}
fn some(text: Option<&str>, max: usize) -> Option<String> {
    text.map(|s| clean(s, max)).filter(|s| !s.is_empty())
}
/// The projection of `maintenance state` and `aur-review policy`; pure so it is tested on canned
/// documents.
pub fn project(state: &Value, policy: Option<&Value>) -> SystemStatus {
    let number = |value: &Value| value.as_f64().filter(|n| n.is_finite()).map(|n| n.max(0.0));
    SystemStatus {
        checked: clean(state["checked"].as_str().unwrap_or(""), 64),
        release: some(state["release"]["revision"].as_str(), 64),
        services: state["services"]
            .as_object()
            .into_iter()
            .flatten()
            .filter_map(|(name, value)| Some((clean(name, 96), clean(value.as_str()?, 48))))
            .take(32)
            .collect(),
        failed_services: strings(&state["failedServices"], 64, 256),
        config_errors: clean(state["configErrors"].as_str().unwrap_or(""), 2048),
        drift: strings(&state["drift"], 256, 512),
        brain_sync: some(
            state["brainSync"]["message"]
                .as_str()
                .or(state["brainSync"]["state"].as_str()),
            256,
        ),
        performance: number(&state["performance"]["cpuCorePercent"]).map(|cpu| ResourceSample {
            label: some(state["performance"]["label"].as_str(), 64),
            cpu_core_percent: cpu,
            memory_mib: number(&state["performance"]["memoryMiB"]).unwrap_or(0.0),
        }),
        session_event: some(state["session"]["event"].as_str(), 128),
        updates_age_seconds: number(&state["updatesAgeSeconds"]).map(|age| age as u64),
        aur_review: policy.and_then(|policy| {
            Some(AurProvenance {
                mode: clean(policy["mode"].as_str()?, 32),
                source: clean(policy["provenance"]["source"].as_str()?, 64),
                profile: clean(policy["provenance"]["profile"].as_str().unwrap_or(""), 64),
            })
        }),
    }
}

pub fn handles(method: &str) -> bool {
    method == "system.status"
}
pub fn available_methods(hub: &Hub) -> BTreeMap<String, Method> {
    let mut methods = nacre_protocol::settings_asks::methods();
    methods.retain(|name, _| handles(name));
    if !super::capabilities::pages_up(hub) {
        methods.clear();
    }
    methods
}
pub async fn dispatch(hub: Shared, params: Value) -> Result<Value, Failure> {
    serde_json::from_value::<PillEmpty>(params).map_err(|_| {
        failure(ErrorCode::Validation, "Invalid status request", false).dispatch_state(false)
    })?;
    if hub.borrow().safe {
        return Err(failure(
            ErrorCode::PermissionDenied,
            "System status is disabled in safe mode",
            false,
        )
        .dispatch_state(false));
    }
    let unavailable = || {
        failure(ErrorCode::Unavailable, "System status is unavailable", true).dispatch_state(false)
    };
    let work = tokio::task::spawn_blocking(|| {
        let state = diagnostics_native::maintenance_state().ok()?;
        let policy = Paths::environment()
            .ok()
            .and_then(|paths| aur_review::policy(&paths).ok());
        Some(project(&state, policy.as_ref()))
    });
    let status = tokio::time::timeout(LIMIT, work)
        .await
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?
        .ok_or_else(unavailable)?;
    status
        .validate()
        .map_err(|_| failure(ErrorCode::Internal, "System status was malformed", false))?;
    serde_json::to_value(&status)
        .map_err(|_| failure(ErrorCode::Internal, "System status was malformed", false))
}

#[cfg(test)]
mod tests;
