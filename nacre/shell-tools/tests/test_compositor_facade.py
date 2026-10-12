"""Exercise the production compositor facade independently of its backend."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SHELL = Path(__file__).resolve().parents[2] / "shell"


class CompositorFacadeTests(unittest.TestCase):
    def test_state_signals_and_typed_actions_forward_without_native_access(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            methods = {
                "switchWorkspace": "id",
                "focusWindow": "id",
                "moveWindowToWorkspace": "id,workspace,follow",
                "moveWorkspaceToMonitor": "workspace,monitor",
                "closeWindow": "id",
                "floatWindow": "id,enabled",
                "pinWindow": "id",
                "moveWindow": "id,x,y",
                "resizeWindow": "id,width,height",
                "placeWindow": "id,rect",
                "hideWindowCommand": "id",
                "reloadConfiguration": "",
                "reload": "",
                "logout": "",
                "refreshToplevels": "",
                "publishPanelRegions": "pid,regions",
            }
            backend = """pragma Singleton
import QtQuick
QtObject {
    property string backendName: "fixture"
    property var capabilities: ({floatingPlacement:true})
    property var clients: []
    property var workspaces: ({values:[]})
    property var monitors: ({values:[]})
    property var activeClient: null
    property var activeWorkspace: ({id:2})
    property var focusedMonitor: null
    property point cursorPos: Qt.point(2,3)
    property string error: ""
    property var calls: []
    signal event(var value)
    signal configurationReloaded()
    signal panelGesture(string data)
"""
            for name, args in methods.items():
                backend += (
                    f"function {name}({args}){{calls=[...calls,"
                    f'{{name:"{name}",args:[{args}]}}];return true}}\n'
                )
            (fixtures / "HyprlandBackend.qml").write_text(backend + "}\n")
            (fixtures / "qmldir").write_text(
                "singleton HyprlandBackend 1.0 HyprlandBackend.qml\n"
            )
            source = (SHELL / "services/NacreCompositor.qml").read_text()
            source = (
                source.replace("pragma Singleton", "")
                .replace("import Quickshell", "")
                .replace(
                    'import "compositor" as Backend', 'import "fixtures" as Backend'
                )
                .replace("Singleton {", "Item {")
            )
            (target / "NacreCompositor.qml").write_text(source)
            (target / "tst_facade.qml").write_text(
                """import QtQuick
import QtTest
import "fixtures" as Backend
TestCase {
    name: "CompositorFacade"
    when: windowShown
    NacreCompositor { id: facade }
    SignalSpy { id: events; target:facade; signalName:"event" }
    SignalSpy { id: configurations; target:facade; signalName:"configurationReloaded" }
    SignalSpy { id: gestures; target:facade; signalName:"panelGesture" }
    function test_facade() {
        compare(facade.backendName, "fixture");
        compare(facade.activeWsId, 2);
        Backend.HyprlandBackend.clients = [{windowId:"opaque-window",captureHandle:null}];
        compare(facade.windows[0].windowId, "opaque-window");
        Backend.HyprlandBackend.activeWorkspace = {id:4};
        compare(facade.activeWsId, 4);
        Backend.HyprlandBackend.event({type:"window-focus",windowId:"opaque-window"});
        Backend.HyprlandBackend.configurationReloaded();
        Backend.HyprlandBackend.panelGesture("end");
        compare(events.count,1); compare(configurations.count,1); compare(gestures.count,1);
        verify(facade.switchWorkspace("opaque-workspace"));
        verify(facade.focusWindow("opaque-window"));
        verify(facade.moveWindowToWorkspace("opaque-window","other-workspace",true));
        verify(facade.moveWorkspaceToMonitor("opaque-workspace","output-1"));
        verify(facade.moveWindow("opaque-window",1,2));
        verify(facade.resizeWindow("opaque-window",300,200));
        verify(facade.placeWindow("opaque-window",{x:1,y:2,width:300,height:200}));
        verify(facade.closeWindow("opaque-window"));
        verify(facade.floatWindow("opaque-window"));
        verify(facade.pinWindow("opaque-window"));
        verify(facade.hideWindowCommand("opaque-window"));
        verify(facade.reloadConfiguration());
        facade.refreshWindows(); facade.reload();
        verify(facade.logout());
        verify(facade.publishPanelRegions(123,[]));
        const calls=Backend.HyprlandBackend.calls;
        compare(calls.length,16);
        compare(calls[0].args[0],"opaque-workspace");
        compare(calls[2].args[2],true);
        compare(calls[12].name,"refreshToplevels");
    }
}
"""
            )
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
