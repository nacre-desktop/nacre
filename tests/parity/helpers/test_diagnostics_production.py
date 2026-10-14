"""The real report router works with no PATH tools and no owner endpoints."""

import importlib.util
import os
from pathlib import Path
import shlex
import unittest

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location(
    "diagnostics_production_runner", HERE / "run.py"
)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class DiagnosticsProductionTests(unittest.TestCase):
    def test_unavailable_optional_tools_still_produce_a_private_report(self):
        binary = Path(
            os.environ.get("NACRE_DIAGNOSTICS_PRODUCTION")
            or runner.ROOT / "target/debug/examples/diagnostics-production"
        )
        self.assertTrue(
            binary.is_file(), "Build the developer diagnostics-production example"
        )
        case = {
            "helper": "diagnostics-production",
            "env": {
                "PATH": "/case/empty",
                "USER": "fixture-user",
                "LOGNAME": "fixture-user",
                "HOSTNAME": "fixture-host",
            },
            "fixtures": {},
        }
        result = runner.execute(case, shlex.join([str(binary)]))
        self.assertEqual(result["exit"], 0)
        report = result["stdout"]
        self.assertEqual(report["schemaVersion"], 1)
        self.assertEqual(
            report["software"]["versions"]["Python"],
            {"status": "unavailable", "value": None},
        )
        self.assertEqual(
            report["dependencies"]["packageManager"],
            "unavailable (command checks only)",
        )
        self.assertEqual(report["monitors"]["devices"], [])
        self.assertEqual(report["audio"]["devices"], [])
        self.assertEqual(result["effects"], [])
        self.assertEqual(result["ipc"], [])
        self.assertEqual(result["files"], {})
        self.assertEqual(result["deleted"], [])
        for section in [
            "software",
            "hardware",
            "monitors",
            "audio",
            "dependencies",
            "services",
            "maintenance",
            "recentErrors",
        ]:
            self.assertIn(section, report)
