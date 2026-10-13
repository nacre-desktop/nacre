from qml_source import install_i18n, install_paths

"""Production request owners: rapid settings, stale preview replies and teardown."""

import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest
from qml_source import remove_objects

TOOLS = Path(__file__).parents[1]
SHELL = TOOLS.parent / "shell"


class ResponsiveServiceTests(unittest.TestCase):
    def test_production_request_lifecycle(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            fixtures = root / "fixtures"
            shutil.copytree(TOOLS / "tests/qml/fixtures", fixtures)
            definitions = {
                "NacrePanelState": "property bool settingsVisible:false;property QtObject view:QtObject{};function getForActive(){return view;}function closeTransient(view){}",
                "NacreCompositor": "property var windows:[];property int activeWsId:1;function moveWindowToWorkspace(id,workspace){}function hideWindowCommand(id){return [id]}function focusWindow(id){}signal configurationReloaded()",
                "NacrePresentation": 'property var active:({mode:"light"})',
                "NacreWallpapers": 'property string actualCurrent:"scene";property string current:"scene";property var preferences:({paletteMode:"auto"});property var appearancePreferences:({});function refresh(){}function preference(value){}',
                "NacreTime": "property int hours:12",
                "NacreColours": "property bool light:true",
            }
            with (fixtures / "qmldir").open("a") as manifest:
                for name, body in definitions.items():
                    (fixtures / (name + ".qml")).write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject{" + body + "}"
                    )
                    manifest.write("\nsingleton " + name + " 1.0 " + name + ".qml\n")
            for name, stubs in {
                "DesktopSettings": """property QtObject worker: QtObject {property bool running:false;property var change:null;property var command:[]}
                """,
                "NacrePaletteTask": """property QtObject server:QtObject{property bool running:true;property var writes:[];function write(value){writes=[...writes,JSON.parse(value)];}}""",
                "NacreColorsApp": """property QtObject previewServer:QtObject {property bool running:false;property var writes:[];function write(value){writes=[...writes,JSON.parse(value)];}}
                property QtObject catalogJob:QtObject{property bool running:false}
                property QtObject actionJob:QtObject{property bool running:false;property var request:null;property string action:"";property string output:"";property string failure:""}
                property QtObject minJob:QtObject{property bool running:false;property var command:[]}
                property QtObject focusTimer:QtObject{function restart(){}function stop(){}}
                property QtObject debounce:QtObject{property int interval:0;function restart(){}function stop(){}}
                """,
            }.items():
                source = (SHELL / "services" / (name + ".qml")).read_text()
                source = re.sub(r"^pragma Singleton\n", "", source)
                source = (
                    source.replace("import Quickshell.Io", "")
                    .replace("import Quickshell", "")
                    .replace("import qs.services", 'import "fixtures"')
                    .replace("import qs.utils", 'import "fixtures"')
                    .replace("Singleton {", "QtObject {", 1)
                )
                if 'import "fixtures"' not in source:
                    source = 'import "fixtures"\n' + source
                source = source.replace('Quickshell.env("HOME")', '"/fixture"')
                source = source.replace(
                    "readonly property Process server: Process {", "Process {"
                )
                pattern = (
                    r"\b(Process|FileView|IpcHandler|Connections)\s*\{"
                    if name == "DesktopSettings"
                    else r"\b(Process|FileView|Timer|IpcHandler|Connections)\s*\{"
                )
                source = remove_objects(source, pattern)
                if name == "DesktopSettings":
                    source = source.replace(
                        'Timer {\n        objectName: "settingsRefreshTimer"',
                        'property Timer refreshClock: Timer {\n        objectName: "settingsRefreshTimer"',
                    )
                source = source.replace("id: root", "id: root\n" + stubs, 1)
                (root / (name + ".qml")).write_text(source)
            (root / "tst_requests.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {
 id:test; name:"ResponsiveOwners"
 Component{id:settings;DesktopSettings{}}
 Component{id:studio;NacreColorsApp{}}
 Component{id:publisher;NacrePaletteTask{}}
 function test_palette_adapters_share_one_serial_transport(){
  const first=createTemporaryObject(publisher,test), second=createTemporaryObject(publisher,test);
  second.transport=first;second.server.running=false;
  first.command=["python3","tool","select","scene"];second.command=["python3","tool","preferences"];
  second.stdinEnabled=true;second.started.connect(()=>second.write(JSON.stringify({layout:"carousel"})));
  first.receive(JSON.stringify({ready:true}));
  first.running=true;second.running=true;wait(5);
  compare(first.server.writes.length,1);compare(second.server.writes.length,0);
  compare(first.server.writes[0].action,"select");
  first.receive(JSON.stringify({done:true,code:0,result:{path:"scene"}}));wait(5);
  compare(first.server.writes.length,2);compare(first.server.writes[1].action,"preferences");
  first.receive(JSON.stringify({done:true,code:0,result:{layout:"carousel"}}));
  verify(!first.running);verify(!second.running);verify(second.ready);
 }
 function test_rapid_toggle_latest_value_and_rollback(){
  const model=createTemporaryObject(settings,test);
  compare(findChild(model,"settingsRefreshTimer").running,false);
  NacrePanelState.settingsVisible=true;compare(findChild(model,"settingsRefreshTimer").running,true);NacrePanelState.settingsVisible=false;
  model.worker.running=false;model.queue=[];model.persistedData={blur:false,gapsOut:8};
  model.set("blur",true);compare(model.data.blur,true);
  model.set("blur",false);compare(model.data.blur,false);
  model.set("blur",true);compare(model.queue.length,1);
  model.set("blur",false);compare(model.queue.length,1);compare(model.data.blur,false);
  model.accept(JSON.stringify({data:{blur:true,gapsOut:8}}));
  compare(model.data.blur,false);model.worker.running=false;model.finish(0);
  compare(model.worker.change.value,false);
  model.accept(JSON.stringify({data:{blur:false,gapsOut:8}}));model.worker.running=false;model.finish(0);
  compare(model.data.blur,false);compare(Object.keys(model.desired).length,0);
  model.set("gapsOut",24);compare(model.data.gapsOut,24);
  model.accept(JSON.stringify({data:{blur:false,gapsOut:8},error:"Rejected"}));model.worker.running=false;model.finish(1);
  compare(model.data.gapsOut,8);compare(model.message,"Rejected");
 }
 function test_preview_stream_latest_sequence_noop_and_close(){
  const model=createTemporaryObject(studio,test);model.visible=true;model.choose("scene");
  compare(model.options.mode,"light");model.acceptPreview(JSON.stringify({ready:true}));
  const first=model.flightSequence;compare(model.previewRequests,1);
  model.change({personality:"harmony"});model.startPreview();compare(model.previewRequests,1);
  model.acceptPreview(JSON.stringify({sequence:first,result:{id:"stale",request:{},palette:{colours:{surface:"old"}}}}));
  compare(model.preview.id,undefined);model.startPreview();compare(model.previewRequests,2);
  model.acceptPreview(JSON.stringify({sequence:model.flightSequence,result:{id:"fresh",request:model.options,palette:{colours:{surface:"new"}}}}));
  compare(model.preview.id,"fresh");verify(model.ready);
  const generation=model.generation;model.change({personality:"harmony"});compare(model.generation,generation);
  model.close();verify(!model.previewServer.running);verify(!model.previewBusy);
  model.acceptPreview(JSON.stringify({sequence:model.flightSequence,result:{id:"late"}}));compare(model.preview.id,"fresh");
 }
}""")
            install_i18n(fixtures)
            install_paths(fixtures)
            result = subprocess.run(
                [str(runner), "-input", str(root), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
