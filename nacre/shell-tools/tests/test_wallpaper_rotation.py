"""Exercise the production rotation timer and shuffle logic in native Qt."""

import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest
from qml_source import remove_objects

ROOT = Path(__file__).resolve().parents[1]


class RotationTests(unittest.TestCase):
    def test_production_timer_and_shuffle(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            shutil.copytree(ROOT / "tests/qml/fixtures", target / "fixtures")
            source = (ROOT.parent / "shell/services/NacreWallpapers.qml").read_text()
            source = (
                source.replace("pragma Singleton", "")
                .replace("import qs.utils", 'import "fixtures"')
                .replace("import Quickshell.Io", "")
                .replace("import Quickshell", "")
                .replace("Singleton {", "Item {")
            )
            source = source.replace("../utils/scripts/", "").replace(
                "Component.onCompleted: refresh()", ""
            )
            shutil.copy2(
                ROOT.parent / "shell/services/app-search.js", target / "app-search.js"
            )
            source = remove_objects(
                source,
                r"\b(Process|NacrePaletteTask|FileView|IpcHandler|Variants)\s*\{",
            )
            source = remove_objects(source, r"\bTimer\s*\{\s*interval:\s*2500\b")
            source = re.sub(
                r"property var list: \[\]",
                'property var list: [{path:"a",dynamic:false},{path:"b",dynamic:true},{path:"c",dynamic:false},{path:"d",dynamic:true}]',
                source,
            )
            source = source.replace('Quickshell.env("HOME")', '"/tmp"')
            stubs = """
                property QtObject commit: QtObject {property bool running:false;property string requestPath:"";property string previousPath:""}
                property QtObject demoRestore: QtObject {property bool running:false;property string token:"";property bool received:false}
                property QtObject prefWorker: QtObject {property bool running:false;property var value:({})}
                property QtObject catalog: QtObject {property bool running:false}
                property QtObject importer: QtObject {property bool running:false;property var files;property var command:[]}
            """
            source = source.replace("id: root", "id: root\n" + stubs, 1)
            (target / "RotatingWalls.qml").write_text(source)
            for name in ["wallpaper-rotation.js"]:
                shutil.copy2(ROOT.parent / "shell/utils/scripts" / name, target / name)
            services = {
                "NacreWelcomeApp": 'property bool active:false;property string page:"home";property var demoEntries:[]',
                "NacrePaths": 'property string state:"/tmp";property string pictures:"/tmp"',
                "NacrePresentation": "property var pending:({});property var active:({})",
                "WallpaperPlayback": "property bool sleeping:false;property bool locked:false;property bool paused:false",
                "NacrePanelState": (
                    "property var screens:({});"
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
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
