"""Actual lock presentation snapshot uses native battery fraction correctly."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class LockSnapshotTests(unittest.TestCase):
    def test_battery_payload_is_percent_and_unavailable_is_null(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            definitions = {
                "NacrePlayers": "property var active:null",
                "NacrePresentation": 'property var active:({colours:{},poster:""})',
                "NacreWallpapers": 'property string poster:""',
                "NacreSystemUsage": "property real cpuPerc:0;property real memPerc:0;property real storagePerc:0;property real cpuTemp:NaN",
                "NacreTime": "property int hours:12",
                "DesktopSettings": "property var data:({})",
                "NacreWeather": 'property string location:"";property string icon:"";property string detail:"";property string range:"";property string description:"";property string displayTemperature:"";property bool stale:true;property string error:""',
                "NacreNotifs": "property var retained:[]",
                "UPower": "property var displayDevice:({percentage:1,ready:true,isLaptopBattery:true})",
            }
            for name, fields in definitions.items():
                (fixtures / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + fields + "}"
                )
            (fixtures / "Process.qml").write_text(
                "import QtQuick\nQtObject {property bool running:false;property bool stdinEnabled:false;property var command:[];signal started();signal exited();function write(value){}}"
            )
            (fixtures / "IpcHandler.qml").write_text(
                'import QtQuick\nQtObject {property string target:""}'
            )
            (fixtures / "qmldir").write_text(
                "\n".join(
                    [
                        *(f"singleton {name} 1.0 {name}.qml" for name in definitions),
                        "Process 1.0 Process.qml",
                        "IpcHandler 1.0 IpcHandler.qml",
                    ]
                )
            )
            source = (
                (ROOT.parent / "shell/services/LockWidgets.qml")
                .read_text()
                .replace("pragma Singleton", "")
                .replace("import Quickshell.Services.UPower", 'import "fixtures"')
                .replace("import Quickshell.Io", "")
                .replace("import Quickshell", "")
                .replace("Singleton {", "Item {")
                .replace('Quickshell.env("HOME")', '"/isolated"')
            )
            (target / "LockSnapshot.qml").write_text(source)
            (target / "tst_lock.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
Item {
 width:400;height:300
 LockSnapshot {id:view}
 TestCase {
  name:"LockBattery";when:windowShown
  function test_percent_and_unavailable() {
   for(const fraction of [0,.5,1]) {
    UPower.displayDevice={percentage:fraction,ready:true,isLaptopBattery:true};
    compare(view.snapshot().battery,Math.round(fraction*100));
   }
   UPower.displayDevice={percentage:NaN,ready:false,isLaptopBattery:false};
   compare(view.snapshot().battery,null);
  }
 }
}""")
            result = subprocess.run(
                [str(runner), "-input", str(target)],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
