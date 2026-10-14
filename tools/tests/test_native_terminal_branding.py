"""The installed native command registers only its private Bash/PTY session."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class NativeTerminalBrandingTests(unittest.TestCase):
    def binary(self):
        return Path(
            os.environ.get("NACRE_HELPER_BINARY") or ROOT / "target/debug/nacre-helper"
        ).resolve(strict=True)

    def test_real_private_pty_matches_frozen_registration_and_prepared_packet(self):
        fixture = json.loads(
            (
                ROOT
                / "crates/nacre-helper/tests/fixtures/terminal-branding-pty-reference.json"
            ).read_text()
        )
        for case in fixture["cases"]:
            with self.subTest(prepare=case["prepare"]):
                command = [
                    sys.executable,
                    str(ROOT / "tools/tests/terminal_pty_probe.py"),
                    str(self.binary()),
                ]
                if case["prepare"]:
                    command.append("--prepare")
                result = subprocess.run(
                    command, capture_output=True, text=True, check=True, timeout=10
                )
                self.assertEqual(result.stderr, "")
                observed = json.loads(result.stdout)
                record = observed["record"]
                self.assertEqual(record["pid"], observed["shell_pid"])
                self.assertIsInstance(record["image"], int)
                self.assertGreater(record["image"], 0)
                self.assertLess(record["image"], 2**32)
                for key in ("pid", "inode", "device", "started", "image"):
                    record[key] = "@" + key.upper() + "@"
                record["tty"] = "/dev/pts/@PTY@"
                self.assertEqual(record, case["record"])
                self.assertEqual(observed["record_mode"], case["record_mode"])
                self.assertEqual(observed["cache_mode"], 0o700)
                self.assertEqual(observed["stdout"], case["stdout"])
                if case["prepare"]:
                    self.assertEqual(observed["raw"], case["raw"])
                    self.assertEqual(observed["raw_mode"], case["raw_mode"])

    def test_plain_fallback_and_preparation_run_without_python_on_path(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            art = home / ".local/share/nacre/branding"
            art.mkdir(parents=True)
            (art / "nacre-text.txt").write_text("Public logo\n\n")
            for args in ([], ["--prepare"], ["--help"]):
                result = subprocess.run(
                    [str(self.binary()), "terminal-branding", *args],
                    env={"HOME": folder, "PATH": str(home / "absent")},
                    text=True,
                    capture_output=True,
                    check=True,
                )
                self.assertEqual(result.stdout, "Public logo\n")
                self.assertEqual(result.stderr, "")
