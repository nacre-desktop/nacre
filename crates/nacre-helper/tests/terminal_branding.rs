use base64::{engine::general_purpose::STANDARD, Engine};
use nacre_helper::{paths::Paths, terminal_branding as terminal, Result};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::{ffi::OsStringExt, fs::PermissionsExt},
    path::Path,
};
fn proc_files(root: &Path, rows: &Value) -> Result<()> {
    for row in rows.as_array().ok_or("Missing process rows")? {
        let path = root.join(row["pid"].to_string());
        fs::create_dir_all(&path)?;
        let mut fields = vec!["0".to_owned(); 25];
        fields[0] = "S".into();
        fields[1] = row["parent"].to_string();
        fields[4] = row["device"].to_string();
        fields[19] = row["started"].as_str().ok_or("Missing start time")?.into();
        fs::write(
            path.join("stat"),
            format!(
                "{} ({}) {}",
                row["pid"],
                row["name"].as_str().ok_or("Missing name")?,
                fields.join(" ")
            ),
        )?;
        fs::write(
            path.join("comm"),
            format!("{}\n", row["name"].as_str().ok_or("Missing name")?),
        )?;
    }
    Ok(())
}
fn normalized(data: &[u8], home: &Path) -> Vec<u8> {
    let old = STANDARD.encode(
        home.join(".local/share/nacre/branding/nacre.png")
            .as_os_str()
            .as_encoded_bytes(),
    );
    let fixed = STANDARD.encode(b"/fixture/home/.local/share/nacre/branding/nacre.png");
    String::from_utf8_lossy(data)
        .replace(&old, &fixed)
        .into_bytes()
}
#[test]
fn frozen_terminal_function_cases_preserve_packets_files_and_stale_cleanup() -> Result<()> {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/terminal-branding-reference.json"))?;
    for case in fixture["cases"].as_array().ok_or("Missing cases")? {
        let home = tempfile::tempdir()?;
        let proc = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        match case["operation"].as_str().ok_or("Missing operation")? {
            "packet" => assert_eq!(
                json!(terminal::packet(
                    &case["image"],
                    Path::new(case["path"].as_str().ok_or("Missing path")?),
                    case["edit"].as_bool().unwrap_or(false)
                )),
                case["observed"]
            ),
            "identity" => {
                proc_files(proc.path(), &json!([case["input"]]))?;
                let info = terminal::identity(500, proc.path())?;
                assert_eq!(
                    json!([info.parent, info.device, info.started]),
                    case["observed"]
                );
            }
            "register" => {
                let input = &case["input"];
                proc_files(proc.path(), &input["processes"])?;
                let meta = terminal::Terminal {
                    uid: nix::unistd::getuid().as_raw()
                        + u32::from(!input["owned"].as_bool().unwrap_or(true)),
                    character: input["character"].as_bool().unwrap_or(true),
                    inode: input["inode"].as_u64().ok_or("Missing inode")?,
                    device: input["device"].as_u64().ok_or("Missing device")?,
                };
                let record = terminal::register_at(
                    &paths,
                    proc.path(),
                    input["tty"].as_str().ok_or("Missing TTY")?,
                    &meta,
                    input["parent"]
                        .as_u64()
                        .ok_or("Missing parent")?
                        .try_into()?,
                    input["image"].as_u64().ok_or("Missing image")?.try_into()?,
                )?;
                assert_eq!(record.unwrap_or(Value::Null), case["observed"]);
            }
            "bounded" => {
                let cache = paths.cache.join("terminal-logos");
                fs::create_dir_all(&cache)?;
                for index in 0..260 {
                    fs::write(cache.join(format!("{index:03}.json")), "{}")?;
                }
                terminal::refresh(&paths, proc.path())?;
                let mut names = fs::read_dir(cache)?
                    .map(|row| row.map(|row| row.file_name().to_string_lossy().into_owned()))
                    .collect::<std::io::Result<Vec<_>>>()?;
                names.sort();
                assert_eq!(json!(names), case["observed"]);
            }
            "refresh" => {
                let master = rustix::pty::openpt(rustix::pty::OpenptFlags::RDWR)?;
                rustix::fs::fcntl_setfl(&master, rustix::fs::OFlags::NONBLOCK)?;
                rustix::pty::grantpt(&master)?;
                rustix::pty::unlockpt(&master)?;
                let tty = rustix::pty::ptsname(&master, Vec::new())?;
                let tty = std::path::PathBuf::from(std::ffi::OsString::from_vec(tty.into_bytes()));
                use std::os::unix::fs::OpenOptionsExt;
                let slave = fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .custom_flags(rustix::fs::OFlags::NOCTTY.bits().try_into()?)
                    .open(&tty)?;
                use std::os::unix::fs::MetadataExt;
                let meta = slave.metadata()?;
                let cache = paths.cache.join("terminal-logos");
                fs::create_dir_all(&cache)?;
                let mut record = json!({"pid":123,"started":"41","tty":tty,"inode":meta.ino(),"device":meta.rdev(),"image":234});
                for (key, value) in case["changes"].as_object().ok_or("Missing changes")? {
                    record[key] = if value == "increment" {
                        json!(record[key].as_u64().ok_or("Expected integer")? + 1)
                    } else if value == "string" {
                        json!(record[key].to_string())
                    } else {
                        value.clone()
                    };
                }
                proc_files(
                    proc.path(),
                    &json!([{"pid":123,"parent":1,"device":meta.rdev(),"started":"41","name":"fixture"}]),
                )?;
                let file = cache.join("test.json");
                fs::write(&file, record.to_string())?;
                terminal::refresh(&paths, proc.path())?;
                let mut data = [0u8; 4096];
                let size = match rustix::io::read(&master, &mut data[..]) {
                    Ok(size) => size,
                    Err(rustix::io::Errno::AGAIN) => 0,
                    Err(error) => return Err(error.into()),
                };
                assert_eq!(
                    file.exists(),
                    case["retained"].as_bool().ok_or("Missing retention")?,
                    "{}",
                    case["name"]
                );
                assert_eq!(
                    json!(normalized(&data[..size], home.path())),
                    case["observed"],
                    "{}",
                    case["name"]
                );
            }
            "render" => {
                let input = &case["input"];
                let art = paths.share.join("branding");
                fs::create_dir_all(&art)?;
                fs::write(art.join("nacre.png"), b"fixture-image")?;
                if let Some(text) = input["text"].as_str() {
                    fs::write(art.join("nacre-text.txt"), text)?;
                }
                fs::create_dir_all(paths.cache.join("terminal-logos"))?;
                let output = terminal::render_with(
                    &paths,
                    &serde_json::from_value(input["environment"].clone())?,
                    input["prepare"].as_bool().unwrap_or(false),
                    || Ok(Some(json!({"image":234}))),
                )?;
                assert_eq!(json!(normalized(&output, home.path())), case["observed"]);
            }
            _ => return Err("Unknown frozen operation".into()),
        }
        if let Some(expected) = case["files"].as_object() {
            let mut found = serde_json::Map::new();
            for entry in walkdir::WalkDir::new(home.path()) {
                let entry = entry?;
                if entry.file_type().is_file() {
                    let path = entry.path();
                    found.insert(path.strip_prefix(home.path())?.to_string_lossy().into_owned(),json!({"bytes":normalized(&fs::read(path)?,home.path()),"mode":fs::metadata(path)?.permissions().mode()&0o777}));
                }
            }
            assert_eq!(&found, expected, "{}", case["name"]);
        }
    }
    Ok(())
}

#[test]
fn identity_refuses_foreign_owner_and_refresh_cannot_write_arbitrary_files() -> Result<()> {
    let home = tempfile::tempdir()?;
    let proc = tempfile::tempdir()?;
    fs::create_dir(proc.path().join("123"))?;
    assert!(
        terminal::identity_for_uid(123, proc.path(), nix::unistd::getuid().as_raw() + 1).is_err()
    );
    let paths = Paths::new(home.path());
    let cache = paths.cache.join("terminal-logos");
    fs::create_dir_all(&cache)?;
    let sentinel = home.path().join("sentinel");
    fs::write(&sentinel, b"untouched")?;
    fs::write(
        cache.join("unsafe.json"),
        json!({"tty":sentinel}).to_string(),
    )?;
    terminal::refresh(&paths, proc.path())?;
    assert_eq!(fs::read(sentinel)?, b"untouched");
    assert!(!cache.join("unsafe.json").exists());
    Ok(())
}
