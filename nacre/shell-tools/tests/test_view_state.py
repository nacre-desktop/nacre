from pathlib import Path
import os
import shutil
import subprocess
import tempfile
import unittest


class ViewStateTests(unittest.TestCase):
    def test_native_scroll_edit_state_and_password_exclusion(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            shutil.copyfile(
                Path(__file__).parents[2] / "shell/utils/scripts/view-state.js",
                root / "state.js",
            )
            (root / "tst_state.qml").write_text("""import QtQuick
import QtTest
import "state.js" as State
Item {
 width:500; height:400
 Item {
  id:surface; width:500; height:400
  property string liveNote:"read-only-sensitive-value"
  TextEdit {id:note;objectName:"note";readOnly:true;text:surface.liveNote}
  TextInput {id:field;objectName:"draft";text:"draft text"}
  TextInput {id:password;objectName:"password";echoMode:TextInput.Password;text:"protected-value"}
  Flickable {id:scroll;objectName:"scroll";y:50;width:200;height:200;contentHeight:1200;contentWidth:200;Rectangle {width:200;height:1200}}
 }
 TestCase {
  name:"ViewRestoration";when:windowShown
  function test_preserves_edit_and_scroll_but_not_passwords() {
   field.select(1,4);scroll.contentY=320;
   const saved=State.capture(surface);
   verify(!JSON.stringify(saved).includes("protected-value"));
   verify(!JSON.stringify(saved).includes("read-only-sensitive-value"));
   field.text="";scroll.contentY=0;
   State.restore(surface,saved);
   compare(field.text,"draft text");compare(field.selectionStart,1);compare(field.selectionEnd,4);compare(scroll.contentY,320);
  }
  function test_read_only_binding_survives_old_snapshot_restore() {
   State.restore(surface, {"root/note": {text:"old rendered text",cursor:0}});
   compare(note.text,surface.liveNote);
   surface.liveNote="new live note";
   compare(note.text,"new live note");
  }
 }
}""")
            result = subprocess.run(
                [str(runner), "-input", str(root)],
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
