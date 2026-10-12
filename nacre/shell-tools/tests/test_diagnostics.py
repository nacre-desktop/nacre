"""Support exports must be complete, bounded and private on unfamiliar machines."""

import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("diagnostics", TOOLS / "diagnostics.py")
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def write(root, files):
    for name, value in files.items():
        path = root / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value)


class DiagnosticsTests(unittest.TestCase):
    def test_absence_and_mixed_scale_inventory_fixtures(self):
        for fixture in (TOOLS / "tests/fixtures/hardware").glob("*.json"):
            data = json.loads(fixture.read_text())
            with (
                self.subTest(fixture=fixture.name),
                tempfile.TemporaryDirectory() as folder,
            ):
                root = Path(folder)
                write(root, data["files"])
                result = m.hardware(root)
                for key, expected in data["expected"].items():
                    self.assertEqual(result[key], expected)
                displays = m.monitors(dict(value=json.dumps(data["monitors"])))
                self.assertEqual(
                    [row["scale"] for row in displays],
                    [row["scale"] for row in data["monitors"]],
                )
                self.assertEqual(
                    [row["refreshRate"] for row in displays],
                    [row["refreshRate"] for row in data["monitors"]],
                )
        with tempfile.TemporaryDirectory() as folder:
            self.assertEqual(m.hardware(Path(folder))["formFactor"], "unknown")

    def test_recursive_redaction_covers_values_keys_and_free_form_errors(self):
        raw = {
            "username": "stranger",
            "hostname": "test-host",
            "SSID": "Coffee House",
            "accessToken": "abc123",
            "private_key": "ssh-private",
            "message": "stranger@test-host /home/stranger/file /Users/another/file 192.168.1.2 2001:db8::42 fe80::2%eth0 AA:BB:CC:DD:EE:FF\nCoffee House\nBearer a.short.but.secret\npassword=two words\nhttps://user:pass@example.com/?code=secret\nabcDEF0123456789abcdefghijklmnop",
            "nested": [{"/home/stranger/key": "test-host"}],
        }
        result = m.redact(raw, {"stranger", "test-host", "Coffee House"})
        serialized = json.dumps(result)
        for private in (
            "stranger",
            "test-host",
            "Coffee House",
            "another",
            "192.168.1.2",
            "2001:db8::42",
            "fe80::2",
            "AA:BB:CC:DD:EE:FF",
            "a.short.but.secret",
            "two words",
            "user:pass",
            "example.com",
            "abcDEF0123456789abcdefghijklmnop",
            "ssh-private",
        ):
            self.assertNotIn(private, serialized)
        self.assertEqual(result["accessToken"], "[redacted]")
        self.assertEqual(
            m.redact("GPU 0% / 42 C kernel 6.12.4"), "GPU 0% / 42 C kernel 6.12.4"
        )

    def test_short_named_credentials_in_logs_are_redacted(self):
        raw = 'OPENAI_API_KEY=short-synthetic\naccess_token":"tiny"\nclientSecret: abc123\ncookie=\'short-cookie\'\npostgres://other:otherpass@db.example/test'
        result = m.redact(raw)
        for secret in (
            "short-synthetic",
            "tiny",
            "abc123",
            "short-cookie",
            "otherpass",
            "db.example",
        ):
            self.assertNotIn(secret, result)

    def test_probes_are_bounded_read_only_and_do_not_return_sensitive_stderr(self):
        with patch.object(
            m.subprocess,
            "run",
            return_value=subprocess.CompletedProcess([], 0, "value", "secret"),
        ) as run:
            self.assertEqual(m.probe(["test"]), dict(status="ok", value="value"))
            self.assertEqual(run.call_args.kwargs["timeout"], 3)
            self.assertEqual(run.call_args.kwargs["stdin"], subprocess.DEVNULL)
            self.assertEqual(run.call_args.kwargs["env"]["LC_ALL"], "C")
        for error in (FileNotFoundError(), subprocess.TimeoutExpired("test", 3)):
            with patch.object(m.subprocess, "run", side_effect=error):
                self.assertIsNone(m.probe(["test"])["value"])
        with patch.object(
            m.subprocess,
            "run",
            return_value=subprocess.CompletedProcess(
                [], 0, "x" * (2 * 1024 * 1024 + 1)
            ),
        ):
            self.assertIsNone(m.probe(["test"])["value"])

    def test_optional_missing_unit_preserves_other_service_evidence(self):
        with patch.object(
            m.subprocess,
            "run",
            return_value=subprocess.CompletedProcess(
                [], 1, "Id=nacre-shell.service\nActiveState=active", "private stderr"
            ),
        ):
            self.assertEqual(
                m.probe(["systemctl", "--user", "show", "nacre-shell.service"])[
                    "status"
                ],
                "partial",
            )
            self.assertIn(
                "ActiveState=active", m.probe(["systemctl", "--user", "show"])["value"]
            )
            self.assertIsNone(m.probe(["other"])["value"])

    def test_report_has_all_sections_and_redacts_before_either_renderer(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            write(
                root,
                {
                    "etc/os-release": 'ID=fixture\nPRETTY_NAME="Fixture Linux"',
                    "proc/cpuinfo": "model name : Fixture CPU",
                },
            )
            seen = []

            def probe(args):
                seen.append(args)
                output = "stranger@test-host /home/stranger/file 192.168.1.2"
                if args[0] == "pacman":
                    output = "python 3.14\nquickshell 0.3.2"
                if args[:3] == ["hyprctl", "-j", "monitors"]:
                    output = json.dumps(
                        [
                            dict(
                                name="DP-1",
                                width=3840,
                                height=2160,
                                scale=2,
                                refreshRate=60,
                                serial="private serial",
                            )
                        ]
                    )
                if args[0] == "pw-dump":
                    output = json.dumps(
                        [
                            {
                                "info": {
                                    "props": {
                                        "media.class": "Audio/Sink",
                                        "node.description": "stranger Headphones",
                                        "device.serial": "private serial",
                                    },
                                    "state": "running",
                                }
                            }
                        ]
                    )
                return dict(status="ok", value=output)

            with (
                patch.object(m, "probe", side_effect=probe),
                patch.object(
                    m, "sensitive_names", return_value={"stranger", "test-host"}
                ),
                patch.object(m.shutil, "which", return_value=None),
            ):
                report = m.collect(root, root / "etc", root / "proc", root)
            for field in (
                "software",
                "hardware",
                "monitors",
                "audio",
                "dependencies",
                "services",
                "recentErrors",
            ):
                self.assertIn(field, report)
            self.assertEqual(report["software"]["distro"]["ID"], "fixture")
            self.assertEqual(report["hardware"]["cpu"]["model"], "Fixture CPU")
            self.assertEqual(report["monitors"]["devices"][0]["scale"], 2)
            self.assertIn("hyprland", report["dependencies"]["missingPackages"])
            self.assertNotIn("python", report["dependencies"]["missingPackages"])
            for serialized in (json.dumps(report), m.markdown(report)):
                for private in (
                    "stranger",
                    "test-host",
                    "/home/stranger",
                    "192.168.1.2",
                    "private serial",
                ):
                    self.assertNotIn(private, serialized)
            self.assertTrue(
                any(args[0] == "journalctl" and "-n" in args for args in seen)
            )
            self.assertTrue(
                any(args[0] == "quickshell" and "--tail" in args for args in seen)
            )
            self.assertFalse(
                any("--show-secrets" in args or "sudo" in args for args in seen)
            )

    def test_malformed_device_payloads_cannot_break_the_export(self):
        self.assertEqual(
            m.audio_devices(dict(value='[{"info":"bad"},{"info":{"props":null}},{}]')),
            [],
        )
        self.assertEqual(m.monitors(dict(value="{}")), [])
        result = m.monitors(
            dict(value='[{"name":"DP-1","scale":NaN,"width":"bad","height":true}]')
        )
        self.assertIsNone(result[0]["scale"])
        self.assertIsNone(result[0]["width"])
        self.assertIsNone(result[0]["height"])
        json.dumps(result, allow_nan=False)

    def test_unavailable_commands_do_not_prevent_a_report(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(
                m, "probe", return_value=dict(status="unavailable", value=None)
            ),
            patch.object(m, "sensitive_names", return_value=set()),
        ):
            root = Path(folder)
            report = m.collect(root, root, root, root)
            self.assertIsNone(report["dependencies"]["missingPackages"])
            self.assertEqual(report["audio"]["devices"], [])
            self.assertEqual(report["monitors"]["devices"], [])
            self.assertIn("Unavailable", m.markdown(report))

    def test_doctor_cli_is_one_valid_json_document_or_markdown(self):
        # This process uses the real read-only collector; never print the raw report.
        for flag in ("--json", "--report"):
            result = subprocess.run(
                [sys.executable, str(TOOLS / "maintenance.py"), "doctor", flag],
                capture_output=True,
                text=True,
                timeout=60,
            )
            self.assertEqual(result.returncode, 0)
            if flag == "--json":
                self.assertEqual(json.loads(result.stdout)["schemaVersion"], 1)
            else:
                self.assertTrue(result.stdout.startswith("# Nacre support report"))
            self.assertNotIn(str(Path.home()), result.stdout)
