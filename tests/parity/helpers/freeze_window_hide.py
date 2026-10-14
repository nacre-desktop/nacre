#!/usr/bin/env python3
"""Freeze CURRENT Python desktop writes, validation order and rollback files."""

import argparse
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
from unittest.mock import patch

import freeze_niri_state as reference

DESTINATION = (
    reference.ROOT / "crates/nacre-helper/tests/fixtures/window-hide-reference.json"
)


SOURCE = reference.ROOT / "hypr/scripts/window-hide.py"
SOURCE_SHA = "a5e7e8d5afb9a5e5b476dbbde7066c799659ad2adcb026f8999a60cd192b48b3"


def verify_source():
    import hashlib

    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Window-hide reference source is absent or changed")


def requests():
    base = {"address": "0xabc", "workspace": {"id": 2, "name": "2"}}
    hidden = {**base, "workspace": {"id": -99, "name": "special:hidden"}}
    for name, action, clients, records, changes in (
        ("hide", "hide", [base], {}, {}),
        ("explicit-hide", "hide", [base], {}, {"window": "0xabc"}),
        ("no-active", "hide", [], {}, {}),
        ("already-hidden", "hide", [hidden], {}, {}),
        ("dispatch-failure", "hide", [base], {}, {"failure": True}),
        ("restore-last", "restore-last", [hidden], {"0xabc": "2\n"}, {}),
        ("restore-current", "restore-current", [hidden], {"0xabc": "2\n"}, {}),
        ("wrong-current", "restore-current", [hidden], {"0xabc": "3\n"}, {}),
        ("stale", "restore-last", [], {"0xabc": "2\n"}, {}),
        ("invalid-record", "restore-last", [hidden], {"0xabc": "invalid\n"}, {}),
        ("no-hidden", "restore-last", [], {}, {}),
        ("invalid-window", "hide", [base], {}, {"window": "bad"}),
    ):
        yield {
            "name": name,
            "action": action,
            "clients": clients,
            "records": records,
            **changes,
        }


def files(home):
    return {
        str(path.relative_to(home)): {
            "hex": path.read_bytes().hex(),
            "mode": oct(path.stat().st_mode & 0o777),
        }
        for path in sorted(home.rglob("*"))
        if path.is_file()
    }


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_window_hide", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    home = Path.home()
    folder = home / ".cache/hypr-minimize"
    calls = []
    queries = []
    notifications = []
    if request["records"]:
        folder.mkdir(parents=True)
        for name, text in request["records"].items():
            (folder / name).write_text(text)

    class Service:
        def valid_window_id(self, value):
            return (
                isinstance(value, str)
                and value.startswith("0x")
                and bool(value[2:])
                and all(c in "0123456789abcdefABCDEF" for c in value[2:])
            )

    def query(name):
        queries.append(name)
        if name == "active_window":
            return request["clients"][0] if request["clients"] else {}
        if name == "active_workspace":
            return {"id": 2}
        return request["clients"] if name == "windows" else []

    def dispatch(method, window, **values):
        calls.append({"method": method, "window": window, "values": values})
        if request.get("failure"):
            raise RuntimeError("fixture dispatch failed")

    with (
        patch.object(module, "compositor", return_value=Service()),
        patch.object(module, "query", side_effect=query),
        patch.object(module, "dispatch", side_effect=dispatch),
        patch.object(module, "notify", side_effect=notifications.append),
    ):
        try:
            module.action(
                request["action"], folder=folder, window=request.get("window")
            )
            error = None
        except (ValueError, RuntimeError, OSError) as failure:
            error = {"error": str(failure), "type": type(failure).__name__}
    print(
        json.dumps(
            {
                "error": error,
                "calls": calls,
                "queries": queries,
                "notifications": notifications,
                "files": files(home),
            }
        )
    )


def capture(destination):
    verify_source()
    if destination.exists():
        raise ValueError(
            "Refusing to overwrite frozen Niri desktop transaction outputs"
        )
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="niri-desktop-reference-") as folder:
            env = {
                "HOME": folder,
                "PATH": "",
                "PYTHONDONTWRITEBYTECODE": "1",
                "XDG_CONFIG_HOME": folder + "/.config",
                "XDG_STATE_HOME": folder + "/.local/state",
                "XDG_CACHE_HOME": folder + "/.cache",
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
                timeout=5,
                check=False,
            )
            if result.returncode or result.stderr:
                raise ValueError(
                    "Unexpected desktop transaction capture failure: "
                    + request["name"]
                    + result.stderr[-500:]
                )
            observed = json.loads(result.stdout)
            if observed["files"] != files(Path(folder)):
                raise ValueError("Desktop capture omitted a file effect")
            cases.append({"request": request, "observed": observed, "stderr": ""})
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
        child(json.loads(args.child))
        return 0
    print("Frozen window-hide cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
