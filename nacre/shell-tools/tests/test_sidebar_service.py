from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Actual sidebar QML isolates asynchronous attachment results by composer."""

import os
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class SidebarServiceTests(unittest.TestCase):
    def test_attachment_result_cannot_cross_composer_context(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            definitions = {
                "Process": "property var command:[]; property bool running:false; property bool stdinEnabled:false; property QtObject stdout; signal started(); signal exited(int code,int status); function write(value){}",
                "SplitParser": 'property string splitMarker:""; signal read(string value)',
                "IpcHandler": 'property string target:""',
            }
            for name, body in definitions.items():
                (fixtures / (name + ".qml")).write_text(
                    "import QtQuick\nQtObject {" + body + "}"
                )
            (fixtures / "qmldir").write_text(
                "\n".join(f"{name} 1.0 {name}.qml" for name in definitions)
            )
            stage_qml(
                ROOT.parent / "shell/services/NacreChatSession.qml",
                target / "SidebarService.qml",
            )
            (target / "tst_sidebar.qml").write_text("""import QtQuick
import QtTest
Item {
 width:400;height:300
 SidebarService {id:service}
 TestCase {
  name:"ComposerOwnership";when:windowShown
  function test_first_connection_preserves_early_typing() {
   const worker=findChild(service,"composerWorker");
   worker.running=false;service.composerQueue=[];
   service.composerKey="";service.draft="Typed before connection";
   service.accept({type:"state",composerKey:"new-chat",features:[]});
   compare(service.draft,"Typed before connection");
   compare(worker.action,"save");compare(worker.payload.text,"Typed before connection");
   worker.running=false;worker.exited(0,0);
  }
  function test_stale_attachment_result_is_ignored() {
   service.composerKey="first";service.attachmentsSupported=true;
   service.addFiles(["/first/file.png"]);
   const worker=findChild(service,"composerWorker");verify(worker);
   verify(worker.running);
   service.addFiles(["/first/queued.png"]);
   service.composerKey="second";service.attachments=[];
   worker.stdout.read(JSON.stringify({attachments:[{path:"/first/file.png"}]}));
   compare(service.attachments.length,0);
   worker.running=false;worker.exited(0,0);
   verify(!worker.running);compare(service.composerQueue.length,0);
  }
  function daemonState(busy) {
   return {defaultProvider:"codex",inWorkspace:false,provider:"claude",account:"Default",title:"Daemon chat",threadId:"t",model:"",features:["attachments"],composerKey:"daemon-key",busy:busy,status:"",error:""};
  }
  function test_daemon_snapshot_drives_state_and_incremental_messages() {
   const topic=service.daemonChat;
   service.daemonMode=true;topic.fresh=true;
   topic.data={channels:{"":{state:daemonState(true),messages:{"0000000001":{id:"u",role:"user",text:"hi"},"0000000002":{id:"a",role:"assistant",text:"str"}}}}};
   verify(service.connected);compare(service.busy,true);compare(service.title,"Daemon chat");
   compare(service.messages.count,2);compare(service.messages.get(1).text,"str");
   let replaced=0;
   service.historyReplaced.connect(()=>replaced++);
   topic.data={channels:{"":{state:daemonState(false),messages:{"0000000001":{id:"u",role:"user",text:"hi"},"0000000002":{id:"a",role:"assistant",text:"streamed"},"0000000003":{id:"b",role:"assistant",text:"next"}}}}};
   compare(service.busy,false);compare(service.messages.count,3);
   compare(service.messages.get(1).text,"streamed");compare(replaced,0);
   // A trimmed window no longer starts with the known messages: replace the list.
   topic.data={channels:{"":{state:daemonState(false),messages:{"0000000002":{id:"a",role:"assistant",text:"streamed"},"0000000003":{id:"b",role:"assistant",text:"next"}}}}};
   compare(service.messages.count,2);compare(replaced,1);
   // The channel leaving the snapshot (worker stopped) is a lost connection.
   topic.data={channels:{}};
   compare(service.connected,false);
   service.daemonMode=false;topic.fresh=false;topic.data=({});
  }
  function test_failed_open_falls_back_to_the_socket_only_before_dispatch() {
   service.daemonMode=false;service.legacy=false;
   service.daemonOpening=true;
   service.daemonOpened(null,{code:"unavailable",message:"Assistant worker is unavailable",retry:true,dispatched:false});
   compare(service.legacy,true);compare(service.daemonOpening,false);compare(service.daemonMode,false);
   // After a successful open a later failure never switches the session to the socket.
   service.legacy=false;service.daemonMode=true;
   service.daemonOpened(null,{code:"unavailable",message:"Daemon disconnected",retry:true});
   compare(service.legacy,false);compare(service.connected,false);
   service.daemonMode=false;
  }
  function test_current_context_result_is_accepted() {
   service.composerKey="second";service.attachments=[];service.attachmentsSupported=true;
   service.addFiles(["/second/file.png"]);
   const worker=findChild(service,"composerWorker");verify(worker);
   compare(worker.action,"files");
   worker.stdout.read(JSON.stringify({attachments:[{path:"/second/file.png"}]}));
   verify(!service.error,service.error+" fromEntries="+typeof Object.fromEntries);
   compare(service.attachments.length,1);compare(service.attachments[0].path,"/second/file.png");
   worker.running=false;worker.exited(0,0);
  }
 }
}""")
            install_i18n(fixtures)
            result = run_qml(
                [str(runner), "-input", str(target)],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                text=True,
                capture_output=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
