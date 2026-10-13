"""Render the shared landing field on all four frame edges."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest

SHELL = Path(__file__).resolve().parents[2] / "shell"


class DockTargetTests(unittest.TestCase):
    def test_iridescent_landing_field_is_compact_on_all_edges(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "NacreDockTarget.qml").write_text(
                (SHELL / "widgets/NacreDockTarget.qml")
                .read_text()
                .replace("import qs.services", 'import "."')
            )
            (root / "NacreTokens.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property color accent:"#85b8c9";property color orientSecondary:"#d48dbc";property color orientHighlight:"#e9e5f7"}\n'
            )
            (root / "qmldir").write_text("singleton NacreTokens 1.0 NacreTokens.qml\n")
            (root / "tst_target.qml").write_text("""import QtQuick
import QtTest
TestCase {
 name:"DockLandingField";width:800;height:600;visible:true;when:windowShown
 Rectangle {id:scene;width:800;height:600;color:"#101419"
  NacreDockTarget{origin:Qt.point(400,70);edge:"top"}
  NacreDockTarget{origin:Qt.point(400,530);edge:"bottom"}
  NacreDockTarget{origin:Qt.point(70,300);edge:"left"}
  NacreDockTarget{origin:Qt.point(730,300);edge:"right"}
 }
 function test_render(){
  wait(40);const image=grabImage(scene);
  for(const point of [[400,70],[400,530],[70,300],[730,300]])verify(image.pixel(point[0],point[1]).g>.2);
  compare(image.pixel(400,100),image.pixel(400,300));
  compare(image.pixel(100,300),image.pixel(400,300));
  verify(image.pixel(330,70).g!==image.pixel(470,70).g);
  const folder=Qt.application.arguments.filter(arg=>arg.indexOf("-landing-output=")===0)[0];
  if(folder)image.save(folder.slice(16));
 }
}
""")
            args = [str(runner), "-input", str(root), "-o", "-,txt"]
            output = os.environ.get("NACRE_LANDING_RENDER")
            if output:
                source = (root / "tst_target.qml").read_text()
                start = source.index("  const folder=Qt.application.arguments")
                end = source.index("\n }", start)
                source = (
                    source[:start]
                    + "  image.save("
                    + json.dumps(output)
                    + ");"
                    + source[end:]
                )
                (root / "tst_target.qml").write_text(source)
            result = subprocess.run(
                args,
                capture_output=True,
                text=True,
                timeout=20,
                env={
                    **os.environ,
                    "QT_QPA_PLATFORM": "offscreen",
                    "QT_QUICK_BACKEND": "rhi",
                    "QSG_RHI_BACKEND": "opengl",
                },
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
