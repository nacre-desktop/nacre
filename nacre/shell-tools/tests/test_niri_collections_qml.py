"""Real Niri daemon snapshot plumbing into the QML desk badge."""

import os
from pathlib import Path
import tempfile
import unittest

from qml_fixtures import (
    install_foundation_interaction,
    install_i18n,
    run_qml,
    stage_qml,
)

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"
BACKENDS = SHELL / "services/compositor"


class NiriCollectionQmlTests(unittest.TestCase):
    def test_daemon_snapshots_reach_output_desk_badge(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            (fixtures / "qmldir").write_text(
                "Process 1.0 Process.qml\n"
                "SplitParser 1.0 SplitParser.qml\n"
                "StdioCollector 1.0 StdioCollector.qml\n"
                "IpcHandler 1.0 IpcHandler.qml\n"
            )
            (fixtures / "Process.qml").write_text(
                'import QtQuick\nQtObject {property var command:[];'
                'property var environment:({});property bool running:false;'
                'property bool stdinEnabled:false;property var writes:[];'
                'property QtObject stdout;property QtObject stderr;'
                'signal started();signal exited(int exitCode,int exitStatus);'
                'function write(data){writes=[...writes,data]}'
                'onRunningChanged:if(running)Qt.callLater(started)}'
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
            (fixtures / "NacreDaemon.qml").write_text(
                '''pragma Singleton
import QtQuick
QtObject {
 id:root
 property bool enabled:false
 property bool connected:false
 property string boot:"fixture-boot"
 property var topics:({})
 signal event(string topic,string name,var data,double sequence)
 signal lost(string topic)
 signal newBoot(string id)
 property Component topicFactory:Component{QtObject{property string name:"";property var data:({available:false});property bool fresh:false;property int users:0}}
 function topic(name){if(!topics[name])topics[name]=topicFactory.createObject(root,{name:name});return topics[name]}
 function subscribe(name){topic(name).users++}
 function unsubscribe(name){topic(name).users=Math.max(0,topic(name).users-1)}
 function resync(value){if(value)value.fresh=false}
 function registerIpc(name,handler,signatures){return true}
 function unregisterIpc(name,handler){}
 function call(method,params,callback){if(method==="niri.source")Qt.callLater(()=>callback({connection_id:"fixture-connection"},null));return 1}
}'''
            )
            (fixtures / "NacreCompositor.qml").write_text(
                '''pragma Singleton
import QtQuick
QtObject {
 property var sourceBackend:null
 readonly property var capabilities:({perOutputWorkspaces:true,workspaceCollections:sourceBackend?sourceBackend.collectionsSupported:false})
 property var windows:[]
 property int activeWsId:1
 property var workspaces:({values:[]})
 property var monitors:({values:[]})
 property var focusedMonitor:({name:"DP-1"})
 readonly property var collections:sourceBackend?sourceBackend.collections:[]
 readonly property string activeCollectionId:sourceBackend?sourceBackend.activeCollectionId:""
}'''
            )
            providers = {
                "NacreColours": 'property bool light:false;property bool publishing:false;property var palette:({m3surface:"#202124",m3surfaceContainer:"#292a2d",m3surfaceContainerHigh:"#343539",m3surfaceContainerHighest:"#414247",m3primary:"#8ab4f8",m3secondary:"#c58af9",m3tertiary:"#f28b82",m3onSurface:"#f1f3f4",m3onSurfaceVariant:"#bdc1c6",m3outline:"#8a8d91",m3outlineVariant:"#5f6368",m3frame:"#202124"})',
                "DesktopSettings": 'property var data:({animations:true,workspaceWindowHints:true,interfaceFont:"Arial",monoFont:"monospace"})',
                "NacreBar": "property int workspaceCount:6;property var workspaceIcons:[]",
                "NacrePresentation": 'property var active:({workspaceColors:false})',
            }
            with (fixtures / "qmldir").open("a") as manifest:
                for name in ("NacreDaemon", "NacreCompositor"):
                    manifest.write(f"singleton {name} 1.0 {name}.qml\n")
                for name, body in providers.items():
                    (fixtures / f"{name}.qml").write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                    )
                    manifest.write(f"singleton {name} 1.0 {name}.qml\n")
            install_foundation_interaction(fixtures, SHELL / "widgets")
            stage_qml(BACKENDS / "NiriBackend.qml", target / "NiriBackend.qml")
            stage_qml(
                SHELL / "modules/topbar/NacreWorkspaceRow.qml",
                target / "NacreWorkspaceRow.qml",
            )
            install_i18n(fixtures)
            for name in ("NiriWindow.qml", "NiriShortcut.qml", "session.js"):
                (target / name).write_text((BACKENDS / name).read_text())
            (target / "tst_collections.qml").write_text(
                '''import QtQuick
import QtTest
import Quickshell
import qs.services as Services
 TestCase {
  id:test
  name:"NiriCollectionBadge"
  width:960
  height:720
  visible:true
  when:windowShown
  property var backend: null
  function init(){backend=NiriBackend;verify(backend.accept({type:"snapshot",windows:[],workspaces:[],monitors:[],capabilities:{workspaceCollections:false,perOutputWorkspaces:true}}))}
  function cleanup(){Services.NacreDaemon.enabled=false;Services.NacreDaemon.connected=false;Services.NacreDaemon.boot="fixture-boot";Services.NacreCompositor.sourceBackend=null;for(const name of Object.keys(Services.NacreDaemon.topics)){const topic=Services.NacreDaemon.topics[name];topic.fresh=false;topic.data={available:false};topic.users=0}backend=null}
  function test_daemon_refresh_rename_focus_remove_reconnect_and_stock(){
   const stateTopic=Services.NacreDaemon.topic("compositor"),eventTopic=Services.NacreDaemon.topic("compositor.events")
   const source={available:true,connectionId:"fixture-connection",edgeTriggersActive:false}
   const rows=[{id:"niri-collection:17",index:1,name:"home",output:"eDP-1",active:false},{id:"niri-collection:28",index:2,name:"studio",output:"DP-1",active:true},{id:"niri-collection:39",index:3,name:null,output:null,active:false}]
   function state(collections,capability=true){return {available:true,backend:"niri",capabilities:{workspaceCollections:capability,perOutputWorkspaces:true},outputs:{"DP-1":{name:"DP-1",focused:true}},outputOrder:["DP-1"],workspaces:{},workspaceOrder:[],windows:{},windowOrder:[],focus:{outputId:"DP-1",workspaceId:""},collections:collections,activeCollection:collections.find(row=>row.active)?.id||""}}
   stateTopic.data=state(rows,false);stateTopic.fresh=true;eventTopic.data=source;eventTopic.fresh=true
   Services.NacreDaemon.enabled=true;Services.NacreDaemon.connected=true
   tryVerify(()=>backend.connected&&backend.collections.length===0)
   Services.NacreCompositor.sourceBackend=backend
   const component=Qt.createComponent("production/modules/topbar/NacreWorkspaceRow.qml")
   tryVerify(()=>component.status===Component.Ready)
   const view=createTemporaryObject(component,test,{outputName:"DP-1",width:420});verify(view)
   verify(!findChild(view,"collectionDeskBadge").visible)
   stateTopic.data=state(rows,true);tryVerify(()=>backend.collections.length===3)
   const badge=findChild(view,"collectionDeskBadge")
   tryVerify(()=>badge.visible)
   compare(view.deskBadgeText,"2/3");compare(view.deskLabel,"studio")
   const renamed=rows.map(row=>row.id==="niri-collection:28"?Object.assign({},row,{name:"music"}):row)
   stateTopic.data=state(renamed,true);tryVerify(()=>backend.collections[1].name==="music");tryVerify(()=>view.deskLabel==="music")
   const focused=renamed.map(row=>Object.assign({},row,{active:row.id==="niri-collection:39",output:row.id==="niri-collection:39"?"DP-1":row.id==="niri-collection:28"?null:row.output}))
   stateTopic.data=state(focused,true);tryVerify(()=>backend.collections[2].output==="DP-1");tryVerify(()=>view.deskLabel==="3"&&view.deskBadgeText==="3/3")
   stateTopic.data=state([renamed[1]],true);tryVerify(()=>backend.collections.length===1);tryVerify(()=>!badge.visible)
   Services.NacreDaemon.connected=false;tryVerify(()=>backend.collections.length===0);tryVerify(()=>!badge.visible)
   Services.NacreDaemon.boot="fixture-reconnected";Services.NacreDaemon.connected=true;stateTopic.data=state(rows,true);tryVerify(()=>backend.collections.length===3);tryVerify(()=>badge.visible)
   stateTopic.data=state([],false);tryVerify(()=>backend.collections.length===0);tryVerify(()=>!badge.visible)
   Services.NacreDaemon.enabled=false
  }
 }
}'''
            )
            result = run_qml(
                [str(runner), "-platform", "offscreen", "-input", str(target)],
                capture_output=True,
                text=True,
                timeout=40,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            for line in result.stdout.splitlines():
                if line.startswith("Totals:"):
                    print(line, flush=True)


if __name__ == "__main__":
    unittest.main()
