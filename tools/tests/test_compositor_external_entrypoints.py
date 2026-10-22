"""External entry points preserve feedback and source-only compositor imports."""

from pathlib import Path
import unittest

ROOT = Path(__file__).parents[2]


class ExternalCompositorEntrypointTests(unittest.TestCase):
    def test_external_callers_have_no_native_compositor_transport(self):
        for filename in (
            "bin/xdg-open",
            "brain/control.py",
            "tools/releases.py",
            "crates/nacre-helper/src/palette_cli.rs",
        ):
            with self.subTest(filename=filename):
                source = (ROOT / filename).read_text()
                self.assertNotIn('"hyprctl"', source)
                self.assertNotIn("hl.dispatch", source)
