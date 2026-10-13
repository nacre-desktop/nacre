from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Exercise actual independent service code with native dependency fixtures."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]


class DeviceServiceTests(unittest.TestCase):
    def run_service(self, name):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            definitions = {
                "NacreWelcomeApp": 'property bool active:false;property string page:"home";property var demoEntries:[]',
                "NacrePanelState": 'property bool settingsVisible:false;property var screens:({});readonly property var providerViews:Object.values(screens);property var panels:({});property string settingsPage:"notifications";property int closes:0;function close(){closes++}',
                "Hyprland": "property var toplevels: QtObject {property var values:[]};property var workspaces:({values:[]});property var monitors:({values:[]});property var focusedMonitor:null;property var focusedWorkspace:null;property var activeToplevel:null;property bool usingLua:true;property var requests:[];property int refreshes:0;signal rawEvent(var event);function dispatch(value){requests=[...requests,value]}function refreshToplevels(){refreshes++}function refreshMonitors(){}function refreshWorkspaces(){}",
                "NacrePresentation": "property var pending:({});property var active:({});property bool available:false;function accept(record){pending=record;return true}",
                "NacrePaths": 'property string state:"file:///fixture";property string pictures:"file:///fixture"',
                "Environment": 'property var screens:[{name:"eDP-1"}]',
                "NacreBrightness": "property bool controlsVisible:false;property var hardware:({keyboard:null})",
                "NacreCompositor": 'property var focusedMonitor:({name:"eDP-1"});function createShortcut(owner,params){return Environment.compositor ? Environment.compositor.createShortcut(owner,params) : null}',
                "DesktopEntries": "property var applications:({values:[]})",
                "LauncherPreferences": "property var hidden:[]",
                "AppLaunch": "property var calls:[];function run(command,cwd){calls=[...calls,{command:command,cwd:cwd}]}",
                "DesktopSettings": "property var data:({dnd:false})",
                "WallpaperPlayback": "property bool sleeping:false;property bool locked:false",
                "NacreIcons": 'function getWeatherIcon(code){return Number(code)===113?"clear_day":"cloud"}',
                "NacreNotifications": "property bool expire:true;property int defaultExpireTimeout:5000",
                "Mpris": "property var players:({values:[]})",
                "Pipewire": "property var nodes:({values:[]});property var defaultAudioSink:null;property var defaultAudioSource:null",
                "Bluetooth": "property var devices:({values:[]});property var defaultAdapter:null",
            }
            if name == "DeviceActions":
                definitions.update(
                    {
                        "NacreNetwork": "property int refreshes:0;function refresh(){refreshes++}",
                        "NacreBluetooth": "function refresh(){}",
                    }
                )
            for kind, body in definitions.items():
                (fixtures / (kind + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                )
            (fixtures / "PwObjectTracker.qml").write_text(
                "import QtQuick\nQtObject {property var objects:[]}"
            )
            (fixtures / "Process.qml").write_text(
                "import QtQuick\nQtObject {property var command:[];property var environment:({});property bool running:false;property QtObject stdout;property QtObject stderr;signal exited(int exitCode,int exitStatus);signal started();property bool stdinEnabled:false;property var writes:[];function write(text){writes=[...writes,text]}property int starts:0;onRunningChanged:if(running){starts++;Qt.callLater(()=>{if(running)started()})}}"
            )
            (fixtures / "NacreHelperTask.qml").write_text(
                "import QtQuick\nProcess {signal output(string text)}"
            )
            if name == "NacreWallpapers":
                (fixtures / "NacrePaletteTask.qml").write_text(
                    "import QtQuick\nProcess {property var transport:null;stdout:SplitParser{}}"
                )
                with (fixtures / "qmldir").open("a") as manifest:
                    manifest.write("NacrePaletteTask 1.0 NacrePaletteTask.qml\n")
            (fixtures / "StdioCollector.qml").write_text(
                'import QtQuick\nQtObject {property string text:"";signal streamFinished()}'
            )
            (fixtures / "SplitParser.qml").write_text(
                "import QtQuick\nQtObject {property string splitMarker;signal read(string data)}"
            )
            (fixtures / "qmldir").write_text(
                "singleton NacreWelcomeApp 1.0 NacreWelcomeApp.qml\nsingleton NacrePresentation 1.0 NacrePresentation.qml\nsingleton NacrePaths 1.0 NacrePaths.qml\nsingleton Environment 1.0 Environment.qml\nsingleton NacreBrightness 1.0 NacreBrightness.qml\nsingleton NacreCompositor 1.0 NacreCompositor.qml\nsingleton Hyprland 1.0 Hyprland.qml\nsingleton DesktopEntries 1.0 DesktopEntries.qml\nsingleton LauncherPreferences 1.0 LauncherPreferences.qml\nsingleton AppLaunch 1.0 AppLaunch.qml\nsingleton DesktopSettings 1.0 DesktopSettings.qml\nsingleton NacreNotifications 1.0 NacreNotifications.qml\nsingleton NacrePanelState 1.0 NacrePanelState.qml\nsingleton Mpris 1.0 Mpris.qml\nsingleton Pipewire 1.0 Pipewire.qml\nsingleton Bluetooth 1.0 Bluetooth.qml\nPwObjectTracker 1.0 PwObjectTracker.qml\nProcess 1.0 Process.qml\nStdioCollector 1.0 StdioCollector.qml\nSplitParser 1.0 SplitParser.qml\nNacreHelperTask 1.0 NacreHelperTask.qml\n"
            )
            if name == "DeviceActions":
                with (fixtures / "qmldir").open("a") as manifest:
                    manifest.write(
                        "singleton NacreNetwork 1.0 NacreNetwork.qml\nsingleton NacreBluetooth 1.0 NacreBluetooth.qml\n"
                    )
            (fixtures / "FileView.qml").write_text(
                'import QtQuick\nQtObject {property string path:"";property bool printErrors:false;property bool watchChanges:false;signal loadFailed(int error);signal fileChanged();signal loaded();function text(){return ""}function reload(){}}'
            )
            with (fixtures / "qmldir").open("a") as manifest:
                manifest.write(
                    "\nFileView 1.0 FileView.qml\nsingleton NacreIcons 1.0 NacreIcons.qml\nsingleton WallpaperPlayback 1.0 WallpaperPlayback.qml\n"
                )
            if name in ("NacreColours", "NacrePresentation"):
                shutil.copy2(
                    ROOT.parent / "shell/services/colour-data.js",
                    target / "colour-data.js",
                )
            if name in ("NacreLightChannel", "NacreBrightness", "NacreKeyboardLight"):
                for component in ("NacreLightChannel", "NacreBacklight"):
                    stage_qml(
                        ROOT.parent / "shell/services" / (component + ".qml"),
                        target / (component + ".qml"),
                    )
            if name == "NacrePresentation":
                (fixtures / "NacrePresentation.qml").unlink()
                manifest = fixtures / "qmldir"
                manifest.write_text(
                    manifest.read_text().replace(
                        "singleton NacrePresentation 1.0 NacrePresentation.qml\n", ""
                    )
                )
            if name == "NacreBrightness":
                manifest = fixtures / "qmldir"
                (fixtures / "NacreBrightness.qml").unlink()
                manifest.write_text(
                    manifest.read_text().replace(
                        "singleton NacreBrightness 1.0 NacreBrightness.qml\n", ""
                    )
                )
            if name == "HyprlandBackend":
                shutil.copy2(
                    ROOT.parent / "shell/services/compositor/HyprlandWindow.qml",
                    target / "HyprlandWindow.qml",
                )
                stage_qml(
                    ROOT.parent / "shell/services/compositor/HyprlandShortcut.qml",
                    target / "HyprlandShortcut.qml",
                )
                stage_qml(
                    ROOT.parent / "shell/widgets/NacreShortcut.qml",
                    target / "NacreShortcut.qml",
                )
                environment = fixtures / "Environment.qml"
                environment.write_text(
                    environment.read_text().replace(
                        "QtObject {", "QtObject {property var compositor:null;"
                    )
                )
                compositor = fixtures / "NacreCompositor.qml"
                compositor.write_text(
                    compositor.read_text().replace(
                        "QtObject {",
                        "QtObject {",
                    )
                )
                (fixtures / "ShortcutRegistry.qml").write_text("""pragma Singleton
import QtQuick
QtObject {
 property var live: []; property int created:0; property int destroyed:0; property int duplicates:0;
 function add(value){if(live.some(entry=>entry.appid===value.appid && entry.name===value.name))duplicates++;live=[...live,value];created++}
 function remove(value){live=live.filter(entry=>entry!==value);destroyed++}
}
""")
                (fixtures / "GlobalShortcut.qml").write_text("""import QtQuick
QtObject {
 id: root
 property string appid:""; property string name:""; property string description:""; property string triggerDescription:"";
 signal pressed(); signal released();
 Component.onCompleted: ShortcutRegistry.add(root)
 Component.onDestruction: ShortcutRegistry.remove(root)
}
""")
                with (fixtures / "qmldir").open("a") as manifest:
                    manifest.write(
                        "\nGlobalShortcut 1.0 GlobalShortcut.qml\nsingleton ShortcutRegistry 1.0 ShortcutRegistry.qml\n"
                    )
            if name in ("NacreApps", "NacreWallpapers"):
                shutil.copy2(
                    ROOT.parent / "shell/services/app-search.js",
                    target / "app-search.js",
                )
            if name == "NacreTime":
                (fixtures / "SystemClock.qml").write_text(
                    "import QtQuick\nQtObject {enum Precision {Seconds, Minutes} property int precision:SystemClock.Minutes;property bool enabled:true;property date date:new Date(2026,9,9,12,34,56);readonly property int hours:date.getHours();readonly property int minutes:date.getMinutes();readonly property int seconds:precision===SystemClock.Seconds?date.getSeconds():0}"
                )
                with (fixtures / "qmldir").open("a") as manifest:
                    manifest.write("\nSystemClock 1.0 SystemClock.qml\n")
            if name == "NacreNotifs":
                stage_qml(
                    ROOT.parent / "shell/services/NacreNotificationEntry.qml",
                    target / "NacreNotificationEntry.qml",
                )
                (fixtures / "NotificationServer.qml").write_text(
                    "import QtQuick\nQtObject {property bool keepOnReload:false;property bool actionsSupported:false;property bool imageSupported:false;property bool persistenceSupported:false;property bool bodyMarkupSupported:false;signal notification(var notification)}"
                )
                with (fixtures / "qmldir").open("a") as manifest:
                    manifest.write("\nNotificationServer 1.0 NotificationServer.qml\n")
            if name == "NacreSystemUsage":
                shutil.copy2(
                    ROOT.parent / "shell/services/resource-data.js",
                    target / "resource-data.js",
                )
            if name == "NacreWallpapers":
                # Keep this dependency inside the same temporary directory.
                stage_qml(
                    ROOT.parent / "shell/utils/scripts/wallpaper-rotation.js",
                    target / "wallpaper-rotation.js",
                )
            stage_qml(
                ROOT.parent
                / "shell/services"
                / ("compositor" if name == "HyprlandBackend" else "")
                / (name + ".qml"),
                target / (name + ".qml"),
            )
            shutil.copy2(
                ROOT / "tests/device-qml" / ("tst_" + name + ".qml"),
                target / "tst_service.qml",
            )
            install_i18n(fixtures)
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                isolate=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_persistent_palette_transport_ready_stream_done_and_restart(self):
        self.run_service("NacrePaletteTask")

    def test_control_tools_discovery_explicit_actions_and_capture_lifetime(self):
        self.run_service("NacreControlTools")

    def test_audio_native_state_and_safe_writes(self):
        self.run_service("NacreAudio")

    def test_bluetooth_native_models_are_readonly_and_reactive(self):
        self.run_service("NacreBluetooth")

    def test_network_snapshot_publication_and_coalescing(self):
        self.run_service("NacreNetwork")

    def test_wifi_connection_window_is_tracked_and_refreshes_after_exit(self):
        self.run_service("DeviceActions")

    def test_media_selection_capabilities_and_removed_manual_player(self):
        self.run_service("NacrePlayers")

    def test_resources_counter_math_missing_data_and_visible_sampling(self):
        self.run_service("NacreSystemUsage")

    def test_notifications_history_races_expiry_and_durable_actions(self):
        self.run_service("NacreNotifs")

    def test_application_rank_hidden_membership_and_safe_native_commands(self):
        self.run_service("NacreApps")

    def test_clock_civil_format_precision_and_enable(self):
        self.run_service("NacreTime")

    def test_compositor_identity_metadata_focus_removal_and_lua_dispatch(self):
        self.run_service("HyprlandBackend")

    def test_palette_snapshot_aliases_authority_and_bad_data(self):
        self.run_service("NacreColours")

    def test_light_channel_user_only_coalescing_and_stale_epochs(self):
        self.run_service("NacreLightChannel")

    def test_screen_light_owner_mapping_and_closed_timer(self):
        self.run_service("NacreBrightness")

    def test_keyboard_light_raw_steps_cycle_and_closed_timer(self):
        self.run_service("NacreKeyboardLight")

    def test_matched_presentation_validation_clone_and_stale_readiness(self):
        self.run_service("NacrePresentation")

    def test_wallpaper_provider_catalogue_queue_and_matched_display(self):
        self.run_service("NacreWallpapers")

    def test_weather_cache_validation_units_and_coalesced_refresh(self):
        self.run_service("NacreWeather")
