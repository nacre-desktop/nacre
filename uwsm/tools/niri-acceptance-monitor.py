#!/usr/bin/env python3
"""Private, bounded observation of a real Hyprland → niri → Hyprland cycle."""

import argparse
import datetime as dt
import hashlib
import json
from pathlib import Path
import subprocess
import time

UNITS = (
    "wayland-wm@hyprland.desktop.service",
    "wayland-wm@niri.desktop.service",
    "nacre-shell.service",
    "nacre-session-watch.service",
    "hypridle.service",
)
SESSION_NAMES = {"XDG_CURRENT_DESKTOP", "WAYLAND_DISPLAY", "NIRI_SOCKET"}


def query(arguments):
    try:
        result = subprocess.run(arguments, capture_output=True, text=True, timeout=4)
        return result.stdout if result.returncode == 0 or result.stdout else ""
    except (OSError, subprocess.SubprocessError):
        return ""


def snapshot():
    text = query(
        [
            "systemctl",
            "--user",
            "show",
            *UNITS,
            "-p",
            "Id",
            "-p",
            "ActiveState",
            "-p",
            "MainPID",
        ]
    )
    units = {}
    for block in text.split("\n\n"):
        fields = dict(line.split("=", 1) for line in block.splitlines() if "=" in line)
        if fields.get("Id") in UNITS:
            units[fields["Id"]] = (
                fields.get("ActiveState") == "active"
                and fields.get("MainPID", "0") != "0"
            )
    # Activation values stay only in memory. Persist booleans, not socket paths,
    # usernames, environment dumps or any window/account content.
    environment = dict(
        line.split("=", 1)
        for line in query(["systemctl", "--user", "show-environment"]).splitlines()
        if "=" in line
    )
    values = {name: environment.get(name, "") for name in SESSION_NAMES}
    niri = units.get(UNITS[1], False)
    hyprland = units.get(UNITS[0], False)
    return dict(
        compositor="niri"
        if niri and not hyprland
        else "hyprland"
        if hyprland and not niri
        else "none"
        if not niri and not hyprland
        else "ambiguous",
        units=units,
        niriSocketPresent=bool(values["NIRI_SOCKET"]),
        waylandDisplayPresent=bool(values["WAYLAND_DISPLAY"]),
        desktopIsNiri="niri" in values["XDG_CURRENT_DESKTOP"].lower().split(":"),
        desktopIsHyprland="hyprland"
        in values["XDG_CURRENT_DESKTOP"].lower().split(":"),
    )


def protected_files_match(backup, home):
    try:
        original = json.loads((backup / "baseline-hashes.json").read_text())
        if not isinstance(original, dict) or not original:
            return False
        for name, expected in original.items():
            path = Path(name)
            if path.is_absolute() or ".." in path.parts:
                return False
            if hashlib.sha256((home / path).read_bytes()).hexdigest() != expected:
                return False
        return True
    except (OSError, ValueError, TypeError):
        return False


class Cycle:
    def __init__(self):
        self.saw_niri = False
        self.plain_niri_isolated = False
        self.returned_hyprland = False
        self.isolation_violation = False

    def observe(self, sample):
        if (
            sample["compositor"] == "niri"
            and sample["niriSocketPresent"]
            and sample["desktopIsNiri"]
        ):
            self.saw_niri = True
            self.isolation_violation |= any(
                sample["units"].get(name, False) for name in UNITS[2:]
            )
            self.plain_niri_isolated = not self.isolation_violation
        if (
            self.saw_niri
            and sample["compositor"] == "hyprland"
            and sample["desktopIsHyprland"]
            and not sample["niriSocketPresent"]
            and sample["units"].get("nacre-shell.service", False)
        ):
            self.returned_hyprland = True
        return dict(
            niriSessionObserved=self.saw_niri,
            plainNiriIsolated=self.plain_niri_isolated,
            returnedHyprland=self.returned_hyprland,
        )


def write(path, data):
    temporary = path.with_suffix(".next")
    temporary.write_text(json.dumps(data, indent=2))
    temporary.chmod(0o600)
    temporary.replace(path)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--seconds", type=int, default=3600)
    args = parser.parse_args()
    if not 10 <= args.seconds <= 7200:
        parser.error("Observation duration must be between 10 and 7200 seconds")
    home = Path.home()
    backup = Path(
        (home / ".local/state/nacre/backups/niri-on-pc-latest").read_text().strip()
    )
    cycle = Cycle()
    previous = None
    deadline = time.monotonic() + args.seconds
    while time.monotonic() < deadline:
        sample = snapshot()
        result = cycle.observe(sample)
        current = dict(sample, **result)
        if current != previous:
            current["protectedFilesMatch"] = protected_files_match(backup, home)
            current["checked"] = dt.datetime.now(dt.timezone.utc).isoformat()
            write(backup / "acceptance-observed.json", current)
            with (backup / "acceptance-events.jsonl").open("a") as stream:
                stream.write(json.dumps(current) + "\n")
            (backup / "acceptance-events.jsonl").chmod(0o600)
            previous = dict(sample, **result)
        if cycle.returned_hyprland:
            # Observation is not operator confirmation of greeter selection, TTY
            # keyboard recovery, appearance or password unlock. Never mark those.
            return (
                0
                if cycle.plain_niri_isolated and protected_files_match(backup, home)
                else 1
            )
        time.sleep(2)
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
