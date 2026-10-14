#!/usr/bin/env python3
"""Freeze actual log output and canceled update dispatch before their Rust port."""

import hashlib
import importlib.util
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    "menu_terminal_reference", HERE / "run.py"
)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
SOURCE_SHA = "d74ca0b2a6d6d41a173b399fa5fbf876472d9ef8b18572d604ea8b20108eb958"


def capture():
    source = runner.ROOT / "nacre/shell-tools/menu-backend.py"
    if hashlib.sha256(source.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Current Python terminal-action source changed")
    entries = []
    for code in [0, 7]:
        entries.append(
            (
                f"logs-exit-{code}",
                {
                    "helper": "menu-backend",
                    "args": ["tool", "logs"],
                    "stdout": "text",
                    "compare_stderr": True,
                    "reference_source_sha256": SOURCE_SHA,
                    "commands": {
                        "journalctl": [
                            {"stdout": "Fixture journal output\n", "code": code}
                        ]
                    },
                },
            )
        )
    entries.append(
        (
            "updates-cancel",
            {
                "helper": "menu-backend",
                "args": ["tool", "updates"],
                "stdout": "text",
                "stdin": "n\n",
                "compare_stderr": True,
                "reference_source_sha256": SOURCE_SHA,
                "commands": {
                    "dirname": [
                        {
                            "argv": ["--", "/repo/nacre/shell-tools/updates.sh"],
                            "stdout": "/repo/nacre/shell-tools\n",
                        }
                    ]
                },
            },
        )
    )
    destination = HERE / "menu-backend"
    if any(
        (destination / f"{prefix}-{name}.json").exists()
        for name, _ in entries
        for prefix in ["case", "expected"]
    ):
        raise ValueError("Refusing to overwrite terminal-action references")
    results = [(name, case, runner.execute(case, None)) for name, case in entries]
    for name, case, expected in results:
        for prefix, value in [("case", case), ("expected", expected)]:
            with (destination / f"{prefix}-{name}.json").open("x") as stream:
                stream.write(json.dumps(value, indent=2) + "\n")
    return results


if __name__ == "__main__":
    results = capture()
    print(
        "Frozen terminal actions:",
        [(name, value["exit"]) for name, _, value in results],
    )
