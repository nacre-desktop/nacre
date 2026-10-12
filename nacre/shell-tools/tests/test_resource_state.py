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
                dict(
                    cpuTemp=None,
                    gpuTemp=None,
                    gpuPerc=None,
                    gpuUsageAvailable=False,
                    gpuFrequencyMHz=None,
                    gpus=[],
                ),
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


class GPUVendorFixtures(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.root = Path(directory.name)

    def gpu(self, card, ident, vendor, driver, values):
        device = self.root / "bus/pci/devices" / ident
        device.mkdir(parents=True)
        (device / "class").write_text("0x030000")
        (device / "vendor").write_text(vendor)
        (device / "device").write_text("0x1234")
        (device / "driver").symlink_to(self.root / "bus/pci/drivers" / driver)
        target = self.root / "class/drm" / card
        target.mkdir(parents=True)
        (target / "device").symlink_to(device)
        for name, value in values.items():
            source = device / name
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_text(value)
        return device

    def test_amd_intel_i915_and_xe_keep_identity_and_optional_fields(self):
        self.gpu(
            "card0",
            "0000:01:00.0",
            "0x1002",
            "amdgpu",
            {
                "gpu_busy_percent": "0",
                "hwmon/hwmon0/name": "amdgpu",
                "hwmon/hwmon0/temp1_input": "45000",
            },
        )
        self.gpu(
            "card1",
            "0000:00:02.0",
            "0x8086",
            "i915",
            {
                "gt_act_freq_mhz": "350",
                "hwmon/hwmon1/name": "i915",
                "hwmon/hwmon1/temp1_input": "51000",
            },
        )
        self.gpu(
            "card2",
            "0000:03:00.0",
            "0x8086",
            "xe",
            {
                "tile0/gt0/freq0/act_freq": "750",
                "tile0/gt1/freq0/act_freq": "950",
                "hwmon/hwmon2/temp1_input": "62000",
                "gpu_busy_percent": "20",
            },
        )
        with patch.object(m, "nvidia_samples") as query:
            result = m.sensors(self.root)
            query.assert_not_called()
        self.assertEqual(len(result["gpus"]), 3)
        rows = {row["driver"]: row for row in result["gpus"]}
        self.assertEqual(rows["amdgpu"]["usage"], 0)
        self.assertIsNone(rows["i915"]["usage"])
        self.assertEqual(rows["i915"]["temperature"], 51)
        self.assertEqual(rows["i915"]["frequencyMHz"], 350)
        self.assertEqual(rows["xe"]["frequencyMHz"], 950)
        self.assertEqual(result["gpuTemp"], 62)
        self.assertEqual(result["gpuPerc"], 0.2)

    def test_nvidia_multiple_devices_unsupported_fields_and_query_deadline(self):
        import subprocess

        self.gpu("card0", "0000:01:00.0", "0x10de", "nvidia", {})
        self.gpu("card1", "0000:02:00.0", "0x10de", "nvidia", {})
        output = "00000000:01:00.0, NVIDIA Fixture, 60, 0, 210, 580.0\n00000000:02:00.0, NVIDIA Other, N/A, 50, [Not Supported], 580.0\nmalformed\n"
        with (
            patch.object(m.shutil, "which", return_value="/usr/bin/nvidia-smi"),
            patch.object(
                m.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 0, output),
            ) as query,
        ):
            result = m.sensors(self.root)
        self.assertEqual(len(result["gpus"]), 2)
        self.assertEqual(result["gpus"][0]["temperature"], 60)
        self.assertEqual(result["gpus"][0]["usage"], 0)
        self.assertIsNone(result["gpus"][1]["temperature"])
        self.assertEqual(result["gpus"][1]["usage"], 0.5)
        self.assertIsNone(result["gpus"][1]["frequencyMHz"])
        self.assertEqual(query.call_count, 1)
        self.assertEqual(query.call_args.kwargs["timeout"], 3)
        for error in (
            FileNotFoundError(),
            subprocess.TimeoutExpired("nvidia-smi", 3),
            subprocess.CalledProcessError(1, "nvidia-smi"),
        ):
            with (
                patch.object(m.shutil, "which", return_value="nvidia-smi"),
                patch.object(m.subprocess, "run", side_effect=error),
            ):
                self.assertFalse(m.sensors(self.root)["gpuUsageAvailable"])
        with (
            patch.object(m.shutil, "which", return_value=None),
            patch.object(m.subprocess, "run") as query,
        ):
            self.assertIsNone(m.sensors(self.root)["gpuTemp"])
            query.assert_not_called()

    def test_nouveau_invalid_metrics_and_unbound_gpu_inventory(self):
        self.gpu(
            "card0",
            "0000:01:00.0",
            "0x10de",
            "nouveau",
            {
                "gpu_busy_percent": "NaN",
                "hwmon/hwmon0/name": "nouveau",
                "hwmon/hwmon0/temp1_input": "48000",
            },
        )
        device = self.gpu(
            "card1",
            "0000:02:00.0",
            "0x8086",
            "i915",
            {"gt_act_freq_mhz": "-1", "hwmon/hwmon1/temp1_input": "999999"},
        )
        (device / "driver").unlink()
        with patch.object(m, "nvidia_samples") as query:
            result = m.sensors(self.root)
            query.assert_not_called()
        self.assertEqual(result["gpuTemp"], 48)
        self.assertFalse(result["gpuUsageAvailable"])
        self.assertIsNone(result["gpuFrequencyMHz"])
        self.assertIsNone(result["gpus"][1]["driver"])

    def test_hwmon_class_links_do_not_duplicate_gpu_records(self):
        device = self.gpu(
            "card0",
            "0000:01:00.0",
            "0x1002",
            "amdgpu",
            {"hwmon/hwmon0/name": "amdgpu", "hwmon/hwmon0/temp1_input": "45000"},
        )
        linked = self.root / "class/hwmon/hwmon0"
        linked.parent.mkdir(parents=True)
        linked.symlink_to(device / "hwmon/hwmon0")
        self.assertEqual(len(m.sensors(self.root)["gpus"]), 1)
