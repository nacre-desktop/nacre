"""Real stdio framing, request isolation, bounded work and legacy equivalence."""

import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).parents[1]
spec = importlib.util.spec_from_file_location(
    "helper_service", TOOLS / "helper-service.py"
)
service = importlib.util.module_from_spec(spec)
spec.loader.exec_module(service)


def request(ident, tool="launcher-preferences", arguments=None, payload=None):
    return {
        "id": ident,
        "tool": tool,
        "arguments": ["load"] if arguments is None else arguments,
        "input": payload,
    }


def packets(values, helpers=None):
    source = io.BytesIO(
        b"".join(json.dumps(value).encode() + b"\n" for value in values)
    )
    output = io.StringIO()
    code = service.serve(source, output, helpers)
    return code, [json.loads(line) for line in output.getvalue().splitlines()]


class HelperServiceTests(unittest.TestCase):
    def test_pending_limit_rejects_excess_work_and_output_is_bounded(self):
        released = threading.Event()

        class Helpers:
            def execute(self, item):
                released.wait(2)
                return "{}"

        output = io.StringIO()
        with patch.object(service, "MAX_PENDING", 2):
            source = io.BytesIO(
                b"".join(json.dumps(request(i)).encode() + b"\n" for i in (1, 2, 3))
            )
            thread = threading.Thread(
                target=service.serve, args=(source, output, Helpers())
            )
            thread.start()
            try:
                deadline = time.monotonic() + 1
                while (
                    "Helper queue is full" not in output.getvalue()
                    and time.monotonic() < deadline
                ):
                    time.sleep(0.01)
                replies = [json.loads(line) for line in output.getvalue().splitlines()]
                rejected = next(row for row in replies if row.get("id") == 3)
                self.assertEqual(rejected["code"], 1)
            finally:
                released.set()
                thread.join(3)
            self.assertFalse(thread.is_alive())

        class LargeReply:
            def execute(self, item):
                return "x" * 1000

        with patch.object(service, "MAX_RESPONSE", 256):
            code, replies = packets([request(1)], LargeReply())
        self.assertEqual(code, 0)
        self.assertEqual(replies[1]["code"], 1)
        self.assertIn("too large", replies[1]["output"])

    def test_network_dispatch_preserves_read_only_snapshot_and_failure(self):
        helpers = service.Helpers()
        module = helpers.module("network-state")
        with patch.object(
            module, "snapshot", return_value={"wifiEnabled": False}
        ) as snapshot:
            self.assertEqual(
                json.loads(helpers.execute(request(1, "network-state", []))),
                {"wifiEnabled": False},
            )
            snapshot.assert_called_once_with()
        with patch.object(
            module, "snapshot", side_effect=module.SnapshotError("Unavailable")
        ):
            with self.assertRaises(module.SnapshotError):
                helpers.execute(request(2, "network-state", []))

    def test_bad_packets_and_exception_fail_only_their_request(self):
        class Helpers:
            def execute(self, item):
                if item["id"] == 3:
                    raise RuntimeError("fixture failed")
                return '{"favorites":[]}\n'

        code, replies = packets(
            [{}, request(1, "../../execute"), request(2), request(3), request(4)],
            Helpers(),
        )
        self.assertEqual(code, 0)
        self.assertEqual(replies[0], {"ready": True, "version": 1})
        by_id = {row["id"]: row for row in replies[1:]}
        self.assertEqual(by_id[1]["code"], 1)
        self.assertEqual(by_id[3]["code"], 1)
        self.assertIn("fixture failed", by_id[3]["output"])
        self.assertEqual(by_id[2]["code"], 0)
        self.assertEqual(by_id[4]["code"], 0)

    def test_nonfinite_oversized_and_invalid_arguments_are_rejected(self):
        output = io.StringIO()
        service.serve(io.BytesIO(b'{"id":1,"input":NaN}\n'), output)
        self.assertEqual(json.loads(output.getvalue().splitlines()[1])["code"], 1)
        with patch.object(service, "MAX_REQUEST", 32):
            output = io.StringIO()
            self.assertEqual(service.serve(io.BytesIO(b"x" * 33), output), 1)
            self.assertIn("fatal", json.loads(output.getvalue().splitlines()[1]))
        for arguments in ("load", [1], ["a\0b"], ["x" * 65537], ["x"] * 9):
            with (
                self.subTest(arguments=str(arguments)[:20]),
                self.assertRaises(ValueError),
            ):
                service.validate(request(1, arguments=arguments))
        with self.assertRaises(ValueError):
            service.validate(request(True))

    def test_queue_is_bounded_and_independent_requests_can_complete_out_of_order(self):
        released = threading.Event()
        output = io.StringIO()

        class Helpers:
            def execute(self, item):
                if item["id"] == 1:
                    released.wait(2)
                return "{}"

        with patch.object(service, "MAX_PENDING", 2):
            source = io.BytesIO(
                b"".join(json.dumps(request(i)).encode() + b"\n" for i in (1, 2, 3))
            )
            thread = threading.Thread(
                target=service.serve, args=(source, output, Helpers())
            )
            thread.start()
            try:
                deadline = time.monotonic() + 2
                while (
                    '"id": 2' not in output.getvalue() and time.monotonic() < deadline
                ):
                    time.sleep(0.01)
                replies = [json.loads(line) for line in output.getvalue().splitlines()]
                self.assertTrue(any(row.get("id") == 2 for row in replies))
                self.assertFalse(any(row.get("id") == 1 for row in replies))
            finally:
                released.set()
                thread.join(3)
            self.assertFalse(thread.is_alive())
        replies = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual({row["id"] for row in replies[1:]}, {1, 2, 3})

    def test_explicit_dispatch_cannot_launch_a_menu_tool(self):
        helpers = service.Helpers()
        module = helpers.module("menu-backend")
        with patch.object(module, "tool") as action:
            with self.assertRaises(ValueError):
                helpers.execute(request(1, "menu-backend", ["tool", "capture-window"]))
            action.assert_not_called()

    def test_cli_and_service_results_match_and_modules_are_reused(self):
        with tempfile.TemporaryDirectory() as directory:
            environment = {
                **os.environ,
                "HOME": directory,
                "XDG_CONFIG_HOME": directory + "/config",
                "XDG_STATE_HOME": directory + "/state",
                "XDG_CACHE_HOME": directory + "/cache",
            }
            worker = subprocess.Popen(
                [sys.executable, str(TOOLS / "helper-service.py")],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                env=environment,
            )
            try:
                self.assertEqual(
                    json.loads(worker.stdout.readline()), {"ready": True, "version": 1}
                )
                operations = [
                    ("launcher-preferences", ["load"], None),
                    (
                        "launcher-preferences",
                        ["favorite"],
                        {"id": "fixture.desktop", "enabled": True},
                    ),
                    (
                        "notification-history",
                        ["save"],
                        [{"key": "fixture", "appName": "fixture", "summary": "test"}],
                    ),
                    ("notification-history", ["load"], None),
                    ("dashboard-data", ["read"], None),
                    ("dashboard-data", ["preferences", '{"focusMinutes":30}'], None),
                    ("menu-backend", ["convert", "invalid"], None),
                ]
                for ident, (tool, arguments, payload) in enumerate(operations, 1):
                    worker.stdin.write(
                        json.dumps(request(ident, tool, arguments, payload)) + "\n"
                    )
                    worker.stdin.flush()
                    reply = json.loads(worker.stdout.readline())
                    self.assertEqual(reply["id"], ident)
                    self.assertEqual(reply["code"], 0, reply)
                    cli = subprocess.run(
                        [sys.executable, str(TOOLS / (tool + ".py")), *arguments],
                        env=environment,
                        input=json.dumps(payload) + "\n",
                        text=True,
                        capture_output=True,
                        timeout=10,
                    )
                    self.assertEqual(cli.returncode, 0, cli.stderr)
                    if cli.stdout:
                        expected, actual = (
                            json.loads(cli.stdout),
                            json.loads(reply["output"]),
                        )
                        if isinstance(expected, dict):
                            expected.pop("checked", None)
                            actual.pop("checked", None)
                        self.assertEqual(actual, expected)
                    else:
                        self.assertEqual(reply["output"], "")
                self.assertTrue(
                    (Path(directory) / ".config/nacre/launcher.json").is_file()
                )
            finally:
                worker.stdin.close()
                worker.wait(timeout=10)
                worker.stdout.close()
                worker.stderr.close()
            self.assertEqual(worker.returncode, 0)
        helpers = service.Helpers()
        self.assertIs(
            helpers.module("launcher-preferences"),
            helpers.module("launcher-preferences"),
        )
