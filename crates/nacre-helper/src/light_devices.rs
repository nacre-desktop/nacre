//! Bounded light discovery and identity-checked brightness tool requests.
use crate::{i18n, json, process, Result};
use clap::Parser;
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs, io,
    path::{Path, PathBuf},
    time::Duration,
};

fn message(id: &str, fallback: &str) -> String {
    i18n::tr(&format!("helper.light-devices.{id}"), fallback)
}

fn entries(directory: &Path) -> Vec<PathBuf> {
    match fs::read_dir(directory) {
        Ok(entries) => {
            let mut paths = Vec::new();
            for entry in entries {
                match entry {
                    Ok(entry) => paths.push(entry.path()),
                    Err(error) => eprintln!("Light device discovery: {error}"),
                }
            }
            paths.sort();
            paths
        }
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                eprintln!("Light device discovery: {error}");
            }
            Vec::new()
        }
    }
}

fn integer(path: &Path) -> Option<i64> {
    match fs::read_to_string(path) {
        Ok(value) => match value.trim().parse::<i64>() {
            Ok(value) if value >= 0 => Some(value),
            _ => {
                eprintln!("Light device value: invalid nonnegative integer");
                None
            }
        },
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                eprintln!("Light device value: {error}");
            }
            None
        }
    }
}

pub fn sys_device(path: &Path, kind: &str) -> Option<Value> {
    let maximum = integer(&path.join("max_brightness"))?;
    let current = integer(&path.join("brightness"))?;
    if maximum == 0 || current > maximum {
        return None;
    }
    let device = path.file_name()?.to_string_lossy();
    Some(json!({"kind":kind,"device":device,"maximum":maximum,"current":current}))
}

fn fingerprint(connector: &Path) -> String {
    match fs::read(connector.join("edid")) {
        Ok(bytes) if !bytes.is_empty() => format!("{:x}", Sha256::digest(bytes)),
        Ok(_) => String::new(),
        Err(error) => {
            if error.kind() != io::ErrorKind::NotFound {
                eprintln!("Display identity: {error}");
            }
            String::new()
        }
    }
}

fn resolved_name(path: &Path) -> String {
    let mut target = path.to_path_buf();
    for _ in 0..40 {
        if let Ok(resolved) = fs::canonicalize(&target) {
            target = resolved;
            break;
        }
        let link = match fs::read_link(&target) {
            Ok(link) => link,
            Err(error) => {
                if !matches!(
                    error.kind(),
                    io::ErrorKind::NotFound | io::ErrorKind::InvalidInput
                ) {
                    eprintln!("Display bus link: {error}");
                }
                break;
            }
        };
        target = if link.is_absolute() {
            link
        } else {
            target.parent().unwrap_or(Path::new("/")).join(link)
        };
    }
    target
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn native_run(program: &str, arguments: &[String], timeout: Duration) -> Result<String> {
    let output = process::checked(program, arguments, None, timeout)?;
    Ok(String::from_utf8(output.stdout)?)
}

pub fn parse_ddc(output: &str) -> Result<(i64, i64)> {
    let expression = Regex::new(r"(?m)^VCP\s+10\s+C\s+(\d+)\s+(\d+)\s*$")?;
    if let Some(captures) = expression.captures(output) {
        let current: i64 = captures[1].parse()?;
        let maximum: i64 = captures[2].parse()?;
        if maximum > 0 && current <= maximum {
            return Ok((current, maximum));
        }
    }
    Err(message(
        "display-brightness-response-was-invalid",
        "Display brightness response was invalid",
    )
    .into())
}

fn ddc_read(
    bus: i64,
    run: &mut impl FnMut(&str, &[String], Duration) -> Result<String>,
) -> Result<(i64, i64)> {
    parse_ddc(&run(
        "ddcutil",
        &[
            "--bus".into(),
            bus.to_string(),
            "--brief".into(),
            "getvcp".into(),
            "10".into(),
        ],
        Duration::from_secs(6),
    )?)
}

pub fn discover(
    screens: &[Value],
    root: &Path,
    run: &mut impl FnMut(&str, &[String], Duration) -> Result<String>,
) -> Result<Value> {
    let backlight = entries(&root.join("class/backlight"))
        .iter()
        .find_map(|path| sys_device(path, "backlight"));
    let keyboard = entries(&root.join("class/leds"))
        .iter()
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains("kbd_backlight"))
        })
        .find_map(|path| sys_device(path, "keyboard"));
    let valid_screen = Regex::new(r"\A[A-Za-z0-9_.-]{1,64}\z")?;
    let valid_bus = Regex::new(r"\Ai2c-(\d+)\z")?;
    let mut externals = Vec::new();
    for screen in screens {
        let Some(screen) = screen.as_str().filter(|name| valid_screen.is_match(name)) else {
            continue;
        };
        let lower = screen.to_ascii_lowercase();
        if lower.starts_with("edp") || lower.starts_with("lvds") {
            continue;
        }
        let mut matches = Vec::new();
        for connector in entries(&root.join("class/drm")) {
            let name = connector.file_name().unwrap_or_default().to_string_lossy();
            if !name.starts_with("card") || !name.ends_with(&format!("-{screen}")) {
                continue;
            }
            let connected = match fs::read_to_string(connector.join("status")) {
                Ok(status) => status.trim() == "connected",
                Err(error) => {
                    if error.kind() != io::ErrorKind::NotFound {
                        eprintln!("Display connection status: {error}");
                    }
                    false
                }
            };
            if connected {
                if let Some(bus) = valid_bus.captures(&resolved_name(&connector.join("ddc"))) {
                    matches.push((connector, bus[1].parse::<i64>()?));
                }
            }
        }
        if matches.len() != 1 {
            continue;
        }
        let (connector, bus) = &matches[0];
        let identity = fingerprint(connector);
        if identity.is_empty() {
            continue;
        }
        match ddc_read(*bus, run) {
            Ok((current, maximum)) => externals.push(json!({"kind":"ddc","screen":screen,"bus":bus,"connector":connector.file_name().unwrap_or_default().to_string_lossy(),"identity":identity,"current":current,"maximum":maximum})),
            Err(error) => eprintln!("Display brightness discovery: {error}"),
        }
    }
    Ok(json!({"backlight":backlight,"keyboard":keyboard,"ddc":externals}))
}

fn equals_integer(value: &Value, expected: i64) -> bool {
    match value {
        Value::Bool(value) => i64::from(*value) == expected,
        Value::Number(value) => value.as_f64() == Some(expected as f64),
        _ => false,
    }
}

pub fn write(
    descriptor: &Value,
    value: i64,
    root: &Path,
    run: &mut impl FnMut(&str, &[String], Duration) -> Result<String>,
) -> Result<Value> {
    let Some(data) = descriptor.as_object() else {
        return Err(message("invalid-brightness-request", "Invalid brightness request").into());
    };
    let kind = data.get("kind").and_then(Value::as_str).unwrap_or("");
    if kind == "backlight" || kind == "keyboard" {
        let valid_device = Regex::new(r"\A[A-Za-z0-9_.:+-]{1,128}\z")?;
        let device = data
            .get("device")
            .and_then(Value::as_str)
            .filter(|name| {
                valid_device.is_match(name)
                    && (kind != "keyboard" || name.contains("kbd_backlight"))
            })
            .ok_or_else(|| message("invalid-light-device", "Invalid light device"))?;
        let class = if kind == "backlight" {
            "backlight"
        } else {
            "leds"
        };
        let path = root.join("class").join(class).join(device);
        let current = sys_device(&path, kind)
            .filter(|current| {
                equals_integer(&descriptor["maximum"], current["maximum"].as_i64().unwrap())
            })
            .ok_or_else(|| {
                message(
                    "light-device-changed-or-is-unavailable",
                    "Light device changed or is unavailable",
                )
            })?;
        let value = value.clamp(
            if kind == "backlight" { 1 } else { 0 },
            current["maximum"].as_i64().unwrap(),
        );
        run(
            "brightnessctl",
            &[
                "--class".into(),
                class.into(),
                "--device".into(),
                device.into(),
                "set".into(),
                value.to_string(),
            ],
            Duration::from_secs(6),
        )?;
        return sys_device(&path, kind).ok_or_else(|| {
            message(
                "light-device-disappeared-after-the-write",
                "Light device disappeared after the write",
            )
            .into()
        });
    }
    if kind == "ddc" {
        let bus = descriptor["bus"]
            .as_i64()
            .filter(|bus| (0..=65535).contains(bus));
        let valid_connector = Regex::new(r"\Acard\d+-[A-Za-z0-9_.-]+\z")?;
        let connector = descriptor["connector"]
            .as_str()
            .filter(|name| valid_connector.is_match(name));
        let (Some(bus), Some(connector)) = (bus, connector) else {
            return Err(message("invalid-display-control", "Invalid display control").into());
        };
        let path = root.join("class/drm").join(connector);
        if fs::read_to_string(path.join("status"))?.trim() != "connected"
            || resolved_name(&path.join("ddc")) != format!("i2c-{bus}")
            || descriptor["identity"].as_str() != Some(fingerprint(&path).as_str())
        {
            return Err(message(
                "display-changed-or-disconnected",
                "Display changed or disconnected",
            )
            .into());
        }
        let (_, maximum) = ddc_read(bus, run)?;
        if !equals_integer(&descriptor["maximum"], maximum) {
            return Err(message(
                "display-brightness-range-changed",
                "Display brightness range changed",
            )
            .into());
        }
        run(
            "ddcutil",
            &[
                "--bus".into(),
                bus.to_string(),
                "setvcp".into(),
                "10".into(),
                value.clamp(0, maximum).to_string(),
            ],
            Duration::from_secs(6),
        )?;
        let (current, maximum) = ddc_read(bus, run)?;
        let mut result = descriptor.clone();
        result["current"] = json!(current);
        result["maximum"] = json!(maximum);
        return Ok(result);
    }
    Err(message("unsupported-light-device", "Unsupported light device").into())
}

pub fn execute(arguments: &[String], _input: Option<Value>) -> Result<Option<Value>> {
    let action = arguments.first().ok_or("Missing action")?;
    let data: Value = serde_json::from_str(arguments.get(1).ok_or("Missing light descriptor")?)?;
    let result = match action.as_str() {
        "discover" => discover(
            data.as_array()
                .ok_or_else(|| message("invalid-display-list", "Invalid display list"))?,
            Path::new("/sys"),
            &mut native_run,
        )?,
        "set" => write(
            &data,
            arguments
                .get(2)
                .ok_or_else(|| message("invalid-brightness-request", "Invalid brightness request"))?
                .trim()
                .parse()?,
            Path::new("/sys"),
            &mut native_run,
        )?,
        "read-ddc" => {
            let bus = data["bus"].as_i64().filter(|bus| (0..=65535).contains(bus));
            if !data.is_object() || data["kind"] != "ddc" || bus.is_none() {
                return Err(message("invalid-display-control", "Invalid display control").into());
            }
            let (current, maximum) = ddc_read(bus.unwrap(), &mut native_run)?;
            let mut result = data;
            result["current"] = json!(current);
            result["maximum"] = json!(maximum);
            result
        }
        _ => return Err("Unknown light action".into()),
    };
    Ok(Some(result))
}

#[derive(Parser)]
#[command(
    name = "light-devices.py",
    infer_long_args = true,
    args_override_self = true
)]
struct Arguments {
    #[arg(value_parser=["discover","set","read-ddc"])]
    action: String,
    #[arg(allow_negative_numbers = true)]
    data: String,
    #[arg(allow_negative_numbers=true, value_parser=crate::arguments::integer)]
    value: Option<i64>,
}
const HELP: &str = "usage: light-devices.py [-h] {discover,set,read-ddc} data [value]\n\nBounded light discovery/actions; native QML owns reads and write coalescing.\n\npositional arguments:\n  {discover,set,read-ddc}\n  data\n  value\n\noptions:\n  -h, --help            show this help message and exit\n";
pub fn run(arguments: &[String]) -> Result<i32> {
    let parsed = match Arguments::try_parse_from(
        std::iter::once("light-devices.py").chain(arguments.iter().map(String::as_str)),
    ) {
        Ok(parsed) => parsed,
        Err(error) if error.kind() == clap::error::ErrorKind::DisplayHelp => {
            print!("{HELP}");
            return Ok(0);
        }
        Err(error) => {
            error.print()?;
            return Ok(2);
        }
    };
    let mut arguments = vec![parsed.action, parsed.data];
    if let Some(value) = parsed.value {
        arguments.push(value.to_string());
    }
    match execute(&arguments, None) {
        Ok(Some(value)) => {
            println!("{}", json::encode(&value)?);
            Ok(0)
        }
        Ok(None) => Ok(0),
        Err(error) => {
            eprintln!("Light control: {error}");
            println!(
                "{}",
                json::encode(
                    &json!({"error":message("light-control-failed-refresh-the-controls-and-check-device-access", "Light control failed; refresh the controls and check device access.")})
                )?
            );
            Ok(1)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn device(root: &Path, class: &str, name: &str, maximum: i64, current: i64) -> Result<()> {
        let path = root.join("class").join(class).join(name);
        fs::create_dir_all(&path)?;
        fs::write(path.join("max_brightness"), maximum.to_string())?;
        fs::write(path.join("brightness"), current.to_string())?;
        Ok(())
    }
    #[test]
    fn discovery_and_clamping_preserve_guards_and_argument_boundaries() -> Result<()> {
        let root = tempfile::tempdir()?;
        device(root.path(), "backlight", "panel", 100, 50)?;
        device(root.path(), "leds", "device::kbd_backlight", 3, 1)?;
        device(root.path(), "leds", "input1::capslock", 1, 1)?;
        let found = discover(&[json!("eDP-1")], root.path(), &mut |_, _, _| {
            panic!("Discovery must not invoke a tool")
        })?;
        assert_eq!(found["backlight"]["current"], 50);
        assert_eq!(found["keyboard"]["device"], "device::kbd_backlight");
        let mut calls = Vec::new();
        let mut capture = |program: &str, args: &[String], timeout: Duration| {
            assert_eq!(timeout, Duration::from_secs(6));
            calls.push((program.to_owned(), args.to_vec()));
            Ok(String::new())
        };
        write(&found["backlight"], 0, root.path(), &mut capture)?;
        write(&found["keyboard"], 0, root.path(), &mut capture)?;
        assert_eq!(
            calls[0],
            (
                "brightnessctl".into(),
                ["--class", "backlight", "--device", "panel", "set", "1"]
                    .map(str::to_owned)
                    .to_vec()
            )
        );
        assert_eq!(calls[1].1.last().unwrap(), "0");
        for descriptor in [
            json!({"kind":"backlight","device":"../bad","maximum":100}),
            json!({"kind":"backlight","device":"panel","maximum":200}),
            json!({"kind":"keyboard","device":"input1::capslock","maximum":1}),
            json!({"kind":"unknown"}),
        ] {
            assert!(write(&descriptor, 50, root.path(), &mut |_, _, _| panic!(
                "Invalid descriptors must not invoke a tool"
            ))
            .is_err());
        }
        Ok(())
    }
    #[test]
    fn terse_ddc_metrics_are_bounded() {
        assert_eq!(parse_ddc("VCP 10 C 70 100\n").unwrap(), (70, 100));
        for output in ["unreadable", "VCP 10 C 101 100", "VCP 10 C 0 0"] {
            assert!(parse_ddc(output).is_err());
        }
    }
    #[test]
    fn monitor_fingerprint_and_ambiguous_connectors_block_writes() -> Result<()> {
        use std::os::unix::fs::symlink;
        let root = tempfile::tempdir()?;
        let connector = root.path().join("class/drm/card0-DP-1");
        let bus = root.path().join("bus/i2c-4");
        fs::create_dir_all(&connector)?;
        fs::create_dir_all(&bus)?;
        fs::write(connector.join("status"), "connected")?;
        fs::write(connector.join("edid"), "fixture-edid")?;
        symlink(&bus, connector.join("ddc"))?;
        let found = discover(&[json!("DP-1")], root.path(), &mut |_, _, _| {
            Ok("VCP 10 C 70 100\n".into())
        })?;
        assert_eq!(found["ddc"][0]["bus"], 4);
        fs::write(connector.join("edid"), "different-monitor")?;
        assert!(
            write(&found["ddc"][0], 40, root.path(), &mut |_, _, _| panic!(
                "Changed monitor must not receive a write"
            ))
            .is_err()
        );
        let other = root.path().join("class/drm/card1-DP-1");
        fs::create_dir_all(&other)?;
        fs::write(other.join("status"), "connected")?;
        fs::write(other.join("edid"), "fixture-edid")?;
        symlink(&bus, other.join("ddc"))?;
        let found = discover(&[json!("DP-1")], root.path(), &mut |_, _, _| {
            panic!("Ambiguous monitor must not invoke a tool")
        })?;
        assert_eq!(found["ddc"], json!([]));
        Ok(())
    }
}
