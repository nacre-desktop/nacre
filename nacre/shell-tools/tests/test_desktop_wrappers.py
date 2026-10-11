"""Actual wrapper/control/poster code with native Qt and safe fixture owners."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from qml_source import install_foundation_interaction, remove_objects

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class DesktopWrappersTests(unittest.TestCase):
    def test_controls_lifetime_events_session_and_poster_buffers(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Native Qt Quick Test unavailable")
        try:
            from PIL import Image
        except ImportError:
            self.skipTest("Pillow unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
            install_foundation_interaction(fixtures, SHELL / "widgets")
            for name in ("NacreText", "NacreSurface", "NacreIcon", "NacreSlider"):
                shutil.copy2(
                    SHELL / "widgets" / (name + ".qml"), fixtures / (name + ".qml")
                )
            definitions = {
                "NacreAudio": 'property bool available:false;property bool micAvailable:true;property real volume:0;property real micVolume:.5;property bool muted:false;property bool micMuted:false;property var writes:[];function setVolume(v){writes=[...writes,v];volume=v}function setMicVolume(v){writes=[...writes,v];micVolume=v}function toggleMute(){writes=[...writes,"mute"];muted=!muted}function toggleMic(){writes=[...writes,"mic"];micMuted=!micMuted}',
                "NacreKeyboardLight": 'property bool available:true;property real brightness:.5;property string error:"";signal adjusted();property var writes:[];function setBrightness(v){writes=[...writes,v];brightness=v}',
                "NacreBrightness": 'property QtObject device:QtObject {property bool available:true;property real brightness:.8;property string error:"";property var writes:[];signal adjusted();function setBrightness(v){writes=[...writes,v];brightness=v}}function getMonitorForScreen(screen){return device}',
                "NacreOsd": "property int hideDelay:80",
                "WallpaperPlayback": "property bool locked:false;property bool sleeping:false;property bool paused:false;property bool batteryPaused:false;property bool pauseCovered:true",
                "NacreHyprland": 'property var focusedMonitor:({name:"test"});property var monitors:({values:[]});property var clients:[];property int activeWsId:1',
                "NacrePanelState": "property var screens:({});property bool hidden:false",
                "AppLaunch": "property var calls:[];function run(command){calls=[...calls,command]}",
                "NacreWallpapers": 'property string pendingPoster:"";property bool displayDynamic:false;property bool displayAnimated:false;property string displayPath:""',
                "NacreFrame": 'property color colour:"#101014"',
                "NacrePresentation": 'property int revision:0;property var pending:({});property var active:({});property var activations:[];function canonicalPoster(value){return value.startsWith("file://")?decodeURIComponent(value.slice(7)):value}function activate(path){if(path!==pending.poster)return false;active=pending;activations=[...activations,path];return true}',
            }
            with (fixtures / "qmldir").open("a") as manifest:
                for name in ("NacreIcon", "NacreSlider"):
                    manifest.write(f"\n{name} 1.0 {name}.qml\n")
                for name, body in definitions.items():
                    (fixtures / (name + ".qml")).write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                    )
                    manifest.write(f"\nsingleton {name} 1.0 {name}.qml\n")
            components = {
                "osd": ["NacreOsdEvents"],
                "session": ["NacreSessionControls", "NacreSessionPanel"],
                "background": ["NacreWallpaperScene", "NacreDesktopVideo"],
            }
            for area, names in components.items():
                for name in names:
                    source = (SHELL / "modules" / area / (name + ".qml")).read_text()
                    for imported in ("qs.widgets", "qs.services", "qs.config"):
                        source = source.replace(
                            "import " + imported, 'import "fixtures"'
                        )
                    source = source.replace("import Quickshell.Io", "")
                    source = remove_objects(source, r"\bIpcHandler\s*\{")
                    (target / (name + ".qml")).write_text(source)
            (target / "NacreLevelNotice.qml").write_text(
                'import QtQuick\nItem {required property var output;property bool shown:false;property string channel:"volume"}'
            )
            for name, color in [
                ("first.png", "#b53535"),
                ("second.png", "#2f7ab4"),
                ("percent % picture.png", "#3c9654"),
            ]:
                Image.new("RGB", (64, 48), color).save(target / name)
            shutil.copy2(
                ROOT / "tests/wrappers-qml/tst_wrappers.qml",
                target / "tst_wrappers.qml",
            )
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_native_muted_video_first_frame_and_pause(self):
        binary = shutil.which("quickshell")
        ffmpeg = shutil.which("ffmpeg")
        if not binary or not ffmpeg:
            self.skipTest("Quickshell/ffmpeg unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            shutil.copy2(
                SHELL / "modules/background/NacreDesktopVideo.qml",
                target / "NacreDesktopVideo.qml",
            )
            subprocess.run(
                [
                    ffmpeg,
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "color=red:s=64x48:r=12:d=2",
                    "-f",
                    "lavfi",
                    "-i",
                    "sine=frequency=440:duration=2",
                    "-threads",
                    "1",
                    "-c:v",
                    "mpeg4",
                    "-c:a",
                    "aac",
                    "-shortest",
                    str(target / "sample.mp4"),
                ],
                check=True,
                capture_output=True,
                timeout=15,
            )
            (target / "shell.qml").write_text("""import QtQuick
import Quickshell
ShellRoot {
    FloatingWindow {
        visible:true
        implicitWidth:64
        implicitHeight:48
        NacreDesktopVideo {
            id:video
            anchors.fill:parent
            path:Qt.resolvedUrl("sample.mp4").toString().slice(7)
            running:true
        }
        Timer {
            interval:50
            repeat:true
            running:true
            onTriggered: if(video.frameReady && video.position>80) {
                stop();video.running=false;
                if(video.failure || video.playing || !video.audioDisabled) throw new Error("video did not pause");
                console.log("NACRE_VIDEO_FIRST_FRAME_PAUSE_OK");Qt.quit();
            }
        }
        Timer {interval:6000;running:true;onTriggered:{throw new Error("no video frame")}}
    }
}""")
            result = subprocess.run(
                [binary, "-p", str(target / "shell.qml")],
                cwd=target,
                env={
                    **os.environ,
                    "QT_QPA_PLATFORM": "offscreen",
                    "QT_LOGGING_RULES": "qml.debug=true;scene.debug=true",
                },
                capture_output=True,
                text=True,
                timeout=10,
            )
            output = result.stdout + result.stderr
            self.assertEqual(result.returncode, 0, output)
            self.assertIn("NACRE_VIDEO_FIRST_FRAME_PAUSE_OK", output)
            for marker in ("TypeError", "ReferenceError", "Binding loop", "ERROR"):
                self.assertNotIn(marker, output)
