"""OSD events and presentation use production QML with isolated devices."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest

from qml_fixtures import (
    install_foundation_interaction,
    install_i18n,
    run_qml,
    stage_qml,
)

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class OsdTests(unittest.TestCase):
    def test_osd_family_events_and_accessible_presentation(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
            install_foundation_interaction(fixtures, SHELL / "widgets")
            definitions = {
                "NacreAudio": 'property var sink:({description:"Studio speakers",name:"speaker"});property var source:({description:"Studio microphone"});property bool available:true;property bool micAvailable:true;property real volume:.4;property real micVolume:.6;property bool muted:false;property bool micMuted:false',
                "NacreBrightness": "property QtObject device:QtObject {property bool available:true;property real brightness:.7;signal adjusted()}function getMonitorForScreen(screen){return device}",
                "NacreKeyboardLight": "property bool available:true;property real brightness:.3;signal adjusted()",
                "NacreCapsLock": "property bool active:false;property bool ready:true;signal changed(bool enabled)",
                "NacrePowerPolicy": "property bool available:true",
                "NacreControlTools": "property var data:({nightLightSupported:true,nightLightEnabled:false})",
                "NacreNotifs": "property bool dnd:false",
                "NacreOsd": "property int hideDelay:120",
                "WallpaperPlayback": "property bool locked:false",
                "NacreCompositor": 'property var focusedMonitor:({name:"test"})',
                "NacrePanelState": "property bool hidden:false",
            }
            with (fixtures / "qmldir").open("a") as manifest:
                for name, body in definitions.items():
                    (fixtures / (name + ".qml")).write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                    )
                    manifest.write(f"\nsingleton {name} 1.0 {name}.qml\n")
            for name in ["NacreOsdEvents", "NacreOsdModel", "NacreOsdPill"]:
                stage_qml(
                    SHELL / "modules/osd" / (name + ".qml"), target / (name + ".qml")
                )
            (target / "NacreLevelNotice.qml").write_text(
                'import QtQuick\nItem {required property var output;property bool shown:false;property string channel:"volume"}'
            )
            stage_qml(SHELL / "services/NacreCapsLock.qml", fixtures / "CapsReader.qml")
            led = target / "leds/input0::capslock"
            led.mkdir(parents=True)
            (led / "brightness").write_text("0\n")
            install_i18n(fixtures)
            shutil.copy2(ROOT / "tests/osd-qml/FileView.qml", fixtures / "FileView.qml")
            shutil.copy2(ROOT / "tests/osd-qml/tst_osd.qml", target / "tst_osd.qml")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
