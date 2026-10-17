//! Frozen implicit inputs for the original staging-output fixtures.
use crate::Result;
use sha2::{Digest, Sha256};
use std::{fs, os::unix::fs::symlink, path::Path};

pub fn legacy_compositor_backend() -> Result<tempfile::TempDir> {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let backend = tempfile::tempdir()?;
    for name in ["niri", "hypr"] {
        symlink(source.join(name), backend.path().join(name))?;
    }
    let nacre = backend.path().join("nacre");
    fs::create_dir(&nacre)?;
    for entry in fs::read_dir(source.join("nacre"))? {
        let entry = entry?;
        if entry.file_name() != "shell-tools" {
            symlink(entry.path(), nacre.join(entry.file_name()))?;
        }
    }
    let tools = nacre.join("shell-tools");
    fs::create_dir(&tools)?;
    for entry in fs::read_dir(source.join("nacre/shell-tools"))? {
        let entry = entry?;
        if entry.file_name() != "compositor-contract.json" {
            symlink(entry.path(), tools.join(entry.file_name()))?;
        }
    }
    let contract = include_bytes!("../tests/fixtures/compositor-contract-before-foundation.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(contract)),
        "c075df8a6763f350a91f2910133cba1f5e00fdd8eeedc52e17dc40efade54c9b"
    );
    fs::write(tools.join("compositor-contract.json"), contract)?;
    Ok(backend)
}
