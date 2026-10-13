"""Exercise production panel size bindings without their provider/UI children."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

from qml_source import remove_objects

SHELL = Path(__file__).resolve().parents[2] / "shell"


class RedockMotionTests(unittest.TestCase):
    def test_restored_panels_are_full_size_and_keep_future_open_close_motion(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            (target / "MotionRoot.qml").write_text("import QtQuick\nRectangle {}\n")
            fixtures = {
                "NacreTokens": 'property bool motionEnabled:true;property color body:"#101419"',
                "NacrePanelState": "property bool hidden:false",
                "NacreDashboardState": 'property var tabs:["today","weather","media","system","focus"]',
                "NacreAiViews": "property var edge:null",
                "NacreMenu": "property var entries:[]",
                "NacreAppearance": "property var anim:({durations:{normal:360}})",
            }
            manifest = ""
            for name, body in fixtures.items():
                (target / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                )
                manifest += f"singleton {name} 1.0 {name}.qml\n"
            (target / "qmldir").write_text(manifest)
            for name, path, root_type in [
                (
                    "Dashboard",
                    "modules/dashboard/NacreDashboardPanel.qml",
                    "NacreSurface",
                ),
                ("Launcher", "modules/launcher/NacreLauncherPanel.qml", "Item"),
                ("Sidebar", "modules/extras/LeftDrawer.qml", "Item"),
                (
                    "Controls",
                    "modules/controlcenter/NacreControlCenter.qml",
                    "FocusScope",
                ),
            ]:
                source = (SHELL / path).read_text()
                source = "\n".join(
                    line
                    for line in source.splitlines()
                    if not line.startswith(("import qs.", "import Quickshell"))
                )
                source = source.replace(root_type + " {", "MotionRoot {", 1)
                source = source.replace(
                    "required property PersistentProperties", "required property var"
                ).replace("required property ShellScreen", "required property var")
                source = remove_objects(source, r"\bcomponent\s+\w+\s*:\s*\w+\s*\{")
                source = remove_objects(
                    source,
                    r"\b(?!(?:MotionRoot|NumberAnimation)\b)[A-Z][A-Za-z0-9_.]*\s*\{",
                )
                if name == "Launcher":
                    source = (
                        source.rsplit("}", 1)[0]
                        + "QtObject {id:page;property var item:null}\n}"
                    )
                (target / (name + ".qml")).write_text(source + "\n")
            (target / "tst_motion.qml").write_text("""import QtQuick
import QtTest
import "."
TestCase {
 id:test;name:"RestoredPanelMotion";width:1920;height:1150;visible:true;when:windowShown
 Component {id:dashboard;Dashboard{}}
 Component {id:launcher;Launcher{}}
 Component {id:sidebar;Sidebar{}}
 Component {id:controls;Controls{}}
 Component {id:flags;QtObject{property bool dashboard:false;property bool launcher:false;property bool left:false;property int dashboardTab:0;property string launcherMode:"apps";property string launcherRoute:"home";property bool previewOnly:false}}
 function test_motion_data(){return [{tag:"dashboard",factory:dashboard,field:"dashboard",axis:"height",target:650},{tag:"launcher",factory:launcher,field:"launcher",axis:"height",target:300},{tag:"sidebar",factory:sidebar,field:"left",axis:"width",target:470},{tag:"controls",factory:controls,field:"visibility",axis:"width",target:424}];}
 function test_motion(data){
  const view=createTemporaryObject(flags,test);
  const args=data.tag==="controls"?{visibility:false,screen:{height:1200}}:{visibilities:view};
  if(data.tag==="sidebar")args.screen={height:1200};
  const panel=createTemporaryObject(data.factory,test,args);verify(panel);
  panel.restoreImmediately=true;
  if(data.tag==="controls")panel.visibility=true;else view[data.field]=true;
  if(panel.restoreAttached)panel.restoreAttached();else panel.prepareLift();
  compare(panel[data.axis],data.target);verify(!panel.opening);
  wait(25);compare(panel[data.axis],data.target);
  if(data.tag==="controls")panel.visibility=false;else view[data.field]=false;
  wait(30);verify(panel[data.axis]>0);wait(400);compare(panel[data.axis],0);
  panel.restoreImmediately=false;
  if(data.tag==="controls")panel.visibility=true;else view[data.field]=true;
  wait(30);verify(panel[data.axis]<data.target);wait(400);compare(panel[data.axis],data.target);
 }
}
""")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                timeout=20,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
