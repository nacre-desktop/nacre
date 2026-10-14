from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Actual media page with native Qt controls and service contracts."""

import os
from pathlib import Path
from PIL import Image
import shutil
import tempfile
import unittest

from dashboard_fixtures import prepare_dashboard

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


def prepare_pages(target):
    prepare_dashboard(target)
    fixtures = target / "fixtures"
    (fixtures / "NacrePlayers.qml").write_text(
        "pragma Singleton\nimport QtQuick\nQtObject {property var list:[];property var active:null;property var manualActive:null;property var streams:[];function streamFor(player){return streams.find(row=>row.player===player)||null}onManualActiveChanged:if(manualActive)active=manualActive}"
    )
    (fixtures / "NacreSystemUsage.qml").write_text(
        'pragma Singleton\nimport QtQuick\nQtObject {property real cpuPerc:.15;property real cpuTemp:23;property real gpuTemp:NaN;property real gpuPerc:0;property bool gpuUsageAvailable:false;property real memPerc:.48;property int memUsed:15309210;property int memTotal:32191283;property real storagePerc:.52;property int storageUsed:190840832;property int storageTotal:367001600;property string loadAverage:".10 .20 .30";property string kernel:"Fixture kernel"}'
    )
    stage_qml(SHELL / "widgets/ActionButton.qml", fixtures / "ActionButton.qml")
    for name, body in {
        "NacreMediaDetails": 'property int mediaSpectrumUsers:0;property var palette:({});property var lines:[];property var spectrum:[];property string message:"";property string plain:""',
        "NacreDashboardState": 'property bool weatherPhotos:false;property var focusState:({status:"idle"});property bool busy:false;readonly property bool running:focusState.status==="running";property string timerText:"24:12";property real progress:0;property var calls:[];function start(minutes){calls=[...calls,minutes];focusState={status:"running"}}function end(){calls=[...calls,"stop"];focusState={status:"idle"}}function request(action){calls=[...calls,action];focusState={status:action==="focus-pause"?"paused":"running"}}property var preferences:({albumPalette:true,focusMinutes:25});function save(key,value){preferences=Object.assign({},preferences,{[key]:value})}',
        "NacreAudio": "property var outputs:[];property var sink:null;function chooseOutput(id){sink=outputs.find(node=>String(node.id)===id);return !!sink}",
        "NacreWeatherPhoto": 'property int consumers:0;property string image:"";property string author:"Demo photographer";property string license:"CC0";property string source:""',
        "NacreWallpapers": 'property string poster:""',
        "WallpaperPlayback": "property bool sleeping:false;property bool locked:false",
    }.items():
        (fixtures / (name + ".qml")).write_text(
            "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
        )
        with (fixtures / "qmldir").open("a") as manifest:
            manifest.write(f"\nsingleton {name} 1.0 {name}.qml\n")
    stage_qml(SHELL / "widgets/NacreScrollBar.qml", fixtures / "NacreScrollBar.qml")
    with (fixtures / "qmldir").open("a") as manifest:
        manifest.write("\nNacreScrollBar 1.0 NacreScrollBar.qml\n")
    (fixtures / "NacreWeather.qml").write_text(
        'pragma Singleton\nimport QtQuick\nQtObject {property var reading:({isDay:true,wmo:0,hours:[],days:[]});property real temperature:12;property string description:"Clear";property string location:"Demo city";property string detail:"";function degrees(n){return Number.isFinite(n)?Math.round(n)+"°C":"—"}}'
    )
    Image.new("RGB", (32, 32), "gray").save(target / "city.png")
    for name in ("NacreMediaPage", "NacreWeatherSky", "NacreWeatherPage"):
        stage_qml(
            SHELL / "modules/dashboard" / (name + ".qml"), target / (name + ".qml")
        )
    stage_qml(
        SHELL / "modules/dashboard/extras/NacreFocusExtra.qml",
        target / "NacreFocusExtra.qml",
    )
    (target / "media").mkdir(exist_ok=True)
    for path in (SHELL / "modules/dashboard/media").iterdir():
        if path.suffix == ".js":
            shutil.copy2(path, target / "media" / path.name)
        else:
            stage_qml(path, target / "media" / path.name)


class MediaUITests(unittest.TestCase):
    def test_control_transactions_resource_data_and_layout(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            prepare_pages(target)
            shutil.copy2(
                ROOT / "tests/media-performance-qml/tst_pages.qml",
                target / "tst_pages.qml",
            )
            install_i18n(target / "fixtures")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_actual_player_service_follows_playback_and_matches_local_streams(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            prepare_pages(target)
            fixtures = target / "fixtures"
            stage_qml(
                SHELL / "services/NacrePlayers.qml", fixtures / "NacrePlayers.qml"
            )
            (fixtures / "NacreAudio.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var playbackStreams:[]}"
            )
            (fixtures / "NacreShortcut.qml").write_text(
                "import QtQuick\nQtObject {property string appid;property string name;signal pressed()}"
            )
            with (fixtures / "qmldir").open("a") as stream:
                stream.write("NacreShortcut 1.0 NacreShortcut.qml\n")
            (target / "tst_players.qml").write_text("""import QtQuick
import QtTest
import Quickshell.Services.Mpris
import "fixtures"
TestCase {
 name:"AutomaticMediaPlayers";when:windowShown
 QtObject {id:spotify;property string identity:"Spotify";property string desktopEntry:"spotify";property bool isPlaying:true;property bool canTogglePlaying:true;property int calls:0;function togglePlaying(){calls++;isPlaying=!isPlaying}}
 QtObject {id:chrome;property string identity:"Google Chrome";property string desktopEntry:"google-chrome";property bool isPlaying:false;property bool canTogglePlaying:true;property int calls:0;function togglePlaying(){calls++;isPlaying=!isPlaying}}
 function test_follow_new_playback_keep_manual_choice_and_route_controls(){
  Mpris.players={values:[spotify,chrome]};NacreAudio.playbackStreams=[{ready:true,name:"Google Chrome",properties:{"application.process.binary":"chrome"}}];
  compare(NacrePlayers.active,spotify);NacrePlayers.manualActive=spotify;
  chrome.isPlaying=true;tryCompare(NacrePlayers,"active",chrome);
  verify(NacrePlayers.streamFor(chrome));compare(NacrePlayers.streamFor(spotify),null);
  verify(NacrePlayers.control("playPause"));compare(chrome.calls,1);compare(spotify.calls,0);verify(!chrome.isPlaying);
  NacrePlayers.manualActive=spotify;chrome.isPlaying=true;tryCompare(NacrePlayers,"active",chrome);
  NacrePlayers.manualActive=spotify;compare(NacrePlayers.active,spotify);
  chrome.isPlaying=false;chrome.isPlaying=true;tryCompare(NacrePlayers,"active",chrome);
  // A newly registered already-playing app also releases a stale choice.
  Mpris.players={values:[spotify]};NacrePlayers.manualActive=spotify;Mpris.players={values:[spotify,chrome]};tryCompare(NacrePlayers,"active",chrome);
  NacreAudio.playbackStreams=[{ready:false,name:"Chrome",properties:{"application.name":"Google Chrome"}}];compare(NacrePlayers.streamFor(chrome),null);
  NacreAudio.playbackStreams=[{ready:true,name:"Audio",properties:{"application.id":"spotify"}}];verify(NacrePlayers.streamFor(spotify));compare(NacrePlayers.streamFor(chrome),null);
  spotify.desktopEntry="com.spotify.Client";
  NacreAudio.playbackStreams=[{ready:true,name:"Audio",properties:{"application.id":"org.other.Client"}}];compare(NacrePlayers.streamFor(spotify),null);
  NacreAudio.playbackStreams=[{ready:true,name:"Audio",properties:{"application.id":"com.spotify.Client"}}];verify(NacrePlayers.streamFor(spotify));
  Mpris.players={values:[spotify]};tryCompare(NacrePlayers,"active",spotify);
  Mpris.players={values:[]};compare(NacrePlayers.active,null);compare(NacrePlayers.control("playPause"),false);
 }
}
""")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
