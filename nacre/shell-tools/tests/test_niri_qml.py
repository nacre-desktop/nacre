"""Execute the real QML niri backend against a non-spawning process fixture."""

import json
from pathlib import Path
import tempfile
import unittest

from qml_fixtures import install_i18n, stage_qml, run_qml

ROOT = Path(__file__).resolve().parents[1]
BACKENDS = ROOT.parent / "shell/services/compositor"


class NiriQmlTests(unittest.TestCase):
    def test_event_cache_action_queue_shortcut_lifecycle_and_backend_selection(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Quick Test unavailable")
        from native_niri_state import NativeState

        fixture = json.loads(
            (ROOT / "tests/fixtures/niri-event-stream.json").read_text()
        )
        state = NativeState(fixture["outputs"])
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
                "import QtQuick\nQtObject {property var command:[];property var environment:({});property bool running:false;property bool stdinEnabled:false;property var writes:[];property QtObject stdout;property QtObject stderr;signal started();signal exited(int exitCode,int exitStatus);function write(data){writes=[...writes,data]}onRunningChanged:if(running)Qt.callLater(started)}"
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
                "pragma Singleton\nimport QtQuick\nQtObject {property var capabilities:({perOutputWorkspaces:true});property var windows:[];property int activeWsId:1}"
            )
            with (fixtures / "qmldir").open("a") as manifest:
                manifest.write("singleton NacreCompositor 1.0 NacreCompositor.qml\n")
            providers = {
                "NacrePanelState": 'property string settingsPage:"appearance";property var screens:({});function closeTransient(view){}function getForActive(){return null}',
                "NacreColours": "property bool light:false;property var palette:({})",
                "NacreTime": "property int hours:12",
                "NacreWallpapers": 'property string current:"";property string actualCurrent:"";property string pendingPoster:"";property bool displayDynamic:false;property bool displayAnimated:false;property string displayPath:"";property var appearancePreferences:({});property var preferences:({});function refresh(){}',
                "NacrePresentation": 'property int revision:0;property var active:({poster:"",mode:"dark"});function canonicalPoster(value){return value}function activate(path){return true}',
                "WallpaperPlayback": "property bool pauseCovered:true;property bool sleeping:false;property bool locked:false;property bool paused:false;property bool batteryPaused:false",
                "NacreFrame": 'property color colour:"#101419"',
            }
            with (fixtures / "qmldir").open("a") as manifest:
                for name, body in providers.items():
                    (fixtures / (name + ".qml")).write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                    )
                    manifest.write(f"singleton {name} 1.0 {name}.qml\n")
            compositor = fixtures / "NacreCompositor.qml"
            compositor.write_text(
                compositor.read_text().replace(
                    "QtObject {", "QtObject {property var monitors:({values:[]});"
                )
            )
            for app in ("NacreSettingsApp", "NacreColorsApp", "NacreWelcomeApp"):
                stage_qml(
                    ROOT.parent / "shell/services" / (app + ".qml"),
                    target / (app + "Activity.qml"),
                )
            stage_qml(
                ROOT.parent / "shell/modules/background/NacreWallpaperScene.qml",
                target / "WallpaperCoverage.qml",
            )
            stage_qml(BACKENDS / "NiriBackend.qml", target / "NiriBackend.qml")
            install_i18n(fixtures)
            for name in ("NiriWindow.qml", "NiriShortcut.qml", "session.js"):
                (target / name).write_text((BACKENDS / name).read_text())
            (target / "snapshots.js").write_text(
                "var initial = " + json.dumps(state.snapshot()) + ";\n"
            )
            (target / "tst_niri.qml").write_text("""import QtQuick
import QtTest
import Quickshell
import "session.js" as Session
import "snapshots.js" as Snapshots
import "fixtures"
Item {
 Component {id: wallpaperCoverage; WallpaperCoverage {}}
 TestCase {
  name:"NiriBackendContract"; when:windowShown
  property var backend:null
  SignalSpy {id:moveEvents;target:backend;signalName:"event"}
  function init(){NacreI18n.language="en";backend=NiriBackend;verify(backend.accept(JSON.parse(JSON.stringify(Snapshots.initial))));moveEvents.target=backend;verify(moveEvents.valid);moveEvents.clear()}
  function cleanup(){backend=null}
  function test_overview_hover_does_not_replace_focus_and_rejects_stale_targets(){
   const active=backend.activeClient
   const next=JSON.parse(JSON.stringify(Snapshots.initial))
   const hovered=next.windows.find(window=>window.windowId!==active.windowId)
   next.overviewOpen=true;next.overviewHoverSupported=true;next.overviewHoveredWindowId=hovered.windowId
   verify(backend.accept(next));compare(backend.overviewHoverWindow.windowId,hovered.windowId);compare(backend.activeClient,active)
   next.overviewHoveredWindowId="niri-window:999";verify(backend.accept(next));compare(backend.overviewHoverWindow,null)
   next.overviewHoveredWindowId=hovered.windowId;next.overviewOpen=false;verify(backend.accept(next));compare(backend.overviewHoverWindow,null);compare(backend.activeClient,active)
  }
  function test_state_identity_focus_and_reconciliation(){
   compare(backend.activeWorkspace.id,"niri-workspace:9007199254740993")
   compare(backend.activeWsId,1);compare(backend.activeClient.windowId,"niri-window:9007199254740995")
   compare(backend.activeClient.pid,123);compare(backend.focusedMonitor.name,"DP-2")
   const window=backend.activeClient
   const next=JSON.parse(JSON.stringify(Snapshots.initial));next.windows[1].title="Changed";next.activeWindow=null
   verify(backend.accept(next));compare(backend.byId[window.windowId],window);compare(window.title,"Changed");compare(backend.activeClient,null)
   next.windows=next.windows.filter(row=>row.windowId!==window.windowId);verify(backend.accept(next));verify(!backend.byId[window.windowId])
  }
  function test_native_move_ends_only_after_real_release_with_geometry(){
   moveEvents.target=backend;verify(moveEvents.valid);moveEvents.clear();
   const next=JSON.parse(JSON.stringify(Snapshots.initial));
   const row=next.windows.find(w=>w.geometryKnown);const id=row.windowId;
   row.moving=true;row.geometryKnown=false;row.workspaceId=null;
   verify(backend.accept(JSON.parse(JSON.stringify(next))));compare(moveEvents.count,1);
   compare(moveEvents.signalArguments[0][0].phase,"begin");
   wait(250);compare(moveEvents.count,1); // Holding still is not a release.
   row.moving=false;row.workspaceId="niri-workspace:9007199254740993";
   verify(backend.accept(JSON.parse(JSON.stringify(next))));compare(moveEvents.count,1); // Wait for usable end geometry.
   row.geometryKnown=true;verify(backend.accept(JSON.parse(JSON.stringify(next))));compare(moveEvents.count,2);
   compare(moveEvents.signalArguments[1][0].phase,"end");
   compare(moveEvents.signalArguments[1][0].body,row.geometry);
   row.geometry.x+=30;verify(backend.accept(JSON.parse(JSON.stringify(next))));compare(moveEvents.count,2); // Programmatic movement never docks.
   row.moving=true;row.geometryKnown=false;verify(backend.accept(JSON.parse(JSON.stringify(next))));compare(moveEvents.count,3);
   row.moving=false;row.geometryKnown=true;verify(backend.accept(JSON.parse(JSON.stringify(next))));compare(moveEvents.count,4);
   row.moving=true;row.geometryKnown=false;verify(backend.accept(next));
   next.windows=next.windows.filter(w=>w.windowId!==id);verify(backend.accept(next));
   compare(moveEvents.signalArguments[5][0].phase,"cancel");
  }
  function test_starting_during_a_move_does_not_invent_a_begin(){
   backend.connected=false;backend.movingWindows={};moveEvents.target=backend;verify(moveEvents.valid);moveEvents.clear();
   const next=JSON.parse(JSON.stringify(Snapshots.initial));const row=next.windows.find(w=>w.geometryKnown);
   row.moving=true;row.geometryKnown=false;verify(backend.accept(JSON.parse(JSON.stringify(next))));compare(moveEvents.count,0);
   row.moving=false;row.geometryKnown=true;verify(backend.accept(JSON.parse(JSON.stringify(next))));compare(moveEvents.count,0);
  }
  function test_actions_are_serialized_and_stale_ids_rejected(){
   verify(!backend.focusWindow("niri-window:404"))
   const worker=findChild(backend,"niriActionWorker");verify(worker)
   verify(backend.focusWindow("niri-window:1"));verify(worker.running)
   tryVerify(()=>worker.writes.length===1)
   compare(worker.command,["/fixture/.local/share/nacre/shell/tools/nacre-helper","compositor","serve-actions"])
   compare(JSON.parse(worker.writes[0]).argv.slice(-3),["focus_window","--window","niri-window:1"])
   verify(backend.closeWindow("niri-window:1"));compare(backend.pendingActions.length,1)
   worker.stdout.read(JSON.stringify({id:JSON.parse(worker.writes[0]).id,ok:true}));tryVerify(()=>worker.writes.length===2)
   compare(JSON.parse(worker.writes[1]).argv.slice(-3),["close_window","--window","niri-window:1"])
   verify(!backend.pinWindow("niri-window:1"));verify(!backend.capabilities.pinWindow)
   verify(!backend.placeWindow("niri-window:1",{x:0,y:0,width:-1,height:10}))
  }
  function test_persistent_worker_failure_and_invalid_response_clear_queue(){
   const worker=findChild(backend,"niriActionWorker");verify(worker);
   verify(backend.focusWindow("niri-window:1"));tryVerify(()=>worker.writes.length===1);
   verify(backend.closeWindow("niri-window:1"));compare(backend.pendingActions.length,1);
   const id=JSON.parse(worker.writes[0]).id;
   worker.stdout.read(JSON.stringify({id:id,ok:false,error:"fixture action failure"}));wait(1);
   compare(backend.pendingActions.length,0);compare(backend.activeAction,null);verify(backend.error.includes("fixture action failure"));
   verify(backend.focusWindow("niri-window:1"));tryVerify(()=>worker.writes.length===2);
   worker.stdout.read(JSON.stringify({id:"unexpected",ok:true}));wait(1);
   compare(backend.activeAction,null);compare(backend.pendingActions.length,0);verify(!worker.running);verify(backend.error.includes("Unexpected compositor action response"));
  }
  function test_persistent_worker_queue_remains_bounded(){
   const worker=findChild(backend,"niriActionWorker");verify(worker);
   verify(backend.focusWindow("niri-window:1"));tryVerify(()=>worker.writes.length===1);
   for(let index=0;index<64;index++)verify(backend.closeWindow("niri-window:1"));
   compare(backend.pendingActions.length,64);verify(!backend.closeWindow("niri-window:1"));
   worker.running=false;worker.exited(1,0);wait(1);compare(backend.activeAction,null);compare(backend.pendingActions.length,0);
  }
  function test_errors_use_actual_catalog_and_pseudo_keeps_machine_state(){
   verify(!backend.pinWindow("niri-window:1"))
   compare(backend.error,"This operation is unavailable on niri.")
   NacreI18n.language="pseudo"
   verify(!backend.pinWindow("niri-window:1"))
   verify(backend.error.startsWith("["));verify(!backend.error.includes("This operation"))
   compare(backend.activeClient.windowId,"niri-window:9007199254740995")
   compare(backend.backendName,"niri");compare(backend.activeWorkspace.id,"niri-workspace:9007199254740993")
  }
  function test_stream_disconnect_blocks_actions_and_reconnects(){
   const stream=findChild(backend,"niriEventStream");verify(stream)
   verify(backend.focusWindow("niri-window:1"));verify(backend.closeWindow("niri-window:1"));compare(backend.pendingActions.length,1)
   const worker=findChild(backend,"niriActionWorker")
   stream.running=false;stream.exited(1,0);verify(!backend.connected);compare(backend.activeClient,null)
   compare(backend.pendingActions.length,0);worker.running=false;worker.exited(0,0);wait(1);verify(!worker.running)
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
   tryCompare(shortcut,"held",false);shortcut.destroy();tryCompare(backend,"registrations",[])
  }
  function test_opaque_workspace_ids_do_not_pause_actual_app_activity_predicates(){
   for (const activity of [NacreSettingsAppActivity,NacreColorsAppActivity,NacreWelcomeAppActivity]) {
    activity.visible=true;activity.window={minimized:false};
    verify(activity.active)
    activity.window={minimized:true};verify(!activity.active)
    activity.window={minimized:false};verify(activity.active)
    NacreCompositor.capabilities={perOutputWorkspaces:false}
    NacreCompositor.windows=[{pid:Quickshell.processId,title:activity.nativeTitle,wmClass:"org.quickshell",workspace:{id:-1}}];verify(!activity.active)
    NacreCompositor.windows=[{pid:Quickshell.processId,title:activity.nativeTitle,wmClass:"org.quickshell",workspace:{id:2}}];verify(activity.active)
    NacreCompositor.capabilities={perOutputWorkspaces:true}
    activity.visible=false
   }
  }
  function test_wallpaper_coverage_preserves_both_workspace_identity_types(){
   const scene=wallpaperCoverage.createObject(null,{screenName:"test"});NacreCompositor.monitors={values:[{name:"test",activeWorkspace:{id:"niri-workspace:9007199254740993"}}]}
   NacreCompositor.windows=[{workspace:{id:"niri-workspace:9007199254740993"},fullscreen:false,floating:false}]
   verify(scene.covered);compare(scene.workspaceId,"niri-workspace:9007199254740993")
   NacreCompositor.windows=[{workspace:{id:"niri-workspace:other"},fullscreen:false,floating:false}]
   verify(!scene.covered)
   NacreCompositor.monitors={values:[{name:"test",activeWorkspace:{id:2}}]}
   NacreCompositor.windows=[{workspace:{id:2},fullscreen:false,floating:false}]
   verify(scene.covered);compare(scene.workspaceId,2)
   NacreCompositor.windows=[];scene.destroy()
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
            result = run_qml(
                [str(runner), "-platform", "offscreen", "-input", str(target)],
                isolate=True,
                capture_output=True,
                text=True,
                timeout=40,
            )
            self.assertEqual(result.returncode, 0, (result.stdout + result.stderr))


if __name__ == "__main__":
    unittest.main()
