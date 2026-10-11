from qml_source import install_foundation_interaction

"""Exercise the production composer with a fixture backend and Qt Quick controls."""

import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


class ChatPaneUITests(unittest.TestCase):
    def test_scrolling_and_sending_during_generation(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test is unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            shutil.copytree(ROOT / "tests/qml/fixtures", target / "fixtures")
            shutil.copy2(
                ROOT / "tests/qml/tst_chatpane.qml", target / "tst_chatpane.qml"
            )
            source = (ROOT.parent / "shell/modules/extras/ChatPane.qml").read_text()
            source = (
                source.replace("import qs.widgets", 'import "fixtures"')
                .replace("import qs.services", "")
                .replace("import Quickshell", "")
                .replace(
                    "required property PersistentProperties visibilities",
                    "required property var visibilities",
                )
            )
            (target / "ChatPane.qml").write_text(source)
            shutil.copy2(
                ROOT.parent / "shell/widgets/FastScroll.qml",
                target / "fixtures/FastScroll.qml",
            )
            button = (
                (ROOT.parent / "shell/widgets/ActionButton.qml")
                .read_text()
                .replace("import qs.services", "")
            )
            (target / "fixtures/ActionButton.qml").write_text(button)
            (target / "fixtures/NacreIcon.qml").write_text("import QtQuick\nText {}")
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write("\nNacreIcon 1.0 NacreIcon.qml\n")
            layer = (
                (ROOT.parent / "shell/widgets/NacreInteraction.qml")
                .read_text()
                .replace("import qs.widgets", "")
                .replace("import qs.services", "")
                .replace("import qs.config", "")
            )
            (target / "fixtures/NacreInteraction.qml").write_text(layer)
            install_foundation_interaction(
                target / "fixtures", ROOT.parent / "shell/widgets"
            )
            result = subprocess.run(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
