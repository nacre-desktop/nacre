//! Resolve a running terminal's named chat from metadata, never conversation/auth data.
use crate::{json, paths::Paths, unicode, Result};
use serde::Serialize;
use serde_json::Value;
use std::{collections::HashSet, fs, io, os::unix::fs::MetadataExt, path::Path};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct Info {
    pub title: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
}

pub fn clean_title(value: &Value) -> String {
    let text: String = json::python_str(value)
        .chars()
        .filter(|ch| !unicode::is_other(*ch))
        .collect();
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(100)
        .collect()
}

fn thread_id(value: &str) -> bool {
    value.len() == 36
        && value
            .bytes()
            .all(|ch| ch.is_ascii_digit() || (b'a'..=b'f').contains(&ch) || ch == b'-')
}

fn chat_identity(args: &[String]) -> Result<(&str, &str)> {
    let Some(program) = args.first().and_then(|arg| Path::new(arg).file_name()) else {
        return Ok(("", ""));
    };
    if program != "codex" && program != "claude" {
        return Ok(("", ""));
    }
    let argument = |option: &str| -> Result<&str> {
        let Some(index) = args.iter().position(|arg| arg == option) else {
            return Ok("");
        };
        args.get(index + 1)
            .map(String::as_str)
            .ok_or_else(|| "Incomplete chat process arguments".into())
    };
    let option = ["resume", "--resume", "-r"]
        .into_iter()
        .find(|option| args.iter().any(|arg| arg == option));
    let identity = option.map(argument).transpose()?.unwrap_or("");
    Ok((
        if thread_id(identity) { identity } else { "" },
        argument("-C")?,
    ))
}

fn optional_failure(context: &str, error: &(dyn std::error::Error + 'static)) {
    if error
        .downcast_ref::<io::Error>()
        .is_some_and(|error| error.kind() == io::ErrorKind::NotFound)
    {
        return;
    }
    eprintln!("{context}: {error}");
}

fn read_json(path: &Path) -> Result<Value> {
    Ok(serde_json::from_slice(&fs::read(path)?)?)
}

pub fn cached_info(paths: &Paths, thread: &str, cwd: &str) -> Info {
    let root = paths.state.join("ai/chat-titles");
    let files = if !thread.is_empty() {
        vec![root.join(format!("{thread}.json"))]
    } else {
        let directory = Path::new(cwd);
        let name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if directory.parent() != Some(paths.share.join("ai/chats").as_path())
            || name.len() != 32
            || !name
                .bytes()
                .all(|ch| ch.is_ascii_digit() || (b'a'..=b'f').contains(&ch))
        {
            return Info::default();
        }
        match read_json(&root.join("directories").join(format!("{name}.json"))) {
            Ok(identity) => {
                if let Some(identity) = identity
                    .get("threadId")
                    .and_then(Value::as_str)
                    .filter(|id| thread_id(id))
                {
                    return cached_info(paths, identity, "");
                }
            }
            Err(error) => optional_failure("Chat directory identity", error.as_ref()),
        }
        match fs::read_dir(&root) {
            Ok(entries) => entries
                .filter_map(|entry| match entry {
                    Ok(entry) if entry.file_name().to_string_lossy().ends_with(".json") => {
                        Some(entry.path())
                    }
                    Ok(_) => None,
                    Err(error) => {
                        optional_failure("Chat-title directory", &error);
                        None
                    }
                })
                .collect(),
            Err(error) => {
                optional_failure("Chat-title directory", &error);
                Vec::new()
            }
        }
    };
    let mut matches = Vec::new();
    for file in files {
        match read_json(&file) {
            Ok(data)
                if !thread.is_empty() || data.get("cwd").and_then(Value::as_str) == Some(cwd) =>
            {
                if let Some(title) = data.get("title") {
                    matches.push(Info {
                        title: clean_title(title),
                        thread_id: file
                            .file_stem()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    });
                } else {
                    eprintln!("Chat-title cache: missing title field");
                }
            }
            Ok(_) => {}
            Err(error) => optional_failure("Chat-title cache", error.as_ref()),
        }
    }
    if matches.len() == 1 {
        matches.remove(0)
    } else {
        Info::default()
    }
}

pub fn resolve_info(pid: i64, proc: &Path, paths: &Paths, uid: u32) -> Info {
    let mut todo = vec![pid];
    let mut seen = HashSet::new();
    while !todo.is_empty() && seen.len() < 64 {
        let current = todo.pop().expect("Checked nonempty process queue");
        if !seen.insert(current) {
            continue;
        }
        let process = proc.join(current.to_string());
        let result = (|| -> Result<Info> {
            if fs::metadata(&process)?.uid() != uid {
                return Ok(Info::default());
            }
            for child in fs::read_to_string(
                process
                    .join("task")
                    .join(current.to_string())
                    .join("children"),
            )?
            .split_whitespace()
            {
                todo.push(child.parse()?);
            }
            // Preserve the second ownership check before reading arguments.
            if fs::metadata(&process)?.uid() != uid {
                return Ok(Info::default());
            }
            let bytes = fs::read(process.join("cmdline"))?;
            let args: Vec<String> = String::from_utf8_lossy(&bytes)
                .split('\0')
                .map(str::to_owned)
                .collect();
            let (thread, cwd) = chat_identity(&args)?;
            if thread.is_empty() && cwd.is_empty() {
                return Ok(Info::default());
            }
            let info = cached_info(paths, thread, cwd);
            if !info.title.is_empty() {
                return Ok(info);
            }
            Ok(Info {
                title: String::new(),
                thread_id: thread.to_owned(),
            })
        })();
        match result {
            Ok(info) if !info.title.is_empty() || !info.thread_id.is_empty() => return info,
            Ok(_) => {}
            Err(error) => optional_failure("Chat process metadata", error.as_ref()),
        }
    }
    Info::default()
}

pub fn execute(arguments: &[String], _request: Option<Value>) -> Result<Option<Value>> {
    let pid = arguments
        .first()
        .ok_or("Missing process ID")?
        .trim()
        .parse()?;
    Ok(Some(serde_json::to_value(resolve_info(
        pid,
        Path::new("/proc"),
        &Paths::environment()?,
        nix::unistd::getuid().as_raw(),
    ))?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const THREAD: &str = "11111111-1111-1111-1111-111111111111";

    #[test]
    fn title_sanitization_preserves_python_categories_and_character_limit() {
        assert_eq!(
            clean_title(&json!("Named\u{1b}\u{7}\n chat\u{200d}\u{e000}\u{378}")),
            "Named chat"
        );
        assert_eq!(clean_title(&json!("é".repeat(110))), "é".repeat(100));
        assert_eq!(clean_title(&json!(true)), "True");
    }

    #[test]
    fn resumed_identity_and_ambiguous_directory_match_existing_tests() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let cache = paths.state.join("ai/chat-titles");
        fs::create_dir_all(&cache)?;
        fs::write(
            cache.join(format!("{THREAD}.json")),
            r#"{"title":"Named\u001b\u0007\n chat"}"#,
        )?;
        assert_eq!(cached_info(&paths, THREAD, "").title, "Named chat");
        let args = ["/usr/bin/codex", "resume", THREAD].map(str::to_owned);
        assert_eq!(chat_identity(&args)?, (THREAD, ""));
        assert_eq!(
            chat_identity(&["/usr/bin/echo".into(), "resume".into(), THREAD.into()])?,
            ("", "")
        );
        let directory = paths.share.join("ai/chats").join("a".repeat(32));
        for name in ["one", "two"] {
            fs::write(
                cache.join(format!("{name}.json")),
                serde_json::to_vec(&json!({"title":name,"cwd":directory}))?,
            )?;
        }
        assert_eq!(
            cached_info(&paths, "", directory.to_str().unwrap()),
            Info::default()
        );
        assert_eq!(cached_info(&paths, "", "/repo/project"), Info::default());
        Ok(())
    }

    #[test]
    fn fresh_chat_uses_named_metadata_without_writing() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let cache = paths.state.join("ai/chat-titles");
        fs::create_dir_all(&cache)?;
        let directory = paths.share.join("ai/chats").join("a".repeat(32));
        fs::write(
            cache.join(format!("{THREAD}.json")),
            serde_json::to_vec(&json!({"title":"Fix Niri Titles", "cwd":directory}))?,
        )?;
        let proc = home.path().join("proc");
        fs::create_dir_all(proc.join("123/task/123"))?;
        fs::write(proc.join("123/task/123/children"), "")?;
        fs::write(
            proc.join("123/cmdline"),
            format!("/usr/bin/codex\0-C\0{}\0", directory.display()),
        )?;
        assert_eq!(
            resolve_info(123, &proc, &paths, nix::unistd::getuid().as_raw()).title,
            "Fix Niri Titles"
        );
        assert_eq!(fs::read_dir(&cache)?.count(), 1);
        Ok(())
    }
}
