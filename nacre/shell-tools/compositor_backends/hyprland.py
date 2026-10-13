"""Hyprland transport and configuration translation; never imported by UI consumers."""

import json
import math
import os
import re
from pathlib import Path
import socket
import shutil
import subprocess

LEGACY_POWER_BINDING = '-- Run after release/input processing so this key does not immediately wake DPMS.\nhl.bind("XF86PowerOff", hl.dsp.exec_cmd("sleep 0.2; ~/.config/hypr/scripts/hyprctl-lua.sh dpms disable"), {\n    release = true,\n    locked = true,\n})\n'

QUERIES = {
    "monitors": ["monitors", "-j"],
    "all_monitors": ["monitors", "all", "-j"],
    "workspaces": ["workspaces", "-j"],
    "windows": ["clients", "-j"],
    "active_window": ["activewindow", "-j"],
    "active_workspace": ["activeworkspace", "-j"],
    "cursor": ["cursorpos", "-j"],
    "instances": ["instances", "-j"],
    "bindings": ["binds", "-j"],
}
DESKTOP_OPTIONS = {
    "animations": "animations.enabled",
    "blur": "decoration.blur.enabled",
    "shadow": "decoration.shadow.enabled",
    "followMouse": "input.follow_mouse",
    "naturalScroll": "input.touchpad.natural_scroll",
    "gapsIn": "general.gaps_in",
    "gapsOut": "general.gaps_out",
    "borderSize": "general.border_size",
    "rounding": "decoration.rounding",
}


def lua_value(value):
    if value is None:
        return "nil"
    if isinstance(value, str):
        return (
            '"'
            + "".join(
                "\\" + char
                if char in '\\"'
                else f"\\{ord(char):03d}"
                if ord(char) < 32
                else char
                for char in value
            )
            + '"'
        )
    if value is True:
        return "true"
    if value is False:
        return "false"
    if isinstance(value, dict):
        return (
            "{"
            + ",".join(
                "[" + lua_value(key) + "]=" + lua_value(item)
                for key, item in value.items()
            )
            + "}"
        )
    if isinstance(value, (list, tuple)):
        return "{" + ",".join(lua_value(item) for item in value) + "}"
    if isinstance(value, float) and not math.isfinite(value):
        raise ValueError("Non-finite compositor value")
    return json.dumps(value, ensure_ascii=True, allow_nan=False)


def lua_table(values):
    if any(not isinstance(key, str) for key in values):
        raise ValueError("Invalid compositor field")

    def field(key):
        return (
            key
            if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", key)
            else "[" + lua_value(key) + "]"
        )

    return (
        "{"
        + ",".join(field(key) + "=" + lua_value(value) for key, value in values.items())
        + "}"
    )


def window_selector(value):
    text = str(value or "")
    if len(text) > 1024 or any(ord(char) < 32 for char in text):
        raise ValueError("Invalid window ID")
    if text.startswith("0x"):
        text = "address:" + text
    if text and not re.fullmatch(
        r"address:0x[0-9a-fA-F]+|class:.+|title:.+|pid:[0-9]+|active", text
    ):
        raise ValueError("Invalid Hyprland window ID")
    return text


def workspace_reference(value):
    if isinstance(value, dict):
        if "output" in value and "id" not in value and "name" not in value:
            raise ValueError(
                "Output-relative workspace indices are not supported by this backend"
            )
        value = value.get("id", value.get("name", value.get("index")))
    if isinstance(value, bool) or value is None:
        raise ValueError("Invalid workspace reference")
    if not isinstance(value, (str, int)) or not str(value) or len(str(value)) > 200:
        raise ValueError("Invalid workspace reference")
    if any(ord(char) < 32 for char in str(value)):
        raise ValueError("Invalid workspace reference")
    return value


def normalize_window(row):
    if not row:
        return row
    return {
        **row,
        "windowId": row.get("address", ""),
        "id": row.get("address", ""),
        "appId": row.get("class", ""),
        "workspaceId": (row.get("workspace") or {}).get("id"),
        "hidden": bool(row.get("hidden", False)),
        "pinned": bool(row.get("pinned", False)),
        "fullscreenMode": row.get("fullscreen", 0),
        "outputId": row.get("monitor"),
        "geometry": {
            "x": row.get("at", [0, 0])[0],
            "y": row.get("at", [0, 0])[1],
            "width": row.get("size", [0, 0])[0],
            "height": row.get("size", [0, 0])[1],
        },
    }


class Backend:
    name = "hyprland"
    query_names = tuple(QUERIES)
    capabilities = {
        "stableWorkspaceIndices": True,
        "globalWorkspaces": True,
        "perOutputWorkspaces": False,
        "scrollingLayout": False,
        "floatingPlacement": True,
        "pinWindow": True,
        "windowCapture": True,
        "windowHide": True,
        "windowTrash": True,
        "scratchpad": True,
        "windowGroups": True,
        "windowForceKill": True,
        "pseudoTiling": True,
        "layoutSplit": True,
        "keymaps": True,
        "nonConsumingShortcuts": True,
        "bindingIntrospection": True,
        "monitorMirroring": True,
        "fullscreenState": True,
    }

    def _run(self, args, *, timeout=10, check=True):
        return subprocess.run(
            ["hyprctl", *args],
            capture_output=True,
            text=True,
            timeout=timeout,
            check=check,
        )

    display_name = "Hyprland"

    def required_commands(self):
        return ("Hyprland", "hyprctl")

    def diagnostic_commands(self):
        return {
            "runningVersion": ["hyprctl", "version"],
            "installedVersion": self.version_command(),
            "allMonitors": ["hyprctl", "-j", "monitors", "all"],
            "configErrors": ["hyprctl", "configerrors"],
        }

    def version_command(self):
        return ["Hyprland", "--version"]

    def valid_window_id(self, value):
        return (
            isinstance(value, str)
            and re.fullmatch(r"0x[0-9a-fA-F]+", value) is not None
        )

    def window_job_token(self, value):
        if not self.valid_window_id(value):
            raise ValueError("Invalid window ID")
        return value[2:]

    def binding_source_files(self, home):
        paths = [
            *sorted((Path(home) / ".config/hypr/conf").glob("*.lua")),
            Path(home) / ".config/nacre/shortcuts.lua",
        ]
        return [
            path
            for path in paths
            if path.exists() and re.search(r"\bhl\.bind\s*\(", path.read_text())
        ]

    def capture_bindings(self, path, kind="bindings"):
        if kind not in ("bindings", "rules", "declarations"):
            raise ValueError("Unsupported declaration kind")
        lua = shutil.which("lua")
        if not lua:
            raise RuntimeError("Lua required for declarative binding checks")
        path = Path(path)
        result = subprocess.run(
            [
                lua,
                str(Path(__file__).with_name("hyprland") / "bindings.lua"),
                str(path),
                kind,
            ],
            capture_output=True,
            text=True,
            timeout=10,
        )
        if result.returncode:
            raise RuntimeError(
                "Cannot capture bindings in " + path.name + ": " + result.stderr
            )
        records = json.loads(result.stdout)
        return records if isinstance(records, list) else []

    def session_id(self):
        return os.environ.get("HYPRLAND_INSTANCE_SIGNATURE", "")

    def environment_updates(self, environment=None):
        environment = os.environ if environment is None else environment
        updates = {}
        result = self.query_result("instances", timeout=None, check=False)
        if result.returncode == 0:
            instances = json.loads(result.stdout)
            if len(instances) == 1:
                updates["HYPRLAND_INSTANCE_SIGNATURE"] = instances[0]["instance"]
        if "XDG_CURRENT_DESKTOP" not in environment:
            updates["XDG_CURRENT_DESKTOP"] = "Hyprland"
        return updates

    def legacy_power_binding(self):
        # Historical bytes are used only to recognize a safely removable block.
        return LEGACY_POWER_BINDING

    def set_animations(self, enabled, *, timeout=10):
        return self.set_options({"animations": {"enabled": enabled}}, timeout=timeout)

    def set_performance(self, enabled, *, timeout=10):
        if not enabled:
            return self.reload(timeout=timeout)
        return self.set_options(
            {
                "animations": {"enabled": False},
                "decoration": {
                    "shadow": {"enabled": False},
                    "blur": {"enabled": False},
                    "active_opacity": 1,
                    "inactive_opacity": 1,
                    "fullscreen_opacity": 1,
                    "rounding": 0,
                },
                "general": {"gaps_in": 0, "gaps_out": 0, "border_size": 1},
            },
            timeout=timeout,
        )

    def query_result(self, name, *, timeout=10, check=True):
        if name not in QUERIES:
            raise ValueError("Unknown compositor query")
        return self._run(QUERIES[name], timeout=timeout, check=check)

    def query(self, name, *, timeout=10, max_bytes=None):
        text = self.query_result(name, timeout=timeout).stdout
        if max_bytes is not None and len(text.encode()) > max_bytes:
            raise ValueError("Compositor response exceeded its size limit")
        result = json.loads(text)
        array_query = name in (
            "monitors",
            "all_monitors",
            "windows",
            "workspaces",
            "instances",
            "bindings",
        )
        if not isinstance(result, list if array_query else dict):
            raise ValueError("Malformed compositor query response")
        if array_query and any(not isinstance(row, dict) for row in result):
            raise ValueError("Malformed compositor query record")
        if name == "windows":
            return [normalize_window(row) for row in result]
        if name == "active_window":
            return normalize_window(result)
        return result

    def config_errors(self, *, timeout=10):
        return self._run(["configerrors"], timeout=timeout).stdout.strip()

    def reload(self, *, timeout=10):
        return self._run(["reload"], timeout=timeout)

    def _action(self, name, values, *, timeout=10, transport="eval"):
        expression = "hl.dispatch(hl.dsp." + name + "(" + lua_table(values) + "))"
        return self._run([transport, expression], timeout=timeout)

    def switch_workspace(self, workspace, *, current_monitor=True, timeout=10):
        return self._action(
            "focus",
            {
                "workspace": workspace_reference(workspace),
                "on_current_monitor": current_monitor,
            },
            timeout=timeout,
        )

    def focus_direction(self, direction, *, timeout=10):
        if direction not in ("left", "right", "up", "down"):
            raise ValueError("Invalid focus direction")
        return self._action("focus", {"direction": direction}, timeout=timeout)

    def cycle_window(self, direction="next", *, timeout=10):
        if direction not in ("next", "previous"):
            raise ValueError("Invalid cycle direction")
        return self._action(
            "window.cycle", {"next": direction == "next"}, timeout=timeout
        )

    def move_direction(self, direction, *, timeout=10):
        if direction not in ("left", "right", "up", "down"):
            raise ValueError("Invalid move direction")
        return self._action("window.move", {"direction": direction}, timeout=timeout)

    def focus_window(self, window="", *, timeout=10):
        return self._action(
            "focus", {"window": window_selector(window)}, timeout=timeout
        )

    def move_window(
        self,
        window="",
        *,
        workspace=None,
        follow=False,
        x=None,
        y=None,
        relative=False,
        timeout=10,
    ):
        values = {"window": window_selector(window)}
        if workspace is not None:
            values.update(workspace=workspace_reference(workspace), follow=follow)
        if x is not None or y is not None:
            values.update(x=x, y=y, relative=relative)
        return self._action("window.move", values, timeout=timeout)

    def move_workspace(self, workspace, monitor, *, timeout=10):
        return self._action(
            "workspace.move",
            {"workspace": workspace_reference(workspace), "monitor": monitor},
            timeout=timeout,
        )

    def close_window(self, window="", *, timeout=10):
        return self._action(
            "window.close", {"window": window_selector(window)}, timeout=timeout
        )

    def kill_window(self, window="", *, timeout=10):
        return self._action(
            "window.kill", {"window": window_selector(window)}, timeout=timeout
        )

    def float_window(self, window="", *, action="toggle", timeout=10):
        return self._action(
            "window.float",
            {"window": window_selector(window), "action": action},
            timeout=timeout,
        )

    def pin_window(self, window="", *, action=None, timeout=10):
        values = {"window": window_selector(window)}
        if action is not None:
            values["action"] = action
        return self._action("window.pin", values, timeout=timeout)

    def fullscreen_window(self, window="", *, mode="fullscreen", timeout=10):
        return self._action(
            "window.fullscreen",
            {"mode": mode, "window": window_selector(window)},
            timeout=timeout,
        )

    def fullscreen_state(
        self, window="", *, internal=0, client=0, action="set", timeout=10
    ):
        return self._action(
            "window.fullscreen_state",
            {
                "internal": internal,
                "client": client,
                "action": action,
                "window": window_selector(window),
            },
            timeout=timeout,
        )

    def resize_window(
        self, window="", width=None, height=None, *, relative=False, timeout=10
    ):
        return self._action(
            "window.resize",
            {
                "x": width,
                "y": height,
                "relative": relative,
                "window": window_selector(window),
            },
            timeout=timeout,
        )

    def place_window(self, window, *, x, y, width=None, height=None, timeout=10):
        self.float_window(window, action="enable", timeout=timeout)
        if width is not None and height is not None:
            self.resize_window(window, width, height, timeout=timeout)
        return self.move_window(window, x=x, y=y, relative=False, timeout=timeout)

    def dpms(self, action, monitor=None, *, timeout=10):
        values = {"action": action}
        if monitor is not None:
            values["monitor"] = monitor
        return self._action("dpms", values, timeout=timeout)

    def logout(self, *, timeout=10, graceful=False):
        if graceful and shutil.which("hyprshutdown"):
            return subprocess.run(
                ["hyprshutdown"],
                capture_output=True,
                text=True,
                check=True,
                timeout=timeout,
            )
        return self._run(["eval", "hl.dispatch(hl.dsp.exit())"], timeout=timeout)

    def set_cursor(self, theme, size, *, timeout=10):
        return self._run(["setcursor", str(theme), str(size)], timeout=timeout)

    def notify(self, icon, timeout_ms, color, message, *, timeout=10):
        return self._run(
            ["notify", str(icon), str(timeout_ms), str(color), message], timeout=timeout
        )

    def render_keyboard(self, values):
        rows = [
            "-- Your system keyboard choice. Edit here; Nacre updates preserve this file.",
            "hl.config({",
        ]
        rows.extend(
            '    ["input.kb_' + key + '"] = ' + lua_value(value) + ","
            for key, value in values.items()
        )
        rows.append("})")
        return "\n".join(rows) + "\n"

    def config_path(self, home, kind):
        if kind not in ("palette", "desktop", "monitor", "profile"):
            raise ValueError("Unknown compositor configuration")
        return Path(home) / ".config/nacre" / (kind + ".lua")

    def palette_template_path(self):
        return Path(__file__).with_name("hyprland") / "palette.lua.in"

    def render_options(self, options):
        return (
            "hl.config({"
            + ",".join(
                "[" + lua_value(key) + "]=" + lua_value(value)
                for key, value in options.items()
            )
            + "})\n"
        )

    def set_options(self, options, *, timeout=10):
        return self._run(["eval", self.render_options(options)], timeout=timeout)

    def render_desktop_options(self, preferences):
        return self.render_options(
            {
                option: int(preferences[key])
                if key == "followMouse"
                else preferences[key]
                for key, option in DESKTOP_OPTIONS.items()
                if key in preferences
            }
        )

    def set_desktop_options(self, preferences, *, timeout=10):
        return self._run(
            ["eval", self.render_desktop_options(preferences)], timeout=timeout
        )

    def get_desktop_option(self, key, *, timeout=8):
        value = json.loads(
            self._run(["getoption", DESKTOP_OPTIONS[key], "-j"], timeout=timeout).stdout
        )
        result = (
            value["bool"]
            if "bool" in value
            else value["int"]
            if "int" in value
            else int(value["css"].split()[0])
        )
        return bool(result) if key == "followMouse" else result

    def render_monitors(self, monitors):
        lines = []
        for monitor in monitors:
            if "output" in monitor:
                lines.append("hl.monitor(" + lua_table(monitor) + ")")
                continue
            mirror = monitor.get("mirrorOf") or ""
            if mirror == "none":
                mirror = ""
            lines.append(
                "hl.monitor({output="
                + lua_value(monitor["name"])
                + ",mode="
                + lua_value(
                    f"{int(monitor['width'])}x{int(monitor['height'])}@{float(monitor['refreshRate']):.3f}"
                )
                + ",scale="
                + str(float(monitor["scale"]))
                + ",position="
                + lua_value(f"{int(monitor['x'])}x{int(monitor['y'])}")
                + ",transform="
                + str(int(monitor.get("transform") or 0))
                + ",mirror="
                + lua_value(mirror)
                + "})"
            )
        return "\n".join(lines)

    def configure_monitors(self, records, *, timeout=10):
        return self._run(["eval", self.render_monitors(records)], timeout=timeout)

    def apply_config(self, text, *, timeout=10, check=True):
        """Apply a previously backend-rendered configuration, never caller-built actions."""
        return self._run(["eval", text], timeout=timeout, check=check)

    def ensure_desktop_loader(self, home):
        text = (home / ".config/hypr/hyprland.lua").read_text()
        if (
            "-- Nacre desktop settings" not in text
            or 'load_private("desktop")' not in text
        ):
            raise RuntimeError(
                "Managed desktop loader missing; run ./install.sh --apply from the repository"
            )

    def night_light_configured_elsewhere(self, home):
        return (Path(home) / ".config/hypr/hyprsunset.conf").exists()

    def night_light_available(self):
        return bool(shutil.which("hyprsunset"))

    def screenshot_window(self, path, window="", *, timeout=10):
        raise RuntimeError("Use the geometry-based capture helper on Hyprland")

    def night_light_external_socket(self, active):
        signature = self.session_id()
        runtime = os.environ.get("XDG_RUNTIME_DIR", "")
        if active or not signature or not runtime:
            return False
        path = Path(runtime) / "hypr" / signature / ".hyprsunset.sock"
        if not path.exists():
            return False
        try:
            with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as connection:
                connection.settimeout(0.15)
                connection.connect(str(path))
            return True
        except (ConnectionRefusedError, FileNotFoundError):
            return False
        except OSError:
            return True

    def set_night_light_temperature(self, enabled, *, timeout=1):
        return self._run(self.night_light_command(enabled)[1:], timeout=timeout)

    def night_light_command(self, enabled):
        return [
            "hyprctl",
            "hyprsunset",
            *(["temperature", "4500"] if enabled else ["identity"]),
        ]

    def event_socket(self):
        path = (
            Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
            / "hypr"
            / os.environ.get("HYPRLAND_INSTANCE_SIGNATURE", "")
            / ".socket2.sock"
        )
        connection = socket.socket(socket.AF_UNIX)
        try:
            connection.connect(str(path))
        except BaseException:
            connection.close()
            raise
        connection.setblocking(False)
        return connection

    def parse_event(self, line):
        name, _, data = line.partition(">>")
        names = {
            "activewindowv2": "window-focus",
            "urgent": "window-urgent",
            "openwindow": "window-open",
            "closewindow": "window-close",
            "movewindow": "window-workspace",
            "movewindowv2": "window-workspace",
            "windowtitle": "window-title",
            "windowtitlev2": "window-title",
            "changefloatingmode": "window-floating",
            "fullscreen": "window-fullscreen",
            "pin": "window-pin",
            "configreloaded": "configuration-reloaded",
        }
        event_type = (
            "outputs-changed"
            if name.startswith(("monitoradded", "monitorremoved"))
            else "workspace-monitor-change"
            if name.startswith(
                ("workspace", "focusedmon", "moveworkspace", "activespecial")
            )
            else names.get(name, "other")
        )
        event = {"type": event_type}
        window = data.split(",", 1)[0]
        if event_type.startswith("window-") and name != "fullscreen":
            candidate = window if window.startswith("0x") else "0x" + window
            if self.valid_window_id(candidate):
                event["windowId"] = candidate
        return event

    def events(self):
        connection = self.event_socket()
        try:
            connection.setblocking(True)
            yield {
                "type": "snapshot",
                "monitors": self.query("monitors"),
                "workspaces": self.query("workspaces"),
                "windows": self.query("windows"),
                "activeWindow": self.query("active_window"),
            }
            with connection.makefile("r") as lines:
                for line in lines:
                    yield self.parse_event(line.rstrip("\n"))
        finally:
            connection.close()

    def toggle_workspace_floating(self):
        expression = 'local ws = hl.get_active_workspace(); local windows = hl.get_workspace_windows(ws); local make_float = false; for _, window in ipairs(windows) do if not window.floating then make_float = true; break end end; for _, window in ipairs(windows) do hl.dispatch(hl.dsp.window.float({ action = make_float and "set" or "unset", window = window })) end'
        return self._run(["eval", expression])
