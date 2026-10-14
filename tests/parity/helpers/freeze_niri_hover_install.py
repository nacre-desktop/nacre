#!/usr/bin/env python3
"""Freeze proof validation, manifest bytes and private backup behavior."""

import datetime
import importlib.util
import json
from pathlib import Path
import tempfile
from unittest.mock import patch

from freeze_niri_session_runtime import ROOT, verify_source

DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-hover-install-reference.json"
)


def capture():
    verify_source()
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite hover install reference")
    spec = importlib.util.spec_from_file_location(
        "hover_reference", ROOT / "tools/build-niri-overview-hover.py"
    )
    builder = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(builder)
    cases = []

    class Clock(datetime.datetime):
        @classmethod
        def now(cls, tz=None):
            return cls(2000, 1, 2, 3, 4, 5, 6000, tzinfo=datetime.timezone.utc)

    for variant in (
        "valid",
        "upgrade",
        *builder.PROOF_FIELDS,
        "wrong-source",
        "wrong-binary",
        "wrong-patch",
    ):
        with tempfile.TemporaryDirectory(prefix="nacre-hover-install-") as folder:
            home = Path(folder)
            source = home / "source"
            binary = source / "target/release/niri"
            binary.parent.mkdir(parents=True)
            binary.write_text("compiled compositor fixture\n")
            (source / "LICENSE").write_text("license fixture\n")
            proof = dict.fromkeys(builder.PROOF_FIELDS, True)
            proof.update(
                sourceRevision=builder.SOURCE,
                binarySha256=builder.digest(binary),
                patchSha256=builder.digest(builder.PATCH),
            )
            if variant in builder.PROOF_FIELDS:
                proof[variant] = False
            if variant.startswith("wrong-"):
                proof[
                    {
                        "wrong-source": "sourceRevision",
                        "wrong-binary": "binarySha256",
                        "wrong-patch": "patchSha256",
                    }[variant]
                ] = "wrong"
            root = home / ".local/share/nacre/niri/overview-hover"
            if variant == "upgrade":
                root.mkdir(parents=True)
                for name in ("niri", "manifest.json", "LICENSE.niri"):
                    (root / name).write_text("previous " + name + "\n")
                    (root / name).chmod(0o600)
            file = home / "proof.json"
            file.write_text(json.dumps(proof))
            error = None
            with patch.object(builder.datetime, "datetime", Clock):
                try:
                    builder.install(source, file, home)
                except ValueError as failure:
                    error = str(failure)
            files = {
                str(path.relative_to(root)): {
                    "text": path.read_text(),
                    "mode": path.stat().st_mode & 0o777,
                }
                for path in sorted(root.rglob("*"))
                if path.is_file()
            }
            cases.append(
                {"variant": variant, "proof": proof, "files": files, "error": error}
            )
    with DESTINATION.open("x") as stream:
        json.dump({"cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} proof/installation cases")


if __name__ == "__main__":
    capture()
