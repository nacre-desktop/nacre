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
    from native_niri_actions import BINARY, NativeBackend

    if not BINARY.is_file():
        raise ValueError(
            "Build niri-action-parity before checking the legacy assertions"
        )
    sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
    spec = importlib.util.spec_from_file_location(
        "native_niri_action_spec", ROOT / "nacre/shell-tools/tests/test_niri_backend.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    with patch.object(module.niri, "Backend", NativeBackend):
        result = unittest.TextTestRunner(verbosity=1).run(
            unittest.defaultTestLoader.loadTestsFromTestCase(module.NiriActionTests)
        )
    if result.skipped:
        raise RuntimeError("Niri action assertions skipped a prerequisite")
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
