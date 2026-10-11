#!/usr/bin/env python3
"""Keep only private idle listeners; managed config owns sleep locking."""

import datetime as dt
from pathlib import Path
import re


def prepare(home=None):
    home = Path.home() if home is None else Path(home)
    target = home / ".config/nacre/hypridle.local.conf"
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.exists():
        text = target.read_text()

        # Historical full-policy overrides have a flat general block. Reject
        # unfamiliar general options instead of silently deleting preferences.
        def remove(match):
            body = re.sub(r"#.*", "", match[1])
            for line in body.splitlines():
                if line.strip() and line.split("=", 1)[0].strip() not in (
                    "lock_cmd",
                    "before_sleep_cmd",
                    "after_sleep_cmd",
                ):
                    raise RuntimeError(
                        "Unrecognized private idle general option; reconcile it before deployment"
                    )
            return "# Sleep hooks are managed by Nacre.\n"

        clean = re.sub(r"\bgeneral\s*\{([^{}]*)\}", remove, text)
        clean = clean.replace(
            "# Manual locking only. No idle timeout and no automatic lock before sleep.",
            "# No idle timeout. Sleep locking remains managed by Nacre.",
        )
        if clean == text:
            return
        backup = (
            home
            / ".local/state/nacre/backups"
            / dt.datetime.now(dt.timezone.utc).strftime("idle-%Y%m%dT%H%M%S%fZ")
        )
        backup.mkdir(parents=True, mode=0o700)
        (backup / "hypridle.local.conf").write_text(text)
        (backup / "hypridle.local.conf").chmod(0o600)
    else:
        # Missing personal policy means no idle timeout, while sleep still locks.
        clean = "# Optional idle listeners. No idle timeout configured.\n"
    temporary = target.with_suffix(".next")
    temporary.write_text(clean)
    temporary.chmod(0o600)
    temporary.replace(target)


if __name__ == "__main__":
    prepare()
