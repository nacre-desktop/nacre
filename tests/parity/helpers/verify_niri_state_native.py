#!/usr/bin/env python3
"""Keep original Niri assertions against the shared developer native adapter."""

import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]


def main():
    sys.path.insert(0, str(ROOT / "nacre/shell-tools/tests"))
    from native_niri_state import BINARY, NativeState

    if not BINARY.is_file():
        raise ValueError(
            "Build niri-state-parity before the state specification checks"
        )
    sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
    spec = importlib.util.spec_from_file_location(
        "native_niri_state_spec", ROOT / "nacre/shell-tools/tests/test_niri_backend.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    with patch.object(module.niri, "State", NativeState):
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(module.NiriStateTests)
        result = unittest.TextTestRunner(verbosity=1).run(suite)
    if result.skipped:
        raise RuntimeError("Niri reducer specification skipped a prerequisite")
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
