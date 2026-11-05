from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Actual marked frame lips and shared state: hover, click, guards and rearming."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest
from qml_fixtures import install_foundation_interaction

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
                "NacreFrame": "property int left:10;property int right:10;property int headerHeight:50;property int rounding:25",
                "NacreCompositor": 'property var focusedMonitor:({name:"test"});property var activeWindow:null;property bool edgeSourceAcknowledged:false;readonly property bool externalEdgeSource:edgeSourceAcknowledged&&DesktopSettings.data.clickEdgeMenus!==true;property bool overviewOpen:false;signal event(var value)',
                "Environment": 'property var screens:[{name:"test"}]',
            }
            for name, text in definitions.items():
                (fixtures / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + text + "}"
                )
            (fixtures / "NacrePearls.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var drag:null;property var pendingDocks:[];signal dragChanged}"
            )
            for name in ("NacreHoverIntent", "NacrePanelState", "NacrePearls"):
                stage_qml(
                    SHELL / "services" / (name + ".qml"), fixtures / (name + ".qml")
                )
            with (fixtures / "qmldir").open("a") as manifest:
                manifest_text = (fixtures / "qmldir").read_text()
                for name in (
                    *definitions,
                    "NacreHoverIntent",
                    "NacrePanelState",
                    "NacrePearls",
                ):
                    entry = f"singleton {name} 1.0 {name}.qml"
                    if entry not in manifest_text:
                        manifest.write(f"\n{entry}\n")
                        manifest_text += f"\n{entry}\n"
            for name, path in (
                ("NacreFrameLip", SHELL / "widgets/NacreFrameLip.qml"),
                ("NacreFrameLips", SHELL / "modules/drawers/NacreFrameLips.qml"),
                ("NacrePanelInput", SHELL / "modules/drawers/NacrePanelInput.qml"),
            ):
                stage_qml(path, target / (name + ".qml"))
            (target / "tst_lips.qml").write_text("""import QtQuick
import QtTest
import qs.services as Services
import "fixtures"
TestCase {
 id:test;name:"MarkedNacreLips";width:1100;height:760;visible:true;when:windowShown
 QtObject {id:eventProbe;property int count:0}
 Connections {target:Services.NacreCompositor;function onEvent(value){eventProbe.count++}}
 Component {id:scene;Item {
  width:1000;height:700
  property alias flags: flags
  property alias lips: lips
  property alias controller: controller
  property var screen:({name:"test",width:1000,height:700})
  QtObject {id:flags;property bool launcher:false;property bool session:false;property bool dashboard:false;property bool dashboardPinned:false;property bool left:false;property bool leftPinned:false;property bool osd:false;property bool controlsPointerOwned:false;property string controlsMode:"sidebar";property bool previewOnly:false;property bool hoverDocked:false;property string controlSection:"home";property string edgeMenu:""}
  Item {id:panels;property alias launcher:emptyPanel;property alias dashboard:emptyPanel;property alias leftDrawer:emptyPanel;property alias osd:emptyPanel;property alias session:emptyPanel;property alias popouts:popup;property alias notifications:notices;Item{id:emptyPanel} Item{id:popup;property bool pinned:false;property bool hasCurrent:false;property bool headerHovered:false} Item{id:notices;property bool visible:false}}
  NacrePanelInput {id:controller;anchors.fill:parent;screen:parent.screen;visibilities:flags;panels:panels;lips:lips}
  NacreFrameLips {id:lips;anchors.fill:parent;screen:parent.screen;visibilities:flags;controller:controller}
  Component.onCompleted: {NacrePanelState.screens={test:flags};NacrePanelState.panels={test:{popouts:{hasCurrent:false,pinned:false}}}}
 }}
 function init(){NacrePanelState.hidden=false;DesktopSettings.data={animations:false,clickEdgeMenus:false};Services.NacreCompositor.activeWindow=null;Services.NacreCompositor.edgeSourceAcknowledged=false;Services.NacreCompositor.overviewOpen=false;NacreHoverIntent.blocked=({});NacreHoverIntent.lipRegions=({});}
 function test_dashboard_delay_cancels_transit_and_keeps_reduced_motion_static(){
  const view=createTemporaryObject(scene,this);mouseMove(view,100,180);wait(30);
  mouseMove(view,500,25);wait(220);verify(!view.flags.dashboard);verify(view.lips.dashboardWaiting);compare(view.lips.dashboardProgress,1);
  mouseMove(view,500,180);wait(340);verify(!view.flags.dashboard);verify(!view.lips.dashboardWaiting);compare(view.lips.dashboardProgress,0);
  mouseMove(view,500,25);wait(250);mouseMove(view,504,25);wait(290);verify(view.flags.dashboard);
 }
 function test_dashboard_progress_animates_without_restarting_for_motion(){
  DesktopSettings.data={animations:true};const view=createTemporaryObject(scene,this);mouseMove(view,100,180);wait(30);
  mouseMove(view,500,25);wait(180);verify(view.lips.dashboardProgress>0 && view.lips.dashboardProgress<.9);verify(!view.flags.dashboard);
  mouseMove(view,504,25);wait(70);verify(!view.flags.dashboard);wait(180);verify(view.flags.dashboard);
 }
 function test_only_marked_regions_open_and_explicit_dismissal_rearms(){
  const view=createTemporaryObject(scene,this);wait(30);
  mouseMove(view,100,25);verify(!view.flags.dashboard);
  mouseMove(view,500,25);verify(!view.flags.dashboard);wait(530);verify(view.flags.dashboard);
  NacreHoverIntent.dismiss(view.screen);view.flags.dashboard=false;
  mouseMove(view,501,25);verify(!view.flags.dashboard);
  mouseMove(view,500,180);NacreHoverIntent.observe(view.screen,500,180);mouseMove(view,500,25);verify(!view.flags.dashboard);wait(530);verify(view.flags.dashboard);
  mouseMove(view,1,90);verify(!view.flags.left);
  mouseMove(view,8,350);verify(!view.flags.left);wait(380);verify(view.flags.left);verify(!view.flags.leftPinned);
  mouseMove(view,995,620);verify(!view.flags.osd);
  mouseMove(view,990,350);verify(!view.flags.osd);wait(380);verify(view.flags.osd);compare(view.flags.edgeMenu,"");
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
  compare(top.y,topY);compare(left.x,leftX);compare(right.x,rightX);
  compare(top.height,NacreFrame.headerHeight);compare(left.width,NacreFrame.left);compare(left.height,144);
  const column=NacreHoverIntent.lipRegions.test.dashboard;
  compare(column.x,396);compare(column.y,0);compare(column.width,208);compare(column.height,50);
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
  compare(view.lips.rimHandles.length,0);
  mouseMove(view,8,350);verify(view.flags.launcher);verify(!view.flags.left);
  mouseMove(view,990,350);verify(view.flags.launcher);verify(!view.flags.osd);
  mouseMove(view,500,25);verify(view.flags.launcher);verify(!view.flags.dashboard);
  view.flags.launcher=false;view.flags.dashboard=true;view.flags.dashboardPinned=true;
  verify(top.visible);verify(left.visible);verify(right.visible);
  view.flags.dashboard=false;view.flags.dashboardPinned=false;view.flags.session=true;
  verify(top.visible);verify(left.visible);verify(right.visible);
  view.flags.session=false;view.flags.launcher=true;
  mouseClick(view,990,350);verify(view.flags.osd);verify(!view.flags.launcher);
  DesktopSettings.data={animations:false,aiSidebarEnabled:false,controlCenterEnabled:false};
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
 function test_side_tab_transit_cancels_and_click_closes_current_detail(){
  DesktopSettings.data={animations:true};const view=createTemporaryObject(scene,this);mouseMove(view,500,180);wait(30);
  const tab=findChild(view,"frameRightLip");
  const cue=findChild(view,"nacreRightDwell"),line=findChild(tab,"edgeTabHoverFill"),track=findChild(tab,"edgeTabHoverCue");
  compare(findChild(tab,"edgeTabStroke").strokeColor,NacreTokens.accent);compare(line.height,0);
  verify(track.x>=0 && track.x+track.width<=tab.width);
  mouseMove(tab,tab.width/2,tab.height/2);wait(170);verify(!view.flags.osd);verify(cue.progress>0 && cue.progress<1);
  verify(line.height>0 && line.height<track.height);compare(track.opacity,1);
  mouseMove(view,500,180);wait(250);verify(!view.flags.osd);compare(cue.progress,0);compare(line.height,0);
  mouseMove(tab,tab.width/2,tab.height/2);wait(430);verify(view.flags.osd);verify(!track.visible);compare(line.height,track.height);tryCompare(track,"opacity",0,500);
  compare(findChild(tab,"edgeTabStroke").strokeColor,NacreTokens.accent);
  view.flags.controlSection="audio";
  mouseClick(tab,tab.width/2,tab.height/2);verify(!view.flags.osd);
  mouseMove(view,500,180);wait(30);
  tab.forceActiveFocus();keyClick(Qt.Key_Return);verify(view.flags.osd);verify(!view.flags.controlsPointerOwned);
 }
 function test_side_arrow_is_readable_and_target_reaches_above_and_below_it(){
  DesktopSettings.data={animations:true};const view=createTemporaryObject(scene,this);mouseMove(view,500,180);wait(30);
  const tab=findChild(view,"frameRightLip"),arrow=findChild(tab,"edgeTabArrow");
  compare(arrow.height,18);verify(arrow.width>=6);verify(arrow.x>=0 && arrow.x+arrow.width<=tab.width);
  verify(!findChild(tab,"edgeTabPatch"));verify(!findChild(tab,"edgeTabTrace"));
  mouseMove(view,view.width-3,view.height/2-60);wait(430);verify(view.flags.osd);
  mouseClick(view,view.width-3,view.height/2-60);verify(!view.flags.osd);
  mouseMove(view,500,180);wait(30);
  mouseMove(view,3,view.height/2+60);wait(430);verify(view.flags.left);verify(!findChild(findChild(view,"frameLeftArrow"),"edgeTabHoverCue").visible);
 }
 function test_flat_targets_never_run_decorative_sheen(){
  mouseMove(this,800,500);
  DesktopSettings.data={animations:true,clickEdgeMenus:false};
  const view=createTemporaryObject(scene,this);wait(30);
  const lip=findChild(view,"frameTopLip");
  view.flags.dashboard=true;
  compare(lip.sheenRunning,false);
  wait(350);compare(lip.sheenRunning,false);
  view.flags.dashboard=false;view.flags.dashboard=true;
  compare(lip.sheenRunning,false);
  DesktopSettings.data={animations:true,reduceMotion:true,clickEdgeMenus:false};
  tryCompare(lip,"sheenRunning",false);
  view.flags.dashboard=false;view.flags.dashboard=true;wait(30);
  compare(lip.sheenRunning,false);
 }
 function test_lip_delay_setting_has_safe_defaults_bounds_and_zero_is_instant(){
  DesktopSettings.data={animations:true};const view=createTemporaryObject(scene,this);mouseMove(view,500,180);wait(30);
  compare(view.lips.hoverDelay,350);
  DesktopSettings.data={animations:true,lipHoverDelay:800};
  for(const id of ["nacreDashboardDwell","nacreLeftDwell","nacreRightDwell"])compare(findChild(view,id).duration,800);
  mouseMove(view,view.width-3,view.height/2);wait(200);verify(!view.flags.osd);
  mouseMove(view,500,180);wait(30);
  DesktopSettings.data={animations:true,lipHoverDelay:0};
  mouseMove(view,view.width-3,view.height/2);tryCompare(view.flags,"osd",true,100);
  mouseMove(view,500,180);view.flags.osd=false;wait(30);
  DesktopSettings.data={animations:true,lipHoverDelay:1000};compare(view.lips.hoverDelay,800);
  DesktopSettings.data={animations:true,lipHoverDelay:-1};compare(view.lips.hoverDelay,0);
  DesktopSettings.data={animations:true,lipHoverDelay:false};compare(view.lips.hoverDelay,350);
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
  NacreCompositor.activeWindow={fullscreenMode:2};
  verify(!view.lips.available);
  view.lips.approach("left",left,Qt.NoButton);verify(!view.flags.left);
  NacreCompositor.activeWindow=null;
 }
 function test_native_edge_source_suppresses_qml_dwell_and_routes_events_with_fallback(){
  const view=createTemporaryObject(scene,this);wait(30);Services.NacreCompositor.edgeSourceAcknowledged=true;
  const top=findChild(view,"frameTopLip");
  NacreHoverIntent.setHeaderHover("test","bar",true);
  view.lips.approach("dashboard",top,Qt.NoButton);verify(NacreHoverIntent.headers.test);verify(!view.lips.dashboardWaiting);
  view.lips.requestDashboard(Qt.NoButton);verify(!view.lips.dashboardWaiting);
  view.lips.leave("dashboard");verify(NacreHoverIntent.headers.test);
  NacreHoverIntent.setHeaderHover("test","bar",false);verify(!NacreHoverIntent.headers.test);
  mouseMove(view,8,350);wait(400);verify(!view.flags.left);
  Services.NacreCompositor.event({type:"edge-approach",output:"other",edge:"Left",position:[0,350],progress:.5});verify(!view.flags.left);
  Services.NacreCompositor.event({type:"edge-triggered",output:"test",edge:"left",position:[4,100]});verify(!view.flags.left);
  Services.NacreCompositor.event({type:"edge-triggered",output:"test",edge:"Left",position:{length:2,0:4,1:"350"}});verify(!view.flags.left);
  NacreHoverIntent.blocked={test:{left:true}};
  Services.NacreCompositor.event({type:"edge-triggered",output:"test",edge:"Left",position:[4,350]});verify(!view.flags.left);
  NacreHoverIntent.blocked={};
  Services.NacreCompositor.event({type:"edge-approach",output:"test",edge:"Left",position:[4,350],progress:.4});
  compare(findChild(view,"frameLeftArrow").amount,.4);verify(!view.flags.left);
  Services.NacreCompositor.event({type:"edge-hover",output:"test",edge:"Left",position:[4,350],progress:.75});
  compare(findChild(view,"frameLeftArrow").amount,.75);verify(!view.flags.left);
  Services.NacreCompositor.event({type:"edge-hover",output:"test",edge:"Left",position:[400,350],progress:.9});
  compare(findChild(view,"frameLeftArrow").amount,0);verify(!view.flags.left);
  Services.NacreCompositor.event({type:"edge-approach",output:"test",edge:"Top",position:[500,25],progress:.2});
  compare(view.lips.nativeDashboardProgress,.2);verify(NacreHoverIntent.headers.test);verify(!view.flags.dashboard);
  verify(!view.controller.leftEdgeAvailable);verify(!view.controller.rightEdgeAvailable);verify(!view.controller.topEdgeAvailable);
  const hasRect=(regions,rect)=>regions.some(r=>r.x===rect.x&&r.y===rect.y&&r.width===rect.width&&r.height===rect.height);
  verify(!hasRect(view.controller.regions,view.controller.leftEdgeRect));verify(!hasRect(view.controller.regions,view.controller.rightEdgeRect));verify(!hasRect(view.controller.regions,view.controller.topEdgeRect));
  verify(hasRect(view.controller.regions,view.controller.headerRect));
  Services.NacreCompositor.event({type:"edge-hover",output:"test",edge:"Top",position:[500,25],progress:.7});
  compare(view.lips.nativeDashboardProgress,.7);verify(NacreHoverIntent.headers.test);verify(!view.flags.dashboard);
  Services.NacreCompositor.event({type:"edge-triggered",output:"test",edge:"Top",position:[50,25]});
  verify(!view.flags.dashboard);compare(view.lips.nativeDashboardProgress,0);
  Services.NacreCompositor.event({type:"edge-hover",output:"test",edge:"Top",position:[500,25],progress:1});
  verify(!view.flags.dashboard);verify(NacreHoverIntent.headers.test);
  Services.NacreCompositor.event({type:"edge-left",output:"test",edge:"Top",position:[500,50]});
  verify(!NacreHoverIntent.headers.test);
  Services.NacreCompositor.event({type:"edge-approach",output:"test",edge:"Top",position:[500,25],progress:.2});
  Services.NacreCompositor.event({type:"edge-triggered",output:"test",edge:"Top",position:[500,25]});
  verify(view.flags.dashboard);compare(view.lips.nativeDashboardProgress,0);verify(!NacreHoverIntent.headers.test);
  Services.NacreCompositor.event({type:"edge-hover",output:"test",edge:"Top",position:[500,25],progress:1});
  compare(view.lips.nativeDashboardProgress,0);verify(NacreHoverIntent.headers.test);verify(view.flags.dashboard);
  Services.NacreCompositor.event({type:"edge-hover",output:"test",edge:"Top",position:[50,25],progress:1});
  compare(view.lips.nativeDashboardProgress,0);verify(!NacreHoverIntent.headers.test);
  Services.NacreCompositor.event({type:"edge-hover",output:"test",edge:"Top",position:[500,25],progress:1});
  compare(view.lips.nativeDashboardProgress,0);verify(NacreHoverIntent.headers.test);
  view.flags.dashboard=false;
  Services.NacreCompositor.event({type:"edge-hover",output:"test",edge:"Top",position:[50,25],progress:.9});
  compare(view.lips.nativeDashboardProgress,0);verify(!NacreHoverIntent.headers.test);
  Services.NacreCompositor.event({type:"edge-approach",output:"test",edge:"Top",position:[500,25],progress:.45});
  compare(view.lips.nativeDashboardProgress,0);compare(view.lips.nativeLeftProgress,0);verify(!NacreHoverIntent.headers.test);
  DesktopSettings.data={animations:false,clickEdgeMenus:true};
  verify(view.controller.leftEdgeAvailable);verify(view.controller.rightEdgeAvailable);verify(view.controller.topEdgeAvailable);
  verify(findChild(view,"frameLeftLip").enabled);verify(findChild(view,"frameRightLip").enabled);
  DesktopSettings.data={animations:false,clickEdgeMenus:false};
  verify(Services.NacreCompositor.externalEdgeSource);
  verify(!findChild(view,"frameLeftLip").enabled);verify(!findChild(view,"frameRightLip").enabled);
  Services.NacreCompositor.event({type:"edge-left",output:"test",edge:"Left",position:[4,350]});
  compare(findChild(view,"frameLeftArrow").amount,0);
  Services.NacreCompositor.event({type:"edge-left",output:"test",edge:"Top",position:[500,50]});verify(!NacreHoverIntent.headers.test);
  const beforeNativeEvent=eventProbe.count;Services.NacreCompositor.event({type:"edge-triggered",output:"test",edge:"Left",position:[4,350]});compare(eventProbe.count,beforeNativeEvent+1);verify(view.flags.left);
  NacreHoverIntent.setHeaderHover("test","lip",true);
  view.flags.launcher=true;
  Services.NacreCompositor.event({type:"edge-left",output:"test",edge:"Top",position:[500,50]});verify(!NacreHoverIntent.headers.test);
  view.flags.launcher=false;
  view.flags.left=false;Services.NacreCompositor.overviewOpen=true;
  Services.NacreCompositor.event({type:"edge-triggered",output:"test",edge:"Left",position:[4,350]});verify(!view.flags.left);
  Services.NacreCompositor.overviewOpen=false;Services.NacreCompositor.edgeSourceAcknowledged=false;
  mouseMove(view,500,180);mouseMove(view,8,350);wait(400);verify(view.flags.left);
 }
}
""")
            install_i18n(fixtures)
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_header_edge_approach_skips_local_trigger_with_native_source(self):
        header = (SHELL / "modules/topbar/NacreHeader.qml").read_text()
        self.assertIn(
            "!NacreCompositor.externalEdgeSource && !root.clickMenus",
            header,
        )
