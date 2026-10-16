use nacre_menu::{config, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
#[test]
fn frozen_js_menu_overrides_errors_and_builtin_fallback_match() -> Result<()> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(include_bytes!("fixtures/config.json.gz").as_slice())
        .read_to_end(&mut bytes)?;
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "0fe6d2de867204a54f7156c44e1a8f7f440ca5fbad460b9660a31ee5bf3aaa58"
    );
    let r: Value = serde_json::from_slice(&bytes)?;
    for case in r["cases"].as_array().unwrap() {
        let actual = config::merge(r["base"].as_str().unwrap(), case["input"].as_str().unwrap())
            .map_err(|e| -> nacre_menu::Error { e.into() })?;
        assert_eq!(actual, case["result"], "{}", case["name"]);
    }
    Ok(())
}
