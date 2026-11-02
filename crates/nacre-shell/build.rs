// SPDX-License-Identifier: GPL-3.0-only
use std::{env, path::PathBuf, process::Command};

fn main() {
    let manifest_dir =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"));
    let workspace_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .expect("nacre-shell must remain under crates/");
    println!(
        "cargo:rerun-if-changed={}",
        workspace_root.join("Cargo.lock").display()
    );

    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(["metadata", "--locked", "--format-version=1"])
        .current_dir(workspace_root)
        .output()
        .expect("run cargo metadata to locate the pinned Bead source");
    if !output.status.success() {
        panic!(
            "cargo metadata could not locate the pinned Bead source:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let metadata: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("parse cargo metadata JSON");
    let framework_manifest = metadata["packages"]
        .as_array()
        .and_then(|packages| {
            packages
                .iter()
                .find(|package| package["name"] == "bead-test")
        })
        .and_then(|package| package["manifest_path"].as_str())
        .map(PathBuf::from)
        .expect("the pinned Bead checkout must include bead-test");
    let framework_root = framework_manifest
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .expect("bead-test manifest must remain under crates/");
    println!(
        "cargo:rustc-env=BEAD_FRAMEWORK_ROOT={}",
        framework_root.display()
    );
}
