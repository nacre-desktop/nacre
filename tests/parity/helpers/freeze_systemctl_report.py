"""Capture real systemctl formatting against a strictly private fake manager."""

import copy
import json
from pathlib import Path
import subprocess
import tempfile

import run

PROPERTIES = ["Id", "LoadState", "ActiveState", "Result", "ExecMainStatus"]
SERVICE = "org.freedesktop.systemd1"
DESTINATION = (
    run.ROOT / "crates/nacre-helper/tests/fixtures/systemctl-report-reference.json"
)


def unit_path(name):
    return "/org/freedesktop/systemd1/unit/" + "".join(
        char if char.isalnum() else f"_{ord(char):02x}" for char in name
    )


def properties(name, **changes):
    rows = {
        "Id": {"variant": "s", "value": name},
        "LoadState": {"variant": "s", "value": "loaded"},
        "ActiveState": {"variant": "s", "value": "active"},
        "Result": {"variant": "s", "value": "success"},
        "ExecMainStatus": {"variant": "i", "value": 0},
    }
    for key, value in changes.items():
        rows[key]["value"] = value
    return rows


def cases():
    active = properties("nacre-shell.service")
    other = properties(
        "fixture.service", ActiveState="failed", Result="exit-code", ExecMainStatus=7
    )
    missing = {"error": SERVICE + ".NoSuchUnit", "text": "Synthetic unit not found"}
    yield "single", [("nacre-shell.service", active)], PROPERTIES
    extras = {
        "Description": {"variant": "s", "value": "Synthetic ignored description"},
        **active,
        "MainPID": {"variant": "u", "value": 100},
    }
    yield "unselected-properties", [("nacre-shell.service", extras)], PROPERTIES
    yield (
        "reversed-reply-order",
        [("nacre-shell.service", dict(reversed(list(active.items()))))],
        PROPERTIES,
    )
    yield (
        "reversed-option-order",
        [("nacre-shell.service", active)],
        list(reversed(PROPERTIES)),
    )
    yield (
        "two-units",
        [("nacre-shell.service", active), ("fixture.service", other)],
        PROPERTIES,
    )
    yield (
        "missing-after-valid",
        [("nacre-shell.service", active), ("missing.service", missing)],
        PROPERTIES,
    )
    yield (
        "missing-before-valid",
        [("missing.service", missing), ("nacre-shell.service", active)],
        PROPERTIES,
    )
    yield "missing-only", [("missing.service", missing)], PROPERTIES
    yield (
        "empty-string-properties",
        [
            (
                "fixture.service",
                properties("fixture.service", LoadState="", ActiveState="", Result=""),
            )
        ],
        PROPERTIES,
    )
    yield (
        "not-found-properties",
        [
            (
                "missing.service",
                properties(
                    "missing.service",
                    LoadState="not-found",
                    ActiveState="inactive",
                    Result="",
                ),
            )
        ],
        PROPERTIES,
    )


def capture_case(name, units, wanted):
    rules = []
    for unit, values in units:
        reply = (
            copy.deepcopy(values)
            if "error" in values
            else {"signature": "a{sv}", "body": [values]}
        )
        rules.append(
            {
                "match": {
                    "destination": SERVICE,
                    "path": unit_path(unit),
                    "interface": "org.freedesktop.DBus.Properties",
                    "member": "GetAll",
                    "signature": "s",
                    "body": [""],
                },
                "reply": reply,
                "effects": [],
            }
        )
    case = {
        "helper": "systemctl-report-reference",
        "stdout": "text",
        "compare_stderr": True,
        "env": {
            "SYSTEMD_OFFLINE": "0",
            "SYSTEMCTL_FORCE_BUS": "1",
            "LC_ALL": "C",
            "NO_COLOR": "1",
        },
        "dbus": rules,
    }
    arguments = [
        "--user",
        "show",
        *[unit for unit, _ in units],
        *[argument for prop in wanted for argument in ("-p", prop)],
    ]
    # The runner mounts this exact executable into the isolated PATH. Neither
    # the process nor the fixture peer can reach the owner session/system bus.
    with tempfile.TemporaryDirectory(prefix="systemctl-wire-") as directory:
        trace = Path(directory) / "wire.json"
        import shlex

        result = run.execute(
            case, shlex.join(["/usr/bin/systemctl", *arguments]), trace
        )
        wire = json.loads(trace.read_text())["dbus_calls"]
    if result["effects"] or result["files"] or result["deleted"]:
        raise ValueError("Unexpected private probe action or filesystem effect")
    expected_wire = []
    for rule in rules:
        expected_wire.append(rule["match"])
        if "error" in rule["reply"]:
            break
    if wire != expected_wire:
        raise ValueError(
            "Real systemctl did not issue exactly the private property queries"
        )
    return {
        "name": name,
        "units": [unit for unit, _ in units],
        "properties": wanted,
        "replies": rules,
        "stdout": result["stdout"],
        "stderr": result["stderr"],
        "exit": result["exit"],
        "wire": wire,
    }


def capture(destination=DESTINATION):
    if destination.exists():
        raise ValueError("Reference exists; refusing overwrite")
    version = subprocess.check_output(
        ["/usr/bin/systemctl", "--version"], text=True, timeout=5
    ).splitlines()[0]
    values = [capture_case(*case) for case in cases()]
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as output:
        output.write(
            json.dumps(
                {
                    "reference": "Real systemctl against private fake systemd; no owner bus or units",
                    "systemctlVersion": version,
                    "cases": values,
                },
                indent=2,
                ensure_ascii=True,
            )
            + "\n"
        )
    return len(values)


if __name__ == "__main__":
    print(f"Captured {capture()} real systemctl property-format cases on a private bus")
