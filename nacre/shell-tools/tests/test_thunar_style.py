"""Exercise actual GTK parsing and event-driven palette reload on the target host."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class ThunarStyleTests(unittest.TestCase):
    @unittest.skipUnless(
        os.environ.get("WAYLAND_DISPLAY") or os.environ.get("DISPLAY"),
        "Native GTK display unavailable",
    )
    def test_atomic_palette_update_recolors_existing_widget(self):
        if not shutil.which("cc") or not shutil.which("pkg-config"):
            self.skipTest("GTK3 compiler tools unavailable")
        query = subprocess.run(
            ["pkg-config", "--cflags", "--libs", "gtk+-3.0"],
            capture_output=True,
            text=True,
        )
        if query.returncode:
            self.skipTest("GTK3 headers unavailable")
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            gtk = directory / "gtk-3.0"
            gtk.mkdir()
            palette = gtk / "gtk.css"
            names = [
                "window_bg_color",
                "window_fg_color",
                "headerbar_bg_color",
                "view_bg_color",
                "view_fg_color",
                "sidebar_bg_color",
                "sidebar_fg_color",
                "accent_bg_color",
                "accent_fg_color",
            ]
            palette.write_text(
                "".join("@define-color " + name + " #ff0000;\n" for name in names)
            )
            module = directory / "theme.so"
            fixture = directory / "fixture"
            flags = query.stdout.split()
            subprocess.run(
                [
                    "cc",
                    "-shared",
                    "-fPIC",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    "-o",
                    str(module),
                    str(ROOT / "thunar-theme.c"),
                    *flags,
                ],
                check=True,
            )
            subprocess.run(
                [
                    "cc",
                    "-o",
                    str(fixture),
                    str(ROOT / "tests/native/thunar-style.c"),
                    *flags,
                ],
                check=True,
            )
            env = dict(
                os.environ,
                XDG_CONFIG_HOME=str(directory),
                GTK_MODULES=str(module),
                NACRE_THUNAR_STYLE=str(ROOT / "thunar.css"),
                NO_AT_BRIDGE="1",
            )
            result = subprocess.run(
                [str(fixture), str(palette)],
                env=env,
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertNotIn("Nacre Thunar style:", result.stderr)
            self.assertIn("live replacement colors passed", result.stdout)


if __name__ == "__main__":
    unittest.main()
