"""Validated compositor-neutral declarations and deterministic Hyprland output.

The JSON contract is the editable source. Generated Lua is checked into Git for
ordinary installations; neither Lua nor the generator runs application commands.
Backend callbacks are explicit capabilities, not portable behavior promises.
"""

import json
import re
from pathlib import Path

DIRECTORY = Path(__file__).resolve().parents[1]
OUTPUTS = (
    "hypr/conf/cursor.lua",
    "hypr/conf/keybinding.lua",
    "hypr/conf/windowrule.lua",
    "hypr/conf/nacre.lua",
    "hypr/conf/brain.lua",
    "nacre/shell-tools/shortcuts.lua",
)
METHODS = {
    "application.launch": "exec_cmd",
    "compositor.reload": "exec_cmd",
    "workspace.switch": "focus",
    "window.focus-direction": "focus",
    "window.move": "window.move",
    "window.fullscreen": "window.fullscreen",
    "window.floating": "window.float",
    "window.pin": "window.pin",
    "window.pseudo": "window.pseudo",
    "window.resize": "window.resize",
    "window.cycle": "window.cycle_next",
    "group.toggle": "group.toggle",
    "group.previous": "group.prev",
    "group.next": "group.next",
    "layout.toggle-split": "layout",
    "workspace.scratchpad": "workspace.toggle_special",
    "keymap.switch": "submap",
    "shell.shortcut": "global",
}
PARAMETERS = {
    "application.launch": {"command"},
    "workspace.switch": {"workspace", "current_monitor"},
    "window.focus-direction": {"direction"},
    "window.move": {"workspace", "direction"},
    "window.fullscreen": {"mode"},
    "window.floating": {"action"},
    "window.resize": {"x", "y", "relative"},
    "window.cycle": {"next"},
    "workspace.scratchpad": {"name"},
    "keymap.switch": {"name"},
    "shell.shortcut": {"name"},
}
EFFECTS = {
    "floating": "float",
    "centered": "center",
    "pinned": "pin",
    "size": "size",
    "workspace": "workspace",
    "corner_radius": "rounding",
}
SELECTORS = {
    "app_id_regex": "class",
    "title_regex": "title",
    "namespace_regex": "namespace",
}
FLAGS = {
    "description",
    "locked",
    "repeating",
    "mouse",
    "non_consuming",
    "release",
    "ignore_mods",
}
EXTENSIONS = {
    "cursor.session": (
        "compositor_backends/hyprland/session-cursor.lua",
        "apply_session_cursor",
    ),
    "panel.drag": ("compositor_backends/hyprland/panel-drag.lua", "nacre_panel_drag"),
}

CALLBACKS = {key: (key, function) for key, (_, function) in EXTENSIONS.items()}
CALLBACKS["panel.drag.finish"] = ("panel.drag", "nacre_panel_drag_finish")


def fields(value, allowed, required=()):
    if (
        not isinstance(value, dict)
        or set(value) - set(allowed)
        or set(required) - set(value)
    ):
        raise ValueError("Invalid compositor declaration fields")


def lua_value(value):
    """Serialize data, never executable source; strings cannot inject Lua."""
    if value is None:
        return "nil"
    if type(value) is bool:
        return "true" if value else "false"
    if type(value) in (int, float):
        return json.dumps(value, allow_nan=False)
    if isinstance(value, str):
        # Lua and JSON share quoted strings except JSON's Unicode escapes. Keep
        # Unicode literals, and encode other control bytes as Lua decimal escapes.
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
    if isinstance(value, list):
        return "{" + ",".join(lua_value(item) for item in value) + "}"
    if isinstance(value, dict):
        if any(not isinstance(key, str) for key in value):
            raise ValueError("Invalid Lua table field")
        reserved = {
            "and",
            "break",
            "do",
            "else",
            "elseif",
            "end",
            "false",
            "for",
            "function",
            "goto",
            "if",
            "in",
            "local",
            "nil",
            "not",
            "or",
            "repeat",
            "return",
            "then",
            "true",
            "until",
            "while",
        }

        def field(key):
            return (
                key
                if re.fullmatch(r"[A-Za-z_][A-Za-z_0-9]*", key) and key not in reserved
                else "[" + lua_value(key) + "]"
            )

        return (
            "{"
            + ",".join(
                field(key) + "=" + lua_value(item) for key, item in value.items()
            )
            + "}"
        )
    raise ValueError("Unsupported compositor declaration value")


def workspace_target(value):
    fields(value, ("kind", "number", "scope", "offset", "name"), ("kind",))
    if value["kind"] == "number" and set(value) == {"kind", "number"}:
        if type(value["number"]) is int and value["number"] > 0:
            return value["number"]
    if value["kind"] == "scratchpad" and set(value) == {"kind", "name"}:
        if isinstance(value["name"], str):
            return "special" + (":" + value["name"] if value["name"] else "")
    if value["kind"] == "relative" and set(value) == {"kind", "scope", "offset"}:
        prefixes = {"monitor": "m", "existing": "e", "global": "r"}
        if value["scope"] in prefixes and type(value["offset"]) is int:
            return prefixes[value["scope"]] + f"{value['offset']:+d}"
    raise ValueError("Invalid workspace target")


def action_lua(row):
    action, params = row["action"], row["parameters"]
    if not isinstance(action, str):
        raise ValueError("Invalid compositor action")
    fields(params, PARAMETERS.get(action, ()))
    required = {
        "workspace.switch": {"workspace"},
        "window.focus-direction": {"direction"},
        "window.fullscreen": {"mode"},
        "window.floating": {"action"},
        "window.cycle": {"next"},
    }.get(action, set())
    if required - set(params):
        raise ValueError("Missing compositor action parameters")
    if action == "window.move" and set(params) not in ({"workspace"}, {"direction"}):
        raise ValueError("Window move requires one workspace or direction")
    if "direction" in params and params["direction"] not in (
        "left",
        "right",
        "up",
        "down",
    ):
        raise ValueError("Invalid window direction")
    if "mode" in params and params["mode"] not in ("fullscreen", "maximized"):
        raise ValueError("Invalid fullscreen mode")
    for key in ("current_monitor", "relative", "next"):
        if key in params and type(params[key]) is not bool:
            raise ValueError("Invalid action boolean")
    if action == "window.resize" and params:
        if set(params) != {"x", "y", "relative"} or any(
            type(params[key]) is not int for key in ("x", "y")
        ):
            raise ValueError("Invalid window resize dimensions")
    if "action" in params and not isinstance(params["action"], str):
        raise ValueError("Invalid floating action")
    if action in CALLBACKS:
        if params:
            raise ValueError("Backend callback accepts no declarative parameters")
        return CALLBACKS[action][1]
    if action not in METHODS:
        raise ValueError("Unknown compositor action: " + action)
    scalar = {
        "application.launch": "command",
        "workspace.scratchpad": "name",
        "keymap.switch": "name",
        "shell.shortcut": "name",
    }.get(action)
    if scalar:
        if set(params) != {scalar} or not isinstance(params[scalar], str):
            raise ValueError("Missing action parameter: " + scalar)
        argument = lua_value(params[scalar])
    elif action == "compositor.reload":
        argument = lua_value("hyprctl reload")
    elif action == "layout.toggle-split":
        argument = lua_value("togglesplit")
    else:
        converted = dict(params)
        if "workspace" in converted:
            converted["workspace"] = workspace_target(converted["workspace"])
        if "current_monitor" in converted:
            converted["on_current_monitor"] = converted.pop("current_monitor")
        argument = lua_value(converted) if converted else ""
    return "hl.dsp." + METHODS[action] + "(" + argument + ")"


def rule_lua(row):
    fields(
        row,
        ("kind", "id", "selector", "effects", "backend"),
        ("kind", "selector", "effects"),
    )
    fields(row["selector"], SELECTORS)
    fields(row["effects"], EFFECTS)
    if any(not isinstance(value, str) for value in row["selector"].values()):
        raise ValueError("Rule selectors must be regex strings")
    if "id" in row and not isinstance(row["id"], str):
        raise ValueError("Rule identity must be a string")
    for key in ("floating", "centered", "pinned"):
        if key in row["effects"] and type(row["effects"][key]) is not bool:
            raise ValueError("Invalid rule boolean")
    if "corner_radius" in row["effects"]:
        radius = row["effects"]["corner_radius"]
        if type(radius) is not int or radius < 0:
            raise ValueError("Corner radius must be a nonnegative integer")
    rule = {}
    if "id" in row:
        rule["name"] = row["id"]
    rule["match"] = {SELECTORS[key]: value for key, value in row["selector"].items()}
    for key, value in row["effects"].items():
        if key == "workspace":
            fields(value, ("target", "activate"), ("target", "activate"))
            if type(value["activate"]) is not bool:
                raise ValueError("Invalid workspace rule activation")
            value = str(workspace_target(value["target"])) + (
                "" if value["activate"] else " silent"
            )
        rule[EFFECTS[key]] = value
    backend = row.get("backend", {})
    fields(backend, ("hyprland",))
    if "hyprland" in backend:
        fields(
            backend["hyprland"],
            (
                "blur",
                "ignore_alpha",
                "no_anim",
                "fullscreen_state",
                "suppress_event",
                "no_initial_focus",
            ),
        )
        rule.update(backend["hyprland"])
    method = {"window-rule": "window_rule", "layer-rule": "layer_rule"}.get(row["kind"])
    if not method:
        raise ValueError("Unknown rule kind")
    return "hl." + method + "(" + lua_value(rule) + ")"


def render(contract, directory=DIRECTORY):
    fields(
        contract,
        ("version", "extensions", "outputs"),
        ("version", "extensions", "outputs"),
    )
    if contract["version"] != 1:
        raise ValueError("Unsupported compositor contract version")
    fields(contract["outputs"], OUTPUTS, OUTPUTS)
    fields(contract["extensions"], EXTENSIONS, EXTENSIONS)
    for key, (path, function) in EXTENSIONS.items():
        extension = contract["extensions"][key]
        fields(
            extension,
            ("backend", "source", "function", "purpose"),
            ("backend", "source", "function", "purpose"),
        )
        if (
            extension["backend"] != "hyprland"
            or extension["source"] != path
            or extension["function"] != function
        ):
            raise ValueError("Unexpected backend extension")
    outputs = {}
    for path, entries in contract["outputs"].items():
        if not isinstance(entries, list):
            raise ValueError("Expected ordered compositor declarations")
        lines = [
            "-- Generated by tools/generate_compositor_config.py; edit compositor-contract.json."
        ]
        keymap = ""
        extensions = set()
        for row in entries:
            if not isinstance(row, dict) or "kind" not in row:
                raise ValueError("Invalid compositor declaration")
            next_map = row.get("keymap", "")
            if next_map != keymap:
                if keymap:
                    lines.append("end)")
                if next_map:
                    if not isinstance(next_map, str):
                        raise ValueError("Invalid keymap name")
                    lines.append(
                        "hl.define_submap(" + lua_value(next_map) + ", function()"
                    )
                keymap = next_map
            kind = row["kind"]
            if kind == "binding":
                fields(
                    row,
                    ("kind", "keys", "action", "parameters", "options", "keymap"),
                    ("keys", "action", "parameters", "options"),
                )
                fields(row["options"], FLAGS)
                for flag, value in row["options"].items():
                    if (flag == "description" and not isinstance(value, str)) or (
                        flag != "description" and type(value) is not bool
                    ):
                        raise ValueError("Invalid binding option type")
                if not isinstance(row["keys"], str) or not row["keys"]:
                    raise ValueError("Invalid binding keys")
                if (
                    row["action"] in CALLBACKS
                    and CALLBACKS[row["action"]][0] not in extensions
                ):
                    raise ValueError(
                        "Callback binding requires its backend extension first"
                    )
                lines.append(
                    "hl.bind("
                    + lua_value(row["keys"])
                    + ", "
                    + action_lua(row)
                    + ", "
                    + lua_value(row["options"])
                    + ")"
                )
            elif kind in ("window-rule", "layer-rule"):
                lines.append(rule_lua(row))
            elif kind == "event":
                fields(row, ("kind", "event", "callback"), ("event", "callback"))
                if (
                    row["event"] != "session.ready"
                    or row["callback"] != "cursor.session"
                    or row["callback"] not in extensions
                ):
                    raise ValueError("Unsupported compositor event/callback")
                lines.append(
                    'hl.on("hyprland.start", ' + EXTENSIONS[row["callback"]][1] + ")"
                )
            elif kind == "unbind":
                fields(row, ("kind", "keys"), ("keys",))
                lines.append("hl.unbind(" + lua_value(row["keys"]) + ")")
            elif kind == "backend-config":
                fields(row, ("kind", "backend", "value"), ("backend", "value"))
                if row["backend"] != "hyprland":
                    raise ValueError("Unsupported configuration backend")
                lines.append("hl.config(" + lua_value(row["value"]) + ")")
            elif kind == "extension":
                fields(row, ("kind", "id"), ("id",))
                if (
                    not isinstance(row["id"], str)
                    or row["id"] not in EXTENSIONS
                    or row["id"] in extensions
                ):
                    raise ValueError("Invalid/repeated backend extension")
                extensions.add(row["id"])
                lines.append(
                    (directory / EXTENSIONS[row["id"]][0]).read_text().rstrip()
                )
            else:
                raise ValueError("Unknown compositor declaration kind")
        if keymap:
            lines.append("end)")
        outputs[path] = "\n".join(lines) + "\n"
    return outputs
