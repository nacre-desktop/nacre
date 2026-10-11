"""Read-only sysfs sensor selection and large-disk units."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from collections import namedtuple

spec = importlib.util.spec_from_file_location(
    "resource_state", Path(__file__).resolve().parents[1] / "resource-state.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class ResourceStateTests(unittest.TestCase):
    def test_package_sensor_priority_and_valid_gpu_idle_zero(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            cpu = root / "class/hwmon/hwmon0"
            cpu.mkdir(parents=True)
            for name, value in {
                "name": "coretemp",
                "temp1_input": "55000",
                "temp1_label": "Package id 0",
                "temp2_input": "60000",
                "temp2_label": "Core 0",
                "temp3_input": "999999",
            }.items():
                (cpu / name).write_text(value)
            gpu = root / "class/drm/card0/device"
            gpu.mkdir(parents=True)
            (gpu / "gpu_busy_percent").write_text("0")
            result = m.sensors(root)
            self.assertEqual(result["cpuTemp"], 55)
            self.assertTrue(result["gpuUsageAvailable"])
            self.assertEqual(result["gpuPerc"], 0)
            (gpu / "gpu_busy_percent").write_text("101")
            self.assertFalse(m.sensors(root)["gpuUsageAvailable"])

    def test_missing_malformed_and_unrelated_sensors_remain_unknown(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            self.assertEqual(
                m.sensors(root),
                dict(cpuTemp=None, gpuTemp=None, gpuPerc=None, gpuUsageAvailable=False),
            )
            hw = root / "class/hwmon/hwmon1"
            hw.mkdir(parents=True)
            (hw / "name").write_text("nvme")
            (hw / "temp1_input").write_text("35000")
            self.assertIsNone(m.sensors(root)["cpuTemp"])
            self.assertIsNone(m.number(hw / "missing"))
            (hw / "bad").write_text("nan")
            self.assertIsNone(m.number(hw / "bad"))

    def test_large_disk_uses_kib_without_signed_integer_overflow(self):
        disk = namedtuple("Disk", "total used free")(
            8 * 1024**4, 3 * 1024**4, 5 * 1024**4
        )
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(m.shutil, "disk_usage", return_value=disk),
        ):
            result = m.snapshot(Path(folder))
            self.assertEqual(result["storageUsed"], 3 * 1024**3)
            self.assertEqual(result["storageTotal"], 8 * 1024**3)
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(m.shutil, "disk_usage", side_effect=OSError()),
        ):
            self.assertIsNone(m.snapshot(Path(folder))["storageTotal"])
