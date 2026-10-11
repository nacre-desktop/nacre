"""Palette event updates must touch only a registered image in its original PTY."""

import importlib.util
import json
import os
from pathlib import Path
import pty
import select
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location(
    "terminal_branding", ROOT / "terminal-branding.py"
)
live = importlib.util.module_from_spec(spec)
spec.loader.exec_module(live)


class LiveTerminalTests(unittest.TestCase):
    def test_update_edits_existing_frame_without_creating_placement_or_moving_cursor(
        self,
    ):
        master, slave = pty.openpty()
        try:
            tty = os.ttyname(slave)
            info = os.fstat(slave)
            with tempfile.TemporaryDirectory() as folder:
                home = Path(folder)
                cache = home / ".cache/nacre/terminal-logos"
                cache.mkdir(parents=True)
                target = cache / "test.json"
                target.write_text(
                    json.dumps(
                        dict(
                            pid=123,
                            started="41",
                            tty=tty,
                            inode=info.st_ino,
                            device=info.st_rdev,
                            image=234,
                        )
                    )
                )
                with patch.object(
                    live, "identity", return_value=("1", str(info.st_rdev), "41")
                ):
                    live.refresh(home)
                self.assertTrue(select.select([master], [], [], 1)[0])
                data = os.read(master, 4096)
                self.assertIn(b"a=f,r=1,X=1", data)
                self.assertIn(b"i=234,q=2", data)
                self.assertNotIn(b"a=T", data)
                self.assertNotIn(b"a=p", data)
                self.assertNotIn(b"\x1b[", data)
                with patch.object(
                    live, "identity", return_value=("1", str(info.st_rdev), "42")
                ):
                    live.refresh(home)
                self.assertFalse(target.exists())
                self.assertFalse(select.select([master], [], [], 0.05)[0])
        finally:
            os.close(master)
            os.close(slave)

    def test_arbitrary_output_paths_are_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            cache = home / ".cache/nacre/terminal-logos"
            cache.mkdir(parents=True)
            target = cache / "test.json"
            target.write_text(json.dumps(dict(tty="/tmp/anything")))
            with patch.object(live.os, "open") as opened:
                live.refresh(home)
            opened.assert_not_called()
            self.assertFalse(target.exists())
