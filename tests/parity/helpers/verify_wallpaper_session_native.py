#!/usr/bin/env python3
"""Strict private logind peer verifies equivalent original session-query effects."""

import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]


def ready(process):
    if not select.select([process.stdout], [], [], 10)[0]:
        raise AssertionError("Private session peer did not start")
    return process.stdout.readline()


def main():
    reference = json.loads(
        (
            ROOT / "crates/nacre-helper/tests/fixtures/wallpaper-session-reference.json"
        ).read_text()
    )
    binary = os.environ.get(
        "NACRE_HELPER_BINARY", str(ROOT / "target/debug/nacre-helper")
    )
    cases = reference["cases"] + [{**reference["cases"][1], "watch": True}]
    for case in cases:
        with tempfile.TemporaryDirectory(prefix="nacre-session-native-") as folder:
            root = Path(folder)
            (root / "runtime").mkdir()
            (root / "effects.jsonl").touch()
            address = "unix:path=" + str(root / "runtime/parity-bus")
            effects = case["effects"]
            user = "/org/freedesktop/login1/user/_fixture"
            path = "/org/freedesktop/login1/session/_7"

            def rule(
                object_path,
                interface,
                member,
                signature,
                body,
                reply_signature,
                reply_body,
                effect=None,
            ):
                return dict(
                    match=dict(
                        destination="org.freedesktop.login1",
                        path=object_path,
                        interface=interface,
                        member=member,
                        signature=signature,
                        body=body,
                    ),
                    reply=dict(signature=reply_signature, body=reply_body),
                    effects=[effect] if effect else [],
                )

            rules = [
                rule(
                    "/org/freedesktop/login1",
                    "org.freedesktop.login1.Manager",
                    "GetUser",
                    "u",
                    [os.getuid()],
                    "o",
                    [user],
                ),
                rule(
                    user,
                    "org.freedesktop.DBus.Properties",
                    "Get",
                    "ss",
                    ["org.freedesktop.login1.User", "Display"],
                    "v",
                    [
                        {
                            "variant": "(so)",
                            "value": [
                                "" if case["name"] == "empty" else "7",
                                "/" if case["name"] == "empty" else path,
                            ],
                        }
                    ],
                    effects[0],
                ),
            ]
            if case["name"] != "empty":
                rules += [
                    rule(
                        path,
                        "org.freedesktop.DBus.Properties",
                        "Get",
                        "ss",
                        ["org.freedesktop.login1.Session", "LockedHint"],
                        "v",
                        [{"variant": "b", "value": case["name"] == "locked"}],
                        effects[1],
                    ),
                    rule(
                        "/org/freedesktop/login1",
                        "org.freedesktop.login1.Manager",
                        "GetSession",
                        "s",
                        ["7"],
                        "o",
                        [path],
                        effects[2],
                    ),
                ]
            if case.get("watch"):
                rules[-1]["signals"] = [
                    dict(
                        delay=0.1,
                        path=path,
                        interface="org.freedesktop.DBus.Properties",
                        member="PropertiesChanged",
                        signature="sa{sv}as",
                        body=[
                            "org.freedesktop.login1.Session",
                            {"LockedHint": {"variant": "b", "value": False}},
                            [],
                        ],
                    ),
                    *[
                        dict(
                            delay=delay,
                            path="/org/freedesktop/login1",
                            interface="org.freedesktop.login1.Manager",
                            member="PrepareForSleep",
                            signature="b",
                            body=[value],
                        )
                        for delay, value in ((0.2, True), (0.3, False))
                    ],
                ]
            (root / "dbus-fixtures.json").write_text(json.dumps(rules))
            daemon = subprocess.Popen(
                [
                    "dbus-daemon",
                    "--session",
                    "--nofork",
                    "--address",
                    address,
                    "--print-address",
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            peer = None
            try:
                ready(daemon)
                peer = subprocess.Popen(
                    [
                        sys.executable,
                        str(ROOT / "tests/parity/helpers/dbus_peer.py"),
                        str(root),
                    ],
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                ready(peer)
                env = {
                    **os.environ,
                    "HOME": str(root),
                    "DBUS_SYSTEM_BUS_ADDRESS": address,
                    "NACRE_HELPER_TOOLS": str(ROOT / "nacre/shell-tools"),
                }
                if case.get("watch"):
                    child = subprocess.Popen(
                        [binary, "wallpaper-media", "session", "--watch"],
                        env=env,
                        stdout=subprocess.PIPE,
                        stderr=subprocess.PIPE,
                        text=True,
                    )
                    try:
                        values = [json.loads(ready(child)) for _ in range(4)]
                        assert [value["locked"] for value in values] == [
                            True,
                            False,
                            False,
                            False,
                        ], "LockedHint events differ"
                        assert [value["sleeping"] for value in values] == [
                            False,
                            False,
                            True,
                            False,
                        ], "Sleep events differ"
                        assert all(value["path"] == path for value in values), (
                            "Session path changed unexpectedly"
                        )
                    finally:
                        child.terminate()
                        child.communicate(timeout=5)
                    calls = [
                        json.loads(line)
                        for line in (root / "effects.jsonl").read_text().splitlines()
                    ]
                    assert calls == effects, (
                        "Session watch performed unexpected requests"
                    )
                    continue
                result = subprocess.run(
                    [binary, "palette-api", "wallpaper-media", "session"],
                    input='{"args":[],"kwargs":{}}',
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=15,
                )
                if result.returncode:
                    artifact = Path("/tmp/nacre-session-native-failure.json")
                    artifact.write_text(
                        json.dumps(
                            dict(
                                reply=result.stdout,
                                stderr=result.stderr,
                                wire=(root / "dbus-wire.jsonl").read_text()
                                if (root / "dbus-wire.jsonl").exists()
                                else "",
                            ),
                            indent=2,
                        )
                    )
                    raise AssertionError(
                        f"{case['name']}: native session failed; details {artifact}"
                    )
                value = json.loads(result.stdout)["result"]
                assert json.dumps(value) + "\n" == case["stdout"], (
                    f"{case['name']}: session response differs"
                )
                calls = [
                    json.loads(line)
                    for line in (root / "effects.jsonl").read_text().splitlines()
                ]
                assert calls == effects, f"{case['name']}: session effects differ"
            finally:
                for child in (peer, daemon):
                    if child:
                        child.terminate()
                        child.communicate(timeout=5)
    print(
        f"Native logind: {len(reference['cases'])} immutable states, lock/sleep events and exact equivalent effects"
    )


if __name__ == "__main__":
    main()
