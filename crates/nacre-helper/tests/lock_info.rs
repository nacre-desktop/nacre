use nacre_helper::{lock_info::Info, paths::Paths, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read};

#[test]
fn command_help_and_errors_match_original_bytes_without_files() -> Result<()> {
    use std::process::Command;
    let reference: Value =
        serde_json::from_str(include_str!("fixtures/lock-info-cli-reference.json"))?;
    for case in reference["cases"].as_array().unwrap() {
        let home = tempfile::tempdir()?;
        let args = case["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap());
        let result = Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
            .arg("lock-info")
            .args(args)
            .env_clear()
            .env("HOME", home.path())
            .env("PATH", home.path())
            .env("NACRE_LANGUAGE", "en")
            .output()?;
        assert_eq!(
            String::from_utf8(result.stdout)?,
            case["stdout"].as_str().unwrap(),
            "{}",
            case["args"]
        );
        assert_eq!(
            String::from_utf8(result.stderr)?,
            case["stderr"].as_str().unwrap(),
            "{}",
            case["args"]
        );
        assert_eq!(
            result.status.code(),
            case["exit"].as_i64().map(|value| value as i32)
        );
        assert_eq!(fs::read_dir(home.path())?.count(), 0);
    }
    Ok(())
}

#[test]
fn labels_match_all_frozen_original_python_cases() -> Result<()> {
    // Cargo runs integration tests in the crate directory. Supply the source
    // catalog explicitly, before its process-local OnceLock is initialized.
    if std::env::var_os("NACRE_LOCK_LABELS_CHILD").is_none() {
        let home = tempfile::tempdir()?;
        let result = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "labels_match_all_frozen_original_python_cases"])
            .env_clear()
            .env("NACRE_LOCK_LABELS_CHILD", "1")
            .env("HOME", home.path())
            .env("LANG", "C.UTF-8")
            .env("NACRE_LANGUAGE", "en")
            .env("XDG_CONFIG_HOME", home.path().join(".config"))
            .env("XDG_DATA_HOME", home.path().join(".local/share"))
            .env("XDG_STATE_HOME", home.path().join(".local/state"))
            .env("XDG_CACHE_HOME", home.path().join(".cache"))
            .env(
                "NACRE_HELPER_TOOLS",
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools"),
            )
            .output()?;
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stdout)
        );
        return Ok(());
    }
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/lock-info-labels-reference.json.gz").as_slice(),
    )
    .read_to_end(&mut raw)?;
    let reference: Value = serde_json::from_slice(&raw)?;
    let config = fontconfig::Fontconfig::new().ok_or("Fontconfig initialization failed")?;
    let font = config.find("DejaVu Sans", Some("Bold"))?;
    assert_eq!(
        format!("{:x}", Sha256::digest(fs::read(&font.path)?)),
        reference["font"]["sha256"].as_str().unwrap(),
        "Explicit reference font bytes differ"
    );
    let root = tempfile::tempdir()?;
    let mut info = Info::new(Paths::new(root.path()));
    info.font_path = Some(font.path);
    info.sys = root.path().join("sys");
    let capacity = info.sys.join("class/power_supply/BAT0/capacity");
    fs::create_dir_all(capacity.parent().unwrap())?;
    fs::write(capacity, "63\n")?;
    for (index, case) in reference["cases"].as_array().unwrap().iter().enumerate() {
        let args = &case["args"];
        let result = match case["operation"].as_str().unwrap() {
            "plain" => info.plain(
                &args[0],
                args[1].as_f64().unwrap(),
                args[2].as_u64().unwrap() as usize,
                args[3].as_u64().unwrap() as u32,
            ),
            "label" => info.label(args[0].as_str().unwrap(), &args[1], &args[2]),
            _ => return Err("Unknown frozen lock operation".into()),
        };
        assert_eq!(
            result?,
            case["result"].as_str().unwrap(),
            "Original lock label case {index}: {case}"
        );
    }
    Ok(())
}
