//! Filesystem exporter adapted from Orient (MIT); see ../LICENSE-Orient.
//! Palette computations and template evaluation call pinned Rust libraries directly.
use crate::{orient::from_image, Result};
use sha2::{Digest, Sha256};
fn fail<T>(message: &str) -> Result<T> {
    Err(message.to_owned().into())
}
use include_dir::{include_dir, Dir};
use serde_json::{json, Map, Value};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
};
static TEMPLATES: Dir<'_> =
    include_dir!("$CARGO_MANIFEST_DIR/../../deps/orient/src/orient/templates");
pub fn render(
    template: &str,
    palette: &Value,
    schemes: &Map<String, Value>,
    custom: Option<&Map<String, Value>>,
) -> Result<String> {
    let mut contexts = schemes.clone();
    for mode in orient_core::template_engine::requested_modes(template)? {
        if Some(mode.as_str()) == palette["mode"].as_str() || contexts.contains_key(&mode) {
            continue;
        }
        if let Some(source) = palette["source"]["path"]
            .as_str()
            .filter(|p| Path::new(p).is_file())
        {
            if palette["name"] == "dynamic" {
                if palette["engine"] != crate::orient::engine_id() {
                    return fail("Palette engine changed; supply an explicit companion palette");
                }
                if format!("{:x}", Sha256::digest(fs::read(source)?))
                    != palette["source"]["digest"].as_str().unwrap_or("")
                {
                    return fail("Source image changed; supply an explicit companion palette");
                }
                let mut opts = palette["input"].clone();
                opts["mode"] = json!(mode);
                opts["smart"] = json!(false);
                contexts.insert(
                    mode,
                    from_image(Path::new(source), &opts)?["colours"].clone(),
                );
            }
        }
    }
    Ok(orient_core::template_engine::render(
        template,
        palette,
        Some(&contexts),
        custom,
    )?)
}
fn builtin(dir: &Dir<'_>, output: &mut Vec<(PathBuf, String)>) -> Result<()> {
    for file in dir.files() {
        if file.path().extension().is_some_and(|v| v == "in") {
            output.push((
                file.path().to_path_buf(),
                file.contents_utf8()
                    .ok_or("Invalid template encoding")?
                    .into(),
            ));
        }
    }
    for dir in dir.dirs() {
        builtin(dir, output)?;
    }
    Ok(())
}
fn custom(root: &Path, folder: &Path, output: &mut Vec<(PathBuf, String)>) -> Result<()> {
    for entry in fs::read_dir(folder)? {
        let path = entry?.path();
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            custom(root, &path, output)?;
        } else if path.extension().is_some_and(|v| v == "in") {
            if meta.file_type().is_symlink() || !path.canonicalize()?.starts_with(root) {
                return fail("Theme templates must be regular files inside their folder");
            }
            output.push((
                path.strip_prefix(root)?.to_path_buf(),
                fs::read_to_string(path)?,
            ));
        }
    }
    Ok(())
}
pub fn export(
    palette: &Value,
    directory: &Path,
    templates: Option<&Path>,
    schemes: &Map<String, Value>,
    variables: Option<&Map<String, Value>>,
) -> Result<Vec<String>> {
    let mut inputs = Vec::new();
    if let Some(root) = templates {
        let root = root.canonicalize()?;
        custom(&root, &root, &mut inputs)?;
    } else {
        builtin(&TEMPLATES, &mut inputs)?
    }
    inputs.sort_by(|a, b| a.0.cmp(&b.0));
    if inputs.is_empty() {
        return fail("No .in theme templates found");
    }
    let directory = if directory.is_absolute() {
        directory.to_path_buf()
    } else {
        std::env::current_dir()?.join(directory)
    };
    let directory = if directory.exists() {
        directory.canonicalize()?
    } else {
        directory
    };
    let mut pending = Vec::new();
    for (path, text) in inputs {
        let target = directory.join(path.with_extension(""));
        if target.exists() {
            return fail(&format!(
                "Export would replace an existing file: {}",
                target.display()
            ));
        }
        let text = render(&text, palette, schemes, variables)?;
        if target.extension().is_some_and(|v| v == "json") {
            let _: Value = serde_json::from_str(&text)?;
        }
        pending.push((target, text));
    }
    fs::create_dir_all(&directory)?;
    let root = directory.canonicalize()?;
    let mut output = Vec::new();
    for (target, text) in pending {
        fs::create_dir_all(target.parent().ok_or("Invalid export path")?)?;
        if !target.parent().unwrap().canonicalize()?.starts_with(&root) {
            return fail("Export path escapes the selected directory");
        }
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        file.write_all(text.as_bytes())?;
        output.push(target.canonicalize()?.to_string_lossy().into());
    }
    Ok(output)
}
