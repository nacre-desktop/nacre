"""Renderer allocator defaults and caller precedence through the actual launcher."""

import os
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]


class AllocatorEnvironmentTests(unittest.TestCase):
    def test_defaults_and_existing_options_keep_precedence(self):
        if not Path("/usr/bin/quickshell").exists():
            self.skipTest("System Quickshell unavailable")
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)
            script = target / "launch.sh"
            script.write_text(
                (ROOT / "launch.sh")
                .read_text()
                .replace(
                    'exec /usr/bin/quickshell "$@"',
                    'printf "%s" "$MALLOC_CONF"',
                )
            )
            (target / "qt-check.sh").write_text("qt_match_check() { return 0; }\n")
            environment = dict(os.environ, HOME=str(target))
            environment.pop("MALLOC_CONF", None)
            for override in (
                "",
                "narenas:4,dirty_decay_ms:5000,background_thread:false",
            ):
                requested = dict(environment)
                if override:
                    requested["MALLOC_CONF"] = override
                result = subprocess.run(
                    ["bash", str(script)],
                    env=requested,
                    text=True,
                    capture_output=True,
                    check=True,
                    timeout=5,
                )
                options = dict(item.split(":", 1) for item in result.stdout.split(","))
                self.assertEqual(options["muzzy_decay_ms"], "2000")
                self.assertEqual(
                    options["background_thread"], "false" if override else "true"
                )
                self.assertEqual(
                    options["dirty_decay_ms"], "5000" if override else "2000"
                )
                if override:
                    self.assertEqual(options["narenas"], "4")
                self.assertNotIn("MALLOC_ARENA_MAX", requested)
