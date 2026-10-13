from qml_source import install_i18n

"""Actual media page with native Qt controls and service contracts."""

import os
from pathlib import Path
import shutil
import subprocess
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
    (fixtures / "ActionButton.qml").write_text(
        (SHELL / "widgets/ActionButton.qml")
        .read_text()
        .replace("import qs.services", 'import "."')
    )
    for name, body in {
        "NacreMediaDetails": 'property var palette:({});property var lines:[];property var spectrum:[];property string message:""',
        "NacreDashboardState": "property var preferences:({albumPalette:true});function save(key,value){preferences=Object.assign({},preferences,{[key]:value})}",
        "NacreAudio": "property var outputs:[];property var sink:null;function chooseOutput(id){sink=outputs.find(node=>String(node.id)===id);return !!sink}",
    }.items():
        (fixtures / (name + ".qml")).write_text(
            "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
        )
        with (fixtures / "qmldir").open("a") as manifest:
            manifest.write(f"\nsingleton {name} 1.0 {name}.qml\n")
    (fixtures / "NacreScrollBar.qml").write_text(
        (SHELL / "widgets/NacreScrollBar.qml").read_text()
    )
    with (fixtures / "qmldir").open("a") as manifest:
        manifest.write("\nNacreScrollBar 1.0 NacreScrollBar.qml\n")
    for name in ("NacreMediaPage",):
        source = (SHELL / "modules/dashboard" / (name + ".qml")).read_text()
        source = source.replace("import Quickshell.Services.Mpris", "").replace(
            "import Quickshell", ""
        )
        source = source.replace("import qs.widgets", 'import "fixtures"').replace(
            "import qs.services", ""
        )
        source = source.replace(
            'Quickshell.iconPath(segment.modelData.desktopEntry || "multimedia-player", true)',
            '""',
        )
        source = source.replace(
            "required property PersistentProperties visibilities",
            "required property var visibilities",
        )
        for enum_name, value in (("None", "0"), ("Track", "1"), ("Playlist", "2")):
            source = source.replace("MprisLoopState." + enum_name, value)
        (target / (name + ".qml")).write_text(source)
    (target / "media").mkdir(exist_ok=True)
    for path in (SHELL / "modules/dashboard/media").iterdir():
        if path.suffix == ".js":
            shutil.copy2(path, target / "media" / path.name)
        else:
            source = (
                path.read_text()
                .replace("import qs.widgets", 'import "../fixtures"')
                .replace("import qs.services", "")
            )
            (target / "media" / path.name).write_text(source)


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
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
