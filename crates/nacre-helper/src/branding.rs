//! Publish the existing approved vector/raster art and refresh registered terminals.
use crate::{classic_state::luminance, i18n::tr, paths::Paths, process, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};
pub const APPLICATIONS: [&str; 5] = ["ai", "brain", "settings", "colors", "notifications"];
pub const VIEWBOX: &str = "2.8 3 58 58";
pub fn master(assets: &Path) -> Result<PathBuf> {
    let parent = assets.parent().ok_or("Invalid branding assets")?;
    [
        parent.join("shell/branding/nacre-master.svg"),
        parent.join("source/branding/nacre-master.svg"),
    ]
    .into_iter()
    .find(|p| p.is_file())
    .ok_or_else(|| "Nacre branding master is unavailable".into())
}
pub fn asset_names() -> Vec<(String, String)> {
    let mut result = Vec::new();
    for app in std::iter::once("").chain(APPLICATIONS) {
        let stem = if app.is_empty() {
            "nacre".into()
        } else {
            format!("nacre-{app}")
        };
        let prefix = if app.is_empty() {
            String::new()
        } else {
            format!("{app}-")
        };
        for variant in ["full", "compact", "symbolic"] {
            result.push((
                format!("{prefix}{variant}"),
                format!(
                    "{stem}{}.svg",
                    if variant == "full" {
                        String::new()
                    } else {
                        format!("-{variant}")
                    }
                ),
            ));
        }
    }
    result
}
pub fn templates(assets: &Path) -> Result<Map<String, Value>> {
    let original = fs::read_to_string(master(assets)?)?;
    let document = roxmltree::Document::parse(&original)?;
    let root = document.root_element();
    let ns = "http://www.w3.org/2000/svg";
    let chambers: Vec<_> = root
        .children()
        .filter(|n| {
            n.has_tag_name((ns, "path")) && n.attribute("fill").is_some_and(|v| v.starts_with('@'))
        })
        .collect();
    let pearl = root
        .descendants()
        .find(|n| {
            n.has_tag_name((ns, "circle"))
                && n.attribute("fill") == Some("url(#nacre-logo-pearl-body)")
        })
        .ok_or("Branding pearl is missing")?;
    let opening = format!("<svg xmlns=\"{ns}\" viewBox=\"{VIEWBOX}\">");
    let dot = format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"{}\" fill=\"@HIGHLIGHT@\"/>",
        pearl.attribute("cx").ok_or("Missing cx")?,
        pearl.attribute("cy").ok_or("Missing cy")?,
        pearl.attribute("r").ok_or("Missing r")?
    );
    let mut compact = opening.clone();
    for node in &chambers {
        compact.push_str(&format!(
            "<path d=\"{}\" fill=\"{}\" fill-rule=\"evenodd\"/>",
            node.attribute("d").ok_or("Missing chamber path")?,
            node.attribute("fill").ok_or("Missing chamber fill")?
        ));
    }
    compact.push_str(&dot);
    compact.push_str("</svg>");
    let symbolic = format!(
        "{opening}<path fill=\"currentColor\" fill-rule=\"evenodd\" d=\"{}\"/>{}</svg>",
        chambers
            .iter()
            .map(|n| n.attribute("d").unwrap_or(""))
            .collect::<Vec<_>>()
            .join(" "),
        dot.replace("@HIGHLIGHT@", "currentColor")
    );
    let full = regex::Regex::new(r#"viewBox="[^"]+""#)?
        .replace(&original, format!("viewBox=\"{VIEWBOX}\""))
        .into_owned();
    let mut values: Map<_, _> = [
        ("full".into(), json!(full)),
        ("compact".into(), json!(compact)),
        ("symbolic".into(), json!(symbolic)),
    ]
    .into_iter()
    .collect();
    let symbolic_roles = regex::Regex::new(r"@[A-Z_]+@")?;
    for app in APPLICATIONS {
        let label = fs::read_to_string(assets.join(format!("{app}-label.svg.in")))?;
        for variant in ["full", "compact", "symbolic"] {
            let suffix = if variant == "symbolic" {
                symbolic_roles
                    .replace_all(&label, "currentColor")
                    .into_owned()
            } else {
                label.clone()
            };
            let pattern = values[variant]
                .as_str()
                .ok_or("Invalid branding template")?;
            values.insert(
                format!("{app}-{variant}"),
                json!(pattern.replace("</svg>", &format!("{suffix}</svg>"))),
            );
        }
    }
    Ok(values)
}
fn clean(value: &str) -> Result<String> {
    let value = value.trim_start_matches('#');
    if value.len() != 6 || !value.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(tr("helper.branding.invalid-logo-color", "Invalid logo color").into());
    }
    Ok(format!("#{value}"))
}
pub fn role_colors(colors: &Map<String, Value>) -> Result<Map<String, Value>> {
    let color = |key: &str, default: &str| {
        colors
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or(default)
            .to_owned()
    };
    let bg = color("frame", &color("surface", "101014"))
        .trim_start_matches('#')
        .to_owned();
    let background = luminance(&bg)?;
    let foreground = clean(&color(
        "onSurface",
        if background < 0.5 { "ffffff" } else { "101014" },
    ))?;
    let ratio = |value: &str| -> Result<f64> {
        let value = luminance(value)?;
        Ok((value.max(background) + 0.05) / (value.min(background) + 0.05))
    };
    let lift = |value: &str, minimum: f64| -> Result<String> {
        let value = clean(value)?;
        if ratio(&value)? >= minimum {
            return Ok(value);
        }
        let a = orient_core::colour::rgb(&value)?.map(|v| v * 255.0);
        let target = if ratio(&foreground)? >= minimum {
            foreground.clone()
        } else if ratio("#ffffff")? >= ratio("#000000")? {
            "#ffffff".into()
        } else {
            "#000000".into()
        };
        let b = orient_core::colour::rgb(&target)?.map(|v| v * 255.0);
        for step in 1..=20 {
            let rgb: Vec<_> = a
                .iter()
                .zip(b)
                .map(|(x, y)| (x + (y - x) * f64::from(step) / 20.0).round_ties_even() as u8)
                .collect();
            let result = format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
            if ratio(&result)? >= minimum {
                return Ok(result);
            }
        }
        Ok(target)
    };
    let primary = lift(&color("primary", "47a99a"), 3.4)?;
    Ok([
        ("PRIMARY", primary.clone()),
        (
            "ON_PRIMARY",
            if luminance(&primary)? < 0.179 {
                "#ffffff"
            } else {
                "#000000"
            }
            .into(),
        ),
        ("SECONDARY", lift(&color("secondary", &primary), 3.4)?),
        ("AI_PRIMARY", lift(&color("primary", &primary), 4.5)?),
        ("AI_SECONDARY", lift(&color("secondary", &primary), 4.5)?),
        (
            "TERTIARY",
            lift(&color("tertiary", &color("secondary", &primary)), 3.4)?,
        ),
        (
            "HIGHLIGHT",
            if background > 0.5 {
                primary
            } else {
                clean(&color("primaryFixed", &color("onSurface", "f4f1ef")))?
            },
        ),
    ]
    .into_iter()
    .map(|(key, value)| (key.into(), json!(value)))
    .collect())
}
pub fn paint_svg(text: &str, roles: &Map<String, Value>) -> Result<String> {
    let mut result = text.to_owned();
    for (name, value) in roles {
        result = result.replace(
            &format!("@{name}@"),
            value.as_str().ok_or("Invalid branding role")?,
        );
    }
    Ok(result.replace(
        "currentColor",
        roles["PRIMARY"]
            .as_str()
            .ok_or("Missing primary branding role")?,
    ))
}
fn renderer() -> Result<PathBuf> {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path)
        .map(|p| p.join("rsvg-convert"))
        .find(|p| fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0))
        .ok_or_else(|| {
            tr(
                "helper.branding.nacre-logo-rasterization-needs-rsvg-convert-from-librsvg",
                "Nacre logo rasterization needs rsvg-convert from librsvg",
            )
            .into()
        })
}
pub fn raster(text: &str, size: u32) -> Result<image::RgbaImage> {
    let binary = renderer()?;
    let output = process::run_bytes(
        binary.to_str().ok_or("Invalid SVG renderer path")?,
        &[
            "--format=png".into(),
            format!("--width={size}"),
            format!("--height={size}"),
        ],
        Some(text.as_bytes()),
        Duration::from_secs(10),
    )?;
    if !output.status.success() {
        return Err(format!(
            "SVG rasterization failed: {}",
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(image::load_from_memory(&output.stdout)?.to_rgba8())
}
pub fn atomic(path: &Path, data: &[u8]) -> Result<()> {
    let parent = path.parent().ok_or("Invalid branding output")?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(data)?;
    temporary
        .as_file()
        .set_permissions(fs::Permissions::from_mode(0o644))?;
    temporary.persist(path)?;
    Ok(())
}
pub fn cached_png(text: &str, paths: &Paths, size: u32) -> Result<Vec<u8>> {
    let metadata = fs::metadata(renderer()?)?;
    let nanos = i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec());
    let key = format!(
        "{:x}",
        Sha256::digest(format!("{text}{size}{}{nanos}", metadata.len()).as_bytes())
    );
    let folder = paths.cache.join("branding");
    let path = folder.join(format!("{key}.png"));
    match fs::read(&path) {
        Ok(raw) => match image::load_from_memory(&raw) {
            Ok(image)
                if image.width() == size
                    && image.height() == size
                    && image.color() == image::ColorType::Rgba8 =>
            {
                return Ok(raw)
            }
            Ok(_) => eprintln!(
                "{}",
                tr(
                    "helper.branding.invalid-branding-cache",
                    "Invalid branding cache"
                )
            ),
            Err(error) => eprintln!("Branding cache validation: {error}"),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(error) => return Err(error.into()),
    }
    let raw = orient_decoder::encode_rgba_png(&raster(text, size)?, 6)?;
    atomic(&path, &raw)?;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
    let mut entries = fs::read_dir(&folder)?
        .map(|entry| -> Result<_> {
            let path = entry?.path();
            let metadata = fs::metadata(&path)?;
            Ok((metadata.modified()?, path))
        })
        .collect::<Result<Vec<_>>>()?;
    entries.retain(|(_, path)| path.extension().is_some_and(|v| v == "png"));
    entries.sort_by_key(|(modified, _)| std::cmp::Reverse(*modified));
    for (_, path) in entries.into_iter().skip(1024) {
        fs::remove_file(path)?;
    }
    Ok(raw)
}
pub fn text_logo(assets: &Path, variant: &str, paths: Option<&Paths>) -> Result<String> {
    let text = templates(assets)?[variant]
        .as_str()
        .ok_or("Missing branding variant")?
        .replace("currentColor", "#ffffff");
    let source = if let Some(paths) = paths {
        image::load_from_memory(&cached_png(&text, paths, 96)?)?.to_rgba8()
    } else {
        raster(&text, 96)?
    };
    let image = orient_decoder::resize_rgba(&source, (24, 12), orient_decoder::Filter::Lanczos)?;
    let mut output = String::new();
    for y in (0..12).step_by(4) {
        for x in (0..24).step_by(2) {
            let mut bits = 0u32;
            for (dx, dy, bit) in [
                (0, 0, 0),
                (0, 1, 1),
                (0, 2, 2),
                (1, 0, 3),
                (1, 1, 4),
                (1, 2, 5),
                (0, 3, 6),
                (1, 3, 7),
            ] {
                if image[(x + dx, y + dy)][3] >= 100 {
                    bits |= 1 << bit;
                }
            }
            output.push(char::from_u32(0x2800 + bits).ok_or("Invalid braille logo")?);
        }
        output.push('\n');
    }
    Ok(output)
}
pub fn publish(colors: &Map<String, Value>, paths: &Paths, assets: &Path) -> Result<()> {
    let folder = paths.share.join("branding");
    let patterns = templates(assets)?;
    let roles = role_colors(colors)?;
    let render = |variant: &str| {
        paint_svg(
            patterns[variant]
                .as_str()
                .ok_or("Missing branding variant")?,
            &roles,
        )
    };
    let raw = cached_png(&render("full")?, paths, 512)?;
    atomic(&folder.join("nacre-lock.png"), &raw)?;
    atomic(&folder.join("nacre.png"), &raw)?;
    for app in APPLICATIONS {
        atomic(
            &folder.join(format!("nacre-{app}.png")),
            &cached_png(&render(&format!("{app}-full"))?, paths, 512)?,
        )?;
    }
    for (variant, name) in asset_names() {
        atomic(&folder.join(name), render(&variant)?.as_bytes())?;
    }
    atomic(
        &folder.join("nacre-text.txt"),
        text_logo(assets, "symbolic", Some(paths))?.as_bytes(),
    )?;
    atomic(
        &folder.join("nacre-ai-text.txt"),
        text_logo(assets, "ai-symbolic", Some(paths))?.as_bytes(),
    )?;
    for app in std::iter::once("").chain(APPLICATIONS) {
        let name = if app.is_empty() {
            "nacre".into()
        } else {
            format!("nacre-{app}")
        };
        let variant = if app.is_empty() {
            "full".into()
        } else {
            format!("{app}-full")
        };
        atomic(
            &paths
                .home
                .join(".local/share/icons/hicolor/scalable/apps")
                .join(format!("{name}.svg")),
            render(&variant)?.as_bytes(),
        )?;
    }
    for (old, current) in [
        ("sh.png", "nacre.png"),
        ("sh-lock.png", "nacre-lock.png"),
        ("sh.svg", "nacre.svg"),
    ] {
        atomic(&folder.join(old), &fs::read(folder.join(current))?)?;
    }
    refresh_terminal_menus(paths, Path::new("/proc"))?;
    crate::terminal_branding::refresh(paths, Path::new("/proc"))
}
pub fn refresh_terminal_menus(paths: &Paths, proc: &Path) -> Result<()> {
    let command = paths.home.join(".local/bin/nacre-ai");
    let resolved = command.canonicalize().unwrap_or_else(|_| command.clone());
    for entry in fs::read_dir(proc)? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<i32>().ok())
            .and_then(rustix::process::Pid::from_raw)
        else {
            continue;
        };
        let root = entry.path();
        let inspect = || -> Result<()> {
            if fs::metadata(&root)?.uid() != nix::unistd::getuid().as_raw() {
                return Ok(());
            }
            let raw = fs::read(root.join("cmdline"))?;
            let args = raw
                .split(|b| *b == 0)
                .filter(|s| !s.is_empty())
                .map(std::str::from_utf8)
                .collect::<std::result::Result<Vec<_>, _>>()?;
            if args.len() < 2
                || !Path::new(args[0])
                    .file_name()
                    .is_some_and(|v| v.to_string_lossy().starts_with("python"))
                || (Path::new(args[1]) != command && Path::new(args[1]) != resolved)
                || (args.len() > 2 && !["dashboard", "settings"].contains(&args[2]))
            {
                return Ok(());
            }
            let environment = fs::read(root.join("environ"))?;
            let home = format!("HOME={}", paths.home.display());
            if !environment
                .split(|byte| *byte == 0)
                .any(|value| value == home.as_bytes())
            {
                return Ok(());
            }
            let descriptor =
                rustix::process::pidfd_open(pid, rustix::process::PidfdFlags::empty())?;
            if fs::read(root.join("cmdline"))? == raw {
                rustix::process::pidfd_send_signal(&descriptor, rustix::process::Signal::WINCH)?;
            }
            Ok(())
        };
        if let Err(error) = inspect() {
            if !error.downcast_ref::<std::io::Error>().is_some_and(|e| {
                matches!(
                    e.kind(),
                    std::io::ErrorKind::NotFound | std::io::ErrorKind::PermissionDenied
                )
            }) {
                eprintln!("Terminal menu refresh: {error}");
            }
        }
    }
    Ok(())
}

pub fn run(arguments: &[String]) -> Result<i32> {
    use std::io::Read;
    let paths = Paths::environment()?;
    let assets = paths.assets();
    if arguments == ["--publish"] {
        let mut text = String::new();
        std::io::stdin()
            .take(16 * 1024 * 1024 + 1)
            .read_to_string(&mut text)?;
        if text.len() > 16 * 1024 * 1024 {
            return Err("Branding request is too large".into());
        }
        let request: Value = serde_json::from_str(&text)?;
        let requested = request["home"]
            .as_str()
            .map(Path::new)
            .map(Paths::for_home)
            .transpose()?
            .unwrap_or(paths);
        publish(
            request["colors"]
                .as_object()
                .ok_or("Invalid branding colors")?,
            &requested,
            &assets,
        )?;
    } else if arguments.is_empty() {
        let data = crate::state::read_strict(&paths.state.join("scheme.json"), json!({}))?;
        publish(
            data["colours"].as_object().ok_or("Missing scheme colors")?,
            &paths,
            &assets,
        )?;
    } else {
        return Err("Unsupported branding command".into());
    }
    Ok(0)
}

pub fn prepare(colors: &Map<String, Value>, paths: &Paths, assets: &Path) -> Result<()> {
    let patterns = templates(assets)?;
    let roles = role_colors(colors)?;
    for variant in
        std::iter::once("full".to_owned()).chain(APPLICATIONS.map(|app| format!("{app}-full")))
    {
        cached_png(
            &paint_svg(
                patterns[&variant]
                    .as_str()
                    .ok_or("Missing branding template")?,
                &roles,
            )?,
            paths,
            512,
        )?;
    }
    for variant in ["symbolic", "ai-symbolic"] {
        cached_png(
            &patterns[variant]
                .as_str()
                .ok_or("Missing symbolic branding")?
                .replace("currentColor", "#ffffff"),
            paths,
            96,
        )?;
    }
    Ok(())
}
