"""Native engine transport, pin enforcement and provisioning boundaries."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

from nacre_shell import ENGINE_ID
from nacre_shell._backend import binary, call


class NativeEngineTests(unittest.TestCase):
    def test_real_binary_identity_and_errors(self):
        self.assertEqual(call("info")["engine"], ENGINE_ID)
        with self.assertRaises(ValueError):
            call("clean", value="bad")
        with self.assertRaises(ValueError):
            call("render_values", template="{{missing}}", values={})

    def test_rejects_wrong_engine_and_interface(self):
        for reply in (
            {"interfaceVersion": 1, "engine": "older-engine", "result": {}},
            {"interfaceVersion": 2, "engine": ENGINE_ID, "result": {}},
        ):
            with (
                self.subTest(reply=reply),
                patch("nacre_shell._backend.subprocess.Popen") as spawn,
            ):
                spawn.return_value.communicate.return_value = (json.dumps(reply), "")
                with self.assertRaisesRegex(OSError, "tested pin"):
                    call("info")

    def test_explicit_binary_and_cancellation_cleanup(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "orient"
            path.touch()
            with patch.dict(os.environ, {"NACRE_ORIENT_BINARY": str(path)}):
                self.assertEqual(binary(), path)
        with (
            patch("nacre_shell._backend.subprocess.Popen") as spawn,
            patch("nacre_shell._backend.os.killpg") as kill,
        ):
            process = spawn.return_value
            process.communicate.side_effect = subprocess.TimeoutExpired("orient", 120)
            process.poll.return_value = None
            with self.assertRaises(subprocess.TimeoutExpired):
                call("info")
            kill.assert_called_once()
            process.wait.assert_called_once_with(timeout=3)
