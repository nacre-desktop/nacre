use nacre_helper::{demo_wallpapers as demo, paths::Paths, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
};
fn assets() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools/demo-wallpapers")
}
fn fixture() -> Result<Value> {
    Ok(serde_json::from_str(include_str!(
        "fixtures/demo-wallpapers-reference.json"
    ))?)
}
#[test]
fn frozen_collection_and_errors() -> Result<()> {
    let frozen = fixture()?;
    assert_eq!(demo::collection(&assets())?, frozen["collection"]);
    for case in frozen["cases"]
        .as_array()
        .ok_or("Missing cases")?
        .iter()
        .filter(|c| c["operation"] == "collection")
    {
        let root = tempfile::tempdir()?;
        for item in frozen["collection"]
            .as_array()
            .ok_or("Missing collection")?
        {
            let name = item["file"].as_str().ok_or("Missing file")?;
            symlink(assets().join(name), root.path().join(name))?;
        }
        fs::write(
            root.path().join("manifest.json"),
            serde_json::to_vec(&case["manifest"])?,
        )?;
        let error = demo::collection(root.path()).expect_err("Invalid collection accepted");
        assert_eq!(
            json!({"type":"ValueError","message":error.to_string()}),
            case["error"],
            "{}",
            case["name"]
        );
    }
    Ok(())
}
#[test]
fn frozen_cache_bytes_modes_repairs_and_stat_receipts() -> Result<()> {
    let frozen = fixture()?;
    for case in frozen["cases"]
        .as_array()
        .ok_or("Missing cases")?
        .iter()
        .filter(|c| c["operation"] == "ensure")
    {
        let home = tempfile::tempdir()?;
        let paths = Paths::new(home.path());
        let name = case["name"].as_str().ok_or("Missing case name")?;
        if name != "fresh" {
            let initial = demo::ensure(&paths, &assets(), false)?;
            if name == "damaged" {
                fs::write(
                    initial[0]["path"].as_str().ok_or("Missing output")?,
                    b"broken cache",
                )?;
            }
            if name == "missing" {
                fs::remove_file(initial[1]["path"].as_str().ok_or("Missing output")?)?;
            }
        }
        let actual = demo::ensure(
            &paths,
            &assets(),
            matches!(name, "fast" | "damaged" | "missing"),
        )?;
        assert_eq!(
            serde_json::from_str::<Value>(
                &serde_json::to_string(&actual)?
                    .replace(home.path().to_str().ok_or("Bad path")?, "/fixture/home")
            )?,
            case["result"]
        );
        for (relative, expected) in case["files"].as_object().ok_or("Missing files")? {
            let path = home.path().join(relative);
            assert_eq!(
                fs::metadata(&path)?.permissions().mode() & 0o777,
                expected["mode"]
            );
            if path.file_name().is_some_and(|v| v == "verified.json") {
                let observed: Value = serde_json::from_slice(&fs::read(&path)?)?;
                assert_eq!(
                    observed,
                    demo::stamps(
                        &assets(),
                        path.parent().ok_or("Missing folder")?,
                        actual.as_array().ok_or("Missing rows")?
                    )?
                );
                let mut normalized = serde_json::Map::new();
                for (key, value) in observed.as_object().ok_or("Missing stamps")? {
                    normalized.insert(
                        key.replace(assets().to_str().ok_or("Bad assets")?, "/fixture/assets")
                            .replace(home.path().to_str().ok_or("Bad home")?, "/fixture/home"),
                        json!(["@DEV@", "@INODE@", value[2], "@MTIME_NS@", "@CTIME_NS@"]),
                    );
                }
                assert_eq!(json!(normalized), expected["json"]);
            } else {
                let bytes = fs::read(&path)?;
                assert_eq!(format!("{:x}", Sha256::digest(&bytes)), expected["sha256"]);
                assert_eq!(bytes.len(), expected["size"]);
            }
        }
        assert_eq!(
            home.path().join("Pictures").exists(),
            case["pictures_created"]
        );
    }
    Ok(())
}
#[test]
fn valid_fast_receipt_skips_checksum_and_repeated_copy_preserves_mtime() -> Result<()> {
    let root = tempfile::tempdir()?;
    let private_assets = root.path().join("assets");
    fs::create_dir(&private_assets)?;
    fs::copy(
        assets().join("manifest.json"),
        private_assets.join("manifest.json"),
    )?;
    let paths = Paths::new(root.path().join("home"));
    let rows = demo::collection(&assets())?;
    for item in rows.as_array().ok_or("Missing rows")? {
        let name = item["file"].as_str().ok_or("Missing file")?;
        fs::copy(assets().join(name), private_assets.join(name))?;
    }
    let output = demo::ensure(&paths, &private_assets, false)?;
    let first = Path::new(output[0]["path"].as_str().ok_or("Missing output")?);
    let mtime = fs::metadata(first)?.modified()?;
    assert_eq!(demo::ensure(&paths, &private_assets, false)?, output);
    assert_eq!(fs::metadata(first)?.modified()?, mtime);
    fs::write(
        private_assets.join("forest.jpg"),
        b"changed source with matching stat receipt",
    )?;
    let folder = first.parent().ok_or("Missing folder")?;
    fs::write(
        folder.join("verified.json"),
        nacre_helper::json::encode(&demo::stamps(
            &private_assets,
            folder,
            rows.as_array().ok_or("Missing rows")?,
        )?)?,
    )?;
    assert_eq!(demo::ensure(&paths, &private_assets, true)?, output);
    assert!(demo::ensure(&paths, &private_assets, false)
        .unwrap_err()
        .to_string()
        .contains("checksum failed"));
    Ok(())
}

#[test]
fn actual_cli_runs_without_python_and_uses_absolute_xdg_cache() -> Result<()> {
    let root = tempfile::tempdir()?;
    let home = root.path().join("home");
    let cache = root.path().join("cache");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .arg("demo-wallpapers")
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CACHE_HOME", &cache)
        .env(
            "NACRE_HELPER_TOOLS",
            assets().parent().ok_or("Missing tools")?,
        )
        .env("PATH", "/nonexistent")
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
    let rows: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(rows.as_array().ok_or("Missing rows")?.len(), 4);
    for item in rows.as_array().ok_or("Missing rows")? {
        assert!(Path::new(item["path"].as_str().ok_or("Missing path")?)
            .starts_with(cache.join("nacre/demo-wallpapers")));
    }
    assert!(!home.join("Pictures").exists());
    assert!(!home.join(".cache").exists());
    Ok(())
}
