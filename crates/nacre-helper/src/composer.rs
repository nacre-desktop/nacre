//! Private sidebar composer drafts, attachments, clipboard and area captures.
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::Write,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, SystemTime},
};

const MAX_FILE: u64 = 30 * 1024 * 1024;
const MAX_ATTACHMENTS: usize = 8;
const MAX_DRAFT: u64 = 1024 * 1024;
const SCREENSHOT_TTL: Duration = Duration::from_secs(30 * 24 * 60 * 60);

fn root() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
        .join(".local/state/nacre/shell/drafts")
}
fn key_path(root: &Path, key: &str) -> PathBuf {
    root.join(format!("{:x}.json", Sha256::digest(key.as_bytes())))
}
fn private_dir(path: &Path) -> Result<(), String> {
    let mut component = PathBuf::new();
    for part in path.components() {
        component.push(part);
        match fs::symlink_metadata(&component) {
            Ok(metadata) if metadata.file_type().is_symlink() || !metadata.is_dir() => {
                return Err("Refusing linked or non-directory composer path".into())
            }
            Ok(_) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&component).map_err(|e| e.to_string())?;
                fs::set_permissions(
                    &component,
                    fs::Permissions::from_mode(if component.as_path() == path {
                        0o700
                    } else {
                        0o755
                    }),
                )
                .map_err(|e| e.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700)).map_err(|e| e.to_string())
}
fn safe_directory_tree(path: &Path) -> bool {
    let mut component = PathBuf::new();
    for part in path.components() {
        component.push(part);
        let Ok(meta) = fs::symlink_metadata(&component) else {
            return false;
        };
        if meta.file_type().is_symlink() || !meta.is_dir() {
            return false;
        }
    }
    true
}
fn save_at(root: &Path, key: &str, value: &Value) -> Result<(), String> {
    private_dir(root)?;
    let target = key_path(root, key);
    if let Ok(meta) = fs::symlink_metadata(&target) {
        if meta.file_type().is_symlink() || !meta.is_file() || meta.nlink() != 1 {
            return Err("Refusing linked or non-regular composer draft".into());
        }
    }
    crate::state::write(&target, value).map_err(|e| e.to_string())
}
fn load_at(root: &Path, key: &str) -> Result<Value, String> {
    let path = key_path(root, key);
    let safe = safe_directory_tree(root)
        .then(|| fs::symlink_metadata(&path).ok())
        .flatten()
        .filter(|m| {
            m.is_file() && !m.file_type().is_symlink() && m.nlink() == 1 && m.len() <= MAX_DRAFT
        })
        .is_some();
    let value = if safe {
        crate::state::read_as(
            &path,
            crate::state::by_id("drafts").ok(),
            json!({"text":"","attachments":[]}),
        )
        .map_err(|error| error.to_string())?
    } else {
        json!({"text":"","attachments":[]})
    };
    Ok(Some(value)
        .filter(Value::is_object)
        .unwrap_or_else(|| json!({"text":"","attachments":[]})))
}

fn decode_file_url(raw: &str) -> Result<String, String> {
    if raw.starts_with("file:") {
        let rest = raw.strip_prefix("file:").unwrap_or(raw);
        let path = if let Some(rest) = rest.strip_prefix("//localhost/") {
            format!("/{rest}")
        } else if rest.starts_with("///") {
            rest[2..].to_owned()
        } else if rest.starts_with("//") {
            return Err("Attachments must be local files".into());
        } else if rest.starts_with('/') {
            rest.to_owned()
        } else {
            return Err("Attachments must be local files".into());
        };
        let bytes = path.as_bytes();
        let mut out = Vec::with_capacity(bytes.len());
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex =
                    std::str::from_utf8(&bytes[i + 1..i + 3]).map_err(|_| "Invalid file URL")?;
                out.push(u8::from_str_radix(hex, 16).map_err(|_| "Invalid file URL")?);
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8(out).map_err(|_| "Invalid file URL".into())
    } else if raw.contains(':')
        && raw.split_once(':').is_some_and(|(scheme, _)| {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        })
    {
        Err("Attachments must be local files".into())
    } else if raw == "~" {
        Ok(std::env::var("HOME").unwrap_or_default())
    } else if let Some(rest) = raw.strip_prefix("~/") {
        Ok(PathBuf::from(std::env::var("HOME").unwrap_or_default())
            .join(rest)
            .to_string_lossy()
            .into_owned())
    } else {
        Ok(raw.to_owned())
    }
}
fn normalize_attachments(values: &Value) -> Result<Vec<Value>, String> {
    let values = values
        .as_array()
        .ok_or("Choose at most eight attachments")?;
    if values.len() > MAX_ATTACHMENTS {
        return Err("Choose at most eight attachments".into());
    }
    let mut out: Vec<Value> = Vec::new();
    for item in values {
        let raw = item
            .get("path")
            .and_then(Value::as_str)
            .map(str::to_owned)
            .or_else(|| item.as_str().map(str::to_owned))
            .unwrap_or_default();
        let raw = decode_file_url(&raw)?;
        let path = PathBuf::from(&raw).canonicalize().map_err(|_| {
            format!(
                "Attachment no longer exists: {}",
                Path::new(&raw)
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
            )
        })?;
        let meta = fs::metadata(&path).map_err(|e| e.to_string())?;
        if !meta.is_file() {
            return Err(format!(
                "Attachment no longer exists: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        if meta.len() > MAX_FILE {
            return Err(format!(
                "Attachment exceeds 30 MiB: {}",
                path.file_name().unwrap_or_default().to_string_lossy()
            ));
        }
        let path = path.to_string_lossy().into_owned();
        if out.iter().any(|v| v["path"] == path) {
            continue;
        }
        let name = Path::new(&path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let image = matches!(
            Path::new(&path)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
            "png" | "jpg" | "jpeg" | "webp" | "gif"
        );
        out.push(json!({"path":path,"name":name,"image":image}));
    }
    Ok(out)
}

fn expire_screenshots(root: &Path) {
    expire_screenshots_at(root, SystemTime::now());
}
fn expire_screenshots_at(root: &Path, now: SystemTime) {
    if !safe_directory_tree(root) {
        return;
    }
    let dir = root.join("screenshots");
    if fs::symlink_metadata(&dir)
        .ok()
        .is_some_and(|m| m.file_type().is_symlink() || !m.is_dir())
    {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_type().is_ok_and(|t| t.is_file())
            && entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|m| now.duration_since(m).ok())
                .is_some_and(|age| age > SCREENSHOT_TTL)
        {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Execute a composer action. `stdin` is the helper's JSON request; errors are returned
/// as the legacy `{"error": ...}` object to retain shell-helper behavior.
pub fn attachments(values: &Value) -> Result<Vec<Value>, String> {
    normalize_attachments(values)
}
pub fn run(action: &str, input: &Value) -> Value {
    let mut reply = run_at(&root(), action, input);
    if let Some(error) = reply["error"].as_str() {
        let translated = match error {
            "Choose at most eight attachments" => crate::i18n::tr(
                "helper.composer.choose-at-most-eight-attachments",
                "Choose at most eight attachments",
            ),
            "Attachments must be local files" => crate::i18n::tr(
                "helper.composer.attachments-must-be-local-files",
                "Attachments must be local files",
            ),
            _ if error.starts_with("Attachment no longer exists: ") => crate::i18n::tr(
                "helper.composer.attachment-no-longer-exists-value0",
                "Attachment no longer exists: {value0}",
            )
            .replace(
                "{value0}",
                error.trim_start_matches("Attachment no longer exists: "),
            ),
            _ if error.starts_with("Attachment exceeds 30 MiB: ") => crate::i18n::tr(
                "helper.composer.attachment-exceeds-30-mib-value0",
                "Attachment exceeds 30 MiB: {value0}",
            )
            .replace(
                "{value0}",
                error.trim_start_matches("Attachment exceeds 30 MiB: "),
            ),
            _ => error.to_owned(),
        };
        reply["error"] = json!(translated);
    }
    reply
}
fn run_at(root: &Path, action: &str, input: &Value) -> Value {
    expire_screenshots(root);
    let result: Result<Value, String> = (|| {
        Ok(match action {
            "load" => load_at(root, input["key"].as_str().ok_or("missing key")?)?,
            "save" => {
                let key = input["key"].as_str().ok_or("missing key")?;
                let text = input.get("text").and_then(Value::as_str).unwrap_or("");
                // A draft must survive an attachment being moved or deleted. Validate
                // existence when selecting/sending files, as the original composer did.
                let files = input
                    .get("attachments")
                    .cloned()
                    .unwrap_or_else(|| json!([]));
                if !files
                    .as_array()
                    .is_some_and(|files| files.len() <= MAX_ATTACHMENTS)
                {
                    return Err("Choose at most eight attachments".into());
                }
                save_at(root, key, &json!({"text":text,"attachments":files}))?;
                json!({})
            }
            "files" => {
                json!({"attachments":normalize_attachments(input.get("files").ok_or("missing files")?)?})
            }
            "copy" => {
                let text = input["text"].as_str().ok_or("missing text")?;
                let mut child = Command::new("wl-copy")
                    .stdin(Stdio::piped())
                    .spawn()
                    .map_err(|e| e.to_string())?;
                child
                    .stdin
                    .take()
                    .ok_or("clipboard stdin unavailable")?
                    .write_all(text.as_bytes())
                    .map_err(|e| e.to_string())?;
                if !child.wait().map_err(|e| e.to_string())?.success() {
                    return Err("wl-copy failed".into());
                }
                json!({})
            }
            "pick" => {
                let output = Command::new("zenity")
                    .args([
                        "--file-selection",
                        "--multiple",
                        "--separator=\n",
                        "--title=Attach local files",
                    ])
                    .output()
                    .map_err(|e| e.to_string())?;
                let selected: Vec<Value> = if output.status.success() {
                    String::from_utf8_lossy(&output.stdout)
                        .trim()
                        .lines()
                        .map(|s| json!(s))
                        .collect()
                } else {
                    vec![]
                };
                json!({"attachments":normalize_attachments(&json!(selected))?})
            }
            "screenshot" => {
                let selected = Command::new("slurp").output().map_err(|e| e.to_string())?;
                if !selected.status.success() {
                    return Ok(json!({"attachments":[]}));
                }
                let geometry = String::from_utf8_lossy(&selected.stdout).trim().to_owned();
                private_dir(root)?;
                let dir = root.join("screenshots");
                private_dir(&dir)?;
                let target = dir.join(format!("screen-{}.png", uuid::Uuid::new_v4().simple()));
                let output = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .mode(0o600)
                    .open(&target)
                    .map_err(|e| e.to_string())?;
                let status = Command::new("grim")
                    .args(["-g", &geometry, "-"])
                    .stdout(Stdio::from(output))
                    .status()
                    .map_err(|e| e.to_string())?;
                if !status.success() {
                    let _ = fs::remove_file(&target);
                    return Err("grim failed".into());
                }
                json!({"attachments":normalize_attachments(&json!([target.to_string_lossy()]))?})
            }
            _ => return Err("invalid action".into()),
        })
    })();
    result.unwrap_or_else(|error| json!({"error":error}))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn drafts_are_private_isolated_and_reloadable() {
        let d = tempfile::tempdir().unwrap();
        save_at(
            d.path(),
            "thread-one",
            &json!({"text":"Draft one","attachments":[]}),
        )
        .unwrap();
        save_at(
            d.path(),
            "thread-two",
            &json!({"text":"Draft two","attachments":[]}),
        )
        .unwrap();
        assert_eq!(
            load_at(d.path(), "thread-one").unwrap()["text"],
            "Draft one"
        );
        assert_eq!(
            load_at(d.path(), "thread-two").unwrap()["text"],
            "Draft two"
        );
        let path = key_path(d.path(), "thread-one");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::read_to_string(path).unwrap(),
            r#"{"text": "Draft one", "attachments": []}"#
        );
        assert_eq!(
            fs::metadata(d.path()).unwrap().permissions().mode() & 0o777,
            0o700
        );
    }
    #[test]
    fn draft_reads_and_writes_reject_links_and_oversized_files() {
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let key = "linked";
        let linked = key_path(d.path(), key);
        let outside = d.path().join("outside.json");
        fs::write(&outside, b"{}\n").unwrap();
        symlink(&outside, &linked).unwrap();
        assert_eq!(load_at(d.path(), key).unwrap()["text"], "");
        assert!(save_at(d.path(), key, &json!({"text":"x","attachments":[]})).is_err());
        fs::remove_file(&linked).unwrap();
        fs::hard_link(&outside, &linked).unwrap();
        assert_eq!(load_at(d.path(), key).unwrap()["text"], "");
        assert!(save_at(d.path(), key, &json!({"text":"x","attachments":[]})).is_err());
        fs::remove_file(&linked).unwrap();
        fs::write(&linked, vec![b' '; MAX_DRAFT as usize + 1]).unwrap();
        assert_eq!(load_at(d.path(), key).unwrap()["text"], "");
    }
    #[test]
    fn screenshot_expiry_removes_only_old_regular_files_from_real_directory() {
        use std::fs::{File, FileTimes};
        use std::os::unix::fs::symlink;
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("screenshots");
        fs::create_dir(&dir).unwrap();
        let old = dir.join("old.png");
        fs::write(&old, b"old").unwrap();
        let young = dir.join("young.png");
        fs::write(&young, b"young").unwrap();
        let now = SystemTime::now();
        File::open(&old)
            .unwrap()
            .set_times(FileTimes::new().set_modified(now - SCREENSHOT_TTL - Duration::from_secs(1)))
            .unwrap();
        let linked = dir.join("linked.png");
        symlink(&young, &linked).unwrap();
        expire_screenshots_at(d.path(), now);
        assert!(!old.exists());
        assert!(young.exists());
        assert!(fs::symlink_metadata(linked)
            .unwrap()
            .file_type()
            .is_symlink());
        let victim = d.path().join("victim");
        fs::write(&victim, b"keep").unwrap();
        fs::remove_file(dir.join("young.png")).unwrap();
        fs::remove_file(dir.join("linked.png")).unwrap();
        fs::remove_dir(&dir).unwrap();
        symlink(&victim, &dir).unwrap();
        expire_screenshots_at(d.path(), now);
        assert_eq!(fs::read(victim).unwrap(), b"keep");
    }
    #[test]
    fn local_attachments_are_validated_and_deduplicated() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("screen.png");
        fs::write(&p, b"fixture").unwrap();
        let result =
            normalize_attachments(&json!([p.to_string_lossy(), p.to_uri_for_test()])).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["image"], true);
        assert!(normalize_attachments(&json!(["https://example.test/photo.png"])).is_err());
        assert!(normalize_attachments(&json!(["file://remote.test/photo.png"])).is_err());
        assert!(
            normalize_attachments(&json!([d.path().join("missing.txt").to_string_lossy()]))
                .is_err()
        );
    }
    #[test]
    fn future_draft_versions_are_reported_and_preserved() {
        let fixture = tempfile::tempdir().unwrap();
        let root = fixture.path().join("nacre/shell/drafts");
        private_dir(&root).unwrap();
        let path = key_path(&root, "future");
        let bytes = br#"{"version":999,"text":"keep this draft","attachments":[]}"#;
        fs::write(&path, bytes).unwrap();
        assert!(load_at(&root, "future").is_err());
        assert!(save_at(
            &root,
            "future",
            &json!({"text":"replacement","attachments":[]})
        )
        .is_err());
        assert_eq!(fs::read(path).unwrap(), bytes);
    }
    #[test]
    fn empty_load_matches_frozen_contract() {
        let d = tempfile::tempdir().unwrap();
        assert_eq!(
            load_at(d.path(), "demo").unwrap(),
            json!({"text":"","attachments":[]})
        );
    }
    trait TestUri {
        fn to_uri_for_test(&self) -> String;
    }
    impl TestUri for Path {
        fn to_uri_for_test(&self) -> String {
            format!("file://{}", self.display())
        }
    }
}
