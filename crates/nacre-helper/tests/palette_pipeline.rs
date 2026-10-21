//! Read-only full-publisher references captured before porting; never update here.
use nacre_helper::{classic_state as publisher, paths::Paths, Result};
use serde_json::Value;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};
fn expanded(value: &Value, strings: &Value) -> Result<Value> {
    Ok(match value {
        Value::Object(map) if map.len() == 1 && map.contains_key("$string") => {
            strings[map["$string"].as_u64().ok_or("Bad pooled string")? as usize].clone()
        }
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(key, value)| Ok((key.clone(), expanded(value, strings)?)))
                .collect::<Result<_>>()?,
        ),
        Value::Array(items) => Value::Array(
            items
                .iter()
                .map(|item| expanded(item, strings))
                .collect::<Result<_>>()?,
        ),
        _ => value.clone(),
    })
}
fn fixture() -> Result<Value> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/palette-pipeline-reference.json.gz").as_slice(),
    )
    .read_to_end(&mut raw)?;
    let packed: Value = serde_json::from_slice(&raw)?;
    assert_eq!(
        packed["payload_sha256"],
        "2deef3c487f3aa54744fac9087eab88313719be43e1adff4c83e9bf3ea1f1cae"
    );
    expanded(&packed["data"], &packed["strings"])
}
fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools")
}
#[test]
fn terminal_and_presentation_match_full_publisher_references() -> Result<()> {
    let reference = fixture()?;
    for case in reference["cases"].as_array().ok_or("Missing cases")? {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let text = |name: &str, which: &str| -> Result<String> {
            Ok(case[which][name]["text"]
                .as_str()
                .ok_or("Missing fixture file")?
                .replace("/fixture/home", home.path().to_str().ok_or("Bad home")?))
        };
        let data: Value =
            serde_json::from_str(&text(".local/state/nacre/scheme.json", "outputs")?)?;
        let prefs: Value =
            serde_json::from_str(&text(".config/nacre/wallpaper-picker.json", "inputs")?)?;
        let colors = publisher::validated_colors(&data, &assets())?;
        publisher::publish_terminal_colors(&paths, &assets(), &colors, &data)?;
        let selected = home.path().join("scene.png");
        publisher::publish_presentation(
            &paths,
            &data,
            &colors,
            selected.to_str().ok_or("Bad image")?,
            case["fixed"].as_bool().ok_or("Missing fixed flag")?,
            prefs["palettePreset"].as_str().ok_or("Missing preset")?,
            &prefs,
            reference["now_ms"].as_i64().ok_or("Missing time")?,
        )?;
        for name in [
            ".config/nacre/kitty-colors.conf",
            ".config/nacre/colors/primary",
            ".config/nacre/colors/secondary",
            ".config/nacre/colors/onsurface",
            ".config/nacre/colors/onprimary",
            ".config/nacre/colors/surface",
            ".config/nacre/colors/surfacecontainer",
            ".local/state/nacre/presentation.json",
        ] {
            assert_eq!(
                fs::read_to_string(home.path().join(name))?,
                text(name, "outputs")?,
                "{}: {name}",
                case["name"]
            );
        }
    }
    Ok(())
}

#[test]
fn gtk_qt_kde_and_icon_overlays_match_full_publisher_references() -> Result<()> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use std::os::unix::fs::{symlink, PermissionsExt};
    if std::env::var_os("NACRE_GTK_FIXTURE_CHILD").is_none() {
        let data_dirs = tempfile::tempdir()?;
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "gtk_qt_kde_and_icon_overlays_match_full_publisher_references",
            ])
            .env("NACRE_GTK_FIXTURE_CHILD", "1")
            .env("XDG_DATA_DIRS", data_dirs.path())
            .output()?;
        assert!(
            output.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let reference = fixture()?;
    for case in reference["cases"].as_array().ok_or("Missing cases")? {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let normalize = |text: &str| text.replace("/fixture/home", home.path().to_str().unwrap());
        for (relative, input) in case["inputs"].as_object().ok_or("Missing inputs")? {
            let path = home.path().join(relative);
            fs::create_dir_all(path.parent().ok_or("Missing parent")?)?;
            if let Some(link) = input["link"].as_str() {
                symlink(normalize(link), path)?;
            } else {
                let raw = if let Some(text) = input["text"].as_str() {
                    normalize(text).into_bytes()
                } else {
                    STANDARD.decode(input["base64"].as_str().ok_or("Missing bytes")?)?
                };
                fs::write(&path, raw)?;
                fs::set_permissions(
                    path,
                    fs::Permissions::from_mode(input["mode"].as_u64().ok_or("Missing mode")? as u32),
                )?;
            }
        }
        let data: Value = serde_json::from_str(&normalize(
            case["outputs"][".local/state/nacre/scheme.json"]["text"]
                .as_str()
                .ok_or("Missing scheme")?,
        ))?;
        let colors = publisher::validated_colors(&data, &assets())?;
        publisher::publish_application_colors(
            &paths,
            &assets(),
            &home.path().join("empty-system-icons"),
            &colors,
            &data,
            false,
            None,
        )?;
        for (relative, expected) in case["outputs"].as_object().ok_or("Missing outputs")? {
            if !relative.starts_with(".config/gtk-")
                && !relative.starts_with(".config/qt")
                && relative != ".config/kdeglobals"
                && relative != ".config/nacre/qt.conf"
                && !relative.starts_with(".local/share/icons/Nacre-Papirus-")
            {
                continue;
            }
            let path = home.path().join(relative);
            if let Some(link) = expected["link"].as_str() {
                assert_eq!(
                    fs::read_link(path)?,
                    PathBuf::from(normalize(link)),
                    "{}: {relative}",
                    case["name"]
                );
            } else {
                assert_eq!(
                    fs::read_to_string(&path)?,
                    normalize(expected["text"].as_str().ok_or("Missing text")?),
                    "{}: {relative}",
                    case["name"]
                );
                assert_eq!(
                    fs::metadata(&path)?.permissions().mode() & 0o777,
                    expected["mode"].as_u64().unwrap() as u32,
                    "{}: {relative}",
                    case["name"]
                );
            }
        }
    }
    Ok(())
}

#[test]
fn batched_dconf_blob_matches_glib_changeset_serialization() -> Result<()> {
    use zvariant::{OwnedValue, Value as Variant};
    let mut values = std::collections::BTreeMap::new();
    for (key, value) in [
        ("color-scheme", "prefer-dark"),
        ("cursor-theme", "breeze_cursors"),
        ("gtk-theme", "Adwaita-dark"),
        ("icon-theme", "Fixture Icons"),
    ] {
        values.insert(
            format!("/org/gnome/desktop/interface/{key}"),
            OwnedValue::try_from(Variant::from(value))?,
        );
    }
    values.insert(
        "/org/gnome/desktop/interface/cursor-size".into(),
        OwnedValue::from(24i32),
    );
    let bytes = publisher::dconf_blob(&values)?;
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/dconf-writer-reference.json"))?;
    assert_eq!(
        bytes.iter().map(|b| format!("{b:02x}")).collect::<String>(),
        reference["hex"]
    );
    Ok(())
}

#[test]
fn approved_brand_vectors_match_full_publisher_references() -> Result<()> {
    let reference = fixture()?;
    for case in reference["cases"].as_array().ok_or("Missing cases")? {
        let data: Value = serde_json::from_str(
            case["outputs"][".local/state/nacre/scheme.json"]["text"]
                .as_str()
                .ok_or("Missing scheme")?,
        )?;
        let colors = publisher::validated_colors(&data, &assets())?;
        let patterns = nacre_helper::branding::templates(&assets())?;
        let roles = nacre_helper::branding::role_colors(&colors)?;
        for (variant, name) in nacre_helper::branding::asset_names() {
            let actual = nacre_helper::branding::paint_svg(
                patterns[&variant].as_str().ok_or("Missing pattern")?,
                &roles,
            )?;
            let expected = &case["outputs"][format!(".local/share/nacre/branding/{name}")]["text"];
            assert_eq!(
                actual,
                expected.as_str().ok_or("Missing expected SVG")?,
                "{}: {variant}",
                case["name"]
            );
        }
    }
    Ok(())
}

#[test]
fn lock_configuration_matches_full_publisher_references() -> Result<()> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    let reference = fixture()?;
    for case in reference["cases"].as_array().ok_or("Missing cases")? {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let normalize = |s: &str| s.replace("/fixture/home", home.path().to_str().unwrap());
        for (relative, input) in case["inputs"]
            .as_object()
            .ok_or("Missing inputs")?
            .iter()
            .filter(|(name, _)| name.starts_with(".cache/nacre/lock-ready/"))
        {
            let path = home.path().join(relative);
            fs::create_dir_all(path.parent().unwrap())?;
            if let Some(text) = input["text"].as_str() {
                fs::write(path, normalize(text))?;
            } else {
                fs::write(
                    path,
                    STANDARD.decode(input["base64"].as_str().ok_or("Missing bytes")?)?,
                )?;
            }
        }
        let data: Value = serde_json::from_str(
            case["outputs"][".local/state/nacre/scheme.json"]["text"]
                .as_str()
                .ok_or("Missing scheme")?,
        )?;
        let colors = publisher::validated_colors(&data, &assets())?;
        let prefs: Value = serde_json::from_str(
            case["inputs"][".config/nacre/desktop.json"]["text"]
                .as_str()
                .ok_or("Missing desktop prefs")?,
        )?;
        let output = nacre_helper::lock_config::render(
            &assets(),
            &paths,
            &colors,
            &normalize("/fixture/home/scene.png"),
            &prefs,
            &paths.tools().join("lock-info.py"),
            reference["monitors"].as_array().ok_or("Missing monitors")?,
            None,
            Some(&paths.share.join("branding/nacre-lock.png")),
        )?;
        assert_eq!(
            output,
            normalize(
                case["outputs"][".config/hypr/hyprlock.conf"]["text"]
                    .as_str()
                    .ok_or("Missing lock config")?
            ),
            "{}",
            case["name"]
        );
    }
    Ok(())
}

#[test]
fn actual_branding_cli_preserves_every_raster_vector_and_text_logo() -> Result<()> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use sha2::{Digest, Sha256};
    use std::{
        io::Write,
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };
    let reference = fixture()?;
    let root = tempfile::tempdir()?;
    let binary = root.path().join("bin");
    fs::create_dir(&binary)?;
    let renderer = binary.join("rsvg-convert");
    fs::copy(
        assets().join("../../tests/parity/helpers/svg_fixture.py"),
        &renderer,
    )?;
    fs::set_permissions(&renderer, fs::Permissions::from_mode(0o755))?;
    let responses = root.path().join("svg-responses.json");
    fs::write(&responses, serde_json::to_vec(&reference["rsvg"])?)?;
    for (index, case) in reference["cases"]
        .as_array()
        .ok_or("Missing cases")?
        .iter()
        .enumerate()
    {
        let home = root.path().join(format!("home-{index}"));
        fs::create_dir(&home)?;
        let data: Value = serde_json::from_str(
            case["outputs"][".local/state/nacre/scheme.json"]["text"]
                .as_str()
                .ok_or("Missing scheme")?,
        )?;
        let colors = publisher::validated_colors(&data, &assets())?;
        let mut child = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .args(["branding", "--publish"])
            .env_clear()
            .env("HOME", &home)
            .env("PATH", &binary)
            .env("NACRE_HELPER_TOOLS", assets())
            .env("NACRE_SVG_REFERENCE", &responses)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        child.stdin.take().ok_or("Missing stdin")?.write_all(
            serde_json::to_string(&serde_json::json!({"home":home,"colors":colors}))?.as_bytes(),
        )?;
        let output = child.wait_with_output()?;
        assert!(
            output.status.success(),
            "{}: {}",
            case["name"],
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(output.stdout.is_empty());
        assert!(
            output.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let mut actual = serde_json::Map::new();
        for entry in walkdir::WalkDir::new(&home) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let raw = fs::read(path)?;
            let mut relative = path.strip_prefix(&home)?.to_string_lossy().into_owned();
            if relative.starts_with(".cache/nacre/branding/") {
                relative = format!(".cache/nacre/branding/@{:x}@.png", Sha256::digest(&raw));
            }
            let content = match String::from_utf8(raw.clone()) {
                Ok(text) => {
                    serde_json::json!({"text":text,"mode":fs::metadata(path)?.permissions().mode()&0o777})
                }
                Err(_) => {
                    serde_json::json!({"base64":STANDARD.encode(&raw),"mode":fs::metadata(path)?.permissions().mode()&0o777})
                }
            };
            actual.insert(relative, content);
        }
        let expected: serde_json::Map<_, _> = case["outputs"]
            .as_object()
            .ok_or("Missing output files")?
            .iter()
            .filter(|(name, _)| {
                name.starts_with(".cache/nacre/branding/")
                    || name.starts_with(".local/share/nacre/branding/")
                    || name.starts_with(".local/share/icons/hicolor/scalable/apps/")
            })
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect();
        assert_eq!(
            actual.len(),
            expected.len(),
            "{}: number of brand files",
            case["name"]
        );
        for (path, expected) in expected {
            assert!(
                actual.get(&path) == Some(&expected),
                "{}: brand file differs: {path}",
                case["name"]
            );
        }
    }
    Ok(())
}

#[test]
fn whole_publication_preserves_every_frozen_file_and_link() -> Result<()> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use sha2::{Digest, Sha256};
    use std::{
        os::unix::fs::{symlink, PermissionsExt},
        process::Command,
    };
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/palette-transactions-reference.json.gz").as_slice(),
    )
    .read_to_end(&mut raw)?;
    let packed: Value = serde_json::from_slice(&raw)?;
    let transactions = expanded(&packed["data"], &packed["strings"])?;
    let references = [fixture()?, transactions];
    let root = tempfile::tempdir()?;
    let historic_root = root.path().join("historical-assets");
    let historic_assets = historic_root.join("shell-tools");
    fs::create_dir_all(&historic_assets)?;
    for entry in fs::read_dir(assets())? {
        let entry = entry?;
        if entry.file_name() == "theme-targets.json" {
            fs::write(
                historic_assets.join(entry.file_name()),
                include_bytes!("fixtures/theme-targets-original-v1.json"),
            )?;
        } else if entry.file_name() != "theme-bundles" {
            symlink(entry.path(), historic_assets.join(entry.file_name()))?;
        }
    }
    symlink(
        assets().parent().unwrap().join("shell"),
        historic_root.join("shell"),
    )?;
    let binary = root.path().join("bin");
    fs::create_dir(&binary)?;
    let empty_data_dirs = root.path().join("empty-data");
    fs::create_dir(&empty_data_dirs)?;
    let renderer = binary.join("rsvg-convert");
    fs::copy(
        assets().join("../../tests/parity/helpers/svg_fixture.py"),
        &renderer,
    )?;
    fs::set_permissions(&renderer, fs::Permissions::from_mode(0o755))?;
    let responses = root.path().join("svg-responses.json");
    let helper = Path::new(env!("CARGO_BIN_EXE_nacre-helper"));
    let parity = helper
        .parent()
        .ok_or("Missing target directory")?
        .join("examples/palette-publisher-parity");
    assert!(
        parity.is_file(),
        "Build the developer palette-publisher-parity example"
    );
    for (set, reference) in references.iter().enumerate() {
        fs::write(&responses, serde_json::to_vec(&reference["rsvg"])?)?;
        for (index, case) in reference["cases"]
            .as_array()
            .ok_or("Missing cases")?
            .iter()
            .enumerate()
        {
            let home = root.path().join(format!("home-{set}-{index}"));
            fs::create_dir(&home)?;
            let replace = |text: &str| text.replace("/fixture/home", home.to_str().unwrap());
            for (relative, input) in case["inputs"].as_object().ok_or("Missing inputs")? {
                let path = home.join(relative);
                fs::create_dir_all(path.parent().ok_or("Missing parent")?)?;
                if let Some(link) = input["link"].as_str() {
                    symlink(replace(link), &path)?;
                } else {
                    let raw = if let Some(text) = input["text"].as_str() {
                        replace(text).into_bytes()
                    } else {
                        STANDARD.decode(input["base64"].as_str().ok_or("Missing bytes")?)?
                    };
                    fs::write(&path, raw)?;
                    fs::set_permissions(
                        &path,
                        fs::Permissions::from_mode(
                            input["mode"].as_u64().ok_or("Missing mode")? as u32
                        ),
                    )?;
                }
            }
            // The original producer fixes this input mtime before login cache stamps
            // are computed. Replaying bytes without the same stat input changes it.
            for filename in ["scene.png", "next.png"] {
                let file = home.join(filename);
                if file.exists() {
                    fs::File::open(file)?.set_times(fs::FileTimes::new().set_modified(
                        std::time::UNIX_EPOCH
                            + std::time::Duration::from_nanos(1700000000123456789),
                    ))?;
                }
            }
            let targets: Value = serde_json::from_str(
                case["outputs"]
                    .get(".local/state/nacre/theme-targets.json")
                    .unwrap_or(
                        &references[0]["cases"][0]["outputs"]
                            [".local/state/nacre/theme-targets.json"],
                    )["text"]
                    .as_str()
                    .ok_or("Missing target manifest")?,
            )?;
            for target in targets["targets"].as_array().ok_or("Missing targets")? {
                if target["available"] == true {
                    if let Some(name) = target["tool"].as_str() {
                        let stub = binary.join(name);
                        fs::write(
                            &stub,
                            "#!/bin/sh\necho 'Unexpected external desktop action' >&2\nexit 99\n",
                        )?;
                        fs::set_permissions(stub, fs::Permissions::from_mode(0o755))?;
                    }
                }
            }
            let mut request = serde_json::json!({"home":home,"backend":case.get("backend").cloned().unwrap_or(serde_json::json!("hyprland")),"assets":historic_assets,"system_icons":home.join("empty-system-icons"),"monitors":reference["monitors"],"now_ms":reference["now_ms"],"wallpaper":home.join("scene.png")});
            if let Some(transaction) = case["request"].as_object() {
                for (key, value) in transaction {
                    let value: Value = serde_json::from_str(
                        &serde_json::to_string(value)?
                            .replace("/fixture/home", home.to_str().unwrap()),
                    )?;
                    request[key] = value;
                }
            }
            let output = Command::new(&parity)
                .arg(serde_json::to_string(&request)?)
                .env_clear()
                .env("HOME", &home)
                .env("PATH", &binary)
                .env("XDG_DATA_DIRS", &empty_data_dirs)
                .env("NACRE_SVG_REFERENCE", &responses)
                .output()?;
            if case["request"].is_null() {
                assert!(
                    output.status.success(),
                    "{}: {}",
                    case["name"],
                    String::from_utf8_lossy(&output.stderr)
                );
                assert!(output.stdout.is_empty());
            } else {
                let reply: Value = serde_json::from_slice(&output.stdout)?;
                assert_eq!(
                    output.status.success(),
                    case["error"].is_null(),
                    "{}: {}",
                    case["name"],
                    String::from_utf8_lossy(&output.stderr)
                );
                let reply: Value = serde_json::from_str(
                    &serde_json::to_string(&reply)?
                        .replace(home.to_str().unwrap(), "/fixture/home"),
                )?;
                assert_eq!(reply["result"], case["result"], "{}", case["name"]);
                assert_eq!(reply["error"], case["error"], "{}", case["name"]);
            }
            assert!(
                output.stderr.is_empty(),
                "{}: {}",
                case["name"],
                String::from_utf8_lossy(&output.stderr)
            );
            let mut aliases = Vec::new();
            for entry in walkdir::WalkDir::new(&home) {
                let entry = entry?;
                let path = entry.path();
                if entry.file_type().is_file()
                    && path.extension().is_some_and(|e| e == "png")
                    && path.file_stem().is_some_and(|s| s.len() == 64)
                    && path
                        .parent()
                        .and_then(Path::file_name)
                        .is_some_and(|name| name == "branding" || name == "login-images")
                {
                    aliases.push((
                        path.file_stem().unwrap().to_string_lossy().into_owned(),
                        format!("@{:x}@", Sha256::digest(fs::read(path)?)),
                    ));
                }
            }
            let normalize = |text: &str| {
                let mut value = text.replace(home.to_str().unwrap(), "/fixture/home");
                for (old, new) in &aliases {
                    value = value.replace(old, new);
                }
                value
            };
            let mut actual = serde_json::Map::new();
            for entry in walkdir::WalkDir::new(&home) {
                let entry = entry?;
                let path = entry.path();
                if entry.file_type().is_dir() {
                    continue;
                }
                let key = normalize(&path.strip_prefix(&home)?.to_string_lossy());
                let value = if entry.file_type().is_symlink() {
                    serde_json::json!({"link":normalize(&fs::read_link(path)?.to_string_lossy())})
                } else {
                    let raw = fs::read(path)?;
                    match String::from_utf8(raw.clone()) {
                        Ok(text) => {
                            serde_json::json!({"text":normalize(&text),"mode":fs::metadata(path)?.permissions().mode()&0o777})
                        }
                        Err(_) => {
                            serde_json::json!({"base64":STANDARD.encode(&raw),"mode":fs::metadata(path)?.permissions().mode()&0o777})
                        }
                    }
                };
                actual.insert(key, value);
            }
            let expected = case["outputs"].as_object().ok_or("Missing outputs")?;
            assert_eq!(
                actual.len(),
                expected.len(),
                "{}: output file count; extra {:?}; missing {:?}",
                case["name"],
                actual
                    .keys()
                    .filter(|name| !expected.contains_key(*name))
                    .collect::<Vec<_>>(),
                expected
                    .keys()
                    .filter(|name| !actual.contains_key(*name))
                    .collect::<Vec<_>>()
            );
            for (path, value) in expected {
                if actual.get(path) != Some(value) {
                    if path.ends_with("theme-targets.json") {
                        let got: Value =
                            serde_json::from_str(actual[path]["text"].as_str().unwrap())?;
                        let want: Value = serde_json::from_str(value["text"].as_str().unwrap())?;
                        for (a, b) in got["targets"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .zip(want["targets"].as_array().unwrap())
                        {
                            if a != b {
                                return Err(
                                    format!("Theme target differs: got {a}; expected {b}").into()
                                );
                            }
                        }
                        return Err(format!(
                            "Theme metadata differs: mode got{}, expected{}; texts equal{}",
                            actual[path]["mode"],
                            value["mode"],
                            got == want
                        )
                        .into());
                    }
                    let got = actual
                        .get(path)
                        .and_then(|v| v["text"].as_str())
                        .map(|v| v.chars().take(300).collect::<String>());
                    let want = value["text"]
                        .as_str()
                        .map(|v| v.chars().take(300).collect::<String>());
                    return Err(format!(
                    "Publication {} differs: {path}; text prefix got {got:?}, expected {want:?}",
                    case["name"]
                )
                    .into());
                }
            }
        }
    }
    Ok(())
}

#[test]
fn legacy_json_byte_format_and_sorted_hash_inputs_match_reference() -> Result<()> {
    let cases: Value = serde_json::from_str(include_str!("fixtures/json-dumps-reference.json"))?;
    for case in cases.as_array().ok_or("Missing JSON cases")? {
        assert_eq!(
            nacre_helper::json::dumps(&case["value"], false)?,
            case["plain"]
        );
        assert_eq!(
            nacre_helper::json::dumps(&case["value"], true)?,
            case["sorted"]
        );
    }
    Ok(())
}

#[test]
fn formatted_state_writes_match_frozen_legacy_bytes_backups_and_modes() -> Result<()> {
    use base64::{engine::general_purpose::STANDARD, Engine};
    use std::os::unix::fs::PermissionsExt;
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/state-text-reference.json"))?;
    for case in reference["cases"]
        .as_array()
        .ok_or("Missing state-text cases")?
    {
        let home = tempfile::tempdir()?;
        let target = home.path().join(case["path"].as_str().unwrap());
        if let Some(before) = case["before"].as_str() {
            fs::create_dir_all(target.parent().unwrap())?;
            fs::write(&target, before)?;
        }
        assert_eq!(
            nacre_helper::state::write_text(&target, case["text"].as_str().unwrap())?,
            case["result"].as_bool().unwrap()
        );
        let mut count = 0;
        for entry in walkdir::WalkDir::new(home.path()) {
            let entry = entry?;
            if !entry.file_type().is_file() {
                continue;
            }
            count += 1;
            let path = entry.path();
            let relative = path.strip_prefix(home.path())?.to_string_lossy();
            let expected = &case["files"][relative.as_ref()];
            assert_eq!(
                STANDARD.encode(fs::read(path)?),
                expected["base64"],
                "{} {relative}",
                case["name"]
            );
            assert_eq!(
                fs::metadata(path)?.permissions().mode() & 0o777,
                expected["mode"].as_u64().unwrap() as u32
            );
        }
        assert_eq!(count, case["files"].as_object().unwrap().len());
    }
    Ok(())
}

#[test]
fn concurrent_main_gradient_preference_matches_its_original_python_outputs() -> Result<()> {
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/palette-controls-reference.json"))?;
    for case in reference["cases"]
        .as_array()
        .ok_or("Missing gradient cases")?
    {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let backend = nacre_helper::compositor::backend::Backend::new(
            paths.clone(),
            [(
                "NACRE_COMPOSITOR".into(),
                case["backend"].as_str().ok_or("Missing backend")?.into(),
            )]
            .into_iter()
            .collect(),
        )?;
        let publisher = publisher::Publisher {
            paths: paths.clone(),
            assets: assets(),
            backend,
            system_icons: home.path().join("absent-icons"),
            monitors: Some(Vec::new()),
            now_ms: None,
        };
        fs::create_dir_all(&paths.config)?;
        for row in case["rows"].as_array().ok_or("Missing preference values")? {
            let preferences = if row["value"].is_null() {
                serde_json::json!({})
            } else {
                serde_json::json!({"interfaceGradients":row["value"]})
            };
            fs::write(
                paths.config.join("desktop.json"),
                serde_json::to_vec(&preferences)?,
            )?;
            let text = publisher.publish_compositor_palette(
                reference["colors"].as_object().ok_or("Missing colors")?,
            )?;
            assert_eq!(text, row["text"].as_str().ok_or("Missing reference text")?);
            assert_eq!(
                fs::read_to_string(publisher.backend.config_path("palette")?)?,
                text
            );
        }
    }
    Ok(())
}

#[test]
fn app_followers_hold_current_colors_and_resume_only_the_selected_group() -> Result<()> {
    use serde_json::json;
    let home = tempfile::tempdir()?;
    let paths = Paths::new(home.path());
    let backend = nacre_helper::compositor::backend::Backend::new(
        paths.clone(),
        [("NACRE_COMPOSITOR".into(), "hyprland".into())]
            .into_iter()
            .collect(),
    )?;
    let mut publisher = publisher::Publisher {
        paths: paths.clone(),
        assets: assets(),
        backend,
        system_icons: home.path().join("no-icons"),
        monitors: Some(Vec::new()),
        now_ms: None,
    };
    fs::create_dir_all(&paths.state)?;
    fs::create_dir_all(&paths.config)?;
    let data: Value = serde_json::from_slice(&fs::read(assets().join("reference-style.json"))?)?;
    fs::write(paths.state.join("scheme.json"), serde_json::to_vec(&data)?)?;
    // Every publisher is stopped, preserving individually chosen application files.
    fs::write(
        paths.config.join("colors.json"),
        serde_json::to_vec(
            &json!({"version":1,"favorites":[],"appThemes":{"gtk3":false,"gtk4":false,"qt":false,"kde":false,"kitty":false,"rofi":false,"lock":false,"login":false}}),
        )?,
    )?;
    let outputs = [
        ("gtk3", ".config/gtk-3.0/gtk.css"),
        ("gtk4", ".config/gtk-4.0/gtk.css"),
        ("qt", ".config/nacre/qt.conf"),
        ("kde", ".config/kdeglobals"),
        ("dolphin", ".local/share/color-schemes/Nacre.colors"),
        ("kitty", ".config/nacre/kitty-colors.conf"),
        ("rofi", ".config/nacre/rofi.rasi"),
        ("lock", ".config/hypr/hyprlock.conf"),
    ];
    for (_, name) in outputs {
        let path = home.path().join(name);
        fs::create_dir_all(path.parent().unwrap())?;
        fs::write(path, "[General]\nName=Custom colors\n")?;
    }
    publisher.apply_palette(None, false)?;
    for (_, name) in outputs {
        assert_eq!(
            fs::read_to_string(home.path().join(name))?,
            "[General]\nName=Custom colors\n",
            "{name}"
        );
    }
    // Shared-output aliases follow exactly the preference shown by Settings.
    publisher.set_theme_target("thunar", true)?;
    assert!(publisher::theme_target_enabled(&paths, "gtk3")?);
    assert!(publisher::theme_target_enabled(&paths, "thunar")?);
    assert_ne!(
        fs::read_to_string(home.path().join(outputs[0].1))?,
        "[General]\nName=Custom colors\n"
    );
    for (_, name) in outputs.into_iter().skip(1) {
        assert_eq!(
            fs::read_to_string(home.path().join(name))?,
            "[General]\nName=Custom colors\n",
            "{name}"
        );
    }
    publisher.set_theme_target("dolphin", true)?;
    assert!(publisher::theme_target_enabled(&paths, "kde")?);
    assert_ne!(
        fs::read_to_string(home.path().join(".local/share/color-schemes/Nacre.colors"))?,
        "[General]\nName=Custom colors\n"
    );
    assert_eq!(
        fs::read_to_string(home.path().join(".config/nacre/qt.conf"))?,
        "[General]\nName=Custom colors\n"
    );
    publisher.set_theme_target("kde", false)?;
    assert!(!publisher::theme_target_enabled(&paths, "dolphin")?);
    let before = fs::read(paths.config.join("colors.json"))?;
    assert!(publisher.set_theme_target("../not-a-target", true).is_err());
    assert_eq!(fs::read(paths.config.join("colors.json"))?, before);
    // Newer-version preferences are never overwritten by a theme click.
    fs::write(
        paths.config.join("colors.json"),
        b"{\"version\":99,\"future\":true}",
    )?;
    assert!(publisher.set_theme_target("kitty", true).is_err());
    assert_eq!(
        fs::read(paths.config.join("colors.json"))?,
        b"{\"version\":99,\"future\":true}"
    );
    Ok(())
}

fn app_theme_home(home: &Path) -> Result<()> {
    let paths = Paths::new(home);
    fs::create_dir_all(&paths.config)?;
    fs::create_dir_all(&paths.state)?;
    fs::write(
        paths.config.join("colors.json"),
        br#"{"version":1,"favorites":[],"appThemes":{}}"#,
    )?;
    fs::copy(
        assets().join("reference-style.json"),
        paths.state.join("scheme.json"),
    )?;
    Ok(())
}

fn app_theme_command(
    home: &Path,
    path: &Path,
    id: &str,
    enabled: bool,
) -> Result<std::process::Output> {
    app_theme_command_with_data_dirs(home, path, id, enabled, None)
}

fn app_theme_command_with_data_dirs(
    home: &Path,
    path: &Path,
    id: &str,
    enabled: bool,
    data_dirs: Option<&Path>,
) -> Result<std::process::Output> {
    app_theme_command_with_data_dirs_value(home, path, id, enabled, data_dirs.map(Path::as_os_str))
}

fn app_theme_command_with_data_dirs_value(
    home: &Path,
    path: &Path,
    id: &str,
    enabled: bool,
    data_dirs: Option<&std::ffi::OsStr>,
) -> Result<std::process::Output> {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    command
        .args(["app-theme", id, if enabled { "true" } else { "false" }])
        .env_clear()
        .env("HOME", home)
        .env("PATH", path)
        .env("NACRE_HELPER_TOOLS", assets())
        .env("NACRE_COMPOSITOR", "hyprland");
    if let Some(data_dirs) = data_dirs {
        command.env("XDG_DATA_DIRS", data_dirs);
    }
    Ok(command.output()?)
}

fn fake_executable(path: &Path, body: &str) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::write(path, body)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[test]
fn app_theme_outputs_are_gated_rendered_private_and_refresh_only() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let home = tempfile::tempdir()?;
    app_theme_home(home.path())?;
    let bin = home.path().join("bin");
    fs::create_dir(&bin)?;
    let absent = app_theme_command(home.path(), &bin, "chromium", true)?;
    assert!(absent.status.success());
    assert!(!home
        .path()
        .join(".local/share/nacre/themes/chromium/manifest.json")
        .exists());
    let absent = app_theme_command(home.path(), &bin, "discord", true)?;
    assert!(absent.status.success());
    assert!(!home
        .path()
        .join(".config/Vencord/themes/nacre.theme.css")
        .exists());
    let absent = app_theme_command(home.path(), &bin, "spotify", true)?;
    assert!(absent.status.success());
    assert!(!home
        .path()
        .join(".config/spicetify/Themes/Nacre/color.ini")
        .exists());
    let status: Value = serde_json::from_slice(&fs::read(
        home.path().join(".local/state/nacre/theme-targets.json"),
    )?)?;
    for id in ["chromium", "discord", "spotify"] {
        assert_eq!(
            status["targets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|target| target["id"] == id)
                .unwrap()["status"],
            "tool-missing"
        );
    }

    fake_executable(&bin.join("google-chrome-stable"), "#!/bin/sh\nexit 0\n")?;
    let chromium_root = home.path().join(".local/share/nacre/themes/chromium");
    fs::create_dir_all(&chromium_root)?;
    fs::set_permissions(&chromium_root, fs::Permissions::from_mode(0o700))?;
    fs::write(chromium_root.join("Cached Theme.pak"), [1, 2, 0, 255])?;
    let output = app_theme_command(home.path(), &bin, "chromium", true)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest_path = home
        .path()
        .join(".local/share/nacre/themes/chromium/manifest.json");
    let manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    assert_eq!(manifest["manifest_version"], 3);
    assert_eq!(manifest["name"], "Nacre");
    assert!(manifest["version"].as_str().unwrap().starts_with("1."));
    for key in [
        "frame",
        "frame_inactive",
        "toolbar",
        "tab_text",
        "tab_background_text",
        "bookmark_text",
        "ntp_background",
        "ntp_text",
        "ntp_link",
        "omnibox_background",
        "omnibox_text",
        "button_background",
        "toolbar_button_icon",
    ] {
        assert!(manifest["theme"]["colors"][key].is_array(), "{key}");
    }
    assert!(manifest["theme"]["properties"]["ntp_logo_alternate"].is_number());
    assert!(!chromium_root.join("Cached Theme.pak").exists());
    assert_eq!(
        fs::metadata(&manifest_path)?.permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(manifest_path.parent().unwrap())?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );

    fs::create_dir_all(home.path().join(".config/Vencord"))?;
    let output = app_theme_command(home.path(), &bin, "discord", true)?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let css = fs::read_to_string(home.path().join(".config/Vencord/themes/nacre.theme.css"))?;
    assert!(!css.contains("{{"));
    assert_eq!(
        fs::metadata(home.path().join(".config/Vencord/themes/nacre.theme.css"))?
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        fs::metadata(home.path().join(".config/Vencord/themes"))?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    for variable in [
        "--background-primary",
        "--background-base-lowest",
        "--background-surface-highest",
        "--bg-base-primary",
        "--text-primary",
        "--text-default",
        "--input-border-default",
        "--control-primary-background-default",
        "--brand-500",
        "--scrollbar-thin-thumb",
    ] {
        assert!(css.contains(variable), "{variable}");
    }
    assert!(css.contains("@name Nacre"));

    let dark_version = manifest["version"].clone();
    let mut light = serde_json::from_slice::<Value>(&fs::read(
        home.path().join(".local/state/nacre/scheme.json"),
    )?)?;
    light["mode"] = serde_json::json!("light");
    fs::write(
        home.path().join(".local/state/nacre/scheme.json"),
        serde_json::to_vec(&light)?,
    )?;
    fs::write(chromium_root.join("Cached Theme.pak"), [4, 0, 5])?;
    let output = app_theme_command(home.path(), &bin, "chromium", true)?;
    assert!(output.status.success());
    let light_manifest: Value = serde_json::from_slice(&fs::read(&manifest_path)?)?;
    assert_ne!(light_manifest["version"], dark_version);
    assert!(!chromium_root.join("Cached Theme.pak").exists());
    let saved_manifest = fs::read(&manifest_path)?;
    let saved_css = fs::read(home.path().join(".config/Vencord/themes/nacre.theme.css"))?;
    assert!(app_theme_command(home.path(), &bin, "chromium", false)?
        .status
        .success());
    assert!(app_theme_command(home.path(), &bin, "discord", false)?
        .status
        .success());
    assert_eq!(fs::read(&manifest_path)?, saved_manifest);
    assert_eq!(
        fs::read(home.path().join(".config/Vencord/themes/nacre.theme.css"))?,
        saved_css
    );

    let log = home.path().join("spicetify.log");
    fake_executable(
        &bin.join("spicetify"),
        "#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$SPICETIFY_LOG\"\n",
    )?;
    fake_executable(&bin.join("timeout"), "#!/bin/sh\nshift 3\nexec \"$@\"\n")?;
    fs::create_dir_all(home.path().join(".config/spicetify"))?;
    fs::write(
        home.path().join(".config/spicetify/config-xpui.ini"),
        "current_theme = Nacre\n",
    )?;
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    let output = command
        .args(["app-theme", "spotify", "true"])
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", &bin)
        .env("NACRE_HELPER_TOOLS", assets())
        .env("NACRE_COMPOSITOR", "hyprland")
        .env("SPICETIFY_LOG", &log)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let ini_path = home.path().join(".config/spicetify/Themes/Nacre/color.ini");
    let ini = fs::read_to_string(&ini_path)?;
    assert_eq!(
        fs::metadata(ini_path.parent().unwrap())?
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    assert!(ini.starts_with("[Nacre]\n"));
    assert!(!ini.contains("{{"));
    assert_eq!(fs::metadata(&ini_path)?.permissions().mode() & 0o777, 0o600);
    for key in [
        "text",
        "subtext",
        "main",
        "sidebar",
        "player",
        "card",
        "shadow",
        "selected-row",
        "button",
        "button-active",
        "button-disabled",
        "tab-active",
        "notification",
        "notification-error",
        "misc",
    ] {
        assert!(
            ini.lines().any(|line| line.starts_with(&format!("{key} "))),
            "{key}"
        );
    }
    for _ in 0..100 {
        if log.exists() && fs::read_to_string(&log)?.contains("refresh") {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    assert_eq!(fs::read_to_string(&log)?.trim(), "refresh");
    let color_ini = fs::read(&ini_path)?;
    let user_css_path = home.path().join(".config/spicetify/Themes/Nacre/user.css");
    let user_css = fs::read(&user_css_path)?;
    assert_eq!(
        fs::metadata(&user_css_path)?.permissions().mode() & 0o777,
        0o600
    );
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_nacre-helper"));
    let output = command
        .args(["app-theme", "spotify", "false"])
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", &bin)
        .env("NACRE_HELPER_TOOLS", assets())
        .env("NACRE_COMPOSITOR", "hyprland")
        .env("SPICETIFY_LOG", &log)
        .output()?;
    assert!(output.status.success());
    assert_eq!(fs::read(&ini_path)?, color_ini);
    assert_eq!(fs::read(&user_css_path)?, user_css);
    assert_eq!(fs::read_to_string(&log)?.trim(), "refresh");
    Ok(())
}

#[test]
fn app_theme_target_failure_restores_each_target_output() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for id in ["chromium", "discord", "spotify"] {
        let home = tempfile::tempdir()?;
        app_theme_home(home.path())?;
        let bin = home.path().join("bin");
        fs::create_dir(&bin)?;
        let output_root = match id {
            "chromium" => {
                fake_executable(&bin.join("chromium"), "#!/bin/sh\nexit 0\n")?;
                home.path().join(".local/share/nacre/themes/chromium")
            }
            "discord" => {
                fs::create_dir_all(home.path().join(".config/Vencord"))?;
                home.path().join(".config/Vencord/themes")
            }
            "spotify" => {
                fake_executable(&bin.join("spicetify"), "#!/bin/sh\nexit 0\n")?;
                fs::create_dir_all(home.path().join(".config/spicetify"))?;
                fs::write(
                    home.path().join(".config/spicetify/config-xpui.ini"),
                    "current_theme=Nacre\n",
                )?;
                home.path().join(".config/spicetify/Themes/Nacre")
            }
            _ => unreachable!(),
        };
        fs::create_dir_all(&output_root)?;
        let outputs: Vec<(&str, Vec<u8>)> = match id {
            "chromium" => vec![
                ("manifest.json", b"old manifest\n".to_vec()),
                ("Cached Theme.pak", vec![0, 255, 17, 0, 128]),
            ],
            "discord" => vec![("nacre.theme.css", b"old discord theme\n".to_vec())],
            "spotify" => vec![
                ("color.ini", b"old colors\n".to_vec()),
                ("user.css", b"old css\n".to_vec()),
            ],
            _ => unreachable!(),
        };
        for (name, bytes) in &outputs {
            fs::write(output_root.join(name), bytes)?;
        }
        if id == "chromium" {
            fs::set_permissions(
                output_root.join("Cached Theme.pak"),
                fs::Permissions::from_mode(0o640),
            )?;
        }
        let before = outputs
            .iter()
            .map(|(name, _)| (name.to_string(), fs::read(output_root.join(name)).unwrap()))
            .collect::<std::collections::BTreeMap<_, _>>();
        let state_target = home.path().join(".local/state/nacre/theme-targets.json");
        fs::create_dir(state_target)?;
        let output = app_theme_command(home.path(), &bin, id, true)?;
        assert!(
            !output.status.success(),
            "{id}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        for (name, bytes) in &before {
            assert_eq!(fs::read(output_root.join(name))?, *bytes, "{id}: {name}");
        }
        if id == "chromium" {
            assert_eq!(
                fs::metadata(output_root.join("Cached Theme.pak"))?
                    .permissions()
                    .mode()
                    & 0o777,
                0o640
            );
        }
        let preferences: Value =
            serde_json::from_slice(&fs::read(home.path().join(".config/nacre/colors.json"))?)?;
        assert!(preferences["appThemes"].get(id).is_none(), "{id}");
    }
    Ok(())
}

#[test]
fn gtk_theme_discovery_uses_home_xdg_roots_and_adwaita_fallback() -> Result<()> {
    let home = tempfile::tempdir()?;
    app_theme_home(home.path())?;
    let bin = home.path().join("bin");
    fs::create_dir(&bin)?;
    let empty_data = tempfile::tempdir()?;
    let config = home.path().join(".config/gtk-3.0/settings.ini");
    fs::create_dir_all(config.parent().unwrap())?;
    fs::write(&config, "[Settings]\ngtk-theme-name=Before\n")?;
    let scheme_path = home.path().join(".local/state/nacre/scheme.json");

    let write_mode = |mode: &str| -> Result<()> {
        let mut scheme: Value = serde_json::from_slice(&fs::read(&scheme_path)?)?;
        scheme["mode"] = serde_json::json!(mode);
        fs::write(&scheme_path, serde_json::to_vec(&scheme)?)?;
        Ok(())
    };
    let publish = |dirs: &Path| -> Result<String> {
        let output = app_theme_command_with_data_dirs(home.path(), &bin, "gtk3", true, Some(dirs))?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(fs::read_to_string(&config)?)
    };

    let local_themes = home.path().join(".local/share/themes");
    fs::create_dir_all(local_themes.join("adw-gtk3"))?;
    fs::create_dir_all(local_themes.join("adw-gtk3-dark"))?;
    write_mode("light")?;
    assert!(publish(empty_data.path())?.contains("gtk-theme-name=adw-gtk3\n"));
    write_mode("dark")?;
    assert!(publish(empty_data.path())?.contains("gtk-theme-name=adw-gtk3-dark\n"));

    fs::remove_dir_all(&local_themes)?;
    let system_data = tempfile::tempdir()?;
    let system_themes = system_data.path().join("themes");
    fs::create_dir_all(system_themes.join("adw-gtk3-dark"))?;
    assert!(publish(system_data.path())?.contains("gtk-theme-name=adw-gtk3-dark\n"));
    write_mode("light")?;
    assert!(publish(system_data.path())?.contains("gtk-theme-name=Adwaita\n"));
    fs::create_dir_all(system_themes.join("adw-gtk3"))?;
    assert!(publish(system_data.path())?.contains("gtk-theme-name=adw-gtk3\n"));

    let roots = std::env::join_paths([empty_data.path(), system_data.path()])?;
    write_mode("light")?;
    let output =
        app_theme_command_with_data_dirs_value(home.path(), &bin, "gtk3", true, Some(&roots))?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(fs::read_to_string(&config)?.contains("gtk-theme-name=adw-gtk3\n"));

    write_mode("dark")?;
    assert!(publish(empty_data.path())?.contains("gtk-theme-name=Adwaita-dark\n"));

    let unset = app_theme_command(home.path(), &bin, "gtk3", true)?;
    assert!(unset.status.success());
    let unset_name = fs::read_to_string(&config)?
        .lines()
        .find(|line| line.starts_with("gtk-theme-name="))
        .unwrap()
        .to_owned();
    let empty = app_theme_command_with_data_dirs_value(
        home.path(),
        &bin,
        "gtk3",
        true,
        Some(std::ffi::OsStr::new("")),
    )?;
    assert!(empty.status.success());
    let empty_name = fs::read_to_string(&config)?
        .lines()
        .find(|line| line.starts_with("gtk-theme-name="))
        .unwrap()
        .to_owned();
    assert_eq!(unset_name, empty_name);
    Ok(())
}
