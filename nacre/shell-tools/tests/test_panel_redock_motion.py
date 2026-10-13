"""Exercise production panel size bindings without their provider/UI children."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest

from qml_fixtures import stage_qml, run_qml, install_i18n

SHELL = Path(__file__).resolve().parents[2] / "shell"


class RedockMotionTests(unittest.TestCase):
    def test_restored_panels_are_full_size_and_keep_future_open_close_motion(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            shutil.copytree(
                SHELL.parent / "shell-tools/tests/qml/fixtures",
                target,
                dirs_exist_ok=True,
            )
            fixtures = {
                "NacreTokens": 'property bool motionEnabled:true;property color body:"#101419";property color ink:"white";property color mutedInk:"gray";property color accent:"cyan";property color outline:"gray";property color raised:"#20242a"',
                "NacrePanelState": "property bool hidden:false",
                "NacreDashboardState": 'property var tabs:["today","weather","media","system","focus"]',
                "NacreAiViews": 'property var edge:QtObject{property bool busy:false;property string defaultProvider:"codex";function start(){}};property var recent:[];function select(value){}function load(key){}',
                "DesktopSettings": "property var data:({animations:true})",
                "NacreWallpapers": 'property var preferences:({layout:"carousel"})',
                "DesktopExtras": 'property string brainQuery:"";property var notes:[];property var chats:[];property var busy:({capture:false});property string captured:"";property string message:"";function request(kind,payload){}',
                "NacreNetwork": "property var active:null;property bool wifiEnabled:false;function refresh(){}",
                "NacreBluetooth": "property bool powered:false",
                "NacreBrightness": "function getMonitorForScreen(screen){return null}",
                "NacreNotifs": "property bool dnd:false;property var retained:[];function clearHistory(){}",
                "NacreControlTools": 'property bool busy:false;property var data:({});property string error:"";function capture(kind){}function toggleNightLight(){}',
                "NacreCompositor": "property var windows:[];function focusWindow(id){}",
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
            for name, path, _root_type in [
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
                stage_qml(SHELL / path, target / (name + ".qml"))
            fake_views = {
                "NacreTodayPage": "property bool shouldUpdate:false;property var visibilities",
                "NacreWeatherPage": "property bool shouldUpdate:false",
                "NacreMediaPage": "property bool shouldUpdate:false;property var visibilities",
                "NacreSystemPage": "property bool shouldUpdate:false",
                "NacreFocusPage": "property bool shouldUpdate:false",
                "NacreDashboardCustomize": "signal done",
                "NacreAppBrowser": "property var visibilities;property bool detached:false;property bool closeAfterLaunch:false;property real viewportWidth:0;property real viewportHeight:0;property var menu;signal closeRequested;signal queryChanged;signal routeChanged",
                "NacreWallpaperPicker": "property var visibilities;property bool detached:false",
                "NacreSearchPanel": "property var visibilities;property bool detached:false",
                "Palette": "property var visibilities;property bool detached:false",
                "Overview": "property var visibilities;property bool detached:false",
                "Clipboard": "property var visibilities;property bool detached:false",
                "Keybindings": "property var visibilities;property bool detached:false",
                "ChatPane": "property var visibilities;property var chat;property bool active:false",
                "NacreControlLevels": "property var monitor",
                "NacreControlTile": 'property string label:"";property string detail:"";property string icon:"";property bool selected:false;signal clicked',
                "NacreNetworkPopup": "",
                "NacreBluetoothPopup": "",
                "NacreSoundPopup": "",
                "NacreBatteryPopup": "property bool profilesEnabled:true",
                "NacreHistoryPopup": "",
                "NacreNotice": "property var modelData;property bool history:false",
                "NacreInteraction": 'property string accessibleName:"";function onClicked(){}',
                "BrandLogo": "property bool ai:false",
            }
            with (target / "qmldir").open("a") as manifest:
                for name, body in fake_views.items():
                    extra = (
                        "property real viewportWidth:0;property real viewportHeight:0;"
                        if "viewportWidth" not in body
                        else ""
                    )
                    (target / (name + ".qml")).write_text(
                        "import QtQuick\nItem {implicitWidth:800;implicitHeight:300;"
                        + extra
                        + body
                        + "}\n"
                    )
                    manifest.write(f"{name} 1.0 {name}.qml\n")
            (target / "NacreIcon.qml").write_text("import QtQuick\nText {}")
            (target / "NacreScrollBar.qml").write_text(
                "import QtQuick.Controls\nScrollBar {}"
            )
            with (target / "qmldir").open("a") as manifest:
                manifest.write(
                    "NacreIcon 1.0 NacreIcon.qml\nNacreScrollBar 1.0 NacreScrollBar.qml\n"
                )
            scroll = target / "FastScroll.qml"
            scroll.write_text(
                scroll.read_text().replace("Item {", "Item {function cancel(){}", 1)
            )
            button = target / "ActionButton.qml"
            button.write_text(
                button.read_text().replace(
                    "property bool selected:",
                    "property var accessibleLabel:"
                    ";property bool compact:false;property bool selected:",
                )
            )
            (target / "tst_motion.qml").write_text("""import QtQuick
import QtTest
import Quickshell
import "."
TestCase {
 id:test;name:"RestoredPanelMotion";width:1920;height:1150;visible:true;when:windowShown
 Component {id:dashboard;Dashboard{}}
 Component {id:launcher;Launcher{}}
 Component {id:sidebar;Sidebar{}}
 Component {id:controls;Controls{}}
 Component {id:flags;PersistentProperties{property bool dashboard:false;property bool launcher:false;property bool left:false;property bool leftPinned:false;property int dashboardTab:0;property string launcherMode:"apps";property string launcherRoute:"home";property bool previewOnly:false}}
 function test_motion_data(){return [{tag:"dashboard",factory:dashboard,field:"dashboard",axis:"height",target:650},{tag:"launcher",factory:launcher,field:"launcher",axis:"height",target:300},{tag:"sidebar",factory:sidebar,field:"left",axis:"width",target:470},{tag:"controls",factory:controls,field:"visibility",axis:"width",target:424}];}
 function test_motion(data){
  const view=flags.createObject(test);
  const screen=Qt.createQmlObject('import Quickshell;ShellScreen{height:1200}',test);
  const args=data.tag==="controls"?{visibility:false,screen:screen}:{visibilities:view};
  if(data.tag==="sidebar")args.screen=screen;
  const panel=createTemporaryObject(data.factory,test,args);verify(panel);
  panel.restoreImmediately=true;
  if(data.tag==="controls")panel.visibility=true;else view[data.field]=true;
  if(panel.restoreAttached)panel.restoreAttached();else panel.prepareLift();
  compare(panel[data.axis],data.target);verify(!panel.opening);
  wait(25);compare(panel[data.axis],data.target);
  if(data.tag==="controls")panel.visibility=false;else view[data.field]=false;
  tryCompare(panel,data.axis,0);
  let started=false;panel.openingChanged.connect(()=>{if(panel.opening)started=true});
  panel.restoreImmediately=false;
  if(data.tag==="controls")panel.visibility=true;else view[data.field]=true;
  tryVerify(()=>started);tryCompare(panel,data.axis,data.target);
 }
}
""")
            install_i18n(target)
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                timeout=20,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
