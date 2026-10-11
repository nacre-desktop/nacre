"""Read-only hardware discovery and guarded command boundaries."""

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "light_devices", Path(__file__).resolve().parents[1] / "light-devices.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


def device(root, kind, name, maximum, current):
    path = root / "class" / kind / name
    path.mkdir(parents=True)
    (path / "max_brightness").write_text(str(maximum))
    (path / "brightness").write_text(str(current))
    return path


class LightDeviceTests(unittest.TestCase):
    def test_discovery_chooses_only_backlight_and_keyboard_without_writes(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            device(root, "backlight", "panel", 100, 50)
            device(root, "leds", "asus::kbd_backlight", 3, 1)
            device(root, "leds", "input1::capslock", 1, 1)
            with patch.object(m.subprocess, "run") as run:
                result = m.discover(["eDP-1"], root)
            self.assertEqual(result["backlight"]["current"], 50)
            self.assertEqual(result["keyboard"]["device"], "asus::kbd_backlight")
            run.assert_not_called()

    def test_panel_minimum_keyboard_off_and_safe_argv(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            panel = device(root, "backlight", "panel", 100, 50)
            keys = device(root, "leds", "asus::kbd_backlight", 3, 1)
            with patch.object(
                m.subprocess, "run", return_value=subprocess.CompletedProcess([], 0)
            ) as run:
                m.write(dict(kind="backlight", device="panel", maximum=100), 0, root)
                self.assertEqual(run.call_args.args[0][-1], "1")
                self.assertEqual(run.call_args.kwargs["timeout"], 6)
                m.write(
                    dict(kind="keyboard", device="asus::kbd_backlight", maximum=3),
                    0,
                    root,
                )
                self.assertEqual(run.call_args.args[0][-1], "0")
                self.assertEqual(panel.joinpath("brightness").read_text(), "50")
                self.assertEqual(keys.joinpath("brightness").read_text(), "1")

    def test_stale_maximum_bad_class_and_paths_cannot_write(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            device(root, "backlight", "panel", 100, 50)
            for data in [
                dict(kind="backlight", device="../bad", maximum=100),
                dict(kind="backlight", device="panel", maximum=200),
                dict(kind="keyboard", device="input1::capslock", maximum=1),
                dict(kind="unknown"),
            ]:
                with patch.object(m.subprocess, "run") as run:
                    with self.assertRaises(ValueError):
                        m.write(data, 50, root)
                    run.assert_not_called()

    def test_ddc_terse_response_and_invalid_output(self):
        with patch.object(
            m.subprocess,
            "run",
            return_value=subprocess.CompletedProcess([], 0, stdout="VCP 10 C 70 100\n"),
        ) as run:
            self.assertEqual(m.ddc_read(4), (70, 100))
            self.assertEqual(
                run.call_args.args[0],
                ["ddcutil", "--bus", "4", "--brief", "getvcp", "10"],
            )
        for text in ["unreadable", "VCP 10 C 101 100", "VCP 10 C 0 0"]:
            with patch.object(
                m.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 0, stdout=text),
            ):
                with self.assertRaises(ValueError):
                    m.ddc_read(4)

    def test_ddc_connector_mapping_and_fingerprint_guard(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            connector = root / "class/drm/card0-DP-1"
            connector.mkdir(parents=True)
            (connector / "status").write_text("connected")
            (connector / "edid").write_bytes(b"fixture-edid")
            bus = root / "bus/i2c-4"
            bus.mkdir(parents=True)
            (connector / "ddc").symlink_to(bus)
            with patch.object(m, "ddc_read", return_value=(70, 100)):
                row = m.discover(["DP-1"], root)["ddc"][0]
            self.assertEqual(row["bus"], 4)
            self.assertEqual(row["screen"], "DP-1")
            (connector / "edid").write_bytes(b"different-monitor")
            with patch.object(m.subprocess, "run") as run:
                with self.assertRaises(ValueError):
                    m.write(row, 40, root)
                run.assert_not_called()

    def test_ambiguous_connector_mapping_stays_unavailable(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            for index in [0, 1]:
                connector = root / ("class/drm/card" + str(index) + "-DP-1")
                connector.mkdir(parents=True)
                (connector / "status").write_text("connected")
                (connector / "edid").write_bytes(b"fixture")
                bus = root / ("bus/i2c-" + str(index))
                bus.mkdir(parents=True)
                (connector / "ddc").symlink_to(bus)
            with patch.object(m.subprocess, "run") as run:
                self.assertEqual(m.discover(["DP-1"], root)["ddc"], [])
                run.assert_not_called()
