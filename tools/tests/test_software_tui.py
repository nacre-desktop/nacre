"""Actual terminal app CLI/state/desktop entries; no desktop commands executed."""

import json
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/debug/nacre-helper"))


@unittest.skipUnless(BINARY.is_file(), "Candidate native helper required")
class TerminalApplicationTests(unittest.TestCase):
    def test_create_does_not_run_launch_keeps_literal_arguments_and_remove_is_scoped(
        self,
    ):
        with tempfile.TemporaryDirectory(prefix="nacre-tui-") as name:
            root = Path(name)
            home = root / "home"
            home.mkdir()
            commands = root / "bin"
            commands.mkdir()
            log = root / "launched.json"
            terminal = commands / "kitty"
            terminal.write_text(
                "#!/usr/bin/python3\nimport json,sys\n"
                f"open({str(log)!r},'w').write(json.dumps(sys.argv[1:]))\n"
            )
            terminal.chmod(0o755)
            env = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(root / "config"),
                "XDG_DATA_HOME": str(root / "data"),
                "XDG_STATE_HOME": str(root / "state"),
                "XDG_CACHE_HOME": str(root / "cache"),
                "XDG_RUNTIME_DIR": str(root / "runtime"),
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(root / "absent-bus"),
                "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(root / "absent-bus"),
                "PATH": str(commands) + os.pathsep + "/usr/bin",
            }

            def cli(action, ident="", payload=None):
                result = subprocess.run(
                    [str(BINARY), "software-tui", action, ident],
                    input=json.dumps(payload) if payload is not None else None,
                    env=env,
                    capture_output=True,
                    text=True,
                    timeout=5,
                )
                return result, json.loads(result.stdout)

            result, value = cli(
                "create",
                payload={
                    "name": "My monitor",
                    "icon": "utilities-terminal",
                    "command": ["printf", "literal space; $(not run)"],
                    "terminal": "kitty",
                },
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            ident = value["id"]
            self.assertFalse(log.exists())
            app = home / ".local/share/applications" / f"nacre-tui-{ident}.desktop"
            self.assertIn(f"X-Nacre-Tui-ID={ident}", app.read_text())
            store = root / "state/nacre/menu/tui-apps.json"
            self.assertEqual(json.loads(store.read_text())["version"], 1)
            self.assertEqual(store.stat().st_mode & 0o777, 0o600)
            result, value = cli("load")
            self.assertTrue(value["rows"][0]["available"])
            self.assertFalse(log.exists())
            result, value = cli("launch", ident)
            self.assertEqual(result.returncode, 0, result.stderr)
            for _ in range(50):
                if log.exists():
                    break
                time.sleep(0.02)
            args = json.loads(log.read_text())
            self.assertEqual(args[-2:], ["printf", "literal space; $(not run)"])
            self.assertIn(str(home), args)
            other = app.with_name("other.desktop")
            other.write_text("unrelated\n")
            result, value = cli("remove", ident)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertFalse(app.exists())
            self.assertEqual(other.read_text(), "unrelated\n")
            self.assertEqual(json.loads(store.read_text())["items"], [])

    def test_invalid_requests_and_future_state_are_preserved(self):
        with tempfile.TemporaryDirectory(prefix="nacre-tui-invalid-") as name:
            root = Path(name)
            env = {
                **os.environ,
                "HOME": str(root),
                "XDG_CONFIG_HOME": str(root / "config"),
                "XDG_DATA_HOME": str(root / "data"),
                "XDG_STATE_HOME": str(root / "state"),
                "XDG_CACHE_HOME": str(root / "cache"),
                "XDG_RUNTIME_DIR": str(root / "runtime"),
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(root / "absent-bus"),
                "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(root / "absent-bus"),
            }
            store = root / "state/nacre/menu/tui-apps.json"
            store.parent.mkdir(parents=True)
            future = b'{"version":99,"items":[]}'
            store.write_bytes(future)
            result = subprocess.run(
                [str(BINARY), "software-tui", "create"],
                input=json.dumps(
                    {
                        "name": "Test",
                        "command": ["true"],
                        "terminal": "kitty",
                        "icon": "terminal",
                    }
                ),
                env=env,
                capture_output=True,
                text=True,
                timeout=5,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(store.read_bytes(), future)
            self.assertFalse((root / ".local/share/applications").exists())
