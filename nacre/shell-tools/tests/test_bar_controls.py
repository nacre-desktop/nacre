from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Actual independent bar/popout views under native Qt with safe service fixtures."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest

from qml_fixtures import install_foundation_interaction

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class BarControlsTests(unittest.TestCase):
    def test_reactive_bar_titles_status_power_profiles_and_calendar(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
            install_foundation_interaction(fixtures, SHELL / "widgets")
            for name in ("NacreSurface", "NacreText", "NacreIcon"):
                shutil.copy2(
                    SHELL / "widgets" / (name + ".qml"), fixtures / (name + ".qml")
                )
            (fixtures / "NacreAppearance.qml").write_text(
                (SHELL / "config/NacreAppearance.qml").read_text()
            )
            definitions = {
                "NacreMenu": 'property QtObject workflows:QtObject {property var data:({recording:false});property string elapsed:"1:23";property var calls:[];function work(action){calls=[...calls,action]}}',
                "NacreBar": "property var sizes:({batteryWidth:200})",
                "NacreCompositor": 'property var activeWindow:null;property var overviewHoverWindow:null;property var windows:[];property var activeWorkspaceId:null;property string backendName:"hyprland"',
                "ChatWindowTitle": "property var titles:({})",
                "NacreIcons": 'function getAppCategoryIcon(name,fallback){return name==="editor"?"code":fallback}function getNetworkIcon(strength){return strength>50?"signal_wifi_4_bar":"signal_wifi_0_bar"}',
                "NacreAudio": "property real volume:.5;property bool available:true;property bool muted:false;property var writes:[];function setVolume(value){writes=[...writes,value];volume=value}function toggleMute(){muted=!muted}",
                "NacreNetwork": "property bool wiredConnected:false;property var active:null;property bool wifiAvailable:true;property bool wifiEnabled:true",
                "NacreBluetooth": "property bool powered:false;property bool available:true",
                "NacreControlActions": "property bool busy:false;property var calls:[];function request(args){calls=[...calls,args]}",
                "NacreBrightness": "property QtObject monitor:QtObject {property bool available:true;property real brightness:.5;property var writes:[];function setBrightness(value){writes=[...writes,value];brightness=value}}function getMonitorForScreen(screen){return monitor}",
                "NacreNotifs": "property var retained:[]",
                "NacreTime": "property date date:new Date(2026,9,9,12,34)",
                "UPower": "property bool onBattery:true;property var displayDevice:({ready:true,isLaptopBattery:true,percentage:.53,timeToEmpty:5400,timeToFull:1800})",
                "PowerProfiles": 'property int profile:PowerProfile.Balanced;property bool hasPerformanceProfile:true;property string degradationReason:"";property int writes:0;onProfileChanged:writes++',
                "NacrePanelState": 'property var view:({session:false,launcher:true,dashboard:true,osd:true});property var panels:({test:{popouts:{hasCurrent:true,pinned:true}}});function getForActive(){return view}function toggleSession(){const opening=!(view.osd && view.controlSection==="session");view.launcher=false;view.dashboard=false;view.session=false;view.controlSection="session";view.osd=opening;for(const panel of Object.values(panels)){panel.popouts.hasCurrent=false;panel.popouts.pinned=false}}',
            }
            manifest = fixtures / "qmldir"
            with manifest.open("a") as stream:
                stream.write(
                    "\nNacreIcon 1.0 NacreIcon.qml\nPowerProfile 1.0 PowerProfile.qml\n"
                )
                for name, body in definitions.items():
                    (fixtures / (name + ".qml")).write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                    )
                    stream.write(f"singleton {name} 1.0 {name}.qml\n")
            (fixtures / "PowerProfile.qml").write_text(
                "import QtQuick\nQtObject {enum Kind {PowerSaver, Balanced, Performance}}\n"
            )
            shutil.copy2(
                SHELL / "modules/dashboard/overview/overview.js", target / "overview.js"
            )
            shutil.copy2(
                SHELL / "utils/scripts/window-title.js", target / "window-title.js"
            )
            for area, name in (
                ("components", "NacreActiveTitle"),
                ("components", "NacreStatusIcons"),
                ("components", "NacrePowerButton"),
                ("popouts", "NacreBatteryPopup"),
                ("popouts", "NacreCalendarPopup"),
            ):
                stage_qml(
                    SHELL / "modules/bar" / area / (name + ".qml"),
                    target / (name + ".qml"),
                )
            shutil.copy2(ROOT / "tests/bar-qml/tst_bar.qml", target / "tst_bar.qml")
            install_i18n(fixtures)
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
