#!/usr/bin/env python3
"""Restore public modes on tracked files after checkout in a private container."""

import os
import stat
import subprocess
import sys
from pathlib import Path


def restore(root: Path) -> int:
    root = root.resolve()
    result = subprocess.run(
        [
            "git",
            "-c",
            f"safe.directory={root}",
            "-C",
            str(root),
            "ls-files",
            "--stage",
            "--recurse-submodules",
            "-z",
        ],
        check=True,
        stdout=subprocess.PIPE,
    )
    changed = 0
    for entry in result.stdout.split(b"\0"):
        if not entry:
            continue
        metadata, raw_path = entry.split(b"\t", 1)
        mode, _object, stage = metadata.split()
        if stage != b"0":
            raise RuntimeError("checkout has unmerged index entries")
        target_mode = {b"100644": 0o644, b"100755": 0o755}.get(mode)
        if target_mode is None:
            continue
        relative = Path(os.fsdecode(raw_path))
        path = root / relative
        current = root
        parts = relative.parts
        if relative.is_absolute() or any(part in ("..", ".") for part in parts):
            continue
        safe = True
        for part in parts[:-1]:
            current = current / part
            try:
                if not stat.S_ISDIR(current.lstat().st_mode):
                    safe = False
                    break
            except FileNotFoundError:
                safe = False
                break
        if not safe:
            continue
        try:
            file_stat = path.lstat()
        except FileNotFoundError:
            continue
        if (
            stat.S_ISREG(file_stat.st_mode)
            and stat.S_IMODE(file_stat.st_mode) != target_mode
        ):
            os.chmod(path, target_mode, follow_symlinks=False)
            changed += 1
    return changed


if __name__ == "__main__":
    try:
        count = restore(Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd())
    except (OSError, RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"public checkout mode restore failed: {exc}", file=sys.stderr)
        raise SystemExit(1) from None
    print(f"restored public modes on {count} tracked files")
