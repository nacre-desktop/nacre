from qml_fixtures import run_qml
from qml_fixtures import stage_qml
from qml_fixtures import install_i18n
from qml_fixtures import install_foundation_interaction

"""Exercise the production composer with a fixture backend and Qt Quick controls."""

import os
import shutil
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
            stage_qml(
                ROOT.parent / "shell/modules/extras/ChatPane.qml",
                target / "ChatPane.qml",
            )
            shutil.copy2(
                ROOT.parent / "shell/widgets/FastScroll.qml",
                target / "fixtures/FastScroll.qml",
            )
            stage_qml(
                ROOT.parent / "shell/widgets/ActionButton.qml",
                target / "fixtures/ActionButton.qml",
            )
            (target / "fixtures/NacreIcon.qml").write_text("import QtQuick\nText {}")
            with (target / "fixtures/qmldir").open("a") as manifest:
                manifest.write("\nNacreIcon 1.0 NacreIcon.qml\n")
            stage_qml(
                ROOT.parent / "shell/widgets/NacreInteraction.qml",
                target / "fixtures/NacreInteraction.qml",
            )
            install_foundation_interaction(
                target / "fixtures", ROOT.parent / "shell/widgets"
            )
            install_i18n(target / "fixtures")
            result = run_qml(
                [str(runner), "-input", str(target), "-o", "-,txt"],
                env=dict(os.environ, QT_QPA_PLATFORM="offscreen"),
                capture_output=True,
                text=True,
                timeout=30,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertNotIn("QWARN", result.stdout + result.stderr)
