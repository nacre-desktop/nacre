"""Opt-in speed actions use fake tools and clean only owned temporary data."""

import json
import os
from pathlib import Path
import pty
import select
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/debug/nacre-helper"))


@unittest.skipUnless(BINARY.is_file(), "Candidate native helper required")
class SpeedTests(unittest.TestCase):
    def test_disk_action_is_opt_in_and_removes_only_owned_file(self):
        with tempfile.TemporaryDirectory(prefix="nacre-speed-") as name:
            root = Path(name)
            commands = root / "bin"
            commands.mkdir()
            log = root / "fio.json"
            for command in [
                "kitty",
                "systemctl",
                "fwupdmgr",
                "busctl",
                "nmcli",
                "fido2-token",
            ]:
                path = commands / command
                path.write_text(
                    "#!/usr/bin/python3\nimport json\nprint(json.dumps({'Devices':[],'data':[[]]}))\n"
                )
                path.chmod(0o755)
            fio = commands / "fio"
            fio.write_text(
                "#!/usr/bin/python3\nimport sys,json,pathlib\n"
                + f"pathlib.Path({str(log)!r}).write_text(json.dumps(sys.argv[1:]))\n"
                + "target=next(a.split('=',1)[1] for a in sys.argv[1:] if a.startswith('--filename='))\npathlib.Path(target).write_text('owned fixture')\n"
            )
            fio.chmod(0o755)
            env = {**os.environ, "HOME": str(root), "PATH": str(commands) + ":/usr/bin"}
            for key, suffix in [
                ("XDG_CONFIG_HOME", "config"),
                ("XDG_STATE_HOME", "state"),
                ("XDG_DATA_HOME", "data"),
                ("XDG_CACHE_HOME", "cache"),
                ("XDG_RUNTIME_DIR", "runtime"),
            ]:
                env[key] = str(root / suffix)
            env.update(
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(root / "absent"),
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + str(root / "absent"),
            )
            for key in [
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
            ]:
                env.pop(key, None)
            unrelated = root / "cache/nacre/other.txt"
            unrelated.parent.mkdir(parents=True)
            unrelated.write_text("preserve")
            master, slave = pty.openpty()
            child = subprocess.Popen(
                [str(BINARY), "software-actions", "run", "disk-speed"],
                env=env,
                stdin=slave,
                stdout=slave,
                stderr=slave,
            )
            os.close(slave)
            text = b""
            try:
                deadline = time.monotonic() + 10
                while (
                    b"Type yes to run these commands:" not in text
                    and time.monotonic() < deadline
                ):
                    if select.select([master], [], [], 0.2)[0]:
                        text += os.read(master, 8192)
                self.assertIn(b"Type yes to run these commands:", text)
                self.assertFalse(log.exists())
                os.write(master, b"yes\n\n")
                self.assertEqual(child.wait(timeout=5), 0)
                args = json.loads(log.read_text())
                filename = next(
                    a.split("=", 1)[1] for a in args if a.startswith("--filename=")
                )
                self.assertTrue(Path(filename).is_relative_to(root / "cache/nacre"))
                self.assertFalse(Path(filename).exists())
                self.assertFalse(Path(filename).parent.exists())
                self.assertIn("--size=256m", args)
                self.assertIn("--runtime=10", args)
                self.assertEqual(unrelated.read_text(), "preserve")
            finally:
                if child.poll() is None:
                    child.terminate()
                    child.wait(timeout=3)
                os.close(master)
            result = subprocess.run(
                [str(BINARY), "software-actions", "plan", "download-speed"],
                env=env,
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("bytes=10000000", result.stdout)
            self.assertFalse(
                any(Path(filename).parent.parent.glob("tmp*/nacre-speed.bin"))
            )
