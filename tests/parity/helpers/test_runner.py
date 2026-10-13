"""Harness integrity, negative controls, framing and transport equivalence."""

import importlib.util
import json
import os
from pathlib import Path
import shlex
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location("parity_runner", HERE / "run.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)


class RunnerTests(unittest.TestCase):
    def test_scripted_interpreter_version_preserves_execution_and_executable(self):
        script = (
            "import json,subprocess,sys; "
            "print(json.dumps({'value':6*7,'executable':sys.executable,"
            "'version':subprocess.check_output([sys.executable,'--version']).decode().strip()}))"
        )
        result = runner.execute(
            {
                "helper": "fixture",
                "python_version": "Python 3.99.1",
                "command": shlex.join(["python3", "-c", script]),
            },
            None,
        )
        self.assertEqual(result["exit"], 0)
        self.assertEqual(
            result["stdout"],
            {"value": 42, "executable": "/usr/bin/python3", "version": "Python 3.99.1"},
        )
        self.assertEqual(result["effects"], [])

    def test_root_created_ipc_socket_belongs_to_fixture_identity(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with (
                patch.object(runner.os, "getuid", return_value=0),
                patch.object(runner.os, "chown") as chown,
            ):
                ipc = runner.IPC(root, [{"path": "runtime/fixture.sock"}])
            try:
                chown.assert_called_once_with(root / "runtime/fixture.sock", 1000, 1000)
                self.assertEqual(
                    (root / "runtime/fixture.sock").stat().st_mode & 0o777, 0o600
                )
            finally:
                ipc.close()

    def test_fixture_cannot_write_outside_root(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaisesRegex(ValueError, "escapes"):
                runner.fixtures(Path(directory), {"../outside": "bad"})

    def test_unavailable_sandbox_refuses_host_execution(self):
        with patch.object(runner.shutil, "which", return_value=None):
            with self.assertRaisesRegex(RuntimeError, "live-host fallback"):
                runner.execute({"helper": "launcher-preferences"}, None)

    def test_sandbox_start_failure_surfaces_stderr(self):
        with patch.object(runner.subprocess, "Popen") as spawn:
            process = spawn.return_value.__enter__.return_value
            process.returncode = 1
            process.communicate.return_value = (
                "",
                "bwrap: Creating new namespace failed: Operation not permitted\n",
            )
            with self.assertRaisesRegex(
                RuntimeError, "Parity sandbox.*Operation not permitted"
            ):
                runner.execute({"helper": "fixture"}, "python3 -c pass")

    def test_clean_environment_and_filesystem_observation(self):
        script = (
            "import json,os,pathlib; "
            "p=pathlib.Path.home()/'created';p.write_bytes(b'public');p.chmod(0o640);"
            "print(json.dumps({'home':str(pathlib.Path.home()),"
            "'secret_inherited':'PARITY_TEST_SECRET' in os.environ,"
            "'proc':list(pathlib.Path('/proc').iterdir())}))"
        )
        with patch.dict(os.environ, {"PARITY_TEST_SECRET": "test-only-sentinel"}):
            result = runner.execute(
                {"helper": "fixture"}, shlex.join(["python3", "-c", script])
            )
        self.assertEqual(
            result["stdout"],
            {"home": "/case/home", "secret_inherited": False, "proc": []},
        )
        self.assertEqual(
            result["files"], {"home/created": {"mode": "0o640", "text": "public"}}
        )

    def test_changed_output_and_missing_writes_fail_cli(self):
        command = shlex.join(["python3", "-c", "print('{}')"])
        result = subprocess.run(
            [
                sys.executable,
                str(HERE / "run.py"),
                "--case",
                "launcher-preferences/initialize",
                "--command",
                command,
            ],
            capture_output=True,
            text=True,
        )
        self.assertEqual(result.returncode, 1)
        self.assertIn("FAIL", result.stderr)
        self.assertIn("files", result.stderr)

    def test_effect_argv_and_binary_stdin_are_preserved(self):
        case = json.loads((HERE / "screenshot/case-region.json").read_text())
        result = runner.execute(case, None)
        copied = next(
            effect for effect in result["effects"] if effect["command"] == "wl-copy"
        )
        self.assertEqual(copied["argv"], ["--type", "image/png"])
        self.assertEqual(copied["stdin_base64"], "iVBORw0KGgo=")
        changed = json.loads(json.dumps(result))
        changed["effects"][0]["argv"].append("unexpected")
        self.assertNotEqual(changed, result)

    def test_partial_packet_timeout_and_buffered_packets(self):
        with subprocess.Popen(
            [
                sys.executable,
                "-c",
                "import sys,time;sys.stdout.write('{');sys.stdout.flush();time.sleep(5)",
            ],
            stdout=subprocess.PIPE,
        ) as process:
            try:
                with self.assertRaisesRegex(RuntimeError, "timed out"):
                    runner.read_packet(process, timeout=0.05)
            finally:
                process.kill()
                process.wait()
        with subprocess.Popen(
            [sys.executable, "-c", "print('{}');print('{\"id\":2}')"],
            stdout=subprocess.PIPE,
        ) as process:
            self.assertEqual(runner.read_packet(process), {})
            self.assertEqual(runner.read_packet(process), {"id": 2})
            process.wait()

    def test_normalization_preserves_other_fields_and_bytes(self):
        result = {
            "stdout": {"checked": 123, "count": 5},
            "files": {"text": '{"time": 123, "count":5}'},
        }
        runner.normalize(
            result,
            [
                {
                    "kind": "timestamp",
                    "pointer": "/stdout/checked",
                    "reason": "sampling time",
                },
                {
                    "kind": "timestamp",
                    "pointer": "/files/text",
                    "json_pointer": "/time",
                    "reason": "persisted sampling time",
                },
            ],
        )
        self.assertEqual(result["stdout"], {"checked": "<timestamp>", "count": 5})
        self.assertEqual(result["files"]["text"], '{"time": "<timestamp>", "count":5}')
        with self.assertRaises(ValueError):
            runner.normalize(
                result,
                [{"kind": "uuid", "pointer": "/stdout/count", "reason": "forbidden"}],
            )

    def test_direct_orient_policy_keeps_data_writes_and_other_actions_exact(self):
        call = {
            "command": "orient",
            "argv": ["--rpc", "--serve"],
            "stdin": '{"id":1,"operation":"from_image","payload":{"path":"/case/home/demo.png"}}\n',
        }
        desktop = {
            "command": "gsettings",
            "argv": ["set", "schema", "key", "value"],
            "stdin": "",
        }
        reference = {
            "stdout": {"engine": "exact-pin", "colours": {"primary": "224466"}},
            "files": {"home/state": {"text": "exact"}},
            "effects": [call, desktop],
        }
        direct = {**reference, "effects": [desktop]}
        case = {"native_engine": True}
        self.assertEqual(
            runner.orient_direct_equivalence(reference, case),
            runner.orient_direct_equivalence(direct, case),
        )
        changed = {**direct, "stdout": {"engine": "different-pin"}}
        self.assertNotEqual(
            runner.orient_direct_equivalence(reference, case),
            runner.orient_direct_equivalence(changed, case),
        )
        self.assertEqual(runner.orient_direct_equivalence(reference, {}), reference)
        for event in (
            {**call, "argv": ["--rpc", "--unexpected"]},
            {**call, "stdin": '{"operation":"publish","payload":{}}\n'},
            {**call, "stdin": "malformed\n"},
        ):
            value = {**direct, "effects": [event]}
            self.assertEqual(
                runner.orient_direct_equivalence(value, case)["effects"], [event]
            )

    def test_dbus_equivalent_action_and_wrong_argument(self):
        case = json.loads(
            (HERE / "device-actions/case-wifi-radio-dbus-equivalent.json").read_text()
        )
        baseline = runner.execute(case, None)

        def candidate(enabled):
            script = f"""import asyncio,json,os
from dbus_next import Message, MessageType, Variant
from dbus_next.aio import MessageBus
async def main():
    bus=await MessageBus(bus_address=os.environ['DBUS_SYSTEM_BUS_ADDRESS']).connect()
    reply=await bus.call(Message(destination='org.freedesktop.NetworkManager',path='/org/freedesktop/NetworkManager',interface='org.freedesktop.DBus.Properties',member='Set',signature='ssv',body=['org.freedesktop.NetworkManager','WirelessEnabled',Variant('b',{enabled!r})]))
    bus.disconnect()
    await bus.wait_for_disconnect()
    print(json.dumps({{'message':'Device updated'}}))
asyncio.run(main())
"""
            return shlex.join(["python3", "-c", script])

        with tempfile.TemporaryDirectory() as directory:
            trace = Path(directory) / "trace.json"
            equivalent = runner.execute(case, candidate(False), trace)
            self.assertEqual(equivalent, baseline)
            calls = json.loads(trace.read_text())["dbus_calls"]
            self.assertEqual(calls[0], case["dbus"][0]["match"])
        wrong = runner.execute(case, candidate(True))
        self.assertNotEqual(wrong, baseline)
        self.assertIn("unexpected_dbus", wrong["effects"][0])


if __name__ == "__main__":
    unittest.main()
