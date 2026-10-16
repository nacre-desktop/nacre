//! Locale/Exec adaptation: Quickshell contributors, LGPL-3.0, relicensed GPL-3.0-only.
//! See nacre/shell-tools/launcher-engine-NOTICE.txt for source and license.
//! Desktop-entry compatibility with the frozen Quickshell 0.3.2 catalog.
//! Visibility and Exec field-code behavior intentionally retain the old launcher.
use crate::{text::OrderingRules, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Clone, Default)]
struct Locale {
    language: String,
    territory: String,
    modifier: String,
}
impl Locale {
    fn parse(text: &str) -> Self {
        let (base, modifier) = text.split_once('@').unwrap_or((text, ""));
        let base = base.split('.').next().unwrap_or("");
        let (language, territory) = base.split_once('_').unwrap_or((base, ""));
        Self {
            language: language.into(),
            territory: territory.into(),
            modifier: modifier.into(),
        }
    }
    fn score(&self, other: &Self) -> u8 {
        if self.language != other.language
            || (!other.territory.is_empty() && self.territory != other.territory)
            || (!other.modifier.is_empty() && self.modifier != other.modifier)
        {
            0
        } else {
            1 + 2 * u8::from(!other.territory.is_empty()) + u8::from(!other.modifier.is_empty())
        }
    }
}
/// Tokenize literally, retaining the old Qt parser's escaping and field codes.
/// No shell interpretation or process is involved.
pub fn command(text: &str) -> Vec<String> {
    let mut output = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut escape = 0;
    let mut percent = false;
    for c in text.chars() {
        if escape == 0 && c == '\\' {
            escape = 1;
        } else if quoted {
            if c == '\\' {
                escape += 1;
                if escape == 4 {
                    word.push('\\');
                    escape = 0;
                }
            } else if escape == 2 {
                word.push(c);
                escape = 0;
            } else if escape != 0 {
                word.push(match c {
                    's' => ' ',
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    v => v,
                });
                escape = 0;
            } else if matches!(c, '"' | '\'') {
                quoted = false;
            } else {
                word.push(c);
            }
        } else if escape != 0 {
            word.push(c);
            escape = 0;
        } else if percent {
            if c == '%' {
                word.push('%');
            }
            percent = false;
        } else if c == '%' {
            percent = true;
        } else if matches!(c, '"' | '\'') {
            quoted = true;
        } else if c == ' ' {
            if !word.is_empty() {
                output.push(std::mem::take(&mut word));
            }
        } else {
            word.push(c);
        }
    }
    if !word.is_empty() {
        output.push(word);
    }
    output
}
#[derive(Clone)]
pub struct Entry {
    pub data: std::sync::Arc<Value>,
    pub hidden: bool,
    pub path: PathBuf,
    /// Malformed non-comment lines, reported without discarding valid fields.
    pub invalid_lines: Vec<usize>,
}
impl Entry {
    pub fn valid(&self) -> bool {
        self.data["name"].as_str().is_some_and(|s| !s.is_empty())
    }
}
pub fn parse(id: &str, text: &str, message_locale: &str) -> Entry {
    let locale = Locale::parse(message_locale);
    let mut values: BTreeMap<String, (Locale, String)> = BTreeMap::new();
    let mut data = BTreeMap::<String, String>::new();
    let mut group = "";
    let finish = |group: &str,
                  values: &mut BTreeMap<String, (Locale, String)>,
                  data: &mut BTreeMap<String, String>| {
        if group == "Desktop Entry" && values.get("Type").is_some_and(|v| v.1 == "Application") {
            for (key, (_, value)) in values.iter() {
                data.insert(key.clone(), value.clone());
            }
        }
        values.clear();
    };
    let mut invalid_lines = Vec::new();
    for (number, line) in text.split('\n').enumerate().filter(|(_, s)| !s.is_empty()) {
        if line.starts_with('#') {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            finish(group, &mut values, &mut data);
            group = &line[1..line.len() - 1];
            continue;
        }
        let Some((raw_key, value)) = line.split_once('=') else {
            invalid_lines.push(number + 1);
            continue;
        };
        let (key, language) = if let Some((key, suffix)) = raw_key.split_once('[') {
            if let Some(suffix) = suffix.strip_suffix(']') {
                (key, Locale::parse(suffix))
            } else {
                (raw_key, Locale::default())
            }
        } else {
            (raw_key, Locale::default())
        };
        let replace = values.get(key).is_none_or(|(old, _)| {
            locale.score(&language) > locale.score(old)
                || (locale.score(old) == 0 && language.language.is_empty())
        });
        if replace {
            values.insert(key.into(), (language, value.into()));
        }
    }
    finish(group, &mut values, &mut data);
    let get = |name: &str| data.get(name).cloned().unwrap_or_default();
    let list = |name: &str| {
        get(name)
            .split(';')
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    Entry {
        data: std::sync::Arc::new(
            json!({"id":id,"name":get("Name"),"genericName":get("GenericName"),"comment":get("Comment"),"icon":get("Icon"),"categories":list("Categories"),"keywords":list("Keywords"),"command":command(&get("Exec")),"workingDirectory":get("Path"),"runInTerminal":get("Terminal")=="true","noDisplay":get("NoDisplay")=="true"}),
        ),
        hidden: get("Hidden") == "true",
        path: PathBuf::new(),
        invalid_lines,
    }
}
pub fn install(entries: &mut BTreeMap<String, Entry>, entry: Entry) {
    let id = entry.data["id"].as_str().unwrap_or("").to_owned();
    if entry.hidden {
        entries.remove(&id);
    } else if entry.valid() {
        entries.insert(id, entry);
    }
}
pub fn applications(
    entries: &BTreeMap<String, Entry>,
    ordering: &OrderingRules,
) -> Vec<std::sync::Arc<Value>> {
    let mut rows: Vec<_> = entries
        .values()
        .filter(|e| e.data["noDisplay"] != true)
        .map(|e| e.data.clone())
        .collect();
    rows.sort_by(|a, b| {
        ordering
            .compare(
                a["name"].as_str().unwrap_or(""),
                b["name"].as_str().unwrap_or(""),
            )
            .then_with(|| {
                ordering.compare(
                    a["id"].as_str().unwrap_or(""),
                    b["id"].as_str().unwrap_or(""),
                )
            })
    });
    rows
}
/// Read roots from lowest to highest precedence. The caller watches these roots.
/// Bound directory depth and detect cycles while preserving exported symlink files.
pub fn scan(roots: &[PathBuf], message_locale: &str) -> Result<BTreeMap<String, Entry>> {
    let mut entries = BTreeMap::new();
    for root in roots.iter().rev() {
        let mut ancestors = Vec::new();
        scan_dir(root, "", message_locale, &mut ancestors, &mut entries)?;
    }
    Ok(entries)
}
fn scan_dir(
    directory: &Path,
    prefix: &str,
    locale: &str,
    ancestors: &mut Vec<PathBuf>,
    entries: &mut BTreeMap<String, Entry>,
) -> Result<()> {
    let canonical = match fs::canonicalize(directory) {
        Ok(p) => p,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    if ancestors.len() >= 32 || ancestors.contains(&canonical) {
        return Err("Application directory contains a cycle or exceeds the depth limit".into());
    }
    ancestors.push(canonical);
    let mut paths = fs::read_dir(directory)?.collect::<std::result::Result<Vec<_>, _>>()?;
    paths.sort_by_key(|e| e.file_name().to_string_lossy().to_lowercase());
    for path in paths {
        let name = path.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let path = path.path();
        let metadata = fs::metadata(&path)?;
        if metadata.is_dir() {
            let prefix = if prefix.is_empty() {
                name
            } else {
                format!("{prefix}-{name}")
            };
            scan_dir(&path, &prefix, locale, ancestors, entries)?;
        } else if metadata.is_file() && name.ends_with(".desktop") {
            if metadata.len() > 1048576 {
                return Err("Desktop entry is too large".into());
            }
            if entries.len() >= 16384 {
                return Err("Application catalog is too large".into());
            }
            let name = name.strip_suffix(".desktop").unwrap();
            let id = if prefix.is_empty() {
                name.into()
            } else {
                format!("{prefix}-{name}")
            };
            let mut bytes = Vec::new();
            fs::File::open(&path)?
                .take(1048577)
                .read_to_end(&mut bytes)?;
            if bytes.len() > 1048576 {
                return Err("Desktop entry is too large".into());
            }
            let mut entry = parse(&id, &String::from_utf8_lossy(&bytes), locale);
            entry.path = path;
            install(entries, entry);
        }
    }
    ancestors.pop();
    Ok(())
}
