#!/usr/bin/env python3
"""Freeze remaining hosted CLI boundaries before retiring their Python files."""

import hashlib
import importlib.util
import json
from pathlib import Path

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("hosted_cli_reference", HERE / "run.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


def capture():
    probes = {
        "dashboard-data": [
            [],
            ["--help"],
            ["-h"],
            ["--h"],
            ["preferences"],
            ["preferences", "unused"],
            ["preferences", "one", "two"],
            ["--bad", "--help"],
            ["preferences", "--bad"],
            ["--", "preferences", "--help"],
            ["preferences", "-2"],
        ],
        "launcher-preferences": [
            [],
            ["--help"],
            ["-h"],
            ["--h"],
            ["load"],
            ["load", "extra"],
            ["favorite"],
            ["hide"],
            ["record"],
            ["unknown"],
            ["unknown", "--help"],
            ["--help", "unknown"],
            ["load", "--bad"],
            ["--bad", "--help"],
            ["--", "load"],
        ],
        "notification-history": [
            [],
            ["load"],
            ["load", "extra"],
            ["save"],
            ["save", "extra"],
            ["unknown"],
            ["--help"],
        ],
        "dashboard-media": [
            [],
            ["{}"],
            ["{}", "extra"],
            ["bad json"],
            ["bad json", "extra"],
            ["spectrum", "extra"],
        ],
    }
    entries = []
    for helper, rows in probes.items():
        source = runner.ROOT / f"nacre/shell-tools/{helper}.py"
        digest = hashlib.sha256(source.read_bytes()).hexdigest()
        for index, args in enumerate(rows):
            case = {
                "helper": helper,
                "args": args,
                "compare_stderr": True,
                "reference_source_sha256": digest,
            }
            if (
                helper == "launcher-preferences"
                and args
                and args[0] in ["favorite", "hide", "record"]
            ):
                case["stdin"] = '{"id":"fixture.desktop","enabled":true}\n'
                if args[0] == "record":
                    original = json.loads(
                        (HERE / helper / "case-record.json").read_text()
                    )
                    case["normalizations"] = [
                        {
                            key: value.replace("demo.desktop", "fixture.desktop")
                            if isinstance(value, str)
                            else value
                            for key, value in rule.items()
                        }
                        for rule in original["normalizations"]
                    ]
            if helper == "notification-history" and args and args[0] == "save":
                case["stdin"] = "[]\n"
            if args and args[0] in ["--help", "-h", "--h"] or "--help" in args:
                case["stdout"] = "text"
            entries.append(
                (HERE / helper / f"case-cli-boundary-{index:02d}.json", case)
            )
    for path, _ in entries:
        if (
            path.exists()
            or path.with_name(path.name.replace("case-", "expected-")).exists()
        ):
            raise ValueError("Refusing to overwrite hosted CLI references")
    results = [
        (
            path,
            case,
            runner.normalize(
                runner.execute(case, None), case.get("normalizations", [])
            ),
        )
        for path, case in entries
    ]
    for path, case, expected in results:
        with path.open("x") as stream:
            stream.write(json.dumps(case, indent=2) + "\n")
        with path.with_name(path.name.replace("case-", "expected-")).open(
            "x"
        ) as stream:
            stream.write(json.dumps(expected, indent=2) + "\n")
    return results


if __name__ == "__main__":
    results = capture()
    print("Frozen hosted CLI cases:", len(results))
