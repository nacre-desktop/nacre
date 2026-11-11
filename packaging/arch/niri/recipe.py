#!/usr/bin/env python3
"""Stage the pinned fork's own Arch recipe and its companion source files."""

import argparse
from pathlib import Path
import subprocess
import tempfile

SOURCE = "1028c5b919abac907c06b63c6610584e42bded0f"  # nacre r3079
URL = "https://github.com/nacre-desktop/nacre-niri.git"
FILES = ("PKGBUILD", "nacre-niri-session", "nacre-niri-fork.desktop")


def stage(repository, output):
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    for name in FILES:
        content = subprocess.check_output(
            ["git", "-C", str(repository), "show", f"{SOURCE}:packaging/arch/{name}"]
        )
        if name == "PKGBUILD":
            # The fork supports _commit for standalone recipe copies.
            content = f"_commit={SOURCE}\n".encode() + content
        (output if name == "PKGBUILD" else output.with_name(name)).write_bytes(content)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--fork-root", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.fork_root:
        stage(args.fork_root, args.output)
    else:
        with tempfile.TemporaryDirectory() as folder:
            subprocess.run(
                ["git", "clone", "--bare", "--filter=blob:none", URL, folder], check=True
            )
            stage(folder, args.output)
