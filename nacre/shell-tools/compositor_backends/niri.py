"""Niri 26.04 JSON IPC, event-driven state and semantic compositor actions.

No connection is opened on import. Native u64 identities are encoded as strings
before crossing the QML boundary, where JavaScript numbers can lose precision.
"""

import json
import math
import os
from pathlib import Path
import re
import socket
import subprocess

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
DESKTOP_OPTIONS = {}
CAPABILITIES = {
    "stableWorkspaceIndices": False,
    "globalWorkspaces": False,
    "perOutputWorkspaces": True,
    "scrollingLayout": True,
    "floatingPlacement": True,
    "pinWindow": False,
    "windowCapture": False,
    "cursorPosition": False,
    "nativePanelDrag": False,
    "windowHide": False,
    "fullscreenState": False,
    "shortcutRelease": False,
    "desktopOptions": [],
    "monitorConfiguration": False,
    "nightLightController": False,
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

    def apply(self, event):
        if not isinstance(event, dict) or len(event) != 1:
            raise ValueError("Malformed niri event")
        name, payload = next(iter(event.items()))
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
            existed = key in self.windows
            self.windows[key] = dict(row)
            if row.get("is_focused"):
                for other in self.windows.values():
                    other["is_focused"] = other["id"] == row["id"]
            result = {
                "type": "window-changed" if existed else "window-open",
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
            result = {"type": "overview-changed"}
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
                        f"{mode['width']}x{mode['height']}@{mode['refresh_rate'] / 1000:.3f}"
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
                    "workspaceId": workspace["id"] if workspace else None,
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
        }


class Backend:
    name = "niri"
    display_name = "niri"
    query_names = tuple(QUERIES)
    capabilities = CAPABILITIES

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
        return response[name]

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
        if name in ("cursor", "instances", "bindings"):
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
        connection = self._connection()
        try:
            connection.sendall(b'"EventStream"\n')
            # Read exactly the acknowledgement; do not consume buffered events.
            reply = bytearray()
            while not reply.endswith(b"\n"):
                chunk = connection.recv(1)
                if not chunk or len(reply) >= MAX_BYTES:
                    raise ValueError("Invalid event stream acknowledgement")
                reply.extend(chunk)
            if json.loads(reply) != {"Ok": "Handled"}:
                raise RuntimeError("niri event stream was rejected")
            connection.setblocking(False)
            return connection
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
                    if "WorkspacesChanged" in event or "ConfigLoaded" in event:
                        # Outputs are not a niri event variant; hotplug/config updates
                        # change workspace membership, so refresh output metadata then.
                        state.outputs = self._response("Outputs")
                    if state.ready == {"window", "workspace"}:
                        if normalized["type"] != "other":
                            yield state.snapshot()
                            yield normalized
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
        return sorted((Path(home) / ".config/nacre/niri").glob("*bindings*.kdl"))

    def legacy_power_binding(self):
        return ""

    def _unsupported(self, *args, **kwargs):
        raise RuntimeError(
            "Operation is not supported by the niri backend; check compositor capabilities"
        )

    pin_window = kill_window = fullscreen_state = set_cursor = notify = _unsupported
    set_animations = set_performance = set_options = render_options = _unsupported
    render_desktop_options = set_desktop_options = get_desktop_option = _unsupported
    render_monitors = configure_monitors = apply_config = ensure_desktop_loader = (
        _unsupported
    )
    night_light_command = night_light_configured_elsewhere = (
        night_light_external_socket
    ) = _unsupported
    set_night_light_temperature = capture_bindings = _unsupported
