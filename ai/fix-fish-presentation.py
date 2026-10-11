#!/usr/bin/env python3
"""Guard only the known presentation block; leave PATH and shell setup intact."""

import datetime as dt
from pathlib import Path
import shutil
import subprocess

target = (Path.home() / ".config/fish/conf.d/30-autostart.fish").resolve()
text = target.read_text()
before = "$HOME/.config/fastfetch/render-logo.sh >/dev/null 2>&1\nfastfetch\n"
after = "if status is-interactive\n    $HOME/.config/fastfetch/render-logo.sh >/dev/null 2>&1\n    fastfetch\nend\n"
if after in text:
    print("Fish presentation already limited to interactive shells")
elif text.count(before) != 1:
    raise SystemExit("Unexpected fish presentation block; inspect before editing")
else:
    backup = (
        Path.home()
        / ".local/state/nacre/ai/backups"
        / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    )
    backup.mkdir(parents=True, mode=0o700)
    shutil.copy2(target, backup / "30-autostart.fish")
    updated = text.replace(before, after)
    subprocess.run(["fish", "--no-execute"], input=updated, text=True, check=True)
    target.write_text(updated)
    print("Guarded fish presentation; original saved in", backup)
