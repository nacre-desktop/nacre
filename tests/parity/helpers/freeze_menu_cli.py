"""Capture new menu argument boundaries from the immutable Python reference.

Run once before retiring that source. Existing cases are never overwritten.
All execution uses the parity sandbox, with no live desktop or package tools.
"""

import copy
import hashlib
import json

import run

SOURCE_SHA256 = "d74ca0b2a6d6d41a173b399fa5fbf876472d9ef8b18572d604ea8b20108eb958"
ACTIONS = (
    "state",
    "files",
    "convert",
    "packages",
    "installed",
    "fonts",
    "ownership",
    "tool",
    "install",
    "remove",
    "webapp-create",
    "webapp-remove",
    "webapp-launch",
    "reminder",
    "cancel-reminder",
    "config",
)


def cases():
    folder = run.HERE / "menu-backend"
    state = json.loads((folder / "case-state.json").read_text())
    for action in ACTIONS:
        for form in ("default", "value"):
            case = {"helper": "menu-backend", "args": [action], "compare_stderr": True}
            if form == "value":
                case["args"].append("x" if action in ("files", "packages") else "demo")
            if action == "state":
                case["commands"] = copy.deepcopy(state["commands"])
            elif action in ("installed", "ownership"):
                case["commands"] = {"pacman": [{"stdout": "demo\n"}]}
            elif action == "fonts":
                case["commands"] = {
                    "fc-list": [{"stdout": "Fixture Sans,Other Sans\n"}]
                }
            elif action in ("install", "remove") and form == "value":
                case["commands"] = {"paru": [{"code": 0}]}
            elif action in ("webapp-create", "reminder"):
                case["stdin"] = "{}\n"
            yield f"cli-{action}-{form}", case
    probes = {
        "help-long": ["--help"],
        "help-short": ["-h"],
        "help-before-invalid": ["--help", "not-an-action"],
        "help-after-invalid": ["not-an-action", "--help"],
        "help-after-extra": ["files", "one", "two", "--help"],
        "missing-action": [],
        "missing-action-force": ["--force"],
        "invalid-action": ["not-an-action"],
        "action-abbreviation": ["st"],
        "unknown-option": ["files", "--unknown"],
        "unknown-short-option": ["files", "-f"],
        "extra-value": ["files", "one", "two"],
        "force-before-action": ["--force", "state"],
        "force-between-action-value": ["state", "--force", "demo"],
        "force-after-value": ["state", "demo", "--force"],
        "force-abbreviation": ["--f", "state"],
        "force-longer-abbreviation": ["state", "--for"],
        "force-repeat": ["state", "--force", "--force"],
        "force-ignored-other-action": ["files", "--force"],
        "force-explicit-value": ["files", "--force=true"],
        "force-explicit-empty": ["files", "--force="],
        "negative-number-value": ["files", "-2"],
        "hyphen-word-value": ["files", "-demo"],
        "delimiter-hyphen-value": ["files", "--", "-demo"],
        "delimiter-force-value": ["files", "--", "--force"],
        "delimiter-before-action": ["--", "files", "--force"],
        "empty-query": ["files", ""],
        "whitespace-query": ["files", "   "],
        "empty-conversion": ["convert", ""],
        "empty-packages-query": ["packages", ""],
    }
    for label, arguments in probes.items():
        case = {"helper": "menu-backend", "args": arguments, "compare_stderr": True}
        if "help" in label:
            case["stdout"] = "text"
        if "state" in arguments:
            case["commands"] = copy.deepcopy(state["commands"])
        yield "cli-" + label, case


def capture():
    source = run.ROOT / "nacre/shell-tools/menu-backend.py"
    if (
        not source.is_file()
        or hashlib.sha256(source.read_bytes()).hexdigest() != SOURCE_SHA256
    ):
        raise ValueError(
            "Original Python menu source is absent or changed; refusing capture"
        )
    entries = list(cases())
    folder = run.HERE / "menu-backend"
    paths = [
        folder / f"{prefix}-{label}.json"
        for label, _ in entries
        for prefix in ("case", "expected")
    ]
    if any(path.exists() for path in paths):
        raise ValueError(
            "A capture destination already exists; refusing to overwrite references"
        )
    snapshots = []
    state = json.loads((folder / "case-state.json").read_text())
    for label, case in entries:
        case["reference_source_sha256"] = SOURCE_SHA256
        observed = run.execute(case, None)
        if isinstance(observed["stdout"], dict) and "created" in observed["stdout"]:
            case["normalizations"] = copy.deepcopy(state["normalizations"])
            observed = run.normalize(observed, case["normalizations"])
        snapshots.append((label, case, observed))
    # All sandbox probes succeed before any references are written.
    for label, case, observed in snapshots:
        for prefix, value in (("case", case), ("expected", observed)):
            path = folder / f"{prefix}-{label}.json"
            with path.open("x") as output:
                output.write(json.dumps(value, indent=2, ensure_ascii=True) + "\n")
    return snapshots


if __name__ == "__main__":
    snapshots = capture()
    counts = {}
    for _, _, observed in snapshots:
        counts[observed["exit"]] = counts.get(observed["exit"], 0) + 1
    print(
        f"Captured {len(snapshots)} new isolated Python menu CLI boundaries; exits {counts}"
    )
