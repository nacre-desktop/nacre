from qml_fixtures import run_qml
from qml_fixtures import stage_qml

"""Exercise the production compositor facade independently of its backend."""

import os
from pathlib import Path
import tempfile
import unittest

from qml_fixtures import install_i18n

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
                "refreshState": "",
                "createShortcut": "owner,params",
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
            (fixtures / "HyprlandBackend.qml").write_text(
                backend + "Component.onCompleted: Registry.hyprlandCreated++\n}\n"
            )
            (fixtures / "NiriBackend.qml").write_text(
                backend.replace('"fixture"', '"niri-fixture"').replace(
                    "({floatingPlacement:true})",
                    "({floatingPlacement:true,perOutputWorkspaces:true})",
                )
                + "Component.onCompleted: Registry.niriCreated++\n}\n"
            )
            (fixtures / "TestEnvironment.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property var values:({});function env(name){return values[name]||""}}'
            )
            (fixtures / "Registry.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property int hyprlandCreated:0;property int niriCreated:0}"
            )
            compositor_dir = target / "compositor"
            compositor_dir.mkdir()
            stage_qml(
                SHELL / "services/compositor/session.js", compositor_dir / "session.js"
            )
            (fixtures / "qmldir").write_text(
                "singleton HyprlandBackend 1.0 HyprlandBackend.qml\nsingleton NiriBackend 1.0 NiriBackend.qml\nsingleton TestEnvironment 1.0 TestEnvironment.qml\nsingleton Registry 1.0 Registry.qml\n"
            )
            install_i18n(fixtures)
            stage_qml(
                SHELL / "services/NacreCompositor.qml", target / "NacreCompositor.qml"
            )
            (target / "tst_facade.qml").write_text(
                """import QtQuick
import QtTest
import Quickshell
import "fixtures" as Backend
TestCase {
    name: "CompositorFacade"
    when: windowShown
    readonly property var facade: NacreCompositor
    SignalSpy { id: events; target:facade; signalName:"event" }
    SignalSpy { id: configurations; target:facade; signalName:"configurationReloaded" }
    SignalSpy { id: gestures; target:facade; signalName:"panelGesture" }
    function test_facade() {
        compare(facade.backendName, "fixture");
        compare(Backend.Registry.niriCreated,0);
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
        facade.refreshWindows(); facade.refreshState();
        verify(facade.reload());
        verify(facade.createShortcut(facade,{appid:"fixture",name:"action"}));
        verify(facade.logout());
        verify(facade.publishPanelRegions(123,[]));
        const calls=Backend.HyprlandBackend.calls;
        compare(calls.length,18);
        compare(calls[0].args[0],"opaque-workspace");
        compare(calls[2].args[2],true);
        compare(calls[12].name,"refreshToplevels");
        compare(calls[13].name,"refreshState");
        compare(calls[14].name,"reload");
        compare(calls[15].name,"createShortcut");
    }
    function test_niri_output_indices_identity_and_invalid_selection(){
        compare(Backend.Registry.niriCreated,0)
        Quickshell.environment={NACRE_COMPOSITOR:"niri",HYPRLAND_INSTANCE_SIGNATURE:"legacy"}
        compare(facade.backendName,"niri-fixture");compare(Backend.Registry.niriCreated,1)
        Backend.NiriBackend.focusedMonitor={name:"DP-1"}
        Backend.NiriBackend.activeWorkspace={id:"opaque-ws-1",index:2}
        Backend.NiriBackend.workspaces={values:[{id:"opaque-ws-1",index:2,outputId:"DP-1",urgent:true},{id:"other-ws",index:2,outputId:"DP-2"}]}
        Backend.NiriBackend.clients=[{windowId:"first",workspaceId:"opaque-ws-1"},{windowId:"other",workspaceId:"other-ws"}]
        compare(facade.activeWorkspaceId,"opaque-ws-1");compare(facade.activeWsId,2)
        compare(facade.windowsForWorkspace(2).length,1);compare(facade.windowsForWorkspace(2)[0].windowId,"first")
        verify(facade.workspaceUrgent(2));compare(facade.windowsForWorkspace(1).length,0)
        Quickshell.environment={NACRE_COMPOSITOR:"invalid"}
        compare(facade.backend,null);compare(facade.backendName,"unsupported");compare(facade.error,"Unknown compositor backend.")
        compare(facade.windows.length,0);compare(Backend.Registry.hyprlandCreated,1)
    }
}
"""
            )
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                isolate=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
