"""Native engine transport, pin enforcement and provisioning boundaries."""

import json
import os
from pathlib import Path
import subprocess
import signal
import sys
import tempfile
import time
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
        with patch("nacre_shell._backend.subprocess.Popen") as spawn:
            process = spawn.return_value
            process.communicate.side_effect = subprocess.TimeoutExpired("orient", 120)
            process.poll.return_value = None
            with self.assertRaises(subprocess.TimeoutExpired):
                call("info")
            process.send_signal.assert_called_once_with(signal.SIGINT)
            process.wait.assert_called_once_with(timeout=3)

    def test_job_group_cancellation_reaps_video_decoder(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            probe = root / "ffprobe"
            probe.write_text(
                '#!/bin/sh\nsleep 30 &\necho $! > "$NACRE_TEST_CHILD"\nwait\n'
            )
            probe.chmod(0o700)
            video = root / "test.mp4"
            video.touch()
            child = root / "child"
            process = subprocess.Popen(
                [
                    sys.executable,
                    "-c",
                    "import sys;from nacre_shell.extract import analyze;analyze(sys.argv[1])",
                    str(video),
                ],
                env={
                    **os.environ,
                    "PATH": str(root) + os.pathsep + os.environ["PATH"],
                    "NACRE_TEST_CHILD": str(child),
                },
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 5
                while not child.exists() and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertTrue(child.exists(), "decoder did not start")
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=3)
                status = Path("/proc") / child.read_text().strip() / "status"
                deadline = time.monotonic() + 3
                while time.monotonic() < deadline:
                    try:
                        state = status.read_text()
                    except (FileNotFoundError, ProcessLookupError):
                        break
                    if "State:\tZ" in state:
                        break
                    time.sleep(0.01)
                else:
                    self.fail("decoder descendant is still running")
            finally:
                if process.poll() is None:
                    os.killpg(process.pid, signal.SIGKILL)
                    process.wait()
                process.stdout.close()
                process.stderr.close()
