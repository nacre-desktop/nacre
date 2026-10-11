"""Production notification cards and stack with service-owned fixture entries."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from qml_source import install_foundation_interaction

ROOT = Path(__file__).resolve().parents[1]
SHELL = ROOT.parent / "shell"


class NotificationUITests(unittest.TestCase):
    def test_cards_and_popup_stack(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            fixtures = target / "fixtures"
            shutil.copytree(ROOT / "tests/qml/fixtures", fixtures)
            install_foundation_interaction(fixtures, SHELL / "widgets")
            for name in ("NacreNotice", "NacreNotificationStack"):
                source = (SHELL / "modules/notifications" / (name + ".qml")).read_text()
                source = source.replace("import Quickshell", "")
                source = source.replace("import qs.widgets", 'import "fixtures"')
                source = source.replace("import qs.services", "")
                source = source.replace("import qs.config", "")
                source = source.replace(
                    "Quickshell.iconPath(root.modelData.appIcon, true)", '""'
                )
                (target / (name + ".qml")).write_text(source)
            for name in ("NacreSurface", "NacreText", "ActionButton", "FastScroll"):
                source = (SHELL / "widgets" / (name + ".qml")).read_text()
                (fixtures / (name + ".qml")).write_text(
                    source.replace("import qs.services", 'import "."')
                )
            (fixtures / "NacreIcon.qml").write_text("import QtQuick\nText {}")
            (fixtures / "NacreNotifications.qml").write_text(
                (SHELL / "config/NacreNotifications.qml").read_text()
            )
            (fixtures / "NacreNotifs.qml").write_text(
                'pragma Singleton\nimport QtQuick\nQtObject {property var popups:[];property string dismissed:"";function dismiss(entry){dismissed=entry.key}}'
            )
            with (fixtures / "qmldir").open("a") as output:
                output.write(
                    "\nNacreIcon 1.0 NacreIcon.qml\nsingleton NacreNotifications 1.0 NacreNotifications.qml\nsingleton NacreNotifs 1.0 NacreNotifs.qml\n"
                )
            shutil.copy2(
                ROOT / "tests/notification-qml/tst_notifications.qml",
                target / "tst_notifications.qml",
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
