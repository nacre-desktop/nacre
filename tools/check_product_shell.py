#!/usr/bin/env python3
"""Check the product shell against Nacre's pinned Bead framework."""

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
LUAU_URL = "https://github.com/luau-lang/luau/releases/download/0.742/luau-ubuntu.zip"
LUAU_SHA256 = "152058b98c60ff1f5b5ec9bca0b8eb37793cae161dd3a05608cc3d8738ba1a4d"
BEAD_REVISION = "ed5d1b8fc814b5f0a2dda23dac4dda3ff4c62849"
BEAD_PACKAGES = {
    "bead",
    "bead-test",
    "bead-services",
    "bead-backends",
    "bead-core",
    "bead-kit",
    "bead-widgets",
    "bead-ui",
    "bead-wayland",
    "bead-render",
    "bead-wallpaper",
}


def run(command, *, capture=False):
    result = subprocess.run(
        command,
        cwd=ROOT,
        text=True,
        capture_output=capture,
        stdin=subprocess.DEVNULL,
    )
    if result.returncode:
        if capture:
            print(result.stdout, end="")
            print(result.stderr, end="", file=sys.stderr)
        raise SystemExit(result.returncode)
    return result


def download_analyzer(destination):
    destination = destination.resolve()
    destination.parent.mkdir(parents=True, exist_ok=True)
    archive = destination.parent / "luau-0.742.zip"
    run(["curl", "-fsSL", LUAU_URL, "-o", str(archive)])
    check = subprocess.run(
        ["sha256sum", "-c", "-"],
        input=f"{LUAU_SHA256}  {archive}\n",
        cwd=ROOT,
        text=True,
        capture_output=True,
    )
    if check.returncode:
        raise RuntimeError("Luau 0.742 archive checksum failed: " + check.stderr)
    run(["unzip", "-qo", str(archive), "-d", str(destination.parent)])
    analyzer = destination.parent / "luau-analyze"
    if not analyzer.is_file():
        raise RuntimeError("Luau 0.742 archive did not contain luau-analyze")
    analyzer.chmod(0o755)
    if analyzer != destination:
        analyzer.rename(destination)
    print(f"Luau analyzer 0.742 installed at {destination}", flush=True)


def framework_checkout():
    metadata = json.loads(
        run(
            [
                "cargo",
                "metadata",
                "--locked",
                "--all-features",
                "--format-version",
                "1",
            ],
            capture=True,
        ).stdout
    )
    packages = [
        package for package in metadata["packages"] if package["name"] in BEAD_PACKAGES
    ]
    facades = [package for package in packages if package["name"] == "bead"]
    if len(facades) != 1:
        raise RuntimeError("Cargo metadata must contain one pinned Bead facade")
    source = facades[0]["source"]
    if not isinstance(source, str):
        raise RuntimeError("The Bead facade did not resolve from its pinned Git source")
    prefix, separator, revision = source.partition("#")
    if (
        not separator
        or not prefix.startswith("git+https://github.com/nacre-desktop/bead?")
        or revision != BEAD_REVISION
    ):
        raise RuntimeError(
            "Cargo metadata did not resolve the frozen Bead revision " + BEAD_REVISION
        )
    framework_packages = [
        package for package in packages if package["source"] == source
    ]
    found = {package["name"]: package for package in framework_packages}
    if set(found) != BEAD_PACKAGES:
        raise RuntimeError(
            "Cargo metadata omitted pinned Bead framework packages: "
            + repr(BEAD_PACKAGES - set(found))
        )
    bead_root = Path(found["bead"]["manifest_path"]).resolve().parents[2]
    for name in ("bead-test", "bead-wallpaper", "bead-widgets"):
        package_source = found[name]["source"]
        if not isinstance(package_source, str) or not package_source.endswith(
            "#" + BEAD_REVISION
        ):
            raise RuntimeError(
                f"{name} did not resolve from frozen Bead revision {BEAD_REVISION}"
            )
    for package in framework_packages:
        manifest_path = Path(package["manifest_path"]).resolve()
        if not manifest_path.is_relative_to(bead_root):
            raise RuntimeError(
                f"{package['name']} is outside the pinned Bead checkout: {manifest_path}"
            )
        if package["source"] is not None and package["source"] != source:
            raise RuntimeError(
                f"{package['name']} resolved from a different Git source"
            )
    for relative in (
        "crates/bead-kit/assets/kit.luau",
        "crates/bead-widgets/src/luau/measure.luau",
    ):
        if not (bead_root / relative).is_file():
            raise RuntimeError(
                f"Pinned Bead checkout is missing {relative}: {bead_root}"
            )
    print(f"Pinned Bead framework checkout: {revision} ({bead_root})", flush=True)
    return bead_root


def native_checks(binary, analyzer):
    binary, analyzer = binary.resolve(), analyzer.resolve()
    if not binary.is_file() or not analyzer.is_file():
        raise SystemExit("The built nacre-shell binary and Luau analyzer must exist")
    bead_root = framework_checkout()
    run([str(binary), "--check", "shell/ui/main.luau"])
    run(
        [
            sys.executable,
            "tools/check_luau.py",
            "--analyzer",
            str(analyzer),
            "--binary",
            str(binary),
            "--framework-root",
            str(bead_root),
            "--check-generated",
        ]
    )
    run(
        [
            sys.executable,
            "tools/wallpaper_video_smoke.py",
            "--engine",
            str(binary.with_name("bead-wallpaper-video")),
            "--expect-presentation",
            "shm",
        ]
    )
    print(
        "Bead shell, generated Luau schema and wallpaper renderer checks passed.",
        flush=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--download-analyzer", type=Path, metavar="DESTINATION")
    parser.add_argument("--analyzer", type=Path)
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    parser.add_argument("--binary", type=Path, default=target_dir / "debug/nacre-shell")
    args = parser.parse_args()
    if args.download_analyzer is not None:
        download_analyzer(args.download_analyzer)
        return
    if args.analyzer is None:
        parser.error("--analyzer is required for native checks")
    native_checks(args.binary, args.analyzer)


if __name__ == "__main__":
    main()
