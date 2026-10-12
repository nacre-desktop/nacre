#!/usr/bin/env python3
"""Private drafts, user-selected local attachments, clipboard and area screenshots."""

import argparse
from nacre_i18n import tr
import hashlib
import json
import os
import subprocess
import sys
import uuid
from pathlib import Path
from urllib.parse import urlparse, unquote

ROOT = Path.home() / ".local/state/nacre/shell/drafts"


def key_path(key):
    return ROOT / (hashlib.sha256(str(key).encode()).hexdigest() + ".json")


def attachments(values):
    if not isinstance(values, list) or len(values) > 8:
        raise ValueError(
            tr(
                "helper.composer.choose-at-most-eight-attachments",
                "Choose at most eight attachments",
            )
        )
    result = []
    for value in values:
        value = value.get("path", "") if isinstance(value, dict) else str(value)
        parsed = urlparse(value)
        if parsed.scheme and (
            parsed.scheme != "file" or parsed.netloc not in ("", "localhost")
        ):
            raise ValueError(
                tr(
                    "helper.composer.attachments-must-be-local-files",
                    "Attachments must be local files",
                )
            )
        path = (
            Path(unquote(parsed.path) if parsed.scheme else value)
            .expanduser()
            .resolve()
        )
        if not path.is_file():
            raise ValueError(
                tr(
                    "helper.composer.attachment-no-longer-exists-value0",
                    "Attachment no longer exists: {value0}",
                    {"value0": path.name},
                )
            )
        if path.stat().st_size > 30 * 1024 * 1024:
            raise ValueError(
                tr(
                    "helper.composer.attachment-exceeds-30-mib-value0",
                    "Attachment exceeds 30 MiB: {value0}",
                    {"value0": path.name},
                )
            )
        if str(path) not in [r["path"] for r in result]:
            result.append(
                dict(
                    path=str(path),
                    name=path.name,
                    image=path.suffix.lower()
                    in (".png", ".jpg", ".jpeg", ".webp", ".gif"),
                )
            )
    return result


def save(key, text, files):
    ROOT.mkdir(parents=True, exist_ok=True, mode=0o700)
    path = key_path(key)
    temp = path.with_suffix(".next")
    temp.write_text(json.dumps(dict(text=text, attachments=files)))
    temp.chmod(0o600)
    os.replace(temp, path)


def load(key):
    try:
        return json.loads(key_path(key).read_text())
    except (OSError, ValueError):
        return dict(text="", attachments=[])


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "action", choices=["load", "save", "files", "pick", "screenshot", "copy"]
    )
    a = p.parse_args()
    data = (
        json.loads(sys.stdin.readline())
        if a.action in ("load", "save", "files", "copy")
        else {}
    )
    if a.action == "load":
        print(json.dumps(load(data["key"])))
    elif a.action == "save":
        save(data["key"], data.get("text", ""), data.get("attachments", []))
        print("{}")
    elif a.action == "files":
        print(json.dumps(dict(attachments=attachments(data["files"]))))
    elif a.action == "copy":
        subprocess.run(["wl-copy"], input=data["text"], text=True, check=True)
        print("{}")
    elif a.action == "pick":
        r = subprocess.run(
            [
                "zenity",
                "--file-selection",
                "--multiple",
                "--separator=\n",
                "--title=Attach local files",
            ],
            capture_output=True,
            text=True,
        )
        print(
            json.dumps(
                dict(
                    attachments=attachments(r.stdout.strip().splitlines())
                    if r.returncode == 0
                    else []
                )
            )
        )
    else:
        selected = subprocess.run(["slurp"], capture_output=True, text=True)
        if selected.returncode:
            print('{"attachments":[]}')
            return
        target = ROOT / "screenshots" / ("screen-" + uuid.uuid4().hex + ".png")
        target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        subprocess.run(["grim", "-g", selected.stdout.strip(), str(target)], check=True)
        target.chmod(0o600)
        print(json.dumps(dict(attachments=attachments([str(target)]))))


if __name__ == "__main__":
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError, IndexError) as e:
        print(json.dumps(dict(error=str(e))))
