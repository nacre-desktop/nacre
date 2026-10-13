"""Native engine transport, pin enforcement and provisioning boundaries."""

import os
from pathlib import Path
import subprocess
import signal
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

from nacre_shell import ENGINE_ID, _backend
from nacre_shell._backend import binary, call


class NativeEngineTests(unittest.TestCase):
    def setUp(self):
        _backend._close()
        self.addCleanup(_backend._close)

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
                patch(
                    "nacre_shell._backend._exchange",
                    side_effect=lambda *a, reply=reply: {
                        **reply,
                        "id": _backend._next_id,
                    },
                ),
            ):
                with self.assertRaisesRegex(OSError, "tested pin"):
                    call("info")
                self.assertIsNone(_backend._process)

    def test_explicit_binary_and_cancellation_cleanup(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "orient"
            path.touch()
            with patch.dict(os.environ, {"NACRE_ORIENT_BINARY": str(path)}):
                self.assertEqual(binary(), path)
        process = subprocess.Popen(
            [str(binary()), "--rpc", "--serve"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
        )
        with (
            patch("nacre_shell._backend.subprocess.Popen", return_value=process),
            patch(
                "nacre_shell._backend._exchange",
                side_effect=subprocess.TimeoutExpired("orient", 120),
            ),
            patch.object(process, "send_signal", wraps=process.send_signal) as send,
            patch.object(process, "wait", wraps=process.wait) as wait,
        ):
            with self.assertRaises(subprocess.TimeoutExpired):
                call("info")
            send.assert_called_once_with(signal.SIGINT)
            wait.assert_called_once_with(timeout=3)
        self.assertIsNone(_backend._process)

    def test_reuses_child_and_restarts_after_crash(self):
        call("info")
        first = _backend._process
        call("lch", value="aabbcc")
        self.assertIs(_backend._process, first)
        first.kill()
        first.wait()
        self.assertEqual(call("info")["engine"], ENGINE_ID)
        self.assertNotEqual(_backend._process.pid, first.pid)

    def test_concurrent_requests_keep_their_results(self):
        from concurrent.futures import ThreadPoolExecutor

        values = ["ff0000", "00ff00", "0000ff"] * 12
        with ThreadPoolExecutor(max_workers=4) as workers:
            actual = list(workers.map(lambda value: call("rgb", value=value), values))
        self.assertEqual(
            actual, [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]] * 12
        )

    def test_wrong_request_id_discards_child(self):
        with patch(
            "nacre_shell._backend._exchange",
            return_value={"id": -1, "interfaceVersion": 1, "result": {}},
        ):
            with self.assertRaisesRegex(OSError, "invalid reply"):
                call("info")
        self.assertIsNone(_backend._process)
        self.assertEqual(call("info")["engine"], ENGINE_ID)

    def test_large_requests_and_replies_keep_framing(self):
        template = "x" * (256 * 1024) + "\nλ{{primary}}"
        result = call("render_values", template=template, values={"primary": "abcdef"})
        self.assertEqual(result, "x" * (256 * 1024) + "\nλabcdef")
        self.assertEqual(call("info")["engine"], ENGINE_ID)

    def test_deadline_reaps_unresponsive_child(self):
        with tempfile.TemporaryDirectory() as folder:
            executable = Path(folder) / "orient"
            executable.write_text(
                "#!" + sys.executable + "\nimport time\ntime.sleep(30)\n"
            )
            executable.chmod(0o700)
            with (
                patch.dict(os.environ, {"NACRE_ORIENT_BINARY": str(executable)}),
                patch.object(_backend, "TIMEOUT", 0.05),
            ):
                with self.assertRaises(subprocess.TimeoutExpired):
                    call("info")
                self.assertIsNone(_backend._process)
        self.assertEqual(call("info")["engine"], ENGINE_ID)

    @unittest.skipUnless(hasattr(os, "fork"), "POSIX fork")
    def test_fork_does_not_share_or_kill_parent_child(self):
        call("info")
        parent = _backend._process
        pid = os.fork()
        if pid == 0:
            try:
                call("info")
                different = _backend._process.pid != parent.pid
                _backend._close()
                os._exit(0 if different else 1)
            except Exception:  # noqa: BLE001 - forked child must never resume the test runner
                os._exit(2)
        _, status = os.waitpid(pid, 0)
        self.assertEqual(os.waitstatus_to_exitcode(status), 0)
        self.assertIsNone(parent.poll())
        self.assertEqual(call("info")["engine"], ENGINE_ID)

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
