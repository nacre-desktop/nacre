use nacre_menu::{desktop, text::OrderingRules, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read};
#[test]
fn frozen_current_quickshell_catalog_metadata_and_commands_match() -> Result<()> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(include_bytes!("fixtures/desktop-catalog.json.gz").as_slice())
        .read_to_end(&mut bytes)?;
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "a7d234ff7e053b046d54c943306d4c3b0c12e831d09a09615783ef3e5f33df1c"
    );
    let r: Value = serde_json::from_slice(&bytes)?;
    for case in r["cases"].as_array().unwrap() {
        let locale = case["result"]["locale"].as_str().unwrap();
        let ordering = OrderingRules::new(&locale.replace('_', "-"))?;
        let mut entries = BTreeMap::new();
        for root in ["system/applications/", "home/data/applications/"] {
            for (path, body) in r["files"]
                .as_object()
                .unwrap()
                .iter()
                .filter(|(p, _)| p.starts_with(root))
            {
                let id = path
                    .strip_prefix(root)
                    .unwrap()
                    .strip_suffix(".desktop")
                    .unwrap()
                    .replace('/', "-");
                desktop::install(
                    &mut entries,
                    desktop::parse(&id, body.as_str().unwrap(), locale),
                );
            }
        }
        let applications = desktop::applications(&entries, &ordering);
        for app in &applications {
            let entry = &entries[app["id"].as_str().unwrap()];
            assert!(std::sync::Arc::ptr_eq(app, &entry.data));
        }
        assert_eq!(
            serde_json::json!(applications),
            case["result"]["applications"],
            "locale{locale}"
        );
        for (id, expected) in case["result"]["lookups"].as_object().unwrap() {
            assert_eq!(
                entries
                    .get(id)
                    .map(|e| e.data.as_ref())
                    .unwrap_or(&Value::Null),
                expected,
                "lookup{id} locale{locale}"
            );
        }
    }
    Ok(())
}

#[test]
fn private_catalog_shadowing_literal_commands_and_resource_limits() -> Result<()> {
    use std::fs;
    use std::os::unix::fs::symlink;
    let temp = tempfile::tempdir()?;
    let high = temp.path().join("high");
    let low = temp.path().join("low");
    fs::create_dir_all(high.join("nested"))?;
    fs::create_dir_all(&low)?;
    fs::write(
        low.join("masked.desktop"),
        "[Desktop Entry]\nType=Application\nName=Visible\nExec=fixture\n",
    )?;
    fs::write(
        high.join("masked.desktop"),
        "[Desktop Entry]\nType=Application\nHidden=true\n",
    )?;
    fs::write(high.join("nested/safe.desktop"),"[Desktop Entry]\nType=Application\nName=Literal\nExec=fixture '$HOME;$(touch forbidden)' %U\nMalformed line\n")?;
    let roots = vec![high.clone(), low.clone()];
    let entries = desktop::scan(&roots, "en_US")?;
    assert!(!entries.contains_key("masked"));
    assert_eq!(
        entries["nested-safe"].data["command"],
        serde_json::json!(["fixture", "$HOME;$(touch forbidden)"])
    );
    assert_eq!(entries["nested-safe"].invalid_lines, vec![5]);
    assert!(!temp.path().join("forbidden").exists());
    symlink(&high, high.join("nested/cycle"))?;
    assert!(desktop::scan(&roots, "en_US").is_err());
    fs::remove_file(high.join("nested/cycle"))?;
    fs::write(high.join("oversized.desktop"), vec![b'x'; 1048577])?;
    assert!(desktop::scan(&roots, "en_US").is_err());
    Ok(())
}
