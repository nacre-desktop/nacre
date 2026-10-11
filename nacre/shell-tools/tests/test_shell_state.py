"""Actual root routing, IPC helpers, compatibility maps and shortcut methods."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from qml_source import remove_objects

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class ShellStateTests(unittest.TestCase):
    def test_router_bridge_shortcuts_and_mutable_compatibility(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            definitions = {
                "NacreHyprland": 'property var focusedMonitor:({name:"one"});property int activeWsId:2;property var clients:[];property var workspaces:({values:[{},{}]});property var calls:[];function dispatch(command){calls=[...calls,command]}',
                "NacreHoverIntent": "property var dismissed:[];function dismiss(screen){dismissed=[...dismissed,screen.name]}",
                "DesktopSettings": "property var data:({animations:false,clickEdgeMenus:false})",
                "DisplayRecovery": "property var restores:[];function restoreSaved(value){restores=[...restores,value]}",
                "Environment": 'property var screens:[{name:"one"},{name:"two"}]',
            }
            (fixtures / "Process.qml").write_text(
                "import QtQuick\nQtObject {property var command:[];property bool running:false;signal exited(int exitCode,int exitStatus)}"
            )
            with (fixtures / "qmldir").open("w") as manifest:
                manifest.write("Process 1.0 Process.qml\n")
                for name, body in definitions.items():
                    (fixtures / (name + ".qml")).write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                    )
                    manifest.write(f"singleton {name} 1.0 {name}.qml\n")
                for name in ("NacrePanelState", "Visibilities", "NacreSettingsApp"):
                    source = (
                        (SHELL / "services" / (name + ".qml"))
                        .read_text()
                        .replace("import Quickshell.Io", "")
                        .replace("import Quickshell", "")
                        .replace(
                            "Singleton {",
                            "QtObject { default property list<QtObject> objects;"
                            if name == "NacreSettingsApp"
                            else "Item {",
                        )
                        .replace("Quickshell.screens", "Environment.screens")
                        .replace("Quickshell.processId", "0")
                        .replace('Quickshell.env("HOME")', '"/fixture"')
                        .replace(
                            'import "../modules/settings/settings-catalog.js" as Catalog',
                            'import "settings-catalog.js" as Catalog',
                        )
                    )
                    source = remove_objects(source, r"\bIpcHandler\s*\{")
                    if name == "NacreSettingsApp":
                        source = source.replace(
                            "    Timer {", "    property Timer focusTimer: Timer {"
                        ).replace(
                            "    Process {", "    property Process job: Process {"
                        )
                    source = source.replace(
                        "import QtQuick", 'import QtQuick\nimport "."', 1
                    )
                    (fixtures / (name + ".qml")).write_text(source)
                    manifest.write(f"singleton {name} 1.0 {name}.qml\n")
            shutil.copy2(
                SHELL / "modules/settings/settings-catalog.js",
                fixtures / "settings-catalog.js",
            )
            for name in ("NacreShellIpc", "NacreShellShortcuts"):
                source = (SHELL / "modules" / (name + ".qml")).read_text()
                source = (
                    source.replace("import Quickshell.Io", "")
                    .replace("import Quickshell", "")
                    .replace("import qs.widgets", "")
                    .replace("import qs.services", 'import "fixtures"')
                    .replace("Scope {", "Item {")
                    .replace("Quickshell.screens", "Environment.screens")
                    .replace("Quickshell.processId", "0")
                    .replace('Quickshell.env("HOME")', '"/fixture"')
                    .replace(
                        'import "../modules/settings/settings-catalog.js" as Catalog',
                        'import "settings-catalog.js" as Catalog',
                    )
                )
                source = remove_objects(source, r"\b(IpcHandler|NacreShortcut)\s*\{")
                (target / (name + ".qml")).write_text(source)
            shutil.copy2(
                ROOT / "tests/state-qml/tst_state.qml", target / "tst_state.qml"
            )
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=25,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
