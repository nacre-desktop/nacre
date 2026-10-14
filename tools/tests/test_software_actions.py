"""Actual system action CLI: private fixtures and a PTY, never host services."""

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
class SystemActionTests(unittest.TestCase):
    def test_privileged_action_needs_terminal_confirmation_and_keeps_cache_versions(
        self,
    ):
        with tempfile.TemporaryDirectory(prefix="nacre-health-") as name:
            root = Path(name)
            commands = root / "bin"
            commands.mkdir()
            log = root / "mutations.jsonl"
            source = (
                "#!/usr/bin/python3\nimport sys,os,json\n"
                "name=os.path.basename(sys.argv[0]);args=sys.argv[1:]\n"
                "if name=='systemctl' and 'show' in args:print('loaded')\n"
                "elif name=='fwupdmgr':print(json.dumps({'Devices':[]}))\n"
                "elif name=='pacman' and args==['-Qdtq']:print('fixture-orphan')\n"
                "else:\n"
                f" with open({str(log)!r},'a') as f:f.write(json.dumps([name,*args])+'\\n')\n"
            )
            for name in [
                "systemctl",
                "fwupdmgr",
                "pacman",
                "paccache",
                "pkexec",
                "systemd-run",
                "kitty",
            ]:
                path = commands / name
                path.write_text(source)
                path.chmod(0o755)
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
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "DISPLAY",
                "WAYLAND_DISPLAY",
            ]:
                env.pop(key, None)
            noninteractive = subprocess.run(
                [str(BINARY), "software-actions", "run", "cache-clean"],
                env=env,
                capture_output=True,
                timeout=5,
            )
            self.assertNotEqual(noninteractive.returncode, 0)
            self.assertFalse(log.exists())
            for answer in ["no", "yes"]:
                master, slave = pty.openpty()
                child = subprocess.Popen(
                    [str(BINARY), "software-actions", "run", "cache-clean"],
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
                    self.assertIn(b"Administrator permission", text)
                    self.assertFalse(log.exists())
                    os.write(master, (answer + "\n\n").encode())
                    self.assertEqual(child.wait(timeout=5), 0)
                    if answer == "no":
                        self.assertFalse(log.exists())
                finally:
                    if child.poll() is None:
                        child.terminate()
                        child.wait(timeout=3)
                    os.close(master)
            command = json.loads(log.read_text())
            if Path("/proc/self/uid_map").read_text().split() != [
                "0",
                "0",
                "4294967295",
            ]:
                self.assertEqual(command[0], "systemd-run")
                self.assertEqual(
                    command[-3:], ["software-actions", "run", "cache-clean"]
                )
            else:
                self.assertEqual(
                    command, ["pkexec", "paccache", "--remove", "--keep", "3"]
                )
