#!/usr/bin/env python3
"""Run product checks moved from Bead, against Nacre's pinned framework source."""

import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
PRODUCT = ROOT / "bead-shell"
SUITES = (
    "test_dashboard_capture_windows.py",
    "test_menu_engine_trace.py",
    "test_b0003_arm.py",
    "test_b0002_arm.py",
    "test_laptop_reference_capture.py",
    "test_foundation_preview.py",
    "test_fake_system.py",
    "test_fullscreen_lips.py",
    "test_pearl_release_proof.py",
    "test_detach_ghost_proof.py",
    "test_launcher_icons_proof.py",
    "test_launcher_memory_proof.py",
    "test_scrolling_proof.py",
)
PY_COMPILE = (
    "capture_safety.py",
    "measure.py",
    "bar-matrix.py",
    "dashboard-residency.py",
    "launcher-proof.py",
    "b0002-arm.py",
    "laptop-reference-capture.py",
    "laptop-reference-artifacts.py",
    "parity-compare.py",
    "fake-bluez.py",
    "fake-power.py",
    "fake_system.py",
    "foundation-preview.py",
    "menu-engine-proof.py",
    "menu_engine_trace.py",
    "b0003-arm.py",
    "tests/test_dashboard_capture_windows.py",
    "tests/test_menu_engine_trace.py",
    "tests/test_b0002_arm.py",
    "tests/test_laptop_reference_capture.py",
    "tests/test_b0003_arm.py",
    "tests/test_fake_system.py",
    "tests/test_launcher_memory_proof.py",
)
LUAU_URL = "https://github.com/luau-lang/luau/releases/download/0.742/luau-ubuntu.zip"
LUAU_SHA256 = "152058b98c60ff1f5b5ec9bca0b8eb37793cae161dd3a05608cc3d8738ba1a4d"
BEAD_REVISION = "914412f97397eedf54fed3a568ab641ba59ffc15"
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


def suite(directory, pattern="test*.py"):
    result = subprocess.run(
        [
            sys.executable,
            "-m",
            "unittest",
            "discover",
            "-s",
            directory,
            "-p",
            pattern,
        ],
        cwd=ROOT,
        text=True,
        capture_output=True,
        stdin=subprocess.DEVNULL,
    )
    output = result.stdout + result.stderr
    if result.returncode:
        print(output, end="")
        raise SystemExit(result.returncode)
    match = re.search(r"Ran (\d+) tests?", output)
    if not match:
        print(output, end="")
        raise RuntimeError("Could not read unittest count for " + pattern)
    count = int(match.group(1))
    print(f"Bead product suite {directory}/{pattern}: {count} tests OK", flush=True)
    return count


def native_checks(binary, analyzer):
    binary, analyzer = binary.resolve(), analyzer.resolve()
    if not binary.is_file() or not analyzer.is_file():
        raise SystemExit("The built nacre-shell binary and Luau analyzer must exist")
    bead_root = framework_checkout()
    run([str(binary), "--check", "shell/ui/main.luau"])
    run(
        [
            sys.executable,
            str(PRODUCT / "tools/check-luau.py"),
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
            "bead-shell/spikes/wallpaper-video-smoke.py",
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


def python_checks():
    test_count = sum(suite("bead-shell/spikes/tests", pattern) for pattern in SUITES)
    py_compile = [str(PRODUCT / "spikes" / path) for path in PY_COMPILE]
    run([sys.executable, "-m", "py_compile", *py_compile])
    print(
        f"Bead product proof suites: {len(SUITES)} suites, {test_count} tests; "
        "py_compile list OK",
        flush=True,
    )
    package_count = suite("bead-shell/packaging/tests")
    print(
        f"Bead product Python tests: {len(SUITES) + 1} suites, "
        f"{test_count + package_count} tests passed.",
        flush=True,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--python-only", action="store_true")
    modes.add_argument("--native-only", action="store_true")
    modes.add_argument("--download-analyzer", type=Path, metavar="DESTINATION")
    parser.add_argument("--analyzer", type=Path)
    target_dir = Path(os.environ.get("CARGO_TARGET_DIR", ROOT / "target"))
    if not target_dir.is_absolute():
        target_dir = ROOT / target_dir
    parser.add_argument("--binary", type=Path, default=target_dir / "debug/nacre-shell")
    args = parser.parse_args()
    if args.download_analyzer is not None:
        download_analyzer(args.download_analyzer)
        return
    if not args.python_only:
        if args.analyzer is None:
            parser.error("--analyzer is required for native checks")
        native_checks(args.binary, args.analyzer)
    if not args.native_only:
        python_checks()


if __name__ == "__main__":
    main()
