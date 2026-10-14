#!/usr/bin/env python3
"""Freeze current Hyprland semantic actions and pure rendering before porting."""

import argparse
import importlib.util
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile

import freeze_niri_state as state_reference

ROOT = state_reference.ROOT
DESTINATION = ROOT / "crates/nacre-helper/tests/fixtures/compositor-cli-reference.json"


SOURCE = ROOT / "nacre/shell-tools/compositor.py"
SOURCE_SHA = "b7fa8634e202599af946fd828fc8579b9c9db84b0836538747ac0800e8d555fc"


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Hyprland reference source is absent or changed")


def requests():
    actions = [
        ("reload", []),
        ("logout", []),
        ("toggle_workspace_floating", []),
        ("switch_workspace", ["--workspace", "2"]),
        (
            "switch_workspace",
            [
                "--workspace",
                '{"output":"DP-1","index":2}',
                "--current-monitor",
                "false",
            ],
        ),
        ("move_workspace", ["--workspace", "2", "--monitor", "DP-1"]),
        ("move_window", ["--window", "0xabc", "--workspace", "3", "--follow", "true"]),
        ("move_window", ["--window", "0xabc", "--x", "-30", "--y", "40"]),
        ("fullscreen_window", []),
        ("resize_window", ["--width", "600", "--height", "400", "--relative", "true"]),
        ("focus_direction", ["--direction", "left"]),
        ("move_direction", ["--direction", "down"]),
        ("cycle_window", []),
        (
            "place_window",
            [
                "--window",
                "0xabc",
                "--x",
                "-30",
                "--y",
                "40",
                "--width",
                "600",
                "--height",
                "400",
            ],
        ),
        ("focus_window", ["--window", "0xabc"]),
        ("close_window", []),
        ("kill_window", []),
        ("float_window", []),
        ("pin_window", ["--action", "enable"]),
        ("dpms", ["--action", "off", "--monitor", "DP-1"]),
        ("set_cursor", ["--theme", "Example", "--size", "24"]),
        ("set_options", ["--options", '{"general":{"gaps_in":6}}']),
        ("configure_monitors", ["--records", "[]"]),
        ("notify", ["--icon", "1", "--timeout-ms", "5000", "--message", "Hello"]),
    ]
    for index, (action, args) in enumerate(actions):
        yield {"name": "action-" + str(index), "argv": ["action", action, *args]}
    for name in ("windows", "active_window", "monitors"):
        yield {"name": "query-" + name, "argv": ["query", name]}
    yield {"name": "capabilities", "argv": ["capabilities"]}
    for name, args in (
        ("missing", []),
        ("bad-query", ["query", "bad"]),
        ("bad-action", ["action", "bad"]),
        ("bad-integer", ["action", "resize_window", "--width", "oops"]),
        ("bad-bool", ["action", "move_window", "--follow", "yes"]),
        ("unknown-option", ["action", "reload", "--bad"]),
    ):
        yield {"name": name, "argv": args}


def child(request):
    verify_source()
    sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
    spec = importlib.util.spec_from_file_location("current_compositor_cli", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    import io
    from contextlib import redirect_stdout, redirect_stderr
    from unittest.mock import patch

    calls = []

    def callback(name):
        def call(*args, **kwargs):
            calls.append({"method": name, "args": args, "kwargs": kwargs})
            return []

        return call

    for name in (
        "reload",
        "logout",
        "toggle_workspace_floating",
        "switch_workspace",
        "move_workspace",
        "move_window",
        "fullscreen_window",
        "resize_window",
        "focus_direction",
        "move_direction",
        "cycle_window",
        "place_window",
        "focus_window",
        "close_window",
        "kill_window",
        "float_window",
        "pin_window",
        "dpms",
        "set_cursor",
        "set_options",
        "configure_monitors",
        "notify",
        "query",
    ):
        setattr(module, name, callback(name))
    stdout, stderr = io.StringIO(), io.StringIO()
    with (
        patch.object(sys, "argv", ["compositor.py", *request["argv"]]),
        redirect_stdout(stdout),
        redirect_stderr(stderr),
    ):
        try:
            module.main()
            code = 0
        except SystemExit as error:
            code = error.code
    print(
        json.dumps(
            {
                "stdout": stdout.getvalue(),
                "stderr": stderr.getvalue(),
                "exit": code,
                "calls": calls,
            }
        )
    )
    return 0


def capture(destination):
    verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen niri config/query outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(
            prefix="niri-config-query-reference-"
        ) as folder:
            home = Path(folder)
            env = {
                "HOME": folder,
                "PATH": "",
                "NACRE_COMPOSITOR": "hyprland",
                "PYTHONDONTWRITEBYTECODE": "1",
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
            }
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--child",
                    json.dumps(request),
                ],
                env=env,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=10,
                check=False,
            )
            if result.returncode or result.stderr or list(home.rglob("*")):
                raise ValueError("Unexpected action capture effect: " + request["name"])
            cases.append(
                {
                    "request": request,
                    "observed": json.loads(result.stdout),
                    "stderr": "",
                    "files": {},
                }
            )
    verify_source()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as output:
        output.write(
            json.dumps(
                {
                    "source_sha256": SOURCE_SHA,
                    "cases": cases,
                },
                indent=2,
            )
            + "\n"
        )
    return len(cases)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DESTINATION)
    parser.add_argument("--child")
    args = parser.parse_args()
    if args.child:
        return child(json.loads(args.child))
    print("Frozen compositor CLI cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
