//! Direct library integration: decoding and pure palette assembly, without RPC.
use crate::{files, paths::Paths, Result};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::OnceLock,
};

fn pin() -> &'static Value {
    static PIN: OnceLock<Value> = OnceLock::new();
    PIN.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../nacre/shell-cli/src/nacre_shell/engine-pin.json"
        ))
        .expect("checked engine pin")
    })
}

pub fn engine_id() -> &'static str {
    pin()["engine"].as_str().expect("checked engine identity")
}

fn cache_identity(path: &Path, settings: &Value) -> Result<String> {
    let meta = fs::metadata(path)?;
    let identity = json!([
        path,
        meta.dev(),
        meta.ino(),
        meta.size(),
        meta.mtime(),
        meta.mtime_nsec(),
        meta.ctime(),
        meta.ctime_nsec(),
        settings,
        engine_id(),
        pin()["decoder"]
    ]);
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&identity)?)
    ))
}

pub fn from_image(path: &Path, options: &Value) -> Result<Value> {
    if engine_id().rsplit_once('-').map(|(core, _)| core) != Some(orient_core::ENGINE_ID) {
        return Err("Compiled Orient core differs from the pinned engine".into());
    }
    let path = path.canonicalize()?;
    let before = fs::metadata(&path)?;
    let identity = (before.len(), before.modified()?);
    let settings = orient_core::engine::settings(options)?;
    let cache_id = cache_identity(&path, &settings)?;
    let folder = if let Some(folder) = options["cache_dir"].as_str() {
        PathBuf::from(folder)
    } else {
        Paths::environment()?
            .cache
            .parent()
            .ok_or("Missing cache root")?
            .join("orient/palettes")
    };
    let cache = folder.join(format!("rust-{cache_id}.json"));
    match fs::read(&cache) {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value)
                if value["engine"] == engine_id()
                    && value["formatVersion"] == orient_core::FORMAT_VERSION
                    && value["input"] == settings
                    && value["name"] == "dynamic"
                    && value["colours"]
                        .as_object()
                        .is_some_and(|colors| orient_core::palette::validate(colors).is_ok()) =>
            {
                return Ok(value)
            }
            Ok(_) => {}
            Err(error) => eprintln!("Orient palette cache read: {error}"),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => eprintln!("Orient palette cache read: {error}"),
    }
    let frames = orient_decoder::frames(&path)?;
    let mut analysis = orient_core::extract::analyze(&frames)?;
    analysis["path"] = json!(path);
    analysis["digest"] = json!(format!("{:x}", Sha256::digest(fs::read(&path)?)));
    analysis["engine"] = json!(engine_id());
    let classic = if settings["personality"] == "natural" {
        Some(orient_core::classic::analyze(&frames)?)
    } else {
        None
    };
    let mut palette = orient_core::engine::from_analysis(
        analysis,
        classic.as_ref(),
        &settings,
        options.get("hour"),
    )?;
    let after = fs::metadata(&path)?;
    if (after.len(), after.modified()?) != identity || cache_identity(&path, &settings)? != cache_id
    {
        return Err("Wallpaper changed during extraction; select it again".into());
    }
    palette["engine"] = json!(engine_id());
    if let Err(error) = files::atomic(&cache, &serde_json::to_vec(&palette)?) {
        eprintln!("Orient palette cache write: {error}");
    }
    Ok(palette)
}
