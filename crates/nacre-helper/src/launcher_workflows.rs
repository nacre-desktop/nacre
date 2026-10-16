//! Private launcher recipes, configured file roots and bounded local previews.
//! Search never executes a recipe or sends a query to a network service.
use crate::{files, i18n::tr, menu_registry, paths::Paths, process, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

fn available(name: &str) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .map(|p| p.join(name))
        .any(|p| p.is_file() && fs::metadata(p).is_ok_and(|m| m.permissions().mode() & 0o111 != 0))
}
fn read(path: &Path, fallback: Value) -> Result<Value> {
    crate::state::read(path, fallback)
}

fn defaults() -> Value {
    json!({"version":1,"entries":[],"roots":[]})
}
fn emoji_search(query: &str) -> Result<Value> {
    nacre_menu::emoji::bundled().search(query)
}
fn configuration(paths: &Paths) -> Result<Value> {
    let value = read(&paths.config.join("launcher-workflows.json"), defaults())?;
    if value["version"] != 1 || !value["entries"].is_array() || !value["roots"].is_array() {
        return Err("Unsupported launcher workflows format".into());
    }
    if value["entries"].as_array().unwrap().len() > 300
        || value["roots"].as_array().unwrap().len() > 32
    {
        return Err("Too many launcher workflows".into());
    }
    for entry in value["entries"].as_array().unwrap() {
        validate(entry)?;
    }
    Ok(value)
}
fn text<'a>(data: &'a Value, key: &str, max: usize, required: bool) -> Result<&'a str> {
    let value = data[key].as_str().unwrap_or("");
    if value.len() > max || value.contains('\0') || required && value.trim().is_empty() {
        return Err(format!("Enter a valid {key}").into());
    }
    Ok(value)
}
fn argv(value: &Value) -> Result<Vec<String>> {
    let args = value
        .as_array()
        .filter(|v| !v.is_empty() && v.len() <= 32)
        .ok_or("Arguments must be a JSON array of strings")?;
    let args = args
        .iter()
        .map(|v| {
            v.as_str()
                .filter(|s| !s.contains('\0') && s.len() <= 4096)
                .map(str::to_owned)
                .ok_or("Invalid command argument")
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if args[0].is_empty() || args[0].starts_with('-') {
        return Err("Enter an explicit executable".into());
    }
    Ok(args)
}
fn parse_arguments(value: &str) -> Result<Value> {
    if value.trim_start().starts_with('[') {
        Ok(serde_json::from_str(value)?)
    } else {
        Ok(json!(value.lines().map(str::to_owned).collect::<Vec<_>>()))
    }
}
fn valid_url(url: &str) -> bool {
    (url.starts_with("https://") || url.starts_with("http://"))
        && !url.chars().any(char::is_control)
        && url.len() <= 4096
}
fn validate(entry: &Value) -> Result<()> {
    text(entry, "id", 100, true)?;
    text(entry, "label", 100, true)?;
    text(entry, "icon", 80, false)?;
    match entry["type"].as_str().unwrap_or("") {
        "action" => {
            argv(&entry["arguments"])?;
            let cwd = text(entry, "directory", 4096, false)?;
            if !cwd.is_empty() && !Path::new(cwd).is_absolute() {
                return Err("Working directory must be absolute".into());
            }
        }
        "snippet" => {
            text(entry, "text", 65536, true)?;
        }
        "search" | "bookmark" => {
            let url = text(entry, "url", 4096, true)?;
            if !valid_url(url) || entry["type"] == "search" && !url.contains("{query}") {
                return Err("Enter an HTTP or HTTPS URL; searches need {query}".into());
            }
        }
        "workspace" => {
            let path = text(entry, "directory", 4096, true)?;
            if !Path::new(path).is_absolute() {
                return Err("Project directory must be absolute".into());
            }
            argv(&entry["editor"])?;
            argv(&entry["terminal"])?;
            let url = text(entry, "url", 4096, false)?;
            if !url.is_empty() && !valid_url(url) {
                return Err("Enter an HTTP or HTTPS documentation URL".into());
            }
        }
        _ => return Err("Unknown launcher workflow type".into()),
    }
    Ok(())
}
fn projects(paths: &Paths) -> Result<Vec<Value>> {
    let registry = read(
        &paths.config.join("ai/projects.json"),
        json!({"projects":[]}),
    )?;
    Ok(registry["projects"]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .filter(|e| {
                    e["path"].as_str().is_some_and(|p| {
                        Path::new(p).is_absolute() && Path::new(p).is_dir() && p != "/"
                    })
                })
                .take(100)
                .cloned()
                .collect()
        })
        .unwrap_or_default())
}
pub fn roots(paths: &Paths) -> Result<Vec<PathBuf>> {
    let config = configuration(paths)?;
    let mut roots: Vec<_> = ["Documents", "Downloads", "Pictures", "Desktop", "Projects"]
        .iter()
        .map(|p| paths.home.join(p))
        .collect();
    for entry in config["roots"].as_array().unwrap() {
        let path = entry
            .as_str()
            .ok_or("File roots must be absolute directories")?;
        if !Path::new(path).is_absolute() || path == "/" {
            return Err("Choose a specific absolute search folder".into());
        }
        roots.push(PathBuf::from(path));
    }
    roots.extend(
        projects(paths)?
            .iter()
            .filter_map(|e| e["path"].as_str().map(PathBuf::from)),
    );
    let mut resolved = Vec::<PathBuf>::new();
    for path in roots {
        if let Ok(path) = path.canonicalize() {
            if path.is_dir() && !resolved.iter().any(|p| path.starts_with(p)) {
                resolved.retain(|p| !p.starts_with(&path));
                resolved.push(path);
            }
        }
    }
    Ok(resolved)
}
fn checked_path(paths: &Paths, value: &str) -> Result<PathBuf> {
    let path = Path::new(value).canonicalize()?;
    let mut allowed = roots(paths)?;
    allowed.extend(
        std::env::var_os("NACRE_BRAIN")
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.home.join("Documents/Nacre-Brain"))
            .canonicalize()
            .ok(),
    );
    allowed.extend(
        [paths.recordings(), paths.screenshots()]
            .into_iter()
            .filter_map(|p| p.canonicalize().ok()),
    );
    if !allowed.iter().any(|r| path.starts_with(r)) {
        return Err("File is outside your launcher search folders".into());
    }
    Ok(path)
}
fn preview(paths: &Paths, value: &str) -> Result<Value> {
    let path = checked_path(paths, value)?;
    let meta = fs::metadata(&path)?;
    if !meta.is_file() {
        return Ok(json!({"text":"Folder","bytes":meta.len()}));
    }
    let ext = path
        .extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    // Even a deliberately broad root must not preview hidden credentials.
    let relative = roots(paths)?
        .into_iter()
        .find_map(|r| path.strip_prefix(r).ok().map(Path::to_path_buf))
        .unwrap_or_else(|| path.file_name().map(PathBuf::from).unwrap_or_default());
    let hidden = relative
        .components()
        .any(|part| part.as_os_str().to_string_lossy().starts_with('.'));
    if hidden || ["pem", "key", "p12", "pfx", "kdbx"].contains(&ext.as_str()) {
        return Ok(json!({"text":"Preview unavailable for private files","bytes":meta.len()}));
    }
    if ["png", "jpg", "jpeg", "webp", "gif", "bmp"].contains(&ext.as_str())
        && meta.len() <= 16_777_216
    {
        return Ok(json!({"image":file_uri(&path),"bytes":meta.len()}));
    }
    if meta.len() <= 1_048_576
        && [
            "txt", "md", "rs", "py", "js", "qml", "json", "toml", "yaml", "yml", "csv", "log",
            "html", "css", "sh", "c", "h",
        ]
        .contains(&ext.as_str())
    {
        let mut bytes = Vec::new();
        fs::File::open(&path)?.take(8192).read_to_end(&mut bytes)?;
        let text = String::from_utf8_lossy(&bytes)
            .chars()
            .filter(|c| !c.is_control() || ['\n', '\t'].contains(c))
            .take(2000)
            .collect::<String>();
        return Ok(json!({"text":text,"bytes":meta.len()}));
    }
    Ok(json!({"text":"","bytes":meta.len()}))
}
fn save(paths: &Paths, kind: &str, data: &Value) -> Result<Value> {
    let _guard = menu_registry::registry_lock(paths, "launcher-workflows")?;
    let mut config = configuration(paths)?;
    if kind == "roots" {
        let roots = parse_arguments(text(data, "roots", 16384, false)?)?;
        let entries = roots
            .as_array()
            .filter(|v| v.len() <= 32)
            .ok_or("Folders must be a JSON array of absolute paths")?;
        for entry in entries {
            let path = entry.as_str().ok_or("Invalid folder")?;
            if !Path::new(path).is_absolute() || path == "/" || !Path::new(path).is_dir() {
                return Err("Each folder must be a specific existing absolute directory".into());
            }
        }
        config["roots"] = roots;
    } else {
        let id = uuid::Uuid::new_v4().simple().to_string();
        let mut entry = json!({"id":id,"type":kind,"label":text(data,"label",100,true)?,"icon":text(data,"icon",80,false)?});
        match kind {
            "action" => {
                let executable = text(data, "executable", 4096, true)?;
                let mut arguments = vec![json!(executable)];
                let extra = parse_arguments(text(data, "arguments", 65536, false)?)?;
                arguments.extend(
                    extra
                        .as_array()
                        .ok_or("Arguments must be a JSON array, for example [\"--flag\"]")?
                        .iter()
                        .cloned(),
                );
                entry["arguments"] = json!(arguments);
                entry["directory"] = json!(text(data, "directory", 4096, false)?);
            }
            "snippet" => entry["text"] = json!(text(data, "text", 65536, true)?),
            "search" | "bookmark" => entry["url"] = json!(text(data, "url", 4096, true)?),
            "workspace" => {
                entry["directory"] = json!(text(data, "directory", 4096, true)?);
                entry["url"] = json!(text(data, "url", 4096, false)?);
                entry["editor"] = parse_arguments(text(data, "editor", 65536, true)?)?;
                entry["terminal"] = parse_arguments(text(data, "terminal", 65536, true)?)?;
            }
            _ => return Err("Unknown launcher workflow type".into()),
        }
        validate(&entry)?;
        config["entries"].as_array_mut().unwrap().push(entry);
        if config["entries"].as_array().unwrap().len() > 300 {
            return Err("Too many launcher workflows".into());
        }
    }
    if crate::json::encode(&config)?.len() > 2_097_152 {
        return Err("Launcher data is too large; remove an item before adding more".into());
    }
    files::json_write(&paths.config.join("launcher-workflows.json"), &config)?;
    Ok(json!({"message":"Saved"}))
}
fn load(paths: &Paths) -> Result<Value> {
    let config = configuration(paths)?;
    let recording = read(&paths.state.join("menu/recording.json"), json!({}))?;
    let active = crate::systemd::active("nacre-screen-recording.service").unwrap_or(false);
    if !active {
        cleanup_audio(paths)?;
    }
    let captures = [paths.screenshots(), paths.recordings()]
        .into_iter()
        .flat_map(|folder| fs::read_dir(folder).into_iter().flatten().flatten())
        .take(2048)
        .filter_map(|e| {
            let meta = e.metadata().ok()?;
            if !meta.is_file() {
                return None;
            }
            Some((meta.modified().ok()?, e.path()))
        })
        .collect::<Vec<_>>();
    let mut captures = captures;
    captures.sort_by_key(|a| std::cmp::Reverse(a.0));
    let captures = captures
        .into_iter()
        .filter(|(_, p)| !active || p != Path::new(recording["path"].as_str().unwrap_or("")))
        .take(8)
        .map(|(_, p)| json!({"path":p,"name":p.file_name().unwrap_or_default().to_string_lossy()}))
        .collect::<Vec<_>>();
    Ok(
        json!({"ready":true,"capabilities":{"dictation":available("voxtype"),"share":available("localsend"),"annotate":available("satty"),"convert":available("ffmpeg")},"entries":config["entries"],"roots":config["roots"],"projects":projects(paths)?,"recording":active,"recordingStarted":recording["started"],"captures":captures,"brainRoot":std::env::var("NACRE_BRAIN").unwrap_or_else(|_|paths.home.join("Documents/Nacre-Brain").to_string_lossy().into_owned())}),
    )
}
fn remove(paths: &Paths, id: &str) -> Result<Value> {
    let _guard = menu_registry::registry_lock(paths, "launcher-workflows")?;
    let mut config = configuration(paths)?;
    config["entries"]
        .as_array_mut()
        .unwrap()
        .retain(|e| e["id"] != id);
    files::json_write(&paths.config.join("launcher-workflows.json"), &config)?;
    Ok(json!({"message":"Removed"}))
}
fn executable_available(name: &str, cwd: Option<&Path>) -> bool {
    use std::os::unix::fs::PermissionsExt;
    if !name.contains('/') {
        return available(name);
    }
    let path = Path::new(name);
    let base = cwd
        .map(Path::to_path_buf)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base.join(path)
    };
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}
fn file_uri(path: &Path) -> String {
    format!(
        "file://{}",
        path.to_string_lossy()
            .bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            })
            .collect::<String>()
    )
}
fn launch(args: &[String], cwd: Option<&Path>) -> Result<()> {
    // The same clean launch environment used by AppLaunch; never a shell command.
    if args
        .first()
        .is_none_or(|name| !executable_available(name, cwd))
    {
        return Err("Install the selected application before using this action".into());
    }
    let mut command = std::process::Command::new("systemd-run");
    command.args([
        "--user",
        "--scope",
        "--collect",
        "--quiet",
        "env",
        "-u",
        "LD_LIBRARY_PATH",
        "-u",
        "QML_IMPORT_PATH",
        "-u",
        "QML2_IMPORT_PATH",
        "-u",
        "QT_PLUGIN_PATH",
        "-u",
        "NACRE_LIB_DIR",
    ]);
    command.args(args);
    if let Some(path) = cwd {
        command.current_dir(path);
    }
    command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    Ok(())
}
fn workspace(paths: &Paths, entry: &Value) -> Result<Value> {
    let directory = Path::new(
        entry["directory"]
            .as_str()
            .ok_or("Missing project folder")?,
    )
    .canonicalize()?;
    if !directory.is_dir() {
        return Err("Project folder is unavailable".into());
    }
    let key = format!(
        "{:x}",
        Sha256::digest(directory.as_os_str().as_encoded_bytes())
    );
    let mut started = false;
    for tool in ["editor", "terminal"] {
        let unit = format!("nacre-workspace-{}-{tool}.service", &key[..16]);
        if crate::systemd::active(&unit)? {
            continue;
        }
        let arguments = argv(&entry[tool])?
            .into_iter()
            .map(|v| v.replace("{project}", &directory.to_string_lossy()))
            .collect::<Vec<_>>();
        if !executable_available(&arguments[0], Some(&directory)) {
            return Err("Install the workspace editor and terminal, or save a recipe using your own applications".into());
        }
        let mut args = vec![
            "--user".into(),
            "--collect".into(),
            "--quiet".into(),
            format!("--unit={unit}"),
            format!("--working-directory={}", directory.display()),
            "--".into(),
        ];
        args.extend(arguments);
        process::checked("systemd-run", &args, None, Duration::from_secs(5))?;
        started = true;
    }
    if started && entry["url"].as_str().is_some_and(|v| !v.is_empty()) {
        launch(
            &["xdg-open".into(), entry["url"].as_str().unwrap().into()],
            None,
        )?;
    }
    let _ = paths;
    Ok(json!({"message":if started {"Workspace opened"} else {"Workspace is already open"}}))
}
fn activate(paths: &Paths, id: &str, query: &str) -> Result<Value> {
    let config = configuration(paths)?;
    let generated;
    let entry = if let Some(project_id) = id.strip_prefix("project:") {
        let project = projects(paths)?
            .into_iter()
            .find(|p| p["id"] == project_id)
            .ok_or("Project is no longer registered")?;
        generated = json!({"directory":project["path"],"editor":["code","--new-window","--wait","{project}"],"terminal":["kitty","--directory","{project}"],"url":"","type":"workspace"});
        &generated
    } else {
        config["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["id"] == id)
            .ok_or("Launcher action is no longer available")?
    };
    match entry["type"].as_str().unwrap_or("") {
        "action" => {
            let cwd = entry["directory"]
                .as_str()
                .filter(|v| !v.is_empty())
                .map(Path::new);
            launch(&argv(&entry["arguments"])?, cwd)?;
        }
        "snippet" => {
            process::checked(
                "wl-copy",
                &["--type".into(), "text/plain;charset=utf-8".into()],
                entry["text"].as_str(),
                Duration::from_secs(5),
            )?;
        }
        "bookmark" | "search" => {
            let encoded = query
                .bytes()
                .map(|b| {
                    if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    }
                })
                .collect::<String>();
            let url = entry["url"].as_str().unwrap().replace("{query}", &encoded);
            launch(&["xdg-open".into(), url], None)?;
        }
        "workspace" => return workspace(paths, entry),
        _ => return Err("Unknown launcher action".into()),
    }
    Ok(json!({"message":""}))
}
fn file_action(paths: &Paths, action: &str, value: &str) -> Result<Value> {
    let path = checked_path(paths, value)?;
    let name = path.to_string_lossy().into_owned();
    match action {
        "reveal" => {
            let folder = if path.is_dir() {
                &path
            } else {
                path.parent().ok_or("Missing folder")?
            };
            launch(
                &["xdg-open".into(), folder.to_string_lossy().into_owned()],
                None,
            )?;
        }
        "share" => {
            launch(&["localsend".into(), name], None)?;
        }
        "copy-file" => {
            let uri = format!(
                "file://{}",
                name.bytes()
                    .map(|b| if b.is_ascii_alphanumeric() || b"/-._~".contains(&b) {
                        (b as char).to_string()
                    } else {
                        format!("%{b:02X}")
                    })
                    .collect::<String>()
            );
            process::checked(
                "wl-copy",
                &["--type".into(), "text/uri-list".into()],
                Some(&format!("{uri}\r\n")),
                Duration::from_secs(5),
            )?;
        }
        "annotate" => {
            launch(&["satty".into(), "--filename".into(), name], None)?;
        }
        "compress" | "gif" | "image-png" | "image-jpeg" => {
            return convert_file(
                paths,
                &path,
                action,
                &chrono::Local::now().format("%Y%m%d_%H%M%S_%6f").to_string(),
                &mut |args| {
                    process::checked("ffmpeg", args, None, Duration::from_secs(300)).map(|_| ())
                },
            );
        }
        _ => return Err("Unknown file action".into()),
    }
    Ok(json!({"message":""}))
}
fn convert_file(
    paths: &Paths,
    path: &Path,
    action: &str,
    stamp: &str,
    run: &mut impl FnMut(&[String]) -> Result<()>,
) -> Result<Value> {
    use std::io;
    use std::os::unix::fs::PermissionsExt;
    let extension = match action {
        "gif" => "gif",
        "image-png" => "png",
        "image-jpeg" => "jpg",
        _ => "mp4",
    };
    let output = path.with_file_name(format!(
        "{}_{stamp}.{extension}",
        path.file_stem().unwrap_or_default().to_string_lossy()
    ));
    let cache = paths.cache.join("menu/conversions");
    fs::create_dir_all(&cache)?;
    fs::set_permissions(&cache, fs::Permissions::from_mode(0o700))?;
    let temporary = tempfile::tempdir_in(cache)?;
    let converted = temporary.path().join(format!("result.{extension}"));
    let mut args = vec![
        "-nostdin".into(),
        "-n".into(),
        "-i".into(),
        path.to_string_lossy().into_owned(),
    ];
    if action == "compress" {
        args.extend(
            [
                "-c:v",
                "libx264",
                "-crf",
                "28",
                "-preset",
                "fast",
                "-c:a",
                "aac",
                "-movflags",
                "+faststart",
            ]
            .iter()
            .map(|v| v.to_string()),
        );
    }
    if action == "gif" {
        args.extend([
            "-vf".into(),
            "fps=12,scale=640:-1:flags=lanczos,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse"
                .into(),
        ]);
    }
    args.push(converted.to_string_lossy().into_owned());
    run(&args)?;
    let mut published =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("Missing output folder")?)?;
    io::copy(&mut fs::File::open(converted)?, published.as_file_mut())?;
    published.as_file().sync_all()?;
    published.persist_noclobber(&output)?;
    Ok(json!({"message":"Converted file saved alongside the original","path":output}))
}
fn follow_up(path: &Path) -> Result<()> {
    if path.is_file() {
        let binary = std::env::current_exe()?;
        launch(
            &[
                binary.to_str().ok_or("Helper path is not UTF-8")?.into(),
                "launcher-workflows".into(),
                "notify".into(),
                path.to_string_lossy().into_owned(),
            ],
            None,
        )?;
    }
    Ok(())
}
fn notify_capture(paths: &Paths, value: &str) -> Result<Value> {
    let path = checked_path(paths, value)?;
    let video = path.extension().is_some_and(|v| v == "mp4");
    let mut args = vec![
        "-a".into(),
        "Nacre Capture".into(),
        "-i".into(),
        "nacre-notifications".into(),
        "--wait".into(),
        "-t".into(),
        "15000".into(),
    ];
    for (action, label) in [
        ("open", tr("launcher.capture.open", "Open capture")),
        ("reveal", tr("launcher.workflows.reveal", "Show folder")),
        ("copy-file", tr("launcher.workflows.copy-file", "Copy file")),
    ] {
        args.extend(["-A".into(), format!("{action}={label}")]);
    }
    if video && available("ffmpeg") {
        args.extend([
            "-A".into(),
            format!(
                "compress={}",
                tr("launcher.workflows.compress", "Compress video")
            ),
        ]);
    }
    if !video && available("satty") {
        args.extend([
            "-A".into(),
            format!(
                "annotate={}",
                tr("launcher.workflows.annotate", "Annotate image")
            ),
        ]);
    }
    args.extend([
        tr("launcher.capture.saved", "Capture saved"),
        path.file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    ]);
    let result = process::checked("notify-send", &args, None, Duration::from_secs(30))?;
    let selected = String::from_utf8(result.stdout)?.trim().to_string();
    if selected == "open" {
        launch(
            &["xdg-open".into(), path.to_string_lossy().into_owned()],
            None,
        )?;
    } else if ["reveal", "copy-file", "compress", "annotate"].contains(&selected.as_str()) {
        return file_action(paths, &selected, value);
    }
    Ok(json!({"message":""}))
}
fn pactl(args: &[String]) -> Result<String> {
    let result = process::checked("pactl", args, None, Duration::from_secs(5))?;
    Ok(String::from_utf8(result.stdout)?.trim().to_string())
}
fn owned_audio_module(module: &Value, id: &Value, key: &str) -> bool {
    let sink = format!("sink={key}");
    let null = format!("sink_name={key}");
    module["index"] == *id
        && ["module-null-sink", "module-loopback"].contains(&module["name"].as_str().unwrap_or(""))
        && module["argument"].as_str().is_some_and(|args| {
            args.split_whitespace()
                .any(|part| part == sink || part == null)
        })
}
fn audio_modules(listing: &str) -> Vec<Value> {
    listing
        .lines()
        .filter_map(|line| {
            let mut fields = line.splitn(4, '\t');
            let index = fields.next()?.parse::<u64>().ok()?;
            let name = fields.next()?;
            let argument = fields.next()?;
            Some(json!({"index":index,"name":name,"argument":argument}))
        })
        .collect()
}
fn unload_audio(key: &str, ids: &[Value]) -> Result<()> {
    let valid = key
        .strip_prefix("nacre_record_")
        .is_some_and(|v| v.len() == 32 && v.bytes().all(|b| b.is_ascii_hexdigit()));
    if !valid {
        return Err("Invalid recording audio ownership".into());
    }
    // Some Pulse clients omit IDs from the verbose JSON module listing. The
    // short listing supplies the real IDs; retain the type and exact sink guard.
    let modules = audio_modules(&pactl(&["list".into(), "short".into(), "modules".into()])?);
    for id in ids.iter().rev() {
        if modules.iter().any(|m| owned_audio_module(m, id, key)) {
            pactl(&["unload-module".into(), id.to_string()])?;
        }
    }
    Ok(())
}
fn cleanup_audio(paths: &Paths) -> Result<()> {
    let _guard = menu_registry::registry_lock(paths, "recording-audio")?;
    let data = read(&paths.state.join("menu/recording-audio.json"), json!({}))?;
    if let (Some(key), Some(ids)) = (data["key"].as_str(), data["modules"].as_array()) {
        unload_audio(key, ids)?;
        fs::remove_file(paths.state.join("menu/recording-audio.json"))?;
    }
    Ok(())
}
fn record_start(paths: &Paths, screen: bool, data: &Value) -> Result<Value> {
    let _guard = menu_registry::registry_lock(paths, "recording-options")?;
    if crate::systemd::active("nacre-screen-recording.service")? {
        return Err("A Nacre recording is already running".into());
    }
    cleanup_audio(paths)?;
    let audio = data["audio"].as_str().unwrap_or("none");
    if !["none", "system", "mixed"].contains(&audio) {
        return Err("Choose a recording audio mode".into());
    }
    let key = format!("nacre_record_{}", uuid::Uuid::new_v4().simple());
    let mut modules = Vec::<Value>::new();
    let result = (|| -> Result<String> {
        let mut extra = Vec::new();
        if audio != "none" {
            let sink = pactl(&["get-default-sink".into()])?;
            if sink.is_empty() || sink.chars().any(char::is_whitespace) {
                return Err("System audio output is unavailable".into());
            }
            let device = if audio == "mixed" {
                let source = pactl(&["get-default-source".into()])?;
                if source.is_empty()
                    || source.ends_with(".monitor")
                    || source.chars().any(char::is_whitespace)
                {
                    return Err("Choose a microphone as the default input first".into());
                }
                let null = pactl(&[
                    "load-module".into(),
                    "module-null-sink".into(),
                    format!("sink_name={key}"),
                    "channels=2".into(),
                    "sink_properties=device.description=Nacre_Capture".into(),
                ])?;
                modules.push(json!(null.parse::<u64>()?));
                for source in [format!("{sink}.monitor"), source] {
                    let id = pactl(&[
                        "load-module".into(),
                        "module-loopback".into(),
                        format!("source={source}"),
                        format!("sink={key}"),
                        "latency_msec=20".into(),
                    ])?;
                    modules.push(json!(id.parse::<u64>()?));
                }
                format!("{key}.monitor")
            } else {
                format!("{sink}.monitor")
            };
            extra.extend([format!("--audio={device}"), "--audio-backend=pulse".into()]);
        }
        crate::menu_actions::record_configured(screen, &extra)
    })();
    if !modules.is_empty() {
        let guard = menu_registry::registry_lock(paths, "recording-audio")?;
        if let Err(error) = files::json_write(
            &paths.state.join("menu/recording-audio.json"),
            &json!({"key":key,"modules":modules}),
        ) {
            unload_audio(&key, &modules)?;
            return Err(error);
        }
        drop(guard);
    }
    match result {
        Ok(message) if !message.is_empty() => {
            let mut state = read(&paths.state.join("menu/recording.json"), json!({}))?;
            state["started"] = json!(chrono::Utc::now().timestamp());
            state["audio"] = json!(audio);
            files::json_write(&paths.state.join("menu/recording.json"), &state)?;
            Ok(json!({"message":message}))
        }
        Ok(_) => {
            cleanup_audio(paths)?;
            Ok(json!({"message":""}))
        }
        Err(error) => {
            cleanup_audio(paths)?;
            Err(error)
        }
    }
}
fn record_stop(paths: &Paths) -> Result<Value> {
    let _guard = menu_registry::registry_lock(paths, "recording-options")?;
    let was_recording = crate::systemd::active("nacre-screen-recording.service")?;
    let message = crate::menu_actions::record_stop()?;
    cleanup_audio(paths)?;
    let data = read(&paths.state.join("menu/recording.json"), json!({}))?;
    if was_recording {
        if let Some(path) = data["path"].as_str() {
            follow_up(Path::new(path))?;
        }
    }
    Ok(json!({"message":message}))
}
pub fn execute(arguments: &[String], input: Option<Value>) -> Result<Value> {
    let paths = Paths::environment()?;
    match arguments {
        [a, v] if a == "notify" => notify_capture(&paths, v),
        [a, v]
            if a == "capture"
                && ["capture-area", "capture-screen", "capture-window"].contains(&v.as_str()) =>
        {
            let (message, path) = crate::menu_actions::capture_saved(v)?;
            if let Some(path) = path {
                follow_up(&path)?;
            }
            Ok(json!({"message":message}))
        }
        [a, v] if a == "record" => record_start(&paths, v == "screen", &input.unwrap_or(json!({}))),
        [a, _] if a == "record-stop" => record_stop(&paths),
        [a, _] if a == "qr" => Ok(json!({"message":crate::menu_actions::read_qr_sensitive()?})),
        [a] if a == "load" => load(&paths),
        [a, v] if a == "emoji" => emoji_search(v),
        [a, v] if a == "preview" => preview(&paths, v),
        [a, v] if a == "save" => save(&paths, v, &input.ok_or("Missing form input")?),
        [a, v] if a == "remove" => remove(&paths, v),
        [a, v] if a == "activate" => activate(
            &paths,
            v,
            input
                .as_ref()
                .and_then(|v| v["query"].as_str())
                .unwrap_or(""),
        ),
        [a, v]
            if [
                "reveal",
                "share",
                "copy-file",
                "annotate",
                "compress",
                "gif",
                "image-png",
                "image-jpeg",
            ]
            .contains(&a.as_str()) =>
        {
            file_action(&paths, a, v)
        }
        _ => Err("Unsupported launcher workflow".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn emoji_search_uses_unicode_keywords_and_bounds_results() -> Result<()> {
        let happy = emoji_search("happy")?;
        assert!(happy["entries"].as_array().unwrap().len() > 1);
        assert_eq!(happy["entries"][0]["value"], "😀");
        assert_eq!(emoji_search("UK")?["entries"][0]["value"], "🇬🇧");
        assert_eq!(emoji_search("🦄")?["entries"][0]["name"], "unicorn");
        assert!(
            emoji_search("emoji happy")?["entries"]
                .as_array()
                .unwrap()
                .len()
                > 1
        );
        assert_eq!(
            emoji_search("face")?["entries"].as_array().unwrap().len(),
            100
        );
        assert!(emoji_search(&"x".repeat(161)).is_err());
        assert!(emoji_search("\0").is_err());
        Ok(())
    }
    #[test]
    fn saved_argv_is_literal_and_invalid_forms_do_not_replace_configuration() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let paths = Paths::new(dir.path());
        save(
            &paths,
            "action",
            &json!({"label":"Example","executable":"echo","arguments":"[\"$(touch bad)\",\"a b\"]","directory":"","icon":"terminal"}),
        )?;
        let before = configuration(&paths)?;
        assert_eq!(
            before["entries"][0]["arguments"],
            json!(["echo", "$(touch bad)", "a b"])
        );
        assert!(save(
            &paths,
            "search",
            &json!({"label":"Bad","url":"javascript:alert(1)"})
        )
        .is_err());
        assert_eq!(configuration(&paths)?, before);
        assert!(save(&paths, "roots", &json!({"roots":"[\"/\"]"})).is_err());
        Ok(())
    }
    #[test]
    fn conversion_failure_or_name_collision_cannot_remove_existing_files() -> Result<()> {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let original = home.path().join("movie.mp4");
        let existing = home.path().join("movie_fixture.mp4");
        fs::write(&original, b"original movie")?;
        fs::write(&existing, b"existing output")?;
        assert!(
            convert_file(&paths, &original, "compress", "fixture", &mut |_| Err(
                "converter failed".into()
            ))
            .is_err()
        );
        assert_eq!(fs::read(&original)?, b"original movie");
        assert_eq!(fs::read(&existing)?, b"existing output");
        assert!(
            convert_file(&paths, &original, "compress", "fixture", &mut |args| {
                fs::write(args.last().unwrap(), b"new movie")?;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(fs::read(&existing)?, b"existing output");
        let result = convert_file(&paths, &original, "compress", "new", &mut |args| {
            fs::write(args.last().unwrap(), b"new movie")?;
            Ok(())
        })?;
        assert_eq!(fs::read(result["path"].as_str().unwrap())?, b"new movie");
        assert_eq!(fs::read(&original)?, b"original movie");
        Ok(())
    }
    #[test]
    fn audio_cleanup_rejects_reused_identifiers_and_unrelated_modules() {
        let key = "nacre_record_00000000000000000000000000000000";
        let own = json!({"index":7,"name":"module-loopback","argument":format!("source=default sink={key} latency_msec=20")});
        assert!(owned_audio_module(&own, &json!(7), key));
        assert!(!owned_audio_module(&own, &json!(8), key));
        assert!(!owned_audio_module(
            &json!({"index":7,"name":"module-loopback","argument":"sink=other"}),
            &json!(7),
            key
        ));
        assert!(!owned_audio_module(
            &json!({"index":7,"name":"module-null-sink","argument":format!("sink_name=other description={key}")}),
            &json!(7),
            key
        ));
        assert!(!owned_audio_module(
            &json!({"index":7,"name":"module-echo-cancel","argument":format!("sink={key}")}),
            &json!(7),
            key
        ));
    }
    #[test]
    fn short_audio_module_listing_preserves_identifiers_and_argument_fields() {
        let key = "nacre_record_00000000000000000000000000000000";
        let modules = audio_modules(&format!(
            "bad\tmodule-loopback\tsink={key}\n7\tmodule-loopback\tsource=default sink={key} latency_msec=20\tn/a\n8\tmodule-null-sink\tsink_name=other\tn/a\n"
        ));
        assert_eq!(modules.len(), 2);
        assert!(owned_audio_module(&modules[0], &json!(7), key));
        assert!(!owned_audio_module(&modules[0], &json!(8), key));
        assert!(!owned_audio_module(&modules[1], &json!(8), key));
    }
    #[test]
    fn registered_projects_and_configured_roots_are_searched_without_escape() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let paths = Paths::new(dir.path());
        let project = paths.home.join("MusicProject");
        fs::create_dir_all(&project)?;
        files::json_write(
            &paths.config.join("ai/projects.json"),
            &json!({"projects":[{"id":"music","path":project}]}),
        )?;
        assert_eq!(roots(&paths)?, vec![project.clone()]);
        fs::write(project.join("readme.md"), "hello")?;
        assert_eq!(
            preview(&paths, project.join("readme.md").to_str().unwrap())?["text"],
            "hello"
        );
        let outside = tempfile::NamedTempFile::new()?;
        std::os::unix::fs::symlink(outside.path(), project.join("escape.txt"))?;
        assert!(preview(&paths, project.join("escape.txt").to_str().unwrap()).is_err());
        fs::write(project.join(".env"), "secret")?;
        assert!(
            !preview(&paths, project.join(".env").to_str().unwrap())?["text"]
                .as_str()
                .unwrap()
                .contains("secret")
        );
        Ok(())
    }
}
