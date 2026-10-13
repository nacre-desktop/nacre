//! Direct library integration: decoding and pure palette assembly, without RPC.
use crate::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{fs, path::Path, sync::OnceLock};

pub fn engine_id() -> &'static str {
    static PIN: OnceLock<Value> = OnceLock::new();
    PIN.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../nacre/shell-cli/src/nacre_shell/engine-pin.json"
        ))
        .expect("checked engine pin")
    })["engine"]
        .as_str()
        .expect("checked engine identity")
}

pub fn from_image(path: &Path, options: &Value) -> Result<Value> {
    let path = path.canonicalize()?;
    let before = fs::metadata(&path)?;
    let identity = (before.len(), before.modified()?);
    let settings = orient_core::engine::settings(options)?;
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
    if (after.len(), after.modified()?) != identity {
        return Err("Wallpaper changed during extraction; select it again".into());
    }
    palette["engine"] = json!(engine_id());
    Ok(palette)
}
