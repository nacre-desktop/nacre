"""Exercise actual battery policy/owner without D-Bus, notifications or private files."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class BatteryAlertTests(unittest.TestCase):
    def test_native_owner_policy_persistence_and_delivery(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            definitions = {
                "UPower": """pragma Singleton
import QtQuick
QtObject {
 property bool onBattery:true
 property var displayDevice: QtObject {
  property bool ready:true
  property bool isLaptopBattery:true
  property bool isPresent:true
  property int state:2
  property real percentage:1
 }
}""",
                "UPowerDeviceState": "import QtQuick\nQtObject {enum Value {Unknown, Charging, Discharging, Empty, FullyCharged, PendingCharge, PendingDischarge}}",
                "FileViewError": "import QtQuick\nQtObject {enum Value {Success, FileNotFound, PermissionDenied}}",
                "NacrePaths": 'pragma Singleton\nimport QtQuick\nQtObject {property string state:"/fixture"}',
                "FileView": """import QtQuick
Item {
 property string path:""
 property bool printErrors:false
 property bool atomicWrites:true
 property bool watchChanges:false
 property string textValue:""
 property int writes:0
 signal loaded()
 signal loadFailed(int error)
 signal saved()
 signal saveFailed()
 function text(){return textValue}
 function setText(value){textValue=value;writes++}
}""",
                "Process": "import QtQuick\nItem {property var command:[];property bool running:false;signal exited(int code,int status)}",
                "IpcHandler": 'import QtQuick\nItem {property string target:""}',
            }
            entries = []
            for name, text in definitions.items():
                (fixtures / (name + ".qml")).write_text(text)
                entries.append(
                    ("singleton " if name in ("UPower", "NacrePaths") else "")
                    + name
                    + " 1.0 "
                    + name
                    + ".qml"
                )
            (fixtures / "qmldir").write_text("\n".join(entries) + "\n")
            source = (ROOT.parent / "shell/services/NacreBatteryAlerts.qml").read_text()
            source = source.replace("pragma Singleton", "").replace(
                "Singleton {", "Item {"
            )
            for line in (
                "import Quickshell",
                "import Quickshell.Io",
                "import Quickshell.Services.UPower",
                "import qs.utils",
            ):
                source = source.replace(line + "\n", "")
            (target / "NacreBatteryAlerts.qml").write_text(
                'import "fixtures"\n' + source
            )
            shutil.copy2(
                ROOT.parent / "shell/services/NacreBatteryAlertPolicy.qml", target
            )
            shutil.copy2(
                ROOT / "tests/battery-qml/tst_battery.qml", target / "tst_battery.qml"
            )
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                text=True,
                capture_output=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
