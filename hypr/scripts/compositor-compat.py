#!/usr/bin/env python3
"""Compatibility vocabulary for installed desktop scripts, using the common facade."""

import importlib
import json
from pathlib import Path
import sys


def compositor():
    for directory in (
        Path(__file__).resolve().parents[2] / "nacre/shell-tools",
        Path.home() / ".local/share/nacre/shell/tools",
    ):
        if (directory / "nacre_native_compositor.py").is_file():
            if str(directory) not in sys.path:
                sys.path.insert(0, str(directory))
            return importlib.import_module("nacre_native_compositor")
    raise RuntimeError("The Nacre compositor service is not installed")


def boolean(value):
    if value not in ("true", "false"):
        raise ValueError("Expected true or false")
    return value == "true"


def main(argv=None, service=None):
    args = list(sys.argv[1:] if argv is None else argv)
    if not args:
        raise ValueError("A desktop command is required")
    command, *args = args
    service = compositor() if service is None else service
    if command == "valid-window-id":
        return 0 if service.valid_window_id(args[0]) else 1
    if command == "window-job-token":
        print(service.window_job_token(args[0]))
        return 0
    if command == "query":
        print(json.dumps(service.query(args[0], timeout=5)))
    elif command == "focus-workspace":
        service.switch_workspace(
            args[0], current_monitor=boolean(args[1]) if len(args) > 1 else False
        )
    elif command == "focus-window":
        service.focus_window(args[0])
    elif command == "move-window":
        service.move_window(
            args[1],
            workspace=args[0],
            follow=boolean(args[2]) if len(args) > 2 else False,
        )
    elif command == "move-workspace":
        service.move_workspace(args[0], args[1])
    elif command == "close-window":
        service.close_window(args[0])
    elif command == "kill-window":
        service.kill_window(args[0])
    elif command == "fullscreen":
        service.fullscreen_window(
            args[1] if len(args) > 1 else "activewindow",
            mode=args[0] if args else "fullscreen",
        )
    elif command == "resize-window":
        service.resize_window(
            args[2] if len(args) > 2 else "activewindow",
            int(args[0]),
            int(args[1]),
            relative=False,
        )
    elif command == "dpms":
        service.dpms(args[0], monitor=args[1] if len(args) > 1 else None)
    elif command == "monitor":
        record = {"output": args[0], "disabled": args[1] == "disable"}
        if not record["disabled"]:
            record.update(
                mode=args[1],
                position=args[2] if len(args) > 2 else "auto",
                scale=float(args[3]) if len(args) > 3 else 1,
            )
            if len(args) > 4 and args[4]:
                record["mirror"] = args[4]
        service.configure_monitors([record])
    elif command == "animations":
        service.set_animations(boolean(args[0]))
    elif command == "performance":
        service.set_performance(boolean(args[0]))
    elif command == "toggle-all-float":
        service.toggle_workspace_floating()
    elif command == "exit":
        service.logout(graceful=True)
    else:
        raise ValueError("Unknown desktop command: " + command)


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (IndexError, ValueError, RuntimeError, OSError) as error:
        print(str(error), file=sys.stderr)
        raise SystemExit(2) from error
