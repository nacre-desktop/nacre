//! Versioned persistent JSON. Legacy reads are compatible; migration is on save.
use crate::{files, json, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    fs,
    io::{BufRead, Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
    sync::OnceLock,
};

const LIMIT: u64 = 16 * 1024 * 1024;
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    pub id: String,
    pub paths: Vec<String>,
    pub shape: String,
    pub version: u64,
    pub keep_version: bool,
    pub defaults: Value,
    pub settings: Option<SettingsAccess>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct SettingsAccess {
    pub root: SettingsRoot,
    pub access: SettingsMode,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SettingsRoot {
    Config,
    State,
}
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SettingsMode {
    Read,
    Write,
}
#[derive(Deserialize)]
struct Registry {
    files: Vec<Schema>,
}
pub fn schemas() -> &'static [Schema] {
    static REGISTRY: OnceLock<Registry> = OnceLock::new();
    &REGISTRY
        .get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../../nacre/shell-tools/nacre-state-files.json"
            ))
            .expect("valid checked-in state registry")
        })
        .files
}
pub fn by_id(id: &str) -> Result<&'static Schema> {
    schemas()
        .iter()
        .find(|schema| schema.id == id)
        .ok_or_else(|| "Unknown state schema".into())
}
pub fn schema(path: &Path) -> Option<&'static Schema> {
    static MATCHERS: OnceLock<Vec<(usize, usize, regex::Regex)>> = OnceLock::new();
    let matchers = MATCHERS.get_or_init(|| {
        schemas()
            .iter()
            .enumerate()
            .flat_map(|(index, schema)| {
                schema.paths.iter().map(move |pattern| {
                    let expression =
                        format!("(?:^|/){}$", regex::escape(pattern).replace("\\*", ".*"));
                    (
                        index,
                        pattern.len(),
                        regex::Regex::new(&expression).expect("valid state pattern"),
                    )
                })
            })
            .collect()
    });
    let path = path.to_string_lossy();
    matchers
        .iter()
        .filter(|(_, _, expression)| expression.is_match(&path))
        .max_by_key(|(_, length, _)| length)
        .map(|(index, _, _)| &schemas()[*index])
}

fn regular(path: &Path) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(meta) if !meta.is_file() => Err("Refusing linked or non-regular state file".into()),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}
fn bytes(path: &Path) -> Result<Vec<u8>> {
    regular(path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(LIMIT + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > LIMIT {
        return Err("State file too large; original preserved".into());
    }
    Ok(bytes)
}
#[derive(Debug)]
pub enum StateError {
    Corrupt,
    Unsupported,
    Newer,
}
impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Corrupt => "Invalid state document",
            Self::Unsupported => "Unsupported state version; original file preserved",
            Self::Newer => "Newer state version; original file preserved",
        })
    }
}
impl std::error::Error for StateError {}
fn decode(value: Value, schema: &Schema) -> std::result::Result<(Value, u64), StateError> {
    let version = match value.get("version") {
        None => 0,
        Some(value)
            if schema.keep_version
                && (value == true || value.as_f64() == Some(1.0) && value.as_u64().is_none()) =>
        {
            0
        }
        Some(value) => value.as_u64().ok_or(StateError::Unsupported)?,
    };
    if version > schema.version {
        return Err(StateError::Newer);
    }
    let mut data = if schema.shape == "array" {
        if version == 0 {
            value
        } else {
            value["items"].clone()
        }
    } else {
        value
    };
    if schema.shape == "array" && !data.is_array() || schema.shape != "array" && !data.is_object() {
        return Err(StateError::Corrupt);
    }
    if !schema.keep_version {
        if let Some(object) = data.as_object_mut() {
            object.remove("version");
        }
    }
    if schema.keep_version && data.is_object() && version < schema.version {
        data["version"] = json!(schema.version);
    }
    if schema.id == "desktop" && version < 2 {
        let aliases: [(&str, &[&str]); 5] = [
            ("topEdge", &["frameTopEnabled", "topBarEnabled"]),
            ("leftEdge", &["frameLeftEnabled"]),
            ("rightEdge", &["frameRightEnabled", "controlCenterEnabled"]),
            ("bottomEdge", &["frameBottomEnabled"]),
            ("leftDrawer", &["aiSidebarEnabled"]),
        ];
        fn migrate(object: &mut serde_json::Map<String, Value>, aliases: &[(&str, &[&str])]) {
            for (old, keys) in aliases {
                if let Some(value) = object.remove(*old) {
                    for key in *keys {
                        object
                            .entry((*key).to_owned())
                            .or_insert_with(|| value.clone());
                    }
                }
            }
        }
        let object = data.as_object_mut().unwrap();
        migrate(object, &aliases);
        if let Some(snapshot) = object
            .get_mut("normalSnapshot")
            .and_then(Value::as_object_mut)
        {
            migrate(snapshot, &aliases);
        }
        object
            .entry("dashboardEnabled".to_owned())
            .or_insert(json!(true));
    }
    if schema.id == "control-center" && version < 2 {
        if data.get("tiles").is_none() {
            let mut tiles = schema.defaults["tiles"].as_array().unwrap().clone();
            if let Some(optional) = data["optionalTiles"].as_array() {
                tiles.extend(optional.clone());
            }
            data["tiles"] = Value::Array(tiles);
        }
        data.as_object_mut().unwrap().remove("optionalTiles");
        data["version"] = json!(2);
    }
    Ok((data, version))
}
fn encoded(value: &Value, schema: &Schema) -> Result<Vec<u8>> {
    if value
        .get("version")
        .and_then(Value::as_u64)
        .is_some_and(|version| version > schema.version)
    {
        return Err(StateError::Newer.into());
    }
    let mut value = value.clone();
    if schema.shape == "array" {
        if !value.is_array() {
            return Err(StateError::Corrupt.into());
        }
        value = json!({"version":schema.version,"items":value});
    } else {
        value
            .as_object_mut()
            .ok_or(StateError::Corrupt)?
            .insert("version".into(), json!(schema.version));
    }
    Ok(json::encode(&value)?.into_bytes())
}
pub fn read(path: &Path, fallback: Value) -> Result<Value> {
    read_as(path, schema(path), fallback)
}
pub fn read_strict(path: &Path, fallback: Value) -> Result<Value> {
    read_internal(path, schema(path), fallback, false)
}
pub fn read_as(path: &Path, schema: Option<&Schema>, fallback: Value) -> Result<Value> {
    read_internal(path, schema, fallback, true)
}
fn read_internal(
    path: &Path,
    schema: Option<&Schema>,
    fallback: Value,
    recover: bool,
) -> Result<Value> {
    let raw = match bytes(path) {
        Ok(raw) => raw,
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            return Ok(fallback)
        }
        Err(error) => return Err(error),
    };
    let Some(schema) = schema else {
        return Ok(serde_json::from_slice(&raw)?);
    };
    match serde_json::from_slice(&raw)
        .map_err(|_| StateError::Corrupt)
        .and_then(|value| decode(value, schema))
    {
        Ok((value, _)) => Ok(value),
        Err(StateError::Corrupt) if recover => Ok(fallback),
        Err(error) => Err(error.into()),
    }
}
fn backup(path: &Path, raw: &[u8], suffix: &str) -> Result<()> {
    let backup = path.with_file_name(format!(
        "{}{suffix}.bak",
        path.file_name()
            .ok_or("Invalid state path")?
            .to_string_lossy()
    ));
    regular(&backup)?;
    let parent = path.parent().ok_or("Invalid state path")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(raw)?;
    temporary.as_file().sync_all()?;
    match temporary.persist_noclobber(&backup) {
        Ok(_) => {}
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => return Err(error.into()),
    }
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
pub fn write(path: &Path, value: &Value) -> Result<()> {
    write_as(path, schema(path), value)
}
pub fn write_as(path: &Path, schema: Option<&Schema>, value: &Value) -> Result<()> {
    let Some(schema) = schema else {
        return files::atomic(path, json::encode(value)?.as_bytes());
    };
    // Reject a caller-supplied future version before creating a lock/backup.
    encoded(value, schema)?;
    update_as(path, schema, |_| Ok(value.clone()))
}
pub fn update_as(
    path: &Path,
    schema: &Schema,
    change: impl FnOnce(Value) -> Result<Value>,
) -> Result<()> {
    let parent = path.parent().ok_or("Invalid state path")?;
    fs::create_dir_all(parent)?;
    let lock = path.with_file_name(format!(
        "{}.schema.lock",
        path.file_name()
            .ok_or("Invalid state path")?
            .to_string_lossy()
    ));
    use nix::fcntl::{Flock, FlockArg};
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32)
        .open(lock)?;
    let _guard = Flock::lock(file, FlockArg::LockExclusive).map_err(|(_, error)| error)?;
    let raw = match bytes(path) {
        Ok(raw) => Some(raw),
        Err(error)
            if error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|error| error.kind() == std::io::ErrorKind::NotFound) =>
        {
            None
        }
        Err(error) => return Err(error),
    };
    let (current, old, corrupt) = match raw.as_ref() {
        Some(raw) => match serde_json::from_slice(raw)
            .map_err(|_| StateError::Corrupt)
            .and_then(|value| decode(value, schema))
        {
            Ok((value, version)) => (value, version, false),
            Err(StateError::Corrupt) => (schema.defaults.clone(), schema.version, true),
            Err(error) => return Err(error.into()),
        },
        None => (schema.defaults.clone(), schema.version, false),
    };
    let value = change(current)?;
    let encoded = encoded(&value, schema)?;
    if encoded.len() as u64 > LIMIT {
        return Err("State file too large; original preserved".into());
    }
    if let Some(raw) = raw.as_ref() {
        if corrupt {
            backup(path, raw, ".corrupt")?;
        } else if old < schema.version {
            backup(path, raw, &format!(".pre-v{old}-to-v{}", schema.version))?;
        }
    }
    files::atomic(path, &encoded)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}

/// The Settings UI addresses owned stores by registry ID, never an arbitrary path.
pub fn settings_path(id: &str) -> Result<std::path::PathBuf> {
    let schema = by_id(id)?;
    let access = schema
        .settings
        .as_ref()
        .ok_or("Store is not owned by Settings")?;
    let relative = schema
        .paths
        .first()
        .and_then(|path| path.strip_prefix("nacre/"))
        .ok_or("Invalid Settings store path")?;
    if relative.contains('*') || relative.contains("..") {
        return Err("Invalid Settings store path".into());
    }
    let paths = crate::paths::Paths::environment()?;
    Ok(match access.root {
        SettingsRoot::State => paths.state.join(relative),
        SettingsRoot::Config => paths.config.join(relative),
    })
}

/// Restricted QML publisher: no generic arbitrary-file write surface.
pub fn run(arguments: &[String]) -> Result<i32> {
    let result = (|| -> Result<Value> {
        let [action, key] = arguments else {
            return Err("Expected state action and store ID or path".into());
        };
        if action == "load-id" {
            let schema = by_id(key)?;
            let path = settings_path(key)?;
            let data = read(&path, schema.defaults.clone())?;
            return Ok(json!({"ok":true,"id":key,"data":data,"exists":path.exists()}));
        }
        if matches!(action.as_str(), "save-id" | "set-id")
            && by_id(key)?
                .settings
                .as_ref()
                .is_none_or(|access| access.access != SettingsMode::Write)
        {
            return Err("Publisher store is read-only in Settings".into());
        }
        let path = if action == "save-id" || action == "set-id" {
            settings_path(key)?
        } else if action == "save" {
            let path = Path::new(key);
            if path
                .components()
                .any(|part| matches!(part, std::path::Component::ParentDir))
            {
                return Err("Invalid state path".into());
            }
            let paths = crate::paths::Paths::environment()?;
            if ![
                paths.config.join("control-center.json"),
                paths.config.join("dashboard-extras.json"),
                paths.state.join("battery-alert-state.json"),
            ]
            .contains(&path.to_path_buf())
            {
                return Err("State path is not a QML-owned store".into());
            }
            path.to_path_buf()
        } else {
            return Err("Unknown state action".into());
        };
        let mut input = String::new();
        std::io::stdin()
            .lock()
            .take(LIMIT + 1)
            .read_line(&mut input)?;
        if input.len() as u64 > LIMIT {
            return Err("State request too large".into());
        }
        let value: Value = serde_json::from_str(&input)?;
        if action == "set-id" {
            let keys = value["key"].as_array().ok_or("Expected key path")?;
            let keys: Vec<&str> = keys
                .iter()
                .map(|key| key.as_str().ok_or("Invalid key path"))
                .collect::<std::result::Result<_, _>>()?;
            if keys.is_empty()
                || keys.iter().any(|key| {
                    key.is_empty()
                        || ["__proto__", "constructor", "prototype", "version"].contains(key)
                })
            {
                return Err("Invalid key path".into());
            }
            let next = value.get("value").ok_or("Expected setting value")?.clone();
            update_as(&path, by_id(key)?, |mut data| {
                let mut target = &mut data;
                for part in &keys[..keys.len() - 1] {
                    if target.get(*part).is_none() {
                        target[*part] = json!({});
                    }
                    target = target.get_mut(*part).ok_or("Invalid key path")?;
                    if !target.is_object() {
                        return Err("Invalid key path".into());
                    }
                }
                target[keys[keys.len() - 1]] = next;
                Ok(data)
            })?;
        } else {
            write(&path, &value)?;
        }
        if action == "save-id" || action == "set-id" {
            Ok(
                json!({"ok":true,"id":key,"data":read(&path,by_id(key)?.defaults.clone())?,"exists":true}),
            )
        } else {
            Ok(json!({"ok":true}))
        }
    })();
    match result {
        Ok(value) => {
            println!("{}", json::encode(&value)?);
            Ok(0)
        }
        Err(error) => {
            println!("{}", json::encode(&json!({"error":error.to_string()}))?);
            Ok(1)
        }
    }
}

#[cfg(test)]
pub fn legacy_snapshot(mut files: Value) -> Result<Value> {
    // Frozen references predate version metadata. Compare their payloads without
    // regenerating evidence; dedicated tests below assert the new storage bytes.
    if let Some(entries) = files.as_object_mut() {
        entries.retain(|name, row| {
            let base = name
                .strip_suffix(".schema.lock")
                .or_else(|| name.split_once(".pre-v").map(|(base, _)| base))
                .or_else(|| name.strip_suffix(".corrupt.bak"));
            if base.is_some_and(|base| schema(Path::new(base)).is_some()) {
                assert_eq!(row["mode"], "0o600");
                return false;
            }
            true
        });
        for (name, row) in entries {
            let Some(schema) = schema(Path::new(name)) else {
                continue;
            };
            let (field, raw) = if let Some(text) = row["text"].as_str() {
                ("text", text.as_bytes().to_vec())
            } else if let Some(hex) = row["hex"].as_str() {
                let raw = (0..hex.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&hex[offset..offset + 2], 16))
                    .collect::<std::result::Result<Vec<_>, _>>()?;
                ("hex", raw)
            } else {
                continue;
            };
            let Ok(mut data) = serde_json::from_slice::<Value>(&raw) else {
                continue;
            };
            if data["version"] == schema.version || data["version"] == true && schema.version == 1 {
                if schema.shape == "array" {
                    data = data["items"].clone();
                } else if !schema.keep_version {
                    data.as_object_mut().unwrap().remove("version");
                } else {
                    data["version"] = json!(schema.version);
                }
            }
            let text = json::encode(&data)?;
            row[field] = json!(if field == "hex" {
                text.bytes()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            } else {
                text
            });
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_v2_splits_edges_and_profile_snapshots() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("desktop.json");
        let original = br#"{"version":1,"rightEdge":false,"frameRightEnabled":true,"topEdge":false,"leftDrawer":false,"unknown":"keep","normalSnapshot":{"rightEdge":false,"topEdge":true}}"#;
        fs::write(&path, original)?;
        let data = read_as(&path, Some(by_id("desktop")?), json!({}))?;
        assert_eq!(data["frameRightEnabled"], true);
        assert_eq!(data["controlCenterEnabled"], false);
        assert_eq!(data["topBarEnabled"], false);
        assert_eq!(data["frameTopEnabled"], false);
        assert_eq!(data["aiSidebarEnabled"], false);
        assert_eq!(data["dashboardEnabled"], true);
        assert_eq!(data["unknown"], "keep");
        assert_eq!(data["normalSnapshot"]["controlCenterEnabled"], false);
        assert_eq!(data["normalSnapshot"]["topBarEnabled"], true);
        assert!(data.get("rightEdge").is_none());
        assert_eq!(fs::read(&path)?, original);
        write_as(&path, Some(by_id("desktop")?), &data)?;
        assert_eq!(
            fs::read(path.with_file_name("desktop.json.pre-v1-to-v2.bak"))?,
            original
        );
        Ok(())
    }
    #[test]
    fn atomic_field_updates_preserve_concurrent_fields_and_future_bytes() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("power.json");
        fs::write(&path, br#"{"version":1,"unknown":"kept"}"#)?;
        std::thread::scope(|scope| {
            for index in 0..20 {
                let path = &path;
                scope.spawn(move || {
                    update_as(path, by_id("power").unwrap(), |mut data| {
                        data[format!("field{index}")] = json!(index);
                        Ok(data)
                    })
                    .unwrap()
                });
            }
        });
        let saved: Value = serde_json::from_slice(&fs::read(&path)?)?;
        assert_eq!(saved["unknown"], "kept");
        for index in 0..20 {
            assert_eq!(saved[format!("field{index}")], index);
        }
        let original = br#"{"version":99,"future":"kept"}"#;
        fs::write(&path, original)?;
        assert!(update_as(&path, by_id("power")?, |_| Ok(json!({}))).is_err());
        assert_eq!(fs::read(&path)?, original);
        assert!(settings_path("brain-auth").is_err());
        assert!(settings_path("../desktop").is_err());
        Ok(())
    }

    #[test]
    fn every_schema_migrates_with_exact_private_one_time_backup() -> Result<()> {
        for schema in schemas() {
            let folder = tempfile::tempdir()?;
            let path = folder.path().join("state.json");
            let legacy = if schema.shape == "array" {
                json!([])
            } else {
                json!({"custom":"kept"})
            };
            let original = serde_json::to_vec_pretty(&legacy)?;
            fs::write(&path, &original)?;
            let data = read_as(&path, Some(schema), schema.defaults.clone())?;
            write_as(&path, Some(schema), &data)?;
            let saved: Value = serde_json::from_slice(&fs::read(&path)?)?;
            assert_eq!(saved["version"], schema.version, "{}", schema.id);
            let backup =
                path.with_file_name(format!("state.json.pre-v0-to-v{}.bak", schema.version));
            assert_eq!(fs::read(&backup)?, original, "{}", schema.id);
            assert_eq!(fs::metadata(&backup)?.permissions().mode() & 0o777, 0o600);
            write_as(&path, Some(schema), &data)?;
            assert_eq!(fs::read(&backup)?, original);
        }
        Ok(())
    }
    #[test]
    fn every_schema_refuses_future_bytes_on_reads_and_writes() -> Result<()> {
        for schema in schemas() {
            let folder = tempfile::tempdir()?;
            let path = folder.path().join("state.json");
            let original =
                serde_json::to_vec(&json!({"version":schema.version+1,"future":"keep"}))?;
            fs::write(&path, &original)?;
            assert!(
                read_as(&path, Some(schema), schema.defaults.clone()).is_err(),
                "{}",
                schema.id
            );
            assert!(
                write_as(&path, Some(schema), &schema.defaults).is_err(),
                "{}",
                schema.id
            );
            assert_eq!(fs::read(&path)?, original);
            assert_eq!(
                fs::read_dir(folder.path())?
                    .filter_map(std::result::Result::ok)
                    .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "bak"))
                    .count(),
                0
            );
        }
        Ok(())
    }
    #[test]
    fn every_schema_recovers_corrupt_bytes_after_backup() -> Result<()> {
        for schema in schemas() {
            let folder = tempfile::tempdir()?;
            let path = folder.path().join("state.json");
            let original = b"{not JSON\xff";
            fs::write(&path, original)?;
            assert_eq!(
                read_as(&path, Some(schema), schema.defaults.clone())?,
                schema.defaults
            );
            write_as(&path, Some(schema), &schema.defaults)?;
            assert_eq!(
                fs::read(path.with_file_name("state.json.corrupt.bak"))?,
                original
            );
            let saved: Value = serde_json::from_slice(&fs::read(&path)?)?;
            assert_eq!(saved["version"], schema.version);
        }
        Ok(())
    }
    #[test]
    fn linked_backup_or_destination_prevents_replacement() -> Result<()> {
        let folder = tempfile::tempdir()?;
        let path = folder.path().join("state.json");
        let other = folder.path().join("other");
        fs::write(&path, b"{}")?;
        fs::write(&other, b"keep")?;
        std::os::unix::fs::symlink(&other, path.with_file_name("state.json.pre-v0-to-v2.bak"))?;
        assert!(write_as(&path, Some(by_id("desktop")?), &json!({})).is_err());
        assert_eq!(fs::read(&path)?, b"{}");
        assert_eq!(fs::read(&other)?, b"keep");
        Ok(())
    }
}
