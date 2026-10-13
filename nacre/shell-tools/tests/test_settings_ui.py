from qml_source import install_i18n, install_paths
from qml_source import remove_objects, install_foundation_interaction

"""Exercise settings navigation and the shared wheel handler in native Qt."""
import re
import os
import shutil
import subprocess
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

            def adapted(path, imports="fixtures"):
                return (
                    path.read_text()
                    .replace("import qs.widgets", 'import "' + imports + '"')
                    .replace("import qs.services", "")
                    .replace("import qs.utils", "")
                    .replace("import qs.config", "")
                    .replace("import Quickshell.Io", "")
                    .replace("import Quickshell.Services.Pipewire", "")
                    .replace("import Quickshell", "")
                    .replace(
                        "import qs.modules.bar.popouts as Connectivity",
                        'import "../connectivity" as Connectivity',
                    )
                    .replace("Quickshell.screens[0].height", "1080")
                    .replace("Quickshell.screens[0].width", "1920")
                    .replace('Quickshell.env("HOME")', '"/fixture"')
                    .replace('Quickshell.env("XDG_STATE_HOME")', '"/fixture/state"')
                )

            source = adapted(
                ROOT.parent / "shell/modules/settings/NacreSettingsView.qml"
            )
            source = source.replace('import "pages"', 'import "settings"')
            (target / "NacreSettings.qml").write_text(source)
            shutil.copy2(
                ROOT.parent / "shell/modules/settings/settings-catalog.js",
                target / "settings-catalog.js",
            )
            pages = target / "settings"
            pages.mkdir()
            for path in (ROOT.parent / "shell/modules/settings/pages").glob("*.qml"):
                source = (
                    adapted(path, "../fixtures")
                    .replace('import "../../dashboard/media"', 'import "../media"')
                    .replace(
                        'import "../../notifications"', 'import "../notifications"'
                    )
                )
                if path.name == "NacreAppearancePage.qml":
                    source = remove_objects(source, r"\bFileView\s*\{")
                (pages / path.name).write_text(source)
            connectivity = target / "connectivity"
            connectivity.mkdir()
            for name in ("NacreNetworkPopup", "NacreQuickList"):
                (connectivity / (name + ".qml")).write_text(
                    adapted(
                        ROOT.parent / "shell/modules/bar/popouts" / (name + ".qml"),
                        "../fixtures",
                    )
                )
            for folder, names in {
                "media": ["NacreMediaSlider", "NacreMediaButton"],
                "notifications": ["NacreNotice"],
            }.items():
                destination = target / folder
                destination.mkdir()
                for name in names:
                    source = adapted(
                        ROOT.parent
                        / "shell/modules"
                        / ("dashboard/media" if folder == "media" else folder)
                        / (name + ".qml"),
                        "../fixtures",
                    )
                    source = source.replace(
                        "Quickshell.iconPath(root.modelData.appIcon, true)", '""'
                    )
                    (destination / (name + ".qml")).write_text(source)
            (target / "fixtures/NacreNotifications.qml").write_text(
                (ROOT.parent / "shell/config/NacreNotifications.qml").read_text()
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
            preview = (
                adapted(ROOT.parent / "shell/lock-preview.qml")
                .replace('import "widgets"', 'import "fixtures"')
                .replace('import "services"', "")
                .replace("ShellRoot {", "Item {")
                .replace("FloatingWindow {", "Rectangle {")
            )
            preview = re.sub(
                r"(?m)^(\s*)title\s*:\s*([^\n]+)",
                r"\1property string title: \2",
                preview,
            )
            preview = remove_objects(preview, r"\bProcess\s*\{\s*id\s*:\s*reader\b")
            preview = remove_objects(preview, r"\bTimer\s*\{\s*interval\s*:\s*3000\b")
            (target / "LockPreview.qml").write_text(preview)
            (target / "fixtures/ActionButton.qml").write_text(
                adapted(ROOT.parent / "shell/widgets/ActionButton.qml", ".")
            )
            (target / "fixtures/NacreScrollBar.qml").write_text(
                (ROOT.parent / "shell/widgets/NacreScrollBar.qml").read_text()
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write("\nNacreScrollBar 1.0 NacreScrollBar.qml\n")
            services = {
                "NacreCompositor": "property var capabilities:({})",
                "NacrePanelState": 'property string settingsPage:"appearance";function openSettings(page){settingsPage=page}',
                "TimezoneSettings": 'property var status:({timezone:"UTC",localTime:"12:34",automatic:false,installed:false});property string message:"";property bool busy:false;property var changes:[];function refresh(){} function change(kind,value){changes=[...changes,[kind,value]]}',
                "NacreTime": "function format(pattern){return Qt.formatDateTime(new Date(2026,9,8,14,54),pattern)}",
                "NacreWallpapers": 'property string poster:"";property string preview:"";property string current:"";property var list:[]; readonly property var retained:list;property var preferences:({rotationEnabled:true,rotationMinutes:30,rotationKind:"all",rotationShuffle:true,palettePreset:"wallpaper"});property string selectedAccent:"";property var paletteOptions:[{accent:"aabbcc",name:"Blue",surface:"102030",swatches:["aabbcc","ccbbaa","abcabc"]}];property var palettePresets:[{id:"ocean",name:"Ocean",group:"vivid",surface:"102030",swatches:["aabbcc","ccbbaa","abcabc"]}];property bool themeBusy:false;property bool rotationReady:true;property string rotationStatus:"Next wallpaper at 12:30";property string error:"";function preference(value){preferences=Object.assign({},preferences,value)} function advanceRotation(manual){} function setWallpaper(path){}',
                "AppLaunch": "property var commands:[];function run(command){commands=[...commands,command]}",
                "DesktopActions": "property var actions:[];function execute(action){actions=[...actions,action]}",
                "NacreNetwork": 'property string error:"";property string monitorError:"";property bool busy:false;function refresh(){} property bool wifiAvailable:true;property bool wifiEnabled:true;property var active:null;property var networks:[];property var visibleNetworks:[];property string wifiInterface:""',
                "NacreBluetooth": "property bool available:true;property bool powered:false;property var devices:[]",
                "DeviceActions": 'property bool busy:false;property string message:"";property var requests:[];property string connectedSSID:"";function request(args){requests=[...requests,args]} function connectWifi(ssid){connectedSSID=ssid}',
                "NacreWeather": 'property string description:"";property string location:"";property string error:"";property string displayTemperature:"";function reload(){}',
                "NacreNotifs": 'property var list:[]; readonly property var retained:list;property int clears:0;property string dismissed:"";function clearHistory(){clears++} function dismiss(entry){dismissed=entry.key}',
                "Pipewire": "property var nodes:({values:[]});property var defaultAudioSink:null;property var defaultAudioSource:null;property var preferredDefaultAudioSink:null;property var preferredDefaultAudioSource:null",
            }
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
            (target / "fixtures/NacreIcon.qml").write_text("import QtQuick\nText {}")
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
            install_i18n(target / "fixtures")
            install_paths(target / "fixtures")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
