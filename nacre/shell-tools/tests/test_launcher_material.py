"""The launcher material remains fixed-size and follows its page as one layer."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class LauncherMaterialTests(unittest.TestCase):
    def test_geometry_and_shared_motion(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            shutil.copy2(
                SHELL / "modules/launcher/NacreLauncherMaterial.qml",
                target / "NacreLauncherMaterial.qml",
            )
            (target / "tst_material.qml").write_text(
                r"""import QtQuick
import QtTest
TestCase {
 name: "LauncherMaterial"
 when: windowShown
 Item {
  id: animated; objectName: "animated"; width: 240; height: 180
  NacreLauncherMaterial { id: material; objectName: "material"; width: 240; height: 180; cornerRadius: 24 }
  Item { id: search; objectName: "search"; width: 240; height: 44 }
 }
 function test_fixed_geometry_and_radius_clamp() {
  compare(animated.width, 240); compare(animated.height, 180);
  compare(animated.children[0].radius, 24);
  animated.children[0].cornerRadius = 100;
  compare(animated.children[0].radius, 60);
  animated.children[0].width = 40;
  compare(animated.children[0].radius, 10);
 }
 function test_material_and_search_share_every_transform_sample() {
  const material = findChild(this, "material"), search = findChild(this, "search"), animated = findChild(this, "animated");
  for (let frame = 0; frame <= 20; frame++) {
   animated.y = -100 + frame * 5;
   animated.opacity = frame / 20;
   const materialPoint = material.mapToItem(this, 0, 0);
   const searchPoint = search.mapToItem(this, 0, 0);
   compare(materialPoint.x, searchPoint.x);
   compare(materialPoint.y, searchPoint.y);
   compare(animated.opacity, frame / 20);
  }
 }
}"""
            )
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                capture_output=True,
                text=True,
                timeout=30,
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
