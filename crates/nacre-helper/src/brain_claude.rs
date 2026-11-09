//! Shared bounded `claude -p` invocation for Brain features.
//! Temporary duplicate of the curate invocation; delete 7 days after a
//! shared helper replaces both implementations.
use crate::{process, Result};
use std::{process::Command, time::Duration};

pub(crate) fn call(prompt: &str, input: &str, timeout: Duration) -> Result<String> {
    let scratch = tempfile::tempdir()?;
    let mut command = Command::new("claude");
    command
        .args([
            "-p",
            "--model",
            "sonnet",
            "--output-format",
            "json",
            "--tools",
            "",
            "--no-session-persistence",
            "--disable-slash-commands",
            "--strict-mcp-config",
            "--safe-mode",
            "--system-prompt",
            prompt,
        ])
        .current_dir(scratch.path())
        .env_remove("ANTHROPIC_API_KEY");
    let output = process::capture(command, Some(input.as_bytes()), timeout).map_err(|e| {
        let text = e.to_string().to_ascii_lowercase();
        if text.contains("timed out") {
            ClaudeError::new("timeout", "Claude timed out; try again")
        } else if text.contains("no such file") || text.contains("not found") {
            ClaudeError::new(
                "missing_cli",
                "Claude CLI is unavailable; install Claude Code and sign in",
            )
        } else {
            ClaudeError::new("claude", "Claude could not run; check the local Claude CLI")
        }
    })?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let envelope: serde_json::Value = match serde_json::from_str(stdout.trim()) {
        Ok(value) => value,
        Err(_) => {
            if !output.status.success() {
                return Err(failure(&format!(
                    "{stdout} {}",
                    String::from_utf8_lossy(&output.stderr)
                ))
                .into());
            }
            return Err(ClaudeError::new("claude", "Claude returned an invalid response").into());
        }
    };
    if !output.status.success() || envelope["is_error"] == true {
        let detail = format!(
            "{} {}",
            envelope["result"].as_str().unwrap_or(""),
            String::from_utf8_lossy(&output.stderr)
        );
        return Err(failure(&detail).into());
    }
    envelope["result"]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ClaudeError::new("claude", "Claude response has no result").into())
}

fn failure(detail: &str) -> ClaudeError {
    let lower = detail.to_ascii_lowercase();
    if [
        "login",
        "log in",
        "sign in",
        "auth",
        "oauth",
        "invalid api key",
        "expired token",
        "401",
        "403",
    ]
    .iter()
    .any(|word| lower.contains(word))
    {
        ClaudeError::new("auth", "Claude sign-in needs attention")
    } else if ["quota", "limit", "rate", "overload", "credit", "429", "529"]
        .iter()
        .any(|word| lower.contains(word))
    {
        ClaudeError::new("usage", "Claude subscription usage limit needs attention")
    } else {
        ClaudeError::new("claude", "Claude failed; check the local Claude CLI")
    }
}

#[derive(Debug)]
pub(crate) struct ClaudeError {
    pub(crate) code: &'static str,
    message: String,
}
impl ClaudeError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
impl std::fmt::Display for ClaudeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}
impl std::error::Error for ClaudeError {}
