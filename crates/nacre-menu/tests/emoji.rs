use nacre_menu::Result;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;
#[test]
fn frozen_native_provider_results_match_with_a_cached_index() -> Result<()> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(include_bytes!("fixtures/emoji.json.gz").as_slice())
        .read_to_end(&mut bytes)?;
    assert_eq!(
        format!("{:x}", Sha256::digest(&bytes)),
        "cf01051539041b04f673159154557979d32f53eb26c0f47eaede88a640e56fb6"
    );
    let reference: Value = serde_json::from_slice(&bytes)?;
    let cases = reference["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 31);
    for case in cases {
        let result = nacre_menu::emoji::bundled().search(case["query"].as_str().unwrap());
        if case["code"] == 0 {
            assert_eq!(
                result?,
                serde_json::from_str::<Value>(case["stdout"].as_str().unwrap())?
            );
        } else {
            assert!(result.is_err());
        }
    }
    assert!(nacre_menu::emoji::bundled().search("x\0y").is_err());
    Ok(())
}
