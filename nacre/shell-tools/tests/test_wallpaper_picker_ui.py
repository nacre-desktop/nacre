from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n
from qml_fixtures import install_foundation_interaction
import os
import shutil
import tempfile
import unittest
from pathlib import Path

try:
    from PIL import Image
except ImportError:
    Image = None

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipIf(Image is None, "Pillow unavailable")
class WallpaperPickerUITests(unittest.TestCase):
    def test_filter_search_layouts_and_hex_hit_testing(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            shutil.copytree(ROOT / "tests/qml/fixtures", target / "fixtures")
            (target / "fixtures/WallpaperPlayback.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject{property bool locked:false;property bool sleeping:false;property bool batteryPaused:false}"
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write(
                    "\nsingleton WallpaperPlayback 1.0 WallpaperPlayback.qml\n"
                )
            Image.new("RGB", (32, 24), "blue").save(target / "poster.png")
            Image.new("RGB", (32, 24), "green").save(target / "second.png")
            stage_qml(
                ROOT.parent / "shell/modules/launcher/NacreWallpaperBackdrop.qml",
                target / "NacreWallpaperBackdrop.qml",
            )
            stage_qml(
                ROOT.parent / "shell/services/NacrePresentation.qml",
                target / "NacrePresentation.qml",
            )
            shutil.copy2(
                ROOT.parent / "shell/services/colour-data.js", target / "colour-data.js"
            )
            for name in (
                "NacreWallpaperPicker",
                "NacreWallpaperHex",
                "NacreWallpaperMotion",
                "NacreWallpaperVideo",
            ):
                stage_qml(
                    ROOT.parent / "shell/modules/launcher" / (name + ".qml"),
                    target / (name + ".qml"),
                )
            shutil.copy2(
                ROOT.parent / "shell/widgets/FastScroll.qml",
                target / "fixtures/FastScroll.qml",
            )
            (target / "fixtures/NacreIcon.qml").write_text("import QtQuick\nText {}")
            (target / "fixtures/NacreWallpapers.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property bool loading:false;property string current:"one";property string error:"";property var preferences:({kind:"static",layout:"carousel"});property string browsed:"";property var list:[{path:"one",name:"Expedition33 Monolith",poster:"'
                + str(target / "poster.png")
                + '",dynamic:false},{path:"two",name:"Hollow Knight",poster:"'
                + str(target / "poster.png")
                + '",dynamic:false},{path:"three",name:"Final Fantasy Video",poster:"'
                + str(target / "poster.png")
                + '",dynamic:true}];function browse(path){browsed=path;current=path} function setWallpaper(path){browse(path)} function commitSelection(){} function pickFiles(){} function addFiles(paths){} function preference(value){preferences=Object.assign({},preferences,value)}}'
            )
            (target / "fixtures/ActionButton.qml").write_text(
                (target / "fixtures/ActionButton.qml")
                .read_text()
                .replace(
                    "property bool selected:",
                    "property bool compact:false;property bool selected:",
                )
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write(
                    "\nNacreIcon 1.0 NacreIcon.qml\nsingleton NacreWallpapers 1.0 NacreWallpapers.qml\n"
                )
            install_foundation_interaction(
                target / "fixtures", ROOT.parent / "shell/widgets"
            )
            # Ordinary Qt checks geometry/interaction only. Native rounded pixels
            # are checked with the separate Quickshell RHI renderer.
            (target / "fixtures/NacreClip.qml").write_text(
                "import QtQuick\nRectangle {clip:true;color:'transparent'}"
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write("\nNacreClip 1.0 NacreClip.qml\n")
            shutil.copy2(
                ROOT / "tests/wallpaper-qml/tst_wallpapers.qml",
                target / "tst_wallpapers.qml",
            )
            install_i18n(target / "fixtures")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                isolate=True,
                capture_output=True,
                text=True,
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
