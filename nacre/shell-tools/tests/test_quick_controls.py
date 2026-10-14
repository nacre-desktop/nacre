from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n
from qml_fixtures import install_foundation_interaction

"""Real popup controls and navigation, with fake devices to preserve live connections."""

import importlib.util
import os
from pathlib import Path
import shutil
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("actions", ROOT / "device-actions.py")
actions = importlib.util.module_from_spec(spec)
spec.loader.exec_module(actions)


class QuickControlsTests(unittest.TestCase):
    def test_disconnect_is_bounded_to_a_valid_interface(self):
        self.assertEqual(
            actions.command("wifi-disconnect", "wlan0"),
            ["nmcli", "device", "disconnect", "wlan0"],
        )
        for interface in ["", "-all", "wlan0;reboot", "wlan0\n", "x" * 65]:
            with self.assertRaises(ValueError):
                actions.command("wifi-disconnect", interface)

    def test_network_status_ignores_ethernet_and_disconnected_wifi(self):
        with patch.object(
            actions.subprocess,
            "check_output",
            side_effect=[
                "enabled\n",
                "eth0:ethernet:connected\nwlan1:wifi:disconnected\nwlan0:wifi:connected\n",
            ],
        ):
            self.assertEqual(
                actions.network_status(), {"enabled": True, "interface": "wlan0"}
            )
        with patch.object(
            actions.subprocess,
            "check_output",
            side_effect=["disabled\n", "wlan0:wifi:disconnected\n"],
        ):
            self.assertEqual(
                actions.network_status(), {"enabled": False, "interface": ""}
            )

    def test_popups_and_settings_navigation_in_native_qt(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            shutil.copytree(ROOT / "tests/qml/fixtures", target / "fixtures")

            for name, component in [
                ("Audio", "NacreSoundPopup"),
                ("Network", "NacreNetworkPopup"),
                ("Bluetooth", "NacreBluetoothPopup"),
            ]:
                stage_qml(
                    ROOT.parent / "shell/modules/bar/popouts" / (component + ".qml"),
                    target / (name + "Popup.qml"),
                )
            for component in (
                "NacreSoundPopup",
                "NacreNetworkPopup",
                "NacreBluetoothPopup",
            ):
                stage_qml(
                    ROOT.parent / "shell/modules/bar/popouts" / (component + ".qml"),
                    target / (component + ".qml"),
                )
            for component in (
                "NacrePopupContent",
                "NacrePopupPanel",
                "NacreHistoryPopup",
                "NacreBatteryPopup",
                "NacreCalendarPopup",
            ):
                stage_qml(
                    ROOT.parent / "shell/modules/bar/popouts" / (component + ".qml"),
                    target / (component + ".qml"),
                )
            shutil.copy2(
                ROOT.parent / "shell/modules/dashboard/overview/overview.js",
                target / "overview.js",
            )
            stage_qml(
                ROOT.parent / "shell/modules/bar/popouts/NacreQuickList.qml",
                target / "NacreQuickList.qml",
            )
            stage_qml(
                ROOT.parent / "shell/modules/bar/popouts/NacreQuickSlider.qml",
                target / "NacreQuickSlider.qml",
            )
            for component in (
                "NacreControlCenter",
                "NacreControlLevels",
                "NacreControlTile",
                "NacreControlOverview",
                "NacreControlDetail",
                "NacreControlDisplays",
                "NacreControlVpn",
                "NacreControlButton",
                "NacreControlSlider",
                "NacreControlSwitch",
                "NacreControlAppearance",
                "NacreControlMedia",
                "NacreWifiSaved",
                "NacreBluetoothPairing",
            ):
                stage_qml(
                    ROOT.parent / "shell/modules/controlcenter" / (component + ".qml"),
                    target / (component + ".qml"),
                )
            stage_qml(
                ROOT.parent / "shell/widgets/NacreRangeSlider.qml",
                target / "fixtures/NacreRangeSlider.qml",
            )
            for name in ["ActionButton", "NacreInteraction", "NacreScrollBar"]:
                stage_qml(
                    ROOT.parent / "shell/widgets" / (name + ".qml"),
                    target / "fixtures" / (name + ".qml"),
                )
            (target / "fixtures/NacreIcon.qml").write_text(
                "import QtQuick\nText {property real fill:0}"
            )
            (target / "fixtures/PwObjectTracker.qml").write_text(
                "import QtQuick\nQtObject {property var objects:[]}"
            )
            colors = (
                (target / "fixtures/NacreColours.qml")
                .read_text()
                .replace(
                    'm3primary: "cyan",',
                    'm3primary: "cyan", m3primaryContainer:"#303050",',
                )
            )
            (target / "fixtures/NacreColours.qml").write_text(colors)
            services = {
                "NacreAudio": 'property var source:({ready:true,description:"Desk microphone"});property var sink:({ready:true,description:"Speakers"});property real volume:.7;property real micVolume:.4;property bool muted:false;property bool micMuted:false;property bool available:true;property bool micAvailable:true;property int writes:0;function setVolume(v){writes++;volume=v} function setMicVolume(v){writes++;micVolume=v} function toggleMute(){muted=!muted} function toggleMic(){micMuted=!micMuted}',
                "Pipewire": 'property var nodes:({values:[{ready:true,audio:({}),isStream:false,isSink:true,description:"Speakers",name:"speaker"}]});property var defaultAudioSink:nodes.values[0];property var preferredDefaultAudioSink:null;property var defaultAudioSource:null;property var preferredDefaultAudioSource:null',
                "NacreNetwork": 'property string error:"";property string monitorError:"";property bool busy:false;function refresh(){} property bool wifiAvailable:true;property bool wifiEnabled:true;property string wifiInterface:"wlan0";property var active:({ssid:"ab"});property var networks:[{bssid:"AA:BB:CC:DD:EE:01",ssid:"ab",active:false,strength:90},{bssid:"AA:BB:CC:DD:EE:02",ssid:"Guest",active:false,strength:65},{bssid:"AA:BB:CC:DD:EE:03",ssid:"ab",active:true,strength:30}];readonly property var visibleNetworks:Wifi.group(networks)',
                "NacreBluetooth": 'property bool available:true;property bool powered:true;property var devices:[{name:"Headphones",alias:"Headphones",address:"AA:BB:CC:DD:EE:FF",connected:true,paired:true,trusted:true},{name:"Unpaired",alias:"Unpaired",address:"00:11:22:33:44:55",connected:false,paired:false,trusted:false}]',
                "DeviceActions": 'property bool busy:false;property string message:"";property string lastAction:"";property var lastRequest:[];function request(a){lastRequest=a;lastAction=a[0]} function connectWifi(ssid){lastRequest=["wifi-connect",ssid]} function connectWifiInline(point,password){lastRequest=["wifi-inline",point.ssid,password];return true}function cancelWifiInline(){}function setAirplane(on){lastRequest=["airplane",on]}',
                "NacreCompositor": 'property var focusedMonitor:({name:"test"})',
                "DesktopSettings": "property var data:({});property var writes:[];function set(key,value){writes=[...writes,[key,value]]}",
                "NacreHoverIntent": "property int dismissed:0;function dismiss(screen){dismissed++}",
                "NacreNotifs": "property var retained:[];property bool dnd:false;property bool historyReady:true;property int clears:0;function clearHistory(){clears++;retained=[]}",
                "NacreTime": "property date date:new Date(2026,9,9,12,34)",
                "NacreBar": "property var sizes:({batteryWidth:200})",
                "UPower": "property bool onBattery:true;property var displayDevice:({ready:true,isLaptopBattery:true,percentage:.5,timeToEmpty:3600,timeToFull:0})",
                "PowerProfiles": 'property int profile:1;property bool hasPerformanceProfile:true;property string degradationReason:""',
            }
            services["NacreControlActions"] = services["DeviceActions"]
            services.update(
                {
                    "NacreBrightness": 'property var monitors:[device];property QtObject device:QtObject {property string name:"Display";property bool available:true;property real brightness:.8;property string error:"";property var writes:[];function setBrightness(v){writes=[...writes,v];brightness=v}}function getMonitorForScreen(screen){return device}',
                    "NacreKeyboardLight": 'property bool available:true;property real brightness:.5;property string error:"";property var writes:[];function setBrightness(v){writes=[...writes,v];brightness=v}',
                    "NacreControlTools": 'property var data:({screenshot:true,colorPicker:true,powerProfilesSupported:true,nightLightSupported:true,nightLightExternal:false,nightLightEnabled:false});property string error:"";property bool busy:false;property var captures:[];property int writes:0;function capture(kind){captures=[...captures,kind]}function toggleNightLight(){writes++}',
                    "NacreVpn": 'property var profiles:[];property var connected:[];property bool busy:false;property string error:"";function refresh(){}function toggle(){}',
                    "NacreMenu": 'property var snapshot:({stayAwake:false,recording:false,capabilities:({record:false})});property string message:"";function work(action,value){}',
                    "NacreWallpapers": 'property var preferences:({paletteMode:"dark"})',
                    "NacrePlayers": "property var active:null;function control(action){}",
                    "NacreControlPreferences": 'property var optionalTiles:[];property string error:"";function toggle(id){optionalTiles=optionalTiles.includes(id)?optionalTiles.filter(value=>value!==id):[...optionalTiles,id]}',
                    "NacrePanelState": 'property var calls:[];function openMode(mode){calls=[...calls,mode]}function openSettings(page){calls=[...calls,"settings"]}',
                }
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write(
                    "\nNacreRangeSlider 1.0 NacreRangeSlider.qml\nNacreScrollBar 1.0 NacreScrollBar.qml\n"
                )
            for name, body in services.items():
                (target / "fixtures" / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                )
            shutil.copy2(
                ROOT.parent / "shell/utils/scripts/wifi-networks.js",
                target / "wifi-networks.js",
            )
            path = target / "fixtures/NacreNetwork.qml"
            path.write_text(
                path.read_text().replace(
                    "import QtQuick",
                    'import QtQuick\nimport "../wifi-networks.js" as Wifi',
                )
            )
            (target / "fixtures/NacreNotice.qml").write_text(
                "import QtQuick\nItem {required property var modelData;property bool history:false;implicitWidth:360;implicitHeight:modelData.expanded?200:60}"
            )
            (target / "fixtures/PowerProfile.qml").write_text(
                "import QtQuick\nQtObject {enum Kind {PowerSaver, Balanced, Performance}}"
            )
            stage_qml(
                ROOT.parent / "shell/config/NacreAppearance.qml",
                target / "fixtures/NacreAppearance.qml",
            )
            stage_qml(
                ROOT.parent / "shell/services/NacrePanelState.qml",
                target / "fixtures/NacrePanelState.qml",
            )
            with (target / "fixtures/qmldir").open("a") as f:
                for name in [*services, "NacrePanelState"]:
                    f.write(f"\nsingleton {name} 1.0 {name}.qml")
                f.write(
                    "\nNacreNotice 1.0 NacreNotice.qml\nPowerProfile 1.0 PowerProfile.qml\nNacreIcon 1.0 NacreIcon.qml\nNacreInteraction 1.0 NacreInteraction.qml\nPwObjectTracker 1.0 PwObjectTracker.qml\n"
                )
            shutil.copy2(
                ROOT / "tests/qml/tst_quick_controls.qml",
                target / "tst_quick_controls.qml",
            )
            install_foundation_interaction(
                target / "fixtures", ROOT.parent / "shell/widgets"
            )
            shutil.copy2(
                ROOT.parent / "shell/widgets/FastScroll.qml",
                target / "fixtures/FastScroll.qml",
            )
            stage_qml(
                ROOT.parent / "shell/widgets/NacreScrollBar.qml",
                target / "fixtures/NacreScrollBar.qml",
            )
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write("\nNacreScrollBar 1.0 NacreScrollBar.qml\n")
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
