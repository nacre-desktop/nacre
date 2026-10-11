"""Deterministic rain geometry and reversible terminal behavior in an owned PTY."""

import fcntl
import importlib.util
import os
from pathlib import Path
import pty
import random
import re
import select
import signal
import struct
import subprocess
import sys
import tempfile
import termios
import time
import unittest

SCRIPT = Path(__file__).parents[2] / "hypr/scripts/matrix-rest.py"
SPEC = importlib.util.spec_from_file_location("matrix_renderer", SCRIPT)
matrix = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(matrix)


class MatrixRestTests(unittest.TestCase):
    def test_frames_stay_in_bounds_and_resize_rebuilds_columns(self):
        rain = matrix.Rain(random.Random(17))
        colors = {
            role: (40, 80, 120)
            for role in ("background", "primary", "secondary", "highlight")
        }
        for width, height in ((120, 40), (20, 6), (1, 1)):
            rain.resize(width, height)
            self.assertEqual(
                len(rain.columns), (width + 1) // 2 if width >= 100 else width
            )
            for _ in range(20):
                output = rain.frame(colors)
                for row, column in re.findall(r"\x1b\[(\d+);(\d+)H", output):
                    self.assertTrue(1 <= int(row) <= height)
                    self.assertTrue(1 <= int(column) <= width)

    def test_key_and_signal_exit_restore_terminal_and_modes(self):
        for mode in ("key", "signal"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as directory:
                master, slave = pty.openpty()
                fcntl.ioctl(
                    slave, termios.TIOCSWINSZ, struct.pack("HHHH", 24, 80, 0, 0)
                )
                original = termios.tcgetattr(slave)
                process = subprocess.Popen(
                    [sys.executable, str(SCRIPT)],
                    stdin=slave,
                    stdout=slave,
                    stderr=subprocess.PIPE,
                    env={**os.environ, "HOME": directory},
                )
                output = b""
                try:
                    deadline = time.monotonic() + 3
                    while (
                        matrix.ENTER.encode() not in output
                        and time.monotonic() < deadline
                    ):
                        if select.select([master], [], [], 0.1)[0]:
                            output += os.read(master, 65536)
                    self.assertIn(matrix.ENTER.encode(), output)
                    time.sleep(0.4)
                    if mode == "key":
                        os.write(master, b"x")
                    else:
                        process.send_signal(signal.SIGTERM)
                    deadline = time.monotonic() + 3
                    while process.poll() is None and time.monotonic() < deadline:
                        if select.select([master], [], [], 0.1)[0]:
                            output += os.read(master, 65536)
                    process.wait(timeout=1)
                    while select.select([master], [], [], 0)[0]:
                        output += os.read(master, 65536)
                    self.assertEqual(
                        process.returncode, 0, process.stderr.read().decode()
                    )
                    self.assertIn(matrix.LEAVE.encode(), output)
                    self.assertEqual(termios.tcgetattr(slave), original)
                finally:
                    if process.poll() is None:
                        process.kill()
                        process.wait(timeout=2)
                    process.stderr.close()
                    os.close(master)
                    os.close(slave)
