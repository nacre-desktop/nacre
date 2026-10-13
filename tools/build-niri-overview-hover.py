#!/usr/bin/env python3
"""Build the optional pinned Niri hover experiment; never replace packaged Niri."""

import argparse
import datetime
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile

SOURCE = "8ed0da44d974c32c6877d2f4630c314da0717ecb"
PATCH = Path(__file__).with_name("patches") / "niri-26.04-overview-hover.patch"
PROOF_FIELDS = (
    "nestedVerified",
    "focusPreserved",
    "legacyStreamUnaffected",
    "hoverClearedOnClose",
    "qmlCaptionVerified",
)


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def prepare(source):
    source = Path(source).resolve()
    if not source.exists():
        source.parent.mkdir(parents=True, exist_ok=True)
        subprocess.run(
            [
                "git",
                "clone",
                "--depth",
                "1",
                "--branch",
                "v26.04",
                "https://github.com/niri-wm/niri.git",
                str(source),
            ],
            check=True,
        )
    revision = subprocess.check_output(
        ["git", "rev-parse", "HEAD"], cwd=source, text=True
    ).strip()
    if revision != SOURCE:
        raise ValueError(
            "The experiment requires the pinned official Niri26.04 revision"
        )
    untracked = subprocess.check_output(
        ["git", "ls-files", "--others", "--exclude-standard"], cwd=source, text=True
    ).strip()
    if untracked:
        raise ValueError("Keep unrelated files outside the experiment source checkout")
    changes = subprocess.check_output(["git", "diff", "--binary", "HEAD"], cwd=source)
    if not changes:
        subprocess.run(
            ["git", "apply", "--check", str(PATCH.resolve())], cwd=source, check=True
        )
        subprocess.run(["git", "apply", str(PATCH.resolve())], cwd=source, check=True)
    elif changes != PATCH.read_bytes():
        raise ValueError("Source has other changes; refusing to overwrite them")
    return source


def install(source, proof_path, home):
    binary = Path(source) / "target/release/niri"
    proof = json.loads(Path(proof_path).read_text())
    if (
        not isinstance(proof, dict)
        or any(proof.get(field) is not True for field in PROOF_FIELDS)
        or proof.get("sourceRevision") != SOURCE
        or proof.get("binarySha256") != digest(binary)
        or proof.get("patchSha256") != digest(PATCH)
    ):
        raise ValueError(
            "Installation requires matching nested native and QML hover proof"
        )
    root = Path(home) / ".local/share/nacre/niri-on-pc/overview-hover"
    root.mkdir(mode=0o700, parents=True, exist_ok=True)
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    for previous in (root / "niri", root / "manifest.json"):
        if previous.exists():
            shutil.copy2(previous, root / (previous.name + ".before-" + stamp))
    with tempfile.TemporaryDirectory(prefix=".install-", dir=root) as temporary:
        staged = Path(temporary)
        shutil.copyfile(binary, staged / "niri")
        (staged / "niri").chmod(0o700)
        manifest = {
            "schema": 1,
            "enabled": True,
            "nestedVerified": True,
            "sourceRevision": SOURCE,
            "protocol": "NacreOverviewHoverStream",
            "binarySha256": digest(staged / "niri"),
            "patchSha256": digest(PATCH),
            "proof": proof,
        }
        (staged / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
        (staged / "manifest.json").chmod(0o600)
        os.replace(staged / "niri", root / "niri")
        os.replace(staged / "manifest.json", root / "manifest.json")
    return root


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source",
        type=Path,
        default=Path.home() / ".local/share/nacre/niri-overview-hover/source",
    )
    parser.add_argument("--jobs", type=int, default=4)
    parser.add_argument("--install-with-proof", type=Path)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 64:
        parser.error("Jobs must be between1 and64")
    source = prepare(args.source)
    subprocess.run(
        ["cargo", "build", "--release", "--locked", "-j", str(args.jobs)],
        cwd=source,
        check=True,
    )
    binary = source / "target/release/niri"
    subprocess.run([str(binary), "--version"], check=True)
    if args.install_with_proof:
        print(
            "Enabled for the next prepared Niri login:",
            install(source, args.install_with_proof, Path.home()),
        )
    else:
        print("Built:", binary)
        print(
            "Not enabled; verify nested hover, focus and the title bar before installation."
        )


if __name__ == "__main__":
    main()
