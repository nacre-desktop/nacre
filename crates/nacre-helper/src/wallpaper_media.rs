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
pub struct Media {
    pub paths: Paths,
    pub assets: PathBuf,
    pub library: PathBuf,
    pub cache_path: PathBuf,
    pub state_path: PathBuf,
    pub prefs_path: PathBuf,
    pub now_ms: Option<i64>,
    pub hour: Option<u32>,
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
    pub fn settings(&self) -> Result<Value> {
        let prefs = self.preferences_path();
        let mut result = json!({"kind":"static","layout":"carousel","paused":false,"motionMode":"full","pauseCovered":true,"rotationEnabled":false,"rotationMinutes":30,"rotationKind":"all","rotationShuffle":true,"palettePreset":"wallpaper","paletteMode":"dark","paletteAutoMode":false,"paletteHarmony":false,"palettePersonality":"natural"});
        let saved = state::read(&prefs, json!({}))?;
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
        let stored = state::read(&self.paths.config.join("colors.json"), json!({}))?;
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
        let mut result = self.settings()?;
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
        atomic(&prefs, &result)?;
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
        if !thumb.exists() || !preview.exists() {
            let image =
                image::DynamicImage::ImageRgba8(orient_decoder::load_rgba_oriented(poster)?)
                    .to_rgb8();
            for (path, bounds) in [(&thumb, (640, 400)), (&preview, (1600, 1000))] {
                if path.exists() {
                    continue;
                }
                let resized =
                    orient_decoder::thumbnail_rgb(&image, bounds, orient_decoder::Filter::Lanczos)?;
                crate::files::atomic(path, &crate::image_paint::encode_jpeg(&resized, 88)?)?;
            }
        }
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
        let engine = if marker.exists() {
            fs::read_to_string(marker)?
        } else if cli.exists() {
            nanos(&fs::metadata(cli)?).to_string()
        } else {
            "uninstalled".into()
        };
        let scheme = state::read(&self.paths.state.join("scheme.json"), json!({}))?;
        let config = state::read(&self.paths.config.join("cli.json"), json!({}))?;
        let settings = self.settings()?;
        let saved = state::read(&self.paths.config.join("colors.json"), json!({}))?;
        let absolute = crate::paths::weak_canonical(poster)?;
        let choice = saved["wallpapers"]
            .get(absolute.to_string_lossy().as_ref())
            .cloned()
            .unwrap_or(json!({}));
        let generation = json!({"original":[original,original_meta.len(),nanos(&original_meta),i128::from(original_meta.ctime())*1_000_000_000+i128::from(original_meta.ctime_nsec())],"mode":mode.filter(|value|!value.is_empty()).map(|v|json!(v)).unwrap_or_else(||scheme.get("mode").cloned().unwrap_or(json!("dark"))),"variant":scheme.get("variant").cloned().unwrap_or(json!("tonalspot")),"orient":config.get("orient").cloned().unwrap_or(json!({})),"harmony":harmony.map(|v|json!(v)).unwrap_or_else(||settings.get("paletteHarmony").cloned().unwrap_or(json!(false))),"personality":personality.filter(|v|!v.is_empty()).map(|v|json!(v)).unwrap_or_else(||if let Some(harmony)=harmony {json!(if harmony{"harmony"}else{"natural"})}else{settings.get("palettePersonality").cloned().unwrap_or(json!("natural"))}),"backgroundFromWallpaper":settings.get("paletteBackgroundFromWallpaper").cloned().unwrap_or(json!(false)),"clockHour":if json::truthy(&choice["tideAutomatic"]){json!(self.hour.unwrap_or_else(||chrono::Local::now().hour()))}else{Value::Null},"choices":choice});
        let text = format!(
            "{}{}{}{}{}{}{}{}palette-warm-orient-v3",
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
