from qml_source import install_i18n, install_paths

"""Actual sidebar QML isolates asynchronous attachment results by composer."""

import os
from pathlib import Path
import subprocess
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
            source = (ROOT.parent / "shell/services/NacreChatSession.qml").read_text()
            source = (
                source.replace("pragma Singleton", "")
                .replace("import Quickshell.Io", 'import "fixtures"')
                .replace("import Quickshell", "")
                .replace("import qs.utils", 'import "fixtures"')
                .replace("Scope {", "Item {")
                .replace('Quickshell.env("HOME")', '"/isolated"')
            )
            (target / "SidebarService.qml").write_text(source)
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
            install_paths(fixtures)
            result = subprocess.run(
                [str(runner), "-input", str(target)],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                text=True,
                capture_output=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
