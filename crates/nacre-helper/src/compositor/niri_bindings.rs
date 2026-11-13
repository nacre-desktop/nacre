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

fn is_niri_process(comm: &str) -> bool {
    comm.starts_with("niri") || comm.starts_with("nacre-niri")
}

fn action_label(name: &str, args: &[Value]) -> String {
    let arg = args.first().and_then(Value::as_str).unwrap_or("");
    match name {
        "focus-workspace" => format!("Focus workspace {arg}"),
        "move-window-to-workspace" => format!("Move window to workspace {arg}"),
        "focus-window-left" | "focus-window-right" | "focus-window-up" | "focus-window-down" => {
            format!("Focus window {}", name.trim_start_matches("focus-window-"))
        }
        "move-window-to-monitor-left"
        | "move-window-to-monitor-right"
        | "move-window-to-monitor-up"
        | "move-window-to-monitor-down" => {
            format!(
                "Move window to monitor {}",
                name.trim_start_matches("move-window-to-monitor-")
            )
        }
        "close-window" => "Close window".into(),
        "fullscreen-window" => "Toggle fullscreen".into(),
        "toggle-window-floating" => "Toggle floating window".into(),
        "spawn" | "spawn-sh" => spawn_label(args),
        _ => name.replace('-', " "),
    }
}

fn spawn_label(args: &[Value]) -> String {
    let command = args
        .iter()
        .filter_map(Value::as_str)
        .collect::<Vec<_>>()
        .join(" ");
    if let Some(action) = command.split("compositor action ").nth(1) {
        let action = action.split_whitespace().collect::<Vec<_>>();
        let name = action.first().copied().unwrap_or("");
        let value = action
            .windows(2)
            .find(|pair| pair[0] == "--workspace")
            .map(|pair| pair[1]);
        return match name {
            "switch_workspace" => value
                .map(|n| format!("Focus workspace {n}"))
                .unwrap_or_else(|| "Focus workspace".into()),
            "move_window" => value
                .map(|n| format!("Move window to workspace {n}"))
                .unwrap_or_else(|| "Move window to workspace".into()),
            "close_window" => "Close window".into(),
            "fullscreen_window" => "Toggle fullscreen".into(),
            "float_window" => "Toggle floating window".into(),
            "cycle_window" => "Cycle windows".into(),
            "reload" => "Reload compositor configuration".into(),
            _ => name.replace('_', " "),
        };
    }
    if let Some(route) = command
        .split(" menu ")
        .nth(1)
        .or_else(|| command.split("nacre-menu ").nth(1))
        .and_then(|tail| tail.split_whitespace().next())
    {
        return match route {
            "learn.shortcuts" | "keys" => "Keyboard shortcuts".into(),
            "style.picker" | "wallpaper" => "Choose wallpaper".into(),
            "actions.clipboard" | "clipboard" => "Clipboard history".into(),
            "actions.windows" | "overview" => "Window overview".into(),
            "actions.emoji" => "Choose emoji".into(),
            "actions.capture" => "Capture screenshot".into(),
            "power" => "Open power menu".into(),
            _ => route.replace(['.', '-'], " "),
        };
    }
    for (needle, label) in [
        ("nacre-app terminal", "Open terminal"),
        ("nacre-app browser", "Open browser"),
        ("nacre-app files", "Open files"),
        ("nacre-shell left", "Open the AI sidebar"),
        ("nacre-shell detach", "Detach panel"),
        ("nacre-shell palette", "Open command palette"),
    ] {
        if command.contains(needle) {
            return label.into();
        }
    }
    let executable = args.first().and_then(Value::as_str).unwrap_or("command");
    Path::new(executable)
        .file_stem()
        .and_then(|v| v.to_str())
        .unwrap_or("command")
        .to_owned()
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
        if !is_niri_process(fs::read_to_string(format!("/proc/{pid}/comm")).ok()?.trim()) {
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
                .unwrap_or_else(|| action_label(name, &args));
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
    fn nacre_niri_process_is_accepted_and_labels_actions() -> crate::Result<()> {
        assert!(is_niri_process("nacre-niri"));
        assert!(is_niri_process("niri"));
        assert!(!is_niri_process("other"));
        let rows = parse("binds { Mod+3 { focus-workspace 3; }; Mod+Shift+3 { move-window-to-workspace 3; }; Mod+Left { focus-window-left; }; Mod+K { spawn \"nacrectl\" \"ui\" \"nacre\" \"menu\" \"learn.shortcuts\"; }; }")?;
        assert_eq!(rows[0]["flags"]["description"], "Focus workspace 3");
        assert_eq!(
            rows[1]["flags"]["description"],
            "Move window to workspace 3"
        );
        assert_eq!(rows[2]["flags"]["description"], "Focus window left");
        assert_eq!(rows[3]["flags"]["description"], "Keyboard shortcuts");
        assert_eq!(
            spawn_label(&[Value::from(
                "exec env NACRE_COMPOSITOR=niri \"$NACRE_COMPOSITOR_TOOLS/nacre-helper\" compositor action switch_workspace --workspace 3"
            )]),
            "Focus workspace 3"
        );
        assert_eq!(
            spawn_label(&[Value::from("uwsm app -- ~/.local/bin/nacre-app browser")]),
            "Open browser"
        );
        Ok(())
    }

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
    fn generated_nacre_niri_overlay_reads_generated_config_includes_and_custom_binds(
    ) -> crate::Result<()> {
        struct ChildGuard(std::process::Child);
        impl Drop for ChildGuard {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let session = paths.share.join("niri-on-pc/session/config");
        fs::create_dir_all(&session)?;
        let custom = paths.config.join("niri/custom.kdl");
        fs::create_dir_all(custom.parent().unwrap())?;
        let mut root = String::new();
        for name in [
            "wallpaper.kdl",
            "bindings.kdl",
            "window-rules.kdl",
            "keyboard.kdl",
            "profile.kdl",
            "colors.kdl",
            "host.kdl",
        ] {
            root.push_str(&format!(
                "include {}\n",
                serde_json::to_string(&session.join(name).to_string_lossy())?
            ));
            fs::write(
                session.join(name),
                if name == "bindings.kdl" {
                    "binds { Super+Space hotkey-overlay-title=\"Launcher\" { spawn \"nacre-menu\"; }; }\n"
                } else {
                    ""
                },
            )?;
        }
        root.push_str("include \"desktop.kdl\"\n");
        root.push_str(&format!(
            "include {}\n",
            serde_json::to_string(&session.join("bead-pearls.kdl").to_string_lossy())?
        ));
        for name in [
            "desktop.kdl",
            "bead-pearls.kdl",
            "workspaces.kdl",
            "monitor-workspace-actions.kdl",
            "workspace-visitors.kdl",
        ] {
            fs::write(
                session.join(name),
                if name == "workspaces.kdl" {
                    "binds { Mod+1 { focus-workspace 1; }; }\n"
                } else {
                    ""
                },
            )?;
        }
        root.push_str("include \"workspaces.kdl\"\ninclude \"monitor-workspace-actions.kdl\"\ninclude \"workspace-visitors.kdl\"\n");
        root.push_str(&format!(
            "include {}\n",
            serde_json::to_string(&custom.to_string_lossy())?
        ));
        fs::write(session.join("config.kdl"), root)?;
        fs::write(&custom, "binds { Super+Space hotkey-overlay-title=\"Custom launcher\" { close-window; }; Super+K hotkey-overlay-title=\"Custom action\" { close-window; }; }\n")?;
        // This is the actual Tier 1 loader: nacre-niri's overlay includes the
        // generated session root, which in turn includes bindings and custom.kdl.
        let overlay = session.join("nacre-niri.kdl");
        fs::write(
            &overlay,
            "include \"config.kdl\"\nbinds { Super+Alt+H hotkey-overlay-title=\"Nacre:restore-current\" { restore-current-workspace-hidden-windows; }; }\n",
        )?;
        let child_script = home.path().join("set-comm.py");
        fs::write(
            &child_script,
            "import ctypes, time\nctypes.CDLL(None).prctl(15, b'nacre-niri')\ntime.sleep(30)\n",
        )?;
        let mut child = ChildGuard(
            std::process::Command::new("python")
                .arg(&child_script)
                .arg("--config")
                .arg(&overlay)
                .spawn()?,
        );
        let socket = home.path().join(format!("niri.{}.sock", child.0.id()));
        let listener = std::os::unix::net::UnixListener::bind(&socket)?;
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while std::time::Instant::now() < deadline
            && fs::read_to_string(format!("/proc/{}/comm", child.0.id()))
                .is_ok_and(|comm| comm.trim() != "nacre-niri")
        {
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let comm = fs::read_to_string(format!("/proc/{}/comm", child.0.id()))?;
        let args = fs::read(format!("/proc/{}/cmdline", child.0.id()))?;
        let argv_has_overlay = args
            .split(|byte| *byte == 0)
            .any(|arg| arg == overlay.as_os_str().as_encoded_bytes());
        let environment = BTreeMap::from([
            ("NIRI_SOCKET".into(), socket.to_string_lossy().into_owned()),
            (
                "NIRI_CONFIG".into(),
                session.join("config.kdl").to_string_lossy().into_owned(),
            ),
        ]);
        let active = loader(&paths, &environment);
        drop(listener);
        assert_eq!(comm.trim(), "nacre-niri");
        assert!(argv_has_overlay);
        child.0.kill()?;
        child.0.wait()?;
        assert_eq!(active, overlay);
        let (files, rows) = inventory(&active, home.path())?;
        assert!(files.iter().any(|path| path == &overlay));
        assert!(files.iter().any(|path| path == &session.join("config.kdl")));
        assert!(files
            .iter()
            .any(|path| path == &session.join("bindings.kdl")));
        assert!(files.iter().any(|path| path == &custom));
        assert!(rows
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["keys"] == "SUPER+Space"
                && row["flags"]["description"] == "Custom launcher"));
        assert!(rows
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["keys"] == "SUPER+K" && row["flags"]["description"] == "Custom action"));
        assert!(rows
            .as_array()
            .unwrap()
            .iter()
            .any(|row| row["keys"] == "SUPER+1"));
        assert!(rows.as_array().unwrap().iter().any(|row| {
            row["keys"] == "SUPER+ALT+H" && row["flags"]["description"] == "Nacre:restore-current"
        }));
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
