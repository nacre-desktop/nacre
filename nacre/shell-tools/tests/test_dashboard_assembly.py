from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n

"""Production dashboard assembly/navigation with explicitly pending page contracts."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest

from qml_fixtures import install_foundation_interaction

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class DashboardAssemblyTests(unittest.TestCase):
    def test_loading_navigation_clipping_and_teardown(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            fixtures = target / "fixtures"
            shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
            install_foundation_interaction(fixtures, SHELL / "widgets")
            (fixtures / "NacreIcon.qml").write_text("import QtQuick\nText {}")
            for name in ("NacreSurface", "NacreText"):
                shutil.copy2(
                    SHELL / "widgets" / (name + ".qml"), fixtures / (name + ".qml")
                )
            with (fixtures / "qmldir").open("a") as manifest:
                manifest.write("\nNacreIcon 1.0 NacreIcon.qml\n")
            for name in ("NacreDashboardPanel", "NacreDashboardNavigation"):
                stage_qml(
                    SHELL / "modules/dashboard" / (name + ".qml"),
                    target / (name + ".qml"),
                )
            stage_qml(
                SHELL / "widgets/NacreTextField.qml", fixtures / "NacreTextField.qml"
            )
            stage_qml(
                SHELL / "services/NacreDashboardRegistry.qml",
                fixtures / "NacreDashboardRegistry.qml",
            )
            (fixtures / "NacreDashboardState.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property var tabs:["today","weather","media","system","focus"];property var preferences:({focusMinutes:25});property var focusState:({});property real progress:0;property bool running:false;property string timerText:"25:00";property string message:"";function start(value){}function end(value){}function request(value){} }'
            )
            (fixtures / "DesktopExtras.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property var busy:({});property string captured:"";property string message:"";function request(action,data){}}'
            )
            with (fixtures / "qmldir").open("a") as manifest:
                manifest.write("\nsingleton DesktopExtras 1.0 DesktopExtras.qml\n")
            (fixtures / "NacrePanelState.qml").write_text(
                "pragma Singleton\nimport QtQuick\nQtObject {property bool hidden:false;function openSettings(value){}}"
            )
            with (fixtures / "qmldir").open("a") as manifest:
                manifest.write("\nsingleton NacrePanelState 1.0 NacrePanelState.qml\n")
            (fixtures / "ActionButton.qml").write_text(
                "import QtQuick\nRectangle {property string text;property string icon;property bool compact:false;property bool selected:false;property string accessibleLabel;signal clicked;implicitWidth:34;implicitHeight:34}"
            )
            with (fixtures / "qmldir").open("a") as manifest:
                manifest.write(
                    "\nsingleton NacreDashboardState 1.0 NacreDashboardState.qml\n"
                )
            for name in (
                "NacreTodayPage",
                "NacreWeatherPage",
                "NacreMediaPage",
                "NacreSystemPage",
                "NacreFocusPage",
                "NacreDashboardCustomize",
            ):
                fields = (
                    "required property bool shouldUpdate"
                    if name != "NacreDashboardCustomize"
                    else ""
                )
                if name in ("NacreTodayPage", "NacreMediaPage"):
                    fields += ";required property var visibilities"
                (target / (name + ".qml")).write_text(
                    f'import QtQuick\nItem {{implicitWidth:940;implicitHeight:550;property string pageName:"{name}";{fields}}}'
                )
            shutil.copy2(
                ROOT / "tests/dashboard-qml/tst_dashboard.qml",
                target / "tst_dashboard.qml",
            )
            install_i18n(fixtures)
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env={**os.environ, "QT_QPA_PLATFORM": "offscreen"},
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
