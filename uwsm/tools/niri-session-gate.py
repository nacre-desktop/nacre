#!/usr/bin/env python3
"""ExecCondition for safe, opt-in niri acceptance without changing Hyprland."""

import argparse
import json
import os
from pathlib import Path


def compositor(environment):
    desktops = environment.get("XDG_CURRENT_DESKTOP", "").lower().split(":")
    # Explicit desktop context outranks stale sockets from an earlier session.
    if "hyprland" in desktops and "niri" not in desktops:
        return "hyprland"
    if "niri" in desktops and "hyprland" not in desktops:
        return "niri"
    if environment.get("NIRI_SOCKET") and not environment.get(
        "HYPRLAND_INSTANCE_SIGNATURE"
    ):
        return "niri"
    if environment.get("HYPRLAND_INSTANCE_SIGNATURE") and not environment.get(
        "NIRI_SOCKET"
    ):
        return "hyprland"
    return "unknown"


def allowed(action, environment=None, home=None):
    environment = os.environ if environment is None else environment
    home = Path.home() if home is None else Path(home)
    backend = compositor(environment)
    if backend == "hyprland":
        return True
    if backend != "niri":
        return False
    # Phase 1 does not load the Hyprland-only shell/observer or automatic locking.
    # Later steps explicitly record readiness; an unreadable record fails closed.
    try:
        record = json.loads(
            (home / ".config/nacre/niri-on-pc/acceptance.json").read_text()
        )
    except (OSError, ValueError):
        return False
    if not isinstance(record, dict) or record.get("phase") != "nacre":
        return False
    if action == "idle":
        return (
            record.get("backendReady") is True and record.get("unlockVerified") is True
        )
    if action == "observer":
        return (
            record.get("backendReady") is True and record.get("observerReady") is True
        )
    return record.get("backendReady") is True


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("shell", "observer", "idle"))
    args = parser.parse_args()
    # ExecCondition status 1 skips the unit cleanly; it is not a service failure.
    return 0 if allowed(args.action) else 1


if __name__ == "__main__":
    raise SystemExit(main())
