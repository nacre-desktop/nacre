//! Immutable reference inputs captured from the current Python front end.
use base64::{engine::general_purpose::STANDARD, Engine};
use nacre_helper::{wallpaper_media::Media, Result};
use serde_json::Value;
use std::{fs, io::Read};

#[test]
fn current_bundle_catalog_remains_available_beside_original_presets() -> Result<()> {
    let root = tempfile::tempdir()?;
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools");
    let bundles = nacre_helper::theme_bundles::catalog(&assets)?;
    assert!(!bundles.as_array().ok_or("Invalid bundles")?.is_empty());
    let media = Media::new(nacre_helper::paths::Paths::new(root.path()), assets)?;
    let presets = media.palette_presets()?;
    let presets = presets.as_array().ok_or("Invalid combined presets")?;
    for bundle in bundles.as_array().unwrap() {
        assert!(presets.iter().any(|entry| entry["id"] == bundle["id"]));
    }
    Ok(())
}
#[test]
fn automatic_mode_matches_original_pillow_luminance_and_thumbnail_bytes() -> Result<()> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(
        include_bytes!("fixtures/wallpaper-mode-reference.json.gz").as_slice(),
    )
    .read_to_end(&mut raw)?;
    let reference: Value = serde_json::from_slice(&raw)?;
    let root = tempfile::tempdir()?;
    for case in reference["cases"].as_array().ok_or("Missing mode cases")? {
        let path = root.path().join("input");
        fs::write(
            &path,
            STANDARD.decode(case["input"].as_str().ok_or("Missing image")?)?,
        )?;
        let image = Media::wallpaper_mode_pixels(&path)?;
        assert_eq!(
            (image.width(), image.height()),
            (
                case["size"][0].as_u64().unwrap() as u32,
                case["size"][1].as_u64().unwrap() as u32
            ),
            "{}",
            case["name"]
        );
        let expected = STANDARD.decode(case["luma"].as_str().unwrap())?;
        let difference = image
            .as_raw()
            .iter()
            .zip(&expected)
            .position(|(a, b)| a != b);
        assert!(
            difference.is_none(),
            "{}: first luminance difference at {:?}",
            case["name"],
            difference
        );
        assert_eq!(
            Media::wallpaper_mode(&path)?,
            case["result"].as_str().unwrap(),
            "{}",
            case["name"]
        );
    }
    Ok(())
}

#[test]
fn a_different_helper_executable_keeps_the_prepared_palette_marker() -> Result<()> {
    // A deploy swaps the helper binary. This test binary and nacre-helper are different
    // executables with the same palette code, so a prepared palette must stay valid.
    let home = tempfile::tempdir()?;
    let paths = nacre_helper::paths::Paths::new(home.path());
    fs::create_dir_all(&paths.config)?;
    let poster = home.path().join("wall.png");
    image::DynamicImage::new_rgb8(4, 4).save(&poster)?;
    let assets = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../nacre/shell-tools");
    let here = Media::new(paths, assets.clone())?.palette_marker(
        &poster,
        "default",
        None,
        Some(&poster),
        None,
        None,
    )?;
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_nacre-helper"))
        .args(["palette-api", "wallpaper-media", "palette_marker"])
        .arg(serde_json::json!({"args":[poster, "default", null, poster, null, null]}).to_string())
        .env_clear()
        .env("HOME", home.path())
        .env("PATH", "/usr/bin:/bin")
        .env("NACRE_HELPER_TOOLS", &assets)
        .output()?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let reply: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(reply["result"].as_str(), here.to_str());
    Ok(())
}
