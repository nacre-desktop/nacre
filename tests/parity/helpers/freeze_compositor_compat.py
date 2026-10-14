#!/usr/bin/env python3
"""Freeze current compositor compatibility wrapper dispatch before porting."""

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
DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/compositor-compat-reference.json"
)


SOURCE = ROOT / "hypr/scripts/compositor-compat.py"
SOURCE_SHA = "a0e30cb3ea620b5df7f369ab1bad5d977e90fcf8caec1326050b297a21b9dd4a"


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError(
            "Compositor compatibility reference source is absent or changed"
        )


def requests():
    for args in (
        [],
        ["valid-window-id", "0xabc"],
        ["valid-window-id", "bad"],
        ["window-job-token", "0xabc"],
        ["query", "windows"],
        ["focus-workspace", "2"],
        ["focus-workspace", "2", "true"],
        ["focus-workspace", "2", "bad"],
        ["focus-window", "0xabc"],
        ["move-window", "3", "0xabc"],
        ["move-window", "3", "0xabc", "true"],
        ["move-workspace", "2", "DP-1"],
        ["close-window", "0xabc"],
        ["kill-window", "0xabc"],
        ["fullscreen"],
        ["fullscreen", "maximized", "0xabc"],
        ["resize-window", "600", "400"],
        ["resize-window", "600", "400", "0xabc"],
        ["dpms", "off"],
        ["dpms", "off", "DP-1"],
        ["monitor", "DP-1", "disable"],
        ["monitor", "DP-1", "1920x1080@60", "auto", "1.25", "DP-2"],
        ["animations", "true"],
        ["performance", "false"],
        ["toggle-all-float"],
        ["exit"],
        ["unknown"],
    ):
        yield {"name": "case-" + str(args), "argv": args}


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_compat", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    import io
    from contextlib import redirect_stdout, redirect_stderr

    calls = []

    class Service:
        def __getattr__(self, name):
            def call(*args, **kwargs):
                calls.append({"method": name, "args": args, "kwargs": kwargs})
                if name == "valid_window_id":
                    return args[0] == "0xabc"
                if name == "window_job_token":
                    return "abc"
                if name == "query":
                    return []

            return call

    stdout, stderr = io.StringIO(), io.StringIO()
    with redirect_stdout(stdout), redirect_stderr(stderr):
        try:
            code = module.main(request["argv"], Service()) or 0
        except (IndexError, ValueError, RuntimeError, OSError) as error:
            print(str(error), file=sys.stderr)
            code = 2
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
    print("Frozen compositor compatibility cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
