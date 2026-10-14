from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Actual media page with native Qt controls and service contracts."""

import os
from pathlib import Path
from PIL import Image
import shutil
import tempfile
import unittest

from dashboard_fixtures import prepare_dashboard

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


def prepare_pages(target):
    prepare_dashboard(target)
    fixtures = target / "fixtures"
    (fixtures / "NacrePlayers.qml").write_text(
        "pragma Singleton\nimport QtQuick\nQtObject {property var list:[];property var active:null;property var manualActive:null;onManualActiveChanged:if(manualActive)active=manualActive}"
    )
    (fixtures / "NacreSystemUsage.qml").write_text(
        'pragma Singleton\nimport QtQuick\nQtObject {property real cpuPerc:.15;property real cpuTemp:23;property real gpuTemp:NaN;property real gpuPerc:0;property bool gpuUsageAvailable:false;property real memPerc:.48;property int memUsed:15309210;property int memTotal:32191283;property real storagePerc:.52;property int storageUsed:190840832;property int storageTotal:367001600;property string loadAverage:".10 .20 .30";property string kernel:"Fixture kernel"}'
    )
    stage_qml(SHELL / "widgets/ActionButton.qml", fixtures / "ActionButton.qml")
    for name, body in {
        "NacreMediaDetails": 'property int mediaSpectrumUsers:0;property var palette:({});property var lines:[];property var spectrum:[];property string message:"";property string plain:""',
        "NacreDashboardState": 'property bool weatherPhotos:false;property var focusState:({status:"idle"});property bool busy:false;readonly property bool running:focusState.status==="running";property string timerText:"24:12";property real progress:0;property var calls:[];function start(minutes){calls=[...calls,minutes];focusState={status:"running"}}function end(){calls=[...calls,"stop"];focusState={status:"idle"}}function request(action){calls=[...calls,action];focusState={status:action==="focus-pause"?"paused":"running"}}property var preferences:({albumPalette:true,focusMinutes:25});function save(key,value){preferences=Object.assign({},preferences,{[key]:value})}',
        "NacreAudio": "property var outputs:[];property var sink:null;function chooseOutput(id){sink=outputs.find(node=>String(node.id)===id);return !!sink}",
        "NacreWeatherPhoto": 'property string image:""',
        "NacreWallpapers": 'property string poster:""',
        "WallpaperPlayback": "property bool sleeping:false;property bool locked:false",
    }.items():
        (fixtures / (name + ".qml")).write_text(
            "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
        )
        with (fixtures / "qmldir").open("a") as manifest:
            manifest.write(f"\nsingleton {name} 1.0 {name}.qml\n")
    stage_qml(SHELL / "widgets/NacreScrollBar.qml", fixtures / "NacreScrollBar.qml")
    with (fixtures / "qmldir").open("a") as manifest:
        manifest.write("\nNacreScrollBar 1.0 NacreScrollBar.qml\n")
    (fixtures / "NacreWeather.qml").write_text(
        "pragma Singleton\nimport QtQuick\nQtObject {property var reading:({isDay:true,wmo:0})}"
    )
    Image.new("RGB", (32, 32), "gray").save(target / "city.png")
    for name in ("NacreMediaPage", "NacreWeatherSky"):
        stage_qml(
            SHELL / "modules/dashboard" / (name + ".qml"), target / (name + ".qml")
        )
    stage_qml(
        SHELL / "modules/dashboard/extras/NacreFocusExtra.qml",
        target / "NacreFocusExtra.qml",
    )
    (target / "media").mkdir(exist_ok=True)
    for path in (SHELL / "modules/dashboard/media").iterdir():
        if path.suffix == ".js":
            shutil.copy2(path, target / "media" / path.name)
        else:
            stage_qml(path, target / "media" / path.name)


class MediaUITests(unittest.TestCase):
    def test_control_transactions_resource_data_and_layout(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            prepare_pages(target)
            shutil.copy2(
                ROOT / "tests/media-performance-qml/tst_pages.qml",
                target / "tst_pages.qml",
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
