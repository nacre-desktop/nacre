"""Declared storage-only differences; frozen payloads and references stay intact.

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


def compatible_files(result, case):
    result = copy.deepcopy(result)
    helper = case.get("helper")
    for name, record in list(result.get("files", {}).items()):
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
        record["text"] = json.dumps(data, ensure_ascii=True)
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
