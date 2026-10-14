//! Compatible private panel layout persistence and one-line writer requests.
use crate::{i18n, json, paths::Paths, Result};
use serde_json::{json as value, Value};
use std::{
    fs,
    io::{BufRead, Write},
    os::unix::fs::DirBuilderExt,
    path::{Path, PathBuf},
};
fn version(value: &Value) -> bool {
    value == 1 || value == 1.0 || value == true
}
pub fn load(path: &Path) -> Result<Value> {
    match fs::read_to_string(path) {
        Ok(text) => {
            let value = json::parse_value(&text)?;
            if !value.is_object() || !value.get("version").is_some_and(version) {
                return Err(i18n::tr(
                    "helper.pearl-state.unsupported-panel-layout-version",
                    "Unsupported panel layout version",
                )
                .into());
            }
            Ok(value)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok(value!({"version":1,"windows":[],"ai":{"edge":"","channels":[""]}}))
        }
        Err(error) => Err(error.into()),
    }
}
pub fn save(path: &Path, value: &Value) -> Result<()> {
    if !value.is_object()
        || !value.get("version").is_some_and(version)
        || !value.get("windows").is_some_and(Value::is_array)
    {
        return Err(i18n::tr(
            "helper.pearl-state.invalid-panel-layout",
            "Invalid panel layout",
        )
        .into());
    }
    let parent = path.parent().ok_or("Invalid panel layout path")?;
    if !parent.exists() {
        if let Some(ancestor) = parent.parent() {
            fs::create_dir_all(ancestor)?;
        }
        match fs::DirBuilder::new().mode(0o700).create(parent) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists && parent.is_dir() => {}
            Err(error) => return Err(error.into()),
        }
    }
    let mut temporary = tempfile::Builder::new()
        .prefix(".pearls-")
        .tempfile_in(parent)?;
    temporary.write_all(json::encode(value)?.as_bytes())?;
    temporary.persist(path)?;
    Ok(())
}
pub fn location(paths: &Paths) -> PathBuf {
    std::env::var_os("NACRE_PEARL_STATE")
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.state.join("shell/pearls.json"))
}
pub fn run(args: &[String]) -> Result<i32> {
    let result = (|| -> Result<Option<Value>> {
        let paths = Paths::environment()?;
        let path = location(&paths);
        if args.first().is_some_and(|arg| arg == "save") {
            let mut line = String::new();
            std::io::stdin().lock().read_line(&mut line)?;
            save(&path, &json::parse_value(&line)?)?;
            Ok(None)
        } else {
            Ok(Some(load(&path)?))
        }
    })();
    match result {
        Ok(Some(value)) => {
            println!("{}", json::encode(&value)?);
            Ok(0)
        }
        Ok(None) => Ok(0),
        Err(error) => {
            println!("{}", json::encode(&value!({"error":error.to_string()}))?);
            Ok(1)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn frozen_python_stdout_files_modes_and_corruption_preservation() -> Result<()> {
        let reference: Value =
            serde_json::from_str(include_str!("../tests/fixtures/pearl-state-reference.json"))?;
        for case in reference["cases"].as_array().ok_or("Missing cases")? {
            let home = tempfile::tempdir()?;
            let path = home.path().join(".local/state/nacre/shell/pearls.json");
            if let Some(initial) = case["request"]["initial"].as_str() {
                fs::create_dir_all(path.parent().ok_or("Missing parent")?)?;
                fs::write(&path, initial)?;
                fs::set_permissions(
                    path.parent().ok_or("Missing parent")?,
                    fs::Permissions::from_mode(0o755),
                )?;
            }
            let result = if case["request"]["command"] == "save" {
                let line = case["request"]["input"]
                    .as_str()
                    .ok_or("Missing input")?
                    .lines()
                    .next()
                    .unwrap_or("");
                json::parse_value(line)
                    .and_then(|value| save(&path, &value))
                    .map(|()| None)
            } else {
                load(&path).map(Some)
            };
            let (stdout, exit) = match result {
                Ok(Some(value)) => (json::encode(&value)? + "\n", 0),
                Ok(None) => (String::new(), 0),
                Err(error) => (
                    json::encode(&value!({"error":error.to_string()}))? + "\n",
                    1,
                ),
            };
            assert_eq!(stdout, case["stdout"], "{}", case["request"]["name"]);
            assert_eq!(value!(exit), case["exit"]);
            let mut files = serde_json::Map::new();
            for entry in walkdir::WalkDir::new(home.path()) {
                let entry = entry?;
                if entry.file_type().is_file() {
                    let bytes = fs::read(entry.path())?;
                    files.insert(entry.path().strip_prefix(home.path())?.to_string_lossy().into(),value!({"hex":bytes.iter().map(|byte|format!("{byte:02x}")).collect::<String>(),"mode":format!("0o{:o}",entry.metadata()?.permissions().mode()&0o777)}));
                }
            }
            assert_eq!(
                Value::Object(files),
                case["files"],
                "{}",
                case["request"]["name"]
            );
            if !case["parent_mode"].is_null() {
                assert_eq!(
                    format!(
                        "0o{:o}",
                        fs::metadata(path.parent().ok_or("Missing parent")?)?
                            .permissions()
                            .mode()
                            & 0o777
                    ),
                    case["parent_mode"]
                );
            }
        }
        Ok(())
    }
}
