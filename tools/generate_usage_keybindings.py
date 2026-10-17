#!/usr/bin/env python3
"""Keep the user shortcut tables in sync with the compositor contract."""

import argparse
from collections import OrderedDict
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
from nacre_native_config import load, niri, render  # noqa: E402

START = "<!-- BEGIN GENERATED KEYBINDINGS -->"
END = "<!-- END GENERATED KEYBINDINGS -->"

LABELS = {
    "mute": "Mute audio",
    "volume-down": "Lower volume",
    "volume-up": "Raise volume",
    "mic-mute": "Mute microphone",
    "lock": "Lock the session",
    "brightness-down": "Lower display brightness",
    "brightness-up": "Raise display brightness",
    "keyboard-down": "Lower keyboard backlight",
    "keyboard-up": "Raise keyboard backlight",
    "emoji": "Emoji picker",
    "screenshot": "Capture an area to the clipboard",
    "save-screenshot": "Save a desktop screenshot",
    "hide": "Hide the window",
    "unhide": "Restore the last hidden window",
    "launcher": "Universal launcher",
    "overview": "Window overview",
    "clipboard": "Clipboard history",
    "wallpaper": "Wallpaper picker",
    "settings": "Nacre Settings",
    "power": "Power menu",
    "terminal": "Default terminal",
    "browser": "Default browser",
    "files": "Default file manager",
    "palette": "Command palette",
    "shortcuts": "Shortcut list",
    "ai-sidebar": "Optional AI sidebar",
    "detach-panel": "Detach the open panel",
    "put-away": "Put away the window; close after 30 seconds (opt-in)",
    "restore": "Undo the last put-away within 30 seconds (opt-in)",
    "restore-all": "Restore all windows still put away (opt-in)",
    "force-close": "Force close the window (opt-in)",
}
COMMAND_LABELS = {
    "window-minimize.sh restore-current": "Restore hidden windows on this workspace",
    "nacre-shell hide": "Hide or show the shell frame and panels",
    "nacre-shell toggle": "Toggle the main panel",
    "nacrectl ui nacre hide": "Hide or show the shell frame and panels",
    "nacrectl ui drawers toggle dashboard": "Toggle the main panel",
    "nacre-menu actions.capture": "Capture menu",
    "nacrectl ui nacre menu actions.capture ''": "Capture menu",
    "nacre-brain open": "Optional Brain",
    "nacre-brain capture": "Capture a note in optional Brain",
}
KEY_NAMES = {
    "super": "Super",
    "ctrl": "Ctrl",
    "alt": "Alt",
    "shift": "Shift",
    "return": "Enter",
    "escape": "Escape",
    "space": "Space",
    "tab": "Tab",
    "period": ".",
    "page_up": "Page Up",
    "page_down": "Page Down",
    "print": "Print Screen",
    "mouse:272": "left mouse button",
    "mouse:273": "right mouse button",
    "mouse_down": "scroll down",
    "mouse_up": "scroll up",
    "xf86audiomute": "Audio mute key",
    "xf86audiolowervolume": "Volume down key",
    "xf86audioraisevolume": "Volume up key",
    "xf86audiomicmute": "Microphone mute key",
    "xf86poweroff": "Power key",
    "xf86monbrightnessdown": "Brightness down key",
    "xf86monbrightnessup": "Brightness up key",
    "xf86kbdbrightnessdown": "Keyboard backlight down key",
    "xf86kbdbrightnessup": "Keyboard backlight up key",
}


def label(row, backend):
    action, params = row["action"], row.get("parameters", {})
    if action == "application.launch":
        description = (
            row.get("options", {}).get("description", "").removeprefix("Nacre:")
        )
        if backend == "niri" and description in ("screenshot", "save-screenshot"):
            return {
                "screenshot": "Open niri's screenshot picker",
                "save-screenshot": "Save a screenshot of the focused output",
            }[description]
        if description in LABELS:
            return LABELS[description]
        for suffix, text in COMMAND_LABELS.items():
            if params["command"].endswith(suffix):
                return text
    elif action == "window.close":
        return "Close the window"
    elif action == "window.fullscreen":
        return {
            "fullscreen": "Toggle fullscreen",
            "maximized": "Toggle maximization",
        }[params["mode"]]
    elif action == "window.floating":
        return "Toggle floating"
    elif action == "window.pin":
        return "Keep the window on all workspaces"
    elif action == "window.pseudo":
        return "Toggle pseudo tiling"
    elif action == "layout.toggle-split":
        return "Toggle split direction"
    elif action.startswith("group."):
        noun = "column tabs" if backend == "niri" else "window group"
        return {
            "group.toggle": "Toggle " + noun,
            "group.previous": "Previous window in " + noun,
            "group.next": "Next window in " + noun,
        }[action]
    elif action == "window.cycle":
        return "Cycle windows " + ("forward" if params["next"] else "backward")
    elif action == "workspace.scratchpad":
        return "Toggle scratchpad"
    elif action in ("workspace.switch", "window.move") and "workspace" in params:
        target = params["workspace"]
        prefix = "Switch to " if action == "workspace.switch" else "Move the window to "
        if target["kind"] == "number":
            return prefix + "workspace " + str(target["number"])
        if target["kind"] == "relative":
            return (
                prefix + ("next" if target["offset"] > 0 else "previous") + " workspace"
            )
        if target["kind"] == "scratchpad":
            return prefix + "scratchpad"
    elif action in ("window.focus-direction", "window.move"):
        return (
            "Focus " if action == "window.focus-direction" else "Move the window "
        ) + params["direction"]
    elif action == "window.resize":
        if not params:
            return "Resize the window by dragging"
        axis = "width" if params["x"] else "height"
        value = params["x"] or params["y"]
        return ("Increase " if value > 0 else "Decrease ") + axis
    elif action == "keymap.switch":
        return "Enter resize mode" if params["name"] == "resize" else "Exit resize mode"
    elif action == "compositor.reload":
        return "Reload compositor configuration"
    elif action == "shell.shortcut":
        return "Dismiss hover panels (also passes Escape to the app)"
    elif action == "panel.drag":
        return "Move a panel or window by dragging"
    elif action == "panel.drag.finish":
        return "Finish dragging a panel or window"
    raise ValueError("Add a user-facing label for binding: " + row["keys"])


def key_label(row):
    parts = [part.strip() for part in row["keys"].split("+")]
    text = " + ".join(KEY_NAMES.get(part.lower(), part.title()) for part in parts)
    if row.get("keymap"):
        text += " (resize mode)"
    if row.get("options", {}).get("release"):
        text += " (release)"
    return text


def table(rows, backend):
    grouped = OrderedDict()
    for row in rows:
        if backend == "niri":
            try:
                niri.binding_command(row)
                niri.keys(row["keys"])
            except NotImplementedError:
                continue
        grouped.setdefault(label(row, backend), []).append(key_label(row))
    for prefix, modifiers in (
        ("Switch to workspace ", "Super + "),
        ("Move the window to workspace ", "Super + Shift + "),
    ):
        numbered = {
            prefix + str(number): [modifiers + str(number % 10)]
            for number in range(1, 11)
        }
        if all(grouped.get(text) == keys for text, keys in numbered.items()):
            compact = OrderedDict()
            for text, keys in grouped.items():
                if text == prefix + "1":
                    compact[prefix + "1–10 (0 selects 10)"] = [
                        modifiers + "1–9 / " + modifiers + "0"
                    ]
                elif text not in numbered:
                    compact[text] = keys
            grouped = compact
    lines = ["| Keys | Action |", "| --- | --- |"]
    lines.extend(
        "| " + " / ".join(keys) + " | " + text + " |" for text, keys in grouped.items()
    )
    return "\n".join(lines)


def generate(contract):
    # Use the actual compilers for validation and backend support decisions.
    for backend in ("hyprland", "niri"):
        render(contract, backend=backend)
    rows = [
        row
        for entries in contract["outputs"].values()
        for row in entries
        if row["kind"] == "binding"
    ]
    personal = contract["profileBindings"]["windowTrashBindings"]
    return "\n\n".join(
        (
            START,
            "<!-- Generated by tools/generate_usage_keybindings.py; edit the contract, not these tables. -->",
            "## Hyprland shortcuts\n\n" + table(rows, "hyprland"),
            "## Niri shortcuts\n\n"
            "Number keys select existing workspaces on the focused output; their numbers can change.\n\n"
            + table(rows, "niri"),
            "## Personal trash shortcuts (opt-in, Hyprland only)\n\n"
            "Enable `windowTrashBindings` in your profile. These are absent from the default profile\n"
            "and unavailable on niri. Super + Q still closes immediately. After the put-away\n"
            "deadline, a window that resists closing can be forcibly terminated; save work first.\n\n"
            + table(personal, "hyprland"),
            END,
        )
    )


def replace_block(source, generated):
    if source.count(START) != 1 or source.count(END) != 1:
        raise ValueError(
            "usage.md must contain exactly one generated keybindings block"
        )
    before, rest = source.split(START)
    _, after = rest.split(END)
    return before + generated + after


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check", action="store_true", help="Fail on drift; write nothing"
    )
    args = parser.parse_args()
    path = ROOT / "docs/usage.md"
    source = path.read_text()
    expected = replace_block(source, generate(load()))
    if args.check and expected != source:
        parser.exit(
            1, "Shortcut docs differ; run python tools/generate_usage_keybindings.py\n"
        )
    if not args.check:
        path.write_text(expected)
    print(
        "Shortcut documentation matches the contract."
        if args.check
        else "Generated shortcut documentation."
    )


if __name__ == "__main__":
    main()
