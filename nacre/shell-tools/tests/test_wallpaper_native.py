"""Actual Quickshell hexagonal and rounded image clipping, independent of Qt mocks."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class WallpaperNativeTests(unittest.TestCase):
    def test_hexagon_and_rounded_pixels_with_real_renderer(self):
        binary = shutil.which("quickshell")
        if not binary:
            self.skipTest("Quickshell unavailable")
        try:
            from PIL import Image
        except ImportError:
            self.skipTest("Pillow unavailable")
        wrapper = []
        if not os.environ.get("DISPLAY"):
            if not shutil.which("xvfb-run"):
                self.skipTest("OpenGL display unavailable")
            wrapper = ["xvfb-run", "-a"]
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            for name in ("widgets", "services", "launcher"):
                (target / name).mkdir()
            for name in ("NacreTokens", "NacreClip", "NacreText"):
                shutil.copy2(
                    SHELL / "widgets" / (name + ".qml"),
                    target / "widgets" / (name + ".qml"),
                )
            for name in ("NacreWallpaperHex", "NacreWallpaperMotion"):
                shutil.copy2(
                    SHELL / "modules/launcher" / (name + ".qml"),
                    target / "launcher" / (name + ".qml"),
                )
            image = target / "solid.png"
            Image.new("RGB", (64, 64), "#28794d").save(image)
            palette = {
                "m3surface": "#202226",
                "m3surfaceContainer": "#303640",
                "m3frame": "#353840",
                "m3onSurface": "#fafafa",
                "m3onSurfaceVariant": "#aabbcc",
                "m3primary": "#7696ff",
                "m3outline": "#888888",
            }
            (target / "services/NacreColours.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property bool light:false;property var palette:"
                + json.dumps(palette)
                + "}"
            )
            (target / "services/DesktopSettings.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var data:({animations:false})}"
            )
            (target / "shell.qml").write_text(
                """import QtQuick
import Quickshell
import qs.widgets
import qs.launcher
ShellRoot {
 FloatingWindow {
  visible:true;implicitWidth:400;implicitHeight:220
  Item {
   id:sheet;anchors.fill:parent
   Rectangle {anchors.fill:parent;color:"#eceaf0"}
   NacreWallpaperHex {x:20;y:20;width:200;height:174;entry:({poster:POSTER,name:"",dynamic:false})}
   NacreClip {x:260;y:20;width:110;height:110;radius:24;Image{anchors.fill:parent;source:"file://"+POSTER;fillMode:Image.PreserveAspectCrop}}
  }
  Timer {interval:450;running:true;onTriggered:sheet.grabToImage(result=>{if(!result.saveToFile("capture.png"))throw new Error("save failed");console.log("WALLPAPER_NATIVE_OK");Qt.quit();})}
 }
}""".replace("POSTER", json.dumps(str(image)))
            )
            environment = {
                **os.environ,
                "QT_LOGGING_RULES": "qml.debug=true;scene.debug=true",
                "QT_QPA_PLATFORM": "offscreen",
                "QT_QUICK_BACKEND": "rhi",
                "QSG_RHI_BACKEND": "opengl",
                "QT_SCALE_FACTOR": "1",
                "QT_FONT_DPI": "96",
                "XDG_CACHE_HOME": str(target / "cache"),
            }
            environment.pop("WAYLAND_DISPLAY", None)
            result = subprocess.run(
                [*wrapper, binary, "-p", str(target / "shell.qml")],
                cwd=target,
                env=environment,
                capture_output=True,
                text=True,
                timeout=20,
            )
            output = result.stdout + result.stderr
            self.assertEqual(result.returncode, 0, output)
            self.assertIn("WALLPAPER_NATIVE_OK", output)
            self.assertNotIn("ERROR", output)
            for line in output.splitlines():
                if "WARN" in line:
                    self.assertIn("does not support setting window masks", line)
            with Image.open(target / "capture.png") as image:
                pixels = image.convert("RGB")
                self.assertEqual(pixels.getpixel((22, 22)), (236, 234, 240))
                self.assertEqual(pixels.getpixel((120, 90)), (40, 121, 77))
                self.assertEqual(pixels.getpixel((32, 106)), (40, 121, 77))
                self.assertEqual(pixels.getpixel((261, 21)), (236, 234, 240))
                self.assertEqual(pixels.getpixel((315, 75)), (40, 121, 77))
