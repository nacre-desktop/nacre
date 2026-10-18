//! Reviewed offline palette/wallpaper bundles; no import hooks or executable recipes.
use crate::{
    classic_state::{self, Publisher},
    files, state, Result,
};
use orient_core::{accessibility, colour, Colors};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

/// Preserve upstream surfaces and accents, derive the remaining roles with Orient,
/// then repair every foreground against all of its actual backgrounds together.
pub fn map_palette(source: &Value, mode: &str) -> Result<Colors> {
    let color = |key: &str| {
        source[key]
            .as_str()
            .ok_or_else(|| format!("Missing upstream color {key}"))
    };
    let rose = source.get("iris").is_some();
    let primary = color(if rose { "iris" } else { "mauve" })?;
    let mut colors = orient_core::palette::generate(
        primary,
        &json!({"mode":mode,"overrides":{"secondary":color(if rose {"foam"} else {"teal"})?,"tertiary":color(if rose {"rose"} else {"pink"})?}}),
    )?;
    let base = color("base")?;
    let low = color(if rose { "surface" } else { "mantle" })?;
    let high = color(if rose { "overlay" } else { "surface0" })?;
    for (key, value) in [
        ("background", base),
        ("surface", base),
        ("surfaceDim", base),
        ("surfaceBright", high),
        ("surfaceContainerLowest", base),
        ("surfaceContainerLow", low),
        ("surfaceContainer", low),
        ("surfaceContainerHigh", high),
        ("surfaceContainerHighest", high),
        ("frame", low),
        ("surfaceVariant", high),
        ("base", base),
        ("mantle", low),
        ("crust", base),
        ("onSurface", color("text")?),
        ("onBackground", color("text")?),
        (
            "onSurfaceVariant",
            color(if rose { "subtle" } else { "subtext0" })?,
        ),
        ("primary", primary),
        ("primaryDim", primary),
        ("secondary", color(if rose { "foam" } else { "teal" })?),
        ("tertiary", color(if rose { "rose" } else { "pink" })?),
    ] {
        colors.insert(key.into(), json!(value));
    }
    if rose {
        for (target, source) in [
            ("red", "love"),
            ("error", "love"),
            ("green", "pine"),
            ("success", "pine"),
            ("blue", "foam"),
            ("yellow", "gold"),
            ("teal", "foam"),
            ("mauve", "iris"),
            ("pink", "rose"),
            ("peach", "gold"),
            ("lavender", "iris"),
            ("sky", "foam"),
            ("sapphire", "pine"),
            ("flamingo", "rose"),
            ("maroon", "love"),
            ("rosewater", "rose"),
        ] {
            colors.insert(target.into(), json!(color(source)?));
        }
    } else {
        for key in [
            "rosewater",
            "flamingo",
            "pink",
            "mauve",
            "red",
            "maroon",
            "peach",
            "yellow",
            "green",
            "teal",
            "sky",
            "sapphire",
            "blue",
            "lavender",
        ] {
            colors.insert(key.into(), json!(color(key)?));
        }
        colors.insert("error".into(), json!(color("red")?));
        colors.insert("success".into(), json!(color("green")?));
    }
    // The native light-terminal export uses inverse roles for its background.
    if mode == "light" {
        colors.insert("inverseSurface".into(), json!(base));
        colors.insert("inverseOnSurface".into(), json!(color("text")?));
        colors.insert("inversePrimary".into(), json!(primary));
    }
    let mut groups = BTreeMap::<String, Vec<String>>::new();
    for (fg, bg) in accessibility::text_pairs() {
        groups.entry(fg).or_default().push(bg);
    }
    // Repair accents before text drawn on those accents.
    for semantic in [true, false] {
        for (fg, bgs) in &groups {
            if accessibility::TEXT_ROLES.contains(&fg.as_str()) != semantic {
                continue;
            }
            let backgrounds = bgs
                .iter()
                .map(|bg| {
                    colors[bg]
                        .as_str()
                        .ok_or("Invalid background")
                        .map(str::to_owned)
                })
                .collect::<std::result::Result<Vec<_>, _>>()?;
            let repaired = colour::readable(
                colors[fg].as_str().ok_or("Invalid foreground")?,
                &backgrounds,
                4.5,
            )?;
            colors.insert(fg.clone(), json!(repaired));
        }
    }
    for (target, source) in [
        ("text", "onSurface"),
        ("subtext0", "onSurfaceVariant"),
        ("subtext1", "onSurfaceVariant"),
        ("onBackground", "onSurface"),
    ] {
        colors.insert(target.into(), colors[source].clone());
    }
    orient_core::palette::validate(&colors)?;
    Ok(colors)
}

fn filename<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    let name = value[key].as_str().ok_or("Missing bundle asset")?;
    if Path::new(name).components().count() != 1 || name.starts_with('.') || name.contains('\\') {
        return Err("Invalid bundle asset path".into());
    }
    Ok(name)
}
/// Integrity checks precede browsing or applying; a replaced asset cannot reach Apply.
pub fn catalog(assets: &Path) -> Result<Value> {
    let root = assets.join("theme-bundles");
    if !root.join("catalog.json").exists() {
        return Ok(json!([]));
    }
    let mut document: Value = serde_json::from_slice(&fs::read(root.join("catalog.json"))?)?;
    if document["schemaVersion"] != 1 || document["mappingVersion"] != 1 {
        return Err("Unsupported theme bundle schema".into());
    }
    let bundles = document["bundles"]
        .as_array_mut()
        .ok_or("Invalid theme bundles")?;
    let mut ids = BTreeSet::new();
    for bundle in bundles.iter_mut() {
        if !bundle["id"]
            .as_str()
            .is_some_and(|s| s.starts_with("bundle:") && ids.insert(s.to_owned()))
            || bundle["upstream"]["license"] != "MIT"
        {
            return Err("Invalid theme bundle provenance".into());
        }
        for mode in ["dark", "light"] {
            let colors = bundle["modes"][mode]
                .as_object()
                .ok_or("Incomplete bundle modes")?;
            orient_core::palette::validate(colors)?;
        }
        let wallpapers = bundle["wallpapers"]
            .as_array_mut()
            .ok_or("Missing bundle wallpapers")?;
        if wallpapers.len() != 2 {
            return Err("Both wallpaper modes are required".into());
        }
        for item in wallpapers {
            if item["license"] != "CC0-1.0" {
                return Err("Unreviewed wallpaper license".into());
            }
            for (field, hash) in [("file", "sha256"), ("sourceFile", "sourceSha256")] {
                let path = root.join(filename(item, field)?);
                if item[hash] != format!("{:x}", Sha256::digest(fs::read(&path)?)) {
                    return Err("Theme wallpaper checksum failed".into());
                }
                if field == "file" {
                    let (width, height) = image::ImageReader::open(&path)?
                        .with_guessed_format()?
                        .into_dimensions()?;
                    if item["size"] != json!([width, height]) {
                        return Err("Theme wallpaper dimensions do not match provenance".into());
                    }
                    item["path"] = json!(path);
                }
            }
        }
        bundle["bundle"] = json!(true);
        bundle["group"] = json!("bundles");
        bundle["seed"] = bundle["modes"]["dark"]["primary"].clone();
        bundle["surface"] = bundle["modes"]["dark"]["frame"].clone();
        bundle["swatches"] = json!([
            bundle["modes"]["dark"]["primary"],
            bundle["modes"]["dark"]["secondary"],
            bundle["modes"]["light"]["primary"]
        ]);
        bundle["credit"]=json!(format!("{} · MIT palette; Nacre Orbits wallpapers · CC0. Orient role adaptation, not pixel-identical rendering.",bundle["upstream"]["credit"].as_str().unwrap_or("Upstream")));
    }
    Ok(Value::Array(bundles.clone()))
}

#[derive(Clone)]
enum Saved {
    Absent,
    File(Vec<u8>),
    Link(PathBuf),
}
fn save(path: &Path) -> Result<Saved> {
    Ok(if path.is_symlink() {
        Saved::Link(fs::read_link(path)?)
    } else if path.is_file() {
        Saved::File(fs::read(path)?)
    } else if path.exists() {
        return Err("Bundle output is not a file".into());
    } else {
        Saved::Absent
    })
}
fn restore(path: &Path, value: &Saved) -> Result<()> {
    match value {
        Saved::Absent => match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        },
        Saved::File(bytes) => files::atomic(path, bytes),
        Saved::Link(target) => classic_state::atomic_symlink(path, target),
    }
}

/// The injected publisher is a fault-test boundary; production uses the same native publisher.
pub fn apply_with(
    publisher: &mut Publisher,
    id: &str,
    mode: &str,
    scope: &str,
    live: bool,
    mut publish: impl FnMut(
        &mut Publisher,
        Option<&Path>,
        bool,
        &classic_state::PublicationGuard,
    ) -> Result<()>,
) -> Result<Value> {
    if !["dark", "light"].contains(&mode) || !["palette", "wallpaper", "both"].contains(&scope) {
        return Err("Choose a mode and palette, wallpaper or both".into());
    }
    let bundles = catalog(&publisher.assets)?;
    let bundle = bundles
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["id"] == id)
        .ok_or("Unknown theme bundle")?;
    let wallpaper = bundle["wallpapers"]
        .as_array()
        .unwrap()
        .iter()
        .find(|w| w["mode"] == mode)
        .and_then(|w| w["path"].as_str())
        .map(PathBuf::from)
        .ok_or("Missing wallpaper mode")?;
    let paths = publisher.paths.clone();
    let guard = classic_state::publication_lock(&paths)?;
    let _preference_guard = files::lock(&paths.config.join("wallpaper-picker.lock"))?;
    // Strict registry reads refuse future versions instead of overwriting them.
    let legacy_prefs = state::read_strict(&paths.config.join("wallpaper-picker.json"), json!({}))?;
    let mut preference_document = crate::owned_settings::Document::open(
        &paths.config.join("wallpaper-picker.json"),
        "wallpaper-picker",
        legacy_prefs.clone(),
    )?;
    let original_preferences = preference_document.data.clone();
    let mut prefs = if preference_document.is_hosted() {
        original_preferences.clone()
    } else {
        legacy_prefs
    };
    let previous = state::read_strict(&paths.state.join("scheme.json"), json!({}))?;
    let old_image = fs::read_to_string(paths.state.join("wallpaper/last.txt"))
        .ok()
        .map(|s| PathBuf::from(s.trim()));
    let mut outputs = vec![
        paths.config.join("wallpaper-picker.json"),
        paths.state.join("scheme.json"),
        paths.state.join("presentation.json"),
        paths.state.join("theme-targets.json"),
    ];
    for name in [
        "last.txt",
        "path.txt",
        "current",
        "thumbnail.jpg",
        "media.json",
    ] {
        outputs.push(paths.state.join("wallpaper").join(name));
    }
    // Include all declared app exports, retaining exact previous bytes on failed publication.
    let targets: Value =
        serde_json::from_slice(&fs::read(publisher.assets.join("theme-targets.json"))?)?;
    for target in targets["targets"]
        .as_array()
        .ok_or("Invalid app export targets")?
    {
        for name in target["outputs"]
            .as_array()
            .ok_or("Invalid app export paths")?
        {
            let name = Path::new(name.as_str().ok_or("Invalid export path")?);
            if name.is_absolute()
                || name
                    .components()
                    .any(|p| matches!(p, std::path::Component::ParentDir))
            {
                return Err("Invalid export path".into());
            }
            let destination = if let Ok(relative) = name.strip_prefix(".config") {
                paths
                    .config
                    .parent()
                    .ok_or("Missing config root")?
                    .join(relative)
            } else if let Ok(relative) = name.strip_prefix(".local/state/nacre") {
                paths.state.join(relative)
            } else {
                paths.home.join(name)
            };
            outputs.push(destination);
        }
    }
    for name in [
        "gtk-3.0/settings.ini",
        "gtk-4.0/settings.ini",
        "qt5ct/qt5ct.conf",
        "qt6ct/qt6ct.conf",
    ] {
        outputs.push(
            paths
                .config
                .parent()
                .ok_or("Missing config root")?
                .join(name),
        );
    }
    for name in [
        "primary",
        "secondary",
        "onsurface",
        "onprimary",
        "surface",
        "surfacecontainer",
    ] {
        outputs.push(paths.config.join("colors").join(name));
    }
    outputs.push(paths.home.join(".local/share/color-schemes/Nacre.colors"));
    outputs.extend(
        crate::branding::asset_names()
            .into_iter()
            .map(|(_, name)| paths.share.join("branding").join(name)),
    );
    for name in ["nacre.png", "nacre-lock.png", "nacre-text.txt"] {
        outputs.push(paths.share.join("branding").join(name));
    }
    for app in crate::branding::APPLICATIONS {
        outputs.push(
            paths
                .share
                .join("branding")
                .join(format!("nacre-{app}.png")),
        );
    }
    for name in ["scheme/current-mode.txt", "scheme/current.txt"] {
        outputs.push(paths.state.join(name));
    }
    outputs.push(publisher.backend.config_path("palette")?);
    outputs.sort();
    outputs.dedup();
    let saved = outputs
        .iter()
        .map(|path| save(path).map(|value| (path.clone(), value)))
        .collect::<Result<Vec<_>>>()?;
    let result = (|| -> Result<()> {
        if scope != "wallpaper" {
            prefs["palettePreset"] = json!(id);
            prefs["paletteMode"] = json!(mode);
            prefs["paletteAutoMode"] = json!(false);
        }
        if scope != "wallpaper" && !preference_document.save(&prefs)? {
            state::write(&paths.config.join("wallpaper-picker.json"), &prefs)?;
        }
        let mut scheme = if scope == "wallpaper" {
            previous.clone()
        } else {
            json!({"name":"dynamic","mode":mode,"variant":"tonalspot","flavour":"default","colours":bundle["modes"][mode],"engine":crate::orient::engine_id()})
        };
        if scope == "wallpaper" && scheme["colours"].is_null() {
            scheme = orient_core::engine::default_palette(&json!({"mode":mode}))?;
        }
        state::write(&paths.state.join("scheme.json"), &scheme)?;
        if scope != "palette" {
            let media = json!({"path":wallpaper,"poster":wallpaper,"preview":wallpaper,"thumbnail":wallpaper,"name":bundle["name"],"kind":"static","dynamic":false,"animated":false,"appliedAtMs":chrono::Utc::now().timestamp_millis()});
            state::write(&paths.state.join("wallpaper/media.json"), &media)?;
            classic_state::atomic_write(
                &paths.state.join("wallpaper/path.txt"),
                wallpaper.to_str().ok_or("Invalid wallpaper path")?,
            )?;
            classic_state::atomic_symlink(&paths.state.join("wallpaper/current"), &wallpaper)?;
            classic_state::atomic_symlink(
                &paths.state.join("wallpaper/thumbnail.jpg"),
                &wallpaper,
            )?;
        }
        publish(
            publisher,
            if scope == "palette" {
                old_image.as_deref()
            } else {
                Some(&wallpaper)
            },
            live,
            &guard,
        )
    })();
    if let Err(error) = result {
        if preference_document.is_hosted() {
            preference_document.save(&original_preferences)?;
        }
        for (path, value) in &saved {
            if preference_document.is_hosted()
                && path == &paths.config.join("wallpaper-picker.json")
            {
                continue;
            }
            restore(path, value)?;
        }
        // Restore live consumers too; if unavailable, exact persisted selections still survive.
        let recovery = if previous["colours"].is_object() {
            publish(publisher, old_image.as_deref(), live, &guard).err()
        } else {
            None
        };
        for (path, value) in &saved {
            if preference_document.is_hosted()
                && path == &paths.config.join("wallpaper-picker.json")
            {
                continue;
            }
            restore(path, value)?;
        }
        return Err(format!(
            "Theme was not applied; previous selection restored: {error}{}",
            recovery
                .map(|e| format!("; live recovery: {e}"))
                .unwrap_or_default()
        )
        .into());
    }
    Ok(json!({"message":"Theme bundle applied","bundle":id,"mode":mode,"scope":scope}))
}
pub fn apply(id: &str, input: &Value) -> Result<Value> {
    apply_with(
        &mut Publisher::environment()?,
        id,
        input["mode"].as_str().unwrap_or("dark"),
        input["scope"].as_str().unwrap_or("both"),
        true,
        |p, w, live, guard| p.apply_palette_held(w, live, guard),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::Paths;
    fn assets() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools")
    }
    fn publisher(paths: &Paths) -> Result<Publisher> {
        Ok(Publisher {
            paths: paths.clone(),
            assets: assets(),
            backend: crate::compositor::backend::Backend::new(
                paths.clone(),
                [("NACRE_COMPOSITOR".into(), "hyprland".into())]
                    .into_iter()
                    .collect(),
            )?,
            system_icons: paths.home.join("absent-icons"),
            monitors: Some(vec![]),
            now_ms: Some(1000),
        })
    }
    #[test]
    fn pilots_have_complete_reproducible_readable_roles_and_verified_art() -> Result<()> {
        let bundles = catalog(&assets())?;
        assert_eq!(bundles.as_array().unwrap().len(), 2);
        for bundle in bundles.as_array().unwrap() {
            for mode in ["dark", "light"] {
                let colors = bundle["modes"][mode].as_object().unwrap();
                assert_eq!(colors, &map_palette(&bundle["sourceColors"][mode], mode)?);
                orient_core::palette::validate(colors)?;
                assert!(
                    colour::contrast(
                        colors["onPrimary"].as_str().unwrap(),
                        colors["primary"].as_str().unwrap()
                    )? >= 4.5
                );
                assert!(
                    colour::contrast(
                        colors["onSurfaceVariant"].as_str().unwrap(),
                        colors["surfaceContainer"].as_str().unwrap()
                    )? >= 4.5
                );
                let temp = tempfile::tempdir()?;
                let paths = Paths::new(temp.path());
                classic_state::publish_terminal_colors(
                    &paths,
                    &assets(),
                    colors,
                    &json!({"mode":mode}),
                )?;
                let terminal = fs::read_to_string(paths.config.join("kitty-colors.conf"))?;
                assert_eq!(
                    terminal
                        .lines()
                        .filter(|line| line.starts_with("color"))
                        .count(),
                    16
                );
                classic_state::publish_application_colors(
                    &paths,
                    &assets(),
                    &paths.home.join("absent"),
                    colors,
                    &json!({"mode":mode}),
                    false,
                    None,
                )?;
                let qt = fs::read_to_string(paths.config.join("qt.conf"))?;
                for state in ["active_colors=", "inactive_colors=", "disabled_colors="] {
                    assert!(qt.contains(state));
                }
                assert!(paths.home.join(".config/gtk-3.0/gtk.css").is_file());
                assert!(paths.home.join(".config/kdeglobals").is_file());
            }
        }
        Ok(())
    }
    #[test]
    fn all_scopes_preserve_other_choices_and_failure_restores_exact_inputs() -> Result<()> {
        for scope in ["palette", "wallpaper", "both"] {
            let temp = tempfile::tempdir()?;
            let paths = Paths::new(temp.path());
            let prefs = paths.config.join("wallpaper-picker.json");
            state::write(
                &prefs,
                &json!({"palettePreset":"wallpaper","paletteMode":"dark","paletteAutoMode":true,"rotationMinutes":47,"collection":"preserved"}),
            )?;
            let previous = orient_core::engine::default_palette(&json!({"mode":"dark"}))?;
            state::write(&paths.state.join("scheme.json"), &previous)?;
            let poster = paths.home.join("old.png");
            fs::write(&poster, b"original")?;
            classic_state::atomic_write(
                &paths.state.join("wallpaper/last.txt"),
                poster.to_str().unwrap(),
            )?;
            classic_state::atomic_symlink(&paths.state.join("wallpaper/current"), &poster)?;
            let old = fs::read(&prefs)?;
            let failure = apply_with(
                &mut publisher(&paths)?,
                "bundle:catppuccin",
                "light",
                scope,
                false,
                |p, _, _, _| {
                    classic_state::atomic_write(&p.paths.config.join("qt.conf"), "partial export")?;
                    Err("fixture activation failure".into())
                },
            );
            assert!(failure.is_err());
            assert_eq!(fs::read(&prefs)?, old);
            assert_eq!(
                fs::read_link(paths.state.join("wallpaper/current"))?,
                poster
            );
            assert!(!paths.config.join("qt.conf").exists());
            apply_with(
                &mut publisher(&paths)?,
                "bundle:catppuccin",
                "light",
                scope,
                false,
                |_, _, _, _| Ok(()),
            )?;
            let applied = state::read_strict(&prefs, json!({}))?;
            assert_eq!(applied["rotationMinutes"], 47);
            assert_eq!(applied["collection"], "preserved");
            if scope == "wallpaper" {
                assert_eq!(fs::read(&prefs)?, old);
                assert_eq!(
                    state::read(&paths.state.join("scheme.json"), json!({}))?,
                    previous
                );
            } else {
                assert_eq!(applied["palettePreset"], "bundle:catppuccin");
                assert_eq!(applied["paletteMode"], "light");
                assert_eq!(applied["paletteAutoMode"], false);
            }
            if scope == "palette" {
                assert_eq!(
                    fs::read_link(paths.state.join("wallpaper/current"))?,
                    poster
                );
            } else {
                assert_ne!(
                    fs::read_link(paths.state.join("wallpaper/current"))?,
                    poster
                );
            }
        }
        Ok(())
    }
    #[test]
    fn corrupted_art_and_future_preferences_are_rejected_before_publish() -> Result<()> {
        let temp = tempfile::tempdir()?;
        let paths = Paths::new(temp.path());
        fs::create_dir_all(&paths.config)?;
        fs::write(
            paths.config.join("wallpaper-picker.json"),
            "{\"version\":999,\"keep\":true}",
        )?;
        let before = fs::read(paths.config.join("wallpaper-picker.json"))?;
        assert!(apply_with(
            &mut publisher(&paths)?,
            "bundle:rose-pine",
            "dark",
            "both",
            false,
            |_, _, _, _| panic!("must not publish")
        )
        .is_err());
        assert_eq!(
            fs::read(paths.config.join("wallpaper-picker.json"))?,
            before
        );
        let folder = temp.path().join("assets/theme-bundles");
        fs::create_dir_all(&folder)?;
        fs::copy(
            assets().join("theme-bundles/catalog.json"),
            folder.join("catalog.json"),
        )?;
        fs::write(folder.join("catppuccin-dark.png"), b"replaced")?;
        assert!(catalog(folder.parent().unwrap()).is_err());
        Ok(())
    }
}
