"""Niri 26.04 JSON IPC, event-driven state and semantic compositor actions.

No connection is opened on import. Native u64 identities are encoded as strings
before crossing the QML boundary, where JavaScript numbers can lose precision.
"""

import json
import math
import os
from pathlib import Path
import re
import shutil
import socket
import subprocess
import tempfile

MAX_BYTES = 4 * 1024 * 1024
QUERIES = dict.fromkeys(
    (
        "monitors",
        "all_monitors",
        "workspaces",
        "windows",
        "active_window",
        "active_workspace",
        "cursor",
        "instances",
        "bindings",
    )
)
DESKTOP_OPTIONS = dict.fromkeys(
    (
        "animations",
        "blur",
        "shadow",
        "followMouse",
        "naturalScroll",
        "gapsIn",
        "borderSize",
        "rounding",
    )
)
CAPABILITIES = {
    "stableWorkspaceIndices": False,
    "globalWorkspaces": False,
    "perOutputWorkspaces": True,
    "scrollingLayout": True,
    "floatingPlacement": True,
    "pinWindow": False,
    "windowCapture": False,
    "nativeWindowScreenshot": True,
    "shapedBackgroundBlur": False,
    "cursorPosition": False,
    "nativePanelDrag": False,
    "windowHide": False,
    "windowTrash": False,
    "scratchpad": False,
    "windowGroups": True,
    "windowForceKill": False,
    "pseudoTiling": False,
    "layoutSplit": False,
    "keymaps": False,
    "nonConsumingShortcuts": False,
    "bindingIntrospection": False,
    "fullscreenState": False,
    "shortcutRelease": False,
    "desktopOptions": list(DESKTOP_OPTIONS),
    "monitorConfiguration": True,
    "monitorMirroring": False,
    "nightLightController": True,
}


def native_id(value, kind):
    if not isinstance(value, str) or not re.fullmatch(
        r"niri-" + kind + r":(?:0|[1-9][0-9]{0,19})", value
    ):
        raise ValueError("Invalid opaque " + kind + " ID")
    result = int(value.partition(":")[2])
    if result > 2**64 - 1:
        raise ValueError("Invalid native ID range")
    return result


def identity(value, kind):
    if type(value) is not int or not 0 <= value <= 2**64 - 1:
        raise ValueError("Malformed native " + kind + " ID")
    return f"niri-{kind}:{value}"


def finite(value):
    if type(value) not in (int, float) or not math.isfinite(value):
        raise ValueError("Invalid geometry value")
    return value


def records(value):
    if not isinstance(value, list) or any(not isinstance(row, dict) for row in value):
        raise ValueError("Malformed niri records")
    return value


class State:
    """Reduce native snapshots/deltas; unknown future events leave state intact."""

    def __init__(self, outputs=None):
        self.outputs = outputs or {}
        self.workspaces = {}
        self.windows = {}
        self.ready = set()
        self.config_failed = False
        self.overview_open = False
        self.overview_hover_supported = False
        self.overview_hover_id = None

    def apply(self, event):
        if isinstance(event, str):
            return {"type": "other"}
        if not isinstance(event, dict) or len(event) != 1:
            raise ValueError("Malformed niri event")
        name, payload = next(iter(event.items()))
        known = {
            "WorkspacesChanged",
            "WindowsChanged",
            "WorkspaceActivated",
            "WorkspaceUrgencyChanged",
            "WorkspaceActiveWindowChanged",
            "WindowOpenedOrChanged",
            "WindowClosed",
            "WindowFocusChanged",
            "WindowUrgencyChanged",
            "WindowLayoutsChanged",
            "WindowFocusTimestampChanged",
            "ConfigLoaded",
            "OverviewOpenedOrClosed",
            "NacreOverviewHoverChanged",
        }
        if name not in known:
            return {"type": "other"}
        if not isinstance(payload, dict):
            raise ValueError("Malformed niri event payload")
        result = {"type": "other"}
        if name in ("WorkspacesChanged", "WindowsChanged"):
            kind = "workspace" if name == "WorkspacesChanged" else "window"
            key = kind + "s"
            replacement = {
                identity(row["id"], kind): dict(row) for row in records(payload[key])
            }
            setattr(self, key, replacement)
            self.ready.add(kind)
            result["type"] = (
                "workspace-monitor-change" if kind == "workspace" else "windows-changed"
            )
        elif name == "WorkspaceActivated":
            key = identity(payload["id"], "workspace")
            current = self.workspaces.get(key)
            if current:
                for row in self.workspaces.values():
                    if row.get("output") == current.get("output"):
                        row["is_active"] = row["id"] == payload["id"]
                    if payload.get("focused"):
                        row["is_focused"] = row["id"] == payload["id"]
            result = {"type": "workspace-monitor-change", "workspaceId": key}
        elif name in ("WorkspaceUrgencyChanged", "WorkspaceActiveWindowChanged"):
            key = identity(payload.get("id", payload.get("workspace_id")), "workspace")
            if key in self.workspaces:
                if name == "WorkspaceUrgencyChanged":
                    self.workspaces[key]["is_urgent"] = payload["urgent"]
                else:
                    self.workspaces[key]["active_window_id"] = payload[
                        "active_window_id"
                    ]
            result = {"type": "workspace-monitor-change", "workspaceId": key}
        elif name == "WindowOpenedOrChanged":
            row = payload["window"]
            if not isinstance(row, dict):
                raise ValueError("Malformed window event")
            key = identity(row["id"], "window")
            previous = self.windows.get(key)
            changes = [
                name
                for field, name in (
                    ("title", "window-title"),
                    ("workspace_id", "window-workspace"),
                    ("is_floating", "window-floating"),
                )
                if previous is not None and previous.get(field) != row.get(field)
            ]
            self.windows[key] = dict(row)
            if row.get("is_focused"):
                for other in self.windows.values():
                    other["is_focused"] = other["id"] == row["id"]
            result = {
                "type": changes[0]
                if changes
                else "window-changed"
                if previous is not None
                else "window-open",
                "changes": changes,
                "windowId": key,
            }
        elif name == "WindowClosed":
            key = identity(payload["id"], "window")
            self.windows.pop(key, None)
            result = {"type": "window-close", "windowId": key}
        elif name == "WindowFocusChanged":
            focused = payload["id"]
            for row in self.windows.values():
                row["is_focused"] = row["id"] == focused
            result = {
                "type": "window-focus",
                "windowId": identity(focused, "window")
                if focused is not None
                else None,
            }
        elif name == "WindowUrgencyChanged":
            key = identity(payload["id"], "window")
            if key in self.windows:
                self.windows[key]["is_urgent"] = payload["urgent"]
            result = {"type": "window-urgent", "windowId": key}
        elif name == "WindowLayoutsChanged":
            for value, layout in payload["changes"]:
                key = identity(value, "window")
                if key in self.windows:
                    self.windows[key]["layout"] = layout
            result = {"type": "window-geometry"}
        elif name == "WindowFocusTimestampChanged":
            key = identity(payload["id"], "window")
            if key in self.windows:
                self.windows[key]["focus_timestamp"] = payload["focus_timestamp"]
        elif name == "ConfigLoaded":
            self.config_failed = payload["failed"]
            result = {"type": "configuration-reloaded", "failed": self.config_failed}
        elif name == "OverviewOpenedOrClosed":
            self.overview_open = payload["is_open"]
            if not self.overview_open:
                self.overview_hover_id = None
            result = {"type": "overview-changed"}
        elif name == "NacreOverviewHoverChanged":
            value = payload["id"]
            key = identity(value, "window") if value is not None else None
            self.overview_hover_id = key if self.overview_open else None
            self.overview_hover_supported = True
            result = {"type": "overview-hover"}
        return result

    def snapshot(self):
        workspaces = []
        for key, row in self.workspaces.items():
            workspaces.append(
                {
                    "id": key,
                    "workspaceId": key,
                    "index": row["idx"],
                    "name": row.get("name") or str(row["idx"]),
                    "outputId": row.get("output"),
                    "monitor": row.get("output"),
                    "active": bool(row.get("is_active")),
                    "focused": bool(row.get("is_focused")),
                    "urgent": bool(row.get("is_urgent")),
                    "activeWindowId": identity(row["active_window_id"], "window")
                    if row.get("active_window_id") is not None
                    else None,
                }
            )
        workspaces.sort(key=lambda row: (row["outputId"] or "", row["index"]))
        focused = next((row for row in workspaces if row["focused"]), None)
        monitors = []
        transforms = {
            "Normal": 0,
            "90": 1,
            "180": 2,
            "270": 3,
            "Flipped": 4,
            "Flipped90": 5,
            "Flipped180": 6,
            "Flipped270": 7,
        }
        for name, row in self.outputs.items():
            logical = row.get("logical")
            if not logical:
                continue
            modes = row.get("modes", [])
            index = row.get("current_mode")
            mode = (
                modes[index] if type(index) is int and 0 <= index < len(modes) else {}
            )
            monitors.append(
                {
                    "id": name,
                    "name": name,
                    "outputId": name,
                    "x": logical["x"],
                    "y": logical["y"],
                    "width": mode.get("width", logical["width"]),
                    "height": mode.get("height", logical["height"]),
                    "logicalWidth": logical["width"],
                    "logicalHeight": logical["height"],
                    "scale": logical.get("scale", 1),
                    "transform": transforms.get(logical.get("transform"), 0),
                    "refreshRate": mode.get("refresh_rate", 0) / 1000,
                    "reserved": [0, 0, 0, 0],
                    "reservedKnown": False,
                    "focused": focused is not None and focused["outputId"] == name,
                    "activeWorkspace": next(
                        (
                            workspace
                            for workspace in workspaces
                            if workspace["outputId"] == name and workspace["active"]
                        ),
                        None,
                    ),
                    "availableModes": [
                        f"{mode['width']}x{mode['height']}@{mode['refresh_rate'] / 1000:.3f}Hz"
                        for mode in modes
                    ],
                }
            )
        windows = []
        for key, row in self.windows.items():
            workspace = (
                next(
                    (
                        item
                        for item in workspaces
                        if item["id"] == identity(row["workspace_id"], "workspace")
                    ),
                    None,
                )
                if row.get("workspace_id") is not None
                else None
            )
            monitor = next(
                (
                    item
                    for item in monitors
                    if workspace and item["name"] == workspace["outputId"]
                ),
                None,
            )
            layout = row.get("layout") or {}
            position = layout.get("tile_pos_in_workspace_view")
            offset = layout.get("window_offset_in_tile", [0, 0])
            size = layout.get("window_size", [0, 0])
            known = position is not None and monitor is not None
            x = monitor["x"] + position[0] + offset[0] if known else None
            y = monitor["y"] + position[1] + offset[1] if known else None
            geometry = {"x": x, "y": y, "width": size[0], "height": size[1]}
            windows.append(
                {
                    "id": key,
                    "windowId": key,
                    "address": key,
                    "appId": row.get("app_id") or "",
                    "class": row.get("app_id") or "",
                    "title": row.get("title") or "",
                    "pid": row.get("pid") or 0,
                    "workspaceId": identity(row["workspace_id"], "workspace")
                    if row.get("workspace_id") is not None
                    else None,
                    "workspace": workspace
                    or {
                        "id": identity(row["workspace_id"], "workspace")
                        if row.get("workspace_id") is not None
                        else None
                    },
                    "outputId": workspace["outputId"] if workspace else None,
                    "monitor": workspace["outputId"] if workspace else None,
                    "geometry": geometry,
                    "geometryKnown": known,
                    "moving": row.get("workspace_id") is None and position is None,
                    "at": [x, y],
                    "size": size,
                    "floating": bool(row.get("is_floating")),
                    "focused": bool(row.get("is_focused")),
                    "urgent": bool(row.get("is_urgent")) and not row.get("is_focused"),
                    "hidden": False,
                    "pinned": False,
                    "fullscreen": 0,
                    "fullscreenMode": 0,
                    "fullscreenKnown": False,
                }
            )
        return {
            "type": "snapshot",
            "backend": "niri",
            "capabilities": CAPABILITIES,
            "monitors": monitors,
            "workspaces": workspaces,
            "windows": windows,
            "activeWindow": next((row for row in windows if row["focused"]), None),
            "activeWorkspace": focused,
            "focusedMonitor": next((row for row in monitors if row["focused"]), None),
            "overviewOpen": self.overview_open,
            "overviewHoverSupported": self.overview_hover_supported,
            "overviewHoveredWindowId": self.overview_hover_id,
            "configFailed": self.config_failed,
        }


class Backend:
    name = "niri"
    display_name = "niri"
    query_names = tuple(QUERIES)
    capabilities = CAPABILITIES
    night_light_tool = "wlsunset"
    night_light_service = "nacre-night-light.service"
    portal_service = "xdg-desktop-portal-gnome.service"

    def session_id(self):
        return os.environ.get("NIRI_SOCKET", "")

    def version_command(self):
        return ["niri", "--version"]

    def required_commands(self):
        return ("niri",)

    def config_errors(self, *, timeout=10):
        result = subprocess.run(
            ["niri", "validate"],
            capture_output=True,
            text=True,
            timeout=timeout,
            check=False,
        )
        return (result.stderr or result.stdout).strip() if result.returncode else ""

    def diagnostic_commands(self):
        return {
            "runningVersion": ["niri", "msg", "--json", "version"],
            "installedVersion": self.version_command(),
            "allMonitors": ["niri", "msg", "--json", "outputs"],
            "configErrors": ["niri", "validate"],
        }

    def environment_updates(self, environment=None):
        environment = os.environ if environment is None else environment
        return {
            key: environment[key]
            for key in (
                "NIRI_SOCKET",
                "WAYLAND_DISPLAY",
                "XDG_RUNTIME_DIR",
                "NACRE_COMPOSITOR",
            )
            if environment.get(key)
        }

    def valid_window_id(self, value):
        try:
            native_id(value, "window")
            return True
        except ValueError:
            return False

    def window_job_token(self, value):
        return str(native_id(value, "window"))

    def _connection(self, timeout=10):
        path = self.session_id()
        if not path or "\0" in path:
            raise RuntimeError("NIRI_SOCKET is unavailable")
        connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        try:
            connection.settimeout(timeout)
            connection.connect(path)
            return connection
        except BaseException:
            connection.close()
            raise

    def _request(self, request, *, timeout=10, max_bytes=MAX_BYTES):
        with self._connection(timeout) as connection:
            connection.sendall((json.dumps(request, allow_nan=False) + "\n").encode())
            with connection.makefile("rb") as stream:
                line = stream.readline(max_bytes + 1)
            if not line or len(line) > max_bytes or not line.endswith(b"\n"):
                raise ValueError("Truncated or oversized niri reply")
        response = json.loads(line)
        if not isinstance(response, dict) or len(response) != 1:
            raise ValueError("Malformed niri reply")
        if "Err" in response:
            raise RuntimeError("niri IPC: " + str(response["Err"]))
        if "Ok" not in response:
            raise ValueError("Missing niri reply status")
        return response["Ok"]

    def _response(self, name, *, timeout=10, max_bytes=MAX_BYTES):
        response = self._request(name, timeout=timeout, max_bytes=max_bytes)
        if not isinstance(response, dict) or name not in response:
            raise ValueError("Unexpected niri response")
        result = response[name]
        if name == "Outputs" and (
            not isinstance(result, dict)
            or any(not isinstance(row, dict) for row in result.values())
        ):
            raise ValueError("Malformed niri output records")
        return result

    def _action(self, name, values=None, *, timeout=10):
        response = self._request({"Action": {name: values or {}}}, timeout=timeout)
        if response != "Handled":
            raise ValueError("Unexpected niri action reply")
        return subprocess.CompletedProcess([], 0, "Handled", "")

    def _state(self, timeout=10, max_bytes=MAX_BYTES):
        state = State(self._response("Outputs", timeout=timeout, max_bytes=max_bytes))
        state.apply(
            {
                "WorkspacesChanged": {
                    "workspaces": self._response(
                        "Workspaces", timeout=timeout, max_bytes=max_bytes
                    )
                }
            }
        )
        state.apply(
            {
                "WindowsChanged": {
                    "windows": self._response(
                        "Windows", timeout=timeout, max_bytes=max_bytes
                    )
                }
            }
        )
        return state.snapshot()

    def query(self, name, *, timeout=10, max_bytes=None):
        if name not in QUERIES:
            raise ValueError("Unknown compositor query")
        if name == "bindings":
            result = []
            for path in self.binding_source_files(Path.home()):
                for record in self.capture_bindings(path):
                    pieces = record["keys"].split("+")
                    result.append(
                        {
                            "modmask": sum(
                                {"SUPER": 64, "CTRL": 4, "ALT": 8, "SHIFT": 1}.get(
                                    part, 0
                                )
                                for part in pieces[:-1]
                            ),
                            "key": pieces[-1],
                            "submap": "",
                            "description": record["flags"]["description"],
                            "dispatcher": "__lua",
                            "arg": "",
                        }
                    )
            return result
        if name in ("cursor", "instances"):
            raise RuntimeError(
                "niri does not expose " + name + " through this interface"
            )
        state = self._state(timeout, MAX_BYTES if max_bytes is None else max_bytes)
        return state[
            {
                "all_monitors": "monitors",
                "active_window": "activeWindow",
                "active_workspace": "activeWorkspace",
            }.get(name, name)
        ] or ({} if name.startswith("active_") else [])

    def query_result(self, name, *, timeout=10, check=True):
        try:
            value = self.query(name, timeout=timeout)
        except (OSError, ValueError, RuntimeError) as error:
            if check:
                raise
            return subprocess.CompletedProcess([], 1, "", str(error))
        return subprocess.CompletedProcess([], 0, json.dumps(value), "")

    def _window(self, value, timeout=10):
        if value:
            native_id(value, "window")
            windows = self.query("windows", timeout=timeout)
            row = next((row for row in windows if row["windowId"] == value), None)
        else:
            row = self.query("active_window", timeout=timeout)
        if not row:
            raise ValueError("Window no longer exists or no window is focused")
        return row

    def _window_id(self, value, timeout=10):
        return native_id(value or self._window(value, timeout)["windowId"], "window")

    def workspace_reference(self, value, *, timeout=10):
        state = self._state(timeout)
        rows = state["workspaces"]
        output = state["focusedMonitor"]
        output = output["name"] if output else None
        if isinstance(value, dict):
            if "relative" in value:
                offset = value["relative"]
                active = state["activeWorkspace"]
                if type(offset) is not int or offset not in (-1, 1) or not active:
                    raise ValueError("Invalid relative workspace reference")
                match = next(
                    (
                        row
                        for row in rows
                        if row["outputId"] == active["outputId"]
                        and row["index"] == active["index"] + offset
                    ),
                    None,
                )
            elif "id" in value:
                key = value["id"]
                native_id(key, "workspace")
                match = next((row for row in rows if row["id"] == key), None)
            elif "name" in value:
                match = next(
                    (
                        row
                        for row in rows
                        if row["name"] == value["name"]
                        and (
                            "output" not in value or row["outputId"] == value["output"]
                        )
                    ),
                    None,
                )
            else:
                output = value.get("output", output)
                index = value.get("index")
                if type(index) is not int or not 1 <= index <= 255:
                    raise ValueError("Invalid workspace index")
                match = next(
                    (
                        row
                        for row in rows
                        if row["outputId"] == output and row["index"] == index
                    ),
                    None,
                )
        elif isinstance(value, str) and value.startswith("niri-workspace:"):
            native_id(value, "workspace")
            match = next((row for row in rows if row["id"] == value), None)
        elif type(value) is int or isinstance(value, str) and value.isdecimal():
            index = int(value)
            match = next(
                (
                    row
                    for row in rows
                    if row["outputId"] == output and row["index"] == index
                ),
                None,
            )
        elif isinstance(value, str) and value and len(value) <= 200:
            match = next((row for row in rows if row["name"] == value), None)
        else:
            raise ValueError("Invalid workspace reference")
        if not match:
            raise ValueError("Workspace does not exist on the requested output")
        return {"Id": native_id(match["id"], "workspace")}

    def switch_workspace(self, workspace, *, current_monitor=True, timeout=10):
        return self._action(
            "FocusWorkspace",
            {"reference": self.workspace_reference(workspace, timeout=timeout)},
            timeout=timeout,
        )

    def focus_direction(self, direction, *, timeout=10):
        names = {
            "left": "FocusColumnLeft",
            "right": "FocusColumnRight",
            "up": "FocusWindowUp",
            "down": "FocusWindowDown",
        }
        if direction not in names:
            raise ValueError("Invalid focus direction")
        return self._action(names[direction], timeout=timeout)

    def cycle_window(self, direction="next", *, timeout=10):
        if direction not in ("next", "previous"):
            raise ValueError("Invalid cycle direction")
        state = self._state(timeout)
        active = state["activeWorkspace"]
        windows = sorted(
            (
                row
                for row in state["windows"]
                if active and row["workspaceId"] == active["id"]
            ),
            key=lambda row: native_id(row["windowId"], "window"),
        )
        if not windows:
            raise ValueError("No windows in the active workspace")
        current = state["activeWindow"]
        index = next(
            (
                i
                for i, row in enumerate(windows)
                if current and row["windowId"] == current["windowId"]
            ),
            -1,
        )
        target = windows[(index + (1 if direction == "next" else -1)) % len(windows)]
        return self.focus_window(target["windowId"], timeout=timeout)

    def move_direction(self, direction, *, timeout=10):
        names = {
            "left": "MoveColumnLeft",
            "right": "MoveColumnRight",
            "up": "MoveWindowUp",
            "down": "MoveWindowDown",
        }
        if direction not in names:
            raise ValueError("Invalid move direction")
        return self._action(names[direction], timeout=timeout)

    def focus_window(self, window="", *, timeout=10):
        return self._action(
            "FocusWindow", {"id": self._window_id(window, timeout)}, timeout=timeout
        )

    def close_window(self, window="", *, timeout=10):
        return self._action(
            "CloseWindow", {"id": self._window_id(window, timeout)}, timeout=timeout
        )

    def screenshot_window(self, path, window="", *, timeout=10):
        path = Path(path)
        if not path.is_absolute():
            raise ValueError("Screenshot path must be absolute")
        return self._action(
            "ScreenshotWindow",
            {
                "id": self._window_id(window, timeout),
                "path": str(path),
                "write_to_disk": True,
                "show_pointer": False,
            },
            timeout=timeout,
        )

    def move_workspace(self, workspace, monitor, *, timeout=10):
        if monitor not in [
            row["name"] for row in self.query("monitors", timeout=timeout)
        ]:
            raise ValueError("Unknown output")
        return self._action(
            "MoveWorkspaceToMonitor",
            {
                "reference": self.workspace_reference(workspace, timeout=timeout),
                "output": monitor,
            },
            timeout=timeout,
        )

    def float_window(self, window="", *, action="toggle", timeout=10):
        names = {
            "toggle": "ToggleWindowFloating",
            "enable": "MoveWindowToFloating",
            "disable": "MoveWindowToTiling",
            "set": "MoveWindowToFloating",
            "unset": "MoveWindowToTiling",
        }
        if action not in names:
            raise ValueError("Invalid floating action")
        return self._action(
            names[action], {"id": self._window_id(window, timeout)}, timeout=timeout
        )

    def fullscreen_window(self, window="", *, mode="fullscreen", timeout=10):
        if mode not in ("fullscreen", "maximized"):
            raise ValueError("Invalid fullscreen mode")
        return self._action(
            "FullscreenWindow" if mode == "fullscreen" else "MaximizeWindowToEdges",
            {"id": self._window_id(window, timeout)},
            timeout=timeout,
        )

    def resize_window(
        self, window="", width=None, height=None, *, relative=False, timeout=10
    ):
        for value in (width, height):
            finite(value)
            if not relative and value <= 0:
                raise ValueError("Invalid window size")
        identifier = self._window_id(window, timeout)
        change = "AdjustFixed" if relative else "SetFixed"
        self._action(
            "SetWindowWidth",
            {"id": identifier, "change": {change: round(width)}},
            timeout=timeout,
        )
        return self._action(
            "SetWindowHeight",
            {"id": identifier, "change": {change: round(height)}},
            timeout=timeout,
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
        if workspace is not None:
            if x is not None or y is not None:
                raise ValueError("Specify a workspace or geometry, not both")
            return self._action(
                "MoveWindowToWorkspace",
                {
                    "window_id": self._window_id(window, timeout),
                    "reference": self.workspace_reference(workspace, timeout=timeout),
                    "focus": bool(follow),
                },
                timeout=timeout,
            )
        finite(x)
        finite(y)
        row = self._window(window, timeout)
        if not row["floating"]:
            raise ValueError("Absolute placement requires a floating window")
        if not relative:
            outputs = self.query("monitors", timeout=timeout)
            target = next(
                (
                    out
                    for out in outputs
                    if out["x"] <= x < out["x"] + out["logicalWidth"]
                    and out["y"] <= y < out["y"] + out["logicalHeight"]
                ),
                None,
            )
            if target and target["name"] != row["outputId"]:
                self._action(
                    "MoveWindowToMonitor",
                    {
                        "id": native_id(row["windowId"], "window"),
                        "output": target["name"],
                    },
                    timeout=timeout,
                )
                row = self._window(row["windowId"], timeout)
            if not row["geometryKnown"]:
                raise RuntimeError("Floating window position is not yet available")
            x -= row["geometry"]["x"]
            y -= row["geometry"]["y"]
        return self._action(
            "MoveFloatingWindow",
            {
                "id": native_id(row["windowId"], "window"),
                "x": {"AdjustFixed": x},
                "y": {"AdjustFixed": y},
            },
            timeout=timeout,
        )

    def place_window(self, window, *, x, y, width=None, height=None, timeout=10):
        finite(x)
        finite(y)
        if (width is None) != (height is None):
            raise ValueError("Specify both size dimensions")
        if width is not None:
            if finite(width) <= 0 or finite(height) <= 0:
                raise ValueError("Invalid window size")
        self.float_window(window, action="enable", timeout=timeout)
        if width is not None:
            self.resize_window(window, width, height, timeout=timeout)
        return self.move_window(window, x=x, y=y, timeout=timeout)

    def dpms(self, action, monitor=None, *, timeout=10):
        if monitor is not None:
            raise RuntimeError("niri DPMS controls all outputs together")
        if action not in ("enable", "disable", "on", "off"):
            raise ValueError("Invalid DPMS action")
        return self._action(
            "PowerOnMonitors" if action in ("enable", "on") else "PowerOffMonitors",
            timeout=timeout,
        )

    def logout(self, *, timeout=10, graceful=False):
        return self._action("Quit", {"skip_confirmation": True}, timeout=timeout)

    def reload(self, *, timeout=10):
        return self._action("LoadConfigFile", {"path": None}, timeout=timeout)

    def event_socket(self):
        requests = ["EventStream"]
        # Negotiate once at connection time, including ordinary package installs.
        # Stock Niri rejects the opt-in request; reconnect using its legacy stream.
        if os.environ.get("NACRE_NIRI_HOVER_STREAM", "1") != "0":
            requests.insert(0, "NacreOverviewHoverStream")
        for request in requests:
            connection = self._connection()
            try:
                connection.sendall((json.dumps(request) + "\n").encode())
                # Read exactly the acknowledgement; do not consume buffered events.
                reply = bytearray()
                while not reply.endswith(b"\n"):
                    chunk = connection.recv(1)
                    if not chunk or len(reply) >= MAX_BYTES:
                        raise ValueError("Invalid event stream acknowledgement")
                    reply.extend(chunk)
                if json.loads(reply) == {"Ok": "Handled"}:
                    connection.setblocking(False)
                    return connection
                if request == "EventStream":
                    raise RuntimeError("niri event stream was rejected")
                connection.close()
            except BaseException:
                connection.close()
                raise

    def parse_event(self, line):
        return State().apply(json.loads(line))

    def events(self):
        connection = self.event_socket()
        try:
            state = State(self._response("Outputs"))
            connection.setblocking(True)
            with connection.makefile("rb") as stream:
                while True:
                    line = stream.readline(MAX_BYTES + 1)
                    if not line:
                        raise ConnectionError("niri event stream disconnected")
                    if len(line) > MAX_BYTES or not line.endswith(b"\n"):
                        raise ValueError("Truncated or oversized niri event")
                    event = json.loads(line)
                    normalized = state.apply(event)
                    outputs_changed = False
                    if "WorkspacesChanged" in event or "ConfigLoaded" in event:
                        # Outputs are not a niri event variant; hotplug/config updates
                        # change workspace membership, so refresh output metadata then.
                        outputs = self._response("Outputs")
                        outputs_changed = outputs != state.outputs
                        state.outputs = outputs
                    if state.ready == {"window", "workspace"}:
                        if normalized["type"] != "other":
                            yield state.snapshot()
                            yield normalized
                            for event_type in normalized.get("changes", [])[1:]:
                                yield {
                                    "type": event_type,
                                    "windowId": normalized["windowId"],
                                }
                            if outputs_changed:
                                yield {"type": "outputs-changed"}
        finally:
            connection.close()

    def toggle_workspace_floating(self):
        state = self._state()
        active = state["activeWorkspace"]
        windows = [
            row
            for row in state["windows"]
            if active and row["workspaceId"] == active["id"]
        ]
        floating = any(not row["floating"] for row in windows)
        for row in windows:
            self.float_window(
                row["windowId"], action="enable" if floating else "disable"
            )

    def config_path(self, home, kind):
        if kind not in ("palette", "desktop", "monitor", "profile"):
            raise ValueError("Unknown compositor configuration")
        if kind == "desktop" and os.environ.get("NACRE_NIRI_DESKTOP_CONFIG"):
            return Path(os.environ["NACRE_NIRI_DESKTOP_CONFIG"])
        return Path(home) / ".config/nacre/niri" / (kind + ".kdl")

    def render_keyboard(self, values):
        allowed = {"layout", "model", "variant", "options", "rules"}
        if set(values) - allowed:
            raise ValueError("Unknown keyboard option")
        return (
            "input {\n    keyboard {\n        xkb {\n"
            + "".join(
                f"            {key} {json.dumps(value)}\n"
                for key, value in values.items()
            )
            + "        }\n    }\n}\n"
        )

    def palette_template_path(self):
        return Path(__file__).with_name("niri") / "palette.kdl.in"

    def binding_source_files(self, home):
        if os.environ.get("NACRE_NIRI_CONFIG"):
            return sorted(
                Path(os.environ["NACRE_NIRI_CONFIG"]).parent.glob("*bindings*.kdl")
            )
        return sorted((Path(home) / ".config/nacre/niri").glob("*bindings*.kdl"))

    def capture_bindings(self, path, kind="bindings"):
        if kind != "bindings":
            raise ValueError("Only generated niri bindings can be inspected")
        text = Path(path).read_text()
        if not text.startswith("// Generated from compositor-contract.json;"):
            raise RuntimeError("Private KDL overrides require niri's hotkey overlay")
        records = []
        pattern = (
            r'^\s+(\S+)\s+(.+?)\s+\{\s+(spawn-sh "(?:[^"\\]|\\.)*"|[a-z-]+);\s+\}$'
        )
        for line in text.splitlines():
            match = re.fullmatch(pattern, line)
            if not match:
                continue
            key, flags, action = match.groups()
            key = (
                key.replace("Super", "SUPER")
                .replace("Ctrl", "CTRL")
                .replace("Alt", "ALT")
                .replace("Shift", "SHIFT")
            )
            description = re.search(r'hotkey-overlay-title=("(?:[^"\\]|\\.)*")', flags)
            records.append(
                {
                    "keys": key,
                    "submap": "",
                    "flags": {
                        "description": json.loads(description[1]) if description else ""
                    },
                    "action": {
                        "method": "exec_cmd"
                        if action.startswith("spawn-sh ")
                        else action,
                        "args": [json.loads(action[9:])]
                        if action.startswith("spawn-sh ")
                        else [],
                    },
                }
            )
        return records

    def render_desktop_options(self, preferences):
        lines = []
        for key in DESKTOP_OPTIONS:
            if key not in preferences:
                continue
            value = preferences[key]
            if key in ("gapsIn", "borderSize", "rounding"):
                if (
                    type(value) not in (int, float)
                    or not math.isfinite(value)
                    or not 0 <= value <= 100
                ):
                    raise ValueError("Invalid compositor dimension")
            elif type(value) is not bool:
                raise ValueError("Invalid compositor toggle")
        if "animations" in preferences:
            lines.append(
                "animations { " + ("" if preferences["animations"] else "off;") + " }"
            )
        layout = []
        if "gapsIn" in preferences:
            layout.append("gaps " + str(preferences["gapsIn"]))
        if "borderSize" in preferences:
            layout.append("border { width " + str(preferences["borderSize"]) + "; }")
            layout.append(
                "focus-ring { width " + str(preferences["borderSize"]) + "; }"
            )
        if layout:
            lines.append("layout {\n    " + "\n    ".join(layout) + "\n}")
        rules = []
        if "rounding" in preferences:
            lines.append(
                'window-rule {\n    exclude app-id="^org[.]quickshell$" title="^Nacre Pearl "\n    geometry-corner-radius '
                + str(preferences["rounding"])
                + "\n    clip-to-geometry true\n}"
            )
        if "shadow" in preferences:
            rules.append(
                "shadow { " + ("on" if preferences["shadow"] else "off") + "; }"
            )
        if "blur" in preferences:
            value = str(bool(preferences["blur"])).lower()
            rules.append("background-effect { blur " + value + "; }")
            lines.append(
                'layer-rule {\n    match namespace="^nacre-(topbar|lock-preview|left-menu-handle|right-menu-handle)$"\n    background-effect { blur '
                + value
                + "; }\n}"
            )
        if rules:
            lines.append("window-rule {\n    " + "\n    ".join(rules) + "\n}")
        input_rules = []
        if "followMouse" in preferences:
            if preferences["followMouse"]:
                input_rules.append("focus-follows-mouse")
        if "naturalScroll" in preferences:
            input_rules.append(
                "touchpad { "
                + (
                    "natural-scroll; scroll-factor 0.25;"
                    if preferences["naturalScroll"]
                    else "scroll-factor 0.25;"
                )
                + " }"
            )
        if input_rules or "followMouse" in preferences:
            lines.append("input {\n    " + "\n    ".join(input_rules) + "\n}")
        return "\n".join(lines) + "\n"

    def render_monitors(self, monitors):
        lines = []
        for monitor in monitors:
            name = monitor.get("name")
            if (
                not isinstance(name, str)
                or not name
                or any(ord(char) < 32 for char in name)
            ):
                raise ValueError("Invalid output name")
            if monitor.get("mirrorOf") not in (None, "", "none"):
                raise ValueError("Output mirroring is unavailable on niri")
            if monitor.get("disabled"):
                raise ValueError(
                    "Disabling outputs is unavailable in this display transaction"
                )
            for key in (
                "width",
                "height",
                "scale",
                "refreshRate",
                "x",
                "y",
                "transform",
            ):
                value = monitor.get(key, 0)
                if type(value) not in (int, float) or not math.isfinite(value):
                    raise ValueError("Invalid output geometry")
            if (
                not 0 < monitor["scale"] <= 10
                or not 0 < monitor["width"] <= 32768
                or not 0 < monitor["height"] <= 32768
                or not 0 <= monitor.get("refreshRate", 0) <= 1000
                or monitor.get("transform", 0) not in range(8)
            ):
                raise ValueError("Invalid output geometry")
            transforms = (
                "normal",
                "90",
                "180",
                "270",
                "flipped",
                "flipped-90",
                "flipped-180",
                "flipped-270",
            )
            lines.append("output " + json.dumps(name) + " {")
            if monitor.get("refreshRate"):
                lines.append(
                    f'    mode "{int(monitor["width"])}x{int(monitor["height"])}@{monitor["refreshRate"]:.3f}"'
                )
            lines += [
                "    scale " + str(monitor["scale"]),
                f"    position x={int(monitor['x'])} y={int(monitor['y'])}",
                "    transform "
                + json.dumps(transforms[int(monitor.get("transform", 0))]),
                "}",
            ]
        return "\n".join(lines) + ("\n" if lines else "")

    def configure_monitors(self, records, *, timeout=10):
        self.render_monitors(records)  # Validate the whole transaction before mutation.
        previous = {row["name"]: row for row in self.query("monitors", timeout=timeout)}
        if any(row["name"] not in previous for row in records):
            raise ValueError("Selected display is disconnected")
        transforms = (
            "Normal",
            "90",
            "180",
            "270",
            "Flipped",
            "Flipped90",
            "Flipped180",
            "Flipped270",
        )

        def actions(row):
            result = []
            if row.get("refreshRate"):
                result.append(
                    {
                        "Mode": {
                            "mode": {
                                "Specific": {
                                    "width": int(row["width"]),
                                    "height": int(row["height"]),
                                    "refresh": row["refreshRate"],
                                }
                            }
                        }
                    }
                )
            return result + [
                {"Scale": {"scale": {"Specific": row["scale"]}}},
                {"Transform": {"transform": transforms[int(row.get("transform", 0))]}},
                {
                    "Position": {
                        "position": {
                            "Specific": {"x": int(row["x"]), "y": int(row["y"])}
                        }
                    }
                },
            ]

        try:
            for row in records:
                for action in actions(row):
                    response = self._request(
                        {"Output": {"output": row["name"], "action": action}},
                        timeout=timeout,
                    )
                    if response != {"OutputConfigChanged": "Applied"}:
                        raise RuntimeError("Output configuration was not applied")
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            # Restore even an output removed during the transaction: niri queues
            # its original values for reconnection rather than the failed plan.
            for row in records:
                for action in actions(previous[row["name"]]):
                    response = self._request(
                        {"Output": {"output": row["name"], "action": action}},
                        timeout=timeout,
                    )
                    if response not in (
                        {"OutputConfigChanged": "Applied"},
                        {"OutputConfigChanged": "OutputWasMissing"},
                    ):
                        raise RuntimeError(
                            "Output change failed and rollback was not accepted"
                        ) from error
            raise
        return subprocess.CompletedProcess([], 0, "ok\n", "")

    def ensure_desktop_loader(self, home):
        config = Path(
            os.environ.get(
                "NACRE_NIRI_CONFIG", str(Path(home) / ".config/nacre/niri/config.kdl")
            )
        )
        if not config.exists() or 'include "desktop.kdl"' not in config.read_text():
            raise RuntimeError(
                "Managed niri desktop include missing; regenerate the isolated session configuration"
            )

    def set_desktop_options(self, preferences, *, timeout=10):
        home = Path.home()
        self.ensure_desktop_loader(home)
        state = home / ".config/nacre/desktop.json"
        values = json.loads(state.read_text()) if state.exists() else {}
        values.update(preferences)
        target = self.config_path(home, "desktop")
        descriptor, temporary = tempfile.mkstemp(dir=target.parent)
        previous = target.read_bytes() if target.exists() else None
        try:
            with os.fdopen(descriptor, "w") as stream:
                stream.write(self.render_desktop_options(values))
                monitors = values.get("compositorDisplays", {}).get("niri")
                if monitors is None:
                    monitors = self.query("monitors", timeout=timeout)
                stream.write(self.render_monitors(monitors))
            subprocess.run(
                ["niri", "validate", "-c", temporary],
                check=True,
                capture_output=True,
                timeout=timeout,
            )
            os.replace(temporary, target)
            try:
                return self.reload(timeout=timeout)
            except (OSError, RuntimeError, subprocess.SubprocessError):
                if previous is None:
                    target.unlink(missing_ok=True)
                else:
                    target.write_bytes(previous)
                raise
        finally:
            Path(temporary).unlink(missing_ok=True)

    def get_desktop_option(self, key, *, timeout=8):
        raise RuntimeError("Niri does not expose resolved configuration options")

    def legacy_power_binding(self):
        return ""

    def night_light_available(self):
        if not shutil.which(self.night_light_tool) or not shutil.which("wayland-info"):
            return False
        try:
            result = subprocess.run(
                ["wayland-info"], capture_output=True, text=True, timeout=3
            )
            return (
                result.returncode == 0
                and "zwlr_gamma_control_manager_v1" in result.stdout
            )
        except (OSError, subprocess.SubprocessError):
            return False

    def night_light_configured_elsewhere(self, home):
        return any(
            (Path(home) / ".config" / name).exists()
            for name in ("gammastep/config.ini", "wlsunset/config")
        )

    def night_light_external_socket(self, active):
        # Gamma controllers have no common control socket. Inspect only this
        # display's processes, excluding the exact owned unit's MainPID.
        own = ""
        if active:
            own = subprocess.run(
                [
                    "systemctl",
                    "--user",
                    "show",
                    self.night_light_service,
                    "-p",
                    "MainPID",
                    "--value",
                ],
                capture_output=True,
                text=True,
                timeout=3,
            ).stdout.strip()
        display = os.environ.get("WAYLAND_DISPLAY", "")
        if not display:
            return False
        for process in Path("/proc").glob("[0-9]*"):
            if process.name == own:
                continue
            try:
                command = process.joinpath("cmdline").read_bytes().split(b"\0")[0]
                if Path(os.fsdecode(command)).name not in (
                    "wlsunset",
                    "gammastep",
                    "hyprsunset",
                ):
                    continue
                environment = process.joinpath("environ").read_bytes().split(b"\0")
                if (
                    os.fsencode("WAYLAND_DISPLAY=" + display) in environment
                    and os.fsencode(
                        "XDG_RUNTIME_DIR=" + os.environ.get("XDG_RUNTIME_DIR", "")
                    )
                    in environment
                ):
                    return True
            except OSError:
                continue
        return False

    def night_light_command(self, enabled):
        if not enabled:
            return ["systemctl", "--user", "stop", self.night_light_service]
        return [
            "systemd-run",
            "--user",
            "--collect",
            "--unit=" + self.night_light_service,
            "--property=PartOf=graphical-session.target",
            "--setenv=WAYLAND_DISPLAY=" + os.environ.get("WAYLAND_DISPLAY", ""),
            "--setenv=XDG_RUNTIME_DIR=" + os.environ.get("XDG_RUNTIME_DIR", ""),
            # Adjacent warm endpoints avoid schedule-dependent color changes;
            # wlsunset requires high > low. Stopping releases gamma ownership.
            "wlsunset",
            "-t",
            "4500",
            "-T",
            "4501",
            "-S",
            "00:00",
            "-s",
            "23:59",
            "-d",
            "0",
        ]

    def set_night_light_temperature(self, enabled, *, timeout=1):
        if enabled:
            if not self.night_light_available():
                raise RuntimeError(
                    "Night light requires gamma control on a physical output"
                )
            return subprocess.CompletedProcess([], 0, "", "")
        return subprocess.run(
            self.night_light_command(False),
            check=True,
            capture_output=True,
            timeout=timeout,
        )

    def _unsupported(self, *args, **kwargs):
        raise RuntimeError(
            "Operation is not supported by the niri backend; check compositor capabilities"
        )

    pin_window = kill_window = fullscreen_state = set_cursor = notify = _unsupported
    set_animations = set_performance = set_options = render_options = _unsupported
    apply_config = _unsupported
