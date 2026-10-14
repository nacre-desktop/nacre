//! Owned terminal app entries, literal argv and the shared versioned state boundary.
use crate::{files, i18n::tr, menu_registry, paths::Paths, process, state, Result};
use serde_json::{json, Value};
use std::{
    collections::HashSet,
    fs,
    io::Write,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Path, PathBuf},
};

fn store(paths: &Paths) -> PathBuf {
    paths.state.join("menu/tui-apps.json")
}
fn valid_id(id: &str) -> bool {
    id.len() == 32
        && id
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}
fn executable(program: &str) -> bool {
    let check = |p: &Path| {
        p.is_file() && fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
    };
    if Path::new(program).is_absolute() {
        check(Path::new(program))
    } else {
        !program.contains('/')
            && std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
                .any(|p| check(&p.join(program)))
    }
}
fn validate(row: &Value) -> Result<()> {
    let name = row["name"].as_str().ok_or("Choose an application name")?;
    if name.trim().is_empty() || name.chars().count() > 80 || name.chars().any(|c| c.is_control()) {
        return Err("Choose a short application name without control characters".into());
    }
    let icon = row["icon"].as_str().ok_or("Choose an icon name")?;
    if icon.is_empty()
        || icon.len() > 120
        || !icon
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._-".contains(c))
    {
        return Err("Use an icon theme name or Material symbol".into());
    }
    let args = row["command"]
        .as_array()
        .filter(|a| !a.is_empty() && a.len() <= 64)
        .ok_or("Use one command argument per line")?;
    if args.iter().any(|a| {
        a.as_str()
            .is_none_or(|s| s.len() > 4096 || s.contains('\0'))
    }) || args[0].as_str().unwrap().is_empty()
    {
        return Err("Invalid application command arguments".into());
    }
    if !matches!(
        row["terminal"].as_str(),
        Some("kitty" | "foot" | "alacritty" | "wezterm")
    ) {
        return Err("Choose Kitty, Foot, Alacritty or WezTerm".into());
    }
    Ok(())
}
fn load(paths: &Paths) -> Result<Value> {
    let rows = state::read_strict(&store(paths), json!([]))?;
    let entries = rows
        .as_array()
        .filter(|a| a.len() <= 128)
        .ok_or("Invalid terminal app registry; original preserved")?;
    let mut ids = HashSet::new();
    for row in entries {
        validate(row)?;
        let id = row["id"]
            .as_str()
            .filter(|id| valid_id(id))
            .ok_or("Invalid terminal app identity")?;
        if !ids.insert(id) {
            return Err("Duplicate terminal app identity; original preserved".into());
        }
    }
    Ok(rows)
}
pub fn snapshot(paths: &Paths) -> Value {
    match load(paths) {
        Ok(mut rows) => {
            for row in rows.as_array_mut().unwrap() {
                row["available"] = json!(
                    executable(row["terminal"].as_str().unwrap())
                        && executable(row["command"][0].as_str().unwrap())
                );
            }
            json!({"rows":rows,"error":"","terminals":(["kitty","foot","alacritty","wezterm"].iter().filter(|p|executable(p)).collect::<Vec<_>>())})
        }
        Err(error) => json!({"rows":[],"error":error.to_string(),"terminals":[]}),
    }
}
fn desktop_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('`', "\\`")
            .replace('$', "\\$")
            .replace('%', "%%")
    )
}
fn desktop(paths: &Paths, id: &str) -> PathBuf {
    paths.applications().join(format!("nacre-tui-{id}.desktop"))
}
fn create(paths: &Paths, input: Value) -> Result<Value> {
    validate(&input)?;
    let _guard = menu_registry::registry_lock(paths, "tui-apps")?;
    let mut rows = load(paths)?;
    if rows.as_array().unwrap().len() >= 128 {
        return Err("Terminal app limit reached; remove one first".into());
    }
    let id = uuid::Uuid::new_v4().simple().to_string();
    let row = json!({"id":id,"name":input["name"].as_str().unwrap().trim(),"command":input["command"],"terminal":input["terminal"],"icon":input["icon"]});
    let helper = std::env::current_exe()?;
    let body=format!("[Desktop Entry]\nType=Application\nName={}\nIcon={}\nExec={} software-tui launch {}\nTerminal=false\nCategories=System;TerminalEmulator;\nX-Nacre-Tui-ID={}\n",row["name"].as_str().unwrap().replace('\\',"\\\\"),row["icon"].as_str().unwrap(),desktop_quote(helper.to_str().ok_or("Invalid helper path")?),id,id);
    fs::create_dir_all(paths.applications())?;
    let path = desktop(paths, &id);
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&path)?;
    file.write_all(body.as_bytes())?;
    file.sync_all()?;
    rows.as_array_mut().unwrap().push(row);
    if let Err(error) = state::write(&store(paths), &rows) {
        if fs::read(&path)? == body.as_bytes() {
            fs::remove_file(path)?;
        }
        return Err(error);
    }
    menu_registry::invalidate(paths)?;
    Ok(json!({"id":id,"message":tr("software.tui.created","Terminal app added to All apps")}))
}
fn remove(paths: &Paths, id: &str) -> Result<Value> {
    if !valid_id(id) {
        return Err("Invalid terminal app identity".into());
    }
    let _guard = menu_registry::registry_lock(paths, "tui-apps")?;
    let mut rows = load(paths)?;
    if !rows.as_array().unwrap().iter().any(|r| r["id"] == id) {
        return Err("Terminal app no longer exists".into());
    }
    let path = desktop(paths, id);
    let previous = match fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_symlink() || !m.is_file() => {
            return Err("Foreign launcher preserved".into())
        }
        Ok(_) => {
            let data = fs::read_to_string(&path)?;
            if !data
                .lines()
                .any(|line| line == format!("X-Nacre-Tui-ID={id}"))
            {
                return Err("Foreign launcher preserved".into());
            }
            Some(data)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e.into()),
    };
    rows.as_array_mut().unwrap().retain(|r| r["id"] != id);
    // Remove only this generated launcher; restore it if the guarded store write fails.
    if previous.is_some() {
        fs::remove_file(&path)?
    }
    if let Err(error) = state::write(&store(paths), &rows) {
        if let Some(data) = previous {
            if !path.exists() {
                files::atomic(&path, data.as_bytes())?;
            }
        }
        return Err(error);
    }
    menu_registry::invalidate(paths)?;
    Ok(json!({"message":tr("software.tui.removed","Terminal app removed")}))
}
fn terminal_command(row: &Value, home: &Path) -> Vec<String> {
    let terminal = row["terminal"].as_str().unwrap();
    let mut args = match terminal {
        "kitty" => vec![
            "kitty".into(),
            "--directory".into(),
            home.to_string_lossy().into_owned(),
            "--title".into(),
            row["name"].as_str().unwrap().into(),
            "--".into(),
        ],
        "foot" => vec![
            "foot".into(),
            "--working-directory".into(),
            home.to_string_lossy().into_owned(),
            "--title".into(),
            row["name"].as_str().unwrap().into(),
            "--".into(),
        ],
        "alacritty" => vec![
            "alacritty".into(),
            "--working-directory".into(),
            home.to_string_lossy().into_owned(),
            "--title".into(),
            row["name"].as_str().unwrap().into(),
            "-e".into(),
        ],
        _ => vec![
            "wezterm".into(),
            "start".into(),
            "--cwd".into(),
            home.to_string_lossy().into_owned(),
            "--".into(),
        ],
    };
    args.extend(
        row["command"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s.as_str().unwrap().to_string()),
    );
    args
}
fn launch(paths: &Paths, id: &str) -> Result<Value> {
    if !valid_id(id) {
        return Err("Invalid terminal app identity".into());
    }
    let rows = load(paths)?;
    let row = rows
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == id)
        .ok_or("Terminal app no longer exists")?;
    if !executable(row["terminal"].as_str().unwrap())
        || !executable(row["command"][0].as_str().unwrap())
    {
        return Err("Install the selected terminal and command first".into());
    }
    let args = terminal_command(row, &paths.home);
    process::spawn_detached_quiet(&args[0], &args[1..])?;
    Ok(json!({"message":tr("software.tui.opened","Terminal app opened")}))
}
pub fn execute(paths: &Paths, action: &str, id: &str, input: Option<Value>) -> Result<Value> {
    match action {
        "load" => Ok(snapshot(paths)),
        "create" => create(paths, input.ok_or("Missing terminal app fields")?),
        "remove" => remove(paths, id),
        "launch" => launch(paths, id),
        _ => Err("Unknown terminal app action".into()),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roundtrip_is_versioned_and_future_data_and_foreign_launchers_are_preserved() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let result = create(
            &paths,
            json!({"name":"Monitor","command":["printf","literal; $(not interpreted)"],"terminal":"kitty","icon":"utilities-terminal"}),
        )?;
        let id = result["id"].as_str().unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(store(&paths))?)?["version"],
            1
        );
        let row = &load(&paths)?[0];
        assert_eq!(
            terminal_command(row, &paths.home).last().unwrap(),
            "literal; $(not interpreted)"
        );
        fs::write(desktop(&paths, id), "[Desktop Entry]\nName=Foreign\n")?;
        assert!(remove(&paths, id).is_err());
        assert_eq!(load(&paths)?.as_array().unwrap().len(), 1);
        fs::write(store(&paths), b"{\"version\":99,\"items\":[]}")?;
        assert!(create(
            &paths,
            json!({"name":"Next","command":["true"],"terminal":"kitty","icon":"terminal"})
        )
        .is_err());
        assert_eq!(fs::read(store(&paths))?, b"{\"version\":99,\"items\":[]}");
        Ok(())
    }
}
