from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Real fresh mode loader/fallback view with isolated service and page contracts."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class LauncherPanelTests(unittest.TestCase):
    def test_loading_closing_modes_gallery_and_fallback(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            shutil.copytree(ROOT / "tests/qml/fixtures", target / "fixtures")
            for name in ("NacreLauncherPanel", "NacreSearchPanel"):
                stage_qml(
                    SHELL / "modules/launcher" / (name + ".qml"),
                    target / (name + ".qml"),
                )
            shutil.copy2(SHELL / "widgets/NacreIdleLoader.qml", target)
            (target / "extras").mkdir()
            stub = "import QtQuick\nItem {required property var visibilities;property bool detached:false;property bool closeAfterLaunch:false;property var menu:null;signal closeRequested;signal queryChanged;signal routeChanged;property string kind:KIND;property real viewportWidth:0;property real viewportHeight:0;implicitWidth:800;implicitHeight:300}"
            for mode, name in [
                ("apps", "NacreAppBrowser"),
                ("legacy", "NacreSearchPanel"),
            ]:
                if mode == "apps":
                    (target / (name + ".qml")).write_text(
                        stub.replace("KIND", '"' + mode + '"')
                    )
            for mode, name in [
                ("palette", "Palette"),
                ("overview", "Overview"),
                ("clipboard", "Clipboard"),
                ("keys", "Keybindings"),
            ]:
                (target / "extras" / (name + ".qml")).write_text(
                    stub.replace("KIND", '"' + mode + '"')
                )
            (target / "NacreWallpaperPicker.qml").write_text(
                "import QtQuick\nItem {required property var visibilities;property bool detached:false;property real viewportWidth:0;property real viewportHeight:0;property int count:3;property int currentIndex:0;implicitWidth:800;implicitHeight:360;function move(delta){currentIndex=Math.max(0,Math.min(2,currentIndex+delta))}}"
            )
            helpers = {
                "NacreMenu": "",
                "NacreTokens": "property bool motionEnabled:true",
                "NacreWallpapers": "property var preferences:({layout:'carousel'})",
                "NacreApps": "property string last:'';function fuzzyQuery(q){return [{id:'editor',name:'Editor',comment:'Code'}].filter(a=>a.name.toLowerCase().includes(q.toLowerCase()))}function launch(a){last=a.id}",
                "DesktopActions": "property string last:'';property var list:[{name:'Settings',description:'Desktop settings',action:'settings',icon:'settings'},{name:'Power menu',description:'Power',action:'power',icon:'power'}];function execute(a,v){last=a}",
            }
            for name, body in helpers.items():
                (target / "fixtures" / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                )
            colors = target / "fixtures/NacreColours.qml"
            colors.write_text(
                colors.read_text().replace(
                    '"m3onPrimary": "black",',
                    '"m3onPrimary": "black", "m3primaryContainer":"#334455", "m3onPrimaryContainer":"white",',
                )
            )
            (target / "fixtures/NacreIcon.qml").write_text("import QtQuick\nText {}")
            (target / "fixtures/NacreInteraction.qml").write_text(
                "import QtQuick\nMouseArea {anchors.fill:parent;function onClicked(){}}"
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                for name in helpers:
                    manifest.write("\nsingleton " + name + " 1.0 " + name + ".qml")
                manifest.write("\nNacreIcon 1.0 NacreIcon.qml\n")
            shutil.copy2(
                SHELL / "widgets/FastScroll.qml", target / "fixtures/FastScroll.qml"
            )
            shutil.copy2(
                ROOT / "tests/launcher-qml/tst_panel.qml", target / "tst_panel.qml"
            )
            install_i18n(target / "fixtures")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
