use base64::{engine::general_purpose::STANDARD, Engine};
use nacre_helper::{login_appearance as login, paths::Paths, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};
fn write_input(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::create_dir_all(path.parent().ok_or("Missing parent")?)?;
    fs::write(path, bytes)?;
    fs::File::open(path)?.set_times(
        fs::FileTimes::new()
            .set_accessed(UNIX_EPOCH + Duration::from_nanos(1700000000123456789))
            .set_modified(UNIX_EPOCH + Duration::from_nanos(1700000000123456789)),
    )?;
    Ok(())
}
fn fixture() -> Result<Value> {
    Ok(serde_json::from_str(include_str!(
        "fixtures/login-appearance-reference.json"
    ))?)
}
#[test]
fn frozen_cache_pngs_stamps_modes_and_publication_guards() -> Result<()> {
    let fixture = fixture()?;
    for case in fixture["cases"].as_array().ok_or("Missing cases")? {
        let root = tempfile::tempdir()?;
        let home = root.path().join("home");
        fs::create_dir(&home)?;
        let input = STANDARD.decode(
            fixture["inputs"][case["input"].as_str().ok_or("Missing input name")?]
                .as_str()
                .ok_or("Missing input")?,
        )?;
        if case["operation"] == "prepare" {
            let path = home.join("images/input.image");
            write_input(&path, &input)?;
            let cache = home.join("cache");
            let (output, stamp) = login::prepare(&Paths::new(&home), &path, Some(&cache))?;
            assert_eq!(
                stamp.replace(root.path().to_str().ok_or("Bad root")?, "/fixture"),
                case["stamp"]
            );
            assert_eq!(
                output
                    .file_name()
                    .ok_or("Missing output name")?
                    .to_string_lossy(),
                format!("{:x}.png", Sha256::digest(stamp.as_bytes()))
            );
            assert_eq!(
                fs::read(&output)?,
                STANDARD.decode(case["png"].as_str().ok_or("Missing PNG")?)?,
                "{}",
                case["name"]
            );
            assert_eq!(
                fs::metadata(&output)?.permissions().mode() & 0o777,
                case["mode"].as_u64().ok_or("Missing mode")? as u32
            );
            assert_eq!(
                fs::metadata(&cache)?.permissions().mode() & 0o777,
                case["cache_mode"].as_u64().ok_or("Missing cache mode")? as u32
            );
            let before = fs::metadata(&output)?.modified()?;
            assert_eq!(
                login::prepare(&Paths::new(&home), &path, Some(&cache))?,
                (output.clone(), stamp)
            );
            assert_eq!(before, fs::metadata(output)?.modified()?);
        } else {
            let options = &case["options"];
            let requested = if options["staged"].as_bool().unwrap_or(false) {
                root.path().join("stage")
            } else {
                home.clone()
            };
            let path = root.path().join("input.image");
            write_input(&path, &input)?;
            let public = root.path().join("public");
            if options["public_exists"].as_bool().unwrap_or(false) {
                fs::create_dir(&public)?;
                fs::write(public.join("background.png"), b"original image")?;
            }
            let preview = options["custom_preview"]
                .as_bool()
                .unwrap_or(false)
                .then(|| root.path().join("custom-preview"));
            let policy = login::Policy {
                base_home: home.clone(),
                system_public: if options["system"].as_bool().unwrap_or(false) {
                    public.clone()
                } else {
                    PathBuf::from(login::PUBLIC)
                },
                unix_home: root.path().join("account-home"),
            };
            let result = login::publish(
                &Paths::new(&requested),
                &case["colors"],
                &path,
                preview.as_deref(),
                Some(&public),
                &policy,
            );
            let error = result
                .as_ref()
                .err()
                .map(|error| {
                    let kind = error
                        .downcast_ref::<login::LoginError>()
                        .map_or("OSError", |error| error.kind);
                    json!({"type":kind,"message":error.to_string()})
                })
                .unwrap_or(Value::Null);
            assert_eq!(error, case["error"], "{}", case["name"]);
            assert_eq!(
                result
                    .ok()
                    .map(|value| value
                        .to_string_lossy()
                        .replace(root.path().to_str().unwrap_or(""), "/fixture"))
                    .map(Value::String)
                    .unwrap_or(Value::Null),
                case["result"]
            );
            let mut replacements = vec![(
                root.path().to_string_lossy().into_owned(),
                "/fixture".to_owned(),
            )];
            for entry in walkdir::WalkDir::new(root.path()) {
                let entry = entry?;
                if entry.file_name() == "image-stamp.txt" {
                    let original = fs::read_to_string(entry.path())?;
                    let normalized =
                        original.replace(root.path().to_str().ok_or("Bad root")?, "/fixture");
                    replacements.push((
                        format!("{:x}", Sha256::digest(original.as_bytes())),
                        format!("{:x}", Sha256::digest(normalized.as_bytes())),
                    ));
                }
            }
            let normalize = |mut text: String| {
                for (old, new) in &replacements {
                    text = text.replace(old, new);
                }
                text
            };
            let mut files = serde_json::Map::new();
            for entry in walkdir::WalkDir::new(root.path()) {
                let entry = entry?;
                if entry.file_type().is_file() && entry.path() != path {
                    let bytes = fs::read(entry.path())?;
                    let content = String::from_utf8(bytes.clone())
                        .map(|text| normalize(text).into_bytes())
                        .unwrap_or(bytes);
                    files.insert(normalize(entry.path().strip_prefix(root.path())?.to_string_lossy().into_owned()),json!({"bytes":STANDARD.encode(content),"mode":entry.metadata()?.permissions().mode()&0o777}));
                }
            }
            assert_eq!(Value::Object(files), case["files"], "{}", case["name"]);
        }
    }
    Ok(())
}
#[test]
fn cached_image_is_reused_without_decoding_changed_bytes_with_same_stamp() -> Result<()> {
    let fixture = fixture()?;
    let root = tempfile::tempdir()?;
    let paths = Paths::new(root.path());
    let source = root.path().join("source.png");
    let input = STANDARD.decode(fixture["inputs"]["rgb"].as_str().ok_or("Missing input")?)?;
    write_input(&source, &input)?;
    let first = login::prepare(&paths, &source, None)?;
    let content = fs::read(&first.0)?;
    write_input(&source, &vec![0; input.len()])?;
    assert_eq!(login::prepare(&paths, &source, None)?, first);
    assert_eq!(fs::read(first.0)?, content);
    Ok(())
}

#[test]
fn actual_cli_publishes_private_state_with_python_absent_and_refuses_invalid_palette() -> Result<()>
{
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let fixture = fixture()?;
    let root = tempfile::tempdir()?;
    let home = root.path();
    let paths = Paths::new(home);
    let source = home.join("wallpaper.png");
    write_input(
        &source,
        &STANDARD.decode(fixture["inputs"]["rgb"].as_str().ok_or("Missing image")?)?,
    )?;
    let colors = json!({"surface":"aabbcc","surfaceContainer":"aabbcc","primary":"aabbcc","secondary":"aabbcc","tertiary":"aabbcc","primaryFixed":"aabbcc","onSurface":"aabbcc","onSurfaceVariant":"aabbcc","error":"aabbcc"});
    fs::create_dir_all(&paths.state)?;
    fs::write(
        paths.state.join("scheme.json"),
        json!({"colours":colors}).to_string(),
    )?;
    fs::create_dir_all(paths.state.join("wallpaper"))?;
    fs::write(
        paths.state.join("wallpaper/last.txt"),
        source.to_string_lossy().as_bytes(),
    )?;
    let environment = |command: &mut Command| {
        command
            .env_clear()
            .env("HOME", home)
            .env("PATH", home.join("no-python"))
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_STATE_HOME", home.join(".local/state"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env(
                "DBUS_SYSTEM_BUS_ADDRESS",
                format!("unix:path={}/absent-system", home.display()),
            )
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}/absent-session", home.display()),
            );
    };
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command.arg("login-appearance");
    environment(&mut command);
    let output = command.output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let preview = paths.state.join("shell/login-preview");
    let before = fs::read(preview.join("theme.conf"))?;
    let mut command = Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command
        .args(["login-appearance", "--publish"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    environment(&mut command);
    let mut child = command.spawn()?;
    let mut invalid = colors;
    invalid["primary"] = json!("bad\n[Autologin]");
    child.stdin.take().ok_or("Missing stdin")?.write_all(
        json!({"home":home,"colors":invalid,"wallpaper":source})
            .to_string()
            .as_bytes(),
    )?;
    let output = child.wait_with_output()?;
    assert_eq!(output.status.code(), Some(1));
    assert!(String::from_utf8(output.stderr)?.contains("Invalid login palette"));
    assert_eq!(fs::read(preview.join("theme.conf"))?, before);
    Ok(())
}
