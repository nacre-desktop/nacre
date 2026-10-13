#!/usr/bin/env python3
"""Open a Nacre menu route, or ask its shared window for a selection/text value."""

import argparse
from nacre_i18n import tr
import fcntl
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import uuid

ROOT = Path.home() / ".cache/nacre/menu/requests"
SESSION = re.compile("[0-9a-f]{32}\\Z")


def atomic(path, data):
    temporary = path.with_suffix(".next")
    with temporary.open("w") as stream:
        json.dump(data, stream)
    temporary.chmod(0o600)
    temporary.replace(path)


def session_path(ident):
    if not isinstance(ident, str) or not SESSION.fullmatch(ident):
        raise ValueError(
            tr("helper.menu-cli.invalid-selector-session", "Invalid selector session")
        )
    path = ROOT / ident
    if (
        path.is_symlink()
        or not path.is_dir()
        or path.stat().st_uid != os.getuid()
        or path.stat().st_mode & 0o077
    ):
        raise ValueError(
            tr(
                "helper.menu-cli.selector-session-is-unavailable",
                "Selector session is unavailable",
            )
        )
    return path


def load_request(ident):
    path = session_path(ident) / "request.json"
    if (
        path.is_symlink()
        or path.stat().st_size > 1048576
        or path.stat().st_uid != os.getuid()
        or path.stat().st_mode & 0o077
    ):
        raise ValueError(
            tr("helper.menu-cli.invalid-selector-request", "Invalid selector request")
        )
    data = json.loads(path.read_text())
    if (
        not isinstance(data, dict)
        or data.get("version") != 1
        or data.get("id") != ident
        or data.get("kind") not in ("pick", "input")
        or not isinstance(data.get("expires"), (int, float))
        or not math.isfinite(data["expires"])
        or data["expires"] <= time.time()
    ):
        raise ValueError(
            tr(
                "helper.menu-cli.selector-request-expired-or-has-an-invalid-format",
                "Selector request expired or has an invalid format",
            )
        )
    if not isinstance(data.get("title", ""), str) or len(data.get("title", "")) > 200:
        raise ValueError(
            tr("helper.menu-cli.invalid-selector-title", "Invalid selector title")
        )
    if (
        not isinstance(data.get("initial", ""), str)
        or len(data.get("initial", "")) > 16384
    ):
        raise ValueError(
            tr("helper.menu-cli.invalid-initial-input", "Invalid initial input")
        )
    if data["kind"] == "pick":
        data["choices"] = choices(json.dumps(data.get("choices", [])), True)
    return data


def respond(ident, value):
    request = load_request(ident)
    if not isinstance(value, dict) or type(value.get("cancelled")) is not bool:
        raise ValueError(
            tr("helper.menu-cli.invalid-selector-response", "Invalid selector response")
        )
    answer = value.get("value", "")
    if not isinstance(answer, str) or len(answer) > 16384 or "\0" in answer:
        raise ValueError(
            tr("helper.menu-cli.invalid-selector-value", "Invalid selector value")
        )
    if (
        not value["cancelled"]
        and request["kind"] == "pick"
        and not any(
            row.get("value") == answer and not row.get("disabled")
            for row in request.get("choices", [])
        )
    ):
        raise ValueError(
            tr(
                "helper.menu-cli.that-choice-is-not-part-of-this-selector",
                "That choice is not part of this selector",
            )
        )
    atomic(
        session_path(ident) / "response.json",
        dict(
            cancelled=value["cancelled"],
            value=answer,
            error=str(value.get("error", ""))[:200],
        ),
    )


def choices(text, json_input=False):
    if len(text) > 1048576:
        raise ValueError(
            tr(
                "helper.menu-cli.selection-list-is-too-large",
                "Selection list is too large",
            )
        )
    raw = (
        json.loads(text)
        if json_input
        else [
            dict(
                label=parts[0],
                value=parts[1] if len(parts) > 1 else parts[0],
                description=parts[2] if len(parts) > 2 else "",
            )
            for line in text.splitlines()
            if (parts := line.split("\t")) and parts[0]
        ]
    )
    if not isinstance(raw, list) or len(raw) > 2000:
        raise ValueError(
            tr(
                "helper.menu-cli.use-a-list-of-at-most-2000-choices",
                "Use a list of at most 2000 choices",
            )
        )
    result = []
    for row in raw:
        if (
            not isinstance(row, dict)
            or not isinstance(row.get("label"), str)
            or not isinstance(row.get("value", row.get("label")), str)
        ):
            raise ValueError(
                tr(
                    "helper.menu-cli.each-choice-needs-a-label-and-string-value",
                    "Each choice needs a label and string value",
                )
            )
        item = dict(
            label=row["label"],
            value=row.get("value", row["label"]),
            description=str(row.get("description", "")),
            icon=str(row.get("icon", "chevron_right")),
            disabled=row.get("disabled") is True,
        )
        if (
            not item["label"]
            or len(item["label"]) > 200
            or len(item["value"]) > 16384
            or any("\0" in str(v) for v in item.values())
        ):
            raise ValueError(
                tr(
                    "helper.menu-cli.invalid-selection-label-or-value",
                    "Invalid selection label or value",
                )
            )
        result.append(item)
    return result


def ipc(target, *args):
    return subprocess.run(
        [
            str(Path.home() / ".local/share/nacre/shell/bin/qs"),
            "-c",
            "nacre",
            "ipc",
            "call",
            target,
            *args,
        ],
        check=True,
        capture_output=True,
        text=True,
        timeout=5,
    )


def request(kind, title, rows=None, initial="", timeout=300):
    ROOT.mkdir(parents=True, exist_ok=True, mode=0o700)
    ROOT.chmod(0o700)
    lock = ROOT / "selector.lock"
    with lock.open("w") as guard:
        lock.chmod(0o600)
        try:
            fcntl.flock(guard, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            raise ValueError(
                tr(
                    "helper.menu-cli.another-selector-is-already-open",
                    "Another selector is already open",
                )
            ) from None
        ident = uuid.uuid4().hex
        path = ROOT / ident
        path.mkdir(mode=0o700)
        data = dict(
            version=1,
            id=ident,
            kind=kind,
            title=title[:200],
            choices=rows or [],
            initial=initial[:16384],
            expires=time.time() + timeout,
        )
        atomic(path / "request.json", data)
        try:
            ipc("menuPrompt", "open", ident)
            end = time.monotonic() + timeout
            while time.monotonic() < end:
                response = path / "response.json"
                if response.is_file():
                    result = json.loads(response.read_text())
                    if result.get("cancelled"):
                        if result.get("error"):
                            print(result["error"], file=sys.stderr)
                        return 1
                    print(result.get("value", ""))
                    return 0
                time.sleep(0.05)
            raise ValueError(
                tr("helper.menu-cli.selector-timed-out", "Selector timed out")
            )
        finally:
            try:
                ipc("menuPrompt", "cancel", ident)
            except subprocess.SubprocessError:
                pass
            for name in (
                "request.json",
                "request.next",
                "response.json",
                "response.next",
            ):
                (path / name).unlink(missing_ok=True)
            path.rmdir()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("route", nargs="?", default="home")
    parser.add_argument("value", nargs="?", default="")
    parser.add_argument("--title", default="Choose an item")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--initial", default="")
    parser.add_argument("--timeout", type=int, default=300)
    args = parser.parse_args()
    try:
        if args.route == "respond":
            respond(args.value, json.loads(sys.stdin.readline(65537)))
            return 0
        if args.route == "read-request":
            print(json.dumps(load_request(args.value)))
            return 0
        if args.route == "webapp-launch":
            spec = importlib.util.spec_from_file_location(
                "menu_backend", Path(__file__).with_name("menu-backend.py")
            )
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            module.launch_webapp(args.value)
            return 0
        if args.route in ("pick", "input"):
            if not 1 <= args.timeout <= 3600:
                raise ValueError(
                    tr(
                        "helper.menu-cli.timeout-must-be-between-1-and-3600-seconds",
                        "Timeout must be between 1 and 3600 seconds",
                    )
                )
            return request(
                args.route,
                args.title,
                choices(sys.stdin.read(1048577), args.json)
                if args.route == "pick"
                else None,
                args.initial,
                args.timeout,
            )
        ipc("nacre", "menu", args.route, args.value)
        return 0
    except KeyboardInterrupt:
        return 130
    except (OSError, ValueError, subprocess.SubprocessError) as error:
        print(str(error), file=sys.stderr)
        return 1


if __name__ == "__main__":
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(1))
    raise SystemExit(main())
