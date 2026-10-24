"""Declared state/additive preference defaults; frozen references stay intact.

No helper is imported by the parent harness. New storage invariants are tested
independently against both implementations and the native QML publisher.
"""

import copy
from fnmatch import fnmatchcase
import json
from pathlib import Path

REGISTRY = json.loads(
    (
        Path(__file__).resolve().parents[3] / "nacre/shell-tools/nacre-state-files.json"
    ).read_text()
)["files"]


def entry_for(name, helper):
    entries = sorted(
        REGISTRY, key=lambda item: max(map(len, item["paths"])), reverse=True
    )
    for entry in entries:
        if any(fnmatchcase(name, "*/" + pattern) for pattern in entry["paths"]):
            return entry
    if helper == "notification-history" and name.endswith(
        "/notifications/history.json"
    ):
        return next(entry for entry in entries if entry["id"] == "notifications")
    return None


def canonical_desktop(data):
    """Compare declared v2 names/defaults on both sides; never ignore values."""
    if not isinstance(data, dict):
        return data
    data = copy.deepcopy(data)
    aliases = {
        "topEdge": ["frameTopEnabled", "topBarEnabled"],
        "leftEdge": ["frameLeftEnabled"],
        "rightEdge": ["frameRightEnabled", "controlCenterEnabled"],
        "bottomEdge": ["frameBottomEnabled"],
        "leftDrawer": ["aiSidebarEnabled"],
    }
    for old, keys in aliases.items():
        if old in data:
            value = data.pop(old)
            for key in keys:
                data.setdefault(key, value)
    if isinstance(data.get("normalSnapshot"), dict):
        snapshot = data["normalSnapshot"]
        for old, keys in aliases.items():
            if old in snapshot:
                value = snapshot.pop(old)
                for key in keys:
                    snapshot.setdefault(key, value)
    # N0057: the new consent preference defaults off on both sides. Explicit
    # values remain observable; never erase a mistakenly enabled lookup.
    if "weatherIpLookup" in data and not isinstance(data["weatherIpLookup"], bool):
        raise ValueError("Weather IP lookup consent must remain boolean")
    data.setdefault("weatherIpLookup", False)
    data.setdefault("dashboardEnabled", True)
    data.setdefault("lipHoverDelay", 350)
    for key, value in {
        "interfaceGradients": True,
        "monoFont": "",
        "textSize": 12,
        "reduceMotion": False,
    }.items():
        data.setdefault(key, value)
        if isinstance(data.get("normalSnapshot"), dict):
            data["normalSnapshot"].setdefault(key, value)
    return data


def canonical_wallpaper_output(value):
    """The additive automatic-mode preference defaults to false, on both sides."""
    if isinstance(value, dict):
        value = {key: canonical_wallpaper_output(item) for key, item in value.items()}
        if "paletteMode" in value and "palettePreset" in value:
            value.setdefault("paletteAutoMode", False)
    elif isinstance(value, list):
        value = [canonical_wallpaper_output(item) for item in value]
    return value


def compatible_files(result, case):
    result = copy.deepcopy(result)
    helper = case.get("helper")
    if helper == "wallpaper-media":
        result["stdout"] = canonical_wallpaper_output(result.get("stdout"))
    if (
        helper == "desktop-settings"
        and isinstance(result.get("stdout"), dict)
        and "data" in result["stdout"]
    ):
        result["stdout"]["data"] = canonical_desktop(result["stdout"]["data"])
    for name, record in list(result.get("files", {}).items()):
        if name.endswith("/shell/notifications/.writer.lock"):
            if record.get("mode") != "0o600":
                raise ValueError("Notification writer fencing must remain private")
            del result["files"][name]
            continue
        base = (
            name.removesuffix(".schema.lock")
            if name.endswith(".schema.lock")
            else name.split(".pre-v", 1)[0]
            if ".pre-v" in name
            else name.removesuffix(".corrupt.bak")
            if name.endswith(".corrupt.bak")
            else None
        )
        if base and entry_for(base, helper):
            if record.get("mode") != "0o600":
                raise ValueError("State migration metadata must remain private")
            del result["files"][name]
            continue
        if name.endswith("/nacre/session/events.jsonl") and "text" in record:
            rows = [json.loads(line) for line in record["text"].splitlines()]
            for row in rows:
                if row.get("version") == 1:
                    del row["version"]
            record["text"] = "".join(
                json.dumps(row, ensure_ascii=True) + "\n" for row in rows
            )
            continue
        entry = entry_for(name, helper)
        if not entry or "text" not in record:
            continue
        try:
            data = json.loads(record["text"])
        except (ValueError, TypeError):
            continue  # Corrupt original bytes still compare exactly.
        if isinstance(data, dict) and data.get("version") == entry["version"]:
            if entry["shape"] == "array":
                if set(data) != {"version", "items"} or not isinstance(
                    data["items"], list
                ):
                    raise ValueError("Unexpected fields in versioned state envelope")
                data = data["items"]
            elif not entry.get("keepVersion"):
                del data["version"]
            else:
                data["version"] = entry["version"]
        if entry["id"] == "desktop":
            if (
                isinstance(data, dict)
                and isinstance(data.get("version"), int)
                and not isinstance(data.get("version"), bool)
                and data["version"] <= entry["version"]
            ):
                data.pop("version", None)
            data = canonical_desktop(data)
        if entry["id"] == "wallpaper-picker" and isinstance(data, dict):
            data.setdefault("paletteAutoMode", False)
        record["text"] = json.dumps(
            data, ensure_ascii=True, sort_keys=entry["id"] == "wallpaper-picker"
        )
    return result


def recovery_reference(expected, case):
    # Explicit intended difference: an invalid notification list loads an empty
    # view; the original file remains untouched until a backed-up save recovers it.
    if (
        case.get("helper") == "notification-history"
        and case.get("args") == ["load"]
        and case.get("fixtures")
        == {"home/.local/state/nacre/shell/notifications/history.json": "{}"}
    ):
        if expected["exit"] != 1 or expected["stdout"] != "":
            raise ValueError("Unexpected frozen notification recovery reference")
        return {**expected, "stdout": "[]\n", "exit": 0}
    if (
        case.get("helper") == "profile-policy"
        and case.get("args") == ["show"]
        and case.get("fixtures") == {"home/.config/nacre/profile.json": "{broken"}
    ):
        if expected["exit"] != 1 or expected["stdout"] is not None:
            raise ValueError("Unexpected frozen profile recovery reference")
        defaults = json.loads(
            (Path(__file__).resolve().parents[3] / "profiles/default.json").read_text()
        )
        defaults.update(aiFullAccess=False, windowTrashBindings=False)
        return {**expected, "stdout": defaults, "exit": 0}
    return expected
