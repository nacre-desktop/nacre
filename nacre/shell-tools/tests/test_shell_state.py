from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Actual root routing, IPC helpers, compatibility maps and shortcut methods."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest


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
                "NacrePearls": "property var views:[]",
                "NacreCompositor": 'property var focusedMonitor:({name:"one"});property int activeWsId:2;property var windows:[];property var workspaces:({values:[{},{}]});function createShortcut(owner,params){return null}property var calls:[];function switchWorkspace(id){calls=[...calls,"workspace "+id]}function focusWindow(id){calls=[...calls,"focus "+id]}function logout(){calls=[...calls,"logout"]}function hideWindowCommand(id){return ["python3","/fixture/window-hide.py","hide","--window",id,"--quiet"]}',
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
                for name in ("NacrePanelState", "NacreSettingsApp"):
                    stage_qml(
                        SHELL / "services" / (name + ".qml"), fixtures / (name + ".qml")
                    )
                    manifest.write(f"singleton {name} 1.0 {name}.qml\n")
            for name in ("NacrePanelState", "NacreSettingsApp"):
                stage_qml(
                    SHELL / "services" / (name + ".qml"), fixtures / (name + ".qml")
                )
            shutil.copy2(
                SHELL / "modules/settings/settings-catalog.js",
                fixtures / "settings-catalog.js",
            )
            for name in ("NacreShellIpc", "NacreShellShortcuts"):
                stage_qml(SHELL / "modules" / (name + ".qml"), target / (name + ".qml"))
            shutil.copy2(
                ROOT / "tests/state-qml/tst_state.qml", target / "tst_state.qml"
            )
            install_i18n(fixtures)
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=25,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
