#!/usr/bin/env python3
"""Capture current width policy outputs before retiring the Python observer."""

import argparse
import copy
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout, redirect_stderr
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/compositor_backends/niri_widths.py"
SOURCE_SHA = "ae83f0561b32dd7b28f7266e2dbd7578354d82d78174be85a747fb0c62bb9c37"
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/niri-widths-reference.json"


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Niri widths reference source is absent or changed")


def window(**values):
    return {
        "id": 1,
        "app_id": "Fixture",
        "title": "Window",
        "is_floating": False,
        **values,
    }


def requests():
    base = [{"event": {"WindowsChanged": {"windows": [window()]}}}, {"reconcile": True}]
    cases = {
        "restore-floating": [
            {"event": {"WindowOpenedOrChanged": {"window": window(is_floating=True)}}},
            {"reconcile": True},
        ],
        "app-new-minimum": [
            {"hints": {"min_width": 2400}},
            {"reconcile": True},
            {"close": True},
        ],
        "fixed-dialog": [
            {"ordinary": False},
            {"reconcile": True},
            {"ordinary": True},
            {"hints": {"max_width": 1600}},
            {"reconcile": True},
        ],
        "mixed-floating-identity": [
            {
                "event": {
                    "WindowOpenedOrChanged": {"window": window(id=2, is_floating=True)}
                }
            },
            {"reconcile": True},
        ],
        "minimum-removed": [
            {"hints": {"flags": 32}},
            {"reconcile": True},
            {"close": True},
        ],
        "closed-window": [{"event": {"WindowClosed": {"id": 1}}}, {"reconcile": True}],
        "asynchronous-title": [
            {"identity": [["fixture"], "New title"]},
            {"reconcile": True},
            {"event": {"WindowOpenedOrChanged": {"window": window(title="New title")}}},
            {"reconcile": True},
        ],
        "new-maximum": [{"hints": {"max_width": 900}}, {"close": True}],
        "native-title-no-requery": [
            {
                "event": {
                    "WindowOpenedOrChanged": {
                        "window": window(
                            id=2, app_id="nacre-ai-task", title="Spinner update"
                        )
                    }
                }
            }
        ],
        "one-opening-correction": [
            {"apply": 999},
            {"event": {"WindowOpenedOrChanged": {"window": window(id=2)}}},
            {"hints": {"min_width": 2400}},
            {"reconcile": True},
            {"apply": 100.1},
            {"apply": 999},
            {"hints": {"min_width": 2800}},
            {"reconcile": True},
            {"apply": 999},
        ],
        "floating-cancels-opening": [
            {"event": {"WindowOpenedOrChanged": {"window": window(id=2)}}},
            {"hints": {"min_width": 2400}},
            {"reconcile": True},
            {
                "event": {
                    "WindowOpenedOrChanged": {"window": window(id=2, is_floating=True)}
                }
            },
            {"reconcile": True},
            {"apply": 999},
        ],
        "closed-cancels-opening": [
            {"event": {"WindowOpenedOrChanged": {"window": window(id=2)}}},
            {"hints": {"min_width": 2400}},
            {"reconcile": True},
            {"event": {"WindowClosed": {"id": 2}}},
            {"apply": 999},
        ],
        "missing-identity": [{"identity": None}, {"reconcile": True}, {"close": True}],
        "missing-hints": [
            {"hints_available": False},
            {"reconcile": True},
            {"close": True},
        ],
        "x11-window-removed": [{"present": []}, {"reconcile": True}, {"close": True}],
        "unrelated-event": [{"event": {"WorkspaceActivated": {"id": 2}}}],
        "unknown-floating-state": [
            {"event": {"WindowOpenedOrChanged": {"window": window(is_floating=None)}}},
            {"reconcile": True},
        ],
        "unicode-class": [
            {"identity": [["strasse"], "Window"]},
            {"event": {"WindowsChanged": {"windows": [window(app_id="Straße")]}}},
            {"reconcile": True},
        ],
    }
    for name, steps in cases.items():
        yield {"name": name, "steps": base + steps}
    for flags, minimum, maximum in [
        (0, 1600, 4000),
        (16, 1600, 4000),
        (48, 1, 4000),
        (48, 0, 4000),
        (48, 1600, 1600),
        (48, 1600, 900),
        (48, -1, 4000),
    ]:
        yield {
            "name": f"initial-hints-{flags}-{minimum}-{maximum}",
            "steps": [
                {"hints": {"flags": flags, "min_width": minimum, "max_width": maximum}},
                *base,
                {"close": True},
            ],
        }


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_width_policy", SOURCE)
    widths = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(widths)

    class X11:
        def __init__(self):
            self.value = widths.SizeHints()
            self.value.flags = 48
            self.value.min_width, self.value.min_height = 1600, 600
            self.value.max_width, self.value.max_height = 4000, 2000
            self.value.width_inc, self.value.height_inc = 10, 20
            self.value.base_width, self.value.base_height = 30, 40
            self.value.min_aspect.x, self.value.min_aspect.y = 4, 3
            self.value.max_aspect.x, self.value.max_aspect.y = 16, 9
            self.value.win_gravity = 1
            self.present = [10]
            self.ident = ({"fixture"}, "Window")
            self.is_ordinary = True
            self.available = True
            self.calls = []

        def windows(self):
            self.calls.append(["windows"])
            return self.present

        def identity(self, window_id):
            self.calls.append(["identity", window_id])
            return self.ident

        def ordinary(self, window_id):
            self.calls.append(["ordinary", window_id])
            return self.is_ordinary

        def hints(self, window_id):
            self.calls.append(["hints", window_id])
            return (
                widths.SizeHints.from_buffer_copy(bytes(self.value))
                if self.available
                else None
            )

        def set_hints(self, window_id, hints):
            self.value = widths.SizeHints.from_buffer_copy(bytes(hints))
            self.calls.append(["set_hints", window_id, hint_values(self.value)])

    def hint_values(hints):
        return {
            name: (
                {"x": value.x, "y": value.y}
                if isinstance(value, widths.Pair)
                else value
            )
            for name, _ in widths.SizeHints._fields_
            for value in [getattr(hints, name)]
        }

    x11 = X11()
    policy = widths.Policy(x11)
    resizes = []
    observed = []
    stdout, stderr = io.StringIO(), io.StringIO()
    with (
        redirect_stdout(stdout),
        redirect_stderr(stderr),
        patch.object(widths.time, "monotonic", return_value=100),
    ):
        for step in request["steps"]:
            event_result = None
            if "hints" in step:
                for name, value in step["hints"].items():
                    setattr(x11.value, name, value)
            if "ordinary" in step:
                x11.is_ordinary = step["ordinary"]
            if "present" in step:
                x11.present = step["present"]
            if "hints_available" in step:
                x11.available = step["hints_available"]
            if "identity" in step:
                value = step["identity"]
                x11.ident = (set(value[0]), value[1]) if value is not None else None
            if "event" in step:
                event_result = policy.event(step["event"])
            if "reconcile" in step:
                policy.reconcile()
            if "apply" in step:
                policy.apply_openings(resizes.append, step["apply"])
            if "close" in step:
                policy.close()
            observed.append(
                copy.deepcopy(
                    {
                        "event_result": event_result,
                        "hints": hint_values(x11.value),
                        "calls": list(x11.calls),
                        "resizes": list(resizes),
                        "windows": policy.windows,
                        "saved": policy.saved,
                        "mapping": {
                            key: sorted(value) for key, value in policy.mapping.items()
                        },
                        "classes": sorted(policy.classes),
                        "initialized": policy.initialized,
                        "new": sorted(policy.new),
                        "opening": policy.opening,
                        "eligible": sorted(policy.eligible),
                    }
                )
            )
    return {
        "steps": observed,
        "stdout": stdout.getvalue(),
        "stderr": stderr.getvalue(),
        "files": {},
    }


def capture(destination):
    verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen Niri widths reference")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="nacre-widths-reference-") as folder:
            environment = dict(
                os.environ,
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_CACHE_HOME=folder + "/.cache",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-bus",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
            )
            for key in (
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "XDG_CURRENT_DESKTOP",
            ):
                environment.pop(key, None)
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--child",
                    json.dumps(request),
                ],
                env=environment,
                capture_output=True,
                text=True,
                check=True,
            )
        cases.append(
            {
                "request": request,
                "observed": json.loads(result.stdout),
                "exit": result.returncode,
                "stderr": result.stderr,
            }
        )
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as stream:
        json.dump({"source_sha256": SOURCE_SHA, "cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} Niri width policy cases")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DESTINATION)
    parser.add_argument("--child")
    args = parser.parse_args()
    if args.child:
        print(json.dumps(child(json.loads(args.child))))
    else:
        capture(args.output)


if __name__ == "__main__":
    main()
