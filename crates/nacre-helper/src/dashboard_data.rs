//! Dashboard preferences, local calendar/context and private focus state.
use crate::{files, i18n::tr, paths::Paths, process, Result};
use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, Offset, TimeZone, Utc};
use chrono_tz::Tz;
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

pub const PAGES: [&str; 5] = ["today", "weather", "media", "system", "focus"];
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
fn read(path: &Path, fallback: Value) -> Result<Value> {
    if path == Paths::environment()?.config.join("dashboard.json") {
        crate::owned_settings::read(path, "dashboard", fallback)
    } else {
        crate::state::read(path, fallback)
    }
}

fn expanded(path: &str, paths: &Paths) -> PathBuf {
    if path == "~" {
        paths.home.clone()
    } else if let Some(rest) = path.strip_prefix("~/") {
        paths.home.join(rest)
    } else {
        PathBuf::from(path)
    }
}
fn zone(value: &str) -> Result<Tz> {
    value
        .parse()
        .map_err(|_| format!("'No time zone found with key {value}'").into())
}
#[derive(Debug)]
pub struct DashboardTypeError {
    kind: &'static str,
}
impl std::fmt::Display for DashboardTypeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "cannot use '{}' as a set element (unhashable type: '{}')",
            self.kind, self.kind
        )
    }
}
impl std::error::Error for DashboardTypeError {}

#[derive(Debug)]
pub struct DashboardAttribute(&'static str);
impl std::fmt::Display for DashboardAttribute {
    fn fmt(&self, format: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(format, "'{}' object has no attribute 'items'", self.0)
    }
}
impl std::error::Error for DashboardAttribute {}

pub fn validate(data: &Value) -> Result<Value> {
    let data = data.as_object().ok_or_else(|| {
        DashboardAttribute(match data {
            Value::Null => "NoneType",
            Value::Bool(_) => "bool",
            Value::Array(_) => "list",
            Value::String(_) => "str",
            Value::Number(n) if n.is_f64() => "float",
            Value::Number(_) => "int",
            Value::Object(_) => unreachable!(),
        })
    })?;
    let mut result = json!({"tabs":PAGES,"homeZone":"","homeCity":"","calendarFile":"","lyrics":true,"albumPalette":true,"focusMinutes":25});
    let paths = Paths::environment()?;
    for (key, value) in data {
        match key.as_str() {
            "tabs" => {
                let values = value.as_array().ok_or_else(|| {
                    tr(
                        "helper.dashboard-data.keep-at-least-one-unique-dashboard-tab",
                        "Keep at least one unique dashboard tab",
                    )
                })?;
                if let Some(unhashable) = values
                    .iter()
                    .find(|value| value.is_object() || value.is_array())
                {
                    let kind = if unhashable.is_object() {
                        "dict"
                    } else {
                        "list"
                    };
                    return Err(DashboardTypeError { kind }.into());
                }
                let mut seen = HashSet::new();
                if values.is_empty()
                    || values.iter().any(|v| {
                        v.as_str()
                            .is_none_or(|v| !PAGES.contains(&v) || !seen.insert(v))
                    })
                {
                    return Err(tr(
                        "helper.dashboard-data.keep-at-least-one-unique-dashboard-tab",
                        "Keep at least one unique dashboard tab",
                    )
                    .into());
                }
            }
            "lyrics" | "albumPalette" => {
                if !value.is_boolean() {
                    return Err(tr(
                        "helper.dashboard-data.use-true-or-false",
                        "Use true or false",
                    )
                    .into());
                }
            }
            "focusMinutes" => {
                if value.as_u64().is_none_or(|v| !(1..=180).contains(&v)) {
                    return Err(tr(
                        "helper.dashboard-data.focus-duration-must-be1-180minutes",
                        "Focus duration must be1–180minutes",
                    )
                    .into());
                }
            }
            "homeZone" | "homeCity" | "calendarFile" => {
                let text = value
                    .as_str()
                    .filter(|v| v.chars().count() <= 1024 && !v.chars().any(|v| (v as u32) < 32))
                    .ok_or_else(|| tr("helper.dashboard-data.invalid-text", "Invalid text"))?;
                if key == "homeZone" && !text.is_empty() {
                    zone(text)?;
                }
                if key == "calendarFile"
                    && !text.is_empty()
                    && !expanded(text, &paths).is_absolute()
                {
                    return Err(tr(
                        "helper.dashboard-data.use-an-absolute-local-calendar-path",
                        "Use an absolute local calendar path",
                    )
                    .into());
                }
            }
            _ => {
                return Err(tr(
                    "helper.dashboard-data.unknown-dashboard-preference",
                    "Unknown dashboard preference",
                )
                .into())
            }
        }
        result[key] = value.clone();
    }
    Ok(result)
}
pub fn calendar(path: &str, current: DateTime<Utc>) -> Value {
    if path.is_empty() {
        return json!({"events":[],"error":"","configured":false});
    }
    let parse = || -> Result<Value> {
        let paths = Paths::environment()?;
        let source = expanded(path, &paths);
        if fs::metadata(&source)?.len() > 2_000_000 {
            return Err(tr(
                "helper.dashboard-data.calendar-exceeds2mb",
                "Calendar exceeds2MB",
            )
            .into());
        }
        let text = fs::read_to_string(&source)?;
        let text = regex::Regex::new(r"\r?\n[ \t]")?.replace_all(&text, "");
        let mut events = Vec::new();
        let mut recurring = false;
        for block in text.split("BEGIN:VEVENT").skip(1).take(500) {
            let mut fields = serde_json::Map::new();
            for line in block.split("END:VEVENT").next().unwrap_or("").lines() {
                if let Some((key, value)) = line.split_once(':') {
                    fields.insert(key.into(), json!(value));
                }
            }
            let Some(stamp) = fields.keys().find(|key| key.starts_with("DTSTART")) else {
                continue;
            };
            if fields.get("STATUS") == Some(&json!("CANCELLED")) {
                continue;
            }
            let raw = fields[stamp].as_str().unwrap_or("");
            let all_day = stamp.contains("VALUE=DATE") || raw.len() == 8;
            let parsed = if all_day {
                NaiveDate::parse_from_str(raw.trim_end_matches('Z'), "%Y%m%d")
                    .ok()
                    .and_then(|v| v.and_hms_opt(0, 0, 0))
            } else {
                NaiveDateTime::parse_from_str(raw.trim_end_matches('Z'), "%Y%m%dT%H%M%S").ok()
            };
            let Some(parsed) = parsed else {
                continue;
            };
            let current_local = Local::now().fixed_offset();
            let (event, today) = if let Some((_, name)) = stamp.split_once("TZID=") {
                let Ok(zone) = zone(name.split(';').next().unwrap_or("")) else {
                    continue;
                };
                let Some(event) = zone.from_local_datetime(&parsed).earliest() else {
                    continue;
                };
                (
                    event.fixed_offset(),
                    current.with_timezone(&zone).date_naive(),
                )
            } else if raw.ends_with('Z') {
                (
                    Utc.from_utc_datetime(&parsed).fixed_offset(),
                    current.date_naive(),
                )
            } else {
                (
                    current_local
                        .offset()
                        .from_local_datetime(&parsed)
                        .earliest()
                        .unwrap(),
                    current.with_timezone(current_local.offset()).date_naive(),
                )
            };
            recurring |= fields.contains_key("RRULE");
            if event.date_naive() >= today
                && (event.timestamp() as f64) <= current.timestamp() as f64 + 31.0 * 86400.0
            {
                let title = fields
                    .get("SUMMARY")
                    .and_then(Value::as_str)
                    .unwrap_or("Untitled event")
                    .replace("\\n", " ")
                    .replace("\\,", ",")
                    .chars()
                    .take(180)
                    .collect::<String>();
                let local = event.with_timezone(&Local);
                events.push(json!({"title":title,"start":event.timestamp() as f64,"time":if all_day{"All day".into()}else{local.format("%H:%M").to_string()},"date":local.format("%a %d %b").to_string()}));
            }
        }
        events.sort_by_key(|v| v["start"].as_f64().unwrap() as i64);
        events.truncate(8);
        Ok(
            json!({"events":events,"configured":true,"error":if recurring{"Recurring events require an expanded calendar export."}else{""}}),
        )
    };
    parse().unwrap_or_else(|error|json!({"events":[],"configured":true,"error":error.to_string().chars().take(180).collect::<String>()}))
}
fn brain(paths: &Paths) -> String {
    for root in std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()) {
        let candidate = root.join("nacre-brain");
        if candidate.is_file() {
            return candidate.to_string_lossy().into_owned();
        }
    }
    paths
        .home
        .join(".local/bin/nacre-brain")
        .to_string_lossy()
        .into_owned()
}
pub fn brain_context() -> Value {
    let context = || -> Result<Value> {
        let paths = Paths::environment()?;
        let output = process::checked(
            &brain(&paths),
            &["path".into()],
            None,
            Duration::from_secs(3),
        )?;
        brain_context_at(Path::new(String::from_utf8(output.stdout)?.trim()))
    };
    context().unwrap_or_else(|_| json!({"available":false,"recent":[],"tasks":[]}))
}
/// Recent notes and open tasks below one vault root. Only regular files are read:
/// a symlinked note is never followed out of the vault.
pub fn brain_context_at(root: &Path) -> Result<Value> {
    let mut notes: Vec<_> = walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|entry| {
            entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "md")
        })
        .take(2000)
        .filter(|entry| {
            !entry
                .path()
                .strip_prefix(root)
                .unwrap_or(entry.path())
                .components()
                .any(|part| part.as_os_str().to_string_lossy().starts_with('.'))
        })
        .map(|entry| entry.into_path())
        .collect();
    notes.sort_by_key(|path| {
        std::cmp::Reverse(
            fs::metadata(path)
                .and_then(|m| m.modified())
                .unwrap_or(UNIX_EPOCH),
        )
    });
    notes.truncate(10);
    let mut recent = Vec::new();
    let mut tasks = Vec::new();
    for note in notes {
        if fs::metadata(&note)?.len() > 128_000 {
            continue;
        }
        let text = fs::read_to_string(&note)?;
        let title = text
            .lines()
            .find(|line| line.starts_with("# "))
            .map(|line| line.trim_start_matches(['#', ' ']).trim())
            .unwrap_or_else(|| note.file_stem().and_then(|v| v.to_str()).unwrap_or(""));
        recent.push(json!({"title":title.chars().take(100).collect::<String>(),"path":note}));
        for line in text.lines() {
            if let Some(text) = line.strip_prefix("- [ ] ") {
                tasks.push(
                    json!({"text":text.trim().chars().take(160).collect::<String>(),"path":note}),
                );
            }
        }
    }
    recent.truncate(5);
    tasks.truncate(6);
    Ok(json!({"available":true,"recent":recent,"tasks":tasks,"root":root}))
}
pub fn snapshot(include_brain: bool) -> Result<Value> {
    let paths = Paths::environment()?;
    let preferences = validate(&read(&paths.config.join("dashboard.json"), json!({}))?)?;
    let focus = read(&paths.state.join("dashboard-focus.json"), json!({}))?;
    let configured = preferences["homeZone"].as_str().unwrap_or("");
    let system = read(Path::new("/etc/nacre/timezone.json"), json!({}))?;
    let zone_name = if configured.is_empty() {
        system["confirmedTimezone"].as_str().unwrap_or("")
    } else {
        configured
    };
    let mut home = json!({});
    if let Ok(zone) = zone(zone_name) {
        let local = Local::now();
        let at_home = local.with_timezone(&zone);
        if local.offset().fix() != at_home.offset().fix() {
            home = json!({"time":at_home.format("%H:%M").to_string(),"label":if preferences["homeCity"].as_str().unwrap_or("").is_empty(){zone_name.rsplit('/').next().unwrap_or("").replace('_'," ")}else{preferences["homeCity"].as_str().unwrap().into()}});
        }
    }
    Ok(
        json!({"preferences":preferences,"focus":focus,"home":home,"calendar":calendar(preferences["calendarFile"].as_str().unwrap_or(""),Utc::now()),"brain":if include_brain{brain_context()}else{json!({})},"checked":now()}),
    )
}
pub fn focus_action(kind: &str, minutes: &Value, dnd: &Value, clock: f64) -> Result<Value> {
    let path = Paths::environment()?.state.join("dashboard-focus.json");
    let mut state = read(&path, json!({}))?;
    match kind {
        "start" => {
            let minutes = minutes
                .as_f64()
                .or_else(|| minutes.as_bool().map(|v| f64::from(u8::from(v))))
                .filter(|v| *v >= 1.0 && *v <= 180.0)
                .ok_or_else(|| {
                    tr(
                        "helper.dashboard-data.choose1-180minutes",
                        "Choose1–180minutes",
                    )
                })?;
            let total = if minutes.fract() == 0.0 {
                json!((minutes * 60.0) as i64)
            } else {
                json!(minutes * 60.0)
            };
            state = json!({"status":"running","total":total,"remaining":total,"deadline":clock+minutes*60.0,"previousDnd":dnd,"ownsDnd":!dnd.as_bool().unwrap_or(false)});
        }
        "pause" if state["status"] == "running" => {
            state["status"] = json!("paused");
            let remaining = state["deadline"].as_f64().ok_or("Invalid focus deadline")? - clock;
            state["remaining"] = if remaining <= 0.0 {
                json!(0)
            } else {
                json!(remaining)
            };
        }
        "resume" if state["status"] == "paused" => {
            state["status"] = json!("running");
            state["deadline"] = json!(
                clock
                    + state["remaining"]
                        .as_f64()
                        .ok_or("Invalid focus remainder")?
            );
        }
        "stop" | "complete" => {
            state["status"] = json!(if kind == "complete" { "done" } else { "idle" });
            state["remaining"] = json!(0);
            state["deadline"] = json!(0);
            state["ownsDnd"] = json!(false);
        }
        "release" => {
            state["ownsDnd"] = json!(false);
        }
        _ => {
            return Err(tr(
                "helper.dashboard-data.focus-action-unavailable",
                "Focus action unavailable",
            )
            .into())
        }
    }
    files::json_write(&path, &state)?;
    Ok(state)
}
pub fn execute(arguments: &[String], input: Option<Value>) -> Result<Option<Value>> {
    if input.is_some() || arguments.is_empty() || arguments.len() > 2 {
        return Err("Unsupported helper operation".into());
    }
    let paths = Paths::environment()?;
    let config = paths.config.join("dashboard.json");
    let action = &arguments[0];
    let value = arguments.get(1).map(String::as_str).unwrap_or("");
    // This legacy guard is 0644 rather than the private launcher/menu guard.
    fs::create_dir_all(&paths.config)?;
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(config.with_extension("lock"))?;
    let _guard = nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusive)
        .map_err(|(_, error)| error)?;
    if action == "preferences" {
        let value = validate(&crate::json::parse_value(value)?)?;
        let mut document = crate::owned_settings::Document::open(&config, "dashboard", json!({}))?;
        if !document.save(&value)? {
            files::json_write(&config, &value)?;
        }
    } else if let Some(kind) = action.strip_prefix("focus-") {
        let data: Value = serde_json::from_str(if value.is_empty() { "{}" } else { value })?;
        focus_action(
            kind,
            data.get("minutes").unwrap_or(&json!(25)),
            data.get("dnd").unwrap_or(&json!(false)),
            now(),
        )?;
    } else if action == "capture" {
        if value.trim().is_empty() || value.chars().count() > 16000 {
            return Err(tr(
                "helper.dashboard-data.write-a-note-up-to16000characters",
                "Write a note up to16000characters",
            )
            .into());
        }
        process::checked(
            &brain(&paths),
            &[
                "note".into(),
                "--kind".into(),
                "inbox".into(),
                "--title".into(),
                value
                    .lines()
                    .next()
                    .unwrap_or("")
                    .chars()
                    .take(100)
                    .collect(),
                "--source".into(),
                "Nacre Focus quick capture".into(),
                "--confidence".into(),
                "reported".into(),
            ],
            Some(value),
            Duration::from_secs(8),
        )?;
    } else if !["read", "context"].contains(&action.as_str()) {
        return Err(tr(
            "helper.dashboard-data.unknown-dashboard-action",
            "Unknown dashboard action",
        )
        .into());
    }
    Ok(Some(snapshot(
        ["context", "capture"].contains(&action.as_str()),
    )?))
}
