//! Publish existing KDE roles, preserving unrelated ordered INI settings.
use crate::{files, paths::Paths, Result};
use serde_json::Value;
use std::{fs, io::Read, path::Path};

type Entries = Vec<(String, String)>;
#[derive(Default)]
struct Ini {
    defaults: Entries,
    sections: Vec<(String, Entries)>,
}
fn set(entries: &mut Entries, key: &str, value: String) {
    if let Some((_, old)) = entries.iter_mut().find(|(name, _)| name == key) {
        *old = value;
    } else {
        entries.push((key.to_owned(), value));
    }
}
impl Ini {
    fn section(&mut self, name: &str) -> &mut Entries {
        let index = self.sections.iter().position(|(key, _)| key == name);
        let index = index.unwrap_or_else(|| {
            self.sections.push((name.to_owned(), Vec::new()));
            self.sections.len() - 1
        });
        &mut self.sections[index].1
    }
    fn parse(text: &str) -> Result<Self> {
        let mut result = Self::default();
        let mut section: Option<String> = None;
        let mut previous: Option<(String, usize)> = None;
        for line in text.lines() {
            let value = line.trim();
            if value.is_empty() || value.starts_with(['#', ';']) {
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            if let Some((key, depth)) = &previous {
                if indent > *depth {
                    let entries = if section.as_deref() == Some("DEFAULT") {
                        &mut result.defaults
                    } else {
                        result.section(section.as_deref().ok_or("MissingSectionHeaderError")?)
                    };
                    let (_, old) = entries
                        .iter_mut()
                        .find(|(name, _)| name == key)
                        .ok_or("ParsingError")?;
                    old.push('\n');
                    old.push_str(value);
                    continue;
                }
            }
            if value.starts_with('[') {
                let end = value.rfind(']').ok_or("ParsingError")?;
                let name = &value[1..end];
                section = Some(name.to_owned());
                if name != "DEFAULT" {
                    result.section(name);
                }
                previous = None;
                continue;
            }
            let name = section.as_deref().ok_or("MissingSectionHeaderError")?;
            let split = value.find(['=', ':']).ok_or("ParsingError")?;
            let key = value[..split].trim();
            if key.is_empty() {
                return Err("ParsingError".into());
            }
            let entries = if name == "DEFAULT" {
                &mut result.defaults
            } else {
                result.section(name)
            };
            set(entries, key, value[split + 1..].trim().to_owned());
            previous = Some((key.to_owned(), indent));
        }
        Ok(result)
    }
    fn write(&self) -> String {
        fn section(out: &mut String, name: &str, entries: &Entries) {
            out.push_str(&format!("[{name}]\n"));
            for (key, value) in entries {
                out.push_str(&format!("{key}={}\n", value.replace('\n', "\n\t")));
            }
            out.push('\n');
        }
        let mut text = String::new();
        if !self.defaults.is_empty() {
            section(&mut text, "DEFAULT", &self.defaults);
        }
        for (name, entries) in &self.sections {
            section(&mut text, name, entries);
        }
        text
    }
}
fn read_config(path: &Path) -> Result<Ini> {
    let text = match fs::read_to_string(path) {
        Ok(value) => value,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(error) => return Err(error.into()),
    };
    Ini::parse(&text)
}
pub fn read(path: &Path) -> Result<Value> {
    let config = read_config(path)?;
    let mut result = serde_json::Map::new();
    for (name, entries) in config.sections {
        let mut values = serde_json::Map::new();
        for (key, value) in config.defaults.iter().chain(&entries) {
            values.insert(key.clone(), Value::String(value.clone()));
        }
        result.insert(name, Value::Object(values));
    }
    Ok(Value::Object(result))
}
pub fn save(path: &Path, text: &str) -> Result<()> {
    files::atomic(path, text.as_bytes())
}
pub fn publish(paths: &Paths, colors: &Value, icon_theme: &str) -> Result<()> {
    let config_root = paths.config.parent().ok_or("Invalid config path")?;
    let target = config_root.join("kdeglobals");
    let mut config = read_config(&target)?;
    let primary = colors["primary"].as_str().ok_or("Missing primary color")?;
    for (name, background, foreground) in [
        ("Window", "surfaceContainer", "onSurface"),
        ("View", "surface", "onSurface"),
        ("Button", "surfaceContainerHigh", "onSurface"),
        ("Selection", "primary", "onPrimary"),
        ("Tooltip", "surfaceContainerHigh", "onSurface"),
        ("Complementary", "surfaceContainerLow", "onSurface"),
        ("Header", "surfaceContainer", "onSurface"),
        ("Header][Inactive", "surfaceContainer", "onSurface"),
    ] {
        let entries = config.section(&format!("Colors:{name}"));
        for (key, mut role) in [
            ("BackgroundNormal", background),
            ("BackgroundAlternate", "surfaceContainerLow"),
            ("ForegroundNormal", foreground),
            ("ForegroundInactive", "onSurfaceVariant"),
            ("ForegroundActive", foreground),
            ("ForegroundLink", "primary"),
            ("ForegroundVisited", "tertiary"),
            ("ForegroundNegative", "error"),
            ("ForegroundPositive", "success"),
            ("ForegroundNeutral", "secondary"),
            ("DecorationFocus", "primary"),
            ("DecorationHover", "secondary"),
        ] {
            if name == "Selection" {
                if key == "BackgroundAlternate" {
                    role = "primary";
                }
                if key.starts_with("Foreground") {
                    role = "onPrimary";
                }
            }
            let color = colors.get(role).and_then(Value::as_str).unwrap_or(primary);
            set(entries, key, format!("#{}", color.trim_start_matches('#')));
        }
    }
    set(config.section("General"), "ColorScheme", "Nacre".into());
    set(config.section("General"), "ColorSchemeHash", String::new());
    config.section("Icons");
    if !icon_theme.is_empty() {
        set(config.section("Icons"), "Theme", icon_theme.into());
    }
    files::atomic(&target, config.write().as_bytes())?;
    let mut scheme = Ini::default();
    for (name, entries) in &config.sections {
        if name.starts_with("Colors:") {
            // ConfigParser section iteration appends inherited keys after local keys.
            let target = scheme.section(name);
            for (key, value) in entries {
                set(target, key, value.clone());
            }
            for (key, value) in &config.defaults {
                if !target.iter().any(|(name, _)| name == key) {
                    set(target, key, value.clone());
                }
            }
        }
    }
    set(scheme.section("General"), "Name", "Nacre".into());
    set(scheme.section("General"), "ColorScheme", "Nacre".into());
    files::atomic(
        &paths.home.join(".local/share/color-schemes/Nacre.colors"),
        scheme.write().as_bytes(),
    )
}
pub fn run(arguments: &[String]) -> Result<i32> {
    // The former module had no command-line entry point: no arguments is a no-op.
    if arguments != ["--publish"] {
        return Ok(0);
    }
    let mut text = String::new();
    std::io::stdin()
        .take(16 * 1024 * 1024 + 1)
        .read_to_string(&mut text)?;
    if text.len() > 16 * 1024 * 1024 {
        return Err("KDE palette input is too large".into());
    }
    let request: Value = serde_json::from_str(&text)?;
    let paths = if let Some(home) = request["home"].as_str() {
        if std::env::var_os("HOME")
            .is_some_and(|current| std::path::Path::new(&current) == std::path::Path::new(home))
        {
            Paths::environment()?
        } else {
            Paths::new(home)
        }
    } else {
        Paths::environment()?
    };
    publish(
        &paths,
        &request["colors"],
        request["icon_theme"].as_str().unwrap_or(""),
    )?;
    Ok(0)
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn frozen_python_kde_files_modes_and_errors() -> Result<()> {
        let fixture: Value =
            serde_json::from_str(include_str!("../tests/fixtures/kde-palette-reference.json"))?;
        for case in fixture["cases"].as_array().ok_or("Missing cases")? {
            let home = tempfile::tempdir()?;
            let seed = case["seed"].as_str().ok_or("Missing seed")?;
            if !seed.is_empty() {
                fs::create_dir(home.path().join(".config"))?;
                fs::write(home.path().join(".config/kdeglobals"), seed)?;
            }
            let result = publish(
                &Paths::new(home.path()),
                &case["colors"],
                case["icon"].as_str().ok_or("Missing icon")?,
            );
            assert_eq!(
                result.as_ref().err().map(ToString::to_string),
                case["error"].as_str().map(str::to_owned)
            );
            let file_count = walkdir::WalkDir::new(home.path())
                .into_iter()
                .collect::<std::result::Result<Vec<_>, _>>()?
                .iter()
                .filter(|entry| entry.file_type().is_file())
                .count();
            assert_eq!(
                file_count,
                case["files"].as_object().ok_or("Missing files")?.len()
            );
            for (name, expected) in case["files"].as_object().ok_or("Missing files")? {
                let file = home.path().join(name);
                assert_eq!(
                    fs::read_to_string(&file)?,
                    expected["text"],
                    "{name}, seed={seed:?}"
                );
                assert_eq!(
                    fs::metadata(file)?.permissions().mode() & 0o777,
                    expected["mode"].as_u64().ok_or("Missing mode")? as u32
                );
            }
        }
        Ok(())
    }
}
