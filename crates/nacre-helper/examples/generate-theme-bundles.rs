//! Rebuild only the complete Orient mappings; artwork sources/hashes stay immutable.
use nacre_helper::{theme_bundles, Result};
use serde_json::Value;
fn main() -> Result<()> {
    let path = std::path::Path::new("nacre/shell-tools/theme-bundles/catalog.json");
    let mut document: Value = serde_json::from_slice(&std::fs::read(path)?)?;
    for bundle in document["bundles"]
        .as_array_mut()
        .ok_or("Missing bundles")?
    {
        for mode in ["dark", "light"] {
            bundle["modes"][mode] = serde_json::to_value(theme_bundles::map_palette(
                &bundle["sourceColors"][mode],
                mode,
            )?)?;
        }
    }
    std::fs::write(path, serde_json::to_string_pretty(&document)? + "\n")?;
    Ok(())
}
