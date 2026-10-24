//! Shared catalog/preferences/preview behavior. The remaining session front end is ported last.
use crate::{i18n::tr, json, paths::Paths, state, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    time::Duration,
};
use unicode_casefold::UnicodeCaseFold;
pub const IMAGES: [&str; 7] = ["jpg", "jpeg", "png", "webp", "gif", "tif", "tiff"];
pub const VIDEOS: [&str; 6] = ["mp4", "webm", "mkv", "mov", "avi", "m4v"];
const THUMBNAIL_LIMIT_BYTES: u64 = 256 * 1024;
const PREVIEW_LIMIT_BYTES: u64 = 2 * 1024 * 1024;
const THUMBNAIL_CACHE_COUNT: usize = 128;
fn native_engine_identity() -> Result<&'static str> {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    if let Some(value) = ID.get() {
        return Ok(value);
    }
    // /proc/self/exe remains readable if an upgrade unlinks the running binary.
    let binary = fs::read("/proc/self/exe")?;
    let identity = format!(
        "{}:{:x}",
        crate::orient::engine_id(),
        Sha256::digest(binary)
    );
    Ok(ID.get_or_init(|| identity))
}
pub fn atomic(path: &Path, data: &Value) -> Result<()> {
    if state::schema(path).is_some() {
        let mut value = data.clone();
        let entry = state::schema(path).ok_or("Missing state schema")?;
        if value
            .get("version")
            .and_then(Value::as_u64)
            .is_some_and(|version| version > entry.version)
        {
            return Err(state::StateError::Newer.into());
        }
        if entry.shape == "array" {
            if !value.is_array() {
                return Err("Invalid state list".into());
            }
            value = json!({"version":entry.version,"items":value});
        } else {
            value
                .as_object_mut()
                .ok_or("Invalid state object")?
                .insert("version".into(), json!(entry.version));
        }
        state::write_text(path, &(json::dumps(&value, false)? + "\n"))?;
        Ok(())
    } else {
        crate::files::atomic(path, json::dumps(data, false)?.as_bytes())
    }
}
fn nanos(meta: &fs::Metadata) -> i128 {
    i128::from(meta.mtime()) * 1_000_000_000 + i128::from(meta.mtime_nsec())
}

#[derive(Debug)]
pub struct ToolFailure {
    pub command: Vec<String>,
    pub code: i32,
    pub stderr: String,
}
impl std::fmt::Display for ToolFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Command '{}' returned non-zero exit status {}.",
            json::python_repr(&json!(self.command)),
            self.code
        )
    }
}
impl std::error::Error for ToolFailure {}
pub type PaletteCommand = Box<dyn Fn(&[String]) -> Result<Option<String>> + Send + Sync>;

fn cached_thumbnail(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| {
        meta.file_type().is_file() && meta.len() <= THUMBNAIL_LIMIT_BYTES
    })
}

fn cached_preview(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|meta| {
        meta.file_type().is_file() && meta.len() <= PREVIEW_LIMIT_BYTES
    })
}

fn trim_thumbnail_cache(cache: &Path, keep_key: &str) -> Result<()> {
    let mut files = fs::read_dir(cache)?
        .filter_map(std::result::Result::ok)
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let key = name.strip_suffix("-thumbnail.jpg")?;
            if key.len() != 64 || !key.bytes().all(|b| b.is_ascii_hexdigit()) {
                return None;
            }
            Some((key.to_owned(), entry.path(), entry.metadata().ok()))
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|(_, _, meta)| {
        std::cmp::Reverse(
            meta.as_ref()
                .and_then(|meta| meta.modified().ok())
                .unwrap_or(std::time::UNIX_EPOCH),
        )
    });
    let mut kept = 0;
    for (key, path, meta) in files {
        let valid = meta.as_ref().is_some_and(|meta| {
            meta.file_type().is_file() && meta.len() <= THUMBNAIL_LIMIT_BYTES
        });
        if !valid {
            let _ = fs::remove_file(path);
            let preview = cache.join(format!("{key}-preview.jpg"));
            if preview.exists() {
                fs::remove_file(preview)?;
            }
        } else if key == keep_key || kept < THUMBNAIL_CACHE_COUNT - 1 {
            kept += usize::from(key != keep_key);
        } else {
            fs::remove_file(path)?;
            let preview = cache.join(format!("{key}-preview.jpg"));
            if preview.exists() {
                fs::remove_file(preview)?;
            }
        }
    }
    for entry in fs::read_dir(cache)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(key) = name.strip_suffix("-preview.jpg") else {
            continue;
        };
        if key.len() == 64
            && key.bytes().all(|b| b.is_ascii_hexdigit())
            && !cached_thumbnail(&cache.join(format!("{key}-thumbnail.jpg")))
        {
            fs::remove_file(entry.path())?;
        }
    }
    Ok(())
}

pub struct Media {
    pub paths: Paths,
    pub assets: PathBuf,
    pub library: PathBuf,
    pub cache_path: PathBuf,
    pub state_path: PathBuf,
    pub prefs_path: PathBuf,
    pub now_ms: Option<i64>,
    pub hour: Option<u32>,
    pub palette_command: Option<PaletteCommand>,
}
impl Media {
    pub fn new(paths: Paths, assets: PathBuf) -> Result<Self> {
        let fallback = paths.home.join("Pictures/Wallpapers");
        let config = state::read(&paths.config.join("cli.json"), json!({}))?;
        let library = if let Some(value) = config["wallpaper"]["directory"]
            .as_str()
            .filter(|s| !s.is_empty() && !s.contains('\0'))
        {
            crate::paths::expand(Path::new(value))?
        } else {
            fallback
        };
        Ok(Self {
            cache_path: paths.cache.join("wallpaper-media"),
            state_path: paths.state.join("wallpaper"),
            prefs_path: paths.config.join("wallpaper-picker.json"),
            paths,
            assets,
            library,
            now_ms: None,
            hour: None,
            palette_command: None,
        })
    }
    pub fn cache(&self) -> PathBuf {
        self.cache_path.clone()
    }
    pub fn wallpaper_state(&self) -> PathBuf {
        self.state_path.clone()
    }
    pub fn preferences_path(&self) -> PathBuf {
        self.prefs_path.clone()
    }
    pub fn session() -> Result<Value> {
        Self::session_on(&zbus::blocking::Connection::system()?)
    }
    fn session_on(connection: &zbus::blocking::Connection) -> Result<Value> {
        use zbus::blocking::Proxy;
        use zbus::zvariant::OwnedObjectPath;
        let proxy = |path: &str, interface: &str| -> Result<Proxy<'_>> {
            Ok(zbus::blocking::proxy::Builder::new(connection)
                .destination("org.freedesktop.login1")?
                .path(path.to_owned())?
                .interface(interface.to_owned())?
                .cache_properties(zbus::proxy::CacheProperties::No)
                .build()?)
        };
        let manager = proxy("/org/freedesktop/login1", "org.freedesktop.login1.Manager")?;
        let user: OwnedObjectPath = manager.call("GetUser", &(nix::unistd::getuid().as_raw(),))?;
        let user = proxy(user.as_str(), "org.freedesktop.login1.User")?;
        let (display, path): (String, OwnedObjectPath) = user.get_property("Display")?;
        if display.is_empty() {
            return Ok(json!({"locked":false,"path":""}));
        }
        let session = proxy(path.as_str(), "org.freedesktop.login1.Session")?;
        let locked: bool = session.get_property("LockedHint")?;
        let path: OwnedObjectPath = manager.call("GetSession", &(display,))?;
        Ok(json!({"locked":locked,"path":path.as_str()}))
    }
    pub fn watch_session() -> Result<i32> {
        use std::io::Write;
        use zbus::{
            blocking::{Connection, MessageIterator},
            message::Type,
            zvariant::OwnedValue,
            MatchRule,
        };
        let connection = Connection::system()?;
        let rule = MatchRule::builder()
            .msg_type(Type::Signal)
            .sender("org.freedesktop.login1")?
            .path_namespace("/org/freedesktop/login1")?
            .build();
        let messages = MessageIterator::for_match_rule(rule, &connection, Some(128))?;
        let mut state = Self::session_on(&connection)?;
        state["sleeping"] = json!(false);
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        writeln!(output, "{}", json::dumps(&state, false)?)?;
        output.flush()?;
        for message in messages {
            let message = message?;
            let header = message.header();
            let prior = state.clone();
            let mut refresh = false;
            if header
                .interface()
                .is_some_and(|v| v.as_str() == "org.freedesktop.login1.Manager")
                && header
                    .member()
                    .is_some_and(|v| v.as_str() == "PrepareForSleep")
            {
                let (sleeping,): (bool,) = message.body().deserialize()?;
                state["sleeping"] = json!(sleeping);
            } else if header
                .interface()
                .is_some_and(|v| v.as_str() == "org.freedesktop.DBus.Properties")
                && header
                    .member()
                    .is_some_and(|v| v.as_str() == "PropertiesChanged")
            {
                let (interface, mut changed, invalidated): (
                    String,
                    std::collections::HashMap<String, OwnedValue>,
                    Vec<String>,
                ) = message.body().deserialize()?;
                if interface == "org.freedesktop.login1.Session"
                    && header
                        .path()
                        .is_some_and(|v| Some(v.as_str()) == state["path"].as_str())
                {
                    if let Some(locked) = changed.remove("LockedHint") {
                        state["locked"] = json!(bool::try_from(locked)?);
                    }
                    refresh = invalidated.iter().any(|v| v == "LockedHint");
                } else if interface == "org.freedesktop.login1.User" {
                    refresh = changed.contains_key("Display")
                        || invalidated.iter().any(|v| v == "Display");
                }
            } else if header
                .interface()
                .is_some_and(|v| v.as_str() == "org.freedesktop.login1.Manager")
                && header
                    .member()
                    .is_some_and(|v| v.as_str() == "SessionNew" || v.as_str() == "SessionRemoved")
            {
                refresh = true;
            }
            if refresh {
                let sleeping = state["sleeping"].clone();
                state = Self::session_on(&connection)?;
                state["sleeping"] = sleeping;
            }
            if state != prior {
                writeln!(output, "{}", json::dumps(&state, false)?)?;
                output.flush()?;
            }
        }
        Err("Logind session event stream closed".into())
    }
    /// Preserve Pillow's mode-dependent thumbnail before converting to luminance.
    pub fn wallpaper_mode_pixels(path: &Path) -> Result<image::GrayImage> {
        use image::{DynamicImage, ImageDecoder, ImageFormat};
        let bytes = fs::read(path)?;
        let format = image::guess_format(&bytes)?;
        let color_type = image::ImageReader::new(std::io::Cursor::new(&bytes))
            .with_guessed_format()?
            .into_decoder()?
            .color_type();
        let alpha = matches!(
            color_type,
            image::ColorType::Rgba8
                | image::ColorType::Rgba16
                | image::ColorType::La8
                | image::ColorType::La16
        );
        let palette =
            format == ImageFormat::Gif || (format == ImageFormat::Png && bytes.get(25) == Some(&3));
        let source = if format == ImageFormat::Jpeg {
            DynamicImage::ImageRgb8(crate::image_paint::draft_source(path, 128, false)?)
        } else {
            DynamicImage::ImageRgba8(orient_decoder::load_rgba(path)?)
        };
        let image = if palette {
            // P-mode thumbnails use nearest sampling, including its aspect rounding.
            let (width, height) = (source.width(), source.height());
            let (w, h) = if width <= 64 && height <= 64 {
                (width, height)
            } else {
                let aspect = f64::from(width) / f64::from(height);
                let choose = |n: f64, horizontal: bool| {
                    let error = |value: f64| {
                        if horizontal {
                            (aspect - value / 64.0).abs()
                        } else if value == 0.0 {
                            0.0
                        } else {
                            (aspect - 64.0 / value).abs()
                        }
                    };
                    if error(n.floor()) <= error(n.ceil()) {
                        n.floor().max(1.0) as u32
                    } else {
                        n.ceil().max(1.0) as u32
                    }
                };
                if width <= height {
                    (choose(64.0 * aspect, true), 64)
                } else {
                    (64, choose(64.0 / aspect, false))
                }
            };
            image::imageops::resize(
                &source.to_rgba8(),
                w,
                h,
                image::imageops::FilterType::Nearest,
            )
        } else if !alpha {
            DynamicImage::ImageRgb8(orient_decoder::thumbnail_rgb(
                &source.to_rgb8(),
                (64, 64),
                orient_decoder::Filter::Bicubic,
            )?)
            .to_rgba8()
        } else {
            orient_decoder::thumbnail_rgba(
                &source.to_rgba8(),
                (64, 64),
                orient_decoder::Filter::Bicubic,
            )?
        };
        Ok(image::GrayImage::from_fn(
            image.width(),
            image.height(),
            |x, y| {
                let p = image[(x, y)];
                image::Luma([((19595 * u32::from(p[0])
                    + 38470 * u32::from(p[1])
                    + 7471 * u32::from(p[2])
                    + 32768)
                    >> 16) as u8])
            },
        ))
    }
    pub fn wallpaper_mode(path: &Path) -> Result<&'static str> {
        let image = Self::wallpaper_mode_pixels(path)?;
        let sum: u64 = image.pixels().map(|p| u64::from(p[0])).sum();
        Ok(
            if sum >= 140 * u64::from(image.width()) * u64::from(image.height()) {
                "light"
            } else {
                "dark"
            },
        )
    }
    pub fn settings(&self) -> Result<Value> {
        let prefs = self.preferences_path();
        let saved = crate::owned_settings::read(&prefs, "wallpaper-picker", json!({}))?;
        self.settings_with(saved)
    }
    fn settings_with(&self, saved: Value) -> Result<Value> {
        let prefs = self.preferences_path();
        let mut result = json!({"kind":"static","layout":"carousel","paused":false,"motionMode":"full","pauseCovered":true,"rotationEnabled":false,"rotationMinutes":30,"rotationKind":"all","rotationShuffle":true,"palettePreset":"wallpaper","paletteMode":"dark","paletteAutoMode":false,"paletteHarmony":false,"palettePersonality":"natural"});
        for (key, value) in saved.as_object().ok_or("Invalid picker preferences")? {
            result[key] = value.clone();
        }
        let personality = result["palettePersonality"].clone();
        if result.get("paletteBackgroundFromWallpaper").is_none() {
            result["paletteBackgroundFromWallpaper"] = json!(personality != "natural");
        }
        if personality == "source" {
            result["palettePersonality"] = json!("natural");
        }
        if json::truthy(&result["rotationEnabled"]) && !json::truthy(&result["rotationAnchorMs"]) {
            result["rotationAnchorMs"] = json!(if prefs.exists() {
                let meta = fs::metadata(&prefs)?;
                ((meta.mtime() as f64 + meta.mtime_nsec() as f64 / 1e9) * 1000.0) as i64
            } else {
                0
            });
        }
        Ok(result)
    }
    pub fn media_state(&self) -> Result<Value> {
        let path = self.wallpaper_state().join("media.json");
        let mut value = state::read(&path, json!({}))?;
        if json::truthy(&value) && !json::truthy(&value["appliedAtMs"]) {
            let meta = fs::metadata(&path)?;
            value["appliedAtMs"] =
                json!(((meta.mtime() as f64 + meta.mtime_nsec() as f64 / 1e9) * 1000.0) as i64);
        }
        Ok(value)
    }
    pub fn palette_presets(&self) -> Result<Value> {
        let mut presets = state::read(&self.assets.join("palette-presets.json"), json!([]))?;
        let stored = crate::owned_settings::read(
            &self.paths.config.join("colors.json"),
            "colors",
            json!({}),
        )?;
        let id = regex::Regex::new(r"^[0-9a-f]{16}$")?;
        if let Some(favorites) = stored["favorites"].as_array() {
            for favorite in favorites {
                if !favorite["id"].as_str().is_some_and(|s| id.is_match(s)) {
                    continue;
                }
                presets.as_array_mut().ok_or("Invalid palette presets")?.push(json!({"id":format!("favorite:{}",favorite["id"].as_str().unwrap()),"name":favorite["name"],"seed":favorite["palette"]["colours"]["overtone"],"group":"saved","swatches":favorite["swatches"],"surface":favorite["modes"]["dark"]["frame"],"modes":favorite["modes"]}));
            }
        }
        presets
            .as_array_mut()
            .ok_or("Invalid palette presets")?
            .extend(
                crate::theme_bundles::catalog(&self.assets)?
                    .as_array()
                    .ok_or("Invalid bundles")?
                    .clone(),
            );
        Ok(presets)
    }
    pub fn preference(&self, value: &Value) -> Result<Value> {
        let mut value = value.clone();
        if value["palettePersonality"] == "source" {
            value["palettePersonality"] = json!("natural");
            if value.get("paletteBackgroundFromWallpaper").is_none() {
                value["paletteBackgroundFromWallpaper"] = json!(true);
            }
        }
        let presets = self.palette_presets()?;
        let mut palette_ids = vec![json!("wallpaper")];
        palette_ids.extend(
            presets
                .as_array()
                .ok_or("Invalid presets")?
                .iter()
                .map(|p| p["id"].clone()),
        );
        let choices = [
            ("kind", vec![json!("static"), json!("dynamic")]),
            (
                "motionMode",
                vec![json!("full"), json!("battery"), json!("still")],
            ),
            (
                "layout",
                vec![json!("carousel"), json!("spotlight"), json!("hexagons")],
            ),
            (
                "rotationKind",
                vec![json!("all"), json!("static"), json!("dynamic")],
            ),
            ("paletteMode", vec![json!("dark"), json!("light")]),
            (
                "palettePersonality",
                [
                    "natural", "pigment", "harmony", "pop", "mist", "vivid", "pearl", "tide",
                ]
                .into_iter()
                .map(|s| json!(s))
                .collect(),
            ),
            ("palettePreset", palette_ids),
        ];
        let booleans = [
            "paused",
            "pauseCovered",
            "rotationEnabled",
            "rotationShuffle",
            "paletteHarmony",
            "paletteAutoMode",
            "paletteBackgroundFromWallpaper",
        ];
        let Some(object) = value.as_object().filter(|values| {
            values.keys().all(|key| {
                choices.iter().any(|(name, _)| *name == key)
                    || booleans.contains(&key.as_str())
                    || key == "rotationMinutes"
            })
        }) else {
            return Err(tr(
                "helper.wallpaper-media.unknown-picker-preference",
                "Unknown picker preference",
            )
            .into());
        };
        for (key, value) in object {
            if choices
                .iter()
                .any(|(name, allowed)| *name == key && !allowed.contains(value))
            {
                return Err(tr(
                    "helper.wallpaper-media.invalid-picker-preference",
                    "Invalid picker preference",
                )
                .into());
            }
            if booleans.contains(&key.as_str()) && !value.is_boolean() {
                return Err(tr(
                    "helper.wallpaper-media.expected-a-boolean-preference",
                    "Expected a boolean preference",
                )
                .into());
            }
            if key == "rotationMinutes" && !value.as_i64().is_some_and(|n| (5..=1440).contains(&n))
            {
                return Err(tr(
                    "helper.wallpaper-media.choose-a-wallpaper-interval-between-5-and-1440-minutes",
                    "Choose a wallpaper interval between 5 and 1440 minutes",
                )
                .into());
            }
        }
        let prefs = self.preferences_path();
        fs::create_dir_all(prefs.parent().ok_or("Missing preference directory")?)?;
        let _guard = crate::files::lock(&prefs.with_extension("lock"))?;
        let mut document =
            crate::owned_settings::Document::open(&prefs, "wallpaper-picker", json!({}))?;
        let mut result = self.settings_with(document.data.clone())?;
        let previous = result.clone();
        for (key, value) in object {
            result[key] = value.clone();
        }
        if (json::truthy(&result["rotationEnabled"]) && !json::truthy(&previous["rotationEnabled"]))
            || ["rotationMinutes", "rotationKind"]
                .iter()
                .any(|key| object.get(*key).is_some_and(|v| *v != previous[*key]))
        {
            result["rotationAnchorMs"] = json!(self
                .now_ms
                .unwrap_or_else(|| chrono::Utc::now().timestamp_millis()));
        }
        if !document.save(&result)? {
            atomic(&prefs, &result)?;
        }
        Ok(result)
    }
    pub fn preview_key(poster: &Path) -> Result<String> {
        let meta = fs::metadata(poster)?;
        Ok(format!(
            "{:x}",
            Sha256::digest(
                format!(
                    "{}{}{}preview-v1",
                    poster.display(),
                    meta.len(),
                    nanos(&meta)
                )
                .as_bytes()
            )
        ))
    }
    pub fn previews(&self, poster: &Path) -> Result<Value> {
        let key = Self::preview_key(poster)?;
        let cache = self.cache();
        fs::create_dir_all(&cache)?;
        let thumb = cache.join(format!("{key}-thumbnail.jpg"));
        let preview = cache.join(format!("{key}-preview.jpg"));
        if !cached_thumbnail(&thumb) || !cached_preview(&preview) {
            let image =
                image::DynamicImage::ImageRgba8(orient_decoder::load_rgba_oriented(poster)?)
                    .to_rgb8();
            if !cached_thumbnail(&thumb) {
                let resized = orient_decoder::thumbnail_rgb(
                    &image,
                    (640, 400),
                    orient_decoder::Filter::Lanczos,
                )?;
                let mut encoded = None;
                for quality in [88, 80, 72, 64, 56, 48] {
                    let bytes = crate::image_paint::encode_jpeg(&resized, quality)?;
                    if bytes.len() as u64 <= THUMBNAIL_LIMIT_BYTES {
                        encoded = Some(bytes);
                        break;
                    }
                }
                let bytes = encoded.ok_or("Wallpaper thumbnail exceeds the cache entry limit")?;
                crate::files::atomic(&thumb, &bytes)?;
            }
            if !cached_preview(&preview) {
                let resized = orient_decoder::thumbnail_rgb(
                    &image,
                    (1600, 1000),
                    orient_decoder::Filter::Lanczos,
                )?;
                let mut encoded = None;
                for quality in [88, 80, 72, 64, 56, 48] {
                    let bytes = crate::image_paint::encode_jpeg(&resized, quality)?;
                    if bytes.len() as u64 <= PREVIEW_LIMIT_BYTES {
                        encoded = Some(bytes);
                        break;
                    }
                }
                let bytes = encoded.ok_or("Wallpaper preview exceeds the cache entry limit")?;
                crate::files::atomic(&preview, &bytes)?;
            }
        }
        trim_thumbnail_cache(&cache, &key)?;
        Ok(json!({"thumbnail":thumb,"preview":preview}))
    }
    pub fn describe(&self, path: &Path) -> Result<Value> {
        let path = crate::paths::weak_canonical(&crate::paths::expand(path)?)?;
        let suffix = path
            .extension()
            .map(|s| s.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if !path.is_file()
            || (!IMAGES.contains(&suffix.as_str()) && !VIDEOS.contains(&suffix.as_str()))
        {
            return Err(tr(
                "helper.wallpaper-media.select-a-local-image-gif-or-video",
                "Select a local image, GIF or video",
            )
            .into());
        }
        let animated = if suffix == "gif" {
            let mut options = gif::DecodeOptions::new();
            options.skip_frame_decoding(true);
            let mut decoder = options.read_info(fs::File::open(&path)?)?;
            let mut count = 0;
            while decoder.read_next_frame()?.is_some() {
                count += 1;
            }
            count > 1
        } else {
            false
        };
        let dynamic = VIDEOS.contains(&suffix.as_str()) || animated;
        let mut poster = path.clone();
        if dynamic {
            let meta = fs::metadata(&path)?;
            let key = format!(
                "{:x}",
                Sha256::digest(
                    format!("{}{}{}", path.display(), meta.len(), nanos(&meta)).as_bytes()
                )
            );
            let cache = self.cache();
            fs::create_dir_all(&cache)?;
            poster = cache.join(format!("{key}.png"));
            if !poster.exists() {
                let temporary = tempfile::Builder::new()
                    .prefix("tmp")
                    .suffix(".png")
                    .tempfile_in(&cache)?;
                if animated {
                    fs::write(
                        temporary.path(),
                        orient_decoder::rgb_preview_png(&path, (1920, 1080), 0.0)?,
                    )?;
                } else {
                    let args = ["-nostdin", "-v", "error", "-threads", "2", "-i"]
                        .map(str::to_owned)
                        .into_iter()
                        .chain([
                            path.to_string_lossy().into_owned(),
                            "-frames:v".into(),
                            "1".into(),
                            "-vf".into(),
                            "scale=1920:1080:force_original_aspect_ratio=decrease".into(),
                            "-y".into(),
                            temporary.path().to_string_lossy().into_owned(),
                        ])
                        .collect::<Vec<_>>();
                    let output =
                        crate::process::run_bytes("ffmpeg", &args, None, Duration::from_secs(30))?;
                    if !output.status.success() {
                        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                        eprintln!("Video poster extraction: {stderr}");
                        return Err(ToolFailure {
                            command: std::iter::once("ffmpeg".into()).chain(args).collect(),
                            code: output.status.code().unwrap_or(1),
                            stderr,
                        }
                        .into());
                    }
                }
                orient_decoder::load_rgba(temporary.path())?;
                use std::os::unix::fs::PermissionsExt;
                fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o600))?;
                temporary.persist(&poster)?;
            }
        }
        let preview = self.previews(&poster)?;
        Ok(
            json!({"path":path,"name":path.file_stem().unwrap_or_default().to_string_lossy().replace('_'," "),"poster":poster,"dynamic":dynamic,"animated":animated,"thumbnail":preview["thumbnail"],"preview":preview["preview"]}),
        )
    }
    pub fn catalog(&self) -> Result<Value> {
        fs::create_dir_all(&self.library)?;
        fs::create_dir_all(self.library.join("Dynamic"))?;
        let mut entries = walkdir::WalkDir::new(&self.library)
            .min_depth(1)
            .into_iter()
            .map(|entry| {
                entry
                    .map(|entry| entry.path().to_owned())
                    .map_err(Into::into)
            })
            .collect::<Result<Vec<_>>>()?;
        entries.sort_by_key(|path| path.to_string_lossy().case_fold().collect::<String>());
        let (mut rows, mut errors) = (Vec::new(), Vec::new());
        for path in entries {
            let suffix = path
                .extension()
                .map(|s| s.to_string_lossy().to_lowercase())
                .unwrap_or_default();
            if path.is_file()
                && (IMAGES.contains(&suffix.as_str()) || VIDEOS.contains(&suffix.as_str()))
            {
                match self.describe(&path) {
                    Ok(value) => rows.push(value),
                    Err(_) => errors.push(
                        path.file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                    ),
                }
            }
        }
        let presets = self.palette_presets()?;
        let palettes=presets.as_array().ok_or("Invalid palette presets")?.iter().map(|item|json!({"id":item["id"],"name":item["name"],"seed":item["seed"],"group":item["group"],"swatches":[item["modes"]["dark"]["primary"],item["modes"]["dark"]["secondary"],item["modes"]["dark"]["tertiary"]],"surface":item["modes"]["dark"]["surface"]})).collect::<Vec<_>>();
        Ok(
            json!({"entries":rows,"preferences":self.settings()?,"media":self.media_state()?,"errors":errors,"palettes":palettes}),
        )
    }
    pub fn prepared_theme(
        &self,
        publisher: &mut crate::classic_state::Publisher,
        poster: &Path,
        live: bool,
    ) -> Result<bool> {
        let scheme = state::read(&self.paths.state.join("scheme.json"), json!({}))?;
        let settings = self.settings()?;
        let mode = settings["paletteMode"]
            .as_str()
            .or_else(|| scheme["mode"].as_str())
            .unwrap_or("dark");
        let marker = self.palette_marker(
            poster,
            scheme["flavour"].as_str().unwrap_or("default"),
            None,
            None,
            Some(mode),
            None,
        )?;
        if !marker.exists() {
            return Ok(false);
        }
        let outcome = (|| -> Result<bool> {
            let data = state::read(&marker, json!({}))?;
            if !data.is_object() || !data["input"].is_object() || !data["source"].is_object() {
                return Ok(false);
            }
            if data["input"]["harmony"]
                != settings
                    .get("paletteHarmony")
                    .cloned()
                    .unwrap_or(json!(false))
            {
                return Ok(false);
            }
            let Some(digest) = data["source"]["digest"].as_str().filter(|value| {
                value.len() == 64
                    && value
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            }) else {
                return Ok(false);
            };
            let thumbnail = self
                .paths
                .cache
                .join("wallpapers")
                .join(digest)
                .join("thumbnail.jpg");
            publisher.commit_prepared(poster, &data, &thumbnail, live, true)
        })();
        match outcome {
            Ok(value) => Ok(value),
            Err(error) => {
                eprintln!("Prepared wallpaper treatment: {error}");
                Ok(false)
            }
        }
    }
    fn execute_cli(
        &self,
        cli: &mut crate::palette_cli::Cli,
        words: Vec<String>,
        command: Vec<String>,
    ) -> Result<()> {
        if let Some(operation) = &self.palette_command {
            return operation(&command).map(|_| ());
        }
        // The previous child had captured/discarded stdout; keep the same stream boundary.
        let stream = std::env::var_os("NACRE_PRESENTATION_STREAM");
        std::env::remove_var("NACRE_PRESENTATION_STREAM");
        let result = crate::palette_cli::parse(&words).and_then(|args| cli.execute(&args));
        if let Some(value) = stream {
            std::env::set_var("NACRE_PRESENTATION_STREAM", value);
        }
        result.map(|_| ()).map_err(|error| {
            let code = if error.is::<crate::palette_cli::Usage>() {
                2
            } else {
                1
            };
            eprintln!("Wallpaper palette publication: {error}");
            Box::new(ToolFailure {
                command,
                code,
                stderr: error.to_string(),
            }) as Box<dyn std::error::Error + Send + Sync>
        })
    }
    pub fn theme(&self, cli: &mut crate::palette_cli::Cli, value: &Value) -> Result<Value> {
        let allowed = [
            "palettePreset",
            "paletteMode",
            "paletteAutoMode",
            "paletteAccent",
            "paletteHarmony",
            "palettePersonality",
            "paletteBackgroundFromWallpaper",
        ];
        let Some(values) = value
            .as_object()
            .filter(|v| !v.is_empty() && v.keys().all(|key| allowed.contains(&key.as_str())))
        else {
            return Err(tr(
                "helper.wallpaper-media.unknown-appearance-preference",
                "Unknown appearance preference",
            )
            .into());
        };
        let accent = values.get("paletteAccent").filter(|v| !v.is_null());
        if accent.is_some_and(|value| {
            !value.as_str().is_some_and(|value| {
                value == "auto"
                    || (value.len() == 6 && value.bytes().all(|b| b.is_ascii_hexdigit()))
            })
        }) {
            return Err(tr(
                "helper.wallpaper-media.choose-a-valid-wallpaper-accent",
                "Choose a valid wallpaper accent",
            )
            .into());
        }
        let previous = self.settings()?;
        let mut changes = values.clone();
        changes.shift_remove("paletteAccent");
        if values.contains_key("paletteMode") && !values.contains_key("paletteAutoMode") {
            changes.insert("paletteAutoMode".into(), json!(false));
        }
        if let Some(value) = values.get("paletteHarmony") {
            changes.insert(
                "palettePersonality".into(),
                json!(if json::truthy(value) {
                    "harmony"
                } else {
                    "natural"
                }),
            );
        }
        if let Some(value) = values.get("palettePersonality") {
            changes.insert("paletteHarmony".into(), json!(value == "harmony"));
        }
        if accent.is_some() {
            changes.insert("palettePreset".into(), json!("wallpaper"));
        }
        let active = state::read(&self.wallpaper_state().join("media.json"), json!({}))?;
        let poster = match active["poster"].as_str().filter(|v| !v.is_empty()) {
            Some(value) => value.to_owned(),
            None => match fs::read_to_string(self.wallpaper_state().join("last.txt")) {
                Ok(value) => value.trim().to_owned(),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
                Err(error) => return Err(error.into()),
            },
        };
        if poster.is_empty() || !Path::new(&poster).is_file() {
            return Err(tr(
                "helper.wallpaper-media.choose-a-wallpaper-before-changing-its-palette",
                "Choose a wallpaper before changing its palette",
            )
            .into());
        }
        if json::truthy(
            changes
                .get("paletteAutoMode")
                .unwrap_or(&previous["paletteAutoMode"]),
        ) {
            changes.insert(
                "paletteMode".into(),
                json!(Self::wallpaper_mode(Path::new(&poster))?),
            );
        }
        let result = self.preference(&json!(changes))?;
        let outcome = (|| -> Result<()> {
            if result["palettePreset"] == "wallpaper"
                && accent.is_none()
                && self.prepared_theme(&mut cli.publisher, Path::new(&poster), true)?
            {
                return Ok(());
            }
            let words = if let Some(accent) = accent {
                let mut words = vec!["scheme".into(), "set".into(), "-n".into(), "dynamic".into()];
                if let Some(mode) = changes.get("paletteMode") {
                    words.extend([
                        "-m".into(),
                        mode.as_str().ok_or("Invalid palette mode")?.into(),
                    ]);
                }
                if accent == "auto" {
                    words.push("--auto-accent".into());
                } else {
                    words.extend(["--accent".into(), accent.as_str().unwrap().into()]);
                }
                words
            } else if let Some(mode) = changes.get("paletteMode") {
                vec![
                    "scheme".into(),
                    "set".into(),
                    "-m".into(),
                    mode.as_str().ok_or("Invalid palette mode")?.into(),
                ]
            } else {
                vec![
                    "wallpaper".into(),
                    "-f".into(),
                    poster.clone(),
                    "--no-smart".into(),
                ]
            };
            let mut command = vec![self
                .paths
                .share
                .join("shell/bin/nacre_shell")
                .to_string_lossy()
                .into_owned()];
            command.extend(words.clone());
            self.execute_cli(cli, words, command)
        })();
        if let Err(error) = outcome {
            let rollback: serde_json::Map<_, _> = changes
                .keys()
                .map(|key| (key.clone(), previous[key].clone()))
                .collect();
            self.preference(&json!(rollback))?;
            return Err(error);
        }
        Ok(result)
    }
    pub fn select(&self, cli: &mut crate::palette_cli::Cli, path: &Path) -> Result<Value> {
        let previous = self.settings()?;
        let outcome = self.select_inner(cli, path);
        if outcome.is_err() && json::truthy(&previous["paletteAutoMode"]) {
            self.preference(&json!({"paletteMode":previous["paletteMode"]}))?;
        }
        outcome
    }
    fn select_inner(&self, cli: &mut crate::palette_cli::Cli, path: &Path) -> Result<Value> {
        let mut item = self.describe(path)?;
        let poster = PathBuf::from(item["poster"].as_str().ok_or("Missing wallpaper poster")?);
        let automatic = json::truthy(&self.settings()?["paletteAutoMode"]);
        if automatic {
            self.preference(&json!({"paletteMode":Self::wallpaper_mode(&poster)?}))?;
        }
        let scheme = state::read(&self.paths.state.join("scheme.json"), json!({}))?;
        let settings = self.settings()?;
        let marker = self.palette_marker(
            &poster,
            scheme["flavour"].as_str().unwrap_or("default"),
            None,
            item["path"].as_str().map(Path::new),
            if automatic {
                settings["paletteMode"].as_str()
            } else {
                None
            },
            None,
        )?;
        let mut prepared = false;
        if marker.exists() {
            let result = (|| -> Result<bool> {
                let data = state::read(&marker, json!({}))?;
                let digest = format!("{:x}", Sha256::digest(fs::read(&poster)?));
                let thumbnail = self
                    .paths
                    .cache
                    .join("wallpapers")
                    .join(digest)
                    .join("thumbnail.jpg");
                if !thumbnail.exists() {
                    fs::create_dir_all(thumbnail.parent().ok_or("Missing wallpaper cache")?)?;
                    let image = image::DynamicImage::ImageRgba8(orient_decoder::load_rgba(
                        Path::new(item["preview"].as_str().ok_or("Missing preview")?),
                    )?)
                    .to_rgb8();
                    fs::write(&thumbnail, crate::image_paint::encode_jpeg(&image, 85)?)?;
                }
                cli.publisher
                    .commit_prepared(&poster, &data, &thumbnail, true, automatic)
            })();
            match result {
                Ok(value) => prepared = value,
                Err(error) => eprintln!("Prepared wallpaper selection: {error}"),
            }
        }
        if !prepared {
            let mut words = vec![
                "wallpaper".into(),
                "--file".into(),
                poster.to_string_lossy().into_owned(),
            ];
            if automatic {
                words.extend([
                    "--mode".into(),
                    settings["paletteMode"]
                        .as_str()
                        .ok_or("Invalid palette mode")?
                        .into(),
                ]);
            }
            let legacy = self.paths.home.join(".local/bin/nacre-shell");
            let bridge = self.paths.share.join("shell/bin/nacre_shell");
            let mut command = if legacy.is_file() && !automatic {
                vec![
                    legacy.to_string_lossy().into_owned(),
                    "wallpaper-image".into(),
                    poster.to_string_lossy().into_owned(),
                ]
            } else {
                let path = if bridge.is_file() {
                    bridge
                } else {
                    self.assets.join("cli-bridge.sh")
                };
                let mut command = vec![path.to_string_lossy().into_owned()];
                command.extend(words.clone());
                command
            };
            if legacy.is_file() && automatic && command.len() == 3 {
                command.extend([
                    "--mode".into(),
                    settings["paletteMode"].as_str().unwrap().into(),
                ]);
            }
            self.execute_cli(cli, words, command)?;
        }
        item["appliedAtMs"] = json!(self
            .now_ms
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis()));
        atomic(&self.wallpaper_state().join("media.json"), &item)?;
        Ok(item)
    }
    fn prepare_login(&self, poster: &Path) -> Result<()> {
        crate::login_appearance::prepare(&self.paths, poster, None)?;
        Ok(())
    }
    fn query_palette(
        &self,
        cli: &mut crate::palette_cli::Cli,
        words: Vec<String>,
    ) -> Result<Value> {
        let text = if let Some(operation) = &self.palette_command {
            let mut command = vec![self
                .paths
                .share
                .join("shell/bin/nacre_shell")
                .to_string_lossy()
                .into_owned()];
            command.extend(words.clone());
            operation(&command)?.ok_or("Palette query returned no data")?
        } else {
            let args = crate::palette_cli::parse(&words)?;
            cli.execute(&args)?
                .ok_or("Palette query returned no data")?
        };
        Ok(serde_json::from_str(&text)?)
    }
    fn prepare_thumbnail(&self, scene: &Value, candidate: &Value) -> Result<()> {
        let Some(digest) = candidate["source"]["digest"].as_str().filter(|v| {
            v.len() == 64
                && v.bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
        }) else {
            return Ok(());
        };
        let path = self
            .paths
            .cache
            .join("wallpapers")
            .join(digest)
            .join("thumbnail.jpg");
        if !path.exists() {
            fs::create_dir_all(path.parent().ok_or("Missing wallpaper thumbnail folder")?)?;
            let image = Path::new(
                scene["preview"]
                    .as_str()
                    .or_else(|| scene["poster"].as_str())
                    .ok_or("Missing scene preview")?,
            );
            let image =
                image::DynamicImage::ImageRgba8(orient_decoder::load_rgba(image)?).to_rgb8();
            fs::write(path, crate::image_paint::encode_jpeg(&image, 85)?)?;
        }
        Ok(())
    }
    pub fn warm(&self, cli: &mut crate::palette_cli::Cli, include_demo: bool) -> Result<Value> {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let cache = self.cache();
        fs::create_dir_all(&cache)?;
        let path = cache.join("warm.lock");
        let lock = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&path)?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        let _guard =
            match nix::fcntl::Flock::lock(lock, nix::fcntl::FlockArg::LockExclusiveNonblock) {
                Ok(guard) => guard,
                Err((_, nix::errno::Errno::EAGAIN)) => return Ok(json!({"busy":true})),
                Err((_, error)) => return Err(error.into()),
            };
        let available = self.paths.share.join("shell/bin/nacre_shell").exists();
        let current = self.media_state()?;
        let mut scenes = if json::truthy(&current["poster"]) {
            vec![current]
        } else {
            Vec::new()
        };
        if include_demo && self.assets.join("demo-wallpapers").is_dir() {
            for scene in self
                .demo_scenes()?
                .as_array()
                .ok_or("Invalid demo scenes")?
            {
                scenes.push(self.describe(Path::new(
                    scene["path"].as_str().ok_or("Missing demo path")?,
                ))?);
            }
        }
        let scheme = state::read(&self.paths.state.join("scheme.json"), json!({}))?;
        let flavour = scheme["flavour"].as_str().unwrap_or("default");
        for scene in scenes {
            let Some(poster) = scene["poster"]
                .as_str()
                .filter(|p| !p.is_empty())
                .map(Path::new)
            else {
                continue;
            };
            if !available || !poster.is_file() {
                continue;
            }
            self.prepare_login(poster)?;
            for mode in ["dark", "light"] {
                for personality in [
                    "natural", "pigment", "harmony", "pop", "mist", "vivid", "pearl", "tide",
                ] {
                    let source = scene["path"]
                        .as_str()
                        .unwrap_or_else(|| poster.to_str().unwrap_or(""));
                    let marker = self.palette_marker(
                        poster,
                        flavour,
                        Some(personality == "harmony"),
                        Some(Path::new(source)),
                        Some(mode),
                        Some(personality),
                    )?;
                    if marker.exists() {
                        let repair = (|| -> Result<()> {
                            let value = state::read(&marker, json!({}))?;
                            crate::branding::prepare(
                                value["colours"]
                                    .as_object()
                                    .ok_or("Missing cached branding colors")?,
                                &self.paths,
                                &self.assets,
                            )
                        })();
                        match repair {
                            Ok(()) => continue,
                            Err(error) => eprintln!("Warm branding cache repair: {error}"),
                        }
                    }
                    let result = (|| -> Result<()> {
                        let candidate = self.query_palette(
                            cli,
                            vec![
                                "wallpaper".into(),
                                "-p".into(),
                                poster.to_string_lossy().into_owned(),
                                "--source".into(),
                                source.into(),
                                "--mode".into(),
                                mode.into(),
                                "--personality".into(),
                                personality.into(),
                            ],
                        )?;
                        atomic(&marker, &candidate)?;
                        crate::branding::prepare(
                            candidate["colours"]
                                .as_object()
                                .ok_or("Missing generated branding colors")?,
                            &self.paths,
                            &self.assets,
                        )?;
                        self.prepare_thumbnail(&scene, &candidate)
                    })();
                    if let Err(error) = result {
                        eprintln!("Wallpaper treatment preparation: {error}");
                    }
                }
            }
        }
        let mut prepared = 0;
        let mut computed = 0;
        let scheme = state::read(&self.paths.state.join("scheme.json"), json!({}))?;
        let flavour = scheme["flavour"].as_str().unwrap_or("default");
        let catalogue = self.catalog()?;
        for item in catalogue["entries"]
            .as_array()
            .ok_or("Invalid wallpaper catalog")?
        {
            let result = (|| -> Result<()> {
                let poster = Path::new(item["poster"].as_str().ok_or("Missing catalog poster")?);
                let source = Path::new(item["path"].as_str().ok_or("Missing catalog source")?);
                let marker =
                    self.palette_marker(poster, flavour, None, Some(source), None, None)?;
                if available && !marker.exists() {
                    let value = self.query_palette(
                        cli,
                        vec![
                            "wallpaper".into(),
                            "-p".into(),
                            poster.to_string_lossy().into_owned(),
                            "--source".into(),
                            source.to_string_lossy().into_owned(),
                        ],
                    )?;
                    atomic(&marker, &value)?;
                    computed += 1;
                }
                self.prepare_login(poster)?;
                prepared += 1;
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("Wallpaper catalog preparation: {error}");
            }
        }
        Ok(json!({"prepared":prepared,"computed":computed}))
    }
    pub fn demo_scenes(&self) -> Result<Value> {
        let preferences = state::read(&self.paths.config.join("welcome.json"), json!({}))?;
        if let Some(values) = preferences["demoWallpapers"]
            .as_array()
            .filter(|values| values.len() == 4 && values.iter().all(Value::is_string))
        {
            let unique = values
                .iter()
                .filter_map(Value::as_str)
                .collect::<std::collections::BTreeSet<_>>();
            if unique.len() == 4 {
                let result = values
                    .iter()
                    .map(|value| -> Result<Value> {
                        let mut item = self.describe(Path::new(value.as_str().unwrap()))?;
                        for key in ["license", "artist", "source", "licenseUrl"] {
                            item[key] = json!("");
                        }
                        Ok(item)
                    })
                    .collect::<Result<Vec<_>>>();
                match result {
                    Ok(values) => return Ok(json!(values)),
                    Err(error) => eprintln!("Private demo scenes: {error}"),
                }
            }
        }
        crate::demo_wallpapers::ensure(&self.paths, &self.assets.join("demo-wallpapers"), true)
    }
    pub fn demo_start(&self) -> Result<Value> {
        self.demo_start_with_token(&uuid::Uuid::new_v4().simple().to_string())
    }
    pub fn demo_start_with_token(&self, token: &str) -> Result<Value> {
        self.demo_start_with_scenes(token, || self.demo_scenes())
    }
    /// The caller supplies only the asset provider; snapshot/publication rules stay here.
    pub fn demo_start_with_scenes<F>(&self, token: &str, load: F) -> Result<Value>
    where
        F: FnOnce() -> Result<Value>,
    {
        let folder = self.paths.state.join("welcome/demo");
        fs::create_dir_all(&folder)?;
        let scenes;
        {
            let _guard = crate::classic_state::publication_lock(&self.paths)?;
            let mut scheme = state::read(&self.paths.state.join("scheme.json"), Value::Null)?;
            if scheme.is_null() {
                scheme = state::read(&self.assets.join("reference-style.json"), json!({}))?;
            }
            crate::classic_state::validated_colors(&scheme, &self.assets)?;
            let mut original = self.media_state()?;
            let mut poster = original["poster"]
                .as_str()
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .unwrap_or_default();
            if poster.is_empty() {
                match fs::read_to_string(self.wallpaper_state().join("last.txt")) {
                    Ok(value) => poster = value.trim().into(),
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
                    Err(error) => return Err(error.into()),
                }
            }
            if poster.is_empty() {
                let color = scheme["colours"]["background"]
                    .as_str()
                    .ok_or("Missing background color")?;
                let blank = self
                    .paths
                    .cache
                    .join("demo-wallpapers")
                    .join(format!("starting-{color}.png"));
                fs::create_dir_all(blank.parent().ok_or("Missing demo cache")?)?;
                if !blank.exists() {
                    let value = orient_core::colour::rgb(&format!("#{color}"))?
                        .map(|v| (v * 255.0).round() as u8);
                    let image = image::RgbImage::from_pixel(16, 16, image::Rgb(value));
                    fs::write(&blank, orient_decoder::encode_rgb_png(&image, 6)?)?;
                }
                poster = blank.to_string_lossy().into_owned();
            }
            if !Path::new(&poster).is_file() {
                return Err(tr("helper.wallpaper-media.your-starting-wallpaper-is-unavailable-restore-its-file-before-tr","Your starting wallpaper is unavailable. Restore its file before trying the demo.").into());
            }
            if !json::truthy(&original) {
                original = self.describe(Path::new(&poster))?;
            }
            let preferences = state::read(&self.preferences_path(), json!({}))?;
            let selected: serde_json::Map<_, _> = crate::classic_state::DEMO_PREFERENCES
                .iter()
                .filter_map(|key| {
                    preferences
                        .get(*key)
                        .map(|value| ((*key).into(), value.clone()))
                })
                .collect();
            let snapshot = json!({"version":1,"scheme":scheme,"poster":poster,"media":original,"preferences":selected});
            atomic(&folder.join(format!("{token}.json")), &snapshot)?;
            let mut entries = fs::read_dir(&folder)?
                .map(|entry| -> Result<_> {
                    let path = entry?.path();
                    let modified = fs::metadata(&path)?.modified()?;
                    Ok((modified, path))
                })
                .collect::<Result<Vec<_>>>()?;
            entries.retain(|(_, path)| path.extension().is_some_and(|ext| ext == "json"));
            entries.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
            for (_, old) in entries.into_iter().skip(10) {
                fs::remove_file(old)?;
            }
            scenes = load()?;
        }
        let entries = scenes
            .as_array()
            .ok_or("Invalid demo scenes")?
            .iter()
            .map(|scene| -> Result<Value> {
                let mut entry = self.describe(Path::new(
                    scene["path"].as_str().ok_or("Missing demo path")?,
                ))?;
                entry["name"] = scene["name"].clone();
                entry["license"] = scene["license"].clone();
                for key in ["artist", "source", "licenseUrl"] {
                    entry[key] = scene.get(key).cloned().unwrap_or(json!(""));
                }
                Ok(entry)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(json!({"demoEntries":entries,"snapshot":token}))
    }
    pub fn demo_restore(
        &self,
        cli: &mut crate::palette_cli::Cli,
        token: Option<&str>,
    ) -> Result<Value> {
        let snapshot = self.demo_snapshot(token)?;
        let item = cli.publisher.restore_demo(&snapshot, true)?;
        Ok(json!({"media":item,"preferences":self.settings()?}))
    }
    pub fn demo_snapshot(&self, token: Option<&str>) -> Result<Value> {
        let token = token
            .filter(|token| {
                token.len() == 32
                    && token
                        .bytes()
                        .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
            })
            .ok_or_else(|| {
                tr(
                    "helper.wallpaper-media.reopen-welcome-to-capture-a-starting-point",
                    "Reopen Welcome to capture a starting point.",
                )
                .to_string()
            })?;
        let snapshot = state::read(
            &self
                .paths
                .state
                .join("welcome/demo")
                .join(format!("{token}.json")),
            json!({}),
        )?;
        if snapshot["version"] != 1 {
            return Err(tr("helper.wallpaper-media.the-demo-starting-point-expired-reopen-welcome-to-try-again","The demo starting point expired. Reopen Welcome to try again.").into());
        }
        Ok(snapshot)
    }
    pub fn pick(&self) -> Result<Value> {
        let mut command = std::process::Command::new("zenity");
        command.args(["--file-selection","--multiple","--separator=\n","--title=Add wallpapers","--file-filter=Wallpapers | *.png *.jpg *.jpeg *.webp *.gif *.mp4 *.webm *.mkv *.mov *.m4v"]);
        crate::process::restore_child_signals(&mut command);
        let output = command.output().map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Box::<dyn std::error::Error + Send + Sync>::from(
                    "[Errno 2] No such file or directory: 'zenity'",
                )
            } else {
                error.into()
            }
        })?;
        if !output.status.success() {
            return Ok(json!({"cancelled":true}));
        }
        let paths = String::from_utf8(output.stdout)?
            .trim()
            .lines()
            .map(|value| json!(value))
            .collect::<Vec<_>>();
        Ok(json!({"imported":self.import_files(&json!(paths))?}))
    }
    pub fn import_files(&self, values: &Value) -> Result<Value> {
        use std::os::unix::fs::PermissionsExt;
        let values = match values {
            Value::Array(values) => values.clone(),
            Value::Object(values) => values.keys().map(|key| json!(key)).collect(),
            Value::String(values) => values
                .chars()
                .map(|value| json!(value.to_string()))
                .collect(),
            _ => {
                return Err(format!(
                    "'{}' object is not iterable",
                    match values {
                        Value::Null => "NoneType",
                        Value::Bool(_) => "bool",
                        Value::Number(number) if number.is_f64() => "float",
                        _ => "int",
                    }
                )
                .into())
            }
        };
        let library = crate::paths::weak_canonical(&self.library)?;
        let mut copied = Vec::new();
        for value in values {
            let value = value
                .as_str()
                .ok_or("expected str, bytes or os.PathLike object")?;
            let decoded = if let Some(tail) = value.strip_prefix("file:") {
                let path = if let Some(authority) = tail.strip_prefix("//") {
                    authority
                        .find('/')
                        .map(|index| &authority[index..])
                        .unwrap_or("")
                } else {
                    tail
                };
                crate::paths::unquote(path.split(['?', '#']).next().unwrap_or(""))
            } else {
                value.to_owned()
            };
            let source = crate::paths::weak_canonical(&crate::paths::expand(Path::new(&decoded))?)?;
            let item = self.describe(&source)?;
            if source.starts_with(&library) {
                copied.push(json!(source));
                continue;
            }
            let folder = self.library.join(if item["dynamic"] == true {
                "Dynamic"
            } else {
                "Static"
            });
            fs::create_dir_all(&folder)?;
            let mut target = folder.join(source.file_name().ok_or("Missing source filename")?);
            let mut index = 1;
            while target.exists() {
                let suffix = source
                    .extension()
                    .map(|v| format!(".{}", v.to_string_lossy()))
                    .unwrap_or_default();
                target = folder.join(format!(
                    "{}-{index}{suffix}",
                    source.file_stem().unwrap_or_default().to_string_lossy()
                ));
                index += 1;
            }
            let temporary = target.with_file_name(format!(
                "{}.importing",
                target.file_name().unwrap().to_string_lossy()
            ));
            let outcome = (|| -> Result<()> {
                fs::copy(&source, &temporary)?;
                let metadata = fs::metadata(&source)?;
                fs::File::options()
                    .write(true)
                    .open(&temporary)?
                    .set_times(
                        fs::FileTimes::new()
                            .set_accessed(metadata.accessed()?)
                            .set_modified(metadata.modified()?),
                    )?;
                fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600))?;
                fs::rename(&temporary, &target)?;
                Ok(())
            })();
            if temporary.exists() {
                fs::remove_file(&temporary)?;
            }
            outcome?;
            copied.push(json!(target));
        }
        Ok(json!(copied))
    }
    pub fn palette_marker(
        &self,
        poster: &Path,
        flavour: &str,
        harmony: Option<bool>,
        source: Option<&Path>,
        mode: Option<&str>,
        personality: Option<&str>,
    ) -> Result<PathBuf> {
        use chrono::Timelike;
        let meta = fs::metadata(poster)?;
        let active = self.media_state()?;
        let selected = source
            .filter(|path| !path.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .or_else(|| {
                if active["poster"].as_str() == poster.to_str() {
                    active["path"].as_str().map(PathBuf::from)
                } else {
                    Some(poster.to_owned())
                }
            });
        let original = selected
            .as_deref()
            .filter(|path| path.is_file())
            .unwrap_or(poster);
        let original_meta = fs::metadata(original)?;
        let runtime = self.paths.share.join("palette-runtime");
        let cli = runtime.join("venv/bin/nacre_shell");
        let marker = runtime.join("orient-build.json");
        let legacy = if marker.exists() {
            fs::read_to_string(marker)?
        } else if cli.exists() {
            nanos(&fs::metadata(cli)?).to_string()
        } else {
            "uninstalled".into()
        };
        let engine = format!("{}:{legacy}", native_engine_identity()?);
        let scheme = state::read(&self.paths.state.join("scheme.json"), json!({}))?;
        let config = state::read(&self.paths.config.join("cli.json"), json!({}))?;
        let settings = self.settings()?;
        let saved = crate::owned_settings::read(
            &self.paths.config.join("colors.json"),
            "colors",
            json!({}),
        )?;
        let absolute = crate::paths::weak_canonical(poster)?;
        let choice = saved["wallpapers"]
            .get(absolute.to_string_lossy().as_ref())
            .cloned()
            .unwrap_or(json!({}));
        let generation = json!({"original":[original,original_meta.len(),nanos(&original_meta),i128::from(original_meta.ctime())*1_000_000_000+i128::from(original_meta.ctime_nsec())],"mode":mode.filter(|value|!value.is_empty()).map(|v|json!(v)).unwrap_or_else(||scheme.get("mode").cloned().unwrap_or(json!("dark"))),"variant":scheme.get("variant").cloned().unwrap_or(json!("tonalspot")),"orient":config.get("orient").cloned().unwrap_or(json!({})),"harmony":harmony.map(|v|json!(v)).unwrap_or_else(||settings.get("paletteHarmony").cloned().unwrap_or(json!(false))),"personality":personality.filter(|v|!v.is_empty()).map(|v|json!(v)).unwrap_or_else(||if let Some(harmony)=harmony {json!(if harmony{"harmony"}else{"natural"})}else{settings.get("palettePersonality").cloned().unwrap_or(json!("natural"))}),"backgroundFromWallpaper":settings.get("paletteBackgroundFromWallpaper").cloned().unwrap_or(json!(false)),"clockHour":if json::truthy(&choice["tideAutomatic"]){json!(self.hour.unwrap_or_else(||chrono::Local::now().hour()))}else{Value::Null},"choices":choice});
        let text = format!(
            "{}{}{}{}{}{}{}{}palette-warm-native-v4",
            poster.display(),
            meta.len(),
            nanos(&meta),
            i128::from(meta.ctime()) * 1_000_000_000 + i128::from(meta.ctime_nsec()),
            meta.ino(),
            flavour,
            engine,
            json::dumps(&generation, true)?
        );
        Ok(self.cache().join(format!(
            "{:x}.palette.json",
            Sha256::digest(text.as_bytes())
        )))
    }
}

const ACTIONS: [&str; 11] = [
    "catalog",
    "demo-start",
    "demo-restore",
    "select",
    "preferences",
    "theme",
    "import",
    "pick",
    "session",
    "warm",
    "serve",
];
const USAGE: &str = "usage: wallpaper-media.py [-h] [--stream-presentation]\n                          {catalog,demo-start,demo-restore,select,preferences,theme,import,pick,session,warm,serve}\n                          [path]\n";
fn argument_error(message: &str) -> i32 {
    eprintln!("{USAGE}wallpaper-media.py: error: {message}");
    2
}
fn error_text(error: &crate::Error) -> String {
    error.to_string().chars().take(240).collect()
}
fn serve(media: &Media, cli: &mut crate::palette_cli::Cli) -> Result<i32> {
    use std::io::{BufRead, Write};
    std::env::set_var("NACRE_PRESENTATION_STREAM", "1");
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    writeln!(output, "{}", json::dumps(&json!({"ready":true}), false)?)?;
    output.flush()?;
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut line = String::new();
    loop {
        line.clear();
        if input.read_line(&mut line)? == 0 {
            return Ok(0);
        }
        let mut rendered = false;
        let result = (|| -> Result<Value> {
            if line.chars().count() > 65536 {
                return Err(tr(
                    "helper.wallpaper-media.palette-request-is-too-large",
                    "Palette request is too large",
                )
                .into());
            }
            let request = json::parse_legacy_value(&line)?;
            let data = request.as_object().ok_or_else(|| {
                match request {
                    Value::Null => "'NoneType' object is not subscriptable",
                    Value::Array(_) => "list indices must be integers or slices, not str",
                    Value::String(_) => "string indices must be integers, not 'str'",
                    Value::Bool(_) => "'bool' object is not subscriptable",
                    _ => "'int' object is not subscriptable",
                }
                .to_string()
            })?;
            let action = data.get("action").ok_or("'action'")?;
            let value = data.get("value").ok_or("'value'")?;
            match (action.as_str(), value) {
                (Some("select"), Value::String(path)) => {
                    rendered = true;
                    media.select(cli, Path::new(path))
                }
                (Some("theme"), Value::Object(_)) => {
                    rendered = true;
                    media.theme(cli, value)
                }
                (Some("preferences"), Value::Object(_)) => media.preference(value),
                _ => Err(tr(
                    "helper.wallpaper-media.invalid-palette-request",
                    "Invalid palette request",
                )
                .into()),
            }
        })();
        let response = match result {
            Ok(value) => json!({"done":true,"code":0,"result":value}),
            Err(error) => json!({"done":true,"code":1,"result":{"error":error_text(&error)}}),
        };
        writeln!(output, "{}", json::dumps(&response, false)?)?;
        output.flush()?;
        drop(response);
        if rendered {
            // Release raster and response/serialization allocations after all
            // request temporaries are dropped, before blocking for another
            // request. Trimming before response construction misses its pages.
            #[cfg(target_env = "gnu")]
            #[allow(unsafe_code)]
            // SAFETY: malloc_trim takes no pointers and is thread-safe. A zero
            // return means no pages were available, not an operation failure.
            unsafe {
                nix::libc::malloc_trim(0);
            }
        }
    }
}
pub fn run(args: &[String]) -> Result<i32> {
    let mut values = Vec::new();
    let mut stream = false;
    let mut watch = false;
    for arg in args {
        if arg == "--help" || arg == "-h" {
            print!("{USAGE}\npositional arguments:\n  {{catalog,demo-start,demo-restore,select,preferences,theme,import,pick,session,warm,serve}}\n  path\n\noptions:\n  -h, --help            show this help message and exit\n  --stream-presentation\n");
            return Ok(0);
        }
        if arg == "--stream-presentation" {
            stream = true;
        } else if arg == "--watch" {
            watch = true;
        } else {
            values.push(arg.clone());
        }
    }
    let Some(action) = values.first() else {
        return Ok(argument_error(
            "the following arguments are required: action",
        ));
    };
    if !ACTIONS.contains(&action.as_str()) {
        return Ok(argument_error(&format!(
            "argument action: invalid choice: {} (choose from {})",
            json::python_repr(&json!(action)),
            ACTIONS
                .iter()
                .map(|v| format!("'{v}'"))
                .collect::<Vec<_>>()
                .join(", ")
        )));
    }
    if values.len() > 2 {
        return Ok(argument_error(&format!(
            "unrecognized arguments: {}",
            values[2..].join(" ")
        )));
    }
    if stream {
        std::env::set_var("NACRE_PRESENTATION_STREAM", "1");
    }
    if watch {
        if action != "session" {
            return Ok(argument_error("unrecognized arguments: --watch"));
        }
        return Media::watch_session();
    }
    if action == "serve" {
        let paths = Paths::environment()?;
        let media = Media::new(paths.clone(), paths.assets())?;
        let mut cli = crate::palette_cli::Cli::environment(true)?;
        return serve(&media, &mut cli);
    }
    let result = (|| -> Result<Value> {
        let paths = Paths::environment()?;
        let media = Media::new(paths.clone(), paths.assets())?;
        let mut cli = crate::palette_cli::Cli::environment(true)?;
        match action.as_str() {
            "catalog" => media.catalog(),
            "pick" => media.pick(),
            "warm" => media.warm(&mut cli, true),
            "preferences" | "theme" | "import" => {
                use std::io::BufRead;
                let mut line = String::new();
                std::io::stdin().lock().read_line(&mut line)?;
                let value = json::parse_legacy_value(&line)?;
                match action.as_str() {
                    "preferences" => media.preference(&value),
                    "theme" => media.theme(&mut cli, &value),
                    _ => Ok(json!({"imported":media.import_files(&value)?})),
                }
            }
            "select" => media.select(
                &mut cli,
                Path::new(
                    values
                        .get(1)
                        .ok_or("expected str, bytes or os.PathLike object, not NoneType")?,
                ),
            ),
            "session" => Media::session(),
            "demo-start" => media.demo_start(),
            "demo-restore" => media.demo_restore(&mut cli, values.get(1).map(String::as_str)),
            _ => Err("Wallpaper action is not yet implemented".into()),
        }
    })();
    let (value, code) = match result {
        Ok(value) => (value, 0),
        Err(error) => (json!({"error":error.to_string()}), 1),
    };
    println!("{}", json::dumps(&value, false)?);
    Ok(code)
}

#[cfg(test)]
mod thumbnail_cache_tests {
    use super::*;

    #[test]
    fn thumbnail_cache_keeps_current_and_caps_entries_and_bytes() -> Result<()> {
        let directory = tempfile::tempdir()?;
        for index in 0..(THUMBNAIL_CACHE_COUNT + 8) {
            let key = format!("{index:064x}");
            fs::write(
                directory.path().join(format!("{key}-thumbnail.jpg")),
                vec![0_u8; 256],
            )?;
            fs::write(
                directory.path().join(format!("{key}-preview.jpg")),
                vec![0_u8; 512],
            )?;
        }
        let current = format!("{:064x}", THUMBNAIL_CACHE_COUNT + 7);
        trim_thumbnail_cache(directory.path(), &current)?;
        let thumbnails = fs::read_dir(directory.path())?
            .filter_map(std::result::Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().ends_with("-thumbnail.jpg"))
            .collect::<Vec<_>>();
        assert_eq!(thumbnails.len(), THUMBNAIL_CACHE_COUNT);
        assert!(cached_thumbnail(
            &directory.path().join(format!("{current}-thumbnail.jpg"))
        ));
        assert!(thumbnails
            .iter()
            .all(|entry| entry.metadata().unwrap().len() <= THUMBNAIL_LIMIT_BYTES));
        Ok(())
    }
}
