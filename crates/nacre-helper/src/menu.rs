//! Bounded menu discovery, file/unit readers and package ownership.
use crate::{files, i18n::tr, paths::Paths, process, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use unicode_casefold::UnicodeCaseFold;
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
fn folded(value: &str) -> String {
    value.to_lowercase().case_fold().collect()
}
fn which(name: &str) -> bool {
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|root| {
        let path = root.join(name);
        path.is_file() && fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    })
}
fn run(program: &str, args: &[&str], seconds: u64) -> Result<std::process::Output> {
    process::run(
        program,
        &args.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
        None,
        Duration::from_secs(seconds),
    )
}
fn read(path: &Path, fallback: Value) -> Value {
    if fs::metadata(path).is_ok_and(|m| m.len() > 2_097_152) {
        return fallback;
    }
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(fallback)
}
fn regex(pattern: &str) -> regex::Regex {
    regex::Regex::new(pattern).expect("checked helper expression")
}
fn package_valid(value: &str) -> bool {
    static VALID: OnceLock<regex::Regex> = OnceLock::new();
    VALID
        .get_or_init(|| regex(r"\A[A-Za-z0-9][A-Za-z0-9@._+:-]{0,199}\z"))
        .is_match(value)
}
pub fn package_name(value: &str) -> Result<&str> {
    if package_valid(value) {
        Ok(value)
    } else {
        Err(tr(
            "helper.menu-backend.invalid-package-name",
            "Invalid package name",
        )
        .into())
    }
}
fn unit_valid(value: &str) -> bool {
    regex(r"\Anacre-reminder-[0-9a-f]{32}\.timer\z").is_match(value)
}
fn active(unit: &str) -> Result<bool> {
    match crate::systemd::active(unit) {
        Ok(active) => Ok(active),
        Err(error) => {
            eprintln!("Menu unit state {unit}: {error}");
            Ok(false)
        }
    }
}
pub fn installed() -> Result<Value> {
    if !which("pacman") {
        return Ok(json!([]));
    }
    let result = run("pacman", &["-Q"], 10)?;
    Ok(Value::Array(
        String::from_utf8(result.stdout)?
            .lines()
            .filter_map(|line| {
                let fields: Vec<_> = line.split_whitespace().collect();
                (fields.len() == 2).then(|| json!({"name":fields[0],"version":fields[1]}))
            })
            .take(10000)
            .collect(),
    ))
}
pub fn fonts() -> Result<Value> {
    if !which("fc-list") {
        return Ok(json!([]));
    }
    let result = run("fc-list", &[":lang=en", "--format", "%{family}\n"], 5)?;
    let names: BTreeSet<_> = String::from_utf8(result.stdout)?
        .lines()
        .flat_map(|line| line.split(','))
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(str::to_owned)
        .collect();
    let mut names: Vec<_> = names.into_iter().collect();
    names.sort_by_key(|name| folded(name));
    names.truncate(1000);
    Ok(json!(names))
}
pub fn webapps() -> Result<Value> {
    let paths = Paths::environment()?;
    let data = read(&paths.state.join("menu/webapps.json"), json!([]));
    let id = regex(r"\A[0-9a-f]{32}\z");
    Ok(json!(data
        .as_array()
        .map(|rows| rows
            .iter()
            .filter(|row| row.is_object()
                && row["id"].as_str().is_some_and(|v| id.is_match(v))
                && row["label"].is_string())
            .take(500)
            .cloned()
            .collect::<Vec<_>>())
        .unwrap_or_default()))
}
pub fn reminders() -> Result<Value> {
    let paths = Paths::environment()?;
    let data = read(&paths.state.join("menu/reminders.json"), json!([]));
    let now = now();
    Ok(json!(data
        .as_array()
        .map(|rows| rows
            .iter()
            .filter(|row| row.is_object()
                && row["unit"].as_str().is_some_and(unit_valid)
                && row["due"].as_f64().is_some_and(|due| due > now))
            .take(100)
            .cloned()
            .collect::<Vec<_>>())
        .unwrap_or_default()))
}
pub fn snapshot(force: bool) -> Result<Value> {
    let paths = Paths::environment()?;
    let stamp = fs::metadata("/var/lib/pacman/local")
        .map(|meta| meta.mtime() * 1_000_000_000 + meta.mtime_nsec())
        .unwrap_or(0);
    snapshot_with(&paths, force, stamp, json!(now()), || {
        let capture = which("grim") && which("slurp");
        let caps = json!({"capture":capture,"record":which("wf-recorder")&&which("slurp"),"ocr":which("tesseract")&&capture,"qr":which("zbarimg")&&capture,"color-picker":which("hyprpicker"),"packages":which("pacman"),"aur":which("paru")||which("yay"),"ai":which("nacre-ai"),"brain":paths.share.join("brain/control.py").is_file(),"fonts":which("fc-list")});
        Ok(
            json!({"capabilities":caps,"packages":installed()?,"fonts":fonts()?,"stayAwake":active("nacre-stay-awake.service")?,"recording":active("nacre-screen-recording.service")?,"reminders":reminders()?,"webapps":webapps()?}),
        )
    })
}
fn snapshot_with(
    paths: &Paths,
    force: bool,
    stamp: i64,
    at: Value,
    discover: impl FnOnce() -> Result<Value>,
) -> Result<Value> {
    let path = paths.cache.join("menu/state.json");
    let cached = read(&path, json!({}));
    if !force
        && cached["version"] == 1
        && cached["packageStamp"] == stamp
        && cached["capabilities"].is_object()
        && cached["packages"].is_array()
        && cached["created"].as_f64().is_some_and(|created| {
            let age = at.as_f64().unwrap_or(f64::NAN) - created;
            (0.0..30.0).contains(&age)
        })
    {
        return Ok(cached);
    }
    let facts = discover()?;
    let mut value = json!({"version":1,"packageStamp":stamp,"created":at});
    value
        .as_object_mut()
        .ok_or("Invalid menu snapshot")?
        .extend(facts.as_object().ok_or("Invalid menu discovery")?.clone());
    let parent = path.parent().unwrap();
    fs::create_dir_all(parent)?;
    fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    files::json_write(&path, &value)?;
    Ok(value)
}

pub fn search_files(query: &str) -> Result<Value> {
    if query.chars().count() > 160 || query.chars().any(|v| (v as u32) < 32) {
        return Err(tr(
            "helper.menu-backend.invalid-file-search",
            "Invalid file search",
        )
        .into());
    }
    if query.trim().chars().count() < 2 {
        return Ok(
            json!({"rows":[],"message":tr("helper.menu-backend.type-two-or-more-characters-to-find-files","Type two or more characters to find files")}),
        );
    }
    let paths = Paths::environment()?;
    let roots: Vec<PathBuf> = ["Documents", "Downloads", "Pictures", "Desktop", "Projects"]
        .iter()
        .map(|name| paths.home.join(name))
        .filter(|root| root.is_dir())
        .collect();
    if roots.is_empty() {
        return Ok(json!({"rows":[]}));
    }
    if !which("fd") {
        return Ok(
            json!({"rows":[],"message":tr("helper.menu-backend.install-fd-to-search-files","Install fd to search files")}),
        );
    }
    let mut args: Vec<String> = [
        "--type",
        "f",
        "--fixed-strings",
        "--ignore-case",
        "--max-results",
        "50",
        "--print0",
        "--",
        query,
    ]
    .iter()
    .map(|v| v.to_string())
    .collect();
    args.extend(roots.iter().map(|root| root.to_string_lossy().into_owned()));
    let output = process::run("fd", &args, None, Duration::from_secs(2))?;
    let mut rows = Vec::new();
    for value in String::from_utf8(output.stdout)?.split('\0') {
        if value.is_empty() || value.chars().any(|v| (v as u32) < 32) {
            continue;
        }
        let path = Path::new(value);
        if !roots.iter().any(|root| path.starts_with(root)) {
            continue;
        }
        rows.push(json!({"path":value,"name":path.file_name().unwrap_or_default().to_string_lossy(),"id":format!("{:x}",Sha256::digest(value.as_bytes()))[..20]}));
        if rows.len() == 50 {
            break;
        }
    }
    Ok(json!({"rows":rows}))
}
pub fn convert_units(expression: &str) -> Result<Value> {
    if expression.chars().count() > 256
        || !regex(r"\A[+\-]?[0-9][A-Za-z0-9µ°% .+*/^()\-]*\z").is_match(expression)
        || !regex(r"\b(to|in)\b").is_match(expression)
    {
        return Ok(
            json!({"ok":false,"error":tr("helper.menu-backend.try-a-conversion-such-as-10-km-to-miles","Try a conversion such as 10 km to miles")}),
        );
    }
    if !which("qalc") {
        return Ok(
            json!({"ok":false,"error":tr("helper.menu-backend.install-libqalculate-for-unit-and-currency-conversions","Install libqalculate for unit and currency conversions")}),
        );
    }
    let result = run(
        "qalc",
        &[
            "--terse",
            "--defaults",
            "--nodatasets",
            "--time",
            "400",
            "--set",
            "update exchange rates 0",
            "--set",
            "save config 0",
            "--set",
            "save definitions 0",
            "--set",
            "save mode 0",
            "--set",
            "clear history 1",
            expression,
        ],
        2,
    )?;
    let value = String::from_utf8(result.stdout)?.trim().to_owned();
    if !result.status.success()
        || value.is_empty()
        || value.chars().count() > 2048
        || String::from_utf8_lossy(&result.stderr)
            .to_lowercase()
            .contains("error")
    {
        return Ok(
            json!({"ok":false,"error":tr("helper.menu-backend.could-not-convert-these-units","Could not convert these units")}),
        );
    }
    Ok(
        json!({"ok":true,"value":value,"description":tr("helper.menu-backend.enter-to-copy-currencies-use-stored-qalculate-rates","Enter to copy · currencies use stored Qalculate rates")}),
    )
}
pub fn ownership(id: &str) -> Result<Value> {
    if !regex(r"\A[A-Za-z0-9_.-]{1,200}\z").is_match(id) || !which("pacman") {
        return Ok(json!({"package":""}));
    }
    let name = if id.ends_with(".desktop") {
        id.into()
    } else {
        format!("{id}.desktop")
    };
    let paths = Paths::environment()?;
    if paths
        .home
        .join(".local/share/applications")
        .join(&name)
        .exists()
    {
        return Ok(json!({"package":""}));
    }
    let path = Path::new("/usr/share/applications").join(name);
    let result = run("pacman", &["-Qoq", "--", &path.to_string_lossy()], 5)?;
    let value = String::from_utf8(result.stdout)?.trim().to_owned();
    Ok(
        json!({"package":if result.status.success()&&package_valid(&value){value}else{String::new()}}),
    )
}
pub fn search_packages(query: &str) -> Result<Value> {
    let query = query.trim().chars().take(120).collect::<String>();
    if query.chars().count() < 2 {
        return Ok(
            json!({"rows":[],"message":tr("helper.menu-backend.type-at-least-two-characters-to-search-packages","Type at least two characters to search packages")}),
        );
    }
    let mut rows = Vec::new();
    if which("pacman") {
        let escaped = query
            .chars()
            .flat_map(|ch| {
                if "()[]{}?*+-|^$\\.&~# \t\n\r\u{b}\u{c}".contains(ch) {
                    vec!['\\', ch]
                } else {
                    vec![ch]
                }
            })
            .collect::<String>();
        let result = run("pacman", &["-Ss", "--", &escaped], 8)?;
        let output = String::from_utf8(result.stdout)?;
        let lines: Vec<_> = output.lines().collect();
        for (index, line) in lines.iter().enumerate() {
            if line.starts_with(' ') || !line.contains('/') {
                continue;
            }
            let Some(first) = line.split_whitespace().next() else {
                continue;
            };
            let Some((repo, name)) = first.split_once('/') else {
                continue;
            };
            if package_valid(name) {
                rows.push(json!({"name":name,"repository":repo,"description":lines.get(index+1).filter(|line|line.starts_with(' ')).map(|line|line.trim()).unwrap_or(""),"source":"repo"}));
            }
        }
    }
    let mut message = String::new();
    if which("paru") || which("yay") {
        let fetch = || -> Result<Value> {
            let quoted = query
                .as_bytes()
                .iter()
                .map(|byte| {
                    if byte.is_ascii_alphanumeric() || b"_.-~".contains(byte) {
                        char::from(*byte).to_string()
                    } else {
                        format!("%{byte:02X}")
                    }
                })
                .collect::<String>();
            let agent = ureq::Agent::config_builder()
                .timeout_global(Some(Duration::from_secs(3)))
                .build()
                .new_agent();
            let mut response = agent
                .get(format!(
                    "https://aur.archlinux.org/rpc/v5/search/{quoted}?by=name-desc"
                ))
                .header("User-Agent", "Nacre-menu/1")
                .call()?;
            let bytes = response
                .body_mut()
                .with_config()
                .limit(2_097_152)
                .read_to_vec()?;
            Ok(serde_json::from_slice(&bytes)?)
        };
        match fetch() {
            Ok(data) => {
                let mut seen: HashSet<String> = rows
                    .iter()
                    .map(|row| row["name"].as_str().unwrap().to_owned())
                    .collect();
                if let Some(entries) = data["results"].as_array() {
                    for row in entries {
                        let name = row["Name"].as_str().unwrap_or("");
                        if package_valid(name) && seen.insert(name.into()) {
                            rows.push(json!({"name":name,"repository":"AUR","description":row["Description"].as_str().unwrap_or(""),"source":"aur"}));
                        }
                    }
                }
            }
            Err(_) => {
                message =
                    "Community repository unavailable; official results are still shown".into()
            }
        }
    }
    let folded_query = folded(&query);
    rows.sort_by_key(|row| {
        let name = row["name"].as_str().unwrap();
        let fold = folded(name);
        (
            fold != folded_query,
            !fold.starts_with(&folded_query),
            row["source"] == "aur",
            name.to_owned(),
        )
    });
    rows.truncate(60);
    Ok(json!({"rows":rows,"message":message}))
}
/// Launch a saved web app through the user's existing desktop application broker.
pub fn launch_webapp(id: &str) -> Result<()> {
    let rows = webapps()?;
    let row = rows
        .as_array()
        .and_then(|rows| rows.iter().find(|row| row["id"] == id))
        .ok_or_else(|| {
            tr(
                "helper.menu-backend.web-app-no-longer-exists",
                "Web app no longer exists",
            )
        })?;
    let url = row
        .get("url")
        .ok_or("'url'")?
        .as_str()
        .ok_or("Invalid saved website")?;
    let scheme = url
        .trim_start_matches(|ch: char| (ch as u32) <= 32)
        .split_once(':')
        .map(|(scheme, _)| scheme.to_ascii_lowercase())
        .unwrap_or_default();
    if !["http", "https"].contains(&scheme.as_str()) {
        return Err(tr(
            "helper.menu-backend.invalid-saved-website",
            "Invalid saved website",
        )
        .into());
    }
    let browser = ["google-chrome-stable", "google-chrome", "chromium"]
        .iter()
        .find_map(|name| {
            std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .map(|root| root.join(name))
                .find(|path| {
                    path.is_file()
                        && fs::metadata(path)
                            .is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
                })
        });
    let mut arguments = vec!["app".into(), "--".into()];
    if let Some(browser) = browser {
        arguments.extend([
            browser.to_string_lossy().into_owned(),
            format!("--app={url}"),
        ]);
    } else {
        arguments.extend(["nacre-app".into(), "browser".into(), url.into()]);
    }
    process::spawn_detached("uwsm", &arguments)?;
    Ok(())
}

pub fn execute(arguments: &[String], input: Option<Value>) -> Result<Option<Value>> {
    if let [action] = arguments {
        if action == "webapp-create" || action == "reminder" {
            let data = input.ok_or("Missing action input")?;
            let result = if action == "webapp-create" {
                crate::menu_registry::create_webapp(&data["label"], &data["url"])?
            } else {
                crate::menu_registry::set_reminder(&data["minutes"], &data["message"])?
            };
            return Ok(Some(result));
        }
    }
    if input.is_some() {
        return Err("Unsupported helper operation".into());
    }
    let result = match arguments {
        [action] if action == "state" => snapshot(false)?,
        [action, force] if action == "state" && force == "--force" => snapshot(true)?,
        [action, query] if action == "files" => search_files(query)?,
        [action, query] if action == "convert" => convert_units(query)?,
        [action, query] if action == "packages" => search_packages(query)?,
        [action, id] if action == "ownership" => ownership(id)?,
        [action] if action == "installed" => json!({"rows":installed()?}),
        [action] if action == "fonts" => json!({"rows":fonts()?}),
        [action, id] if action == "webapp-remove" => crate::menu_registry::remove_webapp(id)?,
        [action, unit] if action == "cancel-reminder" => {
            crate::menu_registry::cancel_reminder(unit)?
        }
        [action, name] if action == "tool" => {
            if [
                "record-start",
                "record-stop",
                "stay-awake",
                "focus-timer",
                "support-report",
            ]
            .contains(&name.as_str())
            {
                let message = match name.as_str() {
                    "record-start" => crate::menu_actions::record_start()?,
                    "record-stop" => crate::menu_actions::record_stop()?,
                    "stay-awake" => crate::menu_actions::stay_awake()?,
                    "support-report" => crate::menu_actions::support_report()?,
                    _ => {
                        crate::menu_registry::set_reminder(&json!(25), &json!("Time for a break"))?
                            ["message"]
                            .as_str()
                            .ok_or("Missing reminder message")?
                            .to_string()
                    }
                };
                if !message.is_empty() {
                    let application = if name.starts_with("record-") {
                        "Nacre Capture"
                    } else {
                        "Nacre"
                    };
                    let status = std::process::Command::new("notify-send")
                        .args([
                            "-a",
                            application,
                            "-i",
                            "nacre-notifications",
                            "Nacre",
                            &message,
                        ])
                        .status()?;
                    if !status.success() {
                        eprintln!("Menu notification failed: {status}");
                    }
                }
                return Ok(Some(json!({"message":message})));
            }
            if name.starts_with("capture-")
                && [
                    "capture-text",
                    "capture-qr",
                    "capture-window",
                    "capture-screen",
                    "capture-area",
                ]
                .contains(&name.as_str())
            {
                let message = if name == "capture-text" || name == "capture-qr" {
                    crate::menu_actions::read_screen(if name == "capture-text" {
                        "text"
                    } else {
                        "qr"
                    })?
                } else {
                    crate::menu_actions::capture(name)?
                };
                if !message.is_empty() {
                    let status = std::process::Command::new("notify-send")
                        .args([
                            "-a",
                            "Nacre Capture",
                            "-i",
                            "nacre-notifications",
                            "Nacre",
                            &message,
                        ])
                        .status()?;
                    if !status.success() {
                        eprintln!("Capture notification failed: {status}");
                    }
                }
                return Ok(Some(json!({"message":message})));
            }
            if name != "color-picker" {
                return Err(
                    tr("helper.menu-backend.unknown-menu-tool", "Unknown menu tool").into(),
                );
            }
            let picked = std::process::Command::new("hyprpicker")
                .arg("-a")
                .status()?;
            if !picked.success() {
                return Err("Color picker failed".into());
            }
            let message = tr("helper.menu-backend.color-copied", "Color copied");
            let notified = std::process::Command::new("notify-send")
                .args([
                    "-a",
                    "Nacre Capture",
                    "-i",
                    "nacre-notifications",
                    "Nacre",
                    &message,
                ])
                .status()?;
            if !notified.success() {
                eprintln!("Capture notification failed: {notified}");
            }
            json!({"message":message})
        }
        [action, id] if action == "webapp-launch" => {
            launch_webapp(id)?;
            json!({"message":tr("helper.menu-backend.web-app-opened","Web app opened")})
        }
        _ => return Err("Unsupported helper operation".into()),
    };
    Ok(Some(result))
}

#[cfg(test)]
mod cache_tests {
    use super::*;
    #[test]
    fn frozen_cache_hit_stamp_expiry_clock_and_force_contracts() -> Result<()> {
        let reference: Value =
            serde_json::from_str(include_str!("../tests/fixtures/menu-cache-reference.json"))?;
        for case in reference["cases"].as_array().ok_or("Missing cache cases")? {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            let mut calls = Vec::new();
            let mut results = Vec::new();
            for index in 0..2 {
                let expected = &case["results"][index];
                let result = snapshot_with(
                    &paths,
                    case["force"][index].as_bool().ok_or("Missing force")?,
                    case["stamps"][index]
                        .as_i64()
                        .ok_or("Missing package stamp")?,
                    case["times"][index].clone(),
                    || {
                        calls.extend(["installed", "fonts"]);
                        let mut facts = expected
                            .as_object()
                            .ok_or("Missing discovery fixture")?
                            .clone();
                        for key in ["version", "packageStamp", "created"] {
                            facts.shift_remove(key);
                        }
                        // Package changes expose newly available tools on discovery;
                        // a cache hit must preserve the first result without this call.
                        Ok(Value::Object(facts))
                    },
                )?;
                results.push(result);
            }
            assert_eq!(json!(results), case["results"], "{}", case["name"]);
            assert_eq!(json!(calls), case["calls"], "{}", case["name"]);
            let cache = paths.cache.join("menu/state.json");
            assert_eq!(
                fs::read_to_string(&cache)?,
                case["cache"].as_str().ok_or("Missing cache bytes")?
            );
            assert_eq!(
                format!("0o{:o}", fs::metadata(cache)?.permissions().mode() & 0o777),
                case["mode"].as_str().ok_or("Missing cache mode")?
            );
        }
        Ok(())
    }
}
