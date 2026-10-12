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
active={pid=45,title="Nacre Pearl Dashboard 12"}
-- Immediately after detaching, the FIRST native pull must move and end/dock.
assert(down().ok and moves==1 and timer.enabled and #events==1)
cursor={x=800,y=10};timer.callback();assert(#events==2)
up();assert(moves==1 and #events==3)
assert(events[3]=="nacre_pearl_drag,end,Nacre Pearl Dashboard 12,800,10")
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
 function test_matching_edge_only(){
  const screens=[{name:"left",x:-1920,y:0,width:1920,height:1080},{name:"laptop",x:0,y:0,width:1920,height:1200}];
  compare(Geometry.docking("ai",{x:-1910,y:500},screens).screen,"left");
  compare(Geometry.docking("dashboard",{x:800,y:12},screens).screen,"laptop");
  compare(Geometry.docking("launcher",{x:800,y:1190},screens).edge,"bottom");
  compare(Geometry.docking("controls",{x:1910,y:500},screens).edge,"right");
  compare(Geometry.docking("ai",{x:800,y:12},screens),null);
  compare(Geometry.docking("launcher",{x:2200,y:1200},screens),null);
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
