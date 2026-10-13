#!/usr/bin/env python3
"""Public module contract driver for helpers which have no command-line interface.

A replacement may expose the same operation/JSON-argument boundary. This driver
runs in the child sandbox; the parent runner never imports production helpers.
"""

import importlib.util
import importlib.machinery
import configparser
import json
from pathlib import Path
import subprocess
import sys


def encode(value):
    if isinstance(value, set):
        return sorted(value)
    if isinstance(value, configparser.ConfigParser):
        return {section: dict(value[section]) for section in value.sections()}
    if isinstance(value, Path):
        return str(value)
    if isinstance(value, subprocess.CompletedProcess):
        return {
            "code": value.returncode,
            "stdout": value.stdout,
            "stderr": value.stderr,
        }
    raise TypeError(type(value).__name__)


def decode(value):
    if isinstance(value, dict):
        if set(value) == {"$path"}:
            return Path(value["$path"])
        return {key: decode(item) for key, item in value.items()}
    if isinstance(value, list):
        return [decode(item) for item in value]
    return value


def main():
    path, operation, arguments = sys.argv[1:]
    if Path(path).is_file():
        spec = importlib.util.spec_from_file_location("parity_subject", path)
        if spec is None:
            spec = importlib.util.spec_from_loader(
                "parity_subject",
                importlib.machinery.SourceFileLoader("parity_subject", path),
            )
        module = importlib.util.module_from_spec(spec)
        sys.modules[spec.name] = module
        spec.loader.exec_module(module)
    else:
        module = importlib.import_module(path)
    request = decode(json.loads(arguments))
    target = module
    parts = operation.split(".")
    if isinstance(getattr(module, parts[0]), type):
        target = getattr(module, parts[0])(*request.get("construct", []))
        parts = parts[1:]
    for part in parts:
        target = getattr(target, part)
    try:
        value = target(*request.get("args", []), **request.get("kwargs", {}))
        if request.get("iterate"):
            value = list(value)
        print(json.dumps(value, default=encode, allow_nan=False))
    except (ValueError, RuntimeError, OSError, KeyError, TypeError) as error:
        print(json.dumps({"error": str(error), "type": type(error).__name__}))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
