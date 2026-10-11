"""Render actual independent frame geometry and input masks with Quickshell."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class FrameTests(unittest.TestCase):
    def test_registry_preserves_newer_and_other_output_owners(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            shutil.copy2(SHELL / "modules/drawers/registry.js", target / "registry.js")
            shutil.copy2(SHELL / "modules/drawers/layout.js", target / "layout.js")
            shutil.copy2(
                ROOT / "tests/frame-qml/tst_registry.qml", target / "tst_registry.qml"
            )
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_topbar_dismissal_consumes_modal_click_and_preserves_normal_input(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            source = (
                (SHELL / "modules/topbar/NacreHeaderForwarder.qml")
                .read_text()
                .replace("import qs.services", 'import "."')
            )
            (target / "NacreHeaderForwarder.qml").write_text(source)
            (target / "Forwarder.qml").write_text("""import QtQuick
import "."
Item {
 id:root;width:200;height:50
 property alias handler:forwarder.handler
 property alias statusItem:forwarder.statusItem
 property int clicks:0
 property alias childHovered:child.containsMouse
 MouseArea {id:child;anchors.fill:parent;hoverEnabled:true;onClicked:root.clicks++}
 NacreHeaderForwarder {id:forwarder;screen:({name:"test"})}
}""")
            (target / "NacrePanelState.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var panels:({})}\n"
            )
            (target / "qmldir").write_text(
                "singleton NacrePanelState 1.0 NacrePanelState.qml\n"
            )
            (target / "tst_forward.qml").write_text("""import QtQuick
import QtTest
import "."
TestCase {
 name:"TopbarModalForwarder";width:300;height:100;visible:true;when:windowShown
 Component{id:scene;Forwarder{}}
 QtObject{id:controller;property bool modal:true;property int calls:0;function outsideClick(point){calls++}}
 function test_modal_dismissal_does_not_activate_bar_control(){
  const view=createTemporaryObject(scene,this);
  NacrePanelState.panels={test:{input:controller}};
  controller.modal=true;controller.calls=0;
  wait(30);mouseMove(view,80,20);wait(10);
  mouseClick(view,80,20);
  compare(controller.calls,1);
  compare(view.clicks,0);
  controller.modal=false;
  mouseClick(view,80,20);
  compare(controller.calls,1);
  compare(view.clicks,1);
  controller.modal=true;
  view.statusItem=view;
  NacrePanelState.panels={test:{input:controller,popouts:{pinned:true}}};
  mouseClick(view,80,20);
  compare(controller.calls,1);
  compare(view.clicks,2);
  controller.modal=false;
  mouseMove(this,280,80);mouseMove(view,80,20);
  tryCompare(view,"childHovered",true);
 }
}
""")
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_native_geometry_theme_and_immediate_mask_release(self):
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
                self.skipTest("Native frame renderer needs OpenGL display or Xvfb")
            wrapper = ["xvfb-run", "-a"]
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            for name in ("widgets", "config", "services", "modules/drawers"):
                (target / name).mkdir(parents=True)
            for name in (
                "NacreChrome",
                "NacrePanelInput",
                "NacrePanelMask",
                "NacreFrameLips",
                "NacreFrameRim",
            ):
                shutil.copy2(
                    SHELL / "modules/drawers" / (name + ".qml"),
                    target / "modules/drawers" / (name + ".qml"),
                )
            shutil.copy2(
                SHELL / "modules/drawers/contour.js",
                target / "modules/drawers/contour.js",
            )
            shutil.copy2(
                SHELL / "widgets/NacreTokens.qml", target / "widgets/NacreTokens.qml"
            )
            shutil.copy2(
                SHELL / "widgets/NacreFrameLip.qml",
                target / "widgets/NacreFrameLip.qml",
            )
            helpers = {
                "services/NacreColours": "property bool light:false;property var palette:"
                + json.dumps(
                    {
                        "m3surface": "#191a20",
                        "m3surfaceContainer": "#303640",
                        "m3frame": "#353840",
                        "m3onSurface": "#fafafa",
                        "m3onSurfaceVariant": "#aabbcc",
                        "m3primary": "#7696ff",
                        "m3secondary": "#cf99bb",
                        "m3tertiary": "#88bbaa",
                        "m3outline": "#888888",
                    }
                ),
                "services/DesktopSettings": "property var data:({animations:false,leftDrawer:true,rightEdge:true})",
                "services/NacrePanelState": "property bool hidden:false",
                "services/NacreHyprland": "property var focusedMonitor:({name:'test'});property var activeClient:null",
                "config/NacreFrame": "property int left:10;property int right:10;property int bottom:10;property int headerHeight:50;property int rounding:20",
            }
            for name, body in helpers.items():
                (target / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                )
            intent = (
                (SHELL / "services/NacreHoverIntent.qml")
                .read_text()
                .replace("import Quickshell", "")
                .replace("Singleton {", "QtObject {")
            )
            (target / "services/NacreHoverIntent.qml").write_text(intent)
            shutil.copy2(ROOT / "tests/frame-qml/render.qml", target / "shell.qml")
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
            self.assertIn("FRAME_NATIVE_OK", output)
            self.assertNotIn("ERROR", output)
            for line in output.splitlines():
                if "WARN" in line:
                    self.assertIn("does not support setting window masks", line)

            def pixels(name):
                with Image.open(target / (name + ".png")) as image:
                    return image.convert("RGB")

            closed = pixels("closed")
            self.assertEqual(closed.size, (600, 400))
            self.assertEqual(closed.getpixel((5, 200)), (25, 26, 32))
            self.assertEqual(closed.getpixel((300, 20)), (25, 26, 32))
            self.assertEqual(closed.getpixel((300, 200)), (86, 125, 154))
            self.assertEqual(closed.getpixel((10, 50)), (25, 26, 32))
            joined = pixels("joined")
            self.assertEqual(joined.getpixel((300, 100)), (25, 26, 32))
            self.assertEqual(joined.getpixel((300, 320)), (25, 26, 32))
            self.assertEqual(joined.getpixel((300, 200)), (86, 125, 154))
            recoloured = pixels("recoloured")
            self.assertEqual(recoloured.getpixel((300, 100)), (12, 46, 34))
            self.assertEqual(recoloured.getpixel((5, 200)), (12, 46, 34))
            removed = pixels("no-edges")
            self.assertEqual(removed.getpixel((5, 200)), (86, 125, 154))
            self.assertEqual(removed.getpixel((300, 20)), (86, 125, 154))

            notice = pixels("notice-joined")
            self.assertEqual(notice.getpixel((500, 60)), (12, 46, 34))
            self.assertEqual(notice.getpixel((595, 80)), (12, 46, 34))
            self.assertEqual(notice.getpixel((380, 100)), (86, 125, 154))
            gallery = pixels("gallery-clear")
            self.assertEqual(gallery.getpixel((500, 60)), (86, 125, 154))
            self.assertEqual(gallery.getpixel((300, 200)), (86, 125, 154))

            quiet, rim = pixels("quiet"), pixels("rim-enabled")
            self.assertNotEqual(quiet.getpixel((100, 50)), rim.getpixel((100, 50)))
            self.assertEqual(quiet.getpixel((5, 200)), rim.getpixel((5, 200)))
            self.assertEqual(pixels("light").getpixel((5, 200)), (245, 242, 237))

            # Along-length enamel uses different source accents, while the
            # handle's middle stays flat and its shoulders return to the frame.
            top_left = closed.getpixel((250, 54))
            top_right = closed.getpixel((350, 54))
            self.assertGreater(top_left[2], top_left[0] + 30)
            self.assertGreater(top_right[0], top_left[0] + 25)
            self.assertEqual(closed.getpixel((218, 54)), (86, 125, 154))
            self.assertGreater(closed.getpixel((14, 200))[2], 100)
            self.assertGreater(closed.getpixel((585, 200))[2], 100)
            self.assertEqual(closed.getpixel((15, 100)), (86, 125, 154))
