//! Strict source/proof binding and backed-up private companion installation.
use crate::{json, niri_session_runtime::HOVER_SOURCE, paths::Paths, Result};
use serde_json::{json as value, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    time::Duration,
};
const PROOF_FIELDS: &[&str] = &[
    "nestedVerified",
    "focusPreserved",
    "legacyStreamUnaffected",
    "hoverClearedOnClose",
    "qmlCaptionVerified",
];
pub fn digest(path: &Path) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(fs::read(path)?)))
}

pub fn install(
    source: &Path,
    proof: &Value,
    patch: &Path,
    paths: &Paths,
    stamp: &str,
) -> Result<PathBuf> {
    let binary = source.join("target/release/niri");
    if !proof.is_object()
        || PROOF_FIELDS
            .iter()
            .any(|field| proof.get(*field) != Some(&Value::Bool(true)))
        || proof["sourceRevision"] != HOVER_SOURCE
        || proof["binarySha256"] != digest(&binary)?
        || proof["patchSha256"] != digest(patch)?
    {
        return Err("Installation requires matching nested native and QML hover proof".into());
    }
    let root = paths.share.join("niri/overview-hover");
    if !root.exists() {
        fs::create_dir_all(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    }
    for name in ["niri", "manifest.json", "LICENSE.niri"] {
        let previous = root.join(name);
        if previous.exists() {
            crate::niri_session_stage::copied(
                &previous,
                &root.join(format!("{name}.before-{stamp}")),
            )?;
        }
    }
    let staged = tempfile::Builder::new()
        .prefix(".install-")
        .tempdir_in(&root)?;
    fs::copy(&binary, staged.path().join("niri"))?;
    fs::set_permissions(
        staged.path().join("niri"),
        fs::Permissions::from_mode(0o700),
    )?;
    // copyfile creates a new license with the normal creation mode, independent
    // of source permissions; the containing temporary directory stays private.
    fs::write(
        staged.path().join("LICENSE.niri"),
        fs::read(source.join("LICENSE"))?,
    )?;
    let manifest = value!({"schema":1,"enabled":true,"nestedVerified":true,"sourceRevision":HOVER_SOURCE,"protocol":"NacreOverviewHoverStream","binarySha256":digest(&staged.path().join("niri"))?,"patchSha256":digest(patch)?,"proof":proof});
    fs::write(
        staged.path().join("manifest.json"),
        json::pretty(&manifest)? + "\n",
    )?;
    fs::set_permissions(
        staged.path().join("manifest.json"),
        fs::Permissions::from_mode(0o600),
    )?;
    for name in ["niri", "manifest.json", "LICENSE.niri"] {
        fs::rename(staged.path().join(name), root.join(name))?;
    }
    Ok(root)
}

/// The canonical seven-character diff is part of the reviewed source contract.
pub fn prepare_source(source: &Path, revision: &str, patch: &Path) -> Result<()> {
    if !source.exists() {
        fs::create_dir_all(source.parent().ok_or("Invalid source directory")?)?;
        let status = std::process::Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                "--branch",
                "v26.04",
                "https://github.com/niri-wm/niri.git",
            ])
            .arg(source)
            .status()?;
        if !status.success() {
            return Err(format!("Niri source clone failed: {status}").into());
        }
    }
    let git = |arguments: &[&str]| -> Result<Vec<u8>> {
        let mut command = std::process::Command::new("git");
        command.args(arguments).current_dir(source);
        let output = crate::process::capture(command, None, Duration::from_secs(60))?;
        if !output.status.success() {
            return Err(format!(
                "Niri source Git operation failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )
            .into());
        }
        Ok(output.stdout)
    };
    if String::from_utf8(git(&["rev-parse", "HEAD"])?)?.trim() != revision {
        return Err("The experiment requires the pinned official Niri26.04 revision".into());
    }
    if !String::from_utf8(git(&["ls-files", "--others", "--exclude-standard"])?)?
        .trim()
        .is_empty()
    {
        return Err("Keep unrelated files outside the experiment source checkout".into());
    }
    let changes = git(&["diff", "--binary", "--abbrev=7", "HEAD"])?;
    if changes.is_empty() {
        let path = patch.canonicalize()?;
        let patch = path.to_str().ok_or("Invalid patch path")?;
        git(&["apply", "--check", patch])?;
        git(&["apply", patch])?;
    } else if changes != fs::read(patch)? {
        let previous = patch.with_file_name("niri-26.04-overview-hover-v1.patch");
        if !previous.is_file() || changes != fs::read(&previous)? {
            return Err("Source has other changes; refusing to overwrite them".into());
        }
        let previous = previous.canonicalize()?;
        let previous = previous.to_str().ok_or("Invalid previous patch path")?;
        let current = patch.canonicalize()?;
        let current = current.to_str().ok_or("Invalid patch path")?;
        git(&["apply", "--check", "--reverse", previous])?;
        git(&["apply", "--reverse", previous])?;
        if let Err(failure) =
            git(&["apply", "--check", current]).and_then(|_| git(&["apply", current]))
        {
            git(&["apply", previous])?;
            return Err(failure);
        }
    }
    Ok(())
}

pub fn ensure(
    backend: &Path,
    paths: &Paths,
    environment: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    ensure_with(
        || {
            crate::niri_session_runtime::overview_hover_binary(
                paths,
                &mut crate::niri_session_runtime::NativeVersions,
            )
        },
        || build_and_install(backend, paths, environment),
    )
}
pub fn ensure_with(
    mut available: impl FnMut() -> Option<PathBuf>,
    build: impl FnOnce() -> Result<()>,
) -> Result<()> {
    if available().is_some() {
        return Ok(());
    }
    build()?;
    if available().is_none() {
        return Err("Niri hover installation did not produce a usable companion".into());
    }
    Ok(())
}
fn build_and_install(
    backend: &Path,
    paths: &Paths,
    environment: &std::collections::BTreeMap<String, String>,
) -> Result<()> {
    let source = paths.share.join("niri-overview-hover/source");
    let patch = backend.join("tools/patches/niri-26.04-overview-hover.patch");
    prepare_source(&source, HOVER_SOURCE, &patch)?;
    let status = std::process::Command::new("cargo")
        .args(["build", "--release", "--locked", "-j", "4"])
        .current_dir(&source)
        .status()?;
    if !status.success() {
        return Err(format!("Niri companion build failed: {status}").into());
    }
    let binary = source.join("target/release/niri");
    let status = std::process::Command::new(&binary)
        .arg("--version")
        .status()?;
    if !status.success() {
        return Err(format!("Niri companion version check failed: {status}").into());
    }
    let temporary = tempfile::Builder::new()
        .prefix("nacre-hover-proof-")
        .tempdir()?;
    let proof = crate::niri_hover_probe::verify(
        backend,
        &binary,
        &temporary.path().join("proof.json"),
        environment,
        &std::env::current_exe()?,
    )?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S%6fZ").to_string();
    let root = install(&source, &proof, &patch, paths, &stamp)?;
    println!("Installed Nacre Niri companion: {}", root.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_patch_applies_once_and_rejects_unrelated_edits_and_files() -> Result<()> {
        let temporary = tempfile::tempdir()?;
        let source = temporary.path().join("source");
        fs::create_dir(&source)?;
        let git = |args: &[&str]| -> Result<Vec<u8>> {
            let result = std::process::Command::new("git")
                .args(args)
                .current_dir(&source)
                .output()?;
            if !result.status.success() {
                return Err(String::from_utf8(result.stderr)?.into());
            }
            Ok(result.stdout)
        };
        git(&["init", "-q"])?;
        fs::write(source.join("tracked.rs"), "original\n")?;
        fs::write(source.join("unrelated.txt"), "keep original\n")?;
        git(&["add", "."])?;
        git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "fixture",
        ])?;
        let revision = String::from_utf8(git(&["rev-parse", "HEAD"])?)?;
        fs::write(source.join("tracked.rs"), "patched\n")?;
        let patch = temporary.path().join("expected.patch");
        fs::write(&patch, git(&["diff", "--binary", "--abbrev=7", "HEAD"])?)?;
        git(&["restore", "tracked.rs"])?;
        git(&["config", "core.abbrev", "12"])?;
        prepare_source(&source, revision.trim(), &patch)?;
        prepare_source(&source, revision.trim(), &patch)?;
        assert_eq!(fs::read_to_string(source.join("tracked.rs"))?, "patched\n");
        let previous = patch.with_file_name("niri-26.04-overview-hover-v1.patch");
        fs::copy(&patch, &previous)?;
        fs::write(source.join("tracked.rs"), "upgraded\n")?;
        fs::write(&patch, git(&["diff", "--binary", "--abbrev=7", "HEAD"])?)?;
        fs::write(source.join("tracked.rs"), "patched\n")?;
        prepare_source(&source, revision.trim(), &patch)?;
        assert_eq!(fs::read_to_string(source.join("tracked.rs"))?, "upgraded\n");
        let current = fs::read(&patch)?;
        fs::write(&previous, &current)?;
        fs::write(&patch, "invalid candidate patch\n")?;
        assert!(prepare_source(&source, revision.trim(), &patch).is_err());
        assert_eq!(fs::read_to_string(source.join("tracked.rs"))?, "upgraded\n");
        fs::write(&patch, current)?;
        fs::write(source.join("unrelated.txt"), "unrelated edit\n")?;
        assert_eq!(
            prepare_source(&source, revision.trim(), &patch)
                .expect_err("Unrelated diff accepted")
                .to_string(),
            "Source has other changes; refusing to overwrite them"
        );
        assert_eq!(
            fs::read_to_string(source.join("unrelated.txt"))?,
            "unrelated edit\n"
        );
        fs::write(source.join("unrelated.txt"), "keep original\n")?;
        fs::write(source.join("unexpected.txt"), "do not remove\n")?;
        assert_eq!(
            prepare_source(&source, revision.trim(), &patch)
                .expect_err("Untracked file accepted")
                .to_string(),
            "Keep unrelated files outside the experiment source checkout"
        );
        assert_eq!(
            fs::read_to_string(source.join("unexpected.txt"))?,
            "do not remove\n"
        );
        Ok(())
    }
    #[test]
    fn frozen_proof_rejection_manifest_permissions_and_upgrade_backups() -> Result<()> {
        let previous_digest = format!(
            "{:x}",
            Sha256::digest(include_bytes!(
                "../../../tools/patches/niri-26.04-overview-hover-v1.patch"
            ))
        );
        let text = include_str!("../tests/fixtures/niri-hover-install-reference.json").replace(
            &previous_digest,
            &crate::niri_session_runtime::hover_patch_digest(),
        );
        let reference: Value = serde_json::from_str(&text)?;
        let patch = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tools/patches/niri-26.04-overview-hover.patch");
        for case in reference["cases"]
            .as_array()
            .ok_or("Missing install cases")?
        {
            let home = tempfile::tempdir()?;
            let paths = Paths::new(home.path());
            let source = home.path().join("source");
            let binary = source.join("target/release/niri");
            fs::create_dir_all(binary.parent().ok_or("Invalid binary path")?)?;
            fs::write(binary, "compiled compositor fixture\n")?;
            fs::write(source.join("LICENSE"), "license fixture\n")?;
            let root = paths.share.join("niri/overview-hover");
            if case["variant"] == "upgrade" {
                fs::create_dir_all(&root)?;
                for name in ["niri", "manifest.json", "LICENSE.niri"] {
                    let path = root.join(name);
                    fs::write(&path, format!("previous {name}\n"))?;
                    fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
                }
            }
            let result = install(
                &source,
                &case["proof"],
                &patch,
                &paths,
                "20000102T030405006000Z",
            );
            if case["error"].is_null() {
                assert_eq!(result?, root);
            } else {
                assert_eq!(
                    result.expect_err("Invalid proof accepted").to_string(),
                    case["error"].as_str().unwrap()
                );
            }
            let mut files = serde_json::Map::new();
            if root.exists() {
                for entry in fs::read_dir(&root)? {
                    let path = entry?.path();
                    files.insert(path.file_name().unwrap().to_string_lossy().into_owned(), value!({"text":fs::read_to_string(&path)?,"mode":fs::metadata(&path)?.permissions().mode() & 0o777}));
                }
            }
            assert_eq!(Value::Object(files), case["files"], "{}", case["variant"]);
        }
        Ok(())
    }
}
