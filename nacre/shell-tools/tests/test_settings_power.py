"""Native policy checks use injected power sources; no host power operations."""

import os
from pathlib import Path
import shutil
import tempfile
import unittest
from qml_fixtures import run_qml, stage_qml

ROOT = Path(__file__).resolve().parents[3]


class SettingsPowerTests(unittest.TestCase):
    def test_ac_battery_opt_in_and_hardware_limits(self):
        runner = Path("/usr/lib/qt6/bin/qmltestrunner")
        if not runner.exists():
            self.skipTest("Qt Quick Test unavailable")
        with tempfile.TemporaryDirectory() as folder:
            target = Path(folder)
            fixtures = target / "fixtures"
            fixtures.mkdir()
            stage_qml(
                ROOT / "nacre/shell/services/NacrePowerPolicy.qml",
                fixtures / "NacrePowerPolicy.qml",
            )
            for name, body in {
                "NacreSettingsStorage": "property var documents:({});property var present:({})",
                "NacreControlTools": "property var data:({})",
                "NacreI18n": "function tr(id,fallback){return fallback}",
            }.items():
                (fixtures / (name + ".qml")).write_text(
                    "pragma Singleton\nimport QtQuick\nQtObject {" + body + "}"
                )
                with (fixtures / "qmldir").open("a") as manifest:
                    manifest.write("\nsingleton " + name + " 1.0 " + name + ".qml\n")
            shutil.copy2(
                ROOT / "nacre/shell-tools/tests/power-policy-qml/tst_power.qml",
                target / "tst_power.qml",
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
