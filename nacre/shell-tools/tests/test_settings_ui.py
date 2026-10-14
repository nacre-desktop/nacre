from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n
from qml_fixtures import install_foundation_interaction

"""Exercise settings navigation and the shared wheel handler in native Qt."""
import os
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class SettingsUITests(unittest.TestCase):
    def test_pages_and_shared_scrolling(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test is unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            shutil.copytree(ROOT / "tests/qml/fixtures", target / "fixtures")

            (target / "NacreSettings.qml").write_text("""import QtQuick
import "fixtures"
Item {
 id: root
 property bool active: true
 property string page: "appearance"
 property string query: ""
 function open(id) {motion.cancel();page=id;scroll.contentY=0;query=""}
 Flickable {
  id:scroll;objectName:"settingsScroll";anchors.fill:parent;contentWidth:width;contentHeight:loaded.height;clip:true
  Loader {id:loaded;objectName:"settingsPage";width:parent.width;active:root.active && !root.query;sourceComponent:({appearance:appearance,desktop:desktop,displays:displays,sound:sound,network:network,bluetooth:bluetooth,notifications:notifications,workflows:workflows,lock:lock,time:time,ai:ai,maintenance:maintenance})[root.page]}
  FastScroll {id:motion;view:scroll}
 }
 Component {id:appearance;NacreAppearancePage {}}
 Component {id:desktop;NacreDesktopPage {}}
 Component {id:displays;NacreDisplaysPage {}}
 Component {id:sound;NacreSoundPage {}}
 Component {id:network;NacreNetworkPage {}}
 Component {id:bluetooth;NacreBluetoothPage {}}
 Component {id:notifications;NacreNotificationPage {}}
 Component {id:workflows;NacreWorkflowsPage {}}
 Component {id:lock;NacreLockPage {}}
 Component {id:time;NacreTimePage {}}
 Component {id:ai;NacreAiPage {}}
 Component {id:maintenance;NacreMaintenancePage {}}
}
""")
            for path in (ROOT.parent / "shell/modules/settings/pages").glob("*.qml"):
                stage_qml(path, target / path.name)
            for name in ("NacreNetworkPopup", "NacreQuickList"):
                stage_qml(
                    ROOT.parent / "shell/modules/bar/popouts" / (name + ".qml"),
                    target / (name + ".qml"),
                )
            for folder, names in {
                "dashboard/media": ["NacreMediaSlider", "NacreMediaButton"],
                "notifications": ["NacreNotice"],
            }.items():
                for name in names:
                    stage_qml(
                        ROOT.parent / "shell/modules" / folder / (name + ".qml"),
                        target / (name + ".qml"),
                    )
            stage_qml(
                ROOT.parent / "shell/config/NacreNotifications.qml",
                target / "fixtures/NacreNotifications.qml",
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write(
                    "\nsingleton NacreNotifications 1.0 NacreNotifications.qml\n"
                )
            sidebar = target / "fixtures/SidebarChat.qml"
            sidebar.write_text(
                sidebar.read_text()
                .replace(
                    "property string defaultProvider:",
                    "property var choices:[]\n    property string defaultProvider:",
                )
                .replace(
                    "function setProvider(name) {",
                    "function setProvider(name) { choices=[...choices,name];defaultProvider=name;",
                )
            )
            stage_qml(
                ROOT.parent / "shell/lock-preview.qml", target / "LockPreview.qml"
            )
            stage_qml(
                ROOT.parent / "shell/widgets/ActionButton.qml",
                target / "fixtures/ActionButton.qml",
            )
            stage_qml(
                ROOT.parent / "shell/widgets/NacreScrollBar.qml",
                target / "fixtures/NacreScrollBar.qml",
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write("\nNacreScrollBar 1.0 NacreScrollBar.qml\n")
            services = {
                "NacreDashboardState": 'property bool weatherPhotos:false;property bool extrasReady:true;property var coreTabs:["today","media","weather","system"];readonly property var tabs:[...coreTabs,...extraOrder.filter(id=>enabledExtras.includes(id))];function toggleTab(id){const next=coreTabs.includes(id)?coreTabs.filter(x=>x!==id):[...coreTabs,id];if(next.length)coreTabs=next}function reorder(id,delta){const items=[...coreTabs],index=items.indexOf(id),next=Math.max(0,Math.min(items.length-1,index+delta));items.splice(index,1);items.splice(next,0,id);coreTabs=items}property var enabledExtras:[];property var extraOrder:["focus","notifications","clipboard","devices","network","ai-usage","screenshots","brain"];function toggleExtra(id){enabledExtras=enabledExtras.includes(id)?enabledExtras.filter(x=>x!==id):[...enabledExtras,id]}function reorderExtra(id,delta){const items=[...extraOrder],index=items.indexOf(id),next=Math.max(0,Math.min(items.length-1,index+delta));items.splice(index,1);items.splice(next,0,id);extraOrder=items}',
                "NacreCompositor": "property var capabilities:({})",
                "NacrePanelState": 'property string settingsPage:"appearance";function openSettings(page){settingsPage=page}',
                "TimezoneSettings": 'property var status:({timezone:"UTC",localTime:"12:34",automatic:false,installed:false});property string message:"";property bool busy:false;property var changes:[];function refresh(){} function change(kind,value){changes=[...changes,[kind,value]]}',
                "NacreTime": "function format(pattern){return Qt.formatDateTime(new Date(2026,9,8,14,54),pattern)}",
                "NacreWallpapers": 'property string poster:"";property string preview:"";property string current:"";property var list:[]; readonly property var retained:list;property var preferences:({rotationEnabled:true,rotationMinutes:30,rotationKind:"all",rotationShuffle:true,palettePreset:"wallpaper"});property string selectedAccent:"";property var paletteOptions:[{accent:"aabbcc",name:"Blue",surface:"102030",swatches:["aabbcc","ccbbaa","abcabc"]}];property var palettePresets:[{id:"ocean",name:"Ocean",group:"vivid",surface:"102030",swatches:["aabbcc","ccbbaa","abcabc"]}];property bool themeBusy:false;property bool rotationReady:true;property string rotationStatus:"Next wallpaper at 12:30";property string error:"";function preference(value){preferences=Object.assign({},preferences,value)} function advanceRotation(manual){} function setWallpaper(path){}',
                "AppLaunch": "property var commands:[];function run(command){commands=[...commands,command]}",
                "DesktopActions": "property var actions:[];function execute(action){actions=[...actions,action]}",
                "NacreNetwork": 'property bool wiredConnected:false;property string error:"";property string monitorError:"";property bool busy:false;function refresh(){} property bool wifiAvailable:true;property bool wifiEnabled:true;property var active:null;property var networks:[];property var visibleNetworks:[];property string wifiInterface:""',
                "NacreBluetooth": "property bool available:true;property bool powered:false;property var devices:[]",
                "DeviceActions": 'property bool busy:false;property string message:"";property var requests:[];property string connectedSSID:"";function request(args){requests=[...requests,args]} function connectWifi(ssid){connectedSSID=ssid}',
                "NacreWeather": 'property string description:"";property string location:"";property string error:"";property string displayTemperature:"";function reload(){}',
                "NacreNotifs": 'property var list:[]; readonly property var retained:list;property int clears:0;property string dismissed:"";function clearHistory(){clears++} function dismiss(entry){dismissed=entry.key}',
                "Pipewire": "property var nodes:({values:[]});property var defaultAudioSink:null;property var defaultAudioSource:null;property var preferredDefaultAudioSink:null;property var preferredDefaultAudioSource:null",
            }
            services["NacreControlActions"] = (
                "readonly property bool busy:DeviceActions.busy;readonly property string message:DeviceActions.message;function request(a){DeviceActions.request(a)}function connectWifiInline(p,password){DeviceActions.connectedSSID=p.ssid;DeviceActions.busy=true;Qt.callLater(()=>DeviceActions.busy=false);return true}function cancelWifiInline(){}"
            )
            for name, body in services.items():
                (target / "fixtures" / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                )
            (target / "fixtures/PwObjectTracker.qml").write_text(
                "import QtQuick\nQtObject {property var objects:[]}"
            )
            (target / "fixtures/BrandLogo.qml").write_text(
                "import QtQuick\nItem {property bool settings:false;property bool notifications:false;property color background:Qt.rgba(0,0,0,1);implicitWidth:26;implicitHeight:26}"
            )
            (target / "fixtures/NacreIcon.qml").write_text(
                "import QtQuick\nText {property real fill:0}"
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                for name in services:
                    manifest.write("\nsingleton " + name + " 1.0 " + name + ".qml")
                manifest.write(
                    "\nPwObjectTracker 1.0 PwObjectTracker.qml\nBrandLogo 1.0 BrandLogo.qml\nNacreIcon 1.0 NacreIcon.qml\n"
                )
            shutil.copy2(
                ROOT.parent / "shell/widgets/FastScroll.qml",
                target / "fixtures/FastScroll.qml",
            )
            (target / "fixtures/NacreInteraction.qml").write_text(
                "import QtQuick\nMouseArea {anchors.fill:parent}"
            )
            (target / "fixtures/DesktopSettings.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property var data:({});property string message:"";property var monitors:[{name:"eDP-1",width:1920,height:1080}];property bool pending:false;property bool busy:false;property var lastRequest:[];property var writes:[];function set(key,value){writes=[...writes,{key:key,value:value}];data=Object.assign({},data,{[key]:value})} function request(args){lastRequest=args}}'
            )
            (target / "fixtures/Maintenance.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property var data:({});property string message:"";property bool busy:false;property string lastAction:"";function refresh(){} function request(action){lastAction=action} function recover(action){lastAction=action}}'
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write(
                    "\nsingleton DesktopSettings 1.0 DesktopSettings.qml\nsingleton Maintenance 1.0 Maintenance.qml\n"
                )
            install_foundation_interaction(
                target / "fixtures", ROOT.parent / "shell/widgets"
            )
            shutil.copy2(
                ROOT / "tests/qml/tst_settings.qml", target / "tst_settings.qml"
            )
            stage_qml(
                ROOT.parent / "shell/services/NacreDashboardRegistry.qml",
                target / "fixtures/NacreDashboardRegistry.qml",
            )
            stage_qml(
                ROOT.parent / "shell/modules/dashboard/NacreDashboardExtrasOptions.qml",
                target / "NacreDashboardExtrasOptions.qml",
            )
            install_i18n(target / "fixtures")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
