"""Native CLI command boundaries and actual terminal interaction in private homes."""

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
class DevelopmentEnvironmentTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="nacre-development-")
        self.root = Path(self.temp.name)
        self.home = self.root / "home"
        self.home.mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        self.log = self.root / "commands.jsonl"
        self.env = {
            **os.environ,
            "HOME": str(self.home),
            "XDG_CONFIG_HOME": str(self.root / "config"),
            "XDG_DATA_HOME": str(self.root / "data"),
            "XDG_STATE_HOME": str(self.root / "state"),
            "XDG_CACHE_HOME": str(self.root / "cache"),
            "XDG_RUNTIME_DIR": str(self.root / "runtime"),
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(self.root / "absent-bus"),
            "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(self.root / "absent-bus"),
            "PATH": str(self.bin) + os.pathsep + "/usr/bin",
        }
        for key in list(self.env):
            if key.startswith("MISE_") or key in [
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
            ]:
                self.env.pop(key)
        source = (
            "#!/usr/bin/python3\nimport json,os,sys\n"
            "args=sys.argv[1:];name=os.path.basename(sys.argv[0])\n"
            "if name=='mise' and args[:1]==['ls']:\n"
            " print(json.dumps({'node':[{'installed':True,'version':'24'}]}))\n"
            "elif name=='docker' and args[:1]==['info']:print(json.dumps('fixture'))\n"
            "elif name=='docker' and args[:1]==['inspect']:\n"
            " labels={'io.nacre.development':os.getenv('FIXTURE_OWNER','postgres')}\n"
            " print(json.dumps({'id':'a'*64,'labels':labels} if args[2]!='{{json .Config.Labels}}' else labels))\n"
            "elif name=='systemd-ask-password':print('fixture-value')\n"
            "else:\n"
            f" with open({str(self.log)!r},'a') as f:f.write(json.dumps([name,*args])+'\\n')\n"
        )
        for name in ["mise", "docker", "kitty", "systemd-ask-password"]:
            path = self.bin / name
            path.write_text(source)
            path.chmod(0o755)

    def tearDown(self):
        self.temp.cleanup()

    def cli(self, *args):
        return subprocess.run(
            [str(BINARY), "software-environments", *args],
            env=self.env,
            capture_output=True,
            text=True,
            timeout=10,
        )

    def terminal(self, answer):
        master, slave = pty.openpty()
        process = subprocess.Popen(
            [str(BINARY), "software-environments", "run", "install", "node"],
            env=self.env,
            stdin=slave,
            stdout=slave,
            stderr=slave,
        )
        os.close(slave)
        transcript = b""
        try:
            deadline = time.monotonic() + 10
            while (
                b"Type yes to continue:" not in transcript
                and time.monotonic() < deadline
            ):
                if select.select([master], [], [], 0.2)[0]:
                    transcript += os.read(master, 8192)
            self.assertIn(b"Type yes to continue:", transcript)
            self.assertFalse(self.log.exists(), "No mutation before confirmation")
            os.write(master, (answer + "\n\n").encode())
            self.assertEqual(process.wait(timeout=5), 0)
            return transcript
        finally:
            if process.poll() is None:
                process.terminate()
                process.wait(timeout=3)
            os.close(master)

    def test_discovery_only_reads_and_reports_global_installed_status(self):
        data = json.loads(self.cli("load").stdout)
        rows = {row["id"]: row for row in data["profiles"]}
        self.assertEqual(len(rows), 18)
        self.assertTrue(rows["node"]["installed"])
        self.assertFalse(rows["ruby"]["installed"])
        self.assertFalse(self.log.exists())

    def test_plan_retains_project_versions_and_framework_runtimes(self):
        node = json.loads(self.cli("plan", "remove", "node").stdout)
        self.assertEqual(
            node["steps"][0]["arguments"], ["mise", "unuse", "--global", "node"]
        )
        rails = json.loads(self.cli("plan", "remove", "rails").stdout)
        self.assertNotIn("unuse", json.dumps(rails))
        self.assertFalse(self.log.exists())

    def test_noninteractive_run_and_unknown_target_never_mutate(self):
        self.assertNotEqual(self.cli("run", "install", "node").returncode, 0)
        self.assertNotEqual(self.cli("plan", "remove", "../../other").returncode, 0)
        self.assertFalse(self.log.exists())

    def test_terminal_cancel_does_not_execute(self):
        self.terminal("no")
        self.assertFalse(self.log.exists())

    def test_terminal_shows_command_and_executes_after_yes(self):
        output = self.terminal("yes")
        self.assertIn(b"mise", output)
        self.assertIn(b"node@lts", output)
        self.assertEqual(
            json.loads(self.log.read_text()), ["mise", "use", "--global", "node@lts"]
        )

    def test_remove_uses_inspected_container_identity(self):
        value = json.loads(self.cli("plan", "remove", "postgres").stdout)
        self.assertEqual(
            value["steps"][0]["arguments"], ["docker", "rm", "--force", "a" * 64]
        )
        self.assertTrue(value["dataRetained"])

    def test_foreign_database_container_is_preserved(self):
        self.env["FIXTURE_OWNER"] = "foreign"
        result = self.cli("plan", "remove", "postgres")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("preserved", result.stdout)
        self.assertFalse(self.log.exists())
