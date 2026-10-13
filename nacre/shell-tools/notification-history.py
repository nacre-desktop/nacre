#!/usr/bin/env python3
"""Private notification snapshots; native action handles stay in the renderer."""

import json
from nacre_i18n import tr
import sys
import importlib.util
import re
from pathlib import Path

PATH = Path.home() / ".local/state/nacre/shell/notifications/history.json"
FIELDS = ("key", "time", "summary", "body", "appName", "appIcon", "image", "urgency")


POLICY = json.loads(Path(__file__).with_name("notification-policy.json").read_text())


def retained(row):
    if row.get("transient"):
        return False
    if row.get("appName", "").lower() not in POLICY["feedbackApps"]:
        return True
    summary = row.get("summary", "")
    return summary not in POLICY["feedbackSummaries"] and not any(
        re.search(pattern, summary) for pattern in POLICY["feedbackPatterns"]
    )


def load(path=PATH):
    if not path.exists():
        return []
    rows = json.loads(path.read_text())
    if not isinstance(rows, list):
        raise ValueError(
            tr(
                "helper.notification-history.invalid-notification-history",
                "Invalid notification history",
            )
        )
    return [row for row in rows if retained(row)]


def save(rows, path=PATH):
    if not isinstance(rows, list) or any(not isinstance(row, dict) for row in rows):
        raise ValueError(
            tr(
                "helper.notification-history.invalid-notification-history",
                "Invalid notification history",
            )
        )
    # Serialize display data only, never action objects or application handles.
    rows = [
        {key: row[key] for key in FIELDS if key in row} for row in rows if retained(row)
    ]
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    path.parent.chmod(0o700)
    spec = importlib.util.spec_from_file_location(
        "state", Path(__file__).with_name("classic-state.py")
    )
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    m.atomic_write(path, json.dumps(rows))
    path.chmod(0o600)


if __name__ == "__main__":
    if sys.argv[1] == "load":
        print(json.dumps(load()))
    elif sys.argv[1] == "save":
        save(json.loads(sys.stdin.readline()))
    else:
        raise ValueError(
            tr(
                "helper.notification-history.unknown-history-action",
                "Unknown history action",
            )
        )
