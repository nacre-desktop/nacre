"""Pacman-owned runtime references; private configuration uses configure.py."""

import json
from pathlib import Path
import os


def metadata(root):
    return json.loads((root / "package.json.installed").read_text())


def files(root, profile):
    result = {}
    for component in (
        "core",
        *[v for v in ("ai", "brain") if v in profile["components"]],
    ):
        tree = root / "deployment" / component
        for source in tree.rglob("*"):
            if source.is_file():
                result[source.relative_to(tree)] = source
    return result


def links(root, profile):
    values = {
        ".local/share/nacre/shell/source": root / "nacre/shell",
        ".local/share/nacre/shell/tools": root / "nacre/shell-tools",
        ".local/share/nacre/palette-runtime": root / "runtime/palette",
        ".local/share/nacre/dolphin-style": Path("/usr/lib/nacre/dolphin-style"),
        ".local/share/nacre/thunar-style": Path("/usr/lib/nacre/thunar-style"),
        ".config/quickshell/nacre": root / "nacre/shell",
    }
    if "brain" in profile["components"]:
        values[".local/share/nacre/brain"] = root / "brain"
    if "ai" in profile["components"]:
        values[".local/share/nacre/ai/application"] = root / "runtime/ai"
    return {Path(key): value for key, value in values.items()}


def plan(home, root, profile):
    known_path = home / ".local/state/nacre/package-links.json"
    known = json.loads(known_path.read_text()) if known_path.exists() else {}
    desired = links(root, profile)
    changed = []
    for relative, target in desired.items():
        path = home / relative
        if path.is_symlink() and os.readlink(path) == str(target):
            continue
        if path.exists() or path.is_symlink():
            if not path.is_symlink() or os.readlink(path) != known.get(str(relative)):
                raise RuntimeError("Local runtime preserved: " + str(path))
        changed.append((relative, target))
    for relative, target in known.items():
        if Path(relative) in desired:
            continue
        path = home / relative
        if path.is_symlink() and os.readlink(path) == target:
            changed.append((Path(relative), None))
        elif path.exists() or path.is_symlink():
            raise RuntimeError("Local runtime preserved: " + str(path))
    return changed


def apply(home, root, profile):
    for relative, target in plan(home, root, profile):
        path = home / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.unlink(missing_ok=True)
        if target is not None:
            pending = path.with_name("." + path.name + ".next")
            pending.unlink(missing_ok=True)
            pending.symlink_to(target)
            os.replace(pending, path)
    from configure import atomic

    atomic(
        home / ".local/state/nacre/package-links.json",
        json.dumps({str(k): str(v) for k, v in links(root, profile).items()}).encode(),
        0o600,
    )
