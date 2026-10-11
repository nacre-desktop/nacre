"""Native public-Qt/KDE style regression: atomic palette refresh and app isolation."""

import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class DolphinStyleTests(unittest.TestCase):
    @unittest.skipUnless(
        shutil.which("c++")
        and Path("/usr/include/KF6/KColorScheme/KColorScheme").exists(),
        "Native Qt/KDE build tools unavailable",
    )
    def test_atomic_native_palette_replacement_and_unrelated_app_isolation(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            build = base / "build"
            build.mkdir()
            plugins = base / "plugins/styles"
            plugins.mkdir(parents=True)
            for n in ["dolphin-style.cpp", "dolphin-style.json"]:
                shutil.copy2(ROOT / n, build / n)
            subprocess.run(
                [
                    "/usr/lib/qt6/moc",
                    str(build / "dolphin-style.cpp"),
                    "-o",
                    str(build / "dolphin-style.moc"),
                ],
                check=True,
                capture_output=True,
            )
            flags = subprocess.check_output(
                ["pkg-config", "--cflags", "--libs", "Qt6Widgets"], text=True
            ).split()
            subprocess.run(
                [
                    "c++",
                    "-std=c++17",
                    "-shared",
                    "-fPIC",
                    "-Wall",
                    "-Wextra",
                    "-Werror",
                    str(build / "dolphin-style.cpp"),
                    "-o",
                    str(plugins / "libnacre-dolphin.so"),
                    *flags,
                    "-I/usr/include/KF6/KConfig",
                    "-I/usr/include/KF6/KConfigCore",
                    "-I/usr/include/KF6/KColorScheme",
                    "-lKF6ConfigCore",
                    "-lKF6ColorScheme",
                ],
                check=True,
                capture_output=True,
            )
            probe = base / "probe"
            subprocess.run(
                [
                    "c++",
                    "-fPIC",
                    str(ROOT / "tests/native/dolphin-palette.cpp"),
                    "-o",
                    str(probe),
                    *flags,
                ],
                check=True,
                capture_output=True,
            )
            spec = importlib.util.spec_from_file_location(
                "kde_palette", ROOT / "kde-palette.py"
            )
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            colors = json.loads((ROOT / "reference-style.json").read_text())["colours"]
            colors.update(primary="cc5555")
            for args in [[], ["unrelated"]]:
                home = base / ("home" + str(len(args)))
                (home / ".config").mkdir(parents=True)
                module.publish(home, colors, "breeze")
                env = {
                    **os.environ,
                    "HOME": str(home),
                    "XDG_CONFIG_HOME": str(home / ".config"),
                    "QT_QPA_PLATFORM": "offscreen",
                    "QT_QPA_PLATFORMTHEME": "xdgdesktopportal",
                    "QT_PLUGIN_PATH": str(plugins.parent),
                    "QT_STYLE_OVERRIDE": "NacreDolphin",
                }
                result = subprocess.run(
                    [str(probe), *args],
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=15,
                )
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertIn("Scoped native palette replacement passed", result.stdout)
