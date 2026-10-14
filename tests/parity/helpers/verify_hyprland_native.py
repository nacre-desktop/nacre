#!/usr/bin/env python3
"""Run unchanged original Hyprland assertions through native transport callbacks."""

from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]


def main():
    sys.path.insert(0, str(ROOT / "nacre/shell-tools/tests"))
    sys.path.insert(0, str(ROOT / "nacre/shell-tools"))
    import native_hyprland
    import test_compositor_backend
    import test_compositor_helper_migration

    with (
        patch.object(test_compositor_backend, "compositor", native_hyprland),
        patch.object(test_compositor_helper_migration, "compositor", native_hyprland),
    ):
        suite = unittest.defaultTestLoader.loadTestsFromTestCase(
            test_compositor_backend.CompositorBackendTests
        )
        suite.addTests(
            test_compositor_helper_migration.HelperCompositorMigrationTests(name)
            for name in (
                "test_backend_environment_preserves_explicit_desktop_and_unique_instance_policy",
                "test_performance_and_animation_semantics_are_translated_only_by_backend",
            )
        )
        result = unittest.TextTestRunner().run(suite)
    if result.skipped:
        raise RuntimeError("Hyprland assertion gate skipped a prerequisite")
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())
