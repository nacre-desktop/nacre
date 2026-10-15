"""Developer-only historical provisioning contracts; no installed entry point.

Retained original functions support immutable fixtures and rollback assertions.
The retired Python environment provisioner and package installer are absent.
"""

import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess


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
            "Install Rust, CMake and LCMS2 build tools, or supply the tested native Orient binary"
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
