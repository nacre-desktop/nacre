#!/usr/bin/env python3
"""Hide windows without closing them; restore from private compatible records."""

import argparse
import fcntl
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

HIDDEN = "special:hidden"


def cache_directory():
    return (
        Path(os.environ.get("XDG_CACHE_HOME", str(Path.home() / ".cache")))
        / "hypr-minimize"
    )


def query(name):
    result = subprocess.run(
        ["hyprctl", name, "-j"], capture_output=True, text=True, check=True, timeout=5
    )
    if len(result.stdout) > 4 * 1024 * 1024:
        raise ValueError("Compositor response exceeds the limit")
    return json.loads(result.stdout)


def address(value):
    return isinstance(value, str) and re.fullmatch(r"0x[0-9a-fA-F]+", value) is not None


def dispatch(method, window, **options):
    if not address(window):
        raise ValueError("Invalid window address")
    values = {"window": "address:" + window, **options}
    fields = ",".join(
        key + "=" + json.dumps(value, ensure_ascii=False)
        for key, value in values.items()
    )
    expression = f"hl.dispatch(hl.dsp.{method}({{{fields}}}))"
    result = subprocess.run(
        ["hyprctl", "eval", expression],
        capture_output=True,
        text=True,
        check=True,
        timeout=5,
    )
    if result.stdout.strip() != "ok":
        raise RuntimeError("The compositor did not accept the window action")


def destination(workspace):
    if workspace > 0:
        return str(workspace)
    for item in query("workspaces"):
        if item.get("id") == workspace and isinstance(item.get("name"), str):
            name = item["name"]
            if name and not any(ord(char) < 32 for char in name):
                return name if name.startswith("special:") else "name:" + name
    raise ValueError("The original workspace is unavailable")


def notify(summary):
    try:
        subprocess.run(
            ["notify-send", "-t", "2000", summary],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            timeout=2,
        )
    except (OSError, subprocess.SubprocessError):
        pass


def write_record(path, workspace):
    descriptor, temporary = tempfile.mkstemp(dir=path.parent)
    try:
        with os.fdopen(descriptor, "w") as stream:
            stream.write(str(workspace) + "\n")
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


def records(folder, clients):
    live = {
        item.get("address"): item for item in clients if address(item.get("address"))
    }
    result = []
    for path in folder.iterdir():
        if not address(path.name) or path.is_symlink() or not path.is_file():
            continue
        if path.stat().st_size > 128:
            continue
        value = path.read_text().strip()
        if not re.fullmatch(r"-?\d+", value):
            continue
        client = live.get(path.name)
        if not client or client.get("workspace", {}).get("name") != HIDDEN:
            path.unlink()
            continue
        result.append((path.stat().st_mtime_ns, path, int(value)))
    return sorted(result, key=lambda row: (row[0], row[1].name), reverse=True)


def action(name, folder=None, window=None, quiet=False):
    announce = (lambda message: None) if quiet else notify
    if window is not None and not address(window):
        raise ValueError("Invalid window address")
    folder = cache_directory() if folder is None else Path(folder)
    folder.mkdir(parents=True, exist_ok=True, mode=0o700)
    with (folder / ".lock").open("a") as guard:
        os.chmod(guard.name, 0o600)
        fcntl.flock(guard, fcntl.LOCK_EX)
        if name == "hide":
            client = (
                next(
                    (row for row in query("clients") if row.get("address") == window),
                    {},
                )
                if window
                else query("activewindow")
            )
            ident = client.get("address")
            workspace = client.get("workspace", {})
            number = workspace.get("id")
            if not address(ident) or type(number) is not int:
                announce("No active window to hide")
                return
            if workspace.get("name") == HIDDEN:
                return
            path = folder / ident
            if path.is_symlink():
                raise ValueError("The window record is a symlink")
            write_record(path, number)
            try:
                dispatch("window.move", ident, workspace=HIDDEN, follow=False)
            except Exception:  # noqa: BLE001 — Rollback must run for any failure before the original exception is re-raised.
                path.unlink(missing_ok=True)
                raise
            announce("Window hidden")
            return
        candidates = records(folder, query("clients"))
        if window:
            candidates = [row for row in candidates if row[1].name == window]
        if name == "restore-current":
            current = query("activeworkspace").get("id")
            candidates = [row for row in candidates if row[2] == current]
        else:
            candidates = candidates[:1]
        for _, path, workspace in candidates:
            dispatch(
                "window.move", path.name, workspace=destination(workspace), follow=False
            )
            path.unlink()
            if name == "restore-last":
                dispatch("focus", path.name)
        if candidates:
            announce(
                "Window restored"
                if name == "restore-last"
                else "Window restored to current workspace"
            )
        else:
            announce("No hidden windows to restore")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "action",
        choices=("hide", "restore-last", "restore-current"),
        nargs="?",
        default="hide",
    )
    parser.add_argument("--window", help="Target only this compositor window address")
    parser.add_argument(
        "--quiet", action="store_true", help="Suppress transient action feedback"
    )
    args = parser.parse_args()
    action(args.action, window=args.window, quiet=args.quiet)


if __name__ == "__main__":
    main()
