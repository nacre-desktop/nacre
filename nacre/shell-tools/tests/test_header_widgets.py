from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Independent header content/workspace row with safe native-service fixtures."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest

from qml_fixtures import install_foundation_interaction

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class HeaderWidgetTests(unittest.TestCase):
    def test_workspace_state_header_routes_and_hover_map_ownership(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
            install_foundation_interaction(fixtures, SHELL / "widgets")
            for name in ("NacreSurface", "NacreText"):
                shutil.copy2(
                    SHELL / "widgets" / (name + ".qml"), fixtures / (name + ".qml")
                )
            (fixtures / "NacreIcon.qml").write_text(
                "import QtQuick\nText {width:20;height:22}"
            )
            (fixtures / "BrandLogo.qml").write_text(
                "import QtQuick\nItem {property bool compact:false;property bool ai:false;property bool brain:false;property bool colorsApp:false;property bool settings:false;implicitWidth:32;implicitHeight:30}"
            )
            (fixtures / "NacreActiveTitle.qml").write_text(
                'import QtQuick\nItem {objectName:"fixtureActiveTitle";property color colour:"cyan";property bool horizontal:false;property var monitor:null}'
            )
            (fixtures / "NacrePowerButton.qml").write_text(
                "import QtQuick\nItem {property bool routed:false;property bool selected:false;signal activated(bool pointerOwned);implicitWidth:30;implicitHeight:30}"
            )
            (target / "NacreStatusHint.qml").write_text(
                'import QtQuick\nItem {property var output;property string text:"";property real center:0}'
            )
            (fixtures / "NacreStatusIcons.qml").write_text("""import QtQuick
Item {
 id:root
 property bool horizontal:false
 property bool grouped:false
 property string revealName:""
 property real revealProgress:0
 property string screenName:""
 signal activated(string name,var item,bool pointerOwned)
 signal peekRequested(string name,var item,bool hovered)
 signal interactionStarted
 implicitWidth:24;implicitHeight:152
 readonly property Item recordingItem:r
 Item{id:r;visible:false;property string label:"Stop recording 1:23"}
 readonly property Item audioItem:a
 readonly property Item network:n
 readonly property Item bluetoothItem:b
 readonly property Item battery:c
 readonly property Item notificationsItem:d
 Column {Item{id:a;objectName:"fixtureHeaderAudio";width:24;height:24}Item{id:n;objectName:"fixtureHeaderNetwork";width:24;height:24;HoverHandler{blocking:false;onHoveredChanged:root.peekRequested("network",n,hovered)}}Item{id:b;width:24;height:24}Item{id:c;width:24;height:36}Item{id:d;width:24;height:36}}
}""")
            definitions = {
                "NacreMenu": "property QtObject workflows:QtObject {property var calls:[];function work(action){calls=[...calls,action]}}",
                "ChatWindowTitle": "property var titles:({})",
                "NacrePresentation": "property var active:({workspaceColors:false})",
                "NacreIcons": 'function getDesktopEntry(name){return {categories:name==="kitty"?["System","TerminalEmulator"]:name==="firefox"?["Network","WebBrowser"]:name==="discord"?["Network","InstantMessaging"]:[]}}function getAppCategoryIcon(name,fallback){return name==="editor"?"code":fallback}',
                "NacreCompositor": 'property int activeWsId:2;property var capabilities:({});property var monitors:({values:[]});property var workspaces:({values:[]});property var windows:[];property var focusedMonitor:({name:"test"});property var activeWindow:null;property var requests:[];function switchWorkspace(value){requests=[...requests,"workspace "+value];return true}function focusWindow(address){requests=[...requests,"focus "+address];return true}',
                "NacreBar": "property int workspaceCount:5;property var workspaceNames:[];property var workspaceIcons:[]",
                "NacreBrightness": "function getMonitorForScreen(screen){return null}",
                "NacreTime": 'function format(value){return "12:34"}',
                "Updates": 'property int count:3;property string message:""',
                "NacrePearls": "property var entries:[]",
                "AppLaunch": "property var calls:[];function run(command){calls=[...calls,command]}",
                "NacrePanelState": 'property bool hidden:false;property var screens:({test:{session:false,launcher:false,dashboard:false,dashboardPinned:false,edgeMenu:""}});property var panels:({});property var calls:[];property bool controlsPointerOwned:false;property var popupRequests:[];function openMode(mode,q,preview){calls=[...calls,mode]}function popout(name,center,screen){calls=[...calls,name];popupRequests=[...popupRequests,{name:name,center:center,screen:screen}];return true}function openDeviceSettings(name){calls=[...calls,name];return ["audio","network","bluetooth"].includes(name)}function openControls(name,screen,forceOpen,pointerOwned){controlsPointerOwned=pointerOwned===true;calls=[...calls,name]}function hoverEdge(name,screen){calls=[...calls,name]}function openEdge(name,screen){calls=[...calls,name]}',
            }
            with (fixtures / "qmldir").open("a") as manifest:
                for name in (
                    "NacreIcon",
                    "BrandLogo",
                    "NacreActiveTitle",
                    "NacreStatusIcons",
                    "NacrePowerButton",
                ):
                    manifest.write(f"\n{name} 1.0 {name}.qml\n")
                for name, body in definitions.items():
                    (fixtures / (name + ".qml")).write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                    )
                    manifest.write(f"\nsingleton {name} 1.0 {name}.qml\n")
                stage_qml(
                    SHELL / "services/NacreHoverIntent.qml",
                    fixtures / "NacreHoverIntent.qml",
                )
                manifest.write(
                    "\nsingleton NacreHoverIntent 1.0 NacreHoverIntent.qml\n"
                )
            for name in (
                "NacreHeader",
                "NacreWorkspaceRow",
                "NacreWorkspacePearl",
                "NacreWorkspaceAppIcon",
                "NacreHeaderForwarder",
            ):
                stage_qml(
                    SHELL / "modules/topbar" / (name + ".qml"), target / (name + ".qml")
                )
            (target / "PeekFixture.qml").write_text(
                'import QtQuick\nItem {property var output;property string kind:"";property real center:0}'
            )
            shutil.copy2(
                SHELL / "utils/scripts/window-title.js", target / "window-title.js"
            )
            shutil.copy2(
                ROOT / "tests/header-qml/tst_header.qml", target / "tst_header.qml"
            )
            install_i18n(fixtures)
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
