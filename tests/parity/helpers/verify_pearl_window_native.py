#!/usr/bin/env python3
"""Retain original pearl-window assertions with native ownership and geometry."""

import importlib.util
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]


def main():
    sys.path.insert(0, str(ROOT / "nacre/shell-tools/tests"))
    sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
    spec = importlib.util.spec_from_file_location(
        "native_pearl_spec", ROOT / "nacre/shell-tools/tests/test_pearls.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    from native_pearl_window import NativeWindow

    original = module.module
    with patch.object(
        module,
        "module",
        side_effect=lambda name: (
            NativeWindow() if name == "pearl-window" else original(name)
        ),
    ):
        suite = unittest.TestSuite(
            module.PearlTests(name)
            for name in (
                "test_window_identity_requires_both_owned_title_and_process",
                "test_gallery_fit_respects_logical_screen_and_maximized_windows",
            )
        )
        result = unittest.TextTestRunner().run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
