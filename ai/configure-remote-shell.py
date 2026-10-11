#!/usr/bin/env python3
"""Expose user tools before Bash's noninteractive return, with a dated backup."""

import datetime as dt
from pathlib import Path
import shutil
import subprocess

target = Path.home() / ".bashrc"
text = target.read_text()
line = 'export PATH="$HOME/.local/bin:$PATH"\n'
guard = "case $- in\n"
if guard not in text:
    raise SystemExit("Unrecognized Bash startup layout; inspect before editing")
prefix, suffix = text.split(guard, 1)
if line in prefix:
    print("User tools already available to noninteractive SSH")
else:
    backup = (
        Path.home()
        / ".local/state/nacre/ai/backups"
        / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    )
    backup.mkdir(parents=True, mode=0o700)
    shutil.copy2(target, backup / "bashrc")
    updated = prefix + line + guard + suffix.replace(line, "")
    subprocess.run(["bash", "-n"], input=updated, text=True, check=True)
    target.write_text(updated)
    print("Moved user tool PATH before interactive guard; backup:", backup)
