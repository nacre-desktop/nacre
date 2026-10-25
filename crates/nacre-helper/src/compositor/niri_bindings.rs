//! Read the active loader, following includes rather than guessing fragment names.
use super::niri_state::Error;
use crate::paths::Paths;
use kdl::KdlDocument;
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
type Result<T> = std::result::Result<T, Error>;
fn error(message: impl ToString) -> Error {
    Error {
        kind: "RuntimeError",
        message: message.to_string(),
    }
}

pub fn loader(paths: &Paths, env: &BTreeMap<String, String>) -> PathBuf {
    // Niri has no loaded-config query. Its socket name includes the process ID;
    // read that process's arguments without opening an extra IPC connection.
    let standard = paths
        .config
        .parent()
        .unwrap_or(&paths.config)
        .join("niri/config.kdl");
    if let Some(path) = env.get("NIRI_SOCKET").and_then(|socket| {
        let socket = fs::canonicalize(socket).ok()?;
        let name = socket
            .file_name()?
            .to_str()?
            .strip_prefix("niri.")?
            .strip_suffix(".sock")?;
        let pid = name.rsplit('.').next()?.parse::<u32>().ok()?;
        if !fs::read_to_string(format!("/proc/{pid}/comm"))
            .ok()?
            .trim()
            .starts_with("niri")
        {
            return None;
        }
        let args = fs::read(format!("/proc/{pid}/cmdline")).ok()?;
        let args: Vec<_> = args
            .split(|b| *b == 0)
            .map(String::from_utf8_lossy)
            .collect();
        let environ = fs::read(format!("/proc/{pid}/environ")).unwrap_or_default();
        let variable = |name: &[u8]| {
            environ.split(|b| *b == 0).find_map(|entry| {
                entry
                    .strip_prefix(name)
                    .filter(|p| !p.is_empty())
                    .map(|p| PathBuf::from(String::from_utf8_lossy(p).as_ref()))
            })
        };
        let path = args
            .windows(2)
            .find(|a| a[0] == "-c" || a[0] == "--config")
            .map(|a| PathBuf::from(a[1].as_ref()))
            .or_else(|| {
                args.iter().find_map(|a| {
                    a.strip_prefix("--config=")
                        .or_else(|| a.strip_prefix("-c").filter(|p| !p.is_empty()))
                        .map(PathBuf::from)
                })
            })
            .or_else(|| variable(b"NIRI_CONFIG="))
            .or_else(|| variable(b"NACRE_NIRI_CONFIG="))
            .unwrap_or_else(|| {
                let config = variable(b"XDG_CONFIG_HOME=")
                    .filter(|p| p.is_absolute())
                    .map(|p| p.join("niri/config.kdl"))
                    .unwrap_or_else(|| {
                        variable(b"HOME=")
                            .filter(|p| p.is_absolute())
                            .map(|p| p.join(".config/niri/config.kdl"))
                            .unwrap_or_else(|| standard.clone())
                    });
                let system = PathBuf::from("/etc/niri/config.kdl");
                if config.is_file() || !system.is_file() {
                    config
                } else {
                    system
                }
            });
        Some(if path.is_absolute() {
            path
        } else {
            fs::read_link(format!("/proc/{pid}/cwd")).ok()?.join(path)
        })
    }) {
        return path;
    }
    for key in ["NACRE_NIRI_CONFIG", "NIRI_CONFIG"] {
        if let Some(path) = env.get(key).filter(|v| !v.is_empty()) {
            return PathBuf::from(path);
        }
    }
    let session = paths.share.join("niri-on-pc/session/config/config.kdl");
    [session, standard, paths.config.join("niri/config.kdl")]
        .into_iter()
        .find(|p| p.is_file())
        .unwrap_or_else(|| {
            paths
                .config
                .parent()
                .unwrap_or(&paths.config)
                .join("niri/config.kdl")
        })
}

fn key(combo: &str) -> String {
    let mut parts: Vec<_> = combo
        .split('+')
        .map(|p| match p.to_ascii_lowercase().as_str() {
            "mod" | "super" | "win" => "SUPER".into(),
            "control" | "ctrl" => "CTRL".into(),
            "alt" => "ALT".into(),
            "shift" => "SHIFT".into(),
            "iso_level3_shift" | "mod5" => "Mod5".into(),
            "iso_level5_shift" => "ISO_Level5_Shift".into(),
            _ => p.to_owned(),
        })
        .collect();
    let last = parts.pop().unwrap_or_default();
    parts.sort_by_key(|p| match p.as_str() {
        "SUPER" => 0,
        "CTRL" => 1,
        "ALT" => 2,
        "SHIFT" => 3,
        "Mod5" => 4,
        "ISO_Level5_Shift" => 5,
        _ => 6,
    });
    parts.push(last);
    parts.join("+")
}

pub fn parse(source: &str) -> Result<Value> {
    let doc: KdlDocument = source.parse().map_err(error)?;
    let mut rows = Vec::new();
    for block in doc.nodes().iter().filter(|n| n.name().value() == "binds") {
        for bind in block.children().into_iter().flat_map(|d| d.nodes()) {
            let Some(action) = bind.children().and_then(|d| d.nodes().first()) else {
                continue;
            };
            let name = action.name().value();
            let args: Vec<Value> = action
                .entries()
                .iter()
                .filter(|e| e.name().is_none())
                .map(|e| {
                    e.value()
                        .as_string()
                        .map(Value::from)
                        .unwrap_or_else(|| Value::from(e.value().to_string()))
                })
                .collect();
            let description = bind
                .get("hotkey-overlay-title")
                .and_then(|e| e.value().as_string())
                .filter(|v| !v.is_empty())
                .map(str::to_owned)
                .unwrap_or_else(|| {
                    if name == "spawn" || name == "spawn-sh" {
                        let command = args.first().and_then(Value::as_str).unwrap_or("");
                        Path::new(command.split_whitespace().next().unwrap_or("exec cmd"))
                            .file_stem()
                            .and_then(|v| v.to_str())
                            .unwrap_or("exec cmd")
                            .to_owned()
                    } else {
                        name.replace('-', " ")
                    }
                });
            rows.push(json!({"keys":key(bind.name().value()),"submap":"","flags":{"description":description},"action":{"method":if name == "spawn-sh" || name == "spawn" { "exec_cmd" } else { name },"args":args}}));
        }
    }
    Ok(json!(rows))
}

/// Include order is preserved. Track even missing includes so their creation wakes us.
/// Invalid/unreadable input fails the read; the daemon retains its last good list.
fn scan(root: &Path, home: &Path, files: &mut Vec<PathBuf>) -> Result<Value> {
    fn visit(
        path: &Path,
        home: &Path,
        stack: &mut Vec<PathBuf>,
        files: &mut Vec<PathBuf>,
        rows: &mut Vec<Value>,
        total: &mut usize,
        optional: bool,
    ) -> Result<()> {
        let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        if stack.contains(&canonical) || files.len() >= 256 {
            return Err(error("Niri include cycle or file limit"));
        }
        files.push(path.to_path_buf());
        if fs::metadata(path).is_ok_and(|m| m.len() > 2 * 1024 * 1024) {
            return Err(error("Niri file exceeds read limit"));
        }
        let source = match fs::read_to_string(path) {
            Ok(v) => v,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound && optional => return Ok(()),
            Err(e) => return Err(error(e)),
        };
        *total += source.len();
        if *total > 8 * 1024 * 1024 {
            return Err(error("Niri configuration exceeds read limit"));
        }
        let doc: KdlDocument = source.parse().map_err(error)?;
        stack.push(canonical);
        for node in doc.nodes() {
            if node.name().value() == "include" {
                let include = node
                    .get(0)
                    .and_then(|e| e.value().as_string())
                    .ok_or_else(|| error("Missing include path"))?;
                let child = include
                    .strip_prefix("~/")
                    .map(|p| home.join(p))
                    .unwrap_or_else(|| PathBuf::from(include));
                let child = if child.is_absolute() {
                    child
                } else {
                    path.parent().unwrap_or(Path::new(".")).join(child)
                };
                visit(
                    &child,
                    home,
                    stack,
                    files,
                    rows,
                    total,
                    node.get("optional")
                        .and_then(|e| e.value().as_bool())
                        .unwrap_or(false),
                )?;
            } else if node.name().value() == "binds" {
                rows.extend(
                    parse(&node.to_string())?
                        .as_array()
                        .unwrap()
                        .iter()
                        .cloned(),
                );
            }
        }
        stack.pop();
        Ok(())
    }
    let mut rows = Vec::new();
    visit(root, home, &mut Vec::new(), files, &mut rows, &mut 0, true)?;
    // Generated overrides are already removed by the session builder. Also
    // collapse repeated key spellings from includes to the final definition.
    let mut effective = BTreeMap::new();
    for row in rows {
        effective.insert(row["keys"].as_str().unwrap().to_ascii_lowercase(), row);
    }
    Ok(json!(effective.into_values().collect::<Vec<_>>()))
}

// Keep every path reached before a parse/read failure. A subsequent fix must wake
// the daemon even when the broken include lives outside the loader directory.
pub fn source_files(root: &Path, home: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let _ = scan(root, home, &mut files);
    files
}
pub fn inventory(root: &Path, home: &Path) -> Result<(Vec<PathBuf>, Value)> {
    let mut files = Vec::new();
    let rows = scan(root, home, &mut files)?;
    Ok((files, rows))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn session_tree_custom_overrides_and_all_actions() -> crate::Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let session = paths.share.join("niri-on-pc/session/config");
        fs::create_dir_all(session.join("workspaces"))?;
        fs::create_dir_all(paths.config.join("niri"))?;
        let custom = paths.config.join("niri/custom.kdl");
        fs::write(
            session.join("config.kdl"),
            format!(
                "include \"bindings.kdl\"\ninclude \"workspaces/keys.kdl\"\ninclude {}\n",
                serde_json::to_string(&custom)?
            ),
        )?;
        fs::write(session.join("bindings.kdl"), "// Overridden by custom.kdl: Super+Q\nbinds { Super+Q hotkey-overlay-title=\"Old\" { close-window; }; Super+K hotkey-overlay-title=\"Nacre:shortcuts\" { spawn \"nacrectl\" \"ui\" \"nacre\" \"menu\" \"learn.shortcuts\"; }; }\n")?;
        fs::write(
            session.join("workspaces/keys.kdl"),
            "binds { Mod+1 { focus-workspace 1; }; }\n",
        )?;
        fs::write(&custom, "binds {\nQ hotkey-overlay-title=r#\"Quote \" title\"# {\n spawn \"custom-tool\" \"arg\"\n}\nSuper+Q hotkey-overlay-title=\"Custom close\" { close-window; }\n}\n")?;
        let socket = home.path().join("niri.sock");
        let listener = std::os::unix::net::UnixListener::bind(&socket)?;
        listener.set_nonblocking(true)?;
        let root = loader(
            &paths,
            &BTreeMap::from([("NIRI_SOCKET".into(), socket.to_string_lossy().into_owned())]),
        );
        assert_eq!(root, session.join("config.kdl"));
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
        let (files, rows) = inventory(&root, home.path())?;
        assert_eq!(files.len(), 4);
        let rows = rows.as_array().unwrap();
        assert_eq!(rows.len(), 4);
        assert_eq!(
            rows.iter().find(|r| r["keys"] == "SUPER+Q").unwrap()["flags"]["description"],
            "Custom close"
        );
        assert!(rows
            .iter()
            .any(|r| r["flags"]["description"] == "Quote \" title"));
        let query = super::super::niri_config::binding_query(&json!(rows))?;
        assert!(query
            .as_array()
            .unwrap()
            .iter()
            .any(|r| r["arg"] == "nacrectl ui nacre menu learn.shortcuts"));
        let explicit =
            BTreeMap::from([("NIRI_CONFIG".into(), custom.to_string_lossy().into_owned())]);
        assert_eq!(loader(&paths, &explicit), custom);
        Ok(())
    }
    #[test]
    fn empty_missing_invalid_and_cyclic_configs() -> crate::Result<()> {
        let home = tempfile::tempdir()?;
        let root = home.path().join("config.kdl");
        assert_eq!(inventory(&root, home.path())?.1, json!([]));
        fs::write(&root, "")?;
        assert_eq!(inventory(&root, home.path())?.1, json!([]));
        fs::write(&root, "include optional=true \"missing.kdl\"\n")?;
        assert_eq!(inventory(&root, home.path())?.0.len(), 2);
        fs::write(&root, "include \"required.kdl\"\n")?;
        assert!(inventory(&root, home.path()).is_err());
        assert_eq!(source_files(&root, home.path()).len(), 2);
        fs::write(&root, "include \"config.kdl\"\n")?;
        assert!(inventory(&root, home.path()).is_err());
        assert!(parse("binds {").is_err());
        assert!(super::super::niri_config::bindings("", "other").is_err());
        Ok(())
    }
}
