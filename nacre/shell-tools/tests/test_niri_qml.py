"""Execute the real QML niri backend against a non-spawning process fixture."""

import importlib.util
import json
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
BACKENDS = ROOT.parent / "shell/services/compositor"


class NiriQmlTests(unittest.TestCase):
    def test_event_cache_action_queue_shortcut_lifecycle_and_backend_selection(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Quick Test unavailable")
        spec = importlib.util.spec_from_file_location(
            "niri_fixture_state", ROOT / "compositor_backends/niri.py"
        )
        niri = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(niri)
        fixture = json.loads(
            (ROOT / "tests/fixtures/niri-event-stream.json").read_text()
        )
        state = niri.State(fixture["outputs"])
        for event in fixture["events"][:2]:
            state.apply(event)
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            (fixtures / "qmldir").write_text(
                "Process 1.0 Process.qml\nSplitParser 1.0 SplitParser.qml\nStdioCollector 1.0 StdioCollector.qml\nIpcHandler 1.0 IpcHandler.qml\n"
            )
            (fixtures / "Process.qml").write_text(
                "import QtQuick\nQtObject {property var command:[];property var environment:({});property bool running:false;property QtObject stdout;property QtObject stderr;signal exited(int exitCode,int exitStatus)}"
            )
            (fixtures / "SplitParser.qml").write_text(
                "import QtQuick\nQtObject {signal read(string data)}"
            )
            (fixtures / "StdioCollector.qml").write_text(
                'import QtQuick\nQtObject {property string text:""}'
            )
            (fixtures / "IpcHandler.qml").write_text(
                'import QtQuick\nQtObject {property string target:""}'
            )
            (fixtures / "NacreCompositor.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var capabilities:({perOutputWorkspaces:true})}"
            )
            with (fixtures / "qmldir").open("a") as manifest:
                manifest.write("singleton NacreCompositor 1.0 NacreCompositor.qml\n")
            for app in ("NacreSettingsApp", "NacreColorsApp", "NacreWelcomeApp"):
                text = (ROOT.parent / "shell/services" / (app + ".qml")).read_text()
                expression = re.search(
                    r"readonly property bool active: ([^\n]+)", text
                ).group(1)
                (target / (app + "Activity.qml")).write_text(
                    'import QtQuick\nimport "fixtures"\nQtObject {'
                    "property bool visible:true;property var window:({minimized:false});"
                    'property var client:({workspace:{id:"niri-workspace:9007199254740993"}});'
                    "readonly property bool active:" + expression + "}"
                )
            source = (
                (BACKENDS / "NiriBackend.qml")
                .read_text()
                .replace("pragma Singleton\n", "")
                .replace("import Quickshell.Io", 'import "fixtures"')
                .replace("import Quickshell\n", "")
                .replace("Singleton {", "Item {")
            )
            source = re.sub(
                r'Quickshell.env\("([A-Z_]+)"\)',
                lambda match: '"/fixture"' if match[1] == "HOME" else '""',
                source,
            )
            (target / "NiriBackend.qml").write_text(source)
            for name in ("NiriWindow.qml", "NiriShortcut.qml", "session.js"):
                (target / name).write_text((BACKENDS / name).read_text())
            (target / "snapshots.js").write_text(
                "var initial = " + json.dumps(state.snapshot()) + ";\n"
            )
            (target / "tst_niri.qml").write_text("""import QtQuick
import QtTest
import "session.js" as Session
import "snapshots.js" as Snapshots
import "fixtures"
Item {
 Component {id: settingsActivity; NacreSettingsAppActivity {}}
 Component {id: colorsActivity; NacreColorsAppActivity {}}
 Component {id: welcomeActivity; NacreWelcomeAppActivity {}}
 Component {id: factory; NiriBackend {}}
 TestCase {
  name:"NiriBackendContract"; when:windowShown
  property var backend:null
  function init(){backend=factory.createObject(null);verify(backend.accept(JSON.parse(JSON.stringify(Snapshots.initial))))}
  function cleanup(){backend.destroy();backend=null;wait(1)}
  function test_state_identity_focus_and_reconciliation(){
   compare(backend.activeWorkspace.id,"niri-workspace:9007199254740993")
   compare(backend.activeWsId,1);compare(backend.activeClient.windowId,"niri-window:9007199254740995")
   compare(backend.activeClient.pid,123);compare(backend.focusedMonitor.name,"DP-2")
   const window=backend.activeClient
   const next=JSON.parse(JSON.stringify(Snapshots.initial));next.windows[1].title="Changed";next.activeWindow=null
   verify(backend.accept(next));compare(backend.byId[window.windowId],window);compare(window.title,"Changed");compare(backend.activeClient,null)
   next.windows=next.windows.filter(row=>row.windowId!==window.windowId);verify(backend.accept(next));verify(!backend.byId[window.windowId])
  }
  function test_actions_are_serialized_and_stale_ids_rejected(){
   verify(!backend.focusWindow("niri-window:404"))
   const worker=findChild(backend,"niriActionWorker");verify(worker)
   verify(backend.focusWindow("niri-window:1"));verify(worker.running)
   compare(worker.command.slice(-3),["focus_window","--window","niri-window:1"])
   verify(backend.closeWindow("niri-window:1"));compare(backend.pendingActions.length,1)
   worker.running=false;worker.exited(0,0);tryCompare(worker,"running",true)
   compare(worker.command.slice(-3),["close_window","--window","niri-window:1"])
   verify(!backend.pinWindow("niri-window:1"));verify(!backend.capabilities.pinWindow)
   verify(!backend.placeWindow("niri-window:1",{x:0,y:0,width:-1,height:10}))
  }
  function test_stream_disconnect_blocks_actions_and_reconnects(){
   const stream=findChild(backend,"niriEventStream");verify(stream)
   stream.running=false;stream.exited(1,0);verify(!backend.connected);compare(backend.activeClient,null)
   verify(!backend.focusWindow("niri-window:1"));tryCompare(stream,"running",true)
   verify(backend.accept(Snapshots.initial));verify(backend.connected)
  }
  function test_shortcut_registration_pulse_rename_and_retirement(){
   const shortcut=backend.createShortcut(backend,{appid:"nacre_shell",name:"test"})
   verify(shortcut);compare(backend.registrations.length,1)
   verify(backend.triggerShortcut("nacre_shell:test"));verify(shortcut.held)
   tryCompare(shortcut,"held",false)
   shortcut.params={appid:"nacre_shell",name:"other"}
   verify(!backend.triggerShortcut("nacre_shell:test"));verify(backend.triggerShortcut("nacre_shell:other"))
   shortcut.destroy();tryCompare(backend,"registrations",[])
  }
  function test_opaque_workspace_ids_do_not_pause_actual_app_activity_predicates(){
   for (const factory of [settingsActivity,colorsActivity,welcomeActivity]) {
    const activity=factory.createObject(null)
    verify(activity.active)
    activity.window={minimized:true};verify(!activity.active)
    activity.window={minimized:false};verify(activity.active)
    NacreCompositor.capabilities={perOutputWorkspaces:false}
    activity.client={workspace:{id:-1}};verify(!activity.active)
    activity.client={workspace:{id:2}};verify(activity.active)
    NacreCompositor.capabilities={perOutputWorkspaces:true}
    activity.destroy()
   }
  }
  function test_selection_preserves_hyprland_and_niri_is_explicit(){
   compare(Session.selectBackend({}),"hyprland")
   compare(Session.selectBackend({NIRI_SOCKET:"fixture"}),"niri")
   compare(Session.selectBackend({NIRI_SOCKET:"fixture",HYPRLAND_INSTANCE_SIGNATURE:"fixture"}),"hyprland")
   compare(Session.selectBackend({NACRE_COMPOSITOR:"niri",HYPRLAND_INSTANCE_SIGNATURE:"fixture"}),"niri")
   compare(Session.selectBackend({NACRE_COMPOSITOR:"invalid"}),"unsupported")
  }
 }
}
""")
            result = subprocess.run(
                [str(runner), "-platform", "offscreen", "-input", str(target)],
                capture_output=True,
                text=True,
                timeout=40,
            )
            self.assertEqual(
                result.returncode, 0, (result.stdout + result.stderr)[-9000:]
            )


if __name__ == "__main__":
    unittest.main()
