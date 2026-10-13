#!/usr/bin/env python3
"""Provision a tested immutable Orient environment without mutating active releases."""

import hashlib
from nacre_i18n import tr
import json
import os
import re
from pathlib import Path
import shutil
import subprocess
import tempfile

HOME = Path.home()
ROOT = Path(__file__).resolve().parent
RUNTIME = HOME / ".local/share/nacre/palette-runtime"
PACKAGES = (
    "librsvg",
    "quickshell",
    "qt6-imageformats",
    "qt6-multimedia",
    "qt6-multimedia-ffmpeg",
    "ddcutil",
    "cpptrace",
    "libdwarf",
    "wtype",
    "papirus-icon-theme",
)


def source_digest(source):
    digest = hashlib.sha256()
    for path in sorted(source.rglob("*")):
        if path.is_file() and not any(
            p
            in (
                "__pycache__",
                "build",
                "dist",
                ".git",
                ".ruff_cache",
                ".pytest_cache",
                "target",
            )
            or p.endswith(".egg-info")
            for p in path.parts
        ):
            digest.update(str(path.relative_to(source)).encode())
            digest.update(path.read_bytes())
    return digest.hexdigest()


def activate_runtime(candidate, runtime, builds):
    retained = None
    pending = runtime.with_name(".palette-runtime.next")
    try:
        pending.unlink(missing_ok=True)
        pending.symlink_to(candidate)
        if runtime.exists() and not runtime.is_symlink():
            retained = builds / ("legacy-" + candidate.name)
            runtime.rename(retained)
        os.replace(pending, runtime)
    except BaseException:
        pending.unlink(missing_ok=True)
        if retained is not None and not runtime.exists():
            retained.rename(runtime)
        raise


def binary_info(path):
    reply = subprocess.run(
        [str(path), "--rpc"],
        input=json.dumps({"operation": "info", "payload": {}}),
        capture_output=True,
        text=True,
        timeout=10,
        check=True,
    )
    return json.loads(reply.stdout)["result"]


def prepare_binary(engine, candidate, pin):
    choices = [os.environ.get("NACRE_ORIENT_BINARY"), shutil.which("orient")]
    for choice in choices:
        if not choice:
            continue
        try:
            info = binary_info(choice)
            if info.get("backend") == "rust" and info.get("engine") == pin["engine"]:
                return Path(choice)
        except (OSError, ValueError, KeyError, subprocess.SubprocessError):
            pass
    cargo = shutil.which("cargo")
    if not cargo:
        raise RuntimeError(
            "Install Rust and LCMS2 build tools, or supply the tested native Orient binary"
        )
    target = candidate / "cargo-target"
    subprocess.run(
        [
            cargo,
            "build",
            "--release",
            "--locked",
            "--manifest-path",
            str(engine / "Cargo.toml"),
            "--target-dir",
            str(target),
            "-p",
            "orient-cli",
        ],
        check=True,
    )
    binary = target / "release/orient"
    info = binary_info(binary)
    if info.get("backend") != "rust" or info.get("engine") != pin["engine"]:
        raise RuntimeError("Built Orient binary differs from the tested dependency pin")
    return binary


def main():
    result = subprocess.run(["pacman", "-Q", *PACKAGES], capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(
            tr(
                "helper.provision.install-desktop-dependencies-with-pacman-first-value0",
                "Install desktop dependencies with pacman first:\n{value0}",
                {"value0": result.stderr},
            )
        )
    source = ROOT.parent / "shell-cli"
    engine = ROOT.parents[1] / "deps/orient"
    if not (engine / "pyproject.toml").is_file():
        raise RuntimeError(
            tr(
                "helper.provision.initialize-orient-with-git-submodule-update-init-recursive",
                "Initialize Orient with git submodule update --init --recursive",
            )
        )
    lock = json.loads((ROOT.parents[1] / "dependencies.lock.json").read_text())
    pillow = lock["python"]["pillow"]
    if not re.fullmatch(r"[0-9]+\.[0-9]+\.[0-9]+", pillow):
        raise ValueError("Invalid locked Pillow version")
    identity = hashlib.sha256(
        (
            source_digest(source)
            + source_digest(engine)
            + json.dumps(lock, sort_keys=True)
        ).encode()
    ).hexdigest()
    marker = RUNTIME / "orient-build.json"
    if marker.exists() and json.loads(marker.read_text()).get("source") == identity:
        if (RUNTIME / "venv/bin/orient").is_file():
            info = binary_info(RUNTIME / "venv/bin/orient")
            if (
                info.get("engine") == lock["orient"]["engine"]
                and info.get("backend") == "rust"
            ):
                print("Native Orient environment already matches source")
                return
    # A fixed final directory keeps venv shebangs valid. It is not active until tested.
    builds = RUNTIME.parent / "palette-engines"
    builds.mkdir(parents=True, exist_ok=True)
    candidate = Path(
        tempfile.mkdtemp(prefix="orient-" + identity[:12] + "-", dir=builds)
    )
    try:
        venv = candidate / "venv"
        subprocess.run(["/usr/bin/python3", "-m", "venv", str(venv)], check=True)
        native = prepare_binary(engine, candidate, lock["orient"])
        shutil.copy2(native, venv / "bin/orient")
        subprocess.run(
            [
                str(venv / "bin/pip"),
                "install",
                str(source),
                "pillow==" + pillow,
            ],
            check=True,
        )
        subprocess.run(
            [
                str(venv / "bin/python"),
                "-c",
                "from nacre_shell.palette import generate; generate('d01818','dark'); generate('477fe0','light')",
            ],
            check=True,
        )
        engine_id = subprocess.check_output(
            [
                str(venv / "bin/python"),
                "-c",
                "from nacre_shell import ENGINE_ID; print(ENGINE_ID)",
            ],
            text=True,
        ).strip()
        (candidate / "orient-build.json").write_text(
            json.dumps(
                {"source": identity, "engine": engine_id, "backend": "rust-binary"}
            )
            + "\n"
        )
        shutil.rmtree(candidate / "cargo-target", ignore_errors=True)
        activate_runtime(candidate, RUNTIME, builds)
    except BaseException:
        # Candidate was never promoted; active runtime remains unchanged.
        if not RUNTIME.is_symlink() or RUNTIME.resolve() != candidate:
            shutil.rmtree(candidate)
        raise
    print("Orient runtime validated and activated")


if __name__ == "__main__":
    main()
