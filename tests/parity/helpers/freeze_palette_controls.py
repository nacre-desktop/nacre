#!/usr/bin/env python3
"""Capture newly integrated main controls once; never replace existing references."""

import hashlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[3]
DEST = ROOT / "crates/nacre-helper/tests/fixtures/palette-controls-reference.json"
REVISION = "e910ffd7202a332c652ef227ddcf5970c39714bb"


def main():
    if DEST.exists():
        raise ValueError("Refusing to replace control references")
    with tempfile.TemporaryDirectory(prefix="nacre-controls-original-") as directory:
        source = Path(directory)
        archive = subprocess.check_output(
            ["git", "archive", REVISION, "nacre/shell-tools", "nacre/shell-cli"],
            cwd=ROOT,
        )
        with tarfile.open(fileobj=io.BytesIO(archive)) as files:
            files.extractall(source, filter="data")
        code = """import importlib.util,json,sys
from pathlib import Path
source=Path(sys.argv[1]);home=Path.home()
sys.path[:0]=[str(source/"nacre/shell-tools"),str(source/"nacre/shell-cli/src")]
spec=importlib.util.spec_from_file_location("original",source/"nacre/shell-tools/classic-state.py")
module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
colors=json.loads((source/"nacre/shell-tools/reference-style.json").read_text())["colours"]
folder=home/".config/nacre";folder.mkdir(parents=True)
rows=[]
for value in [None,True,False]:
 (folder/"desktop.json").write_text(json.dumps({} if value is None else {"interfaceGradients":value}))
 rows.append(dict(value=value,text=module.publish_compositor_palette(home,colors)))
print(json.dumps(rows))
"""
        cases = []
        for backend in ("hyprland", "niri"):
            home = source / (backend + "-home")
            home.mkdir()
            env = dict(
                os.environ,
                HOME=str(home),
                XDG_CONFIG_HOME=str(home / ".config"),
                XDG_STATE_HOME=str(home / ".local/state"),
                XDG_CACHE_HOME=str(home / ".cache"),
                NACRE_COMPOSITOR=backend,
                NACRE_ORIENT_BINARY=os.environ["NACRE_ORIENT_BINARY"],
                NACRE_HELPER_BINARY=str(ROOT / "target/debug/nacre-helper"),
            )
            result = subprocess.run(
                [sys.executable, "-c", code, str(source)],
                env=env,
                text=True,
                capture_output=True,
                check=True,
                timeout=60,
            )
            cases.append(dict(backend=backend, rows=json.loads(result.stdout)))
        colors = json.loads(
            (source / "nacre/shell-tools/reference-style.json").read_text()
        )["colours"]
        raw = (source / "nacre/shell-tools/classic-state.py").read_bytes()
        DEST.write_text(
            json.dumps(
                dict(
                    commit=REVISION,
                    source_sha256=hashlib.sha256(raw).hexdigest(),
                    colors=colors,
                    cases=cases,
                ),
                indent=2,
            )
            + "\n"
        )
        print("Captured six original-main gradient cases; prior references unchanged")


if __name__ == "__main__":
    main()
