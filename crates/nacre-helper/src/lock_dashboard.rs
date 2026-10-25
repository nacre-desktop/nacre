//! Existing prepared lock textures, independent of authentication and QML.
use crate::{image_paint as paint, image_text::FontBook, json, paths::Paths, process, Result};
use chrono::{DateTime, FixedOffset, Local, NaiveDateTime, Timelike};
use image::{Rgba, RgbaImage};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
pub const WIDTH: u32 = 1440;
pub const HEIGHT: u32 = 810;
pub const RESOLUTION: u32 = 2;
pub struct Context {
    pub fonts: FontBook,
    pub now: DateTime<FixedOffset>,
    pub system: Option<Value>,
}
impl Default for Context {
    fn default() -> Self {
        Self {
            fonts: FontBook::default(),
            now: Local::now().fixed_offset(),
            system: None,
        }
    }
}
pub fn layout(monitor: &Value) -> f64 {
    let mut width = monitor["width"].as_f64().unwrap_or(1920.0);
    let mut height = monitor["height"].as_f64().unwrap_or(1080.0);
    if monitor["transform"].as_i64().unwrap_or(0) % 2 != 0 {
        std::mem::swap(&mut width, &mut height);
    }
    (width / 2048.0).min(height / 1152.0)
}
pub fn rgb(value: &str) -> Result<[u8; 3]> {
    let value = value.trim_start_matches('#');
    if value.len() != 6 || !value.is_ascii() {
        return Err(crate::i18n::tr(
            "helper.lock-dashboard.invalid-palette-color",
            "Invalid palette color",
        )
        .into());
    }
    Ok([
        u8::from_str_radix(&value[0..2], 16)?,
        u8::from_str_radix(&value[2..4], 16)?,
        u8::from_str_radix(&value[4..6], 16)?,
    ])
}
pub fn blend(a: [u8; 3], b: [u8; 3], amount: f64) -> [u8; 3] {
    std::array::from_fn(|i| {
        (f64::from(a[i]) + (f64::from(b[i]) - f64::from(a[i])) * amount).round_ties_even() as u8
    })
}
pub fn fitted(picture: &RgbaImage, size: (u32, u32)) -> Result<RgbaImage> {
    crate::image_paint::fit(picture, size, orient_decoder::Filter::Lanczos)
}
pub fn system_info(home: &Path) -> Result<Value> {
    let os = match fs::read_to_string("/etc/os-release") {
        Ok(text) => text
            .lines()
            .find_map(|line| line.strip_prefix("NAME="))
            .map(|v| v.trim_matches('"').to_owned())
            .unwrap_or_else(|| "Linux".into()),
        Err(error) => {
            eprintln!("Lock operating system information: {error}");
            "Linux".into()
        }
    };
    let uptime = match fs::read_to_string("/proc/uptime")
        .map_err(Into::into)
        .and_then(|text| -> Result<f64> {
            Ok(text
                .split_whitespace()
                .next()
                .ok_or("Missing uptime")?
                .parse()?)
        }) {
        Ok(seconds) => {
            let minutes = seconds as u64 / 60;
            format!("{}h {:02}m", minutes / 60, minutes % 60)
        }
        Err(error) => {
            eprintln!("Lock uptime information: {error}");
            "Unavailable".into()
        }
    };
    let user = nix::unistd::User::from_uid(nix::unistd::Uid::from_raw(fs::metadata(home)?.uid()))?
        .ok_or("Lock account is unavailable")?
        .name;
    Ok(json!({"os":os,"user":user,"uptime":uptime}))
}
pub fn notification_icon(name: &str, app: &str, paths: &Paths) -> Result<Option<PathBuf>> {
    let app = app.to_lowercase();
    let alias = match app.as_str() {
        "google chrome" => "google-chrome",
        "network management" => "nm-device-wireless",
        _ => &app,
    };
    for value in [name, alias] {
        if value.is_empty() || value.contains('/') || value.contains("..") {
            continue;
        }
        for root in [
            "/usr/share/icons/hicolor/48x48/apps",
            "/usr/share/icons/Papirus/48x48/apps",
            "/usr/share/pixmaps",
        ] {
            for suffix in [".png", ".svg"] {
                let source = Path::new(root).join(format!("{value}{suffix}"));
                if !source.is_file() {
                    continue;
                }
                if suffix == ".png" {
                    return Ok(Some(source));
                }
                let meta = fs::metadata(&source)?;
                let nanos =
                    i128::from(meta.mtime()) * 1_000_000_000 + i128::from(meta.mtime_nsec());
                let identity = format!("{}:{nanos}", source.display());
                let target = paths.cache.join("lock-ready").join(format!(
                    "icon-{}.png",
                    &format!("{:x}", Sha256::digest(identity.as_bytes()))[..20]
                ));
                if target.exists() {
                    return Ok(Some(target));
                }
                match process::run_bytes(
                    "rsvg-convert",
                    &[
                        "-w".into(),
                        "72".into(),
                        "-h".into(),
                        "72".into(),
                        source.to_string_lossy().into_owned(),
                    ],
                    None,
                    Duration::from_secs(2),
                ) {
                    Ok(output) if output.status.success() => {
                        let parent = target.parent().ok_or("Missing icon cache directory")?;
                        use std::os::unix::fs::DirBuilderExt;
                        fs::DirBuilder::new()
                            .recursive(true)
                            .mode(0o700)
                            .create(parent)?;
                        fs::write(&target, output.stdout)?;
                        fs::set_permissions(&target, fs::Permissions::from_mode(0o600))?;
                        return Ok(Some(target));
                    }
                    Ok(output) => eprintln!(
                        "Lock notification SVG conversion: {}",
                        String::from_utf8_lossy(&output.stderr)
                    ),
                    Err(error) => eprintln!("Lock notification SVG conversion: {error}"),
                }
            }
        }
    }
    Ok(None)
}
fn truth(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(v) => *v,
        Value::String(v) => !v.is_empty(),
        Value::Number(v) => v.as_f64() != Some(0.0),
        Value::Array(v) => !v.is_empty(),
        Value::Object(v) => !v.is_empty(),
    }
}
fn text(value: &Value) -> String {
    if let Some(value) = value.as_str() {
        value.into()
    } else {
        json::python_repr(value)
    }
}
fn plain(value: &Value) -> String {
    let source: String = if truth(value) {
        text(value).chars().take(2048).collect()
    } else {
        String::new()
    };
    let mut output = String::new();
    let mut index = 0usize;
    while index < source.len() {
        let rest = &source[index..];
        if rest.starts_with("<!--") {
            if let Some(end) = rest.find("-->") {
                index += end + 3;
                output.push(' ');
                continue;
            }
            break;
        }
        if rest.starts_with('<')
            && rest
                .as_bytes()
                .get(1)
                .is_some_and(|c| c.is_ascii_alphabetic() || b"/!?".contains(c))
        {
            let mut quote = None;
            let mut end = None;
            for (offset, c) in rest.char_indices().skip(1) {
                if let Some(current) = quote {
                    if c == current {
                        quote = None;
                    }
                } else if c == '\'' || c == '"' {
                    quote = Some(c);
                } else if c == '>' {
                    end = Some(offset + 1);
                    break;
                }
            }
            if let Some(end) = end {
                index += end;
                output.push(' ');
                continue;
            }
            break;
        }
        let character = rest.chars().next().unwrap();
        output.push(character);
        index += character.len_utf8();
    }
    html_escape::decode_html_entities(&output)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(1024)
        .collect()
}
struct Painter<'a> {
    data: &'a Value,
    colors: &'a Map<String, Value>,
    paths: &'a Paths,
    artwork: Option<&'a Path>,
    context: &'a mut Context,
    image: RgbaImage,
    surface: [u8; 3],
    container: [u8; 3],
    text: [u8; 3],
    muted: [u8; 3],
    accent: [u8; 3],
    details: Value,
}
impl<'a> Painter<'a> {
    fn new(
        data: &'a Value,
        colors: &'a Map<String, Value>,
        wallpaper: &Path,
        artwork: Option<&'a Path>,
        paths: &'a Paths,
        monitor: Option<&Value>,
        context: &'a mut Context,
    ) -> Result<Self> {
        let color = |role: &str| {
            colors
                .get(role)
                .and_then(Value::as_str)
                .ok_or_else(|| format!("Missing lock palette role: {role}"))
        };
        let surface = rgb(color("surface")?)?;
        let container = rgb(color("surfaceContainer")?)?;
        let text = rgb(color("onSurface")?)?;
        let muted = rgb(colors
            .get("onSurfaceVariant")
            .and_then(Value::as_str)
            .unwrap_or(color("onSurface")?))?;
        let accent = rgb(color("primary")?)?;
        let size = (WIDTH * RESOLUTION, HEIGHT * RESOLUTION);
        let mut image = RgbaImage::from_pixel(
            size.0,
            size.1,
            Rgba([surface[0], surface[1], surface[2], 235]),
        );
        if !wallpaper.as_os_str().is_empty() {
            let background = || -> Result<RgbaImage> {
                let source = orient_decoder::load_rgba(wallpaper)?;
                let default = json!({"width":2048,"height":1152});
                let monitor = monitor.unwrap_or(&default);
                let factor = layout(monitor);
                if factor <= 0.0 || !factor.is_finite() {
                    return Err("Invalid lock output scale".into());
                }
                let mut w = monitor["width"].as_f64().unwrap_or(2048.0);
                let mut h = monitor["height"].as_f64().unwrap_or(1152.0);
                if monitor["transform"].as_i64().unwrap_or(0) % 2 != 0 {
                    std::mem::swap(&mut w, &mut h);
                }
                let canvas = fitted(
                    &source,
                    (
                        (w / factor).round_ties_even() as u32,
                        (h / factor).round_ties_even() as u32,
                    ),
                )?;
                drop(source);
                let x = (canvas.width() as i64 - i64::from(WIDTH)) / 2;
                let y = (canvas.height() as i64 - i64::from(HEIGHT)) / 2;
                let crop = image::imageops::crop_imm(&canvas, x as u32, y as u32, WIDTH, HEIGHT)
                    .to_image();
                drop(canvas);
                let resized =
                    orient_decoder::resize_rgba(&crop, size, orient_decoder::Filter::Bicubic)?;
                drop(crop);
                let mut image = orient_decoder::blur_rgba(&resized, 40.0 * RESOLUTION as f32)?;
                drop(resized);
                paint::composite(
                    &mut image,
                    &RgbaImage::from_pixel(
                        size.0,
                        size.1,
                        Rgba([surface[0], surface[1], surface[2], 135]),
                    ),
                    (0, 0),
                    None,
                );
                Ok(image)
            };
            match background() {
                Ok(background) => image = background,
                Err(error) => eprintln!("Lock wallpaper texture: {error}"),
            }
        }
        let mask = paint::rounded_mask(size, 42 * RESOLUTION)?;
        for (x, y, pixel) in image.enumerate_pixels_mut() {
            pixel[3] = mask[(x, y)][0];
        }
        Ok(Self {
            data,
            colors,
            paths,
            artwork,
            context,
            image,
            surface,
            container,
            text,
            muted,
            accent,
            details: Value::Null,
        })
    }
    fn enabled(&self, key: &str) -> bool {
        self.data["preferences"].get(key).is_none_or(truth)
    }
    fn card_color(&self) -> Rgba<u8> {
        Rgba([self.container[0], self.container[1], self.container[2], 180])
    }
    fn rect(&mut self, box_: (f64, f64, f64, f64), color: Rgba<u8>, radius: u32) -> Result<()> {
        let (left, top, right, bottom) = (
            (box_.0 * 2.0).round_ties_even() as i32,
            (box_.1 * 2.0).round_ties_even() as i32,
            (box_.2 * 2.0).round_ties_even() as i32,
            (box_.3 * 2.0).round_ties_even() as i32,
        );
        let card = RgbaImage::from_pixel((right - left) as u32, (bottom - top) as u32, color);
        let mask = paint::rounded_mask(card.dimensions(), radius * 2)?;
        paint::composite(&mut self.image, &card, (left, top), Some(&mask));
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn copy(
        &mut self,
        value: &Value,
        xy: (f64, f64),
        pixels: u32,
        color: Option<[u8; 3]>,
        width: Option<f64>,
        lines: usize,
        bold: bool,
        center: bool,
    ) -> Result<()> {
        let value = plain(value);
        let face = self.context.fonts.get(
            "IBM Plex Sans",
            if bold { "Medium" } else { "Regular" },
            pixels * 2,
        )?;
        let mut rows = Vec::new();
        let mut line = String::new();
        for word in value.split_whitespace() {
            let candidate = format!("{line} {word}").trim().to_owned();
            if width.is_some_and(|width| width != 0.0)
                && face.advance(&candidate)? > width.unwrap() * 2.0
                && !line.is_empty()
            {
                rows.push(line);
                line = word.into();
            } else {
                line = candidate;
            }
        }
        rows.push(line);
        if rows.len() > lines {
            rows.truncate(lines);
            rows.last_mut()
                .ok_or("Invalid lock text line count")?
                .push('…');
        }
        for (i, mut row) in rows.into_iter().enumerate() {
            if let Some(width) = width.filter(|w| *w != 0.0) {
                while face.advance(&row)? > width * 2.0 && row.chars().count() > 1 {
                    row = row
                        .chars()
                        .take(row.chars().count() - 2)
                        .collect::<String>()
                        + "…";
                }
            }
            let mut x = xy.0;
            if center {
                x -= face.advance(&row)? / 2.0 / 2.0;
            }
            let y = xy.1 + (i * pixels as usize) as f64 * 1.45;
            let mask = face.mask(&row)?;
            paint::text(
                &mut self.image,
                &mask,
                (
                    (x * 2.0).round_ties_even() as i32,
                    (y * 2.0).round_ties_even() as i32,
                ),
                color.unwrap_or(self.text),
            );
        }
        Ok(())
    }
    fn icon(
        &mut self,
        code: u32,
        xy: (i32, i32),
        pixels: u32,
        color: Option<[u8; 3]>,
    ) -> Result<()> {
        let value = char::from_u32(code)
            .ok_or("Invalid lock icon code")?
            .to_string();
        let mask = self
            .context
            .fonts
            .get("Material Symbols Rounded", "Regular", pixels * 2)?
            .mask(&value)?;
        paint::text(
            &mut self.image,
            &mask,
            (xy.0 * 2, xy.1 * 2),
            color.unwrap_or(self.accent),
        );
        Ok(())
    }
    fn picture(
        &mut self,
        path: Option<&Path>,
        box_: (i32, i32, i32, i32),
        mask: Option<&image::GrayImage>,
    ) -> Result<bool> {
        let Some(path) = path else {
            return Ok(false);
        };
        let prepare = || -> Result<RgbaImage> {
            fitted(
                &orient_decoder::load_rgba(path)?,
                (
                    ((box_.2 - box_.0) * 2) as u32,
                    ((box_.3 - box_.1) * 2) as u32,
                ),
            )
        };
        let image = match prepare() {
            Ok(image) => image,
            Err(error) => {
                if !error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                {
                    eprintln!("Lock artwork decode: {error}");
                }
                return Ok(false);
            }
        };
        let fallback = if mask.is_none() {
            paint::rounded_mask(image.dimensions(), 52)?
        } else {
            image::GrayImage::new(0, 0)
        };
        paint::paste(
            &mut self.image,
            &image,
            (box_.0 * 2, box_.1 * 2),
            mask.unwrap_or(&fallback),
        );
        Ok(true)
    }
    fn weather(&mut self) -> Result<()> {
        if !self.enabled("lockWeather") {
            return Ok(());
        }
        self.rect((16.0, 16.0, 460.0, 150.0), self.card_color(), 26)?;
        let weather = self.data["weather"].clone();
        let temperature = if truth(&weather["temperature"]) {
            text(&weather["temperature"])
        } else {
            "—".into()
        };
        let temperature = temperature
            .strip_suffix(" (cached)")
            .unwrap_or(&temperature);
        self.copy(
            &json!(temperature),
            (92.0, 60.0),
            38,
            Some(self.accent),
            Some(150.0),
            1,
            false,
            true,
        )?;
        let code = match weather["icon"].as_str() {
            Some("sunny" | "clear_day") => 0xE81A,
            Some("rainy") => 0xF176,
            Some("thunderstorm") => 0xEBDB,
            Some("snowing") => 0xE80F,
            _ => 0xE2BD,
        };
        self.icon(code, (174, 61), 38, None)?;
        paint::vertical_line(
            &mut self.image,
            500,
            116,
            230,
            2,
            Rgba([self.muted[0], self.muted[1], self.muted[2], 90]),
        );
        let description = if truth(&weather["description"]) {
            weather["description"].clone()
        } else {
            json!("Weather unavailable")
        };
        self.copy(
            &description,
            (269.0, 54.0),
            16,
            None,
            Some(175.0),
            1,
            false,
            false,
        )?;
        let detail = if truth(&weather["detail"]) {
            weather["detail"].clone()
        } else {
            weather["location"].clone()
        };
        self.copy(
            &detail,
            (269.0, 80.0),
            12,
            Some(self.muted),
            Some(175.0),
            1,
            false,
            false,
        )?;
        let forecast = if truth(&weather["stale"]) {
            json!("Cached forecast")
        } else {
            weather.get("range").cloned().unwrap_or(json!(""))
        };
        self.copy(
            &forecast,
            (269.0, 101.0),
            12,
            Some(self.muted),
            Some(175.0),
            1,
            false,
            false,
        )
    }
    fn host(&mut self) -> Result<()> {
        self.rect((16.0, 168.0, 460.0, 428.0), self.card_color(), 26)?;
        self.rect(
            (30.0, 184.0, 60.0, 214.0),
            Rgba([self.accent[0], self.accent[1], self.accent[2], 255]),
            9,
        )?;
        self.copy(
            &json!("›"),
            (45.0, 182.0),
            27,
            Some(self.surface),
            None,
            1,
            false,
            true,
        )?;
        self.copy(
            &json!("nacrefetch.sh"),
            (69.0, 188.0),
            14,
            None,
            None,
            1,
            false,
            false,
        )?;
        paint::polygon(
            &mut self.image,
            &[
                (160.0, 246.0),
                (101.0, 356.0),
                (142.0, 337.0),
                (160.0, 308.0),
                (178.0, 337.0),
                (219.0, 356.0),
            ]
            .map(|(x, y)| (x * 2.0, y * 2.0)),
            Rgba([self.accent[0], self.accent[1], self.accent[2], 255]),
        )?;
        self.details = if let Some(info) = &self.context.system {
            info.clone()
        } else {
            system_info(&self.paths.home)?
        };
        for (i, line) in [
            format!("OS : {}", text(&self.details["os"])),
            "WM : Hyprland".into(),
            format!("USER : {}", text(&self.details["user"])),
            format!("UP : {}", text(&self.details["uptime"])),
        ]
        .iter()
        .enumerate()
        {
            self.copy(
                &json!(line),
                (238.0, 253.0 + i as f64 * 25.0),
                14,
                None,
                Some(200.0),
                1,
                false,
                false,
            )?;
        }
        for i in 0..7 {
            let color = blend(self.surface, self.accent, 0.2 + f64::from(i) * 0.13);
            paint::ellipse(
                &mut self.image,
                ((111 + i * 38) * 2, 766, (136 + i * 38) * 2, 816),
                Rgba([color[0], color[1], color[2], 255]),
            )?;
        }
        Ok(())
    }
    fn media(&mut self) -> Result<()> {
        if !self.enabled("lockMedia") {
            return Ok(());
        }
        self.rect((16.0, 446.0, 460.0, 794.0), self.card_color(), 26)?;
        let media = self.data["media"].clone();
        let active = truth(&media);
        if active && self.picture(self.artwork, (16, 446, 460, 794), None)? {
            let overlay = RgbaImage::from_pixel(888, 696, Rgba([0, 0, 0, 145]));
            let mask = paint::rounded_mask(overlay.dimensions(), 52)?;
            paint::composite(&mut self.image, &overlay, (32, 892), Some(&mask));
        } else {
            self.icon(0xE405, (220, 499), 42, None)?;
        }
        let color = if active { [245, 238, 243] } else { self.text };
        let title = if truth(&media["title"]) {
            media["title"].clone()
        } else {
            json!("Nothing playing")
        };
        self.copy(
            &title,
            (238.0, 563.0),
            20,
            Some(color),
            Some(380.0),
            2,
            true,
            true,
        )?;
        let artist = if truth(&media["artist"]) {
            media["artist"].clone()
        } else {
            json!("Your music appears here")
        };
        self.copy(
            &artist,
            (238.0, 622.0),
            15,
            Some(color),
            Some(370.0),
            1,
            false,
            true,
        )?;
        self.rect(
            (202.0, 666.0, 274.0, 708.0),
            Rgba([self.accent[0], self.accent[1], self.accent[2], 255]),
            21,
        )?;
        self.rect(
            (146.0, 669.0, 184.0, 707.0),
            Rgba([self.container[0], self.container[1], self.container[2], 190]),
            19,
        )?;
        self.rect(
            (292.0, 669.0, 330.0, 707.0),
            Rgba([self.container[0], self.container[1], self.container[2], 190]),
            19,
        )?;
        let album = if truth(&media["album"]) {
            media["album"].clone()
        } else {
            json!("MPRIS media")
        };
        self.copy(
            &album,
            (238.0, 754.0),
            11,
            Some(color),
            Some(360.0),
            1,
            false,
            true,
        )
    }
    fn identity(&mut self) -> Result<()> {
        let avatar = [
            self.paths.home.join(".face"),
            self.paths.home.join(".face.icon"),
            self.paths.share.join("branding/nacre-lock.png"),
        ]
        .into_iter()
        .find(|path| path.is_file());
        let mask = paint::avatar_mask(540)?;
        paint::paste(
            &mut self.image,
            &RgbaImage::from_pixel(
                540,
                540,
                Rgba([self.surface[0], self.surface[1], self.surface[2], 255]),
            ),
            (1170, 474),
            &mask,
        );
        if let Some(avatar) = avatar {
            if avatar
                .file_name()
                .is_some_and(|name| name == ".face" || name == ".face.icon")
            {
                self.picture(Some(&avatar), (585, 237, 855, 507), Some(&mask))?;
            } else {
                let logo = orient_decoder::thumbnail_rgba(
                    &orient_decoder::load_rgba(&avatar)?,
                    (360, 360),
                    orient_decoder::Filter::Lanczos,
                )?;
                let position = (
                    1440 - logo.width() as i32 / 2,
                    744 - logo.height() as i32 / 2,
                );
                paint::composite(&mut self.image, &logo, position, None);
            }
        }
        self.rect(
            (610.0, 537.0, 830.0, 579.0),
            Rgba([self.container[0], self.container[1], self.container[2], 140]),
            21,
        )?;
        let hour = self.context.now.hour();
        let greeting = if (5..12).contains(&hour) {
            "Good morning"
        } else if hour < 18 {
            "Good afternoon"
        } else if hour < 22 {
            "Good evening"
        } else {
            "Good night"
        };
        self.copy(
            &json!(format!("{greeting}, {}", text(&self.details["user"]))),
            (720.0, 547.0),
            14,
            Some(self.accent),
            Some(210.0),
            1,
            false,
            true,
        )?;
        let label = self
            .data
            .get("batteryLabel")
            .filter(|value| truth(value))
            .cloned()
            .unwrap_or(json!("Nacre"));
        self.copy(
            &label,
            (720.0, 747.0),
            12,
            Some(self.muted),
            None,
            1,
            false,
            true,
        )
    }
    fn system(&mut self) -> Result<()> {
        self.rect((972.0, 16.0, 1424.0, 190.0), self.card_color(), 26)?;
        let stats = self.data["system"].clone();
        for (i, (key, code, role)) in [
            ("cpu", 0xE322, "primary"),
            ("memory", 0xE322, "secondary"),
            ("storage", 0xE1DB, "tertiary"),
        ]
        .into_iter()
        .enumerate()
        {
            let left = 990 + i as i32 * 143;
            let base = rgb(self
                .colors
                .get(&format!("{role}Container"))
                .unwrap_or(&self.colors["surfaceContainer"])
                .as_str()
                .ok_or("Invalid gauge color")?)?;
            let fill = rgb(self
                .colors
                .get(role)
                .unwrap_or(&self.colors["primary"])
                .as_str()
                .ok_or("Invalid gauge color")?)?;
            let mut gauge = RgbaImage::from_pixel(252, 252, Rgba([base[0], base[1], base[2], 210]));
            let value = &stats[key];
            let fraction = if value.is_null() {
                0.0
            } else if let Some(number) = value.as_f64() {
                number.clamp(0.0, 1.0)
            } else if let Some(number) = value.as_str() {
                number.parse::<f64>()?.clamp(0.0, 1.0)
            } else {
                return Err("Invalid system gauge fraction".into());
            };
            let mut points = vec![(0.0, 252.0), (252.0, 252.0)];
            for x in (0..=126).rev().step_by(2) {
                let y = 126.0 * (1.0 - fraction) + 4.0 * (f64::from(x) / 16.0 + i as f64).sin();
                points.push((f64::from(x) * 2.0, y.clamp(0.0, 126.0) * 2.0));
            }
            paint::polygon(&mut gauge, &points, Rgba([fill[0], fill[1], fill[2], 125]))?;
            let mask = if i == 1 {
                paint::rounded_mask(gauge.dimensions(), 56)?
            } else {
                paint::avatar_mask(252)?
            };
            paint::composite(&mut self.image, &gauge, (left * 2, 84), Some(&mask));
            self.icon(code, (left + 46, 63), 31, Some(self.text))?;
            let label = if value.is_null() {
                "—".into()
            } else {
                format!("{}%", (fraction * 100.0).round_ties_even() as i64)
            };
            self.copy(
                &json!(label),
                (f64::from(left + 63), 109.0),
                30,
                Some(self.text),
                None,
                1,
                false,
                true,
            )?;
        }
        if let Some(temp) = stats.get("temperature").filter(|v| !v.is_null()) {
            let value = temp.as_f64().ok_or("Invalid lock temperature")?;
            self.copy(
                &json!(format!("{}°C", value.round_ties_even() as i64)),
                (1063.0, 23.0),
                14,
                Some(self.muted),
                None,
                1,
                false,
                false,
            )?;
        }
        Ok(())
    }
    fn notifications(&mut self) -> Result<()> {
        if !self.enabled("lockNotifications") {
            return Ok(());
        }
        self.rect((972.0, 208.0, 1424.0, 794.0), self.card_color(), 26)?;
        let notifications = self.data["notifications"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let count = self
            .data
            .get("count")
            .cloned()
            .unwrap_or(json!(notifications.len()));
        self.copy(
            &json!(format!("{} notifications", text(&count))),
            (994.0, 229.0),
            14,
            Some(self.muted),
            None,
            1,
            false,
            false,
        )?;
        self.icon(0xE164, (1383, 229), 19, Some(self.muted))?;
        let mut groups: Vec<(String, Vec<Value>)> = Vec::new();
        for entry in notifications {
            let app = entry
                .get("app")
                .filter(|v| truth(v))
                .map(text)
                .unwrap_or_else(|| "Notification".into());
            if let Some((_, entries)) = groups.iter_mut().find(|(name, _)| *name == app) {
                entries.push(entry);
            } else {
                groups.push((app, vec![entry]));
            }
        }
        if groups.is_empty() {
            self.icon(0xE7F4, (1180, 403), 37, Some(self.accent))?;
            self.copy(
                &json!("You are all caught up"),
                (1198.0, 469.0),
                17,
                Some(self.muted),
                None,
                1,
                false,
                true,
            )?;
        }
        for (i, (app, entries)) in groups.into_iter().take(4).enumerate() {
            let y = 289 + i as i32 * 116;
            self.rect(
                (1003.0, f64::from(y), 1051.0, f64::from(y + 48)),
                Rgba([self.surface[0], self.surface[1], self.surface[2], 130]),
                24,
            )?;
            let icon =
                notification_icon(entries[0]["icon"].as_str().unwrap_or(""), &app, self.paths)?;
            if let Some(icon) = icon {
                self.picture(Some(&icon), (1011, y + 8, 1043, y + 40), None)?;
            } else {
                self.icon(0xE7F4, (1015, y + 12), 25, Some(self.accent))?;
            }
            if let Some(timestamp) = entries[0]["time"].as_str() {
                let seconds = if let Ok(timestamp) = DateTime::parse_from_rfc3339(timestamp) {
                    Some((self.context.now - timestamp).num_milliseconds() as f64 / 1000.0)
                } else if let Ok(timestamp) =
                    NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%dT%H:%M:%S%.f")
                {
                    Some(
                        (self.context.now.naive_local() - timestamp).num_milliseconds() as f64
                            / 1000.0,
                    )
                } else {
                    None
                };
                if let Some(seconds) = seconds {
                    let minutes = (seconds / 60.0).round_ties_even().max(0.0) as u64;
                    let age = if minutes < 60 {
                        format!("{minutes}m")
                    } else {
                        format!("{}h", minutes / 60)
                    };
                    self.copy(
                        &json!(age),
                        (1342.0, f64::from(y + 3)),
                        12,
                        Some(self.muted),
                        None,
                        1,
                        false,
                        true,
                    )?;
                }
            }
            self.copy(
                &json!(app),
                (1067.0, f64::from(y)),
                16,
                None,
                Some(235.0),
                1,
                true,
                false,
            )?;
            self.copy(
                &json!(entries.len().to_string()),
                (1382.0, f64::from(y + 3)),
                12,
                Some(self.muted),
                None,
                1,
                false,
                true,
            )?;
            if self.data["preferences"]["lockNotificationContents"]
                .as_bool()
                .unwrap_or(false)
            {
                self.copy(
                    &entries[0]["summary"],
                    (1067.0, f64::from(y + 29)),
                    14,
                    Some(self.muted),
                    Some(325.0),
                    2,
                    false,
                    false,
                )?;
                self.copy(
                    &entries[0]["body"],
                    (1067.0, f64::from(y + 70)),
                    12,
                    Some(self.muted),
                    Some(325.0),
                    1,
                    false,
                    false,
                )?;
            } else {
                self.copy(
                    &json!("Message previews hidden"),
                    (1067.0, f64::from(y + 32)),
                    14,
                    Some(self.muted),
                    Some(325.0),
                    1,
                    false,
                    false,
                )?;
            }
        }
        Ok(())
    }
    fn render(mut self) -> Result<RgbaImage> {
        self.weather()?;
        self.host()?;
        self.media()?;
        self.identity()?;
        self.system()?;
        self.notifications()?;
        Ok(self.image)
    }
}
pub fn render(
    data: &Value,
    colors: &Map<String, Value>,
    wallpaper: &Path,
    artwork: Option<&Path>,
    paths: &Paths,
    monitor: Option<&Value>,
    context: &mut Context,
) -> Result<RgbaImage> {
    Painter::new(data, colors, wallpaper, artwork, paths, monitor, context)?.render()
}

pub fn publish(
    data: &Value,
    colors: &Map<String, Value>,
    wallpaper: &Path,
    artwork: Option<&Path>,
    paths: &Paths,
    monitors: &[Value],
) -> Result<Value> {
    publish_with_context(
        data,
        colors,
        wallpaper,
        artwork,
        paths,
        monitors,
        &mut Context::default(),
    )
}
pub fn publish_with_context(
    data: &Value,
    colors: &Map<String, Value>,
    wallpaper: &Path,
    artwork: Option<&Path>,
    paths: &Paths,
    monitors: &[Value],
    context: &mut Context,
) -> Result<Value> {
    use std::os::unix::fs::DirBuilderExt;
    let ready = paths.cache.join("lock-ready");
    fs::create_dir_all(ready.parent().ok_or("Invalid lock cache")?)?;
    match fs::DirBuilder::new().mode(0o700).create(&ready) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && ready.is_dir() => (),
        Err(error) => return Err(error.into()),
    }
    let _guard = crate::files::lock(&ready.join("dashboard.lock"))?;
    let fallback = vec![json!({"name":"","width":1920,"height":1080})];
    let mut output = Vec::new();
    for monitor in if monitors.is_empty() {
        &fallback
    } else {
        monitors
    } {
        let key = crate::lock_config::panel_key(monitor)?;
        let picture = render(
            data,
            colors,
            wallpaper,
            artwork,
            paths,
            Some(monitor),
            context,
        )?;
        let raw = orient_decoder::encode_rgba_png(&picture, 2)?;
        let fingerprint = format!("{:x}", Sha256::digest(&raw));
        let path = ready.join(format!("dashboard-{key}-{}.png", &fingerprint[..16]));
        if !path.exists() {
            crate::files::atomic(&path, &raw)?;
        }
        let initial = ready.join(format!("dashboard-{key}-initial.png"));
        let temporary = initial.with_extension("next");
        match fs::remove_file(&temporary) {
            Ok(()) => (),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (),
            Err(e) => return Err(e.into()),
        }
        fs::hard_link(&path, &temporary)?;
        fs::rename(&temporary, &initial)?;
        let pointer = ready.join(format!("dashboard-{key}.txt"));
        crate::files::atomic(&pointer, path.to_string_lossy().as_bytes())?;
        output.push(json!({"monitor":monitor.get("name").cloned().unwrap_or(json!("")),"path":path,"pointer":pointer}));
    }
    let current: std::collections::BTreeSet<_> = output
        .iter()
        .filter_map(|row| row["path"].as_str())
        .map(str::to_owned)
        .collect();
    let mut old = fs::read_dir(&ready)?
        .map(|entry| -> Result<_> {
            let path = entry?.path();
            let metadata = fs::metadata(&path)?;
            Ok((metadata.modified()?, path))
        })
        .collect::<Result<Vec<_>>>()?;
    old.retain(|(_, path)| {
        path.extension().is_some_and(|s| s == "png")
            && path.file_stem().is_some_and(|s| {
                let name = s.to_string_lossy();
                name.starts_with("dashboard-")
                    && name[10..].contains('-')
                    && !name.ends_with("-initial")
            })
    });
    old.sort_by_key(|(mtime, _)| std::cmp::Reverse(*mtime));
    let limit = 12usize.max(output.len() * 3);
    let mut keep = current.clone();
    for (_, path) in old
        .iter()
        .filter(|(_, p)| !current.contains(p.to_string_lossy().as_ref()))
        .take(limit.saturating_sub(current.len()))
    {
        keep.insert(path.to_string_lossy().into_owned());
    }
    for (_, path) in old {
        if !keep.contains(path.to_string_lossy().as_ref()) {
            fs::remove_file(path)?;
        }
    }
    let mut metadata = data
        .as_object()
        .ok_or("Invalid lock dashboard data")?
        .clone();
    metadata.insert(
        "panel".into(),
        output.first().ok_or("No lock panel")?["path"].clone(),
    );
    metadata.insert("wallpaper".into(), json!(wallpaper));
    metadata.insert("colors".into(), json!(colors));
    crate::files::atomic(
        &ready.join("dashboard.json"),
        json::dumps(&metadata, false)?.as_bytes(),
    )?;
    Ok(json!(output))
}
