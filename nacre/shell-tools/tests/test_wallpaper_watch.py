import json
import os
from pathlib import Path
import select
import subprocess
import sys
import tempfile
import time
import unittest


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
                [
                    sys.executable,
                    str(Path(__file__).parents[1] / "wallpaper-watch.py"),
                    "--root",
                    str(root),
                    "--scheme",
                    str(scheme),
                ],
                stdout=subprocess.PIPE,
                text=True,
                env={**os.environ, "HOME": str(base)},
            )
            try:
                time.sleep(0.2)
                self.assertFalse(select.select([process.stdout], [], [], 0.2)[0])
                nested = root / "new collection"
                nested.mkdir()
                time.sleep(0.1)
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
