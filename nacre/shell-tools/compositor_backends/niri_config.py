"""Deterministic niri KDL fragments from the shared semantic declarations.

Includes merge only the generated fields. Never write a user's config.kdl;
unsupported Hyprland-only semantics are recorded, not silently substituted.
"""

import json
import re
import shlex

OUTPUTS = ("niri/bindings.kdl", "niri/window-rules.kdl", "niri/unsupported.json")
SKIPPED_ACTIONS = {
    "window.pin",
    "window.pseudo",
    "group.toggle",
    "group.previous",
    "group.next",
    "layout.toggle-split",
    "workspace.scratchpad",
    "keymap.switch",
    "window.cycle",
    "panel.drag.begin",
    "panel.drag.finish",
}
SKIPPED_LAUNCHES = {
    "Nacre:put-away",
    "Nacre:restore",
    "Nacre:restore-all",
    "Nacre:force-close",
    "Nacre:hide",
    "Nacre:unhide",
}


def quoted(value):
    if not isinstance(value, str) or "\0" in value:
        raise ValueError("Invalid KDL string")
    escapes = {"\\": "\\\\", '"': '\\"', "\n": "\\n", "\r": "\\r", "\t": "\\t"}
    return (
        '"'
        + "".join(
            escapes.get(char, f"\\u{{{ord(char):x}}}" if ord(char) < 32 else char)
            for char in value
        )
        + '"'
    )


def keys(value):
    if not isinstance(value, str):
        raise ValueError("Invalid binding")
    pieces = [piece.strip() for piece in value.split("+")]
    aliases = {
        "SUPER": "Super",
        "CTRL": "Ctrl",
        "ALT": "Alt",
        "SHIFT": "Shift",
        "mouse_down": "WheelScrollDown",
        "mouse_up": "WheelScrollUp",
        "left": "Left",
        "right": "Right",
        "up": "Up",
        "down": "Down",
        "period": "period",
        "return": "Return",
        "escape": "Escape",
    }
    if any(not re.fullmatch(r"[A-Za-z0-9_]+", piece) for piece in pieces):
        raise ValueError("Invalid binding key")
    return "+".join(aliases.get(piece, piece) for piece in pieces)


def action_command(name, arguments=()):
    return (
        'exec env NACRE_COMPOSITOR=niri python3 "${NACRE_COMPOSITOR_TOOLS:-$HOME/.local/share/nacre/shell/tools}/compositor.py" action '
        + shlex.join([name, *map(str, arguments)])
    )


def workspace_args(value):
    if value.get("kind") == "number":
        number = value["number"]
        if type(number) is not int or not 1 <= number <= 255:
            raise ValueError("Invalid workspace index")
        return ["--workspace", str(number)]
    if value.get("kind") == "relative":
        offset = value["offset"]
        if type(offset) is not int or offset not in (-1, 1):
            raise ValueError("Invalid relative workspace step")
        return ["--workspace", json.dumps({"relative": offset})]
    if value.get("kind") == "name":
        return ["--workspace", value["name"]]
    raise NotImplementedError("Niri has no Hyprland special/scratchpad workspaces")


def binding_command(row):
    action, params = row["action"], row.get("parameters", {})
    if row.get("options", {}).get("mouse"):
        raise NotImplementedError(
            "Keep niri's native move/resize gestures; no pointer-hook substitution"
        )
    if action in SKIPPED_ACTIONS or row.get("keymap"):
        raise NotImplementedError("No equivalent niri capability for " + action)
    if row.get("options", {}).get("non_consuming"):
        raise NotImplementedError(
            "Niri bindings consume input; non-consuming hooks are unsupported"
        )
    if action == "application.launch":
        command = params["command"]
        description = row.get("options", {}).get("description", "")
        if description == "Nacre:close":
            return action_command("close_window")
        if description == "Nacre:overview":
            return "niri msg action toggle-overview"
        if description in SKIPPED_LAUNCHES or "/window-minimize.sh" in command:
            raise NotImplementedError(
                "Hyprland hide/trash/force-kill operation is not available"
            )
        if not isinstance(command, str):
            raise ValueError("Invalid launch command")
        return command
    if action == "shell.shortcut":
        return (
            'if [ -n "${NACRE_QS_CONFIG:-}" ]; then exec "${NACRE_QS_BIN:-$HOME/.local/share/nacre/shell/bin/qs}" -p "$NACRE_QS_CONFIG" ipc call nacreCompositor shortcut '
            + shlex.quote(params["name"])
            + '; else exec "${NACRE_QS_BIN:-$HOME/.local/share/nacre/shell/bin/qs}" -c nacre ipc call nacreCompositor shortcut '
            + shlex.quote(params["name"])
            + "; fi"
        )
    if action == "compositor.reload":
        return action_command("reload")
    if action == "workspace.switch":
        return action_command("switch_workspace", workspace_args(params["workspace"]))
    if action == "window.move":
        if "workspace" in params:
            return action_command("move_window", workspace_args(params["workspace"]))
        return action_command("move_direction", ["--direction", params["direction"]])
    if action == "window.focus-direction":
        return action_command("focus_direction", ["--direction", params["direction"]])
    if action == "window.fullscreen":
        return action_command("fullscreen_window", ["--mode", params["mode"]])
    if action == "window.floating":
        return action_command("float_window", ["--action", params["action"]])
    if action == "window.resize":
        return action_command(
            "resize_window",
            [
                "--width",
                params["x"],
                "--height",
                params["y"],
                "--relative",
                "true" if params.get("relative") else "false",
            ],
        )
    raise NotImplementedError("No equivalent niri action for " + action)


def rule_kdl(row):
    selector = row.get("selector", {})
    if set(selector) - {"app_id_regex", "title_regex"}:
        raise NotImplementedError("No matching window selector on niri")
    effects = row.get("effects", {})
    if set(effects) - {
        "floating",
        "centered",
        "pinned",
        "size",
        "workspace",
        "corner_radius",
    }:
        raise ValueError("Unknown window rule effect")
    parts = ["window-rule {"]
    attributes = {"app_id_regex": "app-id", "title_regex": "title"}
    if selector:
        parts.append(
            "    match "
            + " ".join(
                attributes[key] + "=" + quoted(value) for key, value in selector.items()
            )
        )
    if "floating" in effects:
        if type(effects["floating"]) is not bool:
            raise ValueError("Invalid floating rule")
        parts.append("    open-floating " + str(effects["floating"]).lower())
    if "corner_radius" in effects:
        radius = effects["corner_radius"]
        if type(radius) is not int or radius < 0:
            raise ValueError("Corner radius must be a nonnegative integer")
        parts.append("    geometry-corner-radius " + str(radius))
        parts.append("    clip-to-geometry true")
    if "workspace" in effects:
        target = effects["workspace"]["target"]
        if target.get("kind") != "name":
            raise NotImplementedError(
                "Niri initial window routes require a named workspace, not a changing index"
            )
        parts.append("    open-on-workspace " + quoted(target["name"]))
    # Pinned, centered and expression/percentage sizing have no exact semantic
    # match. Floating placement remains caller-owned via measured IPC geometry.
    parts.append("}")
    return "\n".join(parts)


def render(contract, directory=None):
    if (
        not isinstance(contract, dict)
        or contract.get("version") != 1
        or not isinstance(contract.get("outputs"), dict)
    ):
        raise ValueError("Invalid compositor contract")
    binds = []
    rules = []
    unsupported = []
    seen = set()
    for source, rows in contract["outputs"].items():
        if not isinstance(rows, list):
            raise ValueError("Invalid compositor declarations")
        for row in rows:
            if not isinstance(row, dict) or "kind" not in row:
                raise ValueError("Invalid compositor declaration")
            kind = row["kind"]
            try:
                if kind == "binding":
                    command = binding_command(row)
                    key = keys(row["keys"])
                    if key in seen:
                        raise ValueError("Repeated niri binding: " + key)
                    seen.add(key)
                    options = row.get("options", {})
                    flags = ["repeat=" + str(bool(options.get("repeating"))).lower()]
                    body = "spawn-sh " + quoted(command)
                    direction = row.get("parameters", {}).get("direction")
                    if row.get("action") == "window.focus-direction" and direction in (
                        "left",
                        "right",
                    ):
                        body = "focus-column-" + direction
                        flags.append("cooldown-ms=120")
                    if options.get("locked"):
                        flags.append("allow-when-locked=true")
                    if options.get("description"):
                        flags.append(
                            "hotkey-overlay-title=" + quoted(options["description"])
                        )
                    binds.append(
                        "    " + key + " " + " ".join(flags) + " { " + body + "; }"
                    )
                elif kind == "window-rule":
                    rules.append(rule_kdl(row))
                    differing = sorted(
                        set(row.get("effects", {})) & {"centered", "pinned", "size"}
                    )
                    if differing:
                        unsupported.append(
                            {
                                "source": source,
                                "kind": kind,
                                "id": row.get("id", ""),
                                "reason": "Unmapped window effects: "
                                + ", ".join(differing),
                            }
                        )
                elif kind in (
                    "extension",
                    "layer-rule",
                    "backend-config",
                    "unbind",
                    "event",
                ):
                    raise NotImplementedError(
                        "Backend-specific " + kind + " is not emitted for niri"
                    )
                else:
                    raise ValueError("Unknown compositor declaration kind")
            except NotImplementedError as error:
                unsupported.append(
                    {
                        "source": source,
                        "kind": kind,
                        "id": row.get("id", row.get("keys", "")),
                        "reason": str(error),
                    }
                )
    header = "// Generated from compositor-contract.json; edit the neutral source.\n// Include before user bindings/rules so user-owned choices take precedence.\n"
    return {
        OUTPUTS[0]: header + "binds {\n" + "\n".join(binds) + "\n}\n",
        OUTPUTS[1]: header + "\n\n".join(rules) + "\n",
        OUTPUTS[2]: json.dumps(
            {"backend": "niri", "unsupported": unsupported},
            indent=2,
            ensure_ascii=False,
        )
        + "\n",
    }
