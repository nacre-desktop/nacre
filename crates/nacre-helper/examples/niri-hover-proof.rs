//! Developer runner for the production proof in a disposable nested session.
use nacre_helper::{niri_hover_probe, Result};
use std::path::PathBuf;
fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 4 {
        return Err("Expected repository, companion, output and helper paths".into());
    }
    niri_hover_probe::verify(
        &PathBuf::from(&args[0]),
        &PathBuf::from(&args[1]),
        &PathBuf::from(&args[2]),
        &std::env::vars().collect(),
        &PathBuf::from(&args[3]),
    )?;
    Ok(())
}
