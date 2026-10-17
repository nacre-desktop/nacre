use nacre_menu::{calculator, Result};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::io::Read;

fn compare(compressed: &[u8], digest: &str, count: usize) -> Result<()> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(compressed).read_to_end(&mut bytes)?;
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)), digest);
    let reference: Value = serde_json::from_slice(&bytes)?;
    let cases = reference["cases"].as_array().ok_or("Missing cases")?;
    assert_eq!(cases.len(), count);
    for case in cases {
        assert_eq!(
            serde_json::to_value(calculator::evaluate(
                case["input"].as_str().ok_or("Missing input")?
            ))?,
            case["expected"],
            "frozen calculator case {} input {:?}",
            case["id"],
            case["input"]
        );
    }
    Ok(())
}
#[test]
fn frozen_current_quickshell_calculator_outputs_are_exact() -> Result<()> {
    compare(
        include_bytes!("fixtures/calculator.json.gz"),
        "46f8140957260cecafee458bf6cafe0493d2404a254fb12272421f9270ab143e",
        217,
    )
}
#[test]
fn separately_frozen_numeric_and_whitespace_edges_are_exact() -> Result<()> {
    compare(
        include_bytes!("fixtures/calculator-extra.json.gz"),
        "59c81cdcde6307e28a5460de2057f1be4dfa6bdd6f212ad0db26c1aec3170148",
        33,
    )
}
#[test]
fn separately_frozen_nonfinite_power_matrix_is_exact() -> Result<()> {
    compare(
        include_bytes!("fixtures/calculator-powers.json.gz"),
        "15244803816d0cfcd44e8fcdb5b1f46928365df3a7694eaf0a1cbfd4742ced1f",
        63,
    )
}
