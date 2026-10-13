"""Owned compositor operations, private recovery and real docking geometry."""

import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
SHELL = TOOLS.parent / "shell"


def module(name):
    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), TOOLS / (name + ".py")
    )
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


class PearlTests(unittest.TestCase):
    def test_super_drag_routes_only_live_owned_panels_and_preserves_release(self):
        script = r"""
local binds, moves, unbound, active, timer = {}, 0, nil, nil, nil
local events = {}
local cursor, layers = {x=120,y=220}, {{namespace="nacre-drawers",mapped=true,pid=45}}
nacre_panel_pid=45
nacre_panel_regions={{x=100,y=200,w=500,h=300}}
local function proxy(path)
 return setmetatable({}, {__index=function(_,key)return proxy(path=="" and key or path.."."..key) end,__call=function(_,...)return {method=path,args={...}} end})
end
local pressed, pending = false, false
hl={dsp=proxy(""),window_rule=function()end,unbind=function(key)unbound=key end,
 bind=function(key,fn,opts)binds[key]={fn=fn,opts=opts or {}} end,
 get_cursor_pos=function()return cursor end,get_layers=function()return layers end,
 get_active_window=function()return active end,
 timer=function(fn,options) timer={callback=fn,enabled=true,set_enabled=function(self,value)self.enabled=value end};assert(options.timeout==33);return timer end,
 dispatch=function(request)
  if request.method=="event" then events[#events+1]=request.args[1];return {ok=true} end
  assert(request.method=="window.drag");moves=moves+1;pending=true
  -- Installed Hyprland 0.56 arms a native dispatch internally; it neither
  -- returns request_release nor honors that flag from a shell callback.
  return {ok=true}
 end}
dofile(arg[1])
assert(unbound=="SUPER + mouse:272")
local press=binds["SUPER + mouse:272"]
local release=binds["mouse:272"]
assert(release.opts.release and release.opts.ignore_mods and release.opts.non_consuming)
local function down()
 pressed=true;pending=false;return press.fn()
end
local function up()
 pressed=false
 if pending then pending=false;press.fn() end
 release.fn()
 assert(not timer or not timer.enabled)
end
-- A shell press has no implicit release. Explicit cleanup must still happen
-- after crossing outside the popup, even if Super was released first.
assert(down().pass_event and moves==0)
cursor={x=900,y=900};up()
active={pid=45,title="Nacre Pearl Dashboard 12",at={x=100,y=200},size={x=900,y=700}}
-- Immediately after detaching, the FIRST native pull must move and end/dock.
assert(down().ok and moves==1 and timer.enabled and #events==1)
cursor={x=800,y=10};timer.callback();assert(#events==2)
up();assert(moves==1 and #events==3)
assert(events[3]=="nacre_pearl_drag,end,Nacre Pearl Dashboard 12,800,10,100,200,900,700")
-- No fresh key press/reset between pulls: each must re-arm and finish once.
for i=1,3 do
 local count=#events
 down();assert(timer.enabled and #events==count+1)
 cursor={x=810+i,y=10};timer.callback();up()
 assert(#events==count+3 and events[#events]:match("^nacre_pearl_drag,end,"))
end
-- Reattaching and detaching again may not leave a stale shell route.
cursor={x=120,y=220};down();up();assert(not timer.enabled)
cursor={x=700,y=700};local count=#events
down();assert(timer.enabled and #events==count+1);up()
-- Foreign/unmapped layer and foreign pearl PID preserve native movement.
active={pid=99,title="Nacre Pearl Dashboard 12"};count=#events
layers[1].pid=99;cursor={x=120,y=220};down();up()
assert(#events==count and not timer.enabled)
layers[1].pid=45;layers[1].mapped=false;down();up()
assert(#events==count and not timer.enabled)
-- Idle ordinary releases are harmless and never launch a compositor drag.
local before=moves;release.fn();assert(moves==before)
print("PANEL_ROUTING_OK")
"""
        result = subprocess.run(
            ["lua", "-", str(TOOLS / "shortcuts.lua")],
            input=script,
            capture_output=True,
            text=True,
            timeout=10,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("PANEL_ROUTING_OK", result.stdout)

    def test_real_super_drag_handler_moves_live_content_without_plain_click_grab(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            source = (
                (SHELL / "widgets/NacrePanelDrag.qml")
                .read_text()
                .replace("import qs.services", 'import "."')
            )
            (target / "NacrePanelDrag.qml").write_text(source)
            (target / "NacrePearls.qml").write_text("""pragma Singleton
import QtQuick
QtObject {
 property bool enabled:true;property bool cancelOnBegin:false;property var drag:null;property int starts:0;property int finishes:0;property real dx:0
 function begin(kind,host,name,p){if(drag)return;starts++;drag={host:host,start:p};if(cancelOnBegin)host.enabled=false}
 function update(p){dx=p.x-drag.start.x;drag.host.x=dx}
 function finish(){finishes++;drag=null}
 function cancel(){drag=null}
}""")
            (target / "DesktopSettings.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var data:({dragPanels:true})}"
            )
            (target / "qmldir").write_text(
                "singleton NacrePearls 1.0 NacrePearls.qml\nsingleton DesktopSettings 1.0 DesktopSettings.qml\nNacrePanelDrag 1.0 NacrePanelDrag.qml\n"
            )
            (target / "tst_drag.qml").write_text("""import QtQuick
import QtTest
import "."
TestCase {
 name:"SuperPanelLift";width:400;height:300;visible:true;when:windowShown
 Item {id:under;width:350;height:250;z:-1}
 NacrePanelDrag {id:lower;enabled:false;z:-1;x:0;y:0;width:350;height:250;kind:"dashboard";host:under;screen:({name:"test",x:0,y:0})}
 Item {id:panel;width:350;height:250
  MouseArea {anchors.fill:parent;onPressed:mouse=>mouse.accepted=true}
  NacrePanelDrag {id:sensor;parent:panel.parent;x:0;y:0;width:350;height:250;kind:"ai";host:panel;screen:({name:"test",x:0,y:0})}
 }
 function init(){NacrePearls.drag=null;NacrePearls.starts=0;NacrePearls.finishes=0;NacrePearls.dx=0;NacrePearls.cancelOnBegin=false;panel.enabled=true;panel.x=0;lower.enabled=false;DesktopSettings.data={dragPanels:true};}
 function test_single_motion_and_release_use_full_pointer_delta(){
  for(let i=0;i<3;i++){
   panel.x=0;
   mousePress(sensor,20,20,Qt.LeftButton,Qt.MetaModifier);
   mouseMove(sensor,120,20,0,Qt.LeftButton,Qt.MetaModifier);
   compare(NacrePearls.dx,100);
   mouseRelease(sensor,140,20,Qt.LeftButton,Qt.MetaModifier);
   compare(NacrePearls.dx,120);compare(NacrePearls.finishes,i+1);
  }
 }
 function test_modifier_handoff_after_initial_panel_focus(){
  mousePress(sensor,20,20,Qt.LeftButton,Qt.NoModifier);
  mouseMove(sensor,120,20,0,Qt.LeftButton,Qt.MetaModifier);
  compare(NacrePearls.starts,1);compare(NacrePearls.dx,100);
  mouseRelease(sensor,120,20,Qt.LeftButton,Qt.MetaModifier);
  compare(NacrePearls.finishes,1);
 }
 function test_reserved_drag_and_plain_click(){
  mouseDrag(sensor,20,20,100,0,Qt.LeftButton,Qt.NoModifier);compare(NacrePearls.starts,0);
  mouseDrag(sensor,20,20,100,0,Qt.LeftButton,Qt.MetaModifier);
  compare(NacrePearls.starts,1);compare(NacrePearls.finishes,1);compare(NacrePearls.dx,100);compare(panel.x,100);panel.x=0;
  lower.enabled=true;NacrePearls.starts=0;NacrePearls.finishes=0;
  mouseDrag(sensor,20,20,100,0,Qt.LeftButton,Qt.MetaModifier);
  compare(NacrePearls.starts,1);compare(NacrePearls.finishes,1);compare(NacrePearls.dx,100);
  lower.enabled=false;panel.x=0;
  NacrePearls.cancelOnBegin=true;NacrePearls.starts=0;NacrePearls.finishes=0;
  mousePress(sensor,20,20,Qt.LeftButton,Qt.MetaModifier);
  mouseMove(sensor,120,20,1,Qt.LeftButton,Qt.MetaModifier);wait(30);
  compare(NacrePearls.starts,1);compare(NacrePearls.finishes,0);compare(NacrePearls.drag,null);
  mouseRelease(sensor,120,20,Qt.LeftButton,Qt.MetaModifier);
  NacrePearls.cancelOnBegin=false;panel.enabled=true;
  DesktopSettings.data={dragPanels:false};
  mouseDrag(sensor,20,20,100,0,Qt.LeftButton,Qt.MetaModifier);compare(NacrePearls.starts,1);
 }
}""")
            result = subprocess.run(
                [str(runner), "-input", str(target)],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                text=True,
                capture_output=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_layout_writer_finishes_one_request_without_waiting_for_pipe_eof(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "state.json"
            process = subprocess.Popen(
                [sys.executable, str(TOOLS / "pearl-state.py"), "save"],
                env={**os.environ, "NACRE_PEARL_STATE": str(path)},
                stdin=subprocess.PIPE,
                text=True,
            )
            try:
                process.stdin.write(json.dumps({"version": 1, "windows": []}) + "\n")
                process.stdin.flush()
                self.assertEqual(process.wait(timeout=3), 0)
                self.assertEqual(json.loads(path.read_text())["windows"], [])
            finally:
                process.stdin.close()
                if process.poll() is None:
                    process.kill()
                process.wait()

    def test_actual_ai_view_lease_moves_chat_and_preserves_other_draft(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            (target / "NacreChatSession.qml").write_text("""import QtQuick
Item {
 property string channel:""
 property string draft:""
 property string title:"Draft"
 property string threadId:""
 property string provider:"codex"
 property string account:"Default"
 property bool busy:false
 property bool recoveryPending:false
 property bool parked:false
 property int saves:0
 property int starts:0
 function saveDraft(){saves++;}
 function start(){starts++;}
 function load(key){}
}""")
            (target / "SidebarChat.qml").write_text(
                "pragma Singleton\nimport QtQuick\nNacreChatSession {}"
            )
            (target / "NacrePearls.qml").write_text("""pragma Singleton
import QtQuick
QtObject {property var entries:[];property int writes:0;function saveSoon(){writes++;}}""")
            (target / "DesktopExtras.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var chats:[]}"
            )
            (target / "qmldir").write_text(
                "singleton SidebarChat 1.0 SidebarChat.qml\nsingleton NacrePearls 1.0 NacrePearls.qml\nsingleton DesktopExtras 1.0 DesktopExtras.qml\nNacreChatSession 1.0 NacreChatSession.qml\n"
            )
            source = (
                (SHELL / "services/NacreAiViews.qml")
                .read_text()
                .replace("pragma Singleton", "")
                .replace("import Quickshell", 'import "."')
                .replace("Singleton {", "Item {")
            )
            (target / "Views.qml").write_text(source)
            (target / "tst_views.qml").write_text("""import QtQuick
import QtTest
import "."
TestCase {
 name:"AssistantPanelOwnership"
 Views {id:views}
 function test_move_focus_and_dock_without_duplicating_or_losing_drafts(){
  const original=views.edge;original.draft="first draft";original.busy=true;
  compare(views.take(),original);verify(views.edge!==original);
  const next=views.edge;compare(next.draft,"");next.draft="second draft";
  let focused=0;
  NacrePearls.entries=[{kind:"ai",chat:original,window:{activate:()=>focused++}}];
  views.select(original);compare(focused,1);compare(views.edge,next);
  compare(original.draft,"first draft");verify(original.busy);
  NacrePearls.entries=[];views.dock(original);
  compare(views.edge,original);verify(views.parked.includes(next));
  compare(next.draft,"second draft");verify(next.parked);verify(!original.parked);
  verify(original.busy);verify(original.saves>0);verify(next.saves>0);
  const state=views.snapshot();compare(state.edge,"");compare(state.channels.length,2);
  views.restore(state);compare(views.sessions.length,2);compare(views.edge,original);
  next.draft="";next.busy=false;
  compare(views.snapshot().channels.length,2);
  next.recoveryPending=false;
  compare(views.recent.length,0);compare(views.snapshot().channels.length,1);
 }
}""")
            result = subprocess.run(
                [str(runner), "-input", str(target)],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                text=True,
                capture_output=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_window_identity_requires_both_owned_title_and_process(self):
        window = module("pearl-window")
        clients = [{"title": "Nacre Pearl Launcher 1", "pid": 45, "address": "0xabc"}]
        self.assertEqual(
            window.owned("Nacre Pearl Launcher 1", 45, clients)["address"], "0xabc"
        )
        self.assertIsNone(window.owned("Nacre Pearl Launcher 1", 46, clients))
        for title in ("Google Chrome", "Nacre Pearl Launcher 1;exec", "Nacre Settings"):
            with self.assertRaises(ValueError):
                window.owned(title, 45, clients)
        with patch.object(window.subprocess, "run") as run:
            with self.assertRaises(ValueError):
                window.dispatch("move", "not-an-address", x=0, y=0)
            run.assert_not_called()

    def test_gallery_fit_respects_logical_screen_and_maximized_windows(self):
        window = module("pearl-window")
        client = {
            "title": "Nacre Pearl Launcher 1",
            "pid": 45,
            "address": "0xabc",
            "at": [100, 880],
            "size": [1360, 320],
            "monitor": 0,
            "fullscreen": 0,
        }
        monitors = [
            {
                "id": 0,
                "x": 0,
                "y": 0,
                "width": 2880,
                "height": 1800,
                "scale": 1.5,
                "reserved": [10, 50, 10, 10],
            }
        ]

        def read(kind):
            return monitors if kind == "monitors" else [client]

        with (
            patch.object(window, "read", side_effect=read),
            patch.object(window, "dispatch") as dispatch,
            patch.object(
                sys,
                "argv",
                ["pearl-window", "fit", client["title"], "45", "0", "0", "640", "820"],
            ),
            patch("builtins.print"),
        ):
            self.assertEqual(window.main(), 0)
            dispatch.assert_any_call("resize", "0xabc", x=1360, y=820, relative=False)
            dispatch.assert_any_call("move", "0xabc", x=100, y=370, relative=False)
            client["fullscreen"] = 1
            dispatch.reset_mock()
            self.assertEqual(window.main(), 0)
            dispatch.assert_not_called()

    def test_shared_magnetic_film_renders_and_respects_reduced_motion(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            (target / "NacreDockFilm.qml").write_text(
                (SHELL / "widgets/NacreDockFilm.qml")
                .read_text()
                .replace("import qs.services", 'import "."')
            )
            (target / "NacreTokens.qml").write_text("""pragma Singleton
import QtQuick
QtObject {property bool motionEnabled:true;property color accent:"#85b8c9";property color orientSecondary:"#d48dbc";property color orientHighlight:"#e9e5f7"}
""")
            (target / "qmldir").write_text(
                "singleton NacreTokens 1.0 NacreTokens.qml\n"
            )
            (target / "tst_film.qml").write_text("""import QtQuick
import QtTest
import "."
TestCase {
 name:"MagneticFilm";width:640;height:480;visible:true;when:windowShown
 Rectangle {id:scene;width:640;height:480;color:"#101419"
  NacreDockFilm {id:film;anchors.fill:parent;origin:Qt.point(200,50);tip:Qt.point(230,110);armed:true}
 }
 function test_painted_membrane_and_reduced_motion(){
  wait(180);verify(film.visible);
  const image=grabImage(scene);
  verify(image.pixel(220,80).g>image.pixel(10,10).g+.05);
  compare(image.pixel(10,10),Qt.rgba(16/255,20/255,25/255,1));
  NacreTokens.motionEnabled=false;film.tip=Qt.point(300,150);film.armed=false;
  compare(film.stretch,100/170);compare(film.opacity,.6);
  film.tip=Qt.point(200,50);verify(!film.visible);
 }
}
""")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={
                    **os.environ,
                    "QT_QPA_PLATFORM": "offscreen",
                    "QT_QUICK_BACKEND": "rhi",
                    "QSG_RHI_BACKEND": "opengl",
                },
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_actual_service_shares_snap_zone_and_preserves_release_lifecycle(self):
        from qml_source import remove_objects

        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            fixtures = {
                "DesktopSettings": "property var data:({dragPanels:true})",
                "Environment": 'property var screens:[{name:"test",x:0,y:0,width:1920,height:1200}];property int processId:88',
                "NacreFrame": "property int headerHeight:50;property int bottom:10;property int left:10;property int right:10",
                "NacreCompositor": 'property string backendName:"hyprland";property var windows:[];property int refreshes:0;function refreshWindows(){refreshes++}',
                "NacrePearls": "property var views:[]",
                "NacreHyprland": 'property var focusedMonitor:({name:"test"})',
            }
            manifest = ""
            for name, body in fixtures.items():
                (target / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                )
                manifest += f"singleton {name} 1.0 {name}.qml\n"
            source = (
                (SHELL / "services/NacrePanelState.qml")
                .read_text()
                .replace("import Quickshell", "")
                .replace("Singleton {", "Item {")
            )
            (target / "NacrePanelState.qml").write_text(
                source.replace("Quickshell.screens", "Environment.screens")
            )
            manifest += "singleton NacrePanelState 1.0 NacrePanelState.qml\n"
            (target / "qmldir").write_text(manifest)
            source = (
                (SHELL / "services/NacrePearls.qml")
                .read_text()
                .replace("pragma Singleton", "")
                .replace("import Quickshell.Io", "")
                .replace("import Quickshell", "")
                .replace("import qs.modules.pearls as Pearl", "")
                .replace("import qs.config", 'import "."')
                .replace("import qs.widgets", 'import "."')
                .replace("Singleton {", "Item {")
                .replace(
                    'import "../modules/pearls/geometry.js" as Geometry',
                    'import "geometry.js" as Geometry',
                )
            )
            source = remove_objects(
                source,
                r"\b(Component(?!\s*\{\s*id:\s*frameBarrierFactory\b)|Process|IpcHandler|Connections)\s*\{",
            )
            source = source.replace(
                "Quickshell.screens", "Environment.screens"
            ).replace("Quickshell.processId", "Environment.processId")
            (target / "NacreFrameBarrier.qml").write_text(
                (SHELL / "widgets/NacreFrameBarrier.qml").read_text()
            )
            (target / "Controller.qml").write_text(source)
            (target / "geometry.js").write_text(
                (SHELL / "modules/pearls/geometry.js").read_text()
            )
            (target / "tst_magnet.qml").write_text("""import QtQuick
import QtTest
import "."
TestCase {
 name:"MagneticDockLifecycle";when:windowShown
 Controller {id:controller}
 QtObject {id:surface;property bool autoFrames:true;property int requests:0;signal frameSwapped();function update(){requests++;if(autoFrames)Qt.callLater(()=>frameSwapped())}}
 property var view:({dashboard:false,dashboardPinned:false,left:false,leftPinned:false,launcher:false,osd:false,session:false,previewOnly:false,hoverDocked:false,edgeMenu:"",dashboardTab:0})
 property int destroyed:0
 property int prepared:0
 function init(){surface.autoFrames=true;surface.requests=0;controller.entries=[];controller.drag=null;controller.nativeDrag=null;controller.niriMove=null;destroyed=0;prepared=0;NacrePanelState.hidden=false;NacrePanelState.screens={test:view};NacrePanelState.panels={test:{dashboard:{Window:{window:surface},restoreAttached(){prepared++}}}};NacreCompositor.backendName="hyprland";NacreCompositor.windows=[];}
 function addEntry(){const e={kind:"dashboard",title:"Nacre Pearl Dashboard 12",origin:"test",window:{visible:true,ready:true,clientAddress:"niri-window:12",captureState(){return {dashboardTab:2}},destroy(){destroyed++}},destroy(){destroyed++}};controller.entries=[e];return e;}
 function packet(phase,x,y){return "nacre_pearl_drag,"+phase+",Nacre Pearl Dashboard 12,"+x+","+y+",400,300,984,740";}
 function test_native_grab_middle_docks_before_pointer_reaches_edge(){
  for(let i=0;i<2;i++){
   addEntry();controller.nativeGesture(packet("begin",900,600));
   controller.nativeGesture(packet("move",900,390));
   verify(controller.nativeDrag.destination.armed);compare(controller.nativeDrag.body.y,90);
   controller.nativeGesture(packet("end",900,390));
   compare(controller.nativeDrag,null);tryVerify(()=>controller.entries.length===0);
   verify(view.dashboard);compare(view.dashboardTab,2);verify(view.hoverDocked);
   wait(1);
  }
  compare(destroyed,4);compare(prepared,2);
 }
 function niriPacket(phase,body={x:400,y:90,width:984,height:740}){return {type:"window-move",phase:phase,windowId:"niri-window:12",body:body,outputId:"test",floating:true};}
 function niriOwned(){NacreCompositor.backendName="niri";NacreCompositor.windows=[{windowId:"niri-window:12",title:"Nacre Pearl Dashboard 12",pid:88}];}
 function test_niri_requires_observed_move_and_preserves_repeated_docks(){
  niriOwned();addEntry();controller.compositorMove(niriPacket("end"));compare(controller.entries.length,1);
  for(let i=0;i<2;i++){
   if(i) addEntry();controller.compositorMove(niriPacket("begin"));verify(controller.niriMove);
   compare(controller.landingCues.length,1);compare(controller.landingCues[0].edge,"top");compare(controller.landingCues[0].origin.y,50);compare(controller.magneticCue,null);
   wait(250);compare(controller.entries.length,1);
   controller.compositorMove(niriPacket("end"));tryVerify(()=>controller.entries.length===0);
   verify(view.dashboard);verify(view.hoverDocked);compare(view.dashboardTab,2);wait(1);
  }
  compare(prepared,2);compare(destroyed,4);
 }
 function test_niri_cancel_foreign_owner_and_disabled_target_are_safe(){
  niriOwned();addEntry();NacreCompositor.windows[0].pid=99;
  controller.compositorMove(niriPacket("begin"));compare(controller.niriMove,null);
  NacreCompositor.windows[0].pid=88;controller.compositorMove(niriPacket("begin"));
  controller.compositorMove(niriPacket("cancel"));controller.compositorMove(niriPacket("end"));compare(controller.entries.length,1);
  controller.compositorMove(niriPacket("begin"));NacrePanelState.hidden=true;
  controller.compositorMove(niriPacket("end"));compare(controller.entries.length,1);
  NacrePanelState.hidden=false;controller.compositorMove(niriPacket("begin"));
  controller.compositorMove(niriPacket("end",{x:400,y:250,width:984,height:740}));compare(controller.entries.length,1);
  controller.compositorMove(niriPacket("begin"));const tiled=niriPacket("end");tiled.floating=false;
  controller.compositorMove(tiled);compare(controller.entries.length,1);
 }
 function test_dock_keeps_native_source_until_two_destination_frames(){
  surface.autoFrames=false;const entry=addEntry();controller.dock(entry,"test");
  compare(controller.pendingDocks.length,1);verify(entry.window.visible);verify(view.dashboard);
  surface.frameSwapped();verify(entry.window.visible);compare(controller.entries.length,1);compare(surface.requests,2);
  surface.frameSwapped();compare(controller.entries.length,0);verify(!entry.window.visible);wait(1);compare(destroyed,2);
 }
 function test_ready_but_unpainted_destination_cannot_hide_original(){
  const entry=addEntry();entry.window.painted=false;const host={handoffHidden:false};
  controller.drag={host:host,destination:entry};controller.handedOff(entry);
  verify(!host.handoffHidden);verify(controller.drag);controller.cancel();
 }
 function test_missing_destination_frame_keeps_native_window(){
  surface.autoFrames=false;const entry=addEntry();
  ignoreWarning("Nacre dock destination did not present; floating window retained");
  controller.dock(entry,"test");wait(550);
  compare(controller.entries.length,1);verify(entry.window.visible);compare(entry.transfer,null);verify(!view.dashboard);
 }
 function test_new_pull_interrupts_completed_magnetic_return(){
  const body={x:400,y:50,width:984,height:740};
  controller.drag={kind:"dashboard",host:{},screen:"test",body:body,dx:0,dy:0,ready:false,released:true,destination:null};
  const host={canDetach:true,mapToItem(){return {x:400,y:50}},Window:{window:{contentItem:{}}}};
  controller.begin("dashboard",host,"test",{x:600,y:200});verify(controller.drag && !controller.drag.released);
  controller.update({x:600,y:330});compare(controller.drag.dy,130);verify(controller.drag.ready);
 }
 function test_hidden_frame_has_no_magnetic_target(){
  addEntry();NacrePanelState.hidden=true;
  controller.nativeGesture(packet("begin",900,600));controller.nativeGesture(packet("move",900,390));
  compare(controller.magneticCue,null);compare(controller.nativeDrag.destination,null);
  controller.nativeGesture(packet("end",900,390));compare(controller.entries.length,1);
 }
 function test_click_without_pull_does_not_dock(){
  addEntry();controller.nativeGesture(packet("begin",900,600));controller.nativeGesture(packet("end",900,600));
  compare(controller.entries.length,1);compare(controller.nativeDrag,null);
 }
 function test_first_pull_returns_to_same_magnet(){
  const body={x:400,y:50,width:984,height:740};
  controller.drag={kind:"dashboard",host:{},screen:"test",start:{x:900,y:300},body:body,ready:false,released:false,magnet:{screen:"test",edge:"top",armed:true}};
  controller.update({x:900,y:370});verify(controller.drag.magnet.armed);verify(controller.magneticCue.armed);
  controller.finish();verify(controller.drag.released);compare(controller.drag.dy,0);
  wait(230);compare(controller.drag,null);compare(controller.entries.length,0);compare(controller.landingCues.length,0);
 }
}
""")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_floating_release_waits_for_painted_frames_not_placement_time(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        window = (SHELL / "modules/pearls/NacrePearlWindow.qml").read_text()
        barrier = window.split("    NacreFrameBarrier {", 1)[1].split(
            "    PersistentProperties {", 1
        )[0]
        barrier = "    NacreFrameBarrier {" + barrier
        barrier = barrier.replace("root.contentItem.Window.window", "root.testSurface")
        ready = window.split("    onReadyChanged:", 1)[1].split("    Connections {", 1)[
            0
        ]
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            (target / "NacreFrameBarrier.qml").write_text(
                (SHELL / "widgets/NacreFrameBarrier.qml").read_text()
            )
            (target / "qmldir").write_text(
                "singleton NacrePearls 1.0 NacrePearls.qml\nsingleton NacreCompositor 1.0 NacreCompositor.qml\n"
            )
            (target / "NacrePearls.qml").write_text("""pragma Singleton
import QtQuick
QtObject {property var drag:null;property int handoffs:0;property int failures:0;function handedOff(entry){handoffs++} function handoffFailed(entry){failures++}}
""")
            (target / "NacreCompositor.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property string backendName:"hyprland"}\n'
            )
            (target / "Destination.qml").write_text(
                'import QtQuick\nimport "."\nItem {id:root;property bool ready:false;property bool painted:false;property bool revealed:false;property var dropPoint:null;property var entry:({});property var testSurface;onReadyChanged:'
                + ready
                + barrier
                + "}\n"
            )
            (target / "tst_release.qml").write_text("""import QtQuick
import QtTest
import "."
TestCase {
 name:"FloatingReleaseFrames";when:windowShown
 QtObject {id:surface;property int requests:0;signal frameSwapped();function update(){requests++}}
 Component {id:factory;Destination {testSurface:surface}}
 function test_placement_does_not_drop_original_before_paint(){
  const win=factory.createObject(this);NacrePearls.handoffs=0;
  win.ready=true;wait(80);compare(NacrePearls.handoffs,0);verify(!win.painted);
  surface.frameSwapped();compare(NacrePearls.handoffs,0);verify(!win.painted);
  surface.frameSwapped();compare(NacrePearls.handoffs,1);verify(win.painted);
  surface.frameSwapped();compare(NacrePearls.handoffs,1);win.destroy();
 }
 function test_niri_first_placement_stays_hidden_while_native_spring_runs(){
  NacreCompositor.backendName="niri";NacrePearls.handoffs=0;
  const win=factory.createObject(this,{dropPoint:{exact:true}});NacrePearls.drag={destination:win.entry};win.ready=true;
  wait(200);verify(!win.revealed);compare(NacrePearls.handoffs,0);
  tryVerify(()=>win.revealed,500);surface.frameSwapped();surface.frameSwapped();
  verify(win.painted);compare(NacrePearls.handoffs,1);win.destroy();
  NacreCompositor.backendName="hyprland";NacrePearls.drag=null;
 }
 function test_missing_frame_returns_to_original(){
  const win=factory.createObject(this);NacrePearls.handoffs=0;NacrePearls.failures=0;
  win.ready=true;wait(550);compare(NacrePearls.handoffs,0);compare(NacrePearls.failures,1);verify(!win.painted);win.destroy();
 }
}
""")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_client_border_is_round_neutral_and_highlights_without_filling_body(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            (target / "NacrePearlBorder.qml").write_text(
                (SHELL / "widgets/NacrePearlBorder.qml").read_text()
            )
            (target / "NacreTokens.qml").write_text("""pragma Singleton
import QtQuick
QtObject {property bool motionEnabled:false;property color outline:"#777777";property color accent:"#2080e0";property color orientSecondary:"#e060a0"}
""")
            (target / "qmldir").write_text(
                "singleton NacreTokens 1.0 NacreTokens.qml\n"
            )
            (target / "tst_border.qml").write_text("""import QtQuick
import QtTest
import "."
TestCase {
 name:"ClientPearlBorder";width:260;height:220;visible:true;when:windowShown
 Rectangle {id:scene;anchors.fill:parent;color:"#10141b";NacrePearlBorder {id:border;x:20;y:20;width:200;height:160}}
 function test_stroke_and_focus_gradient(){
  wait(50);let image=grabImage(scene);
  verify(image.pixel(120,20).a>.8);verify(Math.abs(image.pixel(120,20).r-image.pixel(120,20).b)<.03);
  compare(image.pixel(120,100),image.pixel(10,10));compare(image.pixel(20,20),image.pixel(10,10));
  border.engaged=true;wait(50);image=grabImage(scene);
  verify(image.pixel(70,20).b>image.pixel(70,20).r+.1);
  verify(image.pixel(218,140).r>image.pixel(218,140).b);
  compare(image.pixel(120,100),image.pixel(10,10));
 }
}
""")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={
                    **os.environ,
                    "QT_QPA_PLATFORM": "offscreen",
                    "QT_QUICK_BACKEND": "rhi",
                    "QSG_RHI_BACKEND": "opengl",
                },
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_private_layout_atomic_permissions_and_corruption_preservation(self):
        state = module("pearl-state")
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "private" / "pearls.json"
            self.assertEqual(state.load(path)["windows"], [])
            value = {
                "version": 1,
                "windows": [
                    {"kind": "launcher", "initial": {"query": "private search"}}
                ],
                "ai": {"edge": "pfirst", "channels": ["", "pfirst"]},
            }
            state.save(path, value)
            self.assertEqual(state.load(path), value)
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertEqual(path.parent.stat().st_mode & 0o777, 0o700)
            path.write_text("bad saved data")
            with self.assertRaises(ValueError):
                state.load(path)
            self.assertEqual(path.read_text(), "bad saved data")

    def test_ai_namespaces_preserve_default_and_reject_path_injection(self):
        sidebar = module("sidebar-chat")
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            with patch.multiple(
                sidebar,
                DEFAULT_STATE=base / "state",
                DEFAULT_SOCKET=base / "default.sock",
            ):
                sidebar.configure_channel("pfirst")
                self.assertEqual(sidebar.STATE, base / "state/views/pfirst")
                self.assertEqual(sidebar.SOCKET.name, "nacre-chat-pfirst.sock")
                for value in ("../escape", "/absolute", "p;bad", "x" * 49):
                    with self.assertRaises(ValueError):
                        sidebar.configure_channel(value)
                sidebar.configure_channel("")
                self.assertEqual(sidebar.STATE, base / "state")
                self.assertEqual(sidebar.SOCKET, base / "default.sock")

    def test_actual_geometry_matching_edges_and_negative_monitor_origins(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            (target / "geometry.js").write_text(
                (SHELL / "modules/pearls/geometry.js").read_text()
            )
            (target / "tst_pearl.qml").write_text("""import QtQuick
import QtTest
import "geometry.js" as Geometry
TestCase {
 name:"PearlDockGeometry"
 function test_panel_edge_magnet_and_hysteresis(){
  const screens=[{name:"left",x:-1920,y:0,width:1920,height:1080,frame:{top:50,bottom:10,left:10,right:10}}, {name:"laptop",x:0,y:0,width:1920,height:1200,frame:{top:50,bottom:10,left:10,right:10}}];
  const body={x:400,y:112,width:1000,height:600};
  const point={x:900,y:412}; // pointer far from the edge; panel is 62px away
  const cue=Geometry.magnet("dashboard",body,point,screens);
  verify(cue.armed);compare(cue.distance,62);compare(cue.origin.y,50);
  verify(!Geometry.magnet("dashboard",{x:400,y:140,width:1000,height:600},point,screens).armed);
  verify(Geometry.magnet("dashboard",{x:400,y:140,width:1000,height:600},point,screens,cue).armed);
  verify(!Geometry.magnet("dashboard",{x:400,y:160,width:1000,height:600},point,screens,cue).armed);
  compare(Geometry.magnet("dashboard",{x:400,y:221,width:1000,height:600},point,screens),null);
  verify(Geometry.magnet("launcher",{x:400,y:500,width:1000,height:630},{x:900,y:800},screens).armed);
  verify(Geometry.magnet("controls",{x:1400,y:300,width:450,height:600},{x:1500,y:500},screens).armed);
  const left=Geometry.magnet("ai",{x:-1860,y:200,width:450,height:600},{x:-1700,y:500},screens);
  verify(left.armed);compare(left.origin.x,-1910);
  compare(Geometry.magnet("ai",{x:800,y:20,width:450,height:600},point,screens),null);
  compare(Geometry.magnet("dashboard",{x:0,y:0,width:0,height:10},point,screens),null);
  compare(Geometry.magnet("dashboard",body,{x:NaN,y:0},screens),null);
  // A shared monitor seam selects one output, rather than flickering between two.
  const seam=Geometry.magnet("dashboard",{x:-200,y:100,width:400,height:600},{x:0,y:400},screens);
  compare(seam.screen,"laptop");
 }
 function test_pull_and_placement(){
  compare(Geometry.outward("ai",70,0),70);compare(Geometry.outward("controls",70,0),-70);
  compare(Geometry.outward("launcher",0,-70),70);
  const result=Geometry.place({x:-1910,y:0},{width:1000,height:700},{x:-1920,y:0,width:1920,height:1080});
  compare(result.x,-1908);compare(result.y,12);
 }
}""")
            result = subprocess.run(
                [str(runner), "-input", str(target)],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                text=True,
                capture_output=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
