"""Actual panel host disables old controls while their closing geometry remains."""

import os
import re
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

SHELL = Path(__file__).resolve().parents[2] / "shell"


class PanelHostInputTests(unittest.TestCase):
    def test_closing_panels_keep_paint_but_release_child_input(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            fixtures = root / "fixtures"
            fixtures.mkdir()
            source = (SHELL / "modules/drawers/NacrePanelHost.qml").read_text()
            source = source.replace("import Quickshell\n", "")
            source = source.replace("import qs.config", 'import "fixtures"')
            for alias in [
                "Popouts",
                "Dashboard",
                "Extras",
                "Launcher",
                "Notifications",
                "Osd",
                "Session",
            ]:
                source = re.sub(
                    r"import qs\.modules\.[^\n]+ as " + alias,
                    'import "fixtures" as ' + alias,
                    source,
                )
            (root / "NacrePanelHost.qml").write_text(source)
            shutil.copy2(SHELL / "modules/drawers/layout.js", root / "layout.js")
            (fixtures / "NacreFrame.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property int left:10;property int right:10;property int headerHeight:50;property int bottom:10;property int rounding:20}"
            )
            manifest = ["singleton NacreFrame 1.0 NacreFrame.qml"]
            names = [
                "LeftDrawer",
                "NacreOsdPanel",
                "NacreOsdEvents",
                "NacreSessionPanel",
                "NacreDashboardPanel",
                "NacrePopupPanel",
                "NacreNotificationStack",
                "NacreLauncherPanel",
            ]
            for name in names:
                (fixtures / (name + ".qml")).write_text("""import QtQuick
Rectangle {
 width:200;height:100;color:"#334455"
 property var screen
 property var visibilities
 property string section:"home"
 property bool keyboardActive:false
 property bool visibility:false
 property bool hovered:false
 property bool hasCurrent:false
 property bool pinned:false
 property real currentCenter:0
 property real targetWidth:200
 property bool suppressed:false
 property int presses:0
 signal sectionRequested(string section)
 MouseArea {anchors.fill:parent;onPressed:parent.presses++}
}""")
                manifest.append(name + " 1.0 " + name + ".qml")
            (fixtures / "qmldir").write_text("\n".join(manifest))
            (root / "tst_host.qml").write_text("""import QtQuick
import QtTest
TestCase {
 name:"ClosingHostInput";width:500;height:400;visible:true;when:windowShown
 Component {id:scene;Item {
  width:400;height:300
  property alias flags:flags
  property alias host:host
  QtObject {id:flags;property bool left:false;property bool osd:false;property bool session:false;property bool dashboard:false;property bool launcher:true;property string controlSection:"home";property string edgeMenu:"";property bool previewOnly:false}
  QtObject {id:input;property bool hovered:false;property bool popoutHovered:false;property bool dashboardHovered:false}
  NacrePanelHost {id:host;anchors.fill:parent;screen:({name:"test"});visibilities:flags;input:input}
 }}
 function test_stale_painted_launcher_does_not_receive_press(){
  const view=createTemporaryObject(scene,this);wait(30);
  mouseClick(view.host.launcher,10,10);compare(view.host.launcher.presses,1);
  view.flags.launcher=false;view.flags.osd=true;
  verify(view.host.launcher.visible);compare(view.host.launcher.width,200);
  mouseClick(view.host.launcher,80,40);compare(view.host.launcher.presses,1);
  verify(!view.host.launcher.enabled);verify(view.host.osd.enabled);
  view.flags.osd=false;verify(!view.host.osd.enabled);
  view.flags.left=true;verify(view.host.leftDrawer.enabled);
 }
}
""")
            result = subprocess.run(
                [str(runner), "-input", str(root), "-o", "-,txt"],
                capture_output=True,
                text=True,
                timeout=20,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
