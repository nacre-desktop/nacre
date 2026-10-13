from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Production request owners: rapid settings, stale preview replies and teardown."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest


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
            for name in ("DesktopSettings", "NacrePaletteTask", "NacreColorsApp"):
                stage_qml(SHELL / "services" / (name + ".qml"), root / (name + ".qml"))
            (root / "tst_requests.qml").write_text("""import QtQuick
import QtTest
import "fixture-control.js" as Fakes
import "fixtures"
TestCase {
 id:test; name:"ResponsiveOwners"


 Component{id:publisher;NacrePaletteTask{}}
 function test_palette_adapters_share_one_serial_transport(){
  const first=createTemporaryObject(publisher,test), second=createTemporaryObject(publisher,test);
  second.transport=first;second.server.running=false;
  first.command=["python3","tool","select","scene"];second.command=["python3","tool","preferences"];
  second.stdinEnabled=true;second.started.connect(()=>second.write(JSON.stringify({layout:"carousel"})));
  first.receive(JSON.stringify({ready:true}));
  first.running=true;second.running=true;wait(5);
  compare(first.server.writes.length,1);compare(second.server.writes.length,0);
  compare(JSON.parse(first.server.writes[0]).action,"select");
  first.receive(JSON.stringify({done:true,code:0,result:{path:"scene"}}));wait(5);
  compare(first.server.writes.length,2);compare(JSON.parse(first.server.writes[1]).action,"preferences");
  first.receive(JSON.stringify({done:true,code:0,result:{layout:"carousel"}}));
  verify(!first.running);verify(!second.running);verify(second.ready);
 }
 function test_rapid_toggle_latest_value_and_rollback(){
  const model=DesktopSettings;
  compare(findChild(model,"settingsRefreshTimer").running,false);
  NacrePanelState.settingsVisible=true;compare(findChild(model,"settingsRefreshTimer").running,true);NacrePanelState.settingsVisible=false;
  Fakes.withProperty(model,"change").running=false;model.queue=[];model.persistedData={blur:false,gapsOut:8};
  model.set("blur",true);compare(model.data.blur,true);
  model.set("blur",false);compare(model.data.blur,false);
  model.set("blur",true);compare(model.queue.length,1);
  model.set("blur",false);compare(model.queue.length,1);compare(model.data.blur,false);
  model.accept(JSON.stringify({data:{blur:true,gapsOut:8}}));
  compare(model.data.blur,false);Fakes.withProperty(model,"change").running=false;model.finish(0);
  compare(Fakes.withProperty(model,"change").change.value,false);
  model.accept(JSON.stringify({data:{blur:false,gapsOut:8}}));Fakes.withProperty(model,"change").running=false;model.finish(0);
  compare(model.data.blur,false);compare(Object.keys(model.desired).length,0);
  model.set("gapsOut",24);compare(model.data.gapsOut,24);
  model.accept(JSON.stringify({data:{blur:false,gapsOut:8},error:"Rejected"}));Fakes.withProperty(model,"change").running=false;model.finish(1);
  compare(model.data.gapsOut,8);compare(model.message,"Rejected");
 }
 function test_preview_stream_latest_sequence_noop_and_close(){
  const model=NacreColorsApp;model.visible=true;model.choose("scene");
  compare(model.options.mode,"light");model.acceptPreview(JSON.stringify({ready:true}));
  const first=model.flightSequence;compare(model.previewRequests,1);
  model.change({personality:"harmony"});model.startPreview();compare(model.previewRequests,1);
  model.acceptPreview(JSON.stringify({sequence:first,result:{id:"stale",request:{},palette:{colours:{surface:"old"}}}}));
  compare(model.preview.id,undefined);model.startPreview();compare(model.previewRequests,2);
  model.acceptPreview(JSON.stringify({sequence:model.flightSequence,result:{id:"fresh",request:model.options,palette:{colours:{surface:"new"}}}}));
  compare(model.preview.id,"fresh");verify(model.ready);
  const generation=model.generation;model.change({personality:"harmony"});compare(model.generation,generation);
  model.close();verify(!Fakes.withProperty(model,"stdinEnabled",true).running);verify(!model.previewBusy);
  model.acceptPreview(JSON.stringify({sequence:model.flightSequence,result:{id:"late"}}));compare(model.preview.id,"fresh");
 }
}""")
            install_i18n(fixtures)
            result = run_qml(
                [str(runner), "-input", str(root), "-o", "-,txt"],
                isolate=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
