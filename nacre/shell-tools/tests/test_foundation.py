"""Actual Nacre primitives with native Qt input and text/layout tests."""

import json
import base64
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
WIDGETS = ROOT.parent / "shell/widgets"


class FoundationTests(unittest.TestCase):
    def test_actual_text_surface_and_interaction_components(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            widgets = target / "qs/widgets"
            services = target / "qs/services"
            widgets.mkdir(parents=True)
            services.mkdir(parents=True)
            names = (
                "NacreTokens",
                "NacreSurface",
                "NacreText",
                "NacreInteraction",
                "ActionButton",
                "NacreIcon",
                "NacreTextField",
                "NacreSlider",
                "NacreScrollBar",
                "NacreImage",
            )
            manifest = "module qs.widgets\n"
            for name in names:
                shutil.copy2(WIDGETS / (name + ".qml"), widgets / (name + ".qml"))
                manifest += (
                    ("singleton " if name == "NacreTokens" else "")
                    + name
                    + " 1.0 "
                    + name
                    + ".qml\n"
                )
            (widgets / "qmldir").write_text(manifest)
            palette = {
                "m3surface": "#202226",
                "m3surfaceContainer": "#303640",
                "m3surfaceContainerHigh": "#363a42",
                "m3frame": "#353840",
                "m3onSurface": "#fafafa",
                "m3onSurfaceVariant": "#aabbcc",
                "m3primary": "#7696ff",
                "m3onPrimary": "#101014",
                "m3outline": "#888888",
            }
            (services / "NacreColours.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property bool light:false;property var palette:"
                + json.dumps(palette)
                + "}\n"
            )
            (services / "DesktopSettings.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var data:({animations:true})}\n"
            )
            (services / "qmldir").write_text(
                "module qs.services\nsingleton NacreColours 1.0 NacreColours.qml\nsingleton DesktopSettings 1.0 DesktopSettings.qml\n"
            )
            config = target / "qs/config"
            config.mkdir()
            config_manifest = "module qs.config\n"
            for path in (WIDGETS.parent / "config").glob("Nacre*.qml"):
                shutil.copy2(path, config / path.name)
                config_manifest += f"singleton {path.stem} 1.0 {path.name}\n"
            (config / "qmldir").write_text(config_manifest)
            (target / "test.png").write_bytes(
                base64.b64decode(
                    "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGP4z8DwHwAFAAH/iZk9HQAAAABJRU5ErkJggg=="
                )
            )
            shutil.copy2(
                ROOT / "tests/foundation-qml/tst_foundation.qml",
                target / "tst_foundation.qml",
            )
            result = subprocess.run(
                [
                    str(runner),
                    "-import",
                    str(target),
                    "-input",
                    str(target / "tst_foundation.qml"),
                    "-o",
                    "-,txt",
                ],
                capture_output=True,
                text=True,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)

    def test_native_rounded_clipping_with_quickshell_graphics_renderer(self):
        binary = shutil.which("quickshell")
        if not binary:
            self.skipTest("Quickshell unavailable")
        try:
            from PIL import Image
        except ImportError:
            self.skipTest("Pillow unavailable for native capture assertions")
        wrapper = []
        if not os.environ.get("DISPLAY"):
            xvfb = shutil.which("xvfb-run")
            if not xvfb:
                self.skipTest("Native clipping requires an OpenGL display or Xvfb")
            wrapper = [xvfb, "-a"]
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            widgets = target / "widgets"
            services = target / "services"
            widgets.mkdir()
            services.mkdir()
            for name in (
                "NacreTokens",
                "NacreSurface",
                "NacreText",
                "NacreClip",
            ):
                shutil.copy2(WIDGETS / (name + ".qml"), widgets / (name + ".qml"))
            palette = {
                "m3surface": "#202226",
                "m3surfaceContainer": "#303640",
                "m3frame": "#353840",
                "m3onSurface": "#fafafa",
                "m3onSurfaceVariant": "#aabbcc",
                "m3primary": "#7696ff",
                "m3outline": "#888888",
            }
            (services / "NacreColours.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property bool light:false;property var palette:"
                + json.dumps(palette)
                + "}\n"
            )
            (services / "DesktopSettings.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var data:({animations:true})}\n"
            )
            shutil.copy2(ROOT / "tests/foundation-qml/render.qml", target / "shell.qml")
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
                capture_output=True,
                text=True,
                env=environment,
                timeout=20,
            )
            output = result.stdout + result.stderr
            self.assertEqual(result.returncode, 0, output)
            self.assertIn("FOUNDATION_CAPTURED", output)
            self.assertNotIn("ERROR", output)
            # Offscreen windows do not support platform window masks. This does
            # not affect the separately tested in-scene rounded clipping.
            for line in output.splitlines():
                if "WARN" in line:
                    self.assertIn("does not support setting window masks", line)
            with Image.open(target / "capture.png") as image:
                image = image.convert("RGB")
                self.assertEqual(image.size, (300, 180))
                self.assertEqual(image.getpixel((23, 23)), (240, 240, 244))
                self.assertEqual(image.getpixel((70, 70)), (38, 121, 205))
                self.assertEqual(image.getpixel((70, 25)), (38, 121, 205))
                self.assertEqual(image.getpixel((150, 21)), (240, 240, 244))
                self.assertEqual(image.getpixel((165, 35)), (48, 54, 64))
