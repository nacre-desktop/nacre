from qml_fixtures import install_i18n, run_qml

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
    def test_hide_preserves_wallpaper_and_releases_space_and_keyboard(self):
        binary = shutil.which("quickshell") or shutil.which("qs")
        if not binary:
            self.skipTest("Quickshell unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            for directory in (
                "services",
                "config",
                "modules/background",
                "modules/drawers",
            ):
                (target / directory).mkdir(parents=True)
            for relative in (
                "modules/background/NacreBackground.qml",
                "modules/drawers/NacreReservedEdges.qml",
            ):
                shutil.copy2(SHELL / relative, target / relative)
            (target / "services/qmldir").write_text(
                "singleton Environment 1.0 Environment.qml\n"
                "singleton NacrePanelState 1.0 NacrePanelState.qml\n"
            )
            (target / "services/Environment.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var windows:[];property int scenesCreated:0}\n"
            )
            (target / "services/NacrePanelState.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property bool hidden:false}\n"
            )
            (target / "config/qmldir").write_text(
                "singleton NacreFrame 1.0 NacreFrame.qml\n"
            )
            (target / "config/NacreFrame.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property int headerHeight:50;property int left:10;property int right:10;property int bottom:10}\n"
            )
            (target / "modules/background/NacreWallpaperScene.qml").write_text(
                'import QtQuick\nimport qs.services\nimport "../.."\nQtObject {property string screenName;property Anchors anchors: Anchors {};Component.onCompleted: Environment.scenesCreated++}\n'
            )
            (target / "Anchors.qml").write_text(
                "import QtQuick\nQtObject {property var fill;property bool top:false;property bool bottom:false;property bool left:false;property bool right:false}\n"
            )
            # Exercise production bindings without allocating compositor surfaces.
            (target / "NacreWindow.qml").write_text("""import QtQuick
import qs.services
QtObject {
 default property list<QtObject> children
 property string name
 property var screen
 property bool visible: true
 property int implicitWidth: 1
 property int implicitHeight: 1
 property int exclusiveZone: 0
 property int exclusionMode: 0
 property int layer: 0
 property int keyboardFocus: 0
 property QtObject mask
 property Anchors anchors: Anchors {}
 Component.onCompleted: Environment.windows = [...Environment.windows, this]
}
""")
            widgets = target / "widgets"
            widgets.mkdir()
            for name in ("NacreWindow.qml", "Anchors.qml"):
                shutil.copy2(target / name, widgets / name)
            shutil.copy2(ROOT / "tests/frame-qml/hide.qml", target / "shell.qml")
            environment = {
                **os.environ,
                "QT_QPA_PLATFORM": "offscreen",
                "QT_QUICK_BACKEND": "software",
                "XDG_CACHE_HOME": str(target / "cache"),
            }
            environment.pop("WAYLAND_DISPLAY", None)
            result = subprocess.run(
                [binary, "-p", str(target / "shell.qml")],
                cwd=target,
                env=environment,
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("HIDE_NATIVE_OK", result.stdout + result.stderr)

    def test_production_screen_releases_and_restores_keyboard_focus(self):
        from qml_fixtures import stage_qml

        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            providers = {
                "NacreCompositor": "property var capabilities:({});property bool overviewOpen:false",
                "NacrePanelDragRouting": 'property string regions:""',
                "NacrePanelState": "property bool hidden:false;property real reveal:1;property var screens:({});property var panels:({})",
                "NacreHoverIntent": "function fullscreenFor(name){return false}",
                "NacreDashboardState": "property real progress:0",
                "NacreTokens": "property bool motionEnabled:false",
                "DesktopSettings": "property var data:({topEdge:true})",
                "NacreFrame": "property int headerHeight:50",
            }
            children = {
                "NacrePanelMask": "QtObject {property var controller}",
                "NacreReservedEdges": "QtObject {property var screen}",
                "NacreChrome": "Item {property var host;property var lips;property bool gallery:false;property var silhouette:({rim:{}})}",
                "NacreFrameRim": "Item {property var contour;property bool engaged:false}",
                "NacrePanelInput": 'Item {objectName:"testInput";property var screen;property var panels;property var lips;property var visibilities;property bool modal:false;function dismiss(){}}',
                "NacrePanelHost": "Item {property var screen;property var visibilities;property var input;property var popouts:({hasCurrent:false})}",
                "NacreHeader": "Item {property var screen}",
                "NacreFrameLips": "Item {property var screen;property var visibilities;property var controller;property real focusProgress:0}",
            }
            with (target / "qmldir").open("w") as manifest:
                for name, body in providers.items():
                    (target / (name + ".qml")).write_text(
                        "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                    )
                    manifest.write(f"singleton {name} 1.0 {name}.qml\n")
                for name, body in children.items():
                    (target / (name + ".qml")).write_text("import QtQuick\n" + body)
                    manifest.write(f"{name} 1.0 {name}.qml\n")
            stage_qml(
                SHELL / "modules/drawers/NacreScreen.qml", target / "NacreScreen.qml"
            )
            (target / "tst_focus.qml").write_text("""import QtQuick
import QtTest
import Quickshell.Wayland
import "."
TestCase {
 name:"ProductionScreenFocus";when:windowShown
 Component {id:surface;NacreScreen{outputName:"test"}}
 function test_focus_tracks_hidden_and_panel_state(){
  const window=createTemporaryObject(surface,this);
  const input=findChild(window,"testInput"), flags=input.visibilities;
  compare(window.WlrLayershell.keyboardFocus,WlrKeyboardFocus.None);
  flags.launcher=true;
  compare(window.WlrLayershell.keyboardFocus,WlrKeyboardFocus.Exclusive);
  NacrePanelState.hidden=true;
  compare(window.WlrLayershell.keyboardFocus,WlrKeyboardFocus.None);
  NacrePanelState.hidden=false;
  compare(window.WlrLayershell.keyboardFocus,WlrKeyboardFocus.Exclusive);
  flags.launcher=false;flags.left=true;
  compare(window.WlrLayershell.keyboardFocus,WlrKeyboardFocus.OnDemand);
  input.modal=true;
  compare(window.WlrLayershell.keyboardFocus,WlrKeyboardFocus.Exclusive);
 }
}
""")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={
                    **os.environ,
                    "QT_QPA_PLATFORM": "offscreen",
                    "QT_QUICK_BACKEND": "software",
                },
                capture_output=True,
                text=True,
                timeout=20,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

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
            result = run_qml(
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
            shutil.copy2(
                SHELL / "modules/topbar/NacreHeaderForwarder.qml",
                target / "NacreHeaderForwarder.qml",
            )
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
            result = run_qml(
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
                SHELL / "widgets/size-scale.json", target / "widgets/size-scale.json"
            )
            shutil.copy2(
                SHELL / "widgets/NacreTokens.qml", target / "widgets/NacreTokens.qml"
            )
            shutil.copy2(
                SHELL / "widgets/NacreFrameLip.qml",
                target / "widgets/NacreFrameLip.qml",
            )
            for name in ("NacreHoverDwell", "NacreEdgeTab", "NacreIcon", "NacreText"):
                shutil.copy2(
                    SHELL / "widgets" / (name + ".qml"),
                    target / "widgets" / (name + ".qml"),
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
                "services/NacrePearls": "property var drag:null",
                "services/NacrePanelState": "property bool hidden:false",
                "services/NacreCompositor": "property var focusedMonitor:({name:'test'});property var activeWindow:null",
                "config/NacreFrame": "property int left:10;property int right:10;property int bottom:10;property int headerHeight:50;property int rounding:20",
            }
            for name, body in helpers.items():
                (target / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}\n"
                )
            install_i18n(target / "services")
            (target / "services/qmldir").unlink()
            shutil.copy2(
                SHELL / "services/NacreHoverIntent.qml",
                target / "services/NacreHoverIntent.qml",
            )
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

            self.assertNotEqual(closed.getpixel((26, 200)), (86, 125, 154))
            self.assertNotEqual(closed.getpixel((574, 200)), (86, 125, 154))

            # Top targets stay flat; only the two side tabs protrude into the wallpaper.
            for point in (
                (250, 54),
                (350, 54),
                (218, 54),
                (32, 200),
                (567, 200),
                (15, 100),
            ):
                self.assertEqual(closed.getpixel(point), (86, 125, 154))
