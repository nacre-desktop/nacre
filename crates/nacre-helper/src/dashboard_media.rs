//! Track-keyed private artwork/lyrics caches, with direct Orient library calls.
use crate::{files, i18n::tr, json, orient, paths::Paths, Result};
use regex::Regex;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::PathBuf,
    sync::OnceLock,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use ureq::ResponseExt;
#[derive(Debug)]
pub struct Attribute(pub String);
impl std::fmt::Display for Attribute {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}
impl std::error::Error for Attribute {}
fn now() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
}
fn sorted(value: &Value) -> Value {
    match value {
        Value::Object(values) => {
            let mut entries: Vec<_> = values.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            Value::Object(
                entries
                    .into_iter()
                    .map(|(key, value)| (key.clone(), sorted(value)))
                    .collect(),
            )
        }
        Value::Array(values) => Value::Array(values.iter().map(sorted).collect()),
        _ => value.clone(),
    }
}
fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::String(v) => !v.is_empty(),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
fn quote(value: &str, plus: bool) -> String {
    value
        .as_bytes()
        .iter()
        .map(|byte| {
            if byte.is_ascii_alphanumeric() || b"_.-~".contains(byte) {
                char::from(*byte).to_string()
            } else if *byte == b' ' && plus {
                "+".into()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}
fn unquote(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut result = Vec::new();
    let mut index = 0;
    let digit = |byte: u8| match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    };
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let (Some(high), Some(low)) = (digit(bytes[index + 1]), digit(bytes[index + 2])) {
                result.push(high * 16 + low);
                index += 3;
                continue;
            }
        }
        result.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&result).into_owned()
}

fn get(url: &str, maximum: u64) -> Result<Vec<u8>> {
    let authority = url
        .strip_prefix("https://")
        .and_then(|tail| tail.split('/').next())
        .filter(|v| !v.is_empty() && !v.contains('@'))
        .ok_or_else(|| {
            tr(
                "helper.dashboard-media.artwork-must-use-https",
                "Artwork must use HTTPS",
            )
        })?;
    if authority.is_empty() {
        return Err(tr(
            "helper.dashboard-media.artwork-must-use-https",
            "Artwork must use HTTPS",
        )
        .into());
    }
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(6)))
        .build()
        .new_agent();
    let mut response = agent
        .get(url)
        .header("User-Agent", "Nacre/1.0 (desktop media)")
        .call()?;
    // Check the final URI, as the original helper did after redirect handling.
    if response.get_uri().scheme_str() != Some("https") {
        return Err(tr(
            "helper.dashboard-media.https-redirect-required",
            "HTTPS redirect required",
        )
        .into());
    }
    let bytes = response
        .body_mut()
        .with_config()
        .limit(maximum + 1)
        .read_to_vec()?;
    if bytes.len() as u64 > maximum {
        return Err(tr(
            "helper.dashboard-media.response-too-large",
            "Response too large",
        )
        .into());
    }
    Ok(bytes)
}
pub fn lyrics(text: &Value) -> Value {
    static TIMESTAMPS: OnceLock<Regex> = OnceLock::new();
    static TAGS: OnceLock<Regex> = OnceLock::new();
    let timestamps =
        TIMESTAMPS.get_or_init(|| Regex::new(r"\[(\d{1,3}):(\d{2}(?:\.\d{1,3})?)\]").unwrap());
    let tags = TAGS.get_or_init(|| Regex::new(r"\[[^\]]*\]").unwrap());
    let text = if truthy(text) {
        json::python_str(text)
    } else {
        String::new()
    };
    let mut lines = Vec::new();
    for line in Regex::new(r"\r\n|[\n\r\x0b\x0c\x1c\x1d\x1e\x{85}\x{2028}\x{2029}]")
        .unwrap()
        .split(&text)
        .take(3000)
    {
        let content = tags
            .replace_all(line, "")
            .trim()
            .chars()
            .take(500)
            .collect::<String>();
        for stamp in timestamps.captures_iter(line) {
            if let (Ok(minutes), Ok(seconds)) = (stamp[1].parse::<u64>(), stamp[2].parse::<f64>()) {
                lines.push(json!({"time":minutes as f64*60.0+seconds,"text":content}));
            }
        }
    }
    lines.sort_by(|a, b| {
        a["time"]
            .as_f64()
            .unwrap()
            .partial_cmp(&b["time"].as_f64().unwrap())
            .unwrap()
    });
    lines.truncate(2000);
    json!(lines)
}
pub fn track_data(meta: &Value) -> Result<Value> {
    let key = format!(
        "{:x}",
        Sha256::digest(json::encode(&sorted(meta))?.as_bytes())
    );
    let cache = Paths::environment()?.cache.join("dashboard-media");
    if !cache.exists() {
        fs::create_dir_all(&cache)?;
        fs::set_permissions(&cache, fs::Permissions::from_mode(0o700))?;
    }
    if !meta.is_object() {
        let name = match meta {
            Value::Null => "NoneType",
            Value::Bool(_) => "bool",
            Value::Array(_) => "list",
            Value::String(_) => "str",
            Value::Number(number) if number.is_f64() => "float",
            _ => "int",
        };
        return Err(Attribute(format!("'{name}' object has no attribute 'get'")).into());
    }
    let path = cache.join(format!("{key}.json"));
    if path.exists() {
        let cached: Value = serde_json::from_slice(&fs::read(&path)?)?;
        let modified = fs::metadata(&path)?
            .modified()?
            .duration_since(UNIX_EPOCH)?
            .as_secs_f64();
        if now() - modified < cached["cacheSeconds"].as_f64().unwrap_or(1800.0) {
            return Ok(cached);
        }
    }
    let mut result = json!({"key":key,"lines":[],"plain":"","palette":{},"lyricsMessage":"No synced lyrics found"});
    if truthy(&meta["lyrics"]) && truthy(&meta["title"]) && truthy(&meta["artist"]) {
        let lyric_query = || -> Result<Value> {
            let duration = meta
                .get("duration")
                .and_then(Value::as_f64)
                .unwrap_or(0.0)
                .round_ties_even();
            let query = format!(
                "track_name={}&artist_name={}&album_name={}&duration={}",
                quote(&json::python_str(&meta["title"]), true),
                quote(&json::python_str(&meta["artist"]), true),
                quote(
                    &json::python_str(meta.get("album").unwrap_or(&json!(""))),
                    true
                ),
                duration as i64
            );
            Ok(serde_json::from_slice(&get(
                &format!("https://lrclib.net/api/get?{query}"),
                300000,
            )?)?)
        };
        match lyric_query() {
            Ok(data) => {
                result["lines"] = lyrics(&data["syncedLyrics"]);
                result["plain"] = json!(if truthy(&data["plainLyrics"]) {
                    json::python_str(&data["plainLyrics"])
                        .chars()
                        .take(60000)
                        .collect::<String>()
                } else {
                    String::new()
                });
                result["lyricsMessage"] = json!(if truthy(&data["instrumental"]) {
                    "Instrumental"
                } else {
                    "No synced lyrics found"
                });
            }
            Err(_) => result["lyricsMessage"] = json!("Lyrics unavailable for this track"),
        }
    }
    if truthy(&meta["palette"]) && truthy(&meta["art"]) {
        let artwork = || -> Result<Value> {
            let art = meta["art"].as_str().ok_or("Invalid artwork URL")?;
            let original = if let Some(file) = art.strip_prefix("file:") {
                let tail = file
                    .strip_prefix("//")
                    .map(|v| v.find('/').map(|i| &v[i..]).unwrap_or(""))
                    .unwrap_or(file);
                let path = PathBuf::from(unquote(tail));
                if fs::metadata(&path)?.len() > 8_000_000 {
                    return Err(tr(
                        "helper.dashboard-media.artwork-too-large",
                        "Artwork too large",
                    )
                    .into());
                }
                path
            } else {
                let path = cache.join(format!("{key}.image"));
                fs::write(&path, get(art, 4_000_000)?)?;
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
                path
            };
            let poster = cache.join(format!("{key}.png"));
            // New Orient API must enforce the old artwork 16 MP bound before allocation.
            let bytes =
                orient_decoder::artwork_png(&original).map_err(|error| -> crate::Error {
                    if error.to_string() == "Artwork dimensions too large" {
                        tr(
                            "helper.dashboard-media.artwork-dimensions-too-large",
                            "Artwork dimensions too large",
                        )
                        .into()
                    } else {
                        error
                    }
                })?;
            files::atomic(&poster, &bytes)?;
            fs::set_permissions(&poster, fs::Permissions::from_mode(0o600))?;
            let data = orient::from_image(
                &poster,
                &json!({"mode":meta.get("mode").cloned().unwrap_or(json!("dark"))}),
            )?;
            let mut palette = serde_json::Map::new();
            for role in ["surface", "onSurface", "primary", "secondary", "tertiary"] {
                palette.insert(
                    role.into(),
                    json!(format!(
                        "#{}",
                        data["colours"][role]
                            .as_str()
                            .ok_or("Invalid artwork palette")?
                            .trim_start_matches('#')
                    )),
                );
            }
            Ok(Value::Object(palette))
        };
        result["palette"] = artwork().unwrap_or(json!({}));
    }
    result["cacheSeconds"] = json!(if truthy(&result["lines"])
        || truthy(&result["plain"])
        || result["lyricsMessage"] == "Instrumental"
        || !truthy(&meta["lyrics"])
    {
        14 * 86400
    } else {
        1800
    });
    // Original cache bytes are ordinary JSON, with no trailing newline.
    fs::write(&path, json::encode(&result)?)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let mut entries: Vec<_> = fs::read_dir(&cache)?
        .filter_map(std::result::Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    entries.sort_by_key(|path| {
        std::cmp::Reverse(
            fs::metadata(path)
                .and_then(|meta| meta.modified())
                .unwrap_or(UNIX_EPOCH),
        )
    });
    for old in entries.into_iter().skip(80) {
        let stem = old
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        for file in fs::read_dir(&cache)? {
            let file = file?.path();
            if file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .starts_with(&format!("{stem}."))
            {
                fs::remove_file(file)?;
            }
        }
    }
    Ok(result)
}
pub fn execute(arguments: &[String], input: Option<Value>) -> Result<Option<Value>> {
    if arguments.len() != 1 || input.is_some() {
        return Err("Unsupported helper operation".into());
    }
    Ok(Some(track_data(&serde_json::from_str(&arguments[0])?)?))
}

pub fn spectrum() -> Result<i32> {
    use std::os::unix::process::CommandExt;
    let available =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).any(|root| {
            let path = root.join("cava");
            path.is_file()
                && fs::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
        });
    if !available {
        return Ok(0);
    }
    let cache = Paths::environment()?.cache.join("dashboard-media");
    if !cache.exists() {
        fs::create_dir_all(&cache)?;
        fs::set_permissions(&cache, fs::Permissions::from_mode(0o700))?;
    }
    let path = cache.join("cava.conf");
    fs::write(&path,"[general]\nframerate=20\nbars=32\nautosens=1\n[input]\nmethod=pipewire\nsource=auto\n[output]\nmethod=raw\nraw_target=/dev/stdout\ndata_format=ascii\nascii_max_range=1000\nchannels=mono\n")?;
    Err(std::process::Command::new("cava")
        .args(["-p", path.to_str().ok_or("Invalid spectrum path")?])
        .exec()
        .into())
}
