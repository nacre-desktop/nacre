"""Developer-only adapter retaining the original artwork/cache assertions."""

import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
ASSETS = ROOT / "nacre/shell-tools/demo-wallpapers"


def binary(module_api=False):
    name = "NACRE_PARITY_API" if module_api else "NACRE_HELPER_BINARY"
    default = (
        "target/debug/examples/parity-api"
        if module_api
        else "target/debug/nacre-helper"
    )
    return os.environ.get(name, str(ROOT / default))


def collection():
    result = subprocess.run(
        [binary(True), "demo-wallpapers", "collection", '{"args": [], "kwargs": {}}'],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)


def ensure(home, fast=False):
    result = subprocess.run(
        [
            binary(),
            "demo-wallpapers",
            "--home",
            str(home),
            *(["--fast"] if fast else []),
        ],
        check=True,
        capture_output=True,
        text=True,
    )
    return json.loads(result.stdout)
