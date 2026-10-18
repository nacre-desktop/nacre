#!/usr/bin/env python3
"""Compare the native controller with immutable private-input Python references."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from run import snapshot

ROOT = Path(__file__).resolve().parents[3]


def policy_effects(effects):
    result = []
    for effect in effects:
        if effect["operation"] != "sleep" or effect["args"] == [20]:
            result.append(effect)
            continue
        # The old recorder patched the shared time module. CPython Popen.wait
        # also called that hook; those diagnostic polls are not helper policy.
        assert effect["kwargs"] == {} and len(effect["args"]) == 1
        value = effect["args"][0]
        assert type(value) in (float, int) and 0 < value <= 0.05
    return result


def main():
    binary = Path(
        os.environ.get(
            "NACRE_DESKTOP_PARITY",
            ROOT / "target/debug/examples/desktop-settings-parity",
        )
    )
    count = 0
    for name in (
        "desktop-settings-controller-reference.json",
        "desktop-settings-controller-rollback-reference.json",
    ):
        reference = json.loads(
            (ROOT / "crates/nacre-helper/tests/fixtures" / name).read_text()
        )
        policy = json.loads(
            (
                ROOT
                / "crates/nacre-helper/tests/fixtures/desktop-settings-delay-reference.json"
            ).read_text()
        )
        assert policy["source_sha256"] == reference["source_sha256"] and policy[
            "sleep_calls"
        ] == [20]
        for case in reference["cases"]:
            with tempfile.TemporaryDirectory(prefix="dsref-") as folder:
                env = dict(
                    PATH="/usr/bin:/bin",
                    HOME=folder,
                    NACRE_HELPER_TOOLS=str(ROOT / "nacre/shell-tools"),
                    NACRE_COMPOSITOR=case["backend"],
                    # The original recorder ran from this checkout-local venv.
                    # It is an inert recorded argv, not an executable to launch.
                    NACRE_REFERENCE_PYTHON=str(ROOT / ".venv/bin/python"),
                    XDG_RUNTIME_DIR=folder + "/runtime",
                    DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                    DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
                )
                for key, value in case.get("environment", {}).items():
                    env[key] = value.replace("{home}", folder).replace(
                        "{root}", str(ROOT)
                    )
                result = subprocess.run(
                    [str(binary)],
                    input=json.dumps(case),
                    env=env,
                    text=True,
                    capture_output=True,
                    timeout=20,
                    umask=0o022,
                )
                if result.returncode:
                    raise AssertionError(result.stderr[-1200:])
                actual = json.loads(result.stdout)
                actual["files"] = snapshot(Path(folder))
                actual = json.loads(
                    json.dumps(actual)
                    .replace(folder, "{home}")
                    .replace(str(ROOT), "{root}")
                    .replace(sys.executable, "{python}")
                )
                expected = {
                    key: case[key] for key in ("stdout", "exit", "effects", "files")
                }
                expected["effects"] = policy_effects(expected["effects"])
                if actual != expected:
                    artifact = (
                        Path(os.environ.get("TMPDIR", "/tmp"))
                        / "desktop-settings-parity-difference.json"
                    )
                    artifact.write_text(
                        json.dumps(dict(expected=expected, actual=actual), indent=2)
                        + "\n"
                    )
                    raise AssertionError(
                        f"{case['backend']} {case['args']} differs: {artifact}"
                    )
                count += 1
    print(count, "immutable settings controller contracts exact")


if __name__ == "__main__":
    main()
