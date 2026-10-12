#!/usr/bin/env python3
"""Private, on-demand data adapters for Nacre desktop panels."""

import argparse
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
        helper = runpy.run_path(str(Path(__file__).with_name("window-chat-title.py")))
        for client in compositor.query("windows", timeout=12):
            if (
                client["class"] == "nacre-ai-task"
                and helper["resolve_info"](client["pid"]).get("threadId") == item["id"]
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
        raise ValueError("Invalid note")
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
    return "text/plain;charset=utf-8"


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
        raise ValueError("Invalid clipboard entry")
    rows = run(["cliphist", "list"]).stdout.decode(errors="replace").splitlines()
    row = next((r for r in rows if r.startswith(ident + "\t")), None)
    if row is None:
        raise ValueError("Clipboard entry no longer exists")
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
    "SUPER+A": "Applications",
    "SUPER+SPACE": "Command palette",
    "SUPER+D": "Window overview",
    "SUPER+V": "Clipboard history",
    "SUPER+K": "Keyboard shortcuts",
    "SUPER+CTRL+B": "AI and brain drawer",
    "SUPER+SHIFT+O": "Desktop settings",
    "SUPER+O": "Dashboard",
    "SUPER+W": "Wallpaper gallery",
    "SUPER+Z": "Hide/show desktop shell",
    "SUPER+B": "Open brain",
    "SUPER+N": "Capture a thought",
    "SUPER+X": "Power menu",
    "SUPER+ESCAPE": "Lock screen",
    "SUPER+R": "Resize mode",
    "SUPER+Q": "Send window to trash",
    "SUPER+SHIFT+Q": "Restore last trashed window",
    "SUPER+CTRL+Q": "Close window",
    "SUPER+F": "Fullscreen",
    "SUPER+M": "Maximize",
    "SUPER+T": "Toggle floating",
    "SUPER+P": "Pin window",
    "SUPER+TAB": "Next workspace",
    "SUPER+SHIFT+TAB": "Previous workspace",
    "SUPER+SHIFT+T": "Terminal",
    "SUPER+SHIFT+B": "Browser",
    "SUPER+SHIFT+D": "Discord",
    "SUPER+SHIFT+S": "Spotify",
    "SUPER+SHIFT+F": "File manager",
    "SUPER+H": "Hide window",
    "SUPER+SHIFT+H": "Restore hidden window",
    "Print": "Screenshot selection to clipboard",
}


def bindings():
    source = {}
    source_routes = {}
    capture = runpy.run_path(str(Path(__file__).with_name("binding-contracts.py")))[
        "capture"
    ]
    for file in [
        *sorted((HOME / ".config/hypr/conf").glob("*.lua")),
        HOME / ".config/nacre/shortcuts.lua",
    ]:
        if not file.exists() or not re.search(r"\bhl\.bind\s*\(", file.read_text()):
            continue
        try:
            records = capture(file)
        except (RuntimeError, OSError, subprocess.TimeoutExpired):
            continue  # Native descriptions remain available for unsupported local code.
        for record in records:
            key = record["keys"].replace(" ", "").replace("space", "SPACE")
            action = record["action"]
            arguments = action.get("args", [])
            command = (
                arguments[0] if action["method"] == "exec_cmd" and arguments else ""
            )
            desc = DESCRIPTIONS.get(key) or action["method"].replace(".", " ").replace(
                "_", " "
            )
            if command and key not in DESCRIPTIONS:
                desc = Path(command.split()[0]).stem.replace("-", " ").replace("_", " ")
            if action["method"] == "submap" and arguments == ["reset"]:
                desc = "Exit resize mode"
            if "wpctl" in command:
                desc = (
                    "Toggle microphone mute"
                    if "@DEFAULT_AUDIO_SOURCE@" in command
                    else "Toggle speaker mute"
                    if "set-mute" in command
                    else "Volume " + ("down" if "5%-" in command else "up")
                )
            if (
                "nacre-shell brightness " in command
                or "nacre-shell keyboard-light " in command
            ):
                desc = (
                    ("Keyboard" if "keyboard-light" in command else "Screen")
                    + " brightness "
                    + command.split()[-1]
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
            or DESCRIPTIONS.get(combo)
            or b.get("description")
            or b.get("dispatcher", "")
        )
        if b.get("submap"):
            desc = b["submap"] + " · " + desc
        if key.isdigit() and b["modmask"] == 64:
            desc = "Go to workspace " + key
        if key.isdigit() and b["modmask"] == 65:
            desc = "Move window to workspace " + key
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
                raise ValueError("Invalid note")
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
                raise ValueError("Capture must contain 1–16000 characters")
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
            raise ValueError("Unknown desktop operation")
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
                    error="Could not complete "
                    + args.action
                    + ". Check the selected item or try again.",
                    query=payload.get("query", ""),
                )
            )
        )


if __name__ == "__main__":
    main()
