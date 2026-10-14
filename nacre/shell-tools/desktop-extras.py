#!/usr/bin/env python3
"""Private, on-demand data adapters for Nacre desktop panels."""

import argparse
from nacre_i18n import tr
import hashlib
import json
import os
import re
import runpy
import subprocess
import sys
import unicodedata
import time
import uuid
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

import compositor
import welcome

HOME = Path.home()
STATE = HOME / ".local/state/nacre/shell/extras"


def clean(text):
    return "".join(
        c for c in str(text) if not unicodedata.category(c).startswith("C")
    ).strip()


def run(args, **kw):
    output = (
        dict(stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if args[0] == "wl-copy"
        else dict(capture_output=True)
    )
    return subprocess.run(
        args, check=True, timeout=kw.pop("timeout", 12), **output, **kw
    )


def save(path, value):
    from importlib.util import spec_from_file_location, module_from_spec

    spec = spec_from_file_location(
        "palette", Path(__file__).with_name("classic-state.py")
    )
    module = module_from_spec(spec)
    spec.loader.exec_module(module)
    module.atomic_write(path, json.dumps(value))


def launch(command):
    return subprocess.Popen(
        [
            "systemd-run",
            "--user",
            "--scope",
            "--collect",
            "--quiet",
            "env",
            "-u",
            "LD_LIBRARY_PATH",
            "-u",
            "QML_IMPORT_PATH",
            "-u",
            "QML2_IMPORT_PATH",
            "-u",
            "QT_PLUGIN_PATH",
            "-u",
            "NACRE_LIB_DIR",
            *command,
        ],
        start_new_session=True,
        stdout=subprocess.DEVNULL,
        stderr=subprocess.DEVNULL,
    )


def ai():
    return runpy.run_path(str((HOME / ".local/bin/nacre-ai").resolve()))


def chat_list():
    workflow = ai()
    records = []
    sources = [p for p in workflow["chat_projects"]() if not p.get("host")]

    def fetch(p):
        items = []
        for method in ("source_chats", "claude_chats"):
            try:
                items.extend(workflow[method](dict(p, task_sources=[]), None))
            except (ValueError, OSError, subprocess.SubprocessError):
                pass
        return items

    with ThreadPoolExecutor(max_workers=3) as pool:
        for batch in pool.map(fetch, sources):
            records.extend(batch)
    records.sort(key=lambda r: r.get("updated_at", 0), reverse=True)
    cached = {}
    for item in records[:30]:
        key = hashlib.sha256(
            json.dumps(
                [item.get(k) for k in ("id", "agent", "account", "cwd")]
            ).encode()
        ).hexdigest()[:24]
        cached[key] = item
    save(STATE / "chats.json", cached)
    return [
        dict(
            key=k,
            id=v["id"],
            title=clean(v["title"])[:100],
            agent=v.get("agent", "codex"),
            account=v.get("account") or "Default",
            state=v.get("state", "saved"),
        )
        for k, v in cached.items()
    ]


def resume(key, terminal=False):
    records = json.loads((STATE / "chats.json").read_text())
    item = records[key]
    if not terminal:
        for client in compositor.query("windows", timeout=12):
            if (
                client["class"] == "nacre-ai-task"
                and json.loads(
                    run(
                        [
                            os.environ.get("NACRE_HELPER_BINARY")
                            or str(Path(__file__).with_name("nacre-helper")),
                            "window-chat-title",
                            str(client["pid"]),
                        ]
                    ).stdout
                ).get("threadId")
                == item["id"]
            ):
                compositor.focus_window(client["address"], timeout=12)
                return
        launch(
            [
                "kitty",
                "--class",
                "nacre-ai-task",
                "--title",
                clean(item["title"]),
                "--",
                sys.executable,
                __file__,
                "resume-terminal",
                key,
            ]
        )
        return
    workflow = ai()
    project = dict(item["project"], path=item.get("cwd") or item["project"]["path"])
    account = item.get("account", "")
    if item.get("agent") == "claude":
        workflow["launch_claude"](project, "resume", account, item["id"])
    else:
        env = workflow["local_environment"](account)
        executable = str(HOME / ".local/bin/codex")
        os.execvpe(
            executable,
            [
                executable,
                "resume",
                item["id"],
                "--sandbox",
                "danger-full-access",
                "--ask-for-approval",
                "never",
                "-C",
                project["path"],
                "--add-dir",
                str(HOME / "Documents/Nacre-Brain"),
            ],
            env,
        )


def brain_search(query):
    vault = Path(
        os.environ.get("NACRE_BRAIN", str(HOME / "Documents/Nacre-Brain"))
    ).resolve()
    words = [w.casefold() for w in query.split() if w]
    if not words:
        return []
    found = []
    for path in vault.rglob("*.md"):
        if (
            any(p.startswith(".") for p in path.relative_to(vault).parts)
            or path.is_symlink()
        ):
            continue
        text = path.read_text(errors="replace")
        fold = text.casefold()
        if not all(w in fold for w in words):
            continue
        title = next(
            (line[2:] for line in text.splitlines() if line.startswith("# ")), path.stem
        )
        excerpt = next(
            (
                line
                for line in text.splitlines()
                if any(w in line.casefold() for w in words) and not line.startswith("#")
            ),
            title,
        )
        found.append(
            dict(
                path=str(path.relative_to(vault)),
                title=clean(title)[:120],
                preview=clean(excerpt)[:180],
                score=sum(fold.count(w) for w in words),
                mtime=path.stat().st_mtime,
            )
        )
    found.sort(key=lambda r: (r["score"], r["mtime"]), reverse=True)
    return found[:25]


def open_note(path):
    vault = Path(
        os.environ.get("NACRE_BRAIN", str(HOME / "Documents/Nacre-Brain"))
    ).resolve()
    file = (vault / path).resolve()
    if not file.is_relative_to(vault) or file.suffix != ".md" or not file.is_file():
        raise ValueError(tr("helper.desktop-extras.invalid-note", "Invalid note"))
    launch(["xdg-open", str(file)])


def mime(data):
    if data.startswith(b"\x89PNG"):
        return "image/png"
    if data.startswith(b"\xff\xd8"):
        return "image/jpeg"
    if data.startswith((b"GIF87a", b"GIF89a")):
        return "image/gif"
    if data.startswith(b"RIFF") and data[8:12] == b"WEBP":
        return "image/webp"
    return tr(
        "helper.desktop-extras.text-plain-charset-utf-8", "text/plain;charset=utf-8"
    )


def label_digest(row):
    return hashlib.sha256(row.partition("\t")[2].encode()).hexdigest()


def fingerprint(row, data=None):
    if data is None:
        data = run(["cliphist", "decode"], input=row.encode()).stdout
    return dict(label=label_digest(row), content=hashlib.sha256(data).hexdigest())


def pins_for(rows):
    path = STATE / "pins.json"
    original = json.loads(path.read_text()) if path.exists() else []
    pins = []
    for pin in original:
        if isinstance(pin, dict):
            pins.append(pin)
        else:
            row = next((r for r in rows if r.startswith(str(pin) + "\t")), None)
            if row:
                pins.append(fingerprint(row))
    if pins != original:
        save(path, pins)
    return pins


def clips():
    rows = run(["cliphist", "list"]).stdout.decode(errors="replace").splitlines()
    pins = pins_for(rows)
    pin_labels = {p["label"] for p in pins}
    recent = rows[:160]
    known = {r.split("\t", 1)[0] for r in recent}
    recent.extend(
        r
        for r in rows
        if label_digest(r) in pin_labels and r.split("\t", 1)[0] not in known
    )
    recent.sort(key=lambda r: label_digest(r) not in pin_labels)
    result = []
    images = 0
    for row in recent:
        ident, sep, label = row.partition("\t")
        if not sep or not ident.isdigit():
            continue
        image = "binary data" in label and any(
            x in label.lower() for x in ("png", "jpg", "jpeg", "gif", "webp")
        )
        thumb = ""
        data = None
        pinned = False
        if label_digest(row) in pin_labels:
            data = run(["cliphist", "decode"], input=row.encode()).stdout
            pinned = fingerprint(row, data) in pins
        if image and images < 16:
            images += 1
            if data is None:
                data = run(["cliphist", "decode"], input=row.encode()).stdout
            if len(data) < 16 * 1024 * 1024 and mime(data).startswith("image/"):
                path = STATE / "images" / (hashlib.sha256(data).hexdigest() + ".img")
                path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
                if not path.exists():
                    path.write_bytes(data)
                    path.chmod(0o600)
                thumb = str(path)
        result.append(
            dict(
                id=ident,
                title=clean(label)[:200],
                image=image,
                thumbnail=thumb,
                pinned=pinned,
            )
        )
    live = {Path(r["thumbnail"]).name for r in result if r["thumbnail"]}
    folder = STATE / "images"
    if folder.exists():
        for p in folder.glob("*.img"):
            if p.name not in live:
                p.unlink()
    return sorted(result, key=lambda r: not r["pinned"])


def clip_action(action, ident):
    if not re.fullmatch(r"\d+", ident):
        raise ValueError(
            tr(
                "helper.desktop-extras.invalid-clipboard-entry",
                "Invalid clipboard entry",
            )
        )
    rows = run(["cliphist", "list"]).stdout.decode(errors="replace").splitlines()
    row = next((r for r in rows if r.startswith(ident + "\t")), None)
    if row is None:
        raise ValueError(
            tr(
                "helper.desktop-extras.clipboard-entry-no-longer-exists",
                "Clipboard entry no longer exists",
            )
        )
    if action == "copy":
        data = run(["cliphist", "decode"], input=row.encode()).stdout
        run(["wl-copy", "--type", mime(data)], input=data)
    elif action == "pin":
        pins = pins_for(rows)
        pin = fingerprint(row)
        save(
            STATE / "pins.json",
            [p for p in pins if p != pin] if pin in pins else [pin, *pins],
        )
    elif action == "delete":
        run(["cliphist", "delete"], input=row.encode())


DESCRIPTIONS = {
    "SUPER+A": "AI sidebar",
    "SUPER+SPACE": "Applications",
    "SUPER+CTRL+SPACE": "Command palette",
    "SUPER+V": "Clipboard history",
    "SUPER+K": "Keyboard shortcuts",
    "SUPER+I": "Desktop settings",
    "SUPER+SHIFT+O": "Desktop settings",
    "SUPER+O": "Dashboard",
    "SUPER+W": "Wallpaper gallery",
    "SUPER+Z": "Hide/show desktop shell",
    "SUPER+B": "Open brain",
    "SUPER+N": "Capture a thought",
    "SUPER+X": "Power menu",
    "SUPER+ESCAPE": "Power menu",
    "SUPER+L": "Lock screen",
    "SUPER+R": "Resize mode",
    "SUPER+Q": "Close (undo with Super+Shift+Q within 30 seconds)",
    "SUPER+SHIFT+Q": "Restore last trashed window",
    "SUPER+CTRL+Q": "Close window",
    "SUPER+F": "Fullscreen",
    "SUPER+M": "Maximize",
    "SUPER+T": "Toggle floating",
    "SUPER+P": "Pin window",
    "SUPER+TAB": "Window overview",
    "SUPER+CTRL+right": "Next workspace",
    "SUPER+CTRL+left": "Previous workspace",
    "SUPER+RETURN": "Terminal",
    "SUPER+SHIFT+B": "Browser",
    "SUPER+SHIFT+S": "Screenshot selection to clipboard",
    "SUPER+E": "File manager",
    "SUPER+H": "Hide window",
    "SUPER+SHIFT+H": "Restore hidden window",
    "Print": "Screenshot selection to clipboard",
}


DESCRIPTION_IDS = {
    "SUPER+A": {"descriptionId": "shortcuts.description.ai-sidebar"},
    "SUPER+SPACE": {"descriptionId": "shortcuts.description.applications"},
    "SUPER+CTRL+SPACE": {"descriptionId": "shortcuts.description.command-palette"},
    "SUPER+V": {"descriptionId": "shortcuts.description.clipboard-history"},
    "SUPER+K": {"descriptionId": "shortcuts.description.keyboard-shortcuts"},
    "SUPER+I": {"descriptionId": "shortcuts.description.desktop-settings"},
    "SUPER+SHIFT+O": {"descriptionId": "shortcuts.description.desktop-settings"},
    "SUPER+O": {"descriptionId": "shortcuts.description.dashboard"},
    "SUPER+W": {"descriptionId": "shortcuts.description.wallpaper-gallery"},
    "SUPER+Z": {"descriptionId": "shortcuts.description.hide-show-desktop-shell"},
    "SUPER+B": {"descriptionId": "shortcuts.description.open-brain"},
    "SUPER+N": {"descriptionId": "shortcuts.description.capture-a-thought"},
    "SUPER+X": {"descriptionId": "shortcuts.description.power-menu"},
    "SUPER+ESCAPE": {"descriptionId": "shortcuts.description.power-menu"},
    "SUPER+L": {"descriptionId": "shortcuts.description.lock-screen"},
    "SUPER+R": {"descriptionId": "shortcuts.description.resize-mode"},
    "SUPER+Q": {"descriptionId": "shortcuts.description.close-with-undo"},
    "SUPER+SHIFT+Q": {
        "descriptionId": "shortcuts.description.restore-last-trashed-window"
    },
    "SUPER+CTRL+Q": {"descriptionId": "shortcuts.description.close-window"},
    "SUPER+F": {"descriptionId": "shortcuts.description.fullscreen"},
    "SUPER+M": {"descriptionId": "shortcuts.description.maximize"},
    "SUPER+T": {"descriptionId": "shortcuts.description.toggle-floating"},
    "SUPER+P": {"descriptionId": "shortcuts.description.pin-window"},
    "SUPER+TAB": {"descriptionId": "shortcuts.description.window-overview"},
    "SUPER+CTRL+right": {"descriptionId": "shortcuts.description.next-workspace"},
    "SUPER+CTRL+left": {"descriptionId": "shortcuts.description.previous-workspace"},
    "SUPER+RETURN": {"descriptionId": "shortcuts.description.terminal"},
    "SUPER+SHIFT+B": {"descriptionId": "shortcuts.description.browser"},
    "SUPER+SHIFT+S": {
        "descriptionId": "shortcuts.description.screenshot-selection-to-clipboard"
    },
    "SUPER+E": {"descriptionId": "shortcuts.description.file-manager"},
    "SUPER+H": {"descriptionId": "shortcuts.description.hide-window"},
    "SUPER+SHIFT+H": {"descriptionId": "shortcuts.description.restore-hidden-window"},
    "Print": {
        "descriptionId": "shortcuts.description.screenshot-selection-to-clipboard"
    },
}


def binding_description(key):
    row = DESCRIPTION_IDS.get(key)
    return (
        tr(row["descriptionId"], DESCRIPTIONS[key], {"shortcut": "Super+Shift+Q"})
        if row
        else None
    )


def bindings():
    source = {}
    source_routes = {}
    descriptions = {
        "Nacre:" + action: tr(welcome.SHORTCUT_IDS[action]["titleId"], title)
        for action, title, _ in welcome.SHORTCUTS
    }
    for file in compositor.binding_source_files(HOME):
        try:
            records = compositor.capture_bindings(file)
        except (RuntimeError, OSError, subprocess.TimeoutExpired):
            continue  # Native descriptions remain available for unsupported local code.
        for record in records:
            key = record["keys"].replace(" ", "").replace("space", "SPACE")
            action = record["action"]
            arguments = action.get("args", [])
            command = (
                arguments[0] if action["method"] == "exec_cmd" and arguments else ""
            )
            semantic = descriptions.get(record.get("flags", {}).get("description"))
            desc = (
                semantic
                or binding_description(key)
                or action["method"].replace(".", " ").replace("_", " ")
            )
            if command and key not in DESCRIPTIONS and not semantic:
                desc = Path(command.split()[0]).stem.replace("-", " ").replace("_", " ")
            if action["method"] == "submap" and arguments == ["reset"]:
                desc = tr("shortcuts.resize.exit", "Exit resize mode")
            if "wpctl" in command:
                desc = (
                    tr("shortcuts.audio.microphone-mute", "Toggle microphone mute")
                    if "@DEFAULT_AUDIO_SOURCE@" in command
                    else tr("shortcuts.audio.speaker-mute", "Toggle speaker mute")
                    if "set-mute" in command
                    else tr("shortcuts.audio.volume-down", "Volume down")
                    if "5%-" in command
                    else tr("shortcuts.audio.volume-up", "Volume up")
                )
            if (
                "nacre-shell brightness " in command
                or "nacre-shell keyboard-light " in command
            ):
                desc = (
                    tr(
                        "shortcuts.brightness.keyboard",
                        "Keyboard brightness {step}",
                        {"step": command.split()[-1]},
                    )
                    if "keyboard-light" in command
                    else tr(
                        "shortcuts.brightness.screen",
                        "Screen brightness {step}",
                        {"step": command.split()[-1]},
                    )
                )
            source[(record["submap"], key)] = desc
            source_routes[(record["submap"], key)] = menu_route(command)
    result = []
    for b in compositor.query("bindings", timeout=12):
        parts = [
            name
            for flag, name in [(64, "SUPER"), (4, "CTRL"), (8, "ALT"), (1, "SHIFT")]
            if b["modmask"] & flag
        ]
        key = b["key"]
        key = "SPACE" if key.lower() == "space" else key
        combo = "+".join([*parts, key])
        desc = (
            source.get((b.get("submap", ""), combo))
            or binding_description(combo)
            or b.get("description")
            or b.get("dispatcher", "")
        )
        if b.get("submap"):
            desc = b["submap"] + " · " + desc
        if key.isdigit() and b["modmask"] == 64:
            desc = tr(
                "shortcuts.workspace.go",
                "Go to workspace {workspace}",
                {"workspace": "10" if key == "0" else key},
            )
        if key.isdigit() and b["modmask"] == 65:
            desc = tr(
                "shortcuts.workspace.move",
                "Move window to workspace {workspace}",
                {"workspace": "10" if key == "0" else key},
            )
        result.append(
            dict(
                key=combo,
                description=desc,
                submap=b.get("submap", ""),
                menu=menu_route(b.get("arg", ""))
                or (
                    source_routes.get((b.get("submap", ""), combo), "")
                    if b.get("dispatcher") == "__lua"
                    else ""
                ),
            )
        )
    return sorted(result, key=lambda r: (not r["key"].startswith("SUPER"), r["key"]))


def menu_route(command):
    if not isinstance(command, str):
        return ""
    if command.startswith("grim -g "):
        return "actions.capture.area"
    if command.startswith("grim ~/Pictures/Screenshots/"):
        return "actions.capture.screen"
    direct = re.search(r"(?:^|/)nacre-menu\s+([a-z][a-z0-9.-]*)\s*$", command)
    if direct:
        return direct.group(1)
    routes = {
        "nacre-shell wallpaper": "style.picker",
        "nacre-shell clipboard": "actions.clipboard",
        "nacre-shell overview": "actions.windows",
        "nacre-shell keys": "learn.shortcuts",
        "nacre-settings": "setup.settings",
        "nacre-app emoji": "actions.emoji",
        "loginctl lock-session": "power.lock",
        "nacre-app terminal": "actions.terminal",
    }
    return next(
        (route for suffix, route in routes.items() if command.endswith(suffix)), ""
    )


def main():
    p = argparse.ArgumentParser()
    p.add_argument("action")
    p.add_argument("key", nargs="?")
    args = p.parse_args()
    STATE.mkdir(mode=0o700, parents=True, exist_ok=True)
    STATE.chmod(0o700)
    if args.action == "resume-terminal":
        return resume(args.key, True)
    payload = json.loads(sys.stdin.read() or "{}")
    result = None
    try:
        if args.action == "chats":
            result = chat_list()
        elif args.action == "resume":
            resume(payload["key"])
            result = True
        elif args.action == "brain":
            result = brain_search(payload.get("query", ""))
        elif args.action == "note":
            open_note(payload["path"])
            result = True
        elif args.action == "explore":
            vault = Path(
                os.environ.get("NACRE_BRAIN", str(HOME / "Documents/Nacre-Brain"))
            ).resolve()
            file = (vault / payload["path"]).resolve()
            if (
                not file.is_relative_to(vault)
                or not file.is_file()
                or file.suffix != ".md"
            ):
                raise ValueError(
                    tr("helper.desktop-extras.invalid-note", "Invalid note")
                )
            save(
                STATE / "brain-focus.json",
                dict(
                    path=str(file.relative_to(vault)),
                    token=uuid.uuid4().hex,
                    created=time.time(),
                ),
            )
            subprocess.Popen(
                ["nacre-shell", "brain"],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                start_new_session=True,
            )
            result = True
        elif args.action == "capture":
            text = payload["text"].strip()
            if not text or len(text) > 16000:
                raise ValueError(
                    tr(
                        "helper.desktop-extras.capture-must-contain-1-16000-characters",
                        "Capture must contain 1–16000 characters",
                    )
                )
            title = clean(text.splitlines()[0])[:80]
            run(
                [
                    "nacre-ai-memory",
                    "--title",
                    title,
                    "--source",
                    "User capture from desktop brain drawer",
                    "--confidence",
                    "reported",
                    "--world",
                    "Personal",
                    "--topic",
                    "Quick captures",
                ],
                input=text.encode(),
                timeout=20,
            )
            result = True
        elif args.action == "clips":
            result = clips()
        elif args.action in ("copy", "pin", "delete"):
            clip_action(args.action, payload["id"])
            result = True
        elif args.action == "keys":
            result = bindings()
        else:
            raise ValueError(
                tr(
                    "helper.desktop-extras.unknown-desktop-operation",
                    "Unknown desktop operation",
                )
            )
        print(
            json.dumps(
                dict(action=args.action, result=result, query=payload.get("query", ""))
            )
        )
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        RuntimeError,
        subprocess.SubprocessError,
    ):
        print(
            json.dumps(
                dict(
                    action=args.action,
                    error=tr(
                        "helper.desktop-extras.could-not-complete-value0-check-the-selected-item-or-try-again",
                        "Could not complete {value0}. Check the selected item or try again.",
                        {"value0": args.action},
                    ),
                    query=payload.get("query", ""),
                )
            )
        )


if __name__ == "__main__":
    main()
