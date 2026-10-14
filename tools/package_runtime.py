"""Pacman-owned runtime references; private configuration uses configure.py."""

import json
import importlib
from pathlib import Path
import os
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "nacre/shell-tools"))
_state = importlib.import_module("nacre_state")


def metadata(root):
    return _state.read_json_file(root / "package.json.installed")


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


def presentation_seed(home, common, temporary):
    """A first image needs its palette record too; keep existing private choices."""
    relative = Path(".local/state/nacre/presentation.json")
    if relative not in common or (home / relative).exists():
        return common
    data = _state.read_json_file(common[relative])
    wallpaper = home / ".local/state/nacre/wallpaper/last.txt"
    if wallpaper.is_file():
        path = Path(wallpaper.read_text().strip())
        if path.is_absolute() and path.is_file():
            data["poster"] = str(path)
    scheme = home / ".local/state/nacre/scheme.json"
    if scheme.is_file():
        values = _state.read_json_file(scheme)
        if isinstance(values.get("colours"), dict) and values.get("mode") in (
            "light",
            "dark",
        ):
            data["colours"] = values["colours"]
            data["mode"] = values["mode"]
    source = temporary / "presentation.json"
    source.write_text(json.dumps(data))
    return {**common, relative: source}


def launcher_seed(home, common, temporary):
    from configure import legacy_empty_launcher

    relative = Path(".config/nacre/launcher.json")
    target = home / relative
    if (
        relative not in common
        or not target.exists()
        or not legacy_empty_launcher(relative, target)
    ):
        return common
    data = _state.read_json_file(target)
    defaults = _state.read_json_file(common[relative])
    data["favorites"] = [
        name for name in defaults["favorites"] if name not in data["hidden"]
    ]
    source = temporary / "launcher.json"
    source.write_text(json.dumps(data))
    return {**common, relative: source}


def links(root, profile, compositor="hyprland"):
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
        for target in ("nacre-niri.target", "nacre-hyprland.target"):
            values[f".config/systemd/user/{target}.wants/nacre-brain.service"] = Path(
                "../nacre-brain.service"
            )
        for name in ("check", "sync"):
            values[
                f".config/systemd/user/timers.target.wants/nacre-brain-{name}.timer"
            ] = Path(f"../nacre-brain-{name}.timer")
    if "ai" in profile["components"]:
        values[".local/share/nacre/ai/application"] = root / "runtime/ai"
    return {Path(key): value for key, value in values.items()}


def plan(home, root, profile, compositor="hyprland"):
    known_path = home / ".local/state/nacre/package-links.json"
    known = _state.read_json_file(known_path) if known_path.exists() else {}
    permitted = {
        str(path)
        for backend in ("hyprland", "niri")
        for path in links(root, {"components": ["ai", "brain"]}, backend)
    }
    # Migrate only the exact links recorded by the old globally-enabled setup.
    permitted.update(
        f".config/systemd/user/graphical-session.target.wants/{name}.service"
        for name in (
            "nacre-shell",
            "nacre-session-watch",
            "nacre-power-key",
            "nacre-niri-widths",
            "hypridle",
            "nacre-brain",
        )
    )
    if not isinstance(known, dict) or any(
        key not in permitted or not isinstance(value, str)
        for key, value in known.items()
    ):
        raise ValueError("Invalid package runtime manifest")
    desired = links(root, profile, compositor)
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


def apply(home, root, profile, compositor="hyprland"):
    for relative, target in plan(home, root, profile, compositor):
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
        json.dumps(
            {str(k): str(v) for k, v in links(root, profile, compositor).items()}
        ).encode(),
        0o600,
    )
