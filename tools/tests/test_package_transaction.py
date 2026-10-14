"""Real PTY provider selection leaves transaction approval in the user's hands."""

import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import package_transaction as owner


class TransactionTests(unittest.TestCase):
    def test_fragmented_provider_prompt_prefers_ffmpeg_and_preserves_other_prompts(
        self,
    ):
        parser = owner.Providers()
        self.assertIsNone(
            parser.feed(
                b":: There are 4 providers available for qt6-multimedia-backend:\r\n1) qt6-multimedia-ffmpeg 2) qt6-multimedia-gstreamer\r\n"
            )
        )
        self.assertIsNone(parser.feed(b"\x1b[1mEnter a number (default="))
        self.assertEqual(parser.feed(b"1):\x1b[0m "), b"1\n")
        for message in (
            b"Proceed with installation? [Y/n] ",
            b"Replace a package? [Y/n] ",
            b"[sudo] password for user: ",
        ):
            self.assertIsNone(parser.feed(message))

    def test_real_terminal_exchange_selects_provider_but_never_approves_transaction(
        self,
    ):
        with tempfile.TemporaryDirectory() as temporary:
            folder = Path(temporary)
            sudo = folder / "sudo"
            sudo.write_text("""#!/usr/bin/env python3
import os, sys
print(":: There are 4 providers available for libfixture:", flush=True)
print("1) fixture 2) fixture 3) fixture 4) fixture", flush=True)
print("Enter a number (default=1): ", end="", flush=True)
assert input() == "1"
print("Proceed with installation? [Y/n] ", end="", flush=True)
assert input() == os.environ.get("EXPECTED", "n"), "Incorrect transaction approval"
print("PROVIDER_ONLY_OK", flush=True)
""")
            sudo.chmod(0o755)
            environment = {
                **os.environ,
                "PATH": str(folder) + os.pathsep + os.environ["PATH"],
                "PYTHONPATH": str(Path(owner.__file__).parent),
            }
            result = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    "from package_transaction import main; main()",
                    "fixture",
                ],
                env=environment,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                timeout=10,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("PROVIDER_ONLY_OK", result.stdout)
            result = subprocess.run(
                [
                    sys.executable,
                    "-c",
                    "from package_transaction import main; main()",
                    "fixture",
                ],
                env={**environment, "EXPECTED": "y"},
                input="y\n",
                capture_output=True,
                timeout=10,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("PROVIDER_ONLY_OK", result.stdout)

    def test_invalid_targets_never_start_a_privileged_process(self):
        for target in ("--overwrite", "bad;command", "a/b"):
            with patch.object(owner, "run") as spawn:
                with self.assertRaises(ValueError):
                    owner.transaction([target])
                spawn.assert_not_called()
