//! Private previews, choices and history; Orient computations stay in-process.
use crate::{
    classic_state::Publisher,
    i18n::tr,
    image_text::FontBook,
    json, orient,
    paths::Paths,
    state,
    wallpaper_media::{self, Media},
    Result,
};
use chrono::Timelike;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fmt, fs,
    os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};
#[derive(Debug)]
pub struct ColorError {
    pub kind: &'static str,
    pub message: String,
}
impl fmt::Display for ColorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.message.fmt(f)
    }
}
impl std::error::Error for ColorError {}

fn key<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    value.get(name).ok_or_else(|| {
        ColorError {
            kind: "KeyError",
            message: json::python_repr(&json!(name)),
        }
        .into()
    })
}
pub fn signature(path: &Path) -> Result<Value> {
    let path = path.canonicalize()?;
    let meta = fs::metadata(&path)?;
    Ok(json!([
        path,
        meta.dev(),
        meta.ino(),
        meta.len(),
        i128::from(meta.mtime()) * 1_000_000_000 + i128::from(meta.mtime_nsec()),
        i128::from(meta.ctime()) * 1_000_000_000 + i128::from(meta.ctime_nsec())
    ]))
}
pub fn preview_key(
    engine: &str,
    stamp: &Value,
    request: &Value,
    favorite: &Value,
) -> Result<String> {
    Ok(format!(
        "{:x}",
        Sha256::digest(json::dumps(&json!([engine, stamp, request, favorite]), true)?.as_bytes())
    ))
}
pub fn palette_args(request: &Value) -> Value {
    let mut values = Map::new();
    for name in [
        "mode",
        "personality",
        "overrides",
        "brightness",
        "saturation",
        "hour",
    ] {
        values.insert(name.into(), request[name].clone());
    }
    values.insert("accent".into(), request["accent"].clone());
    values.insert(
        "background_from_wallpaper".into(),
        request["backgroundFromWallpaper"].clone(),
    );
    values.insert("time_tint".into(), request["timeTint"].clone());
    Value::Object(values)
}
pub struct Colors {
    pub paths: Paths,
    pub assets: PathBuf,
    pub media: Media,
    pub publisher: Publisher,
    pub now_ms: Option<i64>,
    pub hour: Option<u32>,
    pub export_date: Option<String>,
    pub fonts: FontBook,
}
impl Colors {
    pub fn environment() -> Result<Self> {
        let publisher = Publisher::environment()?;
        let paths = publisher.paths.clone();
        let assets = publisher.assets.clone();
        let media = Media::new(paths.clone(), assets.clone())?;
        Ok(Self {
            paths,
            assets,
            media,
            publisher,
            now_ms: None,
            hour: None,
            export_date: None,
            fonts: FontBook::default(),
        })
    }
    pub fn cache(&self) -> PathBuf {
        self.paths.cache.join("colors")
    }
    pub fn store_path(&self) -> PathBuf {
        self.paths.config.join("colors.json")
    }
    pub fn exports(&self) -> PathBuf {
        self.paths.home.join("Documents/Nacre Colors")
    }
    fn now(&self) -> i64 {
        self.now_ms
            .unwrap_or_else(|| chrono::Utc::now().timestamp_millis())
    }
    pub fn stored(&self) -> Result<Value> {
        Self::stored_values(crate::owned_settings::read(
            &self.store_path(),
            "colors",
            json!({}),
        )?)
    }
    fn stored_values(saved: Value) -> Result<Value> {
        let mut values = json!({"version":1,"wallpapers":{},"favorites":[],"history":[]});
        for (key, value) in saved.as_object().ok_or("Invalid saved color choices")? {
            values[key] = value.clone();
        }
        Ok(values)
    }
    fn store_lock(&self) -> Result<nix::fcntl::Flock<fs::File>> {
        let path = self.store_path().with_extension("lock");
        fs::create_dir_all(path.parent().ok_or("Missing color store directory")?)?;
        let file = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .mode(0o600)
            .open(&path)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        Ok(
            nix::fcntl::Flock::lock(file, nix::fcntl::FlockArg::LockExclusive)
                .map_err(|(_, e)| e)?,
        )
    }
    pub fn validate_request(&self, request: &Value) -> Result<Value> {
        let Some(request) = request.as_object() else {
            return Err(ColorError {
                kind: "ValueError",
                message: tr(
                    "helper.colors.invalid-color-request",
                    "Invalid color request",
                ),
            }
            .into());
        };
        let mut result = json!({"mode":"dark","personality":"natural","overrides":{},"brightness":0.0,"saturation":1.0,"hour":self.hour.unwrap_or_else(||chrono::Local::now().hour()),"autoMode":false,"tideAutomatic":false,"timeTint":false,"workspaceColors":false});
        for (key, value) in request {
            result[key] = value.clone();
        }
        let (personality, background) = orient_core::palette::background_policy(
            result["personality"]
                .as_str()
                .ok_or("Expected personality string")?,
            &result["backgroundFromWallpaper"],
        )?;
        result["personality"] = json!(personality);
        result["backgroundFromWallpaper"] = json!(background);
        if !matches!(result["mode"].as_str(), Some("dark" | "light"))
            || !orient_core::palette::PERSONALITIES
                .contains(&result["personality"].as_str().unwrap_or(""))
        {
            return Err(ColorError {
                kind: "ValueError",
                message: tr(
                    "helper.colors.choose-a-supported-color-personality-and-mode",
                    "Choose a supported color personality and mode",
                ),
            }
            .into());
        }
        let Some(image) = result["image"]
            .as_str()
            .filter(|value| Path::new(value).is_file())
        else {
            return Err(ColorError {
                kind: "ValueError",
                message: tr(
                    "helper.colors.choose-a-local-wallpaper",
                    "Choose a local wallpaper",
                ),
            }
            .into());
        };
        let image = PathBuf::from(image);
        if !result["hour"]
            .as_i64()
            .is_some_and(|hour| (0..=23).contains(&hour))
        {
            return Err(ColorError {
                kind: "ValueError",
                message: tr(
                    "helper.colors.choose-an-hour-from-zero-to-23",
                    "Choose an hour from zero to 23",
                ),
            }
            .into());
        }
        for name in ["brightness", "saturation"] {
            if !orient_core::numeric(&result[name]).is_some_and(f64::is_finite) {
                return Err(ColorError {
                    kind: "ValueError",
                    message: tr(
                        "helper.colors.invalid-color-adjustment",
                        "Invalid color adjustment",
                    ),
                }
                .into());
            }
        }
        for name in ["autoMode", "tideAutomatic", "workspaceColors", "timeTint"] {
            if !result[name].is_boolean() {
                return Err(ColorError {
                    kind: "ValueError",
                    message: tr(
                        "helper.colors.expected-a-boolean-option",
                        "Expected a boolean option",
                    ),
                }
                .into());
            }
        }
        if result["tideAutomatic"] == true {
            result["hour"] = json!(self.hour.unwrap_or_else(|| chrono::Local::now().hour()));
        }
        if (result["personality"] == "tide" || result["timeTint"] == true)
            && result["autoMode"] == true
        {
            result["mode"] = json!(if (6..19).contains(&result["hour"].as_i64().unwrap()) {
                "light"
            } else {
                "dark"
            });
        }
        result["image"] = json!(image.canonicalize()?);
        Ok(result)
    }
    pub fn stage(&self, request: &Value) -> Result<Value> {
        let mut request = self.validate_request(request)?;
        if json::truthy(&request["pick"]) {
            let point = request
                .as_object_mut()
                .ok_or("Invalid preview request")?
                .shift_remove("pick")
                .ok_or("Missing picked point")?;
            let item = self
                .media
                .describe(Path::new(request["image"].as_str().unwrap()))?;
            let frame = orient_decoder::still(Path::new(
                item["poster"].as_str().ok_or("Missing wallpaper poster")?,
            ))?;
            request["accent"] = json!(orient_core::extract::sample_color(
                &frame,
                orient_core::numeric(key(&point, "x")?)
                    .ok_or("Pick coordinates between zero and one")?,
                orient_core::numeric(key(&point, "y")?)
                    .ok_or("Pick coordinates between zero and one")?
            )?);
        }
        let path = Path::new(request["image"].as_str().ok_or("Missing preview image")?);
        let item = self.media.describe(path)?;
        let stamp = signature(path)?;
        let saved = self.stored()?;
        let favorite = saved["favorites"]
            .as_array()
            .ok_or("Invalid saved palettes")?
            .iter()
            .find(|favorite| favorite["id"] == request["favoriteId"])
            .cloned()
            .unwrap_or(Value::Null);
        if json::truthy(&request["favoriteId"]) && favorite.is_null() {
            return Err(ColorError {
                kind: "ValueError",
                message: tr(
                    "helper.colors.that-saved-palette-no-longer-exists",
                    "That saved palette no longer exists",
                ),
            }
            .into());
        }
        let identity = preview_key(orient::engine_id(), &stamp, &request, &favorite)?;
        let cache = self.cache().join(format!("{identity}.json"));
        let cached = match state::read(&cache, json!({})) {
            Ok(value) => value,
            Err(error) => {
                eprintln!("Color preview cache read: {error}");
                json!({})
            }
        };
        if cached.is_object()
            && cached["engine"] == orient::engine_id()
            && cached["signature"] == stamp
            && cached["request"] == request
            && cached.get("comparisons").is_some()
            && cached["palette"]["colours"]
                .as_object()
                .is_some_and(|values| orient_core::palette::validate(values).is_ok())
        {
            if let Ok(result) = preview_result(&identity, &cached) {
                return Ok(result);
            }
        }
        let (data, comparisons) = if !favorite.is_null() {
            let mode = request["mode"].as_str().ok_or("Missing preview mode")?;
            let mut data = favorite["palette"].clone();
            data["mode"] = request["mode"].clone();
            data["colours"] = favorite["modes"][mode].clone();
            let mut input = data.get("input").cloned().unwrap_or(json!({}));
            for (key, from) in [
                ("mode", "mode"),
                ("personality", "personality"),
                ("background_from_wallpaper", "backgroundFromWallpaper"),
                ("time_tint", "timeTint"),
            ] {
                input[key] = request[from].clone();
            }
            data["input"] = input;
            let colors = data["colours"]
                .as_object()
                .ok_or("Invalid saved palette colors")?;
            orient_core::palette::validate(colors)?;
            data["accessibility"] = orient_core::accessibility::audit(colors)?;
            data["source"]["options"] = json!([]);
            (data, Vec::new())
        } else {
            let data = orient::from_image(path, &palette_args(&request))?;
            let mut comparisons = Vec::new();
            if json::truthy(&request["compare"]) {
                for personality in orient_core::palette::PERSONALITIES {
                    let comparison = if request["personality"] == personality {
                        data.clone()
                    } else {
                        let mut args = palette_args(&request);
                        args["personality"] = json!(personality);
                        orient::from_image(path, &args)?
                    };
                    comparisons.push(json!({"personality":personality,"mode":comparison["mode"],"colours":comparison["colours"],"minimumContrast":comparison["accessibility"]["minimumTextContrast"]}));
                }
            }
            (data, comparisons)
        };
        let swatches = image_colors(&data, 8)?;
        let record = json!({"version":1,"engine":orient::engine_id(),"signature":stamp,"request":request,"palette":data,"poster":item["poster"],"media":item,"comparisons":comparisons,"imageColors":swatches,"favorite":!favorite.is_null()});
        if signature(path)? != stamp {
            return Err(ColorError {
                kind: "ValueError",
                message: tr(
                    "helper.colors.wallpaper-changed-while-previewing-try-again",
                    "Wallpaper changed while previewing; try again",
                ),
            }
            .into());
        }
        wallpaper_media::atomic(&cache, &record)?;
        let mut entries = fs::read_dir(self.cache())?
            .map(|entry| -> Result<_> {
                let path = entry?.path();
                let metadata = fs::metadata(&path)?;
                Ok((metadata.modified()?, path))
            })
            .collect::<Result<Vec<_>>>()?;
        entries.retain(|(_, path)| path.extension().is_some_and(|s| s == "json"));
        entries.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
        for (_, path) in entries.into_iter().skip(64) {
            fs::remove_file(path)?;
        }
        preview_result(&identity, &record)
    }
    pub fn record(&self, identity: &Value) -> Result<Value> {
        let Some(identity) = identity.as_str().filter(|s| {
            s.len() == 64
                && s.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }) else {
            return Err(ColorError {
                kind: "ValueError",
                message: tr("helper.colors.preview-is-not-valid", "Preview is not valid"),
            }
            .into());
        };
        let data = state::read(&self.cache().join(format!("{identity}.json")), json!({}))?;
        if data["version"] != 1 || data["engine"] != orient::engine_id() {
            return Err(ColorError {
                kind: "ValueError",
                message: tr(
                    "helper.colors.preview-expired-preview-it-again",
                    "Preview expired; preview it again",
                ),
            }
            .into());
        }
        if signature(Path::new(
            data["request"]["image"]
                .as_str()
                .ok_or("Invalid preview image")?,
        ))? != data["signature"]
        {
            return Err(ColorError {
                kind: "ValueError",
                message: tr(
                    "helper.colors.wallpaper-changed-preview-it-again",
                    "Wallpaper changed; preview it again",
                ),
            }
            .into());
        }
        if data["palette"]["formatVersion"] != orient_core::FORMAT_VERSION {
            return Err(ColorError {
                kind: "ValueError",
                message: tr(
                    "helper.colors.unsupported-palette-format",
                    "Unsupported palette format",
                ),
            }
            .into());
        }
        orient_core::palette::validate(
            data["palette"]["colours"]
                .as_object()
                .ok_or("Invalid palette colors")?,
        )?;
        Ok(data)
    }
    pub fn favorite(&self, identity: &Value, name: &Value) -> Result<Value> {
        let data = self.record(identity)?;
        let mode = data["palette"]["mode"]
            .as_str()
            .ok_or("Missing palette mode")?;
        let other = if mode == "dark" { "light" } else { "dark" };
        let mut modes = Map::new();
        modes.insert(mode.into(), data["palette"]["colours"].clone());
        let stored = self.stored()?;
        let existing = stored["favorites"]
            .as_array()
            .ok_or("Invalid saved palettes")?
            .iter()
            .find(|value| value["id"] == data["request"]["favoriteId"]);
        let colors = if let Some(existing) = existing {
            existing["modes"][other].clone()
        } else {
            let mut args = palette_args(&data["request"]);
            args["mode"] = json!(other);
            orient::from_image(Path::new(data["request"]["image"].as_str().unwrap()), &args)?
                ["colours"]
                .clone()
        };
        modes.insert(other.into(), colors);
        let identity = format!(
            "{:x}",
            Sha256::digest(json::dumps(&modes, true)?.as_bytes())
        );
        let identity = &identity[..16];
        let name = json::python_str(name)
            .trim()
            .chars()
            .take(64)
            .collect::<String>();
        let name = if name.is_empty() {
            format!(
                "{} {}",
                title(data["request"]["personality"].as_str().unwrap_or("")),
                data["palette"]["source"]["selected"]
                    .as_str()
                    .ok_or("Missing palette selection")?
            )
        } else {
            name
        };
        let value = json!({"id":identity,"name":name,"modes":modes,"palette":data["palette"],"swatches":[data["palette"]["colours"]["primary"],data["palette"]["colours"]["secondary"],data["palette"]["colours"]["tertiary"]]});
        let _guard = self.store_lock()?;
        let mut document =
            crate::owned_settings::Document::open(&self.store_path(), "colors", json!({}))?;
        let mut stored = Self::stored_values(document.data.clone())?;
        let mut favorites = vec![value];
        favorites.extend(
            stored["favorites"]
                .as_array()
                .ok_or("Invalid saved palettes")?
                .iter()
                .filter(|favorite| favorite["id"] != identity)
                .cloned(),
        );
        favorites.truncate(64);
        stored["favorites"] = json!(favorites);
        if !document.save(&stored)? {
            wallpaper_media::atomic(&self.store_path(), &stored)?;
        }
        Ok(json!({"saved":name}))
    }
    pub fn delete(&self, identity: &Value) -> Result<Value> {
        let _guard = self.store_lock()?;
        let mut document =
            crate::owned_settings::Document::open(&self.store_path(), "colors", json!({}))?;
        let mut values = Self::stored_values(document.data.clone())?;
        values["favorites"] = json!(values["favorites"]
            .as_array()
            .ok_or("Invalid saved palettes")?
            .iter()
            .filter(|favorite| favorite["id"] != *identity)
            .cloned()
            .collect::<Vec<_>>());
        if !document.save(&values)? {
            wallpaper_media::atomic(&self.store_path(), &values)?;
        }
        Ok(json!({"deleted":true}))
    }
}
fn title(value: &str) -> String {
    let mut output = String::new();
    let mut first = true;
    for c in value.chars() {
        if c.is_alphabetic() {
            if first {
                output.extend(c.to_uppercase());
            } else {
                output.extend(c.to_lowercase());
            }
            first = false;
        } else {
            output.push(c);
            first = true;
        }
    }
    output
}
pub fn image_colors(data: &Value, limit: usize) -> Result<Vec<Value>> {
    let source = &data["source"];
    let families = source["families"].as_array().cloned().unwrap_or_default();
    let mut sorted = families.clone();
    sorted.sort_by(|a, b| {
        b["score"]
            .as_f64()
            .unwrap_or(0.0)
            .total_cmp(&a["score"].as_f64().unwrap_or(0.0))
    });
    let mut chosen = Vec::new();
    for family in sorted {
        if family["coverage"]
            .as_f64()
            .ok_or("Missing pigment coverage")?
            < 0.003
        {
            continue;
        }
        let hue =
            orient_core::colour::lch(family["hex"].as_str().ok_or("Missing pigment color")?)?[2];
        let distinct = chosen
            .iter()
            .map(|item: &Value| {
                orient_core::colour::lch(item["hex"].as_str().unwrap())
                    .map(|lch| ((hue - lch[2] + 180.0).rem_euclid(360.0) - 180.0).abs() >= 35.0)
            })
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .all(|v| v);
        if distinct {
            chosen.push(family);
        }
        if chosen.len() == 5 {
            break;
        }
    }
    let members: std::collections::BTreeSet<_> = families
        .iter()
        .flat_map(|family| family["members"].as_array().into_iter().flatten())
        .filter_map(Value::as_str)
        .collect();
    let mut leftovers = source["clusters"].as_array().cloned().unwrap_or_default();
    leftovers.retain(|item| item["hex"].as_str().is_some_and(|s| !members.contains(s)));
    leftovers.sort_by(|a, b| {
        b["coverage"]
            .as_f64()
            .unwrap_or(0.0)
            .total_cmp(&a["coverage"].as_f64().unwrap_or(0.0))
    });
    let mut neutrals = Vec::new();
    for item in leftovers {
        let light =
            orient_core::colour::lch(item["hex"].as_str().ok_or("Missing cluster color")?)?[0];
        let distinct = neutrals
            .iter()
            .map(|other: &Value| {
                orient_core::colour::lch(other["hex"].as_str().unwrap())
                    .map(|lch| (light - lch[0]).abs() >= 0.10)
            })
            .collect::<std::result::Result<Vec<_>, _>>()?
            .into_iter()
            .all(|v| v);
        if distinct {
            neutrals.push(item);
        }
        if neutrals.len() == limit.saturating_sub(chosen.len()) {
            break;
        }
    }
    chosen.extend(neutrals);
    chosen.sort_by(|a, b| {
        b["coverage"]
            .as_f64()
            .unwrap_or(0.0)
            .total_cmp(&a["coverage"].as_f64().unwrap_or(0.0))
    });
    chosen.truncate(limit);
    Ok(chosen)
}
pub fn preview_result(identity: &str, record: &Value) -> Result<Value> {
    for name in [
        "request",
        "palette",
        "comparisons",
        "imageColors",
        "favorite",
    ] {
        key(record, name)?;
    }
    let media = key(record, "media")?;
    for name in ["preview", "path", "name"] {
        key(media, name)?;
    }
    let data = &record["palette"];
    let item = &record["media"];
    let swatches = record["imageColors"]
        .as_array()
        .ok_or("Invalid image colors")?;
    let remaining = if swatches.is_empty() {
        json!(1)
    } else {
        let value = 1.0
            - swatches
                .iter()
                .map(|item| {
                    item["coverage"]
                        .as_f64()
                        .ok_or("Invalid image color coverage")
                })
                .collect::<std::result::Result<Vec<_>, _>>()?
                .into_iter()
                .sum::<f64>();
        if value <= 0.0 {
            json!(0)
        } else {
            json!(value)
        }
    };
    Ok(
        json!({"id":identity,"request":record["request"],"palette":data,"comparisons":record["comparisons"],"imageColors":swatches,"otherCoverage":remaining,"thumbnail":item["preview"],"image":item["path"],"name":item["name"],"favorite":record["favorite"],"frames":data["source"]["sample"].get("frames").cloned().unwrap_or(json!(1))}),
    )
}

impl Colors {
    pub fn catalog(&self) -> Result<Value> {
        let mut data = self.media.catalog()?;
        let values = self.stored()?;
        let scheme = state::read(&self.paths.state.join("scheme.json"), json!({}))?;
        for item in data["entries"]
            .as_array_mut()
            .ok_or("Invalid wallpaper catalog")?
        {
            let marker = self.media.palette_marker(
                Path::new(item["poster"].as_str().ok_or("Missing catalog poster")?),
                scheme["flavour"].as_str().unwrap_or("default"),
                None,
                item["path"].as_str().map(Path::new),
                None,
                None,
            )?;
            let prepared = state::read(&marker, json!({}))?;
            let options = prepared["source"]["options"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let families: std::collections::BTreeSet<_> = options
                .iter()
                .map(|option| {
                    option["name"]
                        .as_str()
                        .ok_or("Invalid prepared color name")
                        .map(|name| name.split(' ').next().unwrap_or("").to_owned())
                })
                .collect::<std::result::Result<_, _>>()?;
            item["families"] = json!(families);
            item["colors"] = json!(options);
        }
        let favorites=values["favorites"].as_array().ok_or("Invalid saved palettes")?.iter().map(|favorite|json!({"id":favorite["id"],"name":favorite["name"],"swatches":favorite["swatches"]})).collect::<Vec<_>>();
        Ok(
            json!({"current":data["media"].get("path").cloned().unwrap_or(json!("")),"entries":data["entries"],"favorites":favorites,"history":values["history"],"choices":values["wallpapers"],"exports":self.exports()}),
        )
    }
    pub fn apply(&mut self, identity: &Value, live: bool) -> Result<Value> {
        let data = self.record(identity)?;
        let request = &data["request"];
        let palette = &data["palette"];
        let image = request["image"].as_str().ok_or("Invalid preview image")?;
        let mut item = data["media"].clone();
        let thumbnail = self
            .paths
            .cache
            .join("wallpapers")
            .join(
                palette["source"]["digest"]
                    .as_str()
                    .ok_or("Missing palette source digest")?,
            )
            .join("thumbnail.jpg");
        if !thumbnail.is_file() {
            fs::create_dir_all(thumbnail.parent().ok_or("Missing thumbnail directory")?)?;
            let image = image::DynamicImage::ImageRgba8(orient_decoder::load_rgba(Path::new(
                item["preview"]
                    .as_str()
                    .ok_or("Missing wallpaper preview")?,
            ))?)
            .to_rgb8();
            fs::write(&thumbnail, crate::image_paint::encode_jpeg(&image, 85)?)?;
        }
        let guard = crate::classic_state::publication_lock(&self.paths)?;
        let _store = self.store_lock()?;
        let mut document =
            crate::owned_settings::Document::open(&self.store_path(), "colors", json!({}))?;
        let mut values = Self::stored_values(document.data.clone())?;
        let previous_values = values.clone();
        let previous_preferences = self.media.settings()?;
        let mut choice = Map::new();
        for key in [
            "personality",
            "backgroundFromWallpaper",
            "overrides",
            "brightness",
            "saturation",
            "hour",
            "autoMode",
            "tideAutomatic",
            "timeTint",
            "workspaceColors",
        ] {
            choice.insert(key.into(), request[key].clone());
        }
        if json::truthy(&request["accent"]) {
            choice.insert("accent".into(), request["accent"].clone());
        }
        choice.insert("mode".into(), palette["mode"].clone());
        if json::truthy(&request["favoriteId"]) {
            choice.insert("favoriteId".into(), request["favoriteId"].clone());
        }
        choice.insert("poster".into(), item["poster"].clone());
        let poster = PathBuf::from(item["poster"].as_str().ok_or("Missing wallpaper poster")?);
        values["wallpapers"][image] = json!(choice);
        values["wallpapers"][crate::paths::weak_canonical(&poster)?
            .to_string_lossy()
            .as_ref()] = json!(choice);
        if !document.save(&values)? {
            wallpaper_media::atomic(&self.store_path(), &values)?;
        }
        let patch = json!({"palettePreset":if json::truthy(&request["favoriteId"]){format!("favorite:{}",request["favoriteId"].as_str().ok_or("Invalid saved palette identity")?)}else{"wallpaper".into()},"paletteMode":palette["mode"],"paletteHarmony":request["personality"]=="harmony","palettePersonality":request["personality"],"paletteBackgroundFromWallpaper":request["backgroundFromWallpaper"]});
        let applied = (|| -> Result<()> {
            self.media.preference(&patch)?;
            if !self
                .publisher
                .commit_prepared_held(&poster, palette, &thumbnail, live, true, &guard)?
            {
                return Err(ColorError { kind: "ValueError", message: tr("helper.colors.the-desktop-cannot-apply-that-prepared-palette-choose-a-wallpaper", "The desktop cannot apply that prepared palette; choose a wallpaper palette first") }.into());
            }
            item["appliedAtMs"] = json!(self.now());
            wallpaper_media::atomic(&self.media.wallpaper_state().join("media.json"), &item)
        })();
        if let Err(error) = applied {
            if !document.save(&previous_values)? {
                wallpaper_media::atomic(&self.store_path(), &previous_values)?;
            }
            let previous: Map<_, _> = patch
                .as_object()
                .unwrap()
                .keys()
                .map(|key| (key.clone(), previous_preferences[key].clone()))
                .collect();
            self.media.preference(&Value::Object(previous))?;
            return Err(error);
        }
        crate::classic_state::post_publish_external_targets(&self.publisher.paths, live);
        let identity = identity.as_str().ok_or("Invalid preview identity")?;
        let history = json!({"id":&identity[..16],"name":item["name"],"time":self.now()/1000,"request":request});
        values["lastApplied"] = json!(identity);
        let mut rows = vec![history];
        rows.extend(
            values["history"]
                .as_array()
                .ok_or("Invalid color history")?
                .iter()
                .filter(|row| row["request"] != *request)
                .cloned(),
        );
        rows.truncate(20);
        values["history"] = json!(rows);
        if !document.save(&values)? {
            wallpaper_media::atomic(&self.store_path(), &values)?;
        }
        Ok(json!({"applied":true,"name":item["name"]}))
    }
    pub fn output_folder(&self, identity: &Value) -> Result<PathBuf> {
        self.record(identity)?;
        let identity = identity.as_str().ok_or("Invalid preview identity")?;
        let date = self
            .export_date
            .clone()
            .unwrap_or_else(|| chrono::Local::now().format("%Y%m%d-%H%M%S").to_string());
        let mut folder = self.exports().join(format!("{date}-{}", &identity[..8]));
        let mut index = 1;
        while folder.exists() {
            folder = folder.with_file_name(format!(
                "{}-{index}",
                folder
                    .file_name()
                    .ok_or("Invalid export directory")?
                    .to_string_lossy()
            ));
            index += 1;
        }
        fs::create_dir_all(&folder)?;
        Ok(folder)
    }
    pub fn export(&self, identity: &Value) -> Result<Value> {
        let data = self.record(identity)?;
        let folder = self.output_folder(identity)?;
        let values = self.stored()?;
        let favorite = values["favorites"]
            .as_array()
            .ok_or("Invalid saved palettes")?
            .iter()
            .find(|favorite| favorite["id"] == data["request"]["favoriteId"]);
        let schemes = favorite
            .and_then(|favorite| favorite["modes"].as_object())
            .cloned()
            .unwrap_or_default();
        let mut paths =
            crate::template_export::export(&data["palette"], &folder, None, &schemes, None)?;
        wallpaper_media::atomic(&folder.join("palette.json"), &data["palette"])?;
        fs::write(folder.join("README.txt"),"Generated by Nacre Colors / Orient. These are exports, not installed themes.\nVS Code: import the color-theme JSON with your extension/theme setup.\nFirefox: load the theme manifest as a temporary add-on for testing; distribution requires Mozilla signing.\nObsidian: enable the CSS snippet. Neovim: load the Lua file. btop: put the theme in its themes directory.\nSpotify requires optional Spicetify; Discord requires an optional user-theme client. Neither is installed or modified here.\nKitty/Foot/Alacritty/GTK/rofi/Hyprland: load explicitly from your own configuration.\nAdd custom .in templates to ~/.config/nacre/colors-templates, using {{role}} or Matugen {{ colors.primary.default.hex }} tokens. Filters/blocks are not supported.\n")?;
        let custom = self.paths.config.join("colors-templates");
        if custom.is_dir() {
            paths.extend(crate::template_export::export(
                &data["palette"],
                &folder.join("custom"),
                Some(&custom),
                &schemes,
                None,
            )?);
        }
        Ok(json!({"directory":folder,"files":paths}))
    }
    pub fn card(&mut self, identity: &Value) -> Result<Value> {
        let data = self.record(identity)?;
        let folder = self.output_folder(identity)?;
        let colors = data["palette"]["colours"]
            .as_object()
            .ok_or("Invalid card palette")?;
        let color = |role: &str| {
            crate::lock_dashboard::rgb(colors[role].as_str().ok_or("Missing card color")?)
        };
        let surface = color("surface")?;
        let mut output = image::RgbaImage::from_pixel(
            1200,
            850,
            image::Rgba([surface[0], surface[1], surface[2], 255]),
        );
        let original = orient_decoder::load_rgba(Path::new(
            data["media"]["preview"]
                .as_str()
                .ok_or("Missing card preview")?,
        ))?;
        let photo =
            crate::image_paint::fit(&original, (1200, 540), orient_decoder::Filter::Bicubic)?;
        crate::image_paint::paste(
            &mut output,
            &photo,
            (0, 0),
            &image::GrayImage::from_pixel(1200, 540, image::Luma([255])),
        );
        let foreground = color("onSurface")?;
        for (index, role) in ["primary", "secondary", "tertiary", "surface", "onSurface"]
            .iter()
            .enumerate()
        {
            let x = 30 + index as i32 * 235;
            let fill = color(role)?;
            let mask = crate::image_paint::rounded_mask((206, 81), 15)?;
            let patch = image::RgbaImage::from_pixel(
                206,
                81,
                image::Rgba([fill[0], fill[1], fill[2], 255]),
            );
            crate::image_paint::paste(&mut output, &patch, (x, 570), &mask);
            let font = self.fonts.get("DejaVu Sans", "Book", 27)?;
            for (y, value) in [
                (675, role.to_string()),
                (716, format!("#{}", colors[*role].as_str().unwrap())),
            ] {
                crate::image_paint::text(&mut output, &font.mask(&value)?, (x, y), foreground);
            }
        }
        let footer = format!(
            "Made with Nacre · {} · Orient",
            title(data["request"]["personality"].as_str().unwrap_or(""))
        );
        let mask = self.fonts.get("DejaVu Sans", "Book", 27)?.mask(&footer)?;
        crate::image_paint::text(&mut output, &mask, (30, 786), foreground);
        let target = folder.join("palette-card.png");
        fs::write(
            &target,
            orient_decoder::encode_rgb_png(&image::DynamicImage::ImageRgba8(output).to_rgb8(), 6)?,
        )?;
        Ok(json!({"directory":folder,"card":target}))
    }
}

impl Colors {
    pub fn preview_response(&self, line: &str) -> Value {
        let mut sequence = json!(-1);
        let response = (|| -> Result<Value> {
            if line.chars().count() > 65536 {
                return Err(ColorError {
                    kind: "ValueError",
                    message: tr(
                        "helper.colors.preview-request-is-too-large",
                        "Preview request is too large",
                    ),
                }
                .into());
            }
            let request = json::parse_legacy_value(line)?;
            sequence = match &request {
                Value::Object(_) => key(&request, "sequence")?.clone(),
                Value::Null => return Err("'NoneType' object is not subscriptable".into()),
                Value::Array(_) => {
                    return Err("list indices must be integers or slices, not str".into())
                }
                Value::String(_) => return Err("string indices must be integers, not 'str'".into()),
                Value::Bool(_) => return Err("'bool' object is not subscriptable".into()),
                Value::Number(number) => {
                    return Err(format!(
                        "'{}' object is not subscriptable",
                        if number.is_f64() { "float" } else { "int" }
                    )
                    .into())
                }
            };
            if !sequence.as_i64().is_some_and(|value| value >= 0) && !sequence.as_u64().is_some() {
                return Err(ColorError {
                    kind: "ValueError",
                    message: tr(
                        "helper.colors.invalid-preview-sequence",
                        "Invalid preview sequence",
                    ),
                }
                .into());
            }
            let result = self.stage(key(&request, "request")?)?;
            Ok(json!({"sequence":sequence,"result":result}))
        })();
        response.unwrap_or_else(|error|json!({"sequence":sequence,"error":error.to_string().chars().take(240).collect::<String>()}))
    }
    pub fn serve(&self) -> Result<i32> {
        use rustix::event::{poll, PollFd, PollFlags};
        use std::io::{Read, Write};
        let signals = crate::signals::Signals::new()?;
        let stdin = std::io::stdin();
        let mut input = stdin.lock();
        let stdout = std::io::stdout();
        let mut output = stdout.lock();
        writeln!(output, "{}", json::dumps(&json!({"ready":true}), false)?)?;
        output.flush()?;
        let mut pending = Vec::new();
        let mut eof = false;
        loop {
            let valid = match std::str::from_utf8(&pending) {
                Ok(text) => text,
                Err(error) if error.error_len().is_none() && !eof => {
                    std::str::from_utf8(&pending[..error.valid_up_to()])?
                }
                Err(error) => return Err(error.into()),
            };
            let end = valid
                .char_indices()
                .enumerate()
                .find_map(|(index, (offset, character))| {
                    if character == '\n' || index == 65536 {
                        Some(offset + character.len_utf8())
                    } else {
                        None
                    }
                })
                .or({
                    if eof && !valid.is_empty() {
                        Some(valid.len())
                    } else {
                        None
                    }
                });
            if let Some(end) = end {
                let line = std::str::from_utf8(&pending[..end])?.to_owned();
                pending.drain(..end);
                let response = self.preview_response(&line);
                if let Some(signal) = signals.fd.read_signal()? {
                    return Ok(
                        if signal.ssi_signo == nix::sys::signal::Signal::SIGTERM as u32 {
                            143
                        } else {
                            130
                        },
                    );
                }
                writeln!(output, "{}", json::dumps(&response, false)?)?;
                output.flush()?;
                if line.chars().count() > 65536 {
                    return Ok(1);
                }
                continue;
            }
            if eof {
                return Ok(0);
            }
            let mut fds = [
                PollFd::new(&stdin, PollFlags::IN),
                PollFd::new(&signals.fd, PollFlags::IN),
            ];
            poll(&mut fds, None)?;
            if fds[1].revents().contains(PollFlags::IN) {
                if let Some(signal) = signals.fd.read_signal()? {
                    return Ok(
                        if signal.ssi_signo == nix::sys::signal::Signal::SIGTERM as u32 {
                            143
                        } else {
                            130
                        },
                    );
                }
            }
            if fds[0].revents().intersects(PollFlags::IN | PollFlags::HUP) {
                let mut bytes = [0u8; 8192];
                let count = input.read(&mut bytes)?;
                if count == 0 {
                    eof = true;
                } else {
                    pending.extend_from_slice(&bytes[..count]);
                }
            }
        }
    }
    pub fn run_action(&mut self, action: &str, request: &Value) -> Result<Value> {
        match action {
            "catalog" => self.catalog(),
            "preview" => self.stage(request),
            "apply" => self.apply(key(request, "id")?, true),
            "favorite" => self.favorite(
                key(request, "id")?,
                request.get("name").unwrap_or(&json!("")),
            ),
            "export" => self.export(key(request, "id")?),
            "card" => self.card(key(request, "id")?),
            "delete" => self.delete(key(request, "id")?),
            _ => Err("Unknown Colors action".into()),
        }
    }
}
pub fn run(args: &[String]) -> Result<i32> {
    use std::io::{BufRead, Write};
    let Some(action) = args.first().map(String::as_str).filter(|action| {
        [
            "catalog", "preview", "serve", "apply", "favorite", "export", "card", "delete",
        ]
        .contains(action)
    }) else {
        eprintln!("Expected Colors action: catalog, preview, serve, apply, favorite, export, card or delete");
        return Ok(2);
    };
    if args.len() != 1 {
        eprintln!("Unexpected Colors arguments");
        return Ok(2);
    }
    let mut app = Colors::environment()?;
    if action == "serve" {
        return app.serve();
    }
    let result = (|| -> Result<Value> {
        let request = if action == "catalog" {
            json!({})
        } else {
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            json::parse_legacy_value(&line)?
        };
        app.run_action(action, &request)
    })();
    match result {
        Ok(value) => {
            writeln!(std::io::stdout(), "{}", json::dumps(&value, false)?)?;
            Ok(0)
        }
        Err(error) => {
            eprintln!("{error}");
            Ok(1)
        }
    }
}
