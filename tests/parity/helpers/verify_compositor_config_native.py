#!/usr/bin/env python3
"""Run the existing Python compiler assertions against the real native API.

This developer-only bridge performs no compiler lowering and reads no expected
outputs. Existing unittest bodies retain their assertions, Lua registration
checks and native niri parser checks. Runtime compiler removal is a later step.
"""

import builtins
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
BINARY = Path(
    os.environ.get("NACRE_COMPOSITOR_CONFIG_PARITY")
    or ROOT / "target/debug/examples/compositor-config-parity"
)


def encode(value):
    if isinstance(value, Path):
        return {"$path": str(value)}
    if isinstance(value, dict):
        if value and all(type(key) is int for key in value):
            return {
                "$integer_map": [[key, encode(item)] for key, item in value.items()]
            }
        return {key: encode(item) for key, item in value.items()}
    if isinstance(value, (list, tuple)):
        return [encode(item) for item in value]
    return value


def native(module, operation):
    def execute(*args, **kwargs):
        request = json.dumps({"args": encode(args), "kwargs": encode(kwargs)})
        result = subprocess.run(
            [str(BINARY), module, operation, request],
            cwd=ROOT,
            stdin=subprocess.DEVNULL,
            capture_output=True,
            text=True,
            timeout=10,
            check=False,
        )
        if result.stderr or result.returncode not in (0, 1):
            raise RuntimeError(
                "Native compiler boundary failed: " + result.stderr[:300]
            )
        value = json.loads(result.stdout)
        if result.returncode:
            exception = getattr(builtins, value["type"], ValueError)
            raise exception(value["error"])
        if operation == "supported_backends":
            return tuple(value)
        return value

    return execute


def load(name):
    spec = importlib.util.spec_from_file_location(
        "native_spec_" + name, ROOT / "tools/tests" / (name + ".py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def main():
    if not BINARY.is_file():
        raise ValueError("Build the compositor-config-parity example first")
    sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
    sys.path.insert(0, str(ROOT / "tools"))
    modules = [load("test_compositor_config"), load("test_niri_config")]
    guards = []
    for spec in modules:
        config = spec.config
        for operation in (
            "load",
            "render",
            "profile_rules",
            "render_profile",
            "rule_lua",
            "workspace_target",
            "rule_kdl",
            "render_keyboard",
            "supported_backends",
            "backend_capabilities",
        ):
            guards.append(patch.object(config, operation, native("neutral", operation)))
        for operation in ("quoted", "keys", "binding_command", "rule_kdl", "render"):
            guards.append(
                patch.object(config.niri, operation, native("niri", operation))
            )
        for operation in (
            "lua_value",
            "workspace_target",
            "action_lua",
            "rule_lua",
            "render",
        ):
            guards.append(
                patch.object(config.hyprland, operation, native("hyprland", operation))
            )
        if hasattr(spec, "niri"):
            for operation in (
                "quoted",
                "keys",
                "binding_command",
                "rule_kdl",
                "render",
            ):
                guards.append(
                    patch.object(spec.niri, operation, native("niri", operation))
                )
    try:
        for guard in guards:
            guard.start()
        suite = unittest.TestSuite(
            unittest.defaultTestLoader.loadTestsFromModule(module) for module in modules
        )
        result = unittest.TextTestRunner(verbosity=1).run(suite)
        if result.skipped:
            raise RuntimeError("Compiler specification checks skipped prerequisites")
        return 0 if result.wasSuccessful() else 1
    finally:
        for guard in reversed(guards):
            guard.stop()


if __name__ == "__main__":
    raise SystemExit(main())
