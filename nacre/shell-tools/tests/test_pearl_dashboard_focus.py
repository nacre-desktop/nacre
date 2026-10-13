from qml_source import install_i18n, install_paths

"""Actual native focus service: ownership, deadlines, no duplicate completion."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from qml_source import remove_objects

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
            source = (SHELL / "services/NacreDashboardState.qml").read_text()
            for name in ("Process", "NacreHelperTask", "FileView", "IpcHandler"):
                source = remove_objects(source, r"\b" + name + r"\s*\{")
            source = (
                source.replace("pragma Singleton\n", "")
                .replace("import Quickshell.Io", "")
                .replace("import Quickshell", 'import "fixtures"')
                .replace("import qs.utils", 'import "fixtures"')
                .replace("Singleton {", "Item {")
                .replace("Quickshell.", "Environment.")
            )
            position = source.index("    id: root")
            source = (
                source[:position]
                + """    QtObject {id: worker;objectName:"fixtureWorker";property bool running:false;property var command:[]}
"""
                + source[position:]
            )
            (target / "NacreDashboardState.qml").write_text(source)
            (target / "tst_focus.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {
 name:"PearlFocusOwnership";width:500;height:400;when:windowShown
 Component {id: service;NacreDashboardState {}}
 function init(){DesktopSettings.data={dnd:false};DesktopSettings.writes=[];Environment.commands=[];NacrePanelState.screens=({});NacrePanelState.hidden=false;}
 function test_start_keeps_prior_setting_and_finish_once(){
  const view=createTemporaryObject(service,this);const worker=findChild(view,"fixtureWorker");worker.running=false;
  view.start(25);compare(DesktopSettings.writes.length,0);
  const request=JSON.parse(worker.command[3]);compare(request.minutes,25);compare(request.dnd,false);
  const focus={status:"running",total:100,deadline:Date.now()/1000+100,previousDnd:false,ownsDnd:true};
  view.publish(JSON.stringify({preferences:view.preferences,focus:focus,calendar:{events:[]},home:{},brain:{}}));
  compare(DesktopSettings.writes.length,1);verify(DesktopSettings.data.dnd);
  view.now=focus.deadline-50;compare(view.remaining,50);compare(view.progress,.5);
  worker.running=false;view.end(true);compare(Environment.commands.length,0);
  view.publish(JSON.stringify({preferences:view.preferences,focus:{status:"done",ownsDnd:false},calendar:{events:[]},home:{},brain:{}}));
  verify(!DesktopSettings.data.dnd);compare(Environment.commands.length,1);
  view.end(true);compare(Environment.commands.length,1);
 }
 function test_failed_start_does_not_enable_dnd_or_repeat_completion(){
  const view=createTemporaryObject(service,this);const worker=findChild(view,"fixtureWorker");worker.running=false;
  view.start(25);view.publish('{"error":"Storage unavailable"}');
  compare(DesktopSettings.writes.length,0);verify(!DesktopSettings.data.dnd);
  compare(view.focusState.status,"paused");verify(!view.running);verify(!view.finishing);
  compare(Environment.commands.length,0);
 }
 function test_preexisting_or_relinquished_dnd_is_preserved(){
  const view=createTemporaryObject(service,this);const worker=findChild(view,"fixtureWorker");worker.running=false;
  DesktopSettings.data={dnd:true};DesktopSettings.writes=[];
  view.focusState={status:"running",total:100,deadline:300,previousDnd:true,ownsDnd:false};
  view.end(false);compare(DesktopSettings.writes.length,0);verify(DesktopSettings.data.dnd);
 }
 function test_manual_change_relinquishes_and_hidden_views_stop_context_sampling(){
  const view=createTemporaryObject(service,this);const worker=findChild(view,"fixtureWorker");worker.running=false;
  view.focusState={status:"running",total:100,deadline:300,previousDnd:false,ownsDnd:true};
  DesktopSettings.data={dnd:false};compare(worker.command[2],"focus-release");
  NacrePanelState.screens={test:{dashboard:true,dashboardTab:4}};verify(view.shown);
  NacrePanelState.hidden=true;verify(!view.shown);verify(!view.focusShown);NacrePanelState.hidden=false;
 }
}
""")
            install_i18n(fixtures)
            install_paths(fixtures)
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
