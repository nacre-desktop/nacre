#!/usr/bin/env python3
"""Stage the pinned Niri companion without replacing distro-owned files."""

import argparse
import importlib.util
import json
from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location(
    "companion_build", ROOT / "tools/build-niri-overview-hover.py"
)
builder = importlib.util.module_from_spec(spec)
spec.loader.exec_module(builder)


def stage(source, destination):
    source = builder.prepare(source)
    root = Path(destination) / "usr/lib/nacre/niri"
    root.mkdir(parents=True, exist_ok=True)
    binary = root / "niri"
    shutil.copyfile(source / "target/release/niri", binary)
    binary.chmod(0o755)
    (root / "manifest.json").write_text(
        json.dumps(
            {
                "schema": 1,
                "enabled": True,
                "sourceRevision": builder.SOURCE,
                "protocol": "NacreOverviewHoverStream",
                "patchSha256": builder.digest(builder.PATCH),
            },
            indent=2,
        )
        + "\n"
    )
    # Pacman ownership is the system trust boundary; makepkg may split debug data.
    # Only private source builds need a matching per-user binary checksum.
    public = Path(destination) / "usr/bin/nacre-niri"
    public.parent.mkdir(parents=True, exist_ok=True)
    public.write_text(
        '#!/bin/sh\nexport NACRE_NIRI_HOVER_STREAM=1\nexec /usr/lib/nacre/niri/niri "$@"\n'
    )
    public.chmod(0o755)
    licenses = Path(destination) / "usr/share/licenses/nacre-niri"
    licenses.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source / "LICENSE", licenses / "LICENSE.niri")
    shutil.copyfile(ROOT / "LICENSE", licenses / "LICENSE.nacre")
    shutil.copyfile(builder.PATCH, licenses / builder.PATCH.name)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("prepare", "stage"))
    parser.add_argument("source", type=Path)
    parser.add_argument("destination", type=Path, nargs="?")
    args = parser.parse_args()
    if args.action == "prepare":
        builder.prepare(args.source)
    elif args.destination is None:
        parser.error("stage requires a destination")
    else:
        stage(args.source, args.destination)
