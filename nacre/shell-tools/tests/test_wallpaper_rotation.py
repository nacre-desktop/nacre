from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Exercise the production rotation timer and shuffle logic in native Qt."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class RotationTests(unittest.TestCase):
    def test_production_timer_and_shuffle(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            shutil.copytree(ROOT / "tests/qml/fixtures", target / "fixtures")
            shutil.copy2(
                ROOT.parent / "shell/services/app-search.js", target / "app-search.js"
            )
            stage_qml(
                ROOT.parent / "shell/services/NacreWallpapers.qml",
                target / "RotatingWalls.qml",
            )
            for name in ["wallpaper-rotation.js"]:
                shutil.copy2(ROOT.parent / "shell/utils/scripts" / name, target / name)
            services = {
                "NacreWelcomeApp": 'property bool active:false;property string page:"home";property var demoEntries:[]',
                "NacrePaths": 'property string state:"/tmp";property string pictures:"/tmp"',
                "NacrePresentation": "property var pending:({});property var active:({})",
                "WallpaperPlayback": "property bool sleeping:false;property bool locked:false;property bool paused:false",
                "NacrePanelState": (
                    "property var screens:({});readonly property var providerViews:Object.values(screens);"
                    "property bool settingsVisible:false;"
                    'property string settingsPage:"appearance"'
                ),
            }
            for name, body in services.items():
                (target / "fixtures" / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                )
            with (target / "fixtures/qmldir").open("a") as stream:
                for name in services:
                    stream.write(f"\nsingleton {name} 1.0 {name}.qml")
            shutil.copy2(
                ROOT / "tests/qml/tst_wallpaper_rotation.qml",
                target / "tst_wallpaper_rotation.qml",
            )
            install_i18n(target / "fixtures")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                isolate=True,
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
