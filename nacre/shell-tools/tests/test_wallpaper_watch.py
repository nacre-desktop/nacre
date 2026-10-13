import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time
import unittest

from helper_cli import command


class WallpaperWatchTests(unittest.TestCase):
    def test_nested_file_change_is_debounced_and_unchanged_library_stays_idle(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            root = base / "pictures"
            root.mkdir()
            scheme = base / "state/scheme.json"
            scheme.parent.mkdir()
            scheme.write_text('{"flavour":"default"}')
            process = subprocess.Popen(
                command(
                    "wallpaper-watch", "--root", str(root), "--scheme", str(scheme)
                ),
                stdout=subprocess.PIPE,
                text=True,
                env={**os.environ, "HOME": str(base)},
            )
            try:
                # A file-change reply proves the watcher is subscribed. Startup
                # duration is unrelated to its readiness, especially under load.
                def ready(directory):
                    probe = directory / "readiness.jpg"
                    deadline = time.monotonic() + 10
                    attempt = 0
                    while time.monotonic() < deadline:
                        self.assertIsNone(
                            process.poll(), "watch helper exited during startup"
                        )
                        probe.write_text(str(attempt))
                        attempt += 1
                        if select.select([process.stdout], [], [], 1)[0]:
                            self.assertTrue(
                                json.loads(process.stdout.readline())["changed"]
                            )
                            return
                    self.fail("watch helper did not observe a readiness probe")

                ready(root)
                self.assertFalse(select.select([process.stdout], [], [], 0.8)[0])
                nested = root / "new collection"
                nested.mkdir()
                ready(nested)
                (nested / "scene.jpg").write_bytes(b"first")
                (nested / "scene.jpg").write_bytes(b"final")
                self.assertTrue(select.select([process.stdout], [], [], 3)[0])
                self.assertTrue(json.loads(process.stdout.readline())["changed"])
                self.assertFalse(select.select([process.stdout], [], [], 0.8)[0])
                scheme.write_text('{"flavour":"default","mode":"light"}')
                self.assertTrue(select.select([process.stdout], [], [], 3)[0])
                self.assertTrue(json.loads(process.stdout.readline())["changed"])
                scheme.write_text('{"flavour":"tonal"}')
                self.assertTrue(select.select([process.stdout], [], [], 3)[0])
            finally:
                process.terminate()
                process.wait(timeout=3)
                process.stdout.close()
