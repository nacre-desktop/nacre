from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n
import os
import shutil
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class ClickAwayTests(unittest.TestCase):
    def test_outside_click_geometry(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder)
            shutil.copytree(ROOT / "tests/qml/fixtures", path / "fixtures")
            stage_qml(
                ROOT.parent / "shell/modules/drawers/NacrePanelInput.qml",
                path / "NacrePanelInput.qml",
            )
            stage_qml(
                ROOT.parent / "shell/services/NacreHoverIntent.qml",
                path / "fixtures/NacreHoverIntent.qml",
            )
            (path / "fixtures/NacreCompositor.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject { property var focusedMonitor:({name:"test"}); property var activeWindow:null }'
            )
            (path / "fixtures/DesktopSettings.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property var data:({leftDrawer:true})}"
            )
            (path / "fixtures/NacrePanelState.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property bool hidden:false}"
            )
            (path / "fixtures/NacreFrame.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property int rounding:20;property int left:10;property int right:10;property int headerHeight:50}"
            )
            with (path / "fixtures/qmldir").open("a") as f:
                f.write(
                    "\nsingleton DesktopSettings 1.0 DesktopSettings.qml\nsingleton NacreFrame 1.0 NacreFrame.qml\nsingleton NacrePanelState 1.0 NacrePanelState.qml\nsingleton NacreHoverIntent 1.0 NacreHoverIntent.qml\nsingleton NacreCompositor 1.0 NacreCompositor.qml\n"
                )
            shutil.copy2(
                ROOT / "tests/click-away-qml/tst_click_away.qml",
                path / "tst_click_away.qml",
            )
            install_i18n(path / "fixtures")
            result = run_qml(
                [str(runner), "-input", str(path), "-o", "-,txt"],
                capture_output=True,
                text=True,
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
