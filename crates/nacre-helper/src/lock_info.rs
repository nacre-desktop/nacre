//! Private lock widget data and labels, shared by the CLI and preparation writer.
use crate::{i18n::tr, image_text::Font, json, paths::Paths, Result};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use ureq::ResponseExt;

#[derive(Debug)]
pub struct DesktopUnavailable;
impl std::fmt::Display for DesktopUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}",
            tr(
                "helper.lock-info.desktop-data-unavailable",
                "Desktop data unavailable"
            )
        )
    }
}
impl std::error::Error for DesktopUnavailable {}

const KINDS: [&str; 9] = [
    "weather",
    "media",
    "notifications",
    "status",
    "art",
    "play-icon",
    "previous",
    "next",
    "toggle",
];
const USAGE: &str = "usage: lock-info.py [-h]\n                    {weather,media,notifications,status,art,play-icon,previous,next,toggle}\n";
const HELP: &str = "\npositional arguments:\n  {weather,media,notifications,status,art,play-icon,previous,next,toggle}\n\noptions:\n  -h, --help            show this help message and exit\n";
pub fn run(arguments: &[String]) -> Result<i32> {
    let mut positional = Vec::new();
    let mut unknown = Vec::new();
    let mut options = true;
    for argument in arguments {
        if options && argument == "--" {
            options = false;
            continue;
        }
        let (flag, value) = argument
            .split_once('=')
            .map_or((argument.as_str(), None), |(key, value)| (key, Some(value)));
        if options
            && (flag == "-h"
                || (flag.starts_with("--") && flag.len() > 2 && "--help".starts_with(flag)))
        {
            if let Some(value) = value {
                eprintln!(
                    "{USAGE}lock-info.py: error: argument -h/--help: ignored explicit argument {}",
                    json::python_repr(&json!(value))
                );
                return Ok(2);
            }
            print!("{USAGE}{HELP}");
            return Ok(0);
        }
        if options && argument.starts_with('-') {
            unknown.push(argument.clone());
        } else {
            positional.push(argument.clone());
        }
    }
    let error = if positional.is_empty() {
        Some("the following arguments are required: kind".into())
    } else if !KINDS.contains(&positional[0].as_str()) {
        Some(format!(
            "argument kind: invalid choice: {} (choose from {})",
            json::python_repr(&json!(positional[0])),
            KINDS
                .iter()
                .map(|s| format!("'{s}'"))
                .collect::<Vec<_>>()
                .join(", ")
        ))
    } else {
        unknown.extend_from_slice(&positional[1..]);
        if unknown.is_empty() {
            None
        } else {
            Some(format!("unrecognized arguments: {}", unknown.join(" ")))
        }
    };
    if let Some(error) = error {
        eprintln!("{USAGE}lock-info.py: error: {error}");
        return Ok(2);
    }
    let mut info = Info::new(Paths::environment()?);
    let settings = match fs::read_to_string(info.paths.config.join("desktop.json")) {
        Ok(raw) => match serde_json::from_str::<Value>(&raw) {
            Ok(settings) => settings,
            Err(error) => {
                eprintln!("Lock settings cache: {error}");
                json!({})
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => json!({}),
        Err(error) => {
            eprintln!("Lock settings cache: {error}");
            json!({})
        }
    };
    let kind = positional[0].as_str();
    if ["previous", "next", "toggle"].contains(&kind) {
        if settings.get("lockMedia").is_none_or(json::truthy) {
            info.ipc("mpris", if kind == "toggle" { "playPause" } else { kind })?;
        }
        return Ok(0);
    }
    let data = info.snapshot()?;
    if kind == "art" {
        println!(
            "{}",
            if settings.get("lockMedia").is_none_or(json::truthy) {
                info.artwork(&data)?.to_string_lossy().into_owned()
            } else {
                String::new()
            }
        );
    } else {
        println!("{}", info.label(kind, &data, &settings)?);
    }
    Ok(0)
}

pub struct Info {
    pub paths: Paths,
    pub cache: PathBuf,
    pub sys: PathBuf,
    /// An explicit font input for developer parity fixtures; the CLI uses Fontconfig.
    pub font_path: Option<PathBuf>,
    pub now: Option<f64>,
    fonts: BTreeMap<u32, Font>,
}
fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}
fn otherwise(value: &Value, fallback: &str) -> Value {
    if json::truthy(value) {
        value.clone()
    } else {
        json!(fallback)
    }
}
impl Info {
    fn seconds(&self) -> Result<f64> {
        self.now.map_or_else(
            || Ok(SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs_f64()),
            Ok,
        )
    }
    pub fn ipc(&self, target: &str, action: &str) -> Result<String> {
        let output = crate::process::run(
            self.paths
                .share
                .join("shell/bin/qs")
                .to_str()
                .ok_or("Invalid shell tool path")?,
            &[
                "-c".into(),
                "nacre".into(),
                "ipc".into(),
                "call".into(),
                target.into(),
                action.into(),
            ],
            None,
            Duration::from_secs(2),
        )?;
        if !output.status.success() {
            return Err(DesktopUnavailable.into());
        }
        Ok(String::from_utf8(output.stdout)?)
    }
    pub fn snapshot(&self) -> Result<Value> {
        self.snapshot_with(|| self.ipc("lockWidgets", "state"))
    }
    pub fn snapshot_with<F>(&self, fetch: F) -> Result<Value>
    where
        F: FnOnce() -> Result<String>,
    {
        let _guard = crate::files::lock(&self.cache.with_extension("lock"))?;
        match fs::read_to_string(&self.cache) {
            Ok(raw) => match json::parse_legacy_value(&raw) {
                Ok(cached) => {
                    if !cached.is_object() {
                        return Err("Cached lock data is not an object".into());
                    }
                    if let Some(time) = cached.get("time").unwrap_or(&json!(0)).as_f64() {
                        let age = self.seconds()? - time;
                        if (0.0..2.0).contains(&age) {
                            if let Some(data) = cached.get("data") {
                                return Ok(data.clone());
                            }
                        }
                    }
                }
                Err(error) => eprintln!("Lock snapshot cache: {error}"),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => eprintln!("Lock snapshot cache: {error}"),
        }
        let data = match fetch().and_then(|text| json::parse_legacy_value(&text)) {
            Ok(value) => value,
            Err(error) if error.is::<DesktopUnavailable>() => return Err(error),
            Err(error) => {
                eprintln!("Lock snapshot unavailable: {error}");
                return Ok(json!({}));
            }
        };
        let temporary = self.cache.with_extension("tmp");
        fs::write(
            &temporary,
            json::dumps(&json!({"time":self.seconds()?,"data":data}), false)?,
        )?;
        fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
        fs::rename(temporary, &self.cache)?;
        Ok(data)
    }
    pub fn artwork(&self, data: &Value) -> Result<PathBuf> {
        let fallback = self.paths.share.join("branding/nacre-lock.png");
        let url = data["media"]["art"].as_str().unwrap_or("");
        let file_path = |url: &str| -> Option<PathBuf> {
            let tail = url.strip_prefix("file:")?;
            let path = if let Some(tail) = tail.strip_prefix("//") {
                tail.find('/').map(|index| &tail[index..]).unwrap_or("")
            } else {
                tail
            };
            Some(PathBuf::from(crate::paths::unquote(
                path.split(['?', '#']).next().unwrap_or(""),
            )))
        };
        let mut fingerprint = url.to_owned();
        if let Some(path) = file_path(url) {
            match fs::metadata(path) {
                Ok(meta) => fingerprint.push_str(&format!(
                    ":{}:{}",
                    i128::from(meta.mtime()) * 1_000_000_000 + i128::from(meta.mtime_nsec()),
                    meta.len()
                )),
                Err(error) => eprintln!("Lock artwork source metadata: {error}"),
            }
        }
        use sha2::{Digest, Sha256};
        let digest = format!("{:x}", Sha256::digest(fingerprint.as_bytes()));
        let output = self
            .cache
            .with_file_name(format!("lock-art-{}.png", &digest[..20]));
        if output.exists() {
            return Ok(output);
        }
        let metadata = self.cache.with_file_name("lock-art-source.json");
        match fs::read_to_string(&metadata) {
            Ok(raw) => match json::parse_legacy_value(&raw) {
                Ok(saved) if saved["fingerprint"].as_str() == Some(&fingerprint) => {
                    if (json::truthy(&saved["ok"]) && output.exists())
                        || self.seconds()? - saved["checked"].as_f64().unwrap_or(0.0) < 120.0
                    {
                        return Ok(if json::truthy(&saved["ok"]) && output.exists() {
                            output
                        } else {
                            fallback
                        });
                    }
                }
                Ok(_) => (),
                Err(error) => eprintln!("Lock artwork cache metadata: {error}"),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => eprintln!("Lock artwork cache metadata read: {error}"),
        }
        let result = (|| -> Result<()> {
            let mut raw = Vec::new();
            if let Some(path) = file_path(url) {
                fs::File::open(path)?
                    .take(4_000_001)
                    .read_to_end(&mut raw)?;
            } else if url.starts_with("https:") {
                let agent = ureq::Agent::config_builder()
                    .timeout_global(Some(Duration::from_secs(4)))
                    .build()
                    .new_agent();
                let mut response = agent.get(url).call()?;
                if response.get_uri().scheme_str() != Some("https") {
                    return Err(tr(
                        "helper.lock-info.unsupported-artwork-redirect",
                        "Unsupported artwork redirect",
                    )
                    .into());
                }
                response
                    .body_mut()
                    .as_reader()
                    .take(4_000_001)
                    .read_to_end(&mut raw)?;
            } else {
                return Err(tr("helper.lock-info.no-artwork", "No artwork").into());
            }
            if raw.len() > 4_000_000 {
                return Err(tr("helper.lock-info.artwork-too-large", "Artwork too large").into());
            }
            let reader =
                image::ImageReader::new(std::io::Cursor::new(&raw)).with_guessed_format()?;
            let format = reader.format();
            let (width, height) = reader.into_dimensions()?;
            if u64::from(width) * u64::from(height) > 20_000_000 {
                return Err(tr(
                    "helper.lock-info.artwork-dimensions-too-large",
                    "Artwork dimensions too large",
                )
                .into());
            }
            let mut encoded = tempfile::NamedTempFile::new_in(
                self.cache.parent().ok_or("Missing lock cache parent")?,
            )?;
            use std::io::Write;
            encoded.write_all(&raw)?;
            let palette = format == Some(image::ImageFormat::Gif)
                || (format == Some(image::ImageFormat::Png) && raw.get(25) == Some(&3));
            let alpha = {
                use image::ImageDecoder;
                image::ImageReader::new(std::io::Cursor::new(&raw))
                    .with_guessed_format()?
                    .into_decoder()?
                    .color_type()
                    .has_alpha()
            };
            let picture = if format == Some(image::ImageFormat::Jpeg) {
                image::DynamicImage::ImageRgb8(crate::image_paint::draft_source(
                    encoded.path(),
                    1024,
                    false,
                )?)
                .to_rgba8()
            } else {
                orient_decoder::load_rgba(encoded.path())?
            };
            let picture = if palette {
                let scale = 512.0 / f64::from(width.max(height));
                let (w, h) = if scale >= 1.0 {
                    (width, height)
                } else {
                    (
                        (f64::from(width) * scale).round().max(1.0) as u32,
                        (f64::from(height) * scale).round().max(1.0) as u32,
                    )
                };
                image::imageops::resize(&picture, w, h, image::imageops::FilterType::Nearest)
            } else if alpha {
                orient_decoder::thumbnail_rgba(
                    &picture,
                    (512, 512),
                    orient_decoder::Filter::Bicubic,
                )?
            } else {
                image::DynamicImage::ImageRgb8(orient_decoder::thumbnail_rgb(
                    &image::DynamicImage::ImageRgba8(picture).to_rgb8(),
                    (512, 512),
                    orient_decoder::Filter::Bicubic,
                )?)
                .to_rgba8()
            };
            let temporary = output.with_extension("next");
            fs::write(&temporary, orient_decoder::encode_rgba_png(&picture, 6)?)?;
            fs::rename(temporary, &output)?;
            fs::set_permissions(&output, fs::Permissions::from_mode(0o600))?;
            Ok(())
        })();
        if let Err(error) = &result {
            if !url.is_empty() {
                eprintln!("Lock artwork unavailable: {error}");
            }
        }
        fs::write(
            &metadata,
            json::dumps(
                &json!({"fingerprint":fingerprint,"checked":self.seconds()?,"ok":result.is_ok()}),
                false,
            )?,
        )?;
        fs::set_permissions(metadata, fs::Permissions::from_mode(0o600))?;
        Ok(if result.is_ok() { output } else { fallback })
    }
    pub fn new(paths: Paths) -> Self {
        Self {
            cache: paths.cache.join("lock-widgets.json"),
            paths,
            sys: "/sys".into(),
            font_path: None,
            now: None,
            fonts: BTreeMap::new(),
        }
    }
    fn font(&mut self, size: u32) -> Result<&mut Font> {
        let pixels = (f64::from(size) * 96.0 / 72.0).ceil() as u32;
        if !self.fonts.contains_key(&size) {
            let font = if let Some(path) = &self.font_path {
                Font::open(path, pixels)?
            } else {
                Font::find("IBM Plex Sans", "Bold", pixels)?
            };
            self.fonts.insert(size, font);
        }
        self.fonts
            .get_mut(&size)
            .ok_or_else(|| "Missing lock label font".into())
    }
    pub fn plain(&mut self, value: &Value, width: f64, lines: usize, size: u32) -> Result<String> {
        let text = if json::truthy(value) {
            json::python_str(value)
        } else {
            String::new()
        };
        let text = text
            .split(|c: char| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c))
            .filter(|part| !part.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(2048)
            .collect::<String>();
        let face = self.font(size)?;
        let mut wrapped = Vec::new();
        let mut line = String::new();
        for character in text.chars() {
            let candidate = format!("{line}{character}");
            if face.advance(&candidate)? > width && !line.is_empty() {
                if let Some(space) = line.rfind(' ').filter(|index| *index > 0) {
                    wrapped.push(line[..space].trim_end().to_owned());
                    line = format!("{}{character}", &line[space + 1..]);
                } else {
                    wrapped.push(line.trim_end().to_owned());
                    line = character.to_string().trim_start().to_owned();
                }
                if wrapped.len() > lines {
                    break;
                }
            } else {
                line = candidate;
            }
        }
        if !line.is_empty() {
            wrapped.push(line.trim_end().to_owned());
        }
        if wrapped.len() > lines {
            wrapped.truncate(lines);
            let last = wrapped.last_mut().ok_or("list index out of range")?;
            while !last.is_empty() && face.advance(&format!("{last}…"))? > width {
                last.pop();
            }
            last.push('…');
        }
        Ok(escape(&wrapped.join("\n")))
    }
    pub fn label(&mut self, kind: &str, data: &Value, settings: &Value) -> Result<String> {
        let enabled = |key: &str| settings.get(key).is_none_or(json::truthy);
        let text = |value: &Value| json::python_str(value);
        match kind {
            "weather" => {
                if !enabled("lockWeather") {
                    return Ok(String::new());
                }
                let weather = data.get("weather").unwrap_or(&Value::Null);
                let description = &weather["description"];
                let location =
                    self.plain(&otherwise(&weather["location"], "Weather"), 288.0, 1, 16)?;
                let temperature = self.plain(
                    if json::truthy(description) {
                        weather.get("temperature").unwrap_or(&Value::Null)
                    } else {
                        &Value::Null
                    },
                    288.0,
                    1,
                    16,
                )?;
                let detail = if json::truthy(description) {
                    description.clone()
                } else {
                    otherwise(&weather["error"], "Weather unavailable")
                };
                let detail = self.plain(&detail, 288.0, 2, 16)?;
                Ok(tr(
                    "helper.lock-info.b-value0-b-value1-value2",
                    "<b>{value0}</b>\n{value1}\n\n{value2}",
                )
                .replace("{value0}", &location)
                .replace("{value1}", &temperature)
                .replace("{value2}", &detail))
            }
            "media" => {
                if !enabled("lockMedia") {
                    return Ok(String::new());
                }
                let media = &data["media"];
                if !json::truthy(media) {
                    return Ok(tr(
                        "helper.lock-info.b-media-b-nothing-playing",
                        "<b>Media</b>\n\nNothing playing",
                    ));
                }
                let title =
                    self.plain(&otherwise(&media["title"], "Untitled track"), 288.0, 2, 14)?;
                let artist = self.plain(&media["artist"], 288.0, 2, 14)?;
                Ok(tr(
                    "helper.lock-info.b-value0-b-value1-value2",
                    "<b>{value0}</b>\n{value1}\n\n{value2}",
                )
                .replace("{value0}", &title)
                .replace("{value1}", &artist)
                .replace(
                    "{value2}",
                    if json::truthy(&media["playing"]) {
                        "Playing"
                    } else {
                        "Paused"
                    },
                ))
            }
            "notifications" => {
                if !enabled("lockNotifications") {
                    return Ok(String::new());
                }
                let empty = Vec::new();
                let rows = data
                    .get("notifications")
                    .and_then(Value::as_array)
                    .unwrap_or(&empty);
                let count = data.get("count").cloned().unwrap_or(json!(rows.len()));
                let mut output = vec![format!("<b>Notifications · {}</b>", text(&count))];
                if rows.is_empty() {
                    return Ok(tr(
                        "helper.lock-info.value0-you-are-all-caught-up",
                        "{value0}\n\nYou are all caught up",
                    )
                    .replace("{value0}", &output[0]));
                }
                if json::truthy(&settings["lockNotificationContents"]) {
                    for row in rows.iter().take(3) {
                        output.push(String::new());
                        output.push(format!(
                            "<b>{}</b>",
                            self.plain(&otherwise(&row["app"], "Notification"), 288.0, 1, 14)?
                        ));
                        output.push(self.plain(&row["summary"], 288.0, 2, 14)?);
                        output.push(self.plain(&row["body"], 288.0, 1, 14)?);
                    }
                } else {
                    let mut names = Vec::new();
                    for row in rows {
                        let name = otherwise(&row["app"], "Notification");
                        if !names.contains(&name) {
                            names.push(name);
                        }
                    }
                    output.extend([
                        String::new(),
                        "Message previews hidden".into(),
                        String::new(),
                    ]);
                    for name in names.iter().take(5) {
                        output.push(self.plain(name, 288.0, 1, 14)?);
                    }
                }
                Ok(output.join("\n"))
            }
            "status" => {
                let root = self.sys.join("class/power_supply");
                let mut batteries = Vec::new();
                match fs::read_dir(&root) {
                    Ok(entries) => {
                        for entry in entries {
                            let path = entry?.path().join("capacity");
                            match fs::read_to_string(&path) {
                                Ok(value) => batteries.push(format!("{}% battery", value.trim())),
                                Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                                Err(error) => eprintln!("Lock battery information: {error}"),
                            }
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Err(error) => return Err(error.into()),
                }
                let status = if batteries.is_empty() {
                    "Nacre".into()
                } else {
                    batteries.join(" · ")
                };
                Ok(escape(&status))
            }
            "play-icon" => Ok(if json::truthy(&data["media"]["playing"]) {
                "pause"
            } else {
                "play_arrow"
            }
            .into()),
            _ => Ok(String::new()),
        }
    }
}
