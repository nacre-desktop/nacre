"""External entry points preserve feedback and source-only compositor imports."""

import contextlib
import importlib.machinery
import importlib.util
import io
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).parents[2]
loader = importlib.machinery.SourceFileLoader(
    "external_settings", str(ROOT / "bin/nacre-settings")
)
spec = importlib.util.spec_from_loader(loader.name, loader)
settings = importlib.util.module_from_spec(spec)
loader.exec_module(settings)


class ExternalCompositorEntrypointTests(unittest.TestCase):
    def test_settings_failure_keeps_existing_compositor_feedback(self):
        with (
            patch.object(sys, "argv", ["nacre-settings"]),
            patch.object(
                settings, "open_settings", side_effect=RuntimeError("Desktop offline")
            ),
            patch.object(settings, "compositor") as service,
            contextlib.redirect_stderr(io.StringIO()),
        ):
            self.assertEqual(settings.main(), 1)
        service.return_value.notify.assert_called_once_with(
            "3", 6000, "0", "Nacre Settings: Desktop offline", timeout=2
        )

    def test_settings_feedback_unavailable_does_not_mask_original_failure(self):
        with (
            patch.object(sys, "argv", ["nacre-settings"]),
            patch.object(
                settings, "open_settings", side_effect=RuntimeError("Desktop offline")
            ),
            patch.object(settings, "compositor") as service,
            contextlib.redirect_stderr(io.StringIO()) as feedback,
        ):
            service.return_value.notify.side_effect = subprocess.TimeoutExpired(
                "compositor", 2
            )
            self.assertEqual(settings.main(), 1)
        self.assertIn("Desktop offline", feedback.getvalue())

    def test_external_callers_have_no_native_compositor_transport(self):
        for filename in (
            "bin/nacre-settings",
            "bin/xdg-open",
            "brain/control.py",
            "tools/releases.py",
            "crates/nacre-helper/src/palette_cli.rs",
        ):
            with self.subTest(filename=filename):
                source = (ROOT / filename).read_text()
                self.assertNotIn('"hyprctl"', source)
                self.assertNotIn("hl.dispatch", source)
