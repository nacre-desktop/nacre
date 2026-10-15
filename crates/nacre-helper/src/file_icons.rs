//! Palette-matched Papirus overlays; regular user overrides remain untouched.
use crate::{paths::Paths, Result};
use std::{fs, os::unix::fs::symlink, path::Path};
pub fn family(color: &str) -> Result<&'static str> {
    let rgb = crate::classic_state::legacy_rgb(color)?;
    let high = rgb.into_iter().fold(f64::NEG_INFINITY, f64::max);
    let low = rgb.into_iter().fold(f64::INFINITY, f64::min);
    let delta = high - low;
    let saturation = if high == 0.0 { 0.0 } else { delta / high };
    if saturation < 0.16 {
        return Ok("grey");
    }
    let mut hue = if delta == 0.0 {
        0.0
    } else if rgb[0] == high {
        (rgb[1] - rgb[2]) / delta
    } else if rgb[1] == high {
        2.0 + (rgb[2] - rgb[0]) / delta
    } else {
        4.0 + (rgb[0] - rgb[1]) / delta
    };
    hue = (hue / 6.0).rem_euclid(1.0) * 360.0;
    if saturation < 0.36 && (20.0..70.0).contains(&hue) {
        return Ok("brown");
    }
    Ok([
        (20.0, "red"),
        (45.0, "orange"),
        (70.0, "yellow"),
        (165.0, "green"),
        (190.0, "teal"),
        (207.0, "cyan"),
        (252.0, "blue"),
        (278.0, "indigo"),
        (307.0, "violet"),
        (342.0, "pink"),
        (360.0, "red"),
    ]
    .into_iter()
    .find(|(end, _)| hue < *end)
    .map_or("red", |(_, name)| name))
}
pub fn overlay_index(directories: &[(String, u32)], fallback: &str) -> String {
    let mut text=format!("[Icon Theme]\nName=Nacre Files\nComment=Palette-matched Papirus folders\nInherits={fallback},Papirus,Adwaita,hicolor\nDirectories={}\n",directories.iter().map(|(name,_)|name.as_str()).collect::<Vec<_>>().join(","));
    for (path, size) in directories {
        text.push_str(&format!(
            "\n[{path}]\nSize={size}\nContext=Places\nType=Fixed\n"
        ));
    }
    text
}
pub fn overlay_link(path: &Path, source: &Path) -> Result<()> {
    if path.exists() && !path.is_symlink() {
        return Ok(());
    }
    let source = source.canonicalize()?;
    if path.is_symlink() && path.canonicalize().ok().as_ref() == Some(&source) {
        return Ok(());
    }
    let parent = path.parent().ok_or("Invalid icon link")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::NamedTempFile::new_in(parent)?.into_temp_path();
    fs::remove_file(&temporary)?;
    symlink(source, &temporary)?;
    fs::rename(&temporary, path)?;
    Ok(())
}
pub fn theme(paths: &Paths, system: &Path, primary: &str, mode: &str) -> Result<String> {
    let local = paths.home.join(".local/share/icons");
    let fallback = if mode == "dark" {
        "Papirus-Dark"
    } else {
        "Papirus"
    };
    if ![&local, system]
        .iter()
        .any(|base| base.join(fallback).join("index.theme").is_file())
    {
        return Ok(String::new());
    }
    let Some(source) = [system, &local]
        .into_iter()
        .find(|base| base.join("Papirus/index.theme").is_file())
        .map(|base| base.join("Papirus"))
    else {
        return Ok(fallback.into());
    };
    let color = family(primary)?;
    let name = format!("Nacre-Papirus-v3-{color}-{mode}");
    let target = local.join(&name);
    let index = target.join("index.theme");
    let previous = match fs::read_to_string(&index) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e.into()),
    };
    if !previous.is_empty() {
        let re = regex::Regex::new(r"(?m)^\[(32x32|48x48|64x64)/places\]$")?;
        let sizes = re
            .captures_iter(&previous)
            .map(|capture| {
                let size = capture[1].split('x').next().unwrap_or("").parse::<u32>()?;
                Ok((format!("{size}x{size}/places"), size))
            })
            .collect::<Result<Vec<_>>>()?;
        if previous != overlay_index(&sizes, fallback) {
            return Ok(name);
        }
    }
    let mut directories = Vec::new();
    let prefix = format!("folder-{color}");
    for size in [32, 48, 64] {
        let relative = format!("{size}x{size}/places");
        let folder = source.join(&relative);
        if !folder.join(format!("{prefix}.svg")).exists() {
            continue;
        }
        let destination = target.join(&relative);
        fs::create_dir_all(&destination)?;
        for entry in fs::read_dir(folder)? {
            let path = entry?.path();
            let Some(stem) = path.file_stem().and_then(|v| v.to_str()) else {
                continue;
            };
            if path.extension().is_none_or(|v| v != "svg")
                || (stem != prefix && !stem.starts_with(&format!("{prefix}-")))
            {
                continue;
            }
            overlay_link(
                &destination.join(format!("folder{}.svg", &stem[prefix.len()..])),
                &path,
            )?;
        }
        for (alias, source) in [
            ("inode-directory.svg", "folder.svg"),
            ("user-desktop.svg", "folder-desktop.svg"),
            ("desktop.svg", "folder-desktop.svg"),
        ] {
            let source = destination.join(source);
            if source.exists() {
                overlay_link(&destination.join(alias), &source)?;
            }
        }
        directories.push((relative, size));
    }
    if directories.is_empty() {
        return Ok(fallback.into());
    }
    let content = overlay_index(&directories, fallback);
    if content != previous {
        fs::write(index, content)?;
    }
    Ok(name)
}
