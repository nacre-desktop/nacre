//! Offline licensed artwork, copied unchanged into the existing content-addressed cache.
use crate::{i18n::tr, json, paths::Paths, Result};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

fn entries(manifest: &Value) -> Result<&Vec<Value>> {
    manifest["wallpapers"]
        .as_array()
        .ok_or_else(|| "Invalid wallpaper manifest entries".into())
}
fn filename(item: &Value) -> Result<&str> {
    item["file"]
        .as_str()
        .ok_or_else(|| "Invalid wallpaper filename".into())
}
fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}
pub fn collection(assets: &Path) -> Result<Value> {
    let manifest: Value = serde_json::from_slice(&fs::read(assets.join("manifest.json"))?)?;
    if manifest["version"] != 1 || entries(&manifest).map_or(true, |rows| rows.len() != 4) {
        return Err(tr("helper.demo-wallpapers.the-bundled-wallpaper-manifest-is-incomplete-reinstall-the-shell", "The bundled wallpaper manifest is incomplete. Reinstall the shell.").into());
    }
    for item in entries(&manifest)? {
        let name = filename(item)?;
        let path = Path::new(name);
        if path.file_name().and_then(|v| v.to_str()) != Some(name)
            || !matches!(
                path.extension().and_then(|v| v.to_str()),
                Some("jpg" | "png" | "webp")
            )
        {
            return Err(tr(
                "helper.demo-wallpapers.invalid-bundled-wallpaper-filename",
                "Invalid bundled wallpaper filename",
            )
            .into());
        }
        let source = assets.join(name);
        if item["sha256"] != digest(&source)? {
            return Err(tr("helper.demo-wallpapers.bundled-wallpaper-checksum-failed-value0-reinstall-the-shell", "Bundled wallpaper checksum failed: {value0}. Reinstall the shell.").replace("{value0}", name).into());
        }
        let (width, height) = image::ImageReader::open(&source)?
            .with_guessed_format()?
            .into_dimensions()?;
        if item["size"] != json!([width, height]) || width.max(height) != 3840 {
            return Err(tr(
                "helper.demo-wallpapers.bundled-wallpaper-resolution-does-not-match-its-manifest",
                "Bundled wallpaper resolution does not match its manifest",
            )
            .into());
        }
        if item["license"] != "CC0"
            || !item["artist"].as_str().is_some_and(|v| !v.is_empty())
            || !item["source"].as_str().is_some_and(|v| !v.is_empty())
        {
            return Err(tr(
                "helper.demo-wallpapers.bundled-wallpaper-source-information-is-incomplete",
                "Bundled wallpaper source information is incomplete",
            )
            .into());
        }
    }
    Ok(manifest["wallpapers"].clone())
}
pub fn stamps(assets: &Path, folder: &Path, rows: &[Value]) -> Result<Value> {
    let mut result = Map::new();
    for item in rows {
        for path in [assets.join(filename(item)?), folder.join(filename(item)?)] {
            let metadata = fs::metadata(&path)?;
            result.insert(
                path.to_string_lossy().into_owned(),
                json!([
                    metadata.dev(),
                    metadata.ino(),
                    metadata.len(),
                    i128::from(metadata.mtime()) * 1_000_000_000
                        + i128::from(metadata.mtime_nsec()),
                    i128::from(metadata.ctime()) * 1_000_000_000
                        + i128::from(metadata.ctime_nsec())
                ]),
            );
        }
    }
    Ok(Value::Object(result))
}
fn located(rows: &[Value], folder: &Path) -> Result<Value> {
    rows.iter()
        .map(|item| {
            let mut item = item.clone();
            item["path"] = json!(folder.join(filename(&item)?));
            Ok(item)
        })
        .collect::<Result<Vec<_>>>()
        .map(Value::Array)
}
pub fn ensure(paths: &Paths, assets: &Path, fast: bool) -> Result<Value> {
    let bytes = fs::read(assets.join("manifest.json"))?;
    let identity = format!("{:x}", Sha256::digest(&bytes));
    let folder = paths.cache.join("demo-wallpapers").join(&identity[..16]);
    fs::create_dir_all(&folder)?;
    let manifest: Value = serde_json::from_slice(&bytes)?;
    let rows = entries(&manifest)?;
    let receipt = folder.join("verified.json");
    if fast {
        // Missing/stale receipts are cache misses, not failures: validate and repair below.
        let previous = match fs::read(&receipt) {
            Ok(bytes) => serde_json::from_slice::<Value>(&bytes).ok(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        match stamps(assets, &folder, rows) {
            Ok(current) if previous.as_ref() == Some(&current) => return located(rows, &folder),
            Ok(_) => (),
            Err(error)
                if error
                    .downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound) => {}
            Err(error) => return Err(error),
        }
    }
    let verified = collection(assets)?;
    let rows = verified
        .as_array()
        .ok_or("Invalid verified wallpaper entries")?;
    for item in rows {
        let name = filename(item)?;
        let output = folder.join(name);
        if !output.is_file() || item["sha256"] != digest(&output)? {
            let mut temporary = tempfile::Builder::new()
                .suffix(
                    Path::new(name)
                        .extension()
                        .and_then(|v| v.to_str())
                        .unwrap_or(""),
                )
                .tempfile_in(&folder)?;
            std::io::copy(
                &mut fs::File::open(assets.join(name))?,
                temporary.as_file_mut(),
            )?;
            temporary.persist(output)?;
        }
    }
    let mut temporary = tempfile::NamedTempFile::new_in(&folder)?;
    temporary.write_all(json::encode(&stamps(assets, &folder, rows)?)?.as_bytes())?;
    temporary.persist(receipt)?;
    located(rows, &folder)
}
pub fn run(args: &[String]) -> Result<i32> {
    let paths = Paths::environment()?;
    let mut home = None;
    let mut fast = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--fast" => fast = true,
            "--home" => {
                index += 1;
                home = Some(PathBuf::from(args.get(index).ok_or("Missing home")?));
            }
            _ => return Err("Unknown demo-wallpapers argument".into()),
        }
        index += 1;
    }
    let requested = home
        .as_deref()
        .map(Paths::for_home)
        .transpose()?
        .unwrap_or_else(|| paths.clone());
    println!(
        "{}",
        json::encode(&ensure(
            &requested,
            &paths.assets().join("demo-wallpapers"),
            fast
        )?)?
    );
    Ok(0)
}
