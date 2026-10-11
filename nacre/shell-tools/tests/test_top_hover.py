"""Actual marked frame lips and shared state: hover, click, guards and rearming."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest
from qml_source import install_foundation_interaction

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class TopHoverTests(unittest.TestCase):
    def test_marked_lips_hover_click_guards_and_dismissal(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
            install_foundation_interaction(fixtures, SHELL / "widgets")
            definitions = {
                "NacreFrame": "property int left:10;property int right:10;property int headerHeight:50",
                "NacreHyprland": 'property var focusedMonitor:({name:"test"});property var activeClient:null',
                "Environment": 'property var screens:[{name:"test"}]',
            }
            for name, text in definitions.items():
                (fixtures / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + text + "}"
                )
            for name in ("NacreHoverIntent", "NacrePanelState"):
                source = (
                    (SHELL / "services" / (name + ".qml"))
                    .read_text()
                    .replace("import Quickshell", "")
                    .replace("Singleton {", "QtObject {")
                    .replace("Quickshell.screens", "Environment.screens")
                )
                source = source.replace(
                    "import QtQuick", 'import QtQuick\nimport "."', 1
                )
                (fixtures / (name + ".qml")).write_text(source)
            with (fixtures / "qmldir").open("a") as manifest:
                for name in (*definitions, "NacreHoverIntent", "NacrePanelState"):
                    manifest.write(f"\nsingleton {name} 1.0 {name}.qml\n")
            for name, path in (
                ("NacreFrameLip", SHELL / "widgets/NacreFrameLip.qml"),
                ("NacreFrameLips", SHELL / "modules/drawers/NacreFrameLips.qml"),
            ):
                source = path.read_text()
                for imported in ("qs.widgets", "qs.config", "qs.services"):
                    source = source.replace("import " + imported, 'import "fixtures"')
                if name == "NacreFrameLip":
                    source = source.replace(
                        "import QtQuick", 'import QtQuick\nimport "fixtures"', 1
                    )
                (target / (name + ".qml")).write_text(source)
            (target / "tst_lips.qml").write_text("""import QtQuick
import QtTest
import "fixtures"
TestCase {
 id:test;name:"MarkedNacreLips";width:1100;height:760;visible:true;when:windowShown
 Component {id:scene;Item {
  width:1000;height:700
  property alias flags: flags
  property alias lips: lips
  property alias controller: controller
  property var screen:({name:"test",width:1000,height:700})
  QtObject {id:flags;property bool launcher:false;property bool session:false;property bool dashboard:false;property bool dashboardPinned:false;property bool left:false;property bool leftPinned:false;property bool osd:false;property bool previewOnly:false;property string controlSection:"home";property string edgeMenu:""}
  QtObject {id:controller;property bool modal:false;property var panels:({dashboard:{height:0},leftDrawer:{width:0},osd:{width:0}});function settleHover(){}}
  NacreFrameLips {id:lips;anchors.fill:parent;screen:parent.screen;visibilities:flags;controller:controller}
  Component.onCompleted: {NacrePanelState.screens={test:flags};NacrePanelState.panels={test:{popouts:{hasCurrent:false,pinned:false}}}}
 }}
 function init(){NacrePanelState.hidden=false;DesktopSettings.data={animations:false,clickEdgeMenus:false};NacreHyprland.activeClient=null;NacreHoverIntent.blocked=({});NacreHoverIntent.lipRegions=({});}
 function test_only_marked_regions_open_and_explicit_dismissal_rearms(){
  const view=createTemporaryObject(scene,this);wait(30);
  mouseMove(view,100,25);verify(!view.flags.dashboard);
  mouseMove(view,500,51);verify(view.flags.dashboard);
  NacreHoverIntent.dismiss(view.screen);view.flags.dashboard=false;
  mouseMove(view,501,52);verify(!view.flags.dashboard);
  mouseMove(view,500,180);NacreHoverIntent.observe(view.screen,500,180);mouseMove(view,500,51);verify(view.flags.dashboard);
  mouseMove(view,1,90);verify(!view.flags.left);
  mouseMove(view,8,350);verify(view.flags.left);verify(!view.flags.leftPinned);
  mouseMove(view,995,620);verify(!view.flags.osd);
  mouseMove(view,990,350);verify(view.flags.osd);compare(view.flags.edgeMenu,"");
  NacreHoverIntent.observe(view.screen,500,53);
  NacreHoverIntent.dismiss(view.screen,false);
  verify(!NacreHoverIntent.blocked.test.dashboard);
  NacreHoverIntent.setHeaderHover("test","lip",true);
  NacreHoverIntent.setHeaderHover("test","bar",true);
  NacreHoverIntent.setHeaderHover("test","lip",false);
  verify(NacreHoverIntent.headers.test);
  NacreHoverIntent.setHeaderHover("test","bar",false);
  verify(!NacreHoverIntent.headers.test);
  compare(view.width,1000);compare(view.height,700);
 }
 function test_lips_follow_panel_edges_and_keep_top_activation_column(){
  const view=createTemporaryObject(scene,this);wait(30);
  const top=findChild(view,"frameTopLip");
  const left=findChild(view,"frameLeftLip");
  const right=findChild(view,"frameRightLip");
  const topY=top.y;const leftX=left.x;const rightX=right.x;
  view.controller.panels={dashboard:{height:240},leftDrawer:{width:320},osd:{width:424}};
  view.flags.dashboard=true;view.flags.left=true;view.flags.osd=true;
  compare(top.y,topY+240);compare(left.x,leftX+320);compare(right.x,rightX-424);
  compare(top.height,7);compare(left.width-NacreFrame.left,5);
  const column=NacreHoverIntent.lipRegions.test.dashboard;
  compare(column.x,396);compare(column.y,0);compare(column.width,208);compare(column.height,56);
  verify(top.visible);verify(left.visible);verify(right.visible);
  view.flags.dashboard=false;view.flags.left=false;view.flags.osd=false;
  verify(top.visible);verify(left.visible);verify(right.visible);
 }
 function test_modal_and_gallery_keep_all_handles_without_hover_switching(){
  mouseMove(this,800,500);
  const view=createTemporaryObject(scene,this);wait(30);
  const top=findChild(view,"frameTopLip"),left=findChild(view,"frameLeftLip"),right=findChild(view,"frameRightLip");
  view.controller.modal=true;
  view.controller.panels={dashboard:{height:0},leftDrawer:{width:0},osd:{width:0},launcher:{fullScreenGallery:true}};
  view.flags.launcher=true;
  verify(top.visible);verify(left.visible);verify(right.visible);
  compare(view.lips.rimHandles.length,3);
  mouseMove(view,8,350);verify(view.flags.launcher);verify(!view.flags.left);
  mouseMove(view,990,350);verify(view.flags.launcher);verify(!view.flags.osd);
  mouseMove(view,500,53);verify(view.flags.launcher);verify(!view.flags.dashboard);
  view.flags.launcher=false;view.flags.dashboard=true;view.flags.dashboardPinned=true;
  verify(top.visible);verify(left.visible);verify(right.visible);
  view.flags.dashboard=false;view.flags.dashboardPinned=false;view.flags.session=true;
  verify(top.visible);verify(left.visible);verify(right.visible);
  view.flags.session=false;view.flags.launcher=true;
  mouseClick(view,990,350);verify(view.flags.osd);verify(!view.flags.launcher);
  DesktopSettings.data={animations:false,leftDrawer:false,rightEdge:false};
  verify(top.visible);verify(!left.visible);verify(!right.visible);
  NacrePanelState.hidden=true;
  verify(!top.visible);verify(!left.visible);verify(!right.visible);
  NacrePanelState.hidden=false;
 }
 function test_moving_handle_dismissal_blocks_return_under_stationary_pointer(){
  const view=createTemporaryObject(scene,this);wait(30);
  view.controller.panels={dashboard:{height:0},leftDrawer:{width:0},osd:{width:424}};
  NacreHoverIntent.observe(view.screen,990,350);
  NacreHoverIntent.dismiss(view.screen,true);
  verify(NacreHoverIntent.blocked.test.osd);
  NacreHoverIntent.rearm("osd",view.screen);
  verify(NacreHoverIntent.blocked.test.osd);
  view.controller.panels={dashboard:{height:0},leftDrawer:{width:0},osd:{width:0}};
  NacreHoverIntent.observe(view.screen,990,350);
  verify(!NacreHoverIntent.canOpen("osd",view.screen,Qt.NoButton));
  NacreHoverIntent.observe(view.screen,800,350);
  verify(NacreHoverIntent.canOpen("osd",view.screen,Qt.NoButton));
 }
 function test_sheen_is_finite_and_reduce_motion_stops_it(){
  mouseMove(this,800,500);
  DesktopSettings.data={animations:true,clickEdgeMenus:false};
  const view=createTemporaryObject(scene,this);wait(30);
  const lip=findChild(view,"frameTopLip");
  view.flags.dashboard=true;
  tryCompare(lip,"sheenRunning",true);
  wait(350);compare(lip.sheenRunning,false);
  view.flags.dashboard=false;view.flags.dashboard=true;
  tryCompare(lip,"sheenRunning",true);
  DesktopSettings.data={animations:true,reduceMotion:true,clickEdgeMenus:false};
  tryCompare(lip,"sheenRunning",false);
  view.flags.dashboard=false;view.flags.dashboard=true;wait(30);
  compare(lip.sheenRunning,false);
 }
 function test_click_only_and_fullscreen_held_button_guards(){
  DesktopSettings.data={animations:false,clickEdgeMenus:true};
  const view=createTemporaryObject(scene,this);wait(30);
  mouseMove(view,990,350);verify(!view.flags.osd);
  mouseClick(view,990,350);verify(view.flags.osd);compare(view.flags.edgeMenu,"osd");
  view.flags.osd=false;view.flags.edgeMenu="";
  DesktopSettings.data={animations:false,clickEdgeMenus:false};
  const left=findChild(view,"frameLeftLip");
  view.lips.approach("left",left,Qt.LeftButton);verify(!view.flags.left);
  NacreHyprland.activeClient={lastIpcObject:{fullscreen:2}};
  verify(!view.lips.available);
  view.lips.approach("left",left,Qt.NoButton);verify(!view.flags.left);
  NacreHyprland.activeClient=null;
 }
}
""")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
