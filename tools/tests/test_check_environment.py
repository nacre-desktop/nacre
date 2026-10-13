"""Repository checks must not route fixtures into the active desktop."""

import importlib.util
import os
from pathlib import Path
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "repository_check", Path(__file__).parents[1] / "check.py"
)
check = importlib.util.module_from_spec(spec)
spec.loader.exec_module(check)


class CheckEnvironmentTests(unittest.TestCase):
    def test_session_isolation_preserves_candidate_binary_and_parent_environment(self):
        inherited = {
            "NACRE_COMPOSITOR": "niri",
            "NACRE_COMPOSITOR_TOOLS": "/deployed/tools",
            "NIRI_SOCKET": "/live/socket",
            "QT_LOGGING_RULES": "*.debug=false",
            "NACRE_ORIENT_BINARY": "/candidate/orient",
        }
        with patch.dict(os.environ, inherited):
            environment = check.test_environment()
            for key in inherited:
                if key != "NACRE_ORIENT_BINARY":
                    self.assertNotIn(key, environment)
                self.assertEqual(os.environ[key], inherited[key])
            self.assertEqual(environment["NACRE_ORIENT_BINARY"], "/candidate/orient")
            self.assertEqual(environment["QT_QPA_PLATFORM"], "offscreen")
            self.assertTrue(
                environment["PYTHONPATH"].startswith(
                    str(check.ROOT / "nacre/shell-cli/src")
                )
            )
