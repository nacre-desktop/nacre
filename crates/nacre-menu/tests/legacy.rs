use nacre_menu::{browser, context::Context, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
#[test]
fn frozen_legacy_actions_and_existing_favorite_ids_remain_readable() -> Result<()> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(include_bytes!("fixtures/legacy.json.gz").as_slice())
        .read_to_end(&mut bytes)?;
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "57e906ed11b851788bfc3ddfc2369270aa4f9db0d74544c2c92d1729d793e223"
    );
    let reference: Value = serde_json::from_slice(&bytes)?;
    for case in reference["cases"].as_array().unwrap() {
        let mut context = Context::new(
            serde_json::json!({"rows":[]}),
            serde_json::from_str(include_str!("../data/defaults.json"))?,
            "en-US",
            0.0,
        )?;
        context.settings = case["settings"].clone();
        context.legacy_actions = browser::default_actions(&context);
        assert_eq!(serde_json::json!(context.legacy_actions), case["result"]);
        let row = context.legacy_actions[0].clone();
        context.preferences = serde_json::json!({"favorites":[row["id"]]});
        assert_eq!(context.favorites()?, vec![row]);
    }
    Ok(())
}
