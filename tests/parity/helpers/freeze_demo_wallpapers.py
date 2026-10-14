#!/usr/bin/env python3
"""Freeze the current offline collection/cache behavior without rewriting goldens."""

import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/demo-wallpapers.py"
SOURCE_SHA = "55da8cf370c09068fb0fcba53289a576a07633932491a6c803fee6fc1915b24c"
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/demo-wallpapers-reference.json"


def capture():
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite demo reference")
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Demo source changed")
    sys.path.insert(0, str(SOURCE.parent))
    spec = importlib.util.spec_from_file_location("demo_reference", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    manifest_bytes = (module.ASSETS / "manifest.json").read_bytes()
    entries = module.collection()
    cases = []
    for name in ("fresh", "repeat", "fast", "damaged", "missing"):
        with tempfile.TemporaryDirectory(prefix="nacre-demo-reference-") as folder:
            home = Path(folder)
            if name != "fresh":
                first = module.ensure(home)
                if name == "damaged":
                    Path(first[0]["path"]).write_bytes(b"broken cache")
                elif name == "missing":
                    Path(first[1]["path"]).unlink()
            if name == "fast":
                with patch.object(
                    module,
                    "collection",
                    side_effect=AssertionError("Fast path must use stat receipt"),
                ):
                    result = module.ensure(home, fast=True)
            else:
                result = module.ensure(home, fast=name in ("damaged", "missing"))
            normalized = [
                {**row, "path": row["path"].replace(str(home), "/fixture/home")}
                for row in result
            ]
            files = {}
            for path in sorted(home.rglob("*")):
                if not path.is_file():
                    continue
                if path.name == "verified.json":
                    receipt = json.loads(path.read_text())
                    data = {}
                    for key, values in receipt.items():
                        original = Path(key)
                        current = original.stat()
                        assert values == [
                            current.st_dev,
                            current.st_ino,
                            current.st_size,
                            current.st_mtime_ns,
                            current.st_ctime_ns,
                        ]
                        data[
                            key.replace(str(module.ASSETS), "/fixture/assets").replace(
                                str(home), "/fixture/home"
                            )
                        ] = ["@DEV@", "@INODE@", values[2], "@MTIME_NS@", "@CTIME_NS@"]
                    content = {"json": data}
                else:
                    content = {
                        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
                        "size": path.stat().st_size,
                    }
                files[str(path.relative_to(home))] = dict(
                    content, mode=path.stat().st_mode & 0o777
                )
            cases.append(
                {
                    "operation": "ensure",
                    "name": name,
                    "result": normalized,
                    "files": files,
                    "pictures_created": (home / "Pictures").exists(),
                }
            )
    for name in (
        "version",
        "count",
        "filename",
        "suffix",
        "checksum",
        "resolution",
        "license",
        "artist",
        "source",
    ):
        with tempfile.TemporaryDirectory(prefix="nacre-demo-invalid-") as folder:
            assets = Path(folder)
            manifest = json.loads(manifest_bytes)
            for entry in entries:
                (assets / entry["file"]).symlink_to(module.ASSETS / entry["file"])
            row = manifest["wallpapers"][0]
            if name == "version":
                manifest["version"] = 2
            elif name == "count":
                manifest["wallpapers"] = manifest["wallpapers"][:3]
            elif name == "filename":
                row["file"] = "../outside.jpg"
            elif name == "suffix":
                row["file"] = "unsafe.JPG"
            elif name == "checksum":
                row["sha256"] = "0" * 64
            elif name == "resolution":
                row["size"] = [1, 1]
            elif name == "license":
                row["license"] = "unknown"
            elif name == "artist":
                row["artist"] = ""
            else:
                row["source"] = ""
            (assets / "manifest.json").write_text(json.dumps(manifest))
            with patch.object(module, "ASSETS", assets):
                try:
                    module.collection()
                except (ValueError, OSError, KeyError) as error:
                    observed = {"type": type(error).__name__, "message": str(error)}
                else:
                    raise AssertionError("Invalid manifest unexpectedly accepted")
            cases.append(
                {
                    "operation": "collection",
                    "name": name,
                    "manifest": manifest,
                    "error": observed,
                }
            )
    DESTINATION.write_text(
        json.dumps(
            {
                "source_sha256": SOURCE_SHA,
                "manifest_sha256": hashlib.sha256(manifest_bytes).hexdigest(),
                "normalizations": "Only disposable/source roots and filesystem device/inode/mtime/ctime are canonicalized; receipt values are first checked against actual stat; cached image SHA256/bytes and all other fields exact.",
                "collection": entries,
                "cases": cases,
            },
            indent=2,
        )
        + "\n"
    )
    print(f"Frozen {len(cases)} demo collection/cache cases")


if __name__ == "__main__":
    capture()
