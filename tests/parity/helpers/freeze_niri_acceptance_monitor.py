#!/usr/bin/env python3
"""Freeze private acceptance observations before replacing their Python runtime."""

import contextlib
import datetime
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "uwsm/tools/niri-acceptance-monitor.py"
SOURCE_SHA = "3cb484564805015acba2722cc9547848b5cb01d91902dac5c4f806b5c5d25211"
DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-acceptance-monitor-reference.json"
)


def capture():
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Acceptance monitor source changed")
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite acceptance reference")
    spec = importlib.util.spec_from_file_location("reference_acceptance", SOURCE)
    monitor = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(monitor)
    cases = []
    snapshots = []
    for compositor in ("none", "hyprland", "niri", "ambiguous"):
        for socket in (False, True):
            for desktop in ("", "Hyprland", "niri", "Hyprland:NIRI"):
                records = [
                    {
                        "Id": name,
                        "ActiveState": "active"
                        if index == 0
                        and compositor in ("hyprland", "ambiguous")
                        or index == 1
                        and compositor in ("niri", "ambiguous")
                        else "inactive",
                        "MainPID": str(index + 10),
                    }
                    for index, name in enumerate(monitor.UNITS)
                ]
                activation = {
                    "XDG_CURRENT_DESKTOP": desktop,
                    "WAYLAND_DISPLAY": "private-display",
                    "NIRI_SOCKET": "private-socket" if socket else "",
                    "PRIVATE_SECRET": "never serialize",
                }
                text = "\n\n".join(
                    "\n".join(key + "=" + value for key, value in record.items())
                    for record in records
                )
                environment = "\n".join(
                    key + "=" + value for key, value in activation.items()
                )
                with patch.object(
                    monitor,
                    "query",
                    side_effect=lambda argv, text=text, environment=environment: (
                        environment if "show-environment" in argv else text
                    ),
                ):
                    snapshot = monitor.snapshot()
                snapshots.append(snapshot)
                cases.append(
                    {
                        "operation": "snapshot",
                        "units": records,
                        "environment": activation,
                        "observed": snapshot,
                    }
                )
    # Preserve the sticky isolation violation across all subsequent samples.
    for violation in (False, True):
        cycle = monitor.Cycle()
        samples = [snapshots[9], snapshots[22], snapshots[13], snapshots[9]]
        samples = json.loads(json.dumps(samples))
        for sample in samples:
            if sample["compositor"] == "hyprland":
                sample["units"]["nacre-shell.service"] = True
        samples[1]["units"]["hypridle.service"] = violation
        observed = [cycle.observe(sample) for sample in samples]
        cases.append({"operation": "cycle", "samples": samples, "observed": observed})
    for variant in (
        "valid",
        "changed",
        "empty",
        "invalid-json",
        "array",
        "absolute",
        "escape",
        "missing-file",
    ):
        with tempfile.TemporaryDirectory(prefix="nacre-acceptance-hashes-") as folder:
            home = Path(folder)
            backup = home / "backup"
            backup.mkdir()
            (home / "protected").write_text("original\n")
            hashes = {"protected": hashlib.sha256(b"original\n").hexdigest()}
            if variant == "changed":
                (home / "protected").write_text("changed\n")
            if variant == "empty":
                hashes = {}
            if variant == "array":
                hashes = []
            if variant == "absolute":
                hashes = {str(home / "protected"): "bad"}
            if variant == "escape":
                hashes = {"../escape": "bad"}
            if variant == "missing-file":
                hashes = {"missing": "bad"}
            manifest = "invalid" if variant == "invalid-json" else json.dumps(hashes)
            (backup / "baseline-hashes.json").write_text(manifest)
            cases.append(
                {
                    "operation": "protected_files_match",
                    "variant": variant,
                    "manifest": manifest.replace(str(home), "$HOME"),
                    "observed": monitor.protected_files_match(backup, home),
                }
            )

    class Clock(datetime.datetime):
        @classmethod
        def now(cls, tz=None):
            return cls(2000, 1, 2, 3, 4, 5, 6000, tzinfo=datetime.timezone.utc)

    for variant in ("success", "isolation-violation", "changed-file", "timeout"):
        with tempfile.TemporaryDirectory(prefix="nacre-acceptance-files-") as folder:
            home = Path(folder)
            backup = home / "backup"
            backup.mkdir()
            state = home / ".local/state/nacre/backups"
            state.mkdir(parents=True)
            (state / "niri-on-pc-latest").write_text(str(backup) + "\n")
            (home / "protected").write_text(
                "changed\n" if variant == "changed-file" else "original\n"
            )
            (backup / "baseline-hashes.json").write_text(
                json.dumps({"protected": hashlib.sha256(b"original\n").hexdigest()})
            )
            samples = json.loads(
                json.dumps(
                    [snapshots[9], snapshots[22], snapshots[9]]
                    if variant != "timeout"
                    else [snapshots[0]]
                )
            )
            for item in samples:
                if item["compositor"] == "hyprland":
                    item["units"]["nacre-shell.service"] = True
            samples[1 if len(samples) > 1 else 0]["units"]["hypridle.service"] = (
                variant == "isolation-violation"
            )
            index = [0]
            instant = [0]

            def sample(samples=samples, index=index):
                result = samples[min(index[0], len(samples) - 1)]
                index[0] += 1
                return result

            def sleep(seconds, instant=instant):
                instant[0] += seconds

            stdout, stderr = io.StringIO(), io.StringIO()
            with (
                patch.dict(os.environ, {"HOME": str(home)}),
                patch.object(
                    sys, "argv", ["niri-acceptance-monitor.py", "--seconds", "10"]
                ),
                patch.object(monitor, "snapshot", side_effect=sample),
                patch.object(
                    monitor.time,
                    "monotonic",
                    side_effect=lambda instant=instant: instant[0],
                ),
                patch.object(monitor.time, "sleep", side_effect=sleep),
                patch.object(monitor.dt, "datetime", Clock),
                contextlib.redirect_stdout(stdout),
                contextlib.redirect_stderr(stderr),
            ):
                exit_code = monitor.main()
            files = {
                path.name: {
                    "text": path.read_text(),
                    "mode": path.stat().st_mode & 0o777,
                }
                for path in sorted(backup.glob("acceptance-*"))
            }
            cases.append(
                {
                    "operation": "run",
                    "variant": variant,
                    "samples": samples,
                    "seconds": 10,
                    "checked": "2000-01-02T03:04:05.006000+00:00",
                    "observed": {
                        "files": files,
                        "stdout": stdout.getvalue(),
                        "stderr": stderr.getvalue(),
                        "exit": exit_code,
                    },
                }
            )
    with DESTINATION.open("x") as stream:
        json.dump({"source_sha256": SOURCE_SHA, "cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} private acceptance cases")


if __name__ == "__main__":
    capture()
