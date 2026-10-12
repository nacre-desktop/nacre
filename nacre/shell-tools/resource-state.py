#!/usr/bin/env python3
"""Read-only GPU/sysfs snapshot; callers sample only while gauges are visible."""

import csv
import json
import math
from pathlib import Path
import re
import shutil
import subprocess

VENDORS = {"0x1002": "AMD", "0x10de": "NVIDIA", "0x8086": "Intel"}
CPU_SENSORS = {"coretemp", "k10temp", "zenpower", "cpu_thermal"}
GPU_SENSORS = {"amdgpu", "nouveau", "i915", "xe"}


def read(path):
    try:
        return path.read_text().strip()
    except OSError:
        return ""


def number(path, scale=1):
    return numeric(read(path), scale)


def numeric(text, scale=1, low=None, high=None):
    try:
        value = float(text) / scale
        if not math.isfinite(value):
            return None
        if low is not None and value < low or high is not None and value > high:
            return None
        return value
    except (ValueError, TypeError):
        return None


def temperatures(hwmon):
    return [
        value
        for source in sorted(hwmon.glob("temp*_input"))
        if (value := numeric(read(source), 1000, -40, 150)) is not None
    ]


def pci_id(value):
    """Normalize NVIDIA's eight-digit PCI domain to the Linux PCI spelling."""
    match = re.fullmatch(
        r"(?:0{4})?([0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7])", value.lower()
    )
    return match[1] if match else ""


def gpu_devices(root):
    """Include unbound PCI GPUs and DRM-only virtual devices; deduplicate cards."""
    devices = {}
    cards = {}
    for card in sorted((root / "class/drm").glob("card*")):
        if not re.fullmatch(r"card\d+", card.name):
            continue
        device = (card / "device").resolve()
        if device.exists():
            devices[device] = device
            cards[device] = card
    for device in sorted((root / "bus/pci/devices").glob("*")):
        if read(device / "class").startswith("0x03"):
            devices[device.resolve()] = device.resolve()
    return [(device, cards.get(device)) for device in sorted(devices)]


def gpu_inventory(root=Path("/sys"), devices=None):
    result = []
    for device, card in gpu_devices(root) if devices is None else devices:
        driver = device / "driver"
        result.append(
            dict(
                id=pci_id(device.name) or (card.name if card else device.name),
                card=card.name if card else None,
                vendor=VENDORS.get(read(device / "vendor"), "Unknown"),
                deviceId=read(device / "device") or None,
                driver=driver.resolve().name if driver.is_symlink() else None,
            )
        )
    return result


def nvidia_samples():
    executable = shutil.which("nvidia-smi")
    if not executable:
        return {}
    try:
        output = subprocess.run(
            [
                executable,
                "--query-gpu=pci.bus_id,name,temperature.gpu,utilization.gpu,clocks.current.graphics,driver_version",
                "--format=csv,noheader,nounits",
            ],
            capture_output=True,
            text=True,
            timeout=3,
            check=True,
        ).stdout
    except (OSError, subprocess.SubprocessError):
        return {}
    if len(output) > 65536:
        return {}
    result = {}
    for row in csv.reader(output.splitlines()):
        if len(row) != 6 or not (ident := pci_id(row[0].strip())):
            continue
        result[ident] = dict(
            name=row[1].strip(),
            temperature=numeric(row[2].strip(), low=-40, high=150),
            usage=numeric(row[3].strip(), 100, 0, 1),
            frequencyMHz=numeric(row[4].strip(), low=0, high=100000),
            driverVersion=row[5].strip(),
        )
    return result


def sensors(root=Path("/sys")):
    cpu = []
    devices = gpu_devices(root)
    records = gpu_inventory(root, devices)
    associated = set()
    for record, (device, card) in zip(records, devices, strict=True):
        hwmons = list((device / "hwmon").glob("hwmon*"))
        for hwmon in (root / "class/hwmon").glob("hwmon*"):
            if (hwmon / "device").resolve() == device:
                hwmons.append(hwmon)
        temps = []
        for hwmon in hwmons:
            associated.add(hwmon.resolve())
            temps.extend(temperatures(hwmon))
        frequency_paths = [device / "gt_act_freq_mhz"]
        if card:
            frequency_paths += [
                card / "gt_act_freq_mhz",
                *sorted((card / "gt").glob("gt*/rps_act_freq_mhz")),
            ]
        frequency_paths += sorted(device.glob("tile*/gt*/freq*/act_freq"))
        frequencies = [
            value
            for path in frequency_paths
            if (value := numeric(read(path), low=0, high=100000)) is not None
        ]
        record.update(
            temperature=max(temps, default=None),
            usage=numeric(read(device / "gpu_busy_percent"), 100, 0, 1),
            frequencyMHz=max(frequencies, default=None),
        )
    # Some older drivers expose only hwmon, without a DRM card.
    for hwmon in sorted((root / "class/hwmon").glob("hwmon*")):
        name = read(hwmon / "name").lower()
        if name in CPU_SENSORS:
            for source in sorted(hwmon.glob("temp*_input")):
                value = numeric(read(source), 1000, -40, 150)
                if value is not None:
                    label = read(
                        source.with_name(source.name.replace("_input", "_label"))
                    ).lower()
                    cpu.append(
                        (
                            any(word in label for word in ("package", "tctl", "tdie")),
                            value,
                        )
                    )
        elif name.split("_gt")[0] in GPU_SENSORS and hwmon.resolve() not in associated:
            records.append(
                dict(
                    id=hwmon.name,
                    card=None,
                    vendor={"amdgpu": "AMD", "nouveau": "NVIDIA"}.get(name, "Intel"),
                    driver=name,
                    temperature=max(temperatures(hwmon), default=None),
                    usage=None,
                    frequencyMHz=None,
                )
            )
    if any(record["driver"] == "nvidia" for record in records):
        samples = nvidia_samples()
        for record in records:
            if record["driver"] == "nvidia":
                record.update(samples.get(record["id"], {}))
    preferred = [value for package, value in cpu if package]
    usage = [record["usage"] for record in records if record["usage"] is not None]
    temps = [
        record["temperature"] for record in records if record["temperature"] is not None
    ]
    frequencies = [
        record["frequencyMHz"]
        for record in records
        if record["frequencyMHz"] is not None
    ]
    return dict(
        cpuTemp=max(preferred or [value for _, value in cpu], default=None),
        gpuTemp=max(temps, default=None),
        gpuPerc=max(usage, default=None),
        gpuUsageAvailable=bool(usage),
        gpuFrequencyMHz=max(frequencies, default=None),
        gpus=records,
    )


def snapshot(root=Path("/sys"), mount="/"):
    result = sensors(root)
    try:
        disk = shutil.disk_usage(mount)
        result.update(storageUsed=disk.used / 1024, storageTotal=disk.total / 1024)
    except OSError:
        result.update(storageUsed=None, storageTotal=None)
    return result


if __name__ == "__main__":
    print(json.dumps(snapshot(), allow_nan=False))
