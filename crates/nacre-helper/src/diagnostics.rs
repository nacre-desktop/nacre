//! Read-only support facts and the existing recursive export privacy boundary.
use crate::{json, Result};
use fancy_regex::{Regex, RegexBuilder};
use serde_json::{Map, Value};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::OnceLock,
};

fn read_fact(path: &Path) -> String {
    let read = (|| -> Result<String> {
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(262_144)
            .read_to_end(&mut bytes)?;
        Ok(String::from_utf8(bytes)?
            .chars()
            .take(65_536)
            .collect::<String>()
            .trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
            .to_owned())
    })();
    match read {
        Ok(text) => text,
        Err(error) => {
            if !error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            {
                eprintln!("Support fact read failed: {error}");
            }
            String::new()
        }
    }
}
fn children(path: &Path) -> Result<Vec<PathBuf>> {
    match fs::read_dir(path) {
        Ok(entries) => entries
            .map(|entry| entry.map(|e| e.path()).map_err(Into::into))
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(error) => Err(error.into()),
    }
}
pub fn hardware(root: &Path) -> Result<Value> {
    let battery = children(&root.join("class/power_supply"))?
        .iter()
        .any(|p| read_fact(&p.join("type")) == "Battery");
    let chassis = read_fact(&root.join("class/dmi/id/chassis_type"));
    let kind = if ["8", "9", "10", "14", "30", "31", "32"].contains(&chassis.as_str()) {
        "laptop"
    } else if ["3", "4", "5", "6", "7", "13", "15", "16"].contains(&chassis.as_str()) {
        "desktop"
    } else if battery {
        "laptop (inferred from battery)"
    } else {
        "unknown"
    };
    let interfaces = children(&root.join("class/net"))?;
    let wireless = |path: &Path| path.join("wireless").exists() || path.join("phy80211").exists();
    Ok(
        serde_json::json!({"formFactor":kind,"chassisType":if chassis.is_empty(){Value::Null}else{Value::String(chassis)},"battery":battery,
        "backlight":!children(&root.join("class/backlight"))?.is_empty(),
        "bluetooth":children(&root.join("class/bluetooth"))?.iter().any(|p|p.file_name().is_some_and(|n|n.to_string_lossy().starts_with("hci"))),
        "wifi":interfaces.iter().any(|p|wireless(p)),
        "ethernet":interfaces.iter().any(|p|p.file_name().is_some_and(|n|n!="lo") && read_fact(&p.join("type"))=="1" && !wireless(p))}),
    )
}

/// Logical dependency boundary shared by collector and maintenance reads.
/// Production implementations may use D-Bus/socket APIs for these operations.
#[derive(Clone)]
pub struct Call {
    pub args: Vec<String>,
    pub timeout: u64,
    pub stdin: Option<i32>,
    pub capture_output: bool,
    pub text: bool,
    pub locale: Option<&'static str>,
    pub no_color: Option<&'static str>,
}
pub trait Probe {
    fn call(&mut self, call: Call) -> Result<std::process::Output>;
    fn configuration_errors(&mut self) -> Result<String>;
    fn gpu_inventory(&mut self, root: &Path) -> Result<Value>;
    fn executable(&self, name: &str) -> Option<PathBuf>;
}

pub struct Context {
    pub paths: crate::paths::Paths,
    pub checked: String,
    pub epoch: f64,
    pub backend: String,
    pub version_command: Vec<String>,
    pub runtime_command: Vec<String>,
    pub portal: String,
}
pub struct Platform {
    pub sys: PathBuf,
    pub etc: PathBuf,
    pub proc: PathBuf,
    pub kernel: String,
    pub architecture: String,
    pub cpus: usize,
    pub user: String,
    pub hostname: String,
    pub additional_identities: Vec<String>,
    pub commands: Map<String, Value>,
    pub required_commands: Vec<String>,
}
const PACKAGES: &[&str] = &[
    "hyprland",
    "uwsm",
    "python",
    "python-pip",
    "python-pillow",
    "quickshell",
    "pipewire",
    "wireplumber",
    "networkmanager",
    "bluez",
    "librsvg",
    "qt6-imageformats",
    "qt6-multimedia",
    "qt6-multimedia-ffmpeg",
    "ddcutil",
    "cpptrace",
    "libdwarf",
    "wtype",
    "papirus-icon-theme",
    "kitty",
    "dolphin",
    "kio-extras",
    "qt6ct",
    "ark",
    "hypridle",
    "hyprlock",
    "grim",
    "slurp",
    "wl-clipboard",
    "libnotify",
    "brightnessctl",
    "ffmpeg",
    "fontconfig",
];
const UNITS: &[&str] = &[
    "nacre-shell.service",
    "nacre-sidebar-ai.service",
    "nacre-brain.service",
    "nacre-session-watch.service",
    "nacre-brain-sync.service",
];
pub fn collect(context: &Context, platform: &Platform, queries: &mut impl Probe) -> Result<Value> {
    let command = |name: &str| -> Result<Vec<String>> {
        platform
            .commands
            .get(name)
            .and_then(Value::as_array)
            .ok_or("Missing compositor diagnostic command")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| "Invalid compositor command".into())
            })
            .collect::<Result<_>>()
    };
    let mut distro = Map::new();
    for line in read_fact(&platform.etc.join("os-release")).lines() {
        if let Some((key, value)) = line.split_once('=') {
            if ["ID", "VERSION_ID", "PRETTY_NAME"].contains(&key) {
                distro.insert(key.into(), Value::String(value.trim_matches('"').into()));
            }
        }
    }
    let mut versions = Map::new();
    versions.insert(
        "compositor".into(),
        probe(&command("runningVersion")?, queries)?,
    );
    versions.insert(
        format!("{}Installed", context.backend),
        probe(&command("installedVersion")?, queries)?,
    );
    versions.insert(
        "Quickshell".into(),
        probe(&["quickshell".into(), "--version".into()], queries)?,
    );
    let qt = queries
        .executable("qtpaths6")
        .unwrap_or_else(|| PathBuf::from("/usr/lib/qt6/bin/qtpaths"));
    versions.insert(
        "Qt".into(),
        probe(
            &[
                qt.to_str().ok_or("Invalid Qt tool path")?.into(),
                "--qt-version".into(),
            ],
            queries,
        )?,
    );
    versions.insert("Python".into(), probe(&context.runtime_command, queries)?);
    let mut gpu = queries.gpu_inventory(&platform.sys)?;
    let pci = regex::Regex::new(r"\A[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]\z")
        .expect("PCI diagnostic identity");
    for row in gpu.as_array_mut().ok_or("Invalid GPU inventory")? {
        let id = row["id"].as_str().ok_or("Missing GPU ID")?.to_owned();
        if pci.is_match(&id) {
            row["model"] = probe(&["lspci".into(), "-s".into(), id], queries)?["value"].clone();
        }
        if let Some(driver) = row["driver"].as_str().filter(|v| !v.is_empty()) {
            let version = probe(
                &[
                    "modinfo".into(),
                    "-F".into(),
                    "version".into(),
                    driver.into(),
                ],
                queries,
            )?;
            row["driverVersion"] = version["value"]
                .as_str()
                .filter(|v| !v.is_empty())
                .map(|s| Value::String(s.into()))
                .unwrap_or(Value::String(platform.kernel.clone()));
        }
    }
    let monitor = probe(&command("allMonitors")?, queries)?;
    let audio = probe(&["pw-dump".into()], queries)?;
    let installed = probe(&["pacman".into(), "-Q".into()], queries)?;
    let packages = installed["value"]
        .as_str()
        .unwrap_or("")
        .lines()
        .filter_map(|line| {
            let words = line.split_whitespace().collect::<Vec<_>>();
            (words.len() == 2).then(|| words[0].to_owned())
        })
        .collect::<BTreeSet<_>>();
    let cpu = read_fact(&platform.proc.join("cpuinfo"))
        .lines()
        .find_map(|line| {
            line.split_once(':')
                .filter(|(key, _)| ["model name", "Hardware"].contains(&key.trim()))
                .map(|(_, value)| value.trim().to_owned())
        })
        .unwrap_or(platform.architecture.clone());
    let mut args = vec!["systemctl".into(), "--user".into(), "show".into()];
    args.extend(UNITS.iter().map(|s| (*s).into()));
    for property in ["Id", "LoadState", "ActiveState", "Result", "ExecMainStatus"] {
        args.extend(["-p".into(), property.into()]);
    }
    let units = probe(&args, queries)?;
    let mut args = vec![
        "journalctl".into(),
        "--user".into(),
        "--no-pager".into(),
        "-o".into(),
        "cat".into(),
        "--since".into(),
        "1 hour ago".into(),
        "-p".into(),
        "warning".into(),
        "-n".into(),
        "80".into(),
    ];
    for unit in UNITS {
        args.extend(["-u".into(), (*unit).into()]);
    }
    let journal = probe(&args, queries)?;
    let shell = probe(
        &[
            "quickshell".into(),
            "log".into(),
            "--config".into(),
            "nacre".into(),
            "--no-color".into(),
            "--tail".into(),
            "60".into(),
        ],
        queries,
    )?;
    let config = probe(&command("configErrors")?, queries)?;
    let release = serde_json::from_str::<Value>(&read_fact(
        &context.paths.state.join("releases/current.json"),
    ))
    .ok()
    .and_then(|v| {
        v.as_object().map(|m| {
            Value::Object(
                ["revision", "status", "checked"]
                    .iter()
                    .map(|k| ((*k).into(), m.get(*k).cloned().unwrap_or(Value::Null)))
                    .collect(),
            )
        })
    })
    .unwrap_or_else(|| serde_json::json!({}));
    let mut hardware = hardware(&platform.sys)?;
    hardware
        .as_object_mut()
        .ok_or("Invalid hardware facts")?
        .shift_insert(0, "gpus".into(), gpu);
    hardware
        .as_object_mut()
        .ok_or("Invalid hardware facts")?
        .shift_insert(
            0,
            "cpu".into(),
            serde_json::json!({"model":cpu,"logicalCpus":platform.cpus}),
        );
    let missing_commands = platform
        .required_commands
        .iter()
        .filter(|name| queries.executable(name).is_none())
        .cloned()
        .collect::<Vec<_>>();
    let mut report = serde_json::json!({"schemaVersion":1,"checked":context.checked,"software":{"distro":distro,"kernel":platform.kernel,"architecture":platform.architecture,"compositor":if versions["compositor"]["status"]=="ok"{context.backend.as_str()}else{"unavailable"},"versions":versions,"release":release},"hardware":hardware,"monitors":{"status":monitor["status"],"devices":monitors(monitor["value"].as_str().unwrap_or(""))},"audio":{"status":audio["status"],"devices":audio_devices(audio["value"].as_str().unwrap_or(""))},"dependencies":{"packageManager":if installed["status"]=="ok" {"pacman"}else{"unavailable (command checks only)"},"missingPackages":if installed["status"]=="ok"{Value::Array(PACKAGES.iter().filter(|name|!packages.contains(**name)).map(|name|Value::String((*name).into())).collect())}else{Value::Null},"missingCommands":missing_commands},"services":units,"maintenance":maintenance_facts(context,queries),"recentErrors":{"journal":journal,"shellLog":shell,"compositorConfig":config},"privacy":"Redacted locally. No account files, environment dump, Wi-Fi scan, uploads or privileged probes. Review free-form logs before sharing."});
    let release_size = probe(
        &[
            "du".into(),
            "-s".into(),
            "-B1".into(),
            context
                .paths
                .state
                .join("releases")
                .to_str()
                .ok_or("Invalid release path")?
                .into(),
        ],
        queries,
    )?;
    report["maintenance"]["releaseDiskBytes"] = release_size["value"]
        .as_str()
        .unwrap_or("")
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<u64>().ok())
        .map(Value::from)
        .unwrap_or(Value::Null);
    let mut names = BTreeSet::from([
        platform.user.clone(),
        platform.hostname.clone(),
        context.paths.home.to_string_lossy().into_owned(),
    ]);
    names.extend(platform.additional_identities.iter().cloned());
    names.extend(
        read_fact(&platform.etc.join("hostname"))
            .lines()
            .map(str::to_owned),
    );
    for args in [
        vec![
            "nmcli", "--terse", "--escape", "no", "--fields", "SSID", "device", "wifi", "list",
            "--rescan", "no",
        ],
        vec![
            "nmcli",
            "--terse",
            "--escape",
            "no",
            "--fields",
            "NAME",
            "connection",
            "show",
        ],
    ] {
        let result = probe(
            &args.iter().map(|s| (*s).into()).collect::<Vec<_>>(),
            queries,
        )?;
        names.extend(
            result["value"]
                .as_str()
                .unwrap_or("")
                .lines()
                .map(str::to_owned),
        );
    }
    names.remove("");
    names.remove("--");
    redact(&report, &names.into_iter().collect::<Vec<_>>())
}
fn json_file(path: &Path, fallback: Value) -> Result<Value> {
    crate::state::read_strict(path, fallback)
}
fn maintenance_run(args: Vec<String>, queries: &mut impl Probe) -> Result<String> {
    match queries.call(Call {
        args,
        timeout: 5,
        stdin: None,
        capture_output: false,
        text: true,
        locale: None,
        no_color: None,
    }) {
        Ok(output) if output.status.success() => {
            Ok(String::from_utf8(output.stdout)?.trim().to_owned())
        }
        Ok(_) => Ok(String::new()),
        Err(error) => {
            eprintln!("Maintenance query unavailable: {error}");
            Ok(String::new())
        }
    }
}
pub fn configuration_errors(queries: &mut impl Probe) -> String {
    match queries.configuration_errors() {
        Ok(value) => value,
        Err(error) => {
            eprintln!("Configuration query unavailable: {error}");
            String::new()
        }
    }
}
pub fn maintenance_state(context: &Context, queries: &mut impl Probe) -> Result<Value> {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::MetadataExt;
    let paths = &context.paths;
    let state = &paths.state;
    let mut drift = Vec::new();
    let managed = json_file(&state.join("configuration.json"), serde_json::json!({}))?;
    for (name, expected) in managed.as_object().ok_or("Invalid managed configuration")? {
        let path = paths.home.join(name);
        if !path.is_file()
            || format!("{:x}", Sha256::digest(fs::read(path)?)) != expected.as_str().unwrap_or("")
        {
            drift.push(name.clone());
        }
    }
    let current = json_file(&state.join("releases/current.json"), serde_json::json!({}))?;
    let versions = serde_json::json!({context.backend.clone():maintenance_run(context.version_command.clone(),queries)?,"Python":maintenance_run(context.runtime_command.clone(),queries)?});
    let services = [
        "nacre-shell.service",
        "nacre-sidebar-ai.service",
        "nacre-brain.service",
        "nacre-session-watch.service",
        "hypridle.service",
        "nacre-brain-sync.timer",
        "xdg-desktop-portal.service",
        context.portal.as_str(),
        "xdg-document-portal.service",
    ];
    let mut args = vec!["systemctl".into(), "--user".into(), "show".into()];
    args.extend(services.iter().map(|s| (*s).into()));
    for property in ["Id", "ActiveState", "Result", "ExecMainStatus"] {
        args.extend(["-p".into(), property.into()]);
    }
    let output = queries.call(Call {
        args,
        timeout: 5,
        stdin: None,
        capture_output: true,
        text: true,
        locale: None,
        no_color: None,
    })?;
    let text = String::from_utf8(output.stdout)?;
    let mut states = Map::new();
    let mut details = Map::new();
    for block in text.split("\n\n") {
        let values: Map<String, Value> = block
            .lines()
            .filter_map(|line| {
                line.split_once('=')
                    .map(|(key, value)| (key.into(), Value::String(value.into())))
            })
            .collect();
        let Some(name) = values.get("Id").and_then(Value::as_str).map(str::to_owned) else {
            continue;
        };
        let state = values
            .get("ActiveState")
            .and_then(Value::as_str)
            .unwrap_or("unavailable");
        let state = if name.starts_with("xdg-")
            && state == "inactive"
            && values
                .get("Result")
                .and_then(Value::as_str)
                .unwrap_or("success")
                != "exit-code"
        {
            "ready on demand"
        } else {
            state
        };
        states.insert(name.clone(), Value::String(state.into()));
        details.insert(name, Value::Object(values));
    }
    let states: Map<String, Value> = services
        .iter()
        .map(|name| {
            (
                (*name).into(),
                states
                    .get(*name)
                    .cloned()
                    .unwrap_or(Value::String("unavailable".into())),
            )
        })
        .collect();
    let failed = maintenance_run(
        vec![
            "systemctl".into(),
            "--user".into(),
            "--failed".into(),
            "--no-legend".into(),
            "--plain".into(),
        ],
        queries,
    )?;
    let config = configuration_errors(queries);
    let update = paths.cache.join("updates.json");
    let age = if update.exists() {
        let meta = fs::metadata(update)?;
        Value::from((context.epoch - (meta.mtime() as f64 + meta.mtime_nsec() as f64 / 1e9)) as i64)
    } else {
        Value::Null
    };
    Ok(
        serde_json::json!({"release":current,"services":states,"brainSync":json_file(&state.join("brain-sync.json"),serde_json::json!({"state":"unrecorded","message":"No synchronization attempt recorded"}))?,"serviceDetails":details,"failedServices":failed.lines().collect::<Vec<_>>(),"drift":drift,"configErrors":config,"updatesAgeSeconds":age,"session":json_file(&state.join("session/latest.json"),serde_json::json!({}))?,"performance":json_file(&state.join("performance/latest.json"),serde_json::json!({}))?,"versions":versions,"checked":context.checked}),
    )
}
pub fn maintenance_facts(context: &Context, queries: &mut impl Probe) -> Value {
    match maintenance_state(context, queries) {
        Ok(state) => Value::Object(
            [
                "services",
                "serviceDetails",
                "failedServices",
                "drift",
                "configErrors",
                "updatesAgeSeconds",
            ]
            .iter()
            .map(|key| ((*key).into(), state[*key].clone()))
            .collect(),
        ),
        Err(error) => {
            eprintln!("Maintenance facts unavailable: {error}");
            serde_json::json!({"status":"unavailable"})
        }
    }
}
pub fn probe(args: &[String], queries: &mut impl Probe) -> Result<Value> {
    let result = queries.call(Call {
        args: args.to_vec(),
        timeout: 3,
        stdin: Some(-3),
        capture_output: true,
        text: true,
        locale: Some("C"),
        no_color: Some("1"),
    });
    let output = match result {
        Ok(output) => output,
        Err(error) => {
            eprintln!("Support probe unavailable: {error}");
            return Ok(serde_json::json!({"status":"unavailable","value":null}));
        }
    };
    let text = match String::from_utf8(output.stdout) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("Support probe returned invalid text: {error}");
            return Ok(serde_json::json!({"status":"unavailable","value":null}));
        }
    };
    let trimmed =
        text.trim_matches(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c));
    let partial = args
        .iter()
        .take(3)
        .map(String::as_str)
        .eq(["systemctl", "--user", "show"])
        && !trimmed.is_empty();
    if !output.status.success() && !partial {
        return Ok(serde_json::json!({"status":"unavailable","value":null}));
    }
    // The historical limit is Unicode characters, not UTF-8 bytes.
    if text.chars().count() > 2 * 1024 * 1024 {
        return Ok(serde_json::json!({"status":"unavailable (output limit)","value":null}));
    }
    Ok(
        serde_json::json!({"status":if output.status.success(){"ok"}else{"partial"},"value":trimmed}),
    )
}

fn expression(pattern: &str) -> Result<Regex> {
    // Python's word class excludes combining marks and join controls.
    let word = r"\p{L}\p{N}_";
    let boundary = format!(r"(?:(?<![{word}])(?=[{word}])|(?<=[{word}])(?![{word}]))");
    let pattern = pattern.replace(r"\w", word).replace(r"\b", &boundary);
    Ok(RegexBuilder::new(&pattern)
        .backtrack_limit(1_000_000)
        .build()?)
}
fn substitute(pattern: &Regex, source: &str, replacement: &str) -> Result<String> {
    Ok(pattern.try_replacen(source, 0, replacement)?.into_owned())
}
fn addresses(pattern: &Regex, source: &str) -> Result<String> {
    let mut result = String::new();
    let mut end = 0;
    for matched in pattern.find_iter(source) {
        let matched = matched?;
        result.push_str(&source[end..matched.start()]);
        let candidate = matched.as_str();
        result.push_str(
            if candidate
                .split('%')
                .next()
                .unwrap_or(candidate)
                .parse::<IpAddr>()
                .is_ok()
            {
                "[IP]"
            } else {
                candidate
            },
        );
        end = matched.end();
    }
    result.push_str(&source[end..]);
    Ok(result)
}

pub fn redact(value: &Value, identities: &[String]) -> Result<Value> {
    static PATTERNS: OnceLock<Vec<Regex>> = OnceLock::new();
    let patterns = PATTERNS.get_or_init(|| [
        r"(?i)(?:token|secret|password|passwd|authorization|cookie|api.?key|ssid|username|hostname|serial|private.?key)",
        r"\x1b\[[0-?]*[ -/]*[@-~]",
        r"[\x00-\x08\x0b-\x1f\x7f]",
        r#"(?im)\b(?:[\w.-]*(?:authorization|token|secret|password|passwd|api[_-]?key|cookie|private[_-]?key)[\w.-]*|bearer|ssid|user(?:name)?|host(?:name)?|wi-?fi (?:name|network))\b["']?\s*[:= ]+[^\n]+"#,
        r"\b[A-Za-z][A-Za-z0-9+.-]*://[^\s]*@[^\s]+",
        r"[\w.+-]+@[\w.-]+",
        r"(?i)https?://[^\s]+",
        r"(?:/home/|/Users/)[^/\s]+|/root(?=/|\s|$)",
        r"(?i)\b(?:[0-9a-f]{2}[:-]){5}[0-9a-f]{2}\b",
        r"(?<![\w.])(?:\d{1,3}\.){3}\d{1,3}(?![\w.])",
        r"(?<![\w:])(?:[0-9a-fA-F]{0,4}:){2,}[0-9a-fA-F:.]+(?:%[\w.-]+)?",
        r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
        r"(?<![\w])[A-Za-z0-9_+/=-]{24,}(?![\w])",
    ].iter().map(|pattern| expression(pattern).expect("fixed support redaction pattern")).collect());
    let mut identities = identities
        .iter()
        .filter(|s| !s.is_empty())
        .cloned()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    identities.sort_by_key(|s| std::cmp::Reverse(s.chars().count()));
    let identities = identities
        .into_iter()
        .map(|value| {
            let pattern = if value.contains('/') {
                None
            } else {
                Some(expression(&format!(
                    r"(?i)(?<![\w-]){}(?![\w-])",
                    regex::escape(&value)
                ))?)
            };
            Ok((value, pattern))
        })
        .collect::<Result<Vec<_>>>()?;
    fn text(source: &str, p: &[Regex], identities: &[(String, Option<Regex>)]) -> Result<String> {
        let mut s = substitute(&p[1], source, "")?;
        s = substitute(&p[2], &s, "")?;
        for (index, replacement) in [
            (3, "[redacted sensitive value]"),
            (4, "[redacted URL]"),
            (5, "[account]"),
            (6, "[redacted URL]"),
            (7, "[home]"),
        ] {
            s = substitute(&p[index], &s, replacement)?;
        }
        for (identity, pattern) in identities {
            s = match pattern {
                Some(pattern) => substitute(pattern, &s, "[identity]")?,
                None => s.replace(identity, "[home]"),
            };
        }
        s = substitute(&p[8], &s, "[MAC]")?;
        s = addresses(&p[9], &s)?;
        s = addresses(&p[10], &s)?;
        s = substitute(&p[11], &s, "[token-like value]")?;
        substitute(&p[12], &s, "[token-like value]")
    }
    fn walk(value: &Value, p: &[Regex], identities: &[(String, Option<Regex>)]) -> Result<Value> {
        Ok(match value {
            Value::String(s) => Value::String(text(s, p, identities)?),
            Value::Array(values) => Value::Array(
                values
                    .iter()
                    .map(|v| walk(v, p, identities))
                    .collect::<Result<_>>()?,
            ),
            Value::Object(values) => {
                let mut result = Map::new();
                for (key, value) in values {
                    result.insert(
                        text(key, p, identities)?,
                        if p[0].is_match(key)? {
                            Value::String("[redacted]".into())
                        } else {
                            walk(value, p, identities)?
                        },
                    );
                }
                Value::Object(result)
            }
            value => value.clone(),
        })
    }
    walk(value, patterns, &identities)
}

fn label(key: &str) -> String {
    match key {
        "cpu" => "CPU".into(),
        "gpus" => "GPUs".into(),
        "Qt" => "Qt".into(),
        "Python" => "Python".into(),
        "Quickshell" => "Quickshell".into(),
        "deviceId" => "Device ID".into(),
        "logicalCpus" => "Logical CPUs".into(),
        "refreshRate" => "Refresh rate (Hz)".into(),
        "releaseDiskBytes" => "Release disk usage (bytes)".into(),
        "aiCredentials" => "AI credentials".into(),
        _ => {
            let mut spaced = String::new();
            let mut previous = None;
            for c in key.chars() {
                if previous.is_some_and(|p: char| p.is_ascii_lowercase() || p.is_ascii_digit())
                    && c.is_ascii_uppercase()
                {
                    spaced.push(' ');
                }
                spaced.push(c);
                previous = Some(c);
            }
            let mut chars = spaced.chars();
            chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase())
                .unwrap_or_default()
        }
    }
}
fn scalar(value: &Value, continuation: &str) -> String {
    if value.is_null() {
        return "Unavailable".into();
    }
    if let Some(value) = value.as_bool() {
        return if value { "Yes" } else { "No" }.into();
    }
    let value = json::python_str(value)
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    let mut result = String::new();
    for c in value.chars() {
        if r"\`*_[]{}".contains(c) {
            result.push('\\');
        }
        if c == '\n' {
            result.push('\n');
            result.push_str(continuation);
        } else {
            result.push(c);
        }
    }
    if result.is_empty() {
        "None reported".into()
    } else {
        result
    }
}
pub fn markdown(report: &Value) -> Result<String> {
    fn render(value: &Value, indent: &str, lines: &mut Vec<String>) {
        match value {
            Value::Object(values) => {
                for (key, child) in values {
                    if child.is_object() || child.is_array() {
                        lines.push(format!("{indent}- **{}**:", label(key)));
                        render(child, &format!("{indent}  "), lines);
                    } else {
                        lines.push(format!(
                            "{indent}- **{}**: {}",
                            label(key),
                            scalar(child, &format!("{indent}  "))
                        ));
                    }
                }
            }
            Value::Array(values) => {
                if values.is_empty() {
                    lines.push(format!("{indent}- None reported"));
                }
                for child in values {
                    if child.is_object() {
                        lines.push(format!("{indent}- Device:"));
                        render(child, &format!("{indent}  "), lines);
                    } else {
                        lines.push(format!(
                            "{indent}- {}",
                            scalar(child, &format!("{indent}  "))
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    let mut lines = vec![
        "# Nacre support report".into(),
        String::new(),
        format!(
            "Collected: {}",
            report["checked"].as_str().ok_or("Missing report time")?
        ),
        String::new(),
        report["privacy"]
            .as_str()
            .ok_or("Missing report privacy boundary")?
            .into(),
        String::new(),
    ];
    for key in [
        "software",
        "hardware",
        "monitors",
        "audio",
        "dependencies",
        "services",
        "maintenance",
        "aiCredentials",
        "recentErrors",
    ] {
        if key == "aiCredentials" && report.get(key).is_none() {
            continue;
        }
        lines.extend([format!("## {}", label(key)), String::new()]);
        render(&report[key], "", &mut lines);
        lines.push(String::new());
    }
    Ok(lines.join("\n"))
}

fn device_json(text: &str) -> Value {
    // Python parse_constant maps unquoted non-finite constants to None.
    let mut normalized = String::new();
    let mut at = 0;
    let mut quoted = false;
    let mut escaped = false;
    while at < text.len() {
        let remaining = &text[at..];
        let c = remaining.chars().next().expect("remaining character");
        if !quoted {
            if let Some(token) = ["-Infinity", "Infinity", "NaN"]
                .iter()
                .find(|token| remaining.starts_with(**token))
            {
                normalized.push_str("null");
                at += token.len();
                continue;
            }
        }
        normalized.push(c);
        at += c.len_utf8();
        if escaped {
            escaped = false;
        } else if quoted && c == '\\' {
            escaped = true;
        } else if c == '"' {
            quoted = !quoted;
        }
    }
    serde_json::from_str(&normalized).unwrap_or(Value::Null)
}
pub fn monitors(text: &str) -> Value {
    let value = device_json(text);
    Value::Array(
        value
            .as_array()
            .map(|rows| {
                rows.iter()
                    .take(32)
                    .filter_map(|row| {
                        let row = row.as_object()?;
                        let mut device = Map::new();
                        device.insert(
                            "name".into(),
                            row.get("name")
                                .filter(|v| v.is_string())
                                .cloned()
                                .unwrap_or(Value::String("Unknown".into())),
                        );
                        for key in [
                            "width",
                            "height",
                            "scale",
                            "refreshRate",
                            "x",
                            "y",
                            "transform",
                        ] {
                            device.insert(
                                key.into(),
                                row.get(key)
                                    .filter(|v| v.is_number())
                                    .cloned()
                                    .unwrap_or(Value::Null),
                            );
                        }
                        device.insert(
                            "disabled".into(),
                            row.get("disabled")
                                .filter(|v| v.is_boolean())
                                .cloned()
                                .unwrap_or(Value::Null),
                        );
                        Some(Value::Object(device))
                    })
                    .collect()
            })
            .unwrap_or_default(),
    )
}
pub fn audio_devices(text: &str) -> Value {
    let value = device_json(text);
    Value::Array(value.as_array().map(|rows|rows.iter().filter_map(|row| {
        let info=row.get("info")?.as_object()?;let props=info.get("props")?.as_object()?;
        let kind=props.get("media.class")?.as_str()?;
        if !["Audio/Sink","Audio/Source"].contains(&kind) {return None;}
        Some(serde_json::json!({"kind":kind,"description":props.get("node.description").filter(|v|v.is_string()).cloned().unwrap_or(Value::String("Unknown".into())),"api":props.get("device.api").filter(|v|v.is_string()).cloned().unwrap_or(Value::Null),"state":info.get("state").filter(|v|v.is_string()).cloned().unwrap_or(Value::Null)}))
    }).take(64).collect()).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn regex_work_limit_failure_is_returned_without_unredacted_output() -> Result<()> {
        let pattern = RegexBuilder::new("(x+x+)+(?>y)")
            .backtrack_limit(1)
            .build()?;
        assert!(substitute(&pattern, "xxxxxxxxxxy", "[redacted]").is_err());
        Ok(())
    }
    fn reference(name: &str) -> Result<Value> {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
            "../../tests/parity/helpers/diagnostics/expected-support-{name}.json"
        ));
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }
    #[test]
    fn frozen_markdown_preserves_escaping_order_and_trailing_newline() -> Result<()> {
        let report = reference("collect")?;
        assert_eq!(
            markdown(&report["stdout"]["value"])?,
            reference("markdown")?["stdout"]["value"]
                .as_str()
                .ok_or("Missing markdown reference")?
        );
        Ok(())
    }
    #[test]
    fn frozen_device_filtering_preserves_fields_and_discards_malformed_values() -> Result<()> {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/parity/helpers/diagnostics");
        for name in ["collect", "malformed-devices"] {
            let case: Value = serde_json::from_slice(&std::fs::read(
                folder.join(format!("case-support-{name}.json")),
            )?)?;
            let request: Value = serde_json::from_str(
                case["args"][0]
                    .as_str()
                    .ok_or("Missing collector request")?,
            )?;
            let expected = reference(name)?;
            assert_eq!(
                redact(
                    &monitors(
                        request["outputs"]["allMonitors"]["stdout"]
                            .as_str()
                            .ok_or("Missing monitor payload")?
                    ),
                    &["fixture-user".into()]
                )?,
                expected["stdout"]["value"]["monitors"]["devices"]
            );
            assert_eq!(
                redact(
                    &audio_devices(
                        request["outputs"]["audio"]["stdout"]
                            .as_str()
                            .ok_or("Missing audio payload")?
                    ),
                    &["fixture-user".into()]
                )?,
                expected["stdout"]["value"]["audio"]["devices"]
            );
        }
        assert_eq!(monitors("{}"), Value::Array(vec![]));
        Ok(())
    }
    #[test]
    fn frozen_recursive_redaction_keeps_public_metrics_and_exact_text() -> Result<()> {
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/parity/helpers/diagnostics");
        for name in [
            "redact-full",
            "redact-short",
            "redact-nested",
            "redact-public",
        ] {
            let case: Value = serde_json::from_slice(&std::fs::read(
                folder.join(format!("case-support-{name}.json")),
            )?)?;
            let request: Value = serde_json::from_str(
                case["args"][0]
                    .as_str()
                    .ok_or("Missing reference request")?,
            )?;
            let expected: Value = serde_json::from_slice(&std::fs::read(
                folder.join(format!("expected-support-{name}.json")),
            )?)?;
            let identities = request["identities"]
                .as_array()
                .map(|values| {
                    values
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            assert_eq!(
                redact(&request["value"], &identities)?,
                expected["stdout"]["value"],
                "{name}"
            );
        }
        Ok(())
    }
}
