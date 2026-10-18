#!/usr/bin/env python3
"""Freeze the current Settings JavaScript validator; never replace a reference."""

import json
import os
import subprocess
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
TARGET = ROOT / "crates/nacre-helper/tests/fixtures/settings-values-reference.json"


def main():
    if TARGET.exists():
        raise SystemExit("Reference already exists; refusing to replace it")
    schema = json.loads(
        (ROOT / "nacre/shell/modules/settings/registry/settings.json").read_text()
    )
    cases = []
    for setting in schema["settings"]:
        values = [None, False, True, 0, 1, -1, 0.5, "", "text", [], {}, [""], ["app"]]
        values.append(setting["default"])
        values.extend(option["value"] for option in setting.get("options", []))
        if "range" in setting:
            limits = setting["range"]
            values.extend(
                [
                    limits["min"],
                    limits["max"],
                    limits["min"] - 1,
                    limits["max"] + 1,
                    limits["min"] + limits["step"] / 2,
                ]
            )
        if setting["type"] == "array":
            values.extend(
                [
                    ["a"] * 512,
                    ["a"] * 513,
                    [42],
                    [None],
                    ["a\0b"],
                    ["x" * 4096],
                    ["x" * 4097],
                    ["😀" * 2048],
                    ["😀" * 2049],
                    ["\u0085"],
                    ["\ufeff"],
                    ["  "],
                ]
            )
        cases.extend({"setting": setting, "value": value} for value in values)
    program = r"""
const fs = require('fs'), vm = require('vm');
const source = fs.readFileSync(process.argv[1], 'utf8').replace(/^\.pragma library\s*/, '');
const context = vm.createContext({}); vm.runInContext(source, context);
const cases = JSON.parse(fs.readFileSync(0, 'utf8'));
for (const test of cases) test.valid = context.validValue(test.setting, test.value);
process.stdout.write(JSON.stringify({source:'registry.js validValue', cases}, null, 2)+'\n');
"""
    result = subprocess.run(
        [
            os.environ.get("NACRE_REFERENCE_NODE", "node"),
            "-e",
            program,
            str(ROOT / "nacre/shell/modules/settings/registry/registry.js"),
        ],
        input=json.dumps(cases),
        text=True,
        capture_output=True,
        check=True,
    )
    with TARGET.open("x") as output:
        output.write(result.stdout)
    print(f"Frozen {len(cases)} JavaScript validation cases")


if __name__ == "__main__":
    main()
