import importlib.util
import tempfile
import unittest
import json
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "timezone_tools", Path(__file__).resolve().parents[1] / "timezone.py"
)
tz = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tz)


class TravelTimezoneTests(unittest.TestCase):
    def test_timezone_validation_rejects_path_and_shell_injection(self):
        self.assertEqual(tz.valid_zone("America/Chicago"), "America/Chicago")
        for value in (
            "../../etc/passwd",
            "America/Chicago;bad",
            "/etc/localtime",
            "Not/A_Real_Zone",
            "America/Chicago\n",
        ):
            with self.assertRaises((ValueError, KeyError)):
                tz.valid_zone(value)

    def test_failed_device_lookup_keeps_confirmed_zone_not_ip_zone(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            config = state / "config.json"
            config.write_text(
                '{"automatic":true,"confirmedTimezone":"America/Chicago"}'
            )
            with (
                patch.object(tz, "STATE", state),
                patch.object(tz, "CONFIG", config),
                patch.object(tz, "device_detect", side_effect=TimeoutError),
                patch.object(tz, "run", return_value="America/New_York") as run,
            ):
                tz.update(force=True)
            run.assert_any_call("timedatectl", "set-timezone", "America/Chicago")
            data = json.loads((state / "status.json").read_text())
            self.assertEqual(data["detected"], "America/Chicago")
            self.assertIn("IP location is not used", data["error"])

    def test_trustworthy_device_zone_applies_and_becomes_fallback(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            config = state / "config.json"
            config.write_text('{"automatic":true}')
            with (
                patch.object(tz, "STATE", state),
                patch.object(tz, "CONFIG", config),
                patch.object(
                    tz,
                    "device_detect",
                    return_value={
                        "timezone": "Europe/Oslo",
                        "source": "Device location",
                        "accuracyMeters": 100,
                    },
                ),
                patch.object(tz, "run", return_value="America/Chicago") as run,
            ):
                tz.update(force=True)
            run.assert_any_call("timedatectl", "set-timezone", "Europe/Oslo")
            self.assertEqual(
                json.loads(config.read_text())["lastTrustedTimezone"], "Europe/Oslo"
            )

    def test_disabled_automatic_mode_never_queries_location(self):
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "config.json"
            config.write_text('{"automatic":false}')
            with (
                patch.object(tz, "CONFIG", config),
                patch.object(tz, "STATE", Path(directory)),
                patch.object(tz, "device_detect") as detect,
            ):
                tz.update(force=True)
            detect.assert_not_called()
