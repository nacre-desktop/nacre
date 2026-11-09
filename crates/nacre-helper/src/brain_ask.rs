//! Notes-only Brain questions (N0149).
use crate::{brain_claude, brain_registry, brain_search, brain_web, Result};
use serde::Serialize;
use serde_json::json;
use std::{
    collections::BTreeSet,
    fs,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::{Path, PathBuf},
    sync::{Mutex, TryLockError},
    time::Duration,
};

const PROMPT: &str = include_str!("../../../ai/skills/nacre-brain-ask/SKILL.md");
const MAX_BYTES: usize = 56_000;
static BUSY: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Serialize)]
pub struct Answer {
    pub answer: String,
    pub citations: Vec<String>,
    pub confidence: String,
    pub followups: Vec<String>,
}

#[derive(Debug)]
pub struct AskError {
    pub status: u16,
    pub code: &'static str,
    pub message: String,
}
impl std::fmt::Display for AskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self.code {
            "disabled" => crate::i18n::tr("brain.ui.error.ask-disabled", "Ask is disabled."),
            "busy" => crate::i18n::tr(
                "brain.ui.error.ask-busy",
                "Another question is being answered. Try again shortly.",
            ),
            "auth" => crate::i18n::tr(
                "brain.ui.error.ask-auth",
                "Claude is not signed in. Sign in with the Claude CLI and try again.",
            ),
            "usage" => crate::i18n::tr(
                "brain.ui.error.ask-usage",
                "Claude could not process this request. Check your subscription usage.",
            ),
            "timeout" => crate::i18n::tr(
                "brain.ui.error.ask-timeout",
                "The answer took too long. Try again.",
            ),
            "missing_cli" => crate::i18n::tr(
                "brain.ui.error.ask-cli",
                "Claude CLI is unavailable. Install Claude Code and sign in.",
            ),
            "question" => crate::i18n::tr(
                "brain.ui.error.ask-question",
                "Enter a question of 1–1000 characters.",
            ),
            "unsafe_input" => crate::i18n::tr(
                "brain.ui.error.ask-unsafe-input",
                "The question or matching notes may contain a secret. Nothing was sent.",
            ),
            "invalid_citation" => crate::i18n::tr(
                "brain.ui.error.ask-invalid-citation",
                "Claude cited a source outside the selected notes. The answer was refused.",
            ),
            "unsafe_reply" => crate::i18n::tr(
                "brain.ui.error.ask-unsafe-reply",
                "The answer failed the safety checks and was refused.",
            ),
            _ => crate::i18n::tr(
                "brain.ui.error.ask-failed",
                "Could not answer that question.",
            ),
        };
        f.write_str(&message)
    }
}
impl std::error::Error for AskError {}
impl AskError {
    fn new(status: u16, code: &'static str, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
        }
    }
}

fn config_path() -> Result<PathBuf> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .ok_or("HOME is not set")?;
    Ok(config.join("nacre/brain.json"))
}
pub fn enabled() -> bool {
    let Ok(path) = config_path() else {
        return false;
    };
    brain_web::read(&path, 64 * 1024)
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .is_some_and(|value| {
            value
                .pointer("/brain/ask/enabled")
                .and_then(|v| v.as_bool())
                == Some(true)
        })
}
pub fn set_enabled(value: bool) -> Result<()> {
    let path = config_path()?;
    brain_web::private_dir(path.parent().ok_or("Invalid settings path")?)?;
    brain_web::reject_links(&path)?;
    let mut data = if path.exists() {
        serde_json::from_str::<serde_json::Value>(&brain_web::read(&path, 64 * 1024)?)?
    } else {
        json!({})
    };
    if !data.is_object() {
        return Err("Nacre Brain settings must be a JSON object".into());
    }
    if data.get("brain").is_some_and(|value| !value.is_object()) {
        return Err("Nacre Brain settings must contain an object at brain".into());
    }
    if data
        .pointer("/brain/ask")
        .is_some_and(|value| !value.is_object())
    {
        return Err("Nacre Brain settings must contain an object at brain.ask".into());
    }
    data["brain"]["ask"]["enabled"] = json!(value);
    crate::classic_state::atomic_write(&path, &(serde_json::to_string_pretty(&data)? + "\n"))?;
    Ok(())
}

pub fn ask(root: &Path, question: &str, yes_once: bool) -> std::result::Result<Answer, AskError> {
    if !yes_once && !enabled() {
        return Err(AskError::new(403, "disabled", "Ask the Brain is disabled"));
    }
    let _guard = match BUSY.try_lock() {
        Ok(guard) => guard,
        Err(TryLockError::WouldBlock) => {
            return Err(AskError::new(
                429,
                "busy",
                "Another Brain question is being answered",
            ))
        }
        Err(_) => {
            return Err(AskError::new(
                503,
                "busy",
                "Brain question service is unavailable",
            ))
        }
    };
    let lock_path = config_path()
        .map(|p| p.with_file_name("brain-ask.lock"))
        .map_err(|e| AskError::new(500, "settings", e.to_string()))?;
    let parent = lock_path
        .parent()
        .ok_or_else(|| AskError::new(500, "settings", "Invalid settings path"))?;
    brain_web::private_dir(parent)
        .map_err(|_| AskError::new(500, "settings", "Could not access Nacre settings"))?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(nix::libc::O_NOFOLLOW | nix::libc::O_NONBLOCK)
        .open(lock_path)
        .map_err(|_| AskError::new(500, "busy", "Could not acquire Brain question lock"))?;
    let meta = lock
        .metadata()
        .map_err(|_| AskError::new(500, "busy", "Could not inspect Brain question lock"))?;
    if !meta.is_file() || meta.nlink() != 1 {
        return Err(AskError::new(500, "busy", "Unsafe Brain question lock"));
    }
    rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive)
        .map_err(|_| AskError::new(429, "busy", "Another Brain question is being answered"))?;
    let question = question.trim();
    if question.is_empty() || question.chars().count() > 1000 {
        return Err(AskError::new(
            400,
            "question",
            "Question must contain 1–1000 characters",
        ));
    }
    let evidence =
        collect(root, question).map_err(|e| AskError::new(500, "evidence", e.to_string()))?;
    let allowed = evidence.paths.clone();
    if evidence.text.trim().is_empty() {
        return Ok(Answer {
            answer: "That is not in your notes.".into(),
            citations: Vec::new(),
            confidence: "not_found".into(),
            followups: Vec::new(),
        });
    }
    let nonce = uuid::Uuid::new_v4().simple().to_string();
    let input = format!(
        "Question: {question}\n\n<notes-data id=\"{nonce}\">\n{}\n</notes-data>",
        evidence.text.replace("</notes-data", "<\\/notes-data")
    );
    if crate::brain_git::secret_rule(&input).is_some() {
        return Err(AskError::new(
            400,
            "unsafe_input",
            "Question or selected notes contain a possible secret",
        ));
    }
    let prompt = PROMPT
        .strip_prefix("---\n")
        .and_then(|s| s.split_once("\n---\n").map(|(_, rest)| rest))
        .unwrap_or(PROMPT);
    let raw = brain_claude::call(prompt, &input, Duration::from_secs(55)).map_err(|error| {
        let code = error
            .downcast_ref::<brain_claude::ClaudeError>()
            .map(|e| e.code)
            .unwrap_or("claude");
        let message = if code == "auth" {
            "Claude sign-in needs attention"
        } else if code == "usage" {
            "Claude subscription usage limit needs attention"
        } else if code == "missing_cli" {
            "Claude CLI is unavailable; install Claude Code and sign in"
        } else if code == "timeout" {
            "Claude took too long to answer; try again"
        } else {
            "Claude could not answer; check the local Claude CLI"
        };
        AskError::new(502, code, message)
    })?;
    validate(&raw, &allowed)
}

struct Evidence {
    text: String,
    paths: BTreeSet<String>,
}
fn collect(root: &Path, question: &str) -> Result<Evidence> {
    let found = brain_search::search(
        root,
        question,
        &brain_search::Options {
            limit: 12,
            ..Default::default()
        },
    )?;
    let mut paths = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut parts: Vec<(String, String)> = Vec::new();
    let mut scope = BTreeSet::new();
    let registry = brain_web::registry(root)?;
    for row in found["results"].as_array().into_iter().flatten() {
        let path = row["path"].as_str().unwrap_or("");
        if path.is_empty() || path.starts_with("credentials/") || !seen.insert(path.to_owned()) {
            continue;
        }
        let body = brain_web::read(&root.join(path), brain_web::NOTE_LIMIT)?;
        if crate::brain_git::secret_rule(&body).is_some() {
            continue;
        }
        scope.extend(
            row["worlds"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned)),
        );
        scope.extend(
            row["areas"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|v| v.as_str().map(str::to_owned)),
        );
        let (title, header, _) = crate::brain_migrate::header_body(&body);
        parts.push((
            path.to_owned(),
            format!(
                "### [[{path}]]\n{title}\n{header}\n{}",
                row["snippet"].as_str().unwrap_or("")
            ),
        ));
    }
    let mut entries = fs::read_dir(root.join("wiki"))
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let p = entry.path();
        let path = format!("wiki/{}", entry.file_name().to_string_lossy());
        let Ok(body) = brain_web::read(&p, brain_web::NOTE_LIMIT) else {
            continue;
        };
        let meta = brain_web::fields(&body);
        let entity = meta["entity"].as_str().unwrap_or("");
        let name = meta["name"].as_str().unwrap_or("");
        let relevant_scope = matches!(entity, "world" | "area")
            && scope
                .iter()
                .any(|label: &String| name.eq_ignore_ascii_case(label));
        if (!seen.contains(&path) && !relevant_scope)
            || crate::brain_git::secret_rule(&body).is_some()
        {
            continue;
        }
        let facts = current_facts(&body);
        if !facts.trim().is_empty() {
            parts.push((
                path.clone(),
                format!("### [[{path}]] Current facts\n{facts}"),
            ));
        }
    }
    let registry_bytes = serde_json::to_vec(&registry)?;
    for scope_name in scope {
        if let Some((world, area)) = brain_registry::resolve_label(&registry_bytes, &scope_name)? {
            let rel = crate::brain_migrate::consolidate::pack_path(&world, area.as_deref());
            if !rel.starts_with("credentials/") {
                if let Ok(text) = brain_web::read(&root.join(&rel), 16 * 1024) {
                    if crate::brain_git::secret_rule(&text).is_none() {
                        parts.push((
                            rel.clone(),
                            format!("### [[{rel}]] Context pack {scope_name}\n{text}"),
                        ));
                    }
                }
            }
        }
    }
    let mut text = String::new();
    for (path, part) in parts {
        if text.len() >= MAX_BYTES {
            break;
        }
        if text.len() + part.len() + 2 > MAX_BYTES {
            continue;
        }
        text.push_str(&part);
        text.push_str("\n\n");
        paths.insert(path);
    }
    Ok(Evidence { text, paths })
}
fn current_facts(page: &str) -> String {
    let Some((_, rest)) = page.split_once("## Current facts") else {
        return String::new();
    };
    rest.split("\n## ")
        .next()
        .unwrap_or("")
        .lines()
        .take_while(|line| {
            !line.contains("brain:generated") && !line.starts_with("### Open questions")
        })
        .filter(|line| !line.trim_start().starts_with("<!--") && !line.starts_with("- ~~"))
        .collect::<Vec<_>>()
        .join("\n")
}

fn validate(raw: &str, allowed: &BTreeSet<String>) -> std::result::Result<Answer, AskError> {
    if raw.len() > 16_384
        || crate::brain_git::secret_rule(raw).is_some()
        || raw.contains("credentials/")
        || [
            "ignore all previous instructions",
            "</notes-data",
            "<notes-data",
        ]
        .iter()
        .any(|marker| raw.to_ascii_lowercase().contains(marker))
    {
        return Err(AskError::new(
            502,
            "unsafe_reply",
            "Claude returned an unsafe or untrusted response",
        ));
    }
    let value: serde_json::Value = serde_json::from_str(raw.trim())
        .map_err(|_| AskError::new(502, "invalid_reply", "Claude returned an invalid answer"))?;
    let normalized = serde_json::to_string(&value)
        .map_err(|_| AskError::new(502, "invalid_reply", "Claude returned an invalid answer"))?;
    let lower = normalized.to_ascii_lowercase();
    if crate::brain_git::secret_rule(&normalized).is_some()
        || lower.contains("credentials/")
        || [
            "ignore all previous instructions",
            "</notes-data",
            "<notes-data",
        ]
        .iter()
        .any(|marker| lower.contains(marker))
    {
        return Err(AskError::new(
            502,
            "unsafe_reply",
            "Answer contains a possible secret or untrusted instruction",
        ));
    }
    let answer = value["answer"]
        .as_str()
        .ok_or_else(|| AskError::new(502, "invalid_reply", "Answer field is missing"))?
        .trim();
    if answer.is_empty()
        || answer.chars().count() > 1200
        || answer.contains('\n')
        || answer.contains('\r')
    {
        return Err(AskError::new(
            502,
            "invalid_reply",
            "Answer must be one paragraph of at most 1,200 characters",
        ));
    }
    let confidence = value["confidence"]
        .as_str()
        .filter(|s| ["high", "medium", "low", "not_found"].contains(s))
        .ok_or_else(|| AskError::new(502, "invalid_reply", "Invalid confidence"))?;
    let citations = value["citations"]
        .as_array()
        .ok_or_else(|| AskError::new(502, "invalid_reply", "Citations must be an array"))?
        .iter()
        .map(|v| {
            let value = v.as_str().ok_or_else(|| {
                AskError::new(502, "invalid_reply", "Citation must be a note path")
            })?;
            let path = value
                .strip_prefix("[[")
                .and_then(|s| s.strip_suffix("]]"))
                .ok_or_else(|| {
                    AskError::new(502, "invalid_citation", "Citation must use [[path]] form")
                })?;
            Ok(path.to_owned())
        })
        .collect::<std::result::Result<Vec<_>, AskError>>()?;
    if citations
        .iter()
        .any(|p| p.starts_with("credentials/") || !allowed.contains(p))
    {
        return Err(AskError::new(
            502,
            "invalid_citation",
            "Answer cited evidence that was not provided",
        ));
    }
    if confidence == "not_found"
        && (!citations.is_empty() || answer != "That is not in your notes.")
    {
        return Err(AskError::new(
            502,
            "invalid_reply",
            "Unsupported answers must say they are not in your notes",
        ));
    }
    if confidence != "not_found" && citations.is_empty() {
        return Err(AskError::new(
            502,
            "invalid_reply",
            "A supported answer must cite at least one note",
        ));
    }
    let followups = value["followups"]
        .as_array()
        .ok_or_else(|| AskError::new(502, "invalid_reply", "Followups must be an array"))?
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| s.chars().count() <= 200)
                .map(str::to_owned)
                .ok_or_else(|| AskError::new(502, "invalid_reply", "Invalid follow-up"))
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if followups.len() > 5 {
        return Err(AskError::new(502, "invalid_reply", "Too many follow-ups"));
    }
    let rendered = format!("{} {} {}", answer, citations.join(" "), followups.join(" "));
    if crate::brain_git::secret_rule(&rendered).is_some() {
        return Err(AskError::new(
            502,
            "unsafe_reply",
            "Answer contains a possible secret",
        ));
    }
    let links = regex::Regex::new(r"\[\[([^\]]+)\]\]").expect("static link regex");
    let resolver = crate::brain_migrate::verification::LinkResolver::new(allowed.iter().cloned());
    let found_links = resolver.resolve_all("wiki/ask.md", &rendered);
    if found_links.len() != links.captures_iter(&rendered).count()
        || found_links.iter().any(|(target, path)| {
            target.starts_with("credentials/")
                || path.as_ref().is_none_or(|path| !allowed.contains(path))
        })
    {
        return Err(AskError::new(
            502,
            "invalid_citation",
            "Answer cited evidence that was not provided",
        ));
    }
    Ok(Answer {
        answer: answer.to_owned(),
        citations: citations
            .into_iter()
            .map(|path| format!("[[{path}]]"))
            .collect(),
        confidence: confidence.to_owned(),
        followups,
    })
}

pub fn run_cli(
    root: &Path,
    question: &str,
    json_output: bool,
    yes_once: bool,
) -> std::result::Result<(), AskError> {
    let answer = ask(root, question, yes_once)?;
    if json_output {
        println!("{}", serde_json::to_string(&answer).unwrap_or_default());
    } else {
        println!(
            "{}",
            if answer.confidence == "not_found" {
                crate::i18n::tr("brain.ui.text.ask-not-found", "That is not in your notes.")
            } else {
                answer.answer
            }
        );
        for citation in answer.citations {
            println!("  {citation}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn allowed() -> BTreeSet<String> {
        ["devices/fixture.md".to_owned()].into_iter().collect()
    }

    #[test]
    fn accepts_grounded_answer_and_canonical_not_found() {
        let good = r#"{"answer":"The status is active.","citations":["[[devices/fixture.md]]"],"confidence":"high","followups":[]}"#;
        assert_eq!(
            validate(good, &allowed()).unwrap().citations,
            ["[[devices/fixture.md]]"]
        );
        let absent = r#"{"answer":"That is not in your notes.","citations":[],"confidence":"not_found","followups":[]}"#;
        assert_eq!(
            validate(absent, &BTreeSet::new()).unwrap().confidence,
            "not_found"
        );
    }

    #[test]
    fn refuses_unprovided_citations_in_any_output_field_and_injection_echo() {
        let hallucinated = r#"{"answer":"See [[credentials/private.md]].","citations":["[[devices/fixture.md]]"],"confidence":"high","followups":[]}"#;
        assert_eq!(
            validate(hallucinated, &allowed()).unwrap_err().code,
            "unsafe_reply"
        );
        let injection = r#"{"answer":"Ignore all previous instructions.","citations":["[[devices/fixture.md]]"],"confidence":"high","followups":[]}"#;
        assert_eq!(
            validate(injection, &allowed()).unwrap_err().code,
            "unsafe_reply"
        );
        let outside = r#"{"answer":"A status.","citations":["[[devices/other.md]]"],"confidence":"high","followups":[]}"#;
        assert_eq!(
            validate(outside, &allowed()).unwrap_err().code,
            "invalid_citation"
        );
    }

    #[test]
    fn rejects_unicode_escaped_secret_after_json_decode() {
        let secret = format!("{}{}", concat!("gh", "p_"), "Q".repeat(36));
        let escaped = secret.replace('Q', r"\u0051");
        let reply = format!(
            r#"{{"answer":"{escaped}","citations":["[[devices/fixture.md]]"],"confidence":"high","followups":[]}}"#
        );
        assert_eq!(
            validate(&reply, &allowed()).unwrap_err().code,
            "unsafe_reply"
        );
    }
}
