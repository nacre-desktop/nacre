//! Dashboard extras: the daemon-owned focus timer, clipboard, screenshots, AI usage
//! and Brain recent topics plus their methods (N0034). Every producer keeps its last
//! good state with `stale` health, never polls while idle, and shares one heavy-work
//! slot. Previews and paths are private: nothing here logs them.
mod brain;
mod clipboard;
mod focus;
mod screenshots;
mod usage;
mod watch;

use super::hub::{failure, Shared};
use crate::{paths::Paths, Result};
use nacre_protocol::{
    core_services::Health,
    dashboard_extras::{
        AiUsageState, BrainRecentState, ClipboardState, DashboardExtrasCall, ScreenshotsState,
    },
    Capability, ErrorCode, Failure, Method,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::{collections::BTreeMap, path::PathBuf, rc::Rc, time::Duration};

pub type Gate = Rc<tokio::sync::Semaphore>;
const QUIET: Duration = Duration::from_millis(300);

pub fn ok() -> Health {
    Health {
        available: true,
        stale: false,
        error: None,
    }
}
/// `message: None` is "not read yet", not a failure.
pub fn down(stale: bool, message: Option<&str>) -> Health {
    Health {
        available: false,
        stale,
        error: message.map(|m| Failure::new(ErrorCode::Unavailable, m, true)),
    }
}

trait Checked: Serialize {
    fn check(&self) -> std::result::Result<(), Failure>;
}
macro_rules! checked {
    ($($state:ty),*) => {$(impl Checked for $state {
        fn check(&self) -> std::result::Result<(), Failure> { self.validate() }
    })*};
}
checked!(
    AiUsageState,
    BrainRecentState,
    ClipboardState,
    ScreenshotsState
);

fn publish<T: Checked>(hub: &Shared, topic: &str, state: &T) -> std::result::Result<(), Failure> {
    state.check()?;
    hub.borrow_mut().publish(
        topic,
        serde_json::to_value(state)
            .map_err(|_| failure(ErrorCode::Internal, "Dashboard encoding failed", false))?,
    )
}

/// Wait for a change, then for `quiet` without further changes.
async fn settle(watch: &mut watch::Watch, quiet: Duration) -> Result<()> {
    watch.next().await?;
    while let Ok(next) = tokio::time::timeout(quiet, watch.next()).await {
        next?;
    }
    Ok(())
}

#[derive(Clone)]
pub struct Requests {
    paths: Paths,
    focus: Rc<focus::Focus>,
    gate: Gate,
    /// The helper that answers `nacre-ai-usage read`.
    helper: PathBuf,
}
impl Requests {
    pub fn new(paths: Paths) -> Self {
        Self {
            focus: focus::Focus::new(paths.clone()),
            paths,
            gate: Rc::new(tokio::sync::Semaphore::new(1)),
            helper: std::env::var_os("NACRE_HELPER_BINARY")
                .map_or_else(|| PathBuf::from("/proc/self/exe"), PathBuf::from),
        }
    }
}

/// Publish the empty states so every topic is subscribable before its first read.
pub fn initial(hub: &Shared, requests: &Requests) -> std::result::Result<(), Failure> {
    requests.focus.publish(hub)?;
    publish(hub, clipboard::TOPIC, &clipboard::initial())?;
    publish(hub, screenshots::TOPIC, &screenshots::initial())?;
    publish(hub, usage::TOPIC, &usage::initial())?;
    publish(hub, brain::TOPIC, &brain::initial())
}

pub fn handles(method: &str) -> bool {
    matches!(
        method,
        "focus.start"
            | "focus.pause"
            | "focus.resume"
            | "focus.stop"
            | "clipboard.copy"
            | "clipboard.clear"
            | "clipboard.clear_history"
            | "screenshots.open"
            | "ai.usage.refresh"
    )
}

pub fn methods() -> BTreeMap<String, Method> {
    let schema =
        |text: &str| serde_json::from_str::<Value>(text).expect("checked-in dashboard schema");
    let result = schema(include_str!(
        "../../../../protocol/schema/core-service-result.json"
    ));
    let none = json!({"type":"object","additionalProperties":false});
    let start = schema(include_str!(
        "../../../../protocol/schema/dashboard-focus-start.json"
    ));
    let target = schema(include_str!(
        "../../../../protocol/schema/dashboard-extras-target.json"
    ));
    [
        ("focus.start", 5000, &start),
        ("focus.pause", 5000, &none),
        ("focus.resume", 5000, &none),
        ("focus.stop", 5000, &none),
        ("clipboard.copy", 15000, &target),
        ("clipboard.clear", 30000, &none),
        ("clipboard.clear_history", 30000, &none),
        ("screenshots.open", 5000, &target),
        ("ai.usage.refresh", 30000, &none),
    ]
    .into_iter()
    .map(|(name, timeout, params)| {
        (
            name.into(),
            Method {
                v: 1,
                class: Capability::Desktop,
                timeout,
                params: params.clone(),
                result: result.clone(),
            },
        )
    })
    .collect()
}

pub async fn dispatch(
    hub: Shared,
    method: &str,
    params: Value,
) -> std::result::Result<Value, Failure> {
    let invalid = || failure(ErrorCode::Validation, "Invalid dashboard request", false);
    let call: DashboardExtrasCall =
        serde_json::from_value(json!({"m":method,"p":params})).map_err(|_| invalid())?;
    call.validate()?;
    let requests = hub.borrow().dashboard.clone().ok_or_else(|| {
        failure(
            ErrorCode::Unavailable,
            "Dashboard service is unavailable",
            true,
        )
    })?;
    let slot = || async {
        requests.gate.acquire().await.map_err(|_| {
            failure(
                ErrorCode::Unavailable,
                "Dashboard service is unavailable",
                true,
            )
        })
    };
    match call {
        DashboardExtrasCall::ClipboardCopy(target) => {
            let _slot = slot().await?;
            clipboard::copy(&hub, &requests.paths, &target.id).await
        }
        DashboardExtrasCall::ClipboardClear(_) => {
            let _slot = slot().await?;
            clipboard::clear(&hub, &requests.paths).await
        }
        DashboardExtrasCall::ClipboardClearHistory(_) => {
            let _slot = slot().await?;
            clipboard::clear_history(&hub, &requests.paths).await
        }
        DashboardExtrasCall::ScreenshotsOpen(target) => {
            screenshots::open(&hub, &requests.paths, &target.id).await
        }
        DashboardExtrasCall::AiUsageRefresh(_) => {
            let _slot = slot().await?;
            usage::refresh(&hub, &requests.helper).await
        }
        focus => requests.focus.call(&hub, focus).await,
    }
}

pub async fn run(hub: Shared, requests: Requests) -> Result<()> {
    let failures = hub
        .borrow()
        .services
        .get("dashboard-extras")
        .and_then(|v| v["failures"].as_u64())
        .unwrap_or(0) as usize;
    hub.borrow_mut()
        .service("dashboard-extras", "running", failures)?;
    tokio::try_join!(
        requests.focus.clone().run(hub.clone()),
        clipboard::run(hub.clone(), requests.paths.clone(), requests.gate.clone()),
        screenshots::run(hub.clone(), requests.paths.clone(), requests.gate.clone()),
        brain::run(hub.clone(), requests.gate.clone()),
    )?;
    Ok(())
}
