from qml_fixtures import install_i18n

"""Actual native focus service: ownership, deadlines, no duplicate completion."""

import os
from pathlib import Path
import tempfile
import unittest
from qml_fixtures import stage_qml, run_qml

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class PearlFocusTests(unittest.TestCase):
    def test_native_dnd_ownership_and_wall_clock_progress(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            definitions = {
                "Environment": 'property var commands:[];function env(name){return "/tmp";}function execDetached(command){commands=[...commands,command];}',
                "NacrePanelState": "property var screens:({});readonly property var providerViews:Object.values(screens);property bool hidden:false",
                "DesktopSettings": "property var data:({dnd:false});property var writes:[];function set(key,value){writes=[...writes,{key:key,value:value}];data=Object.assign({},data,{[key]:value});}",
            }
            for name, body in definitions.items():
                (fixtures / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                )
            (fixtures / "qmldir").write_text(
                "\n".join(f"singleton {name} 1.0 {name}.qml" for name in definitions)
            )
            stage_qml(
                SHELL / "services/NacreDashboardState.qml",
                target / "NacreDashboardState.qml",
            )
            (fixtures / "NacreHelperTask.qml").write_text(
                "import QtQuick\nimport Quickshell.Io\nProcess {signal output(string text)}"
            )
            with (fixtures / "qmldir").open("a") as manifest:
                manifest.write("\nNacreHelperTask 1.0 NacreHelperTask.qml\n")
            (target / "tst_focus.qml").write_text("""import QtQuick
import QtTest
import Quickshell
import "fixture-control.js" as Fakes
import "fixtures"
TestCase {
 name:"PearlFocusOwnership";width:500;height:400;when:windowShown

 function init(){DesktopSettings.data={dnd:false};DesktopSettings.writes=[];Quickshell.launched=[];NacrePanelState.screens=({});NacrePanelState.hidden=false;}
 function test_start_keeps_prior_setting_and_finish_once(){
  const view=NacreDashboardState;const worker=Fakes.withProperty(view,"command");worker.running=false;
  view.start(25);compare(DesktopSettings.writes.length,0);
  const request=JSON.parse(worker.command[3]);compare(request.minutes,25);compare(request.dnd,false);
  const focus={status:"running",total:100,deadline:Date.now()/1000+100,previousDnd:false,ownsDnd:true};
  view.publish(JSON.stringify({preferences:view.preferences,focus:focus,calendar:{events:[]},home:{},brain:{}}));
  compare(DesktopSettings.writes.length,1);verify(DesktopSettings.data.dnd);
  view.now=focus.deadline-50;compare(view.remaining,50);compare(view.progress,.5);
  worker.running=false;view.end(true);compare(Quickshell.launched.length,0);
  view.publish(JSON.stringify({preferences:view.preferences,focus:{status:"done",ownsDnd:false},calendar:{events:[]},home:{},brain:{}}));
  verify(!DesktopSettings.data.dnd);compare(Quickshell.launched.length,1);
  view.end(true);compare(Quickshell.launched.length,1);
 }
 function test_failed_start_does_not_enable_dnd_or_repeat_completion(){
  const view=NacreDashboardState;const worker=Fakes.withProperty(view,"command");worker.running=false;
  view.start(25);view.publish('{"error":"Storage unavailable"}');
  compare(DesktopSettings.writes.length,0);verify(!DesktopSettings.data.dnd);
  compare(view.focusState.status,"paused");verify(!view.running);verify(!view.finishing);
  compare(Quickshell.launched.length,0);
 }
 function test_preexisting_or_relinquished_dnd_is_preserved(){
  const view=NacreDashboardState;const worker=Fakes.withProperty(view,"command");worker.running=false;
  DesktopSettings.data={dnd:true};DesktopSettings.writes=[];
  view.focusState={status:"running",total:100,deadline:300,previousDnd:true,ownsDnd:false};
  view.end(false);compare(DesktopSettings.writes.length,0);verify(DesktopSettings.data.dnd);
 }
 function test_manual_change_relinquishes_and_hidden_views_stop_context_sampling(){
  const view=NacreDashboardState;const worker=Fakes.withProperty(view,"command");worker.running=false;
  view.focusState={status:"running",total:100,deadline:300,previousDnd:false,ownsDnd:true};
  DesktopSettings.data={dnd:false};compare(worker.command[2],"focus-release");
  NacrePanelState.screens={test:{dashboard:true,dashboardTab:4}};verify(view.shown);
  NacrePanelState.hidden=true;verify(!view.shown);verify(!view.focusShown);NacrePanelState.hidden=false;
 }
}
""")
            install_i18n(fixtures)
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                isolate=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
