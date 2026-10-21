from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Actual panel host disables old controls while their closing geometry remains."""

import os
from pathlib import Path
import shutil
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
            stage_qml(
                SHELL / "modules/drawers/NacrePanelHost.qml",
                root / "NacrePanelHost.qml",
            )
            shutil.copy2(SHELL / "modules/drawers/layout.js", root / "layout.js")
            (fixtures / "NacreFrame.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property int left:10;property int right:10;property int headerHeight:50;property int bottom:10;property int rounding:20}"
            )
            manifest = ["singleton NacreFrame 1.0 NacreFrame.qml"]
            for name, body in {
                "NacrePearls": "property var drag:null",
                "NacreTokens": 'property bool gradientsEnabled:true;function gradientColour(base,shaded){return gradientsEnabled?shaded:base}property bool motionEnabled:false;property color body:"#123456";property color orientSecondary:"#abcdef";property color accent:"#fedcba"',
            }.items():
                (fixtures / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                )
                manifest.append("singleton " + name + " 1.0 " + name + ".qml")
            (fixtures / "NacreSurface.qml").write_text("import QtQuick\nRectangle {}")
            manifest.append("NacreSurface 1.0 NacreSurface.qml")
            (fixtures / "NacrePanelDrag.qml").write_text(
                "import QtQuick\nItem {property string kind;property var host;property var screen}"
            )
            manifest.append("NacrePanelDrag 1.0 NacrePanelDrag.qml")
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
 property string screenName
 property var visibilities
 property string section:"home"
 property string presentation:"sidebar"
 signal sidebarRequested(bool pointerOwned)
 property bool restoreImmediately:false
 property bool keyboardActive:false
 property bool visibility:false
 property bool hovered:false
 property bool hasCurrent:false
 property bool pinned:false
 property string currentName:""
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
 function test_status_peeks_follow_the_icon_center_and_clamp_at_the_edge(){
  const view=createTemporaryObject(scene,this),host=view.host,popup=host.popouts;
  popup.currentName="peek";popup.width=100;popup.targetWidth=100;popup.hasCurrent=true;
  for(const center of [100,160,220]){
   popup.currentCenter=center;
   compare(host.popupPosition.x+popup.width/2+host.x,center);
   compare(host.popupPosition.y,0);verify(!host.popupPosition.joinsRight);
  }
  popup.currentCenter=390;
  compare(host.popupPosition.x+popup.width,host.width);verify(host.popupPosition.joinsRight);
  view.width=300;popup.currentCenter=290;
  compare(host.popupPosition.x+popup.width,host.width);
 }
 function test_top_dashboard_does_not_hide_corner_notifications(){
  const view=createTemporaryObject(scene,this);
  view.host.popouts.height=0;view.host.session.width=0;view.flags.launcher=false;
  verify(!view.host.notifications.suppressed);verify(view.host.notifications.enabled);
  view.flags.dashboard=true;view.host.dashboard.height=240;
  verify(!view.host.notifications.suppressed);verify(view.host.notifications.enabled);
  view.flags.dashboard=false;view.host.dashboard.height=15;
  verify(!view.host.notifications.suppressed);
  view.flags.osd=true;verify(view.host.notifications.suppressed);verify(!view.host.notifications.enabled);
  view.flags.osd=false;view.host.popouts.height=100;verify(view.host.notifications.suppressed);
  view.host.popouts.height=0;view.host.session.width=200;verify(view.host.notifications.suppressed);
 }
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
            install_i18n(fixtures)
            result = run_qml(
                [str(runner), "-input", str(root), "-o", "-,txt"],
                capture_output=True,
                text=True,
                timeout=20,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
