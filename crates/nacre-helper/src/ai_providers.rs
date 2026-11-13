//! Provider adapters for the Nacre AI app (N0319). One trait, one adapter per
//! provider; the daemon's `ai.*` topics are built over `registry()` only, so
//! a local model later is one more `impl Provider` here. Adapters wrap the
//! existing helper paths (history lists, launch, login status) and never read
//! a credential: status is a directory name plus a CLI exit code.
use crate::{ai_core, ai_dashboard, Result};
use nacre_protocol::ai_app::{AccountStatus, Capabilities, ProviderInfo, ProviderKind};
use serde_json::Value;

pub trait Provider: Sync {
    fn info(&self) -> ProviderInfo;
    /// `""` is the provider's own home; other names are Nacre profile directories.
    fn profiles(&self) -> Vec<String>;
    /// The profile selected in the AI settings.
    fn active_profile(&self) -> Result<String>;
    /// By CLI exit code only; the output is discarded.
    fn login_status(&self, account: &str) -> AccountStatus;
    /// History rows for one project source, as the TUI lists them (same
    /// Running detection); `project` is a registry entry.
    fn chats(&self, project: &Value, account: &str) -> Result<Vec<Value>>;
    /// Open a new chat window. `thread_id` is honoured only when
    /// `capabilities.caller_session_id` is true.
    fn start(&self, project: &str, account: Option<&str>, thread_id: Option<&str>) -> Result<()>;
}

/// A terminal coding agent driven through the existing `nacre-ai` paths.
struct Cli {
    id: &'static str,
    label: &'static str,
    caller_session_id: bool,
}
impl Provider for Cli {
    fn info(&self) -> ProviderInfo {
        ProviderInfo {
            id: self.id.into(),
            label: self.label.into(),
            kind: ProviderKind::Cli,
            capabilities: Capabilities {
                chat: true,
                resume: true,
                accounts: true,
                usage: true,
                models: false,
                caller_session_id: self.caller_session_id,
            },
        }
    }
    fn profiles(&self) -> Vec<String> {
        ai_dashboard::account_profiles(self.id)
    }
    fn active_profile(&self) -> Result<String> {
        ai_core::effective_account(self.id, None)
    }
    fn login_status(&self, account: &str) -> AccountStatus {
        match ai_dashboard::login_status(self.id, account) {
            "signed in" => AccountStatus::SignedIn,
            "not signed in" => AccountStatus::SignedOut,
            "not installed" => AccountStatus::Missing,
            _ => AccountStatus::Unknown,
        }
    }
    fn chats(&self, project: &Value, account: &str) -> Result<Vec<Value>> {
        ai_dashboard::provider_list(project, self.id, account)
    }
    fn start(&self, project: &str, account: Option<&str>, thread_id: Option<&str>) -> Result<()> {
        ai_dashboard::start_chat(
            self.id,
            Some(project),
            account,
            thread_id.filter(|_| self.caller_session_id),
        )
    }
}

const CODEX: Cli = Cli {
    id: "codex",
    label: "Codex",
    // `codex` has no caller-chosen thread id (checked against `codex --help`, 2026-11-13).
    caller_session_id: false,
};
const CLAUDE: Cli = Cli {
    id: "claude",
    label: "Claude Code",
    // `claude --session-id <uuid>`, passed as `nacre-ai-claude new --thread-id`.
    caller_session_id: true,
};

/// Every provider the app can show, in display order.
pub fn registry() -> &'static [&'static dyn Provider] {
    &[&CODEX, &CLAUDE]
}
pub fn find(id: &str) -> Option<&'static dyn Provider> {
    registry().iter().copied().find(|p| p.info().id == id)
}
