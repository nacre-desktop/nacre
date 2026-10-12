#!/usr/bin/env python3
"""Compositor-neutral Python entrypoint. No compositor processes start on import.

Window IDs are opaque strings. Workspace references may be stable IDs, names, or
{output, index} records; only the active backend interprets them. Legacy fields in
Hyprland snapshots remain during the compatibility migration.
"""

import argparse
import importlib.util
import json
import os
from pathlib import Path


def select_backend(environment=None):
    """Select once at startup; imports never query a compositor session."""
    environment = os.environ if environment is None else environment
    explicit = environment.get("NACRE_COMPOSITOR", "").lower()
    if explicit:
        if explicit not in ("hyprland", "niri"):
            raise ValueError("Unknown NACRE_COMPOSITOR backend")
        return explicit
    desktops = environment.get("XDG_CURRENT_DESKTOP", "").lower().split(":")
    if "niri" in desktops:
        return "niri"
    if environment.get("HYPRLAND_INSTANCE_SIGNATURE"):
        return "hyprland"
    return "niri" if environment.get("NIRI_SOCKET") else "hyprland"


_BACKEND_SPEC = importlib.util.spec_from_file_location(
    "nacre_compositor_backend",
    Path(__file__).with_name("compositor_backends") / (select_backend() + ".py"),
)
_backend_module = importlib.util.module_from_spec(_BACKEND_SPEC)
_BACKEND_SPEC.loader.exec_module(_backend_module)
backend = _backend_module.Backend()
backend_name = backend.name
backend_display_name = backend.display_name
capabilities = backend.capabilities
_MISSING = object()


def query(name, *, timeout=10, default=_MISSING, max_bytes=None):
    if name not in backend.query_names:
        raise ValueError("Unknown compositor query")
    try:
        return backend.query(name, timeout=timeout, max_bytes=max_bytes)
    except (OSError, ValueError, RuntimeError):
        if default is _MISSING:
            raise
        return default
    except _backend_module.subprocess.SubprocessError:
        if default is _MISSING:
            raise
        return default


def config_errors(*, timeout=10, default=_MISSING):
    try:
        return backend.config_errors(timeout=timeout)
    except (OSError, _backend_module.subprocess.SubprocessError):
        if default is _MISSING:
            raise
        return default


def desktop_option_keys():
    return tuple(_backend_module.DESKTOP_OPTIONS)


# The facade exports semantic operations; implementations and native encodings
# belong exclusively to the selected backend.
for _name in (
    "session_id",
    "environment_updates",
    "legacy_power_binding",
    "set_animations",
    "set_performance",
    "query_result",
    "valid_window_id",
    "window_job_token",
    "version_command",
    "required_commands",
    "diagnostic_commands",
    "binding_source_files",
    "capture_bindings",
    "events",
    "reload",
    "switch_workspace",
    "focus_window",
    "move_window",
    "move_workspace",
    "close_window",
    "kill_window",
    "float_window",
    "pin_window",
    "fullscreen_window",
    "fullscreen_state",
    "resize_window",
    "place_window",
    "dpms",
    "logout",
    "set_cursor",
    "notify",
    "set_options",
    "render_options",
    "render_keyboard",
    "config_path",
    "palette_template_path",
    "render_desktop_options",
    "set_desktop_options",
    "get_desktop_option",
    "render_monitors",
    "configure_monitors",
    "apply_config",
    "ensure_desktop_loader",
    "night_light_command",
    "night_light_configured_elsewhere",
    "night_light_external_socket",
    "set_night_light_temperature",
    "event_socket",
    "parse_event",
    "toggle_workspace_floating",
):
    globals()[_name] = getattr(backend, _name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("events", help="Stream canonical state and compositor events")
    commands.add_parser("capabilities", help="Describe the selected backend")
    queries = commands.add_parser("query")
    queries.add_argument("name", choices=_backend_module.QUERIES)
    actions = commands.add_parser("action")
    actions.add_argument(
        "name",
        choices=(
            "reload",
            "logout",
            "switch_workspace",
            "focus_window",
            "close_window",
            "kill_window",
            "move_window",
            "place_window",
            "move_workspace",
            "fullscreen_window",
            "resize_window",
            "float_window",
            "pin_window",
            "dpms",
            "set_cursor",
            "set_options",
            "configure_monitors",
            "toggle_workspace_floating",
            "notify",
        ),
    )
    for name in (
        "window",
        "workspace",
        "monitor",
        "mode",
        "action",
        "theme",
        "message",
        "color",
        "records",
        "options",
    ):
        actions.add_argument("--" + name)
    for name in ("width", "height", "x", "y", "size", "icon", "timeout-ms"):
        actions.add_argument("--" + name, type=int)
    actions.add_argument("--follow", choices=("true", "false"), default="false")
    actions.add_argument("--current-monitor", choices=("true", "false"), default="true")
    args = parser.parse_args()
    if args.command == "action" and args.workspace and args.workspace.startswith("{"):
        args.workspace = json.loads(args.workspace)
    if args.command == "events":
        for value in backend.events():
            print(json.dumps(value, allow_nan=False), flush=True)
        return
    if args.command == "capabilities":
        print(json.dumps({"backend": backend_name, "capabilities": capabilities}))
        return
    if args.command == "query":
        print(json.dumps(query(args.name)))
        return
    method = globals()[args.name]
    if args.name in ("reload", "logout", "toggle_workspace_floating"):
        method()
        return
    if args.name == "switch_workspace":
        method(args.workspace, current_monitor=args.current_monitor == "true")
    elif args.name == "move_workspace":
        method(args.workspace, args.monitor)
    elif args.name == "move_window":
        method(
            args.window or "",
            workspace=args.workspace,
            follow=args.follow == "true",
            x=args.x,
            y=args.y,
        )
    elif args.name == "fullscreen_window":
        method(args.window or "", mode=args.mode or "fullscreen")
    elif args.name == "resize_window":
        method(args.window or "", args.width, args.height)
    elif args.name == "place_window":
        method(
            args.window or "", x=args.x, y=args.y, width=args.width, height=args.height
        )
    elif args.name in ("focus_window", "close_window", "kill_window"):
        method(args.window or "")
    elif args.name in ("float_window", "pin_window"):
        method(args.window or "", action=args.action or "toggle")
    elif args.name == "dpms":
        method(args.action, args.monitor)
    elif args.name == "set_cursor":
        method(args.theme, args.size)
    elif args.name == "set_options":
        method(json.loads(args.options))
    elif args.name == "configure_monitors":
        method(json.loads(args.records))
    elif args.name == "notify":
        method(args.icon, args.timeout_ms, args.color or "0", args.message)


if __name__ == "__main__":
    main()
