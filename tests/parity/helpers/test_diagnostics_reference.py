"""Synthetic collector safety and retained diagnostics/maintenance assertions."""

import json
import hashlib
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import diagnostics_reference as driver
import freeze_diagnostics as capture


class DiagnosticsReferenceTests(unittest.TestCase):
    def value(self, label):
        result = json.loads(
            (
                capture.run.HERE / "diagnostics" / f"expected-support-{label}.json"
            ).read_text()
        )
        self.assertEqual(result["exit"], 0)
        self.assertEqual(result["files"], {})
        self.assertEqual(result["deleted"], [])
        return result["stdout"]

    def test_every_case_is_read_only_and_has_the_verified_source_identity(self):
        labels = []
        for label, _, _ in capture.cases():
            labels.append(label)
            case = json.loads(
                (capture.run.HERE / "diagnostics" / f"case-{label}.json").read_text()
            )
            self.assertEqual(case["reference_source_sha256"], capture.SOURCES)
            self.value(label.removeprefix("support-"))
        self.assertEqual(len(labels), 26)

    def test_driver_refuses_live_collection_before_importing_subjects(self):
        with (
            patch.dict(os.environ, {"NACRE_PARITY_ROOT": "not-a-sandbox"}),
            patch.object(driver, "module") as load,
        ):
            with self.assertRaisesRegex(RuntimeError, "isolated parity sandbox"):
                driver.execute({"operation": "collect"})
            load.assert_not_called()

    def test_unknown_probe_has_no_live_fallback(self):
        case = {
            "helper": "diagnostics",
            "command": "python3 tests/parity/helpers/diagnostics_reference.py",
            "args": [
                json.dumps(
                    {"operation": "probe", "args": ["unscripted-fixture-command"]}
                )
            ],
        }
        observed = capture.run.execute(case, None)
        self.assertEqual(observed["exit"], 1)
        self.assertIsNone(observed["stdout"])
        self.assertEqual(observed["effects"], [])
        self.assertEqual(observed["files"], {})

    def test_capture_refuses_changed_sources_and_existing_destinations(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(capture.run, "ROOT", Path(directory)),
                patch.object(capture.run, "execute") as execute,
            ):
                with self.assertRaisesRegex(ValueError, "source changed"):
                    capture.capture()
                execute.assert_not_called()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            sources = {}
            for name in capture.SOURCES:
                source = root / "nacre/shell-tools" / name
                source.parent.mkdir(parents=True, exist_ok=True)
                source.write_text("immutable synthetic source")
                sources[name] = hashlib.sha256(source.read_bytes()).hexdigest()
            folder = root / "reference/diagnostics"
            folder.mkdir(parents=True)
            reference = folder / "expected-fixture.json"
            reference.write_bytes(b"immutable expected bytes\n")
            with (
                patch.object(capture.run, "ROOT", root),
                patch.object(capture.run, "HERE", folder.parent),
                patch.object(capture, "SOURCES", sources),
                patch.object(
                    capture, "cases", return_value=iter([("fixture", {}, {})])
                ),
                patch.object(capture.run, "execute") as execute,
            ):
                with self.assertRaisesRegex(ValueError, "refusing overwrite"):
                    capture.capture()
                execute.assert_not_called()
            self.assertEqual(reference.read_bytes(), b"immutable expected bytes\n")

    def test_recursive_redaction_retains_public_metrics_and_removes_all_fixture_identifiers(
        self,
    ):
        result = self.value("redact-full")["value"]
        text = json.dumps(result)
        for private in [
            "fixture-user",
            "fixture-host",
            "Fixture Network",
            "fixture-other",
            "192.0.2.1",
            "2001:db8::42",
            "fe80::2",
            "AA:BB:CC:DD:EE:FF",
            "a.short.synthetic",
            "two synthetic words",
            "demo:fake",
            "example.invalid",
            "abcDEF0123456789abcdefghijklmnop",
            "fake-key",
        ]:
            self.assertNotIn(private, text)
        self.assertEqual(result["accessToken"], "[redacted]")
        self.assertEqual(
            self.value("redact-public")["value"], "GPU 0% / 42 C kernel 6.12.4"
        )
        short = self.value("redact-short")["value"]
        for private in [
            "short-synthetic",
            "sample",
            "synthetic-cookie",
            "demo:fake",
            "db.invalid",
        ]:
            self.assertNotIn(private, short)

    def test_probe_deadlines_limits_partial_units_and_stderr_policy(self):
        ok = self.value("probe-ok")
        self.assertEqual(ok["value"], {"status": "ok", "value": "value"})
        self.assertEqual(ok["calls"][0]["timeout"], 3)
        self.assertEqual(ok["calls"][0]["stdin"], -3)
        self.assertEqual(ok["calls"][0]["locale"], "C")
        self.assertEqual(ok["calls"][0]["noColor"], "1")
        for label in ["missing", "timeout", "oversized", "nonunit-error"]:
            self.assertIsNone(self.value("probe-" + label)["value"]["value"])
        partial = self.value("probe-partial-unit")["value"]
        self.assertEqual(partial["status"], "partial")
        self.assertIn("ActiveState=active", partial["value"])

    def test_report_sections_backend_commands_redaction_and_no_privileged_probes(self):
        captured = self.value("collect")
        report = captured["value"]
        for key in [
            "software",
            "hardware",
            "monitors",
            "audio",
            "dependencies",
            "services",
            "recentErrors",
        ]:
            self.assertIn(key, report)
        self.assertEqual(report["software"]["distro"]["ID"], "fixture")
        self.assertEqual(report["software"]["compositor"], "Fixture")
        self.assertIn("FixtureInstalled", report["software"]["versions"])
        self.assertEqual(report["hardware"]["cpu"]["model"], "Fixture CPU")
        self.assertEqual(report["monitors"]["devices"][0]["scale"], 2)
        self.assertIn("hyprland", report["dependencies"]["missingPackages"])
        self.assertNotIn("python", report["dependencies"]["missingPackages"])
        for rendered in [json.dumps(report), self.value("markdown")["value"]]:
            for private in [
                "fixture-user",
                "fixture-host",
                "/case/home",
                "192.0.2.1",
                "synthetic serial",
                "synthetic-short",
                "Fixture Network",
                "not exported",
            ]:
                self.assertNotIn(private, rendered)
        args = [call["args"] for call in captured["calls"]]
        for name in [
            "runningVersion",
            "installedVersion",
            "allMonitors",
            "configErrors",
        ]:
            self.assertIn(["fixture-compositor", name], args)
        self.assertTrue(any(row[0] == "journalctl" and "-n" in row for row in args))
        self.assertTrue(any(row[0] == "quickshell" and "--tail" in row for row in args))
        self.assertFalse(any("sudo" in row or "--show-secrets" in row for row in args))

    def test_hardware_and_malformed_device_payloads_preserve_existing_assertions(self):
        root = capture.run.ROOT / "nacre/shell-tools/tests/fixtures/hardware"
        for path in root.glob("*.json"):
            expected = json.loads(path.read_text())
            value = self.value("hardware-" + path.stem)["value"]
            for key, result in expected["expected"].items():
                self.assertEqual(value["hardware"][key], result)
            for key in ["scale", "refreshRate"]:
                self.assertEqual(
                    [row[key] for row in value["monitors"]],
                    [row[key] for row in expected["monitors"]],
                )
        malformed = self.value("malformed-devices")["value"]
        self.assertEqual(self.value("monitors-object")["value"], [])
        self.assertEqual(malformed["audio"]["devices"], [])
        for key in ["scale", "width", "height"]:
            self.assertIsNone(malformed["monitors"]["devices"][0][key])
        unavailable = self.value("unavailable")["value"]
        self.assertIsNone(unavailable["dependencies"]["missingPackages"])
        self.assertEqual(unavailable["audio"]["devices"], [])
        self.assertEqual(unavailable["monitors"]["devices"], [])

    def test_actual_doctor_cli_formats_and_maintenance_backend_policy(self):
        raw = self.value("doctor-json")["value"]
        self.assertEqual(json.loads(raw)["schemaVersion"], 1)
        markdown = self.value("doctor-report")["value"]
        self.assertTrue(markdown.startswith("# Nacre support report"))
        self.assertNotIn("/case/home", raw)
        self.assertNotIn("/case/home", markdown)
        self.assertEqual(self.value("config-error")["value"], "bad option")
        self.assertEqual(self.value("config-timeout")["value"], "")
        state = self.value("maintenance-state")["value"]
        self.assertEqual(state["versions"]["Fixture"], "Fixture installed version")
        self.assertEqual(
            state["serviceDetails"]["nacre-shell.service"]["ActiveState"], "active"
        )
        self.assertEqual(
            state["services"]["xdg-desktop-portal.service"], "ready on demand"
        )
        self.assertEqual(state["updatesAgeSeconds"], 3600)
        self.assertEqual(
            self.value("corrupt-maintenance")["value"]["maintenance"]["status"],
            "unavailable",
        )


if __name__ == "__main__":
    unittest.main()
