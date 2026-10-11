#!/usr/bin/env python3
"""Occasional read-only disk/sysfs snapshot; fast proc sampling stays native."""

import json
import math
from pathlib import Path
import shutil


def number(path, scale=1):
    try:
        result = float(path.read_text().strip()) / scale
        return result if math.isfinite(result) else None
    except (OSError, ValueError):
        return None


def sensors(root=Path("/sys")):
    cpu = []
    gpu = []
    usage = []
    for hwmon in sorted((root / "class/hwmon").glob("hwmon*")):
        try:
            name = (hwmon / "name").read_text().strip().lower()
        except OSError:
            continue
        if name not in (
            "coretemp",
            "k10temp",
            "zenpower",
            "cpu_thermal",
            "amdgpu",
            "nouveau",
        ):
            continue
        for source in sorted(hwmon.glob("temp*_input")):
            value = number(source, 1000)
            if value is None or not -40 <= value <= 150:
                continue
            if name in ("amdgpu", "nouveau"):
                gpu.append(value)
            else:
                try:
                    label = (
                        source.with_name(source.name.replace("_input", "_label"))
                        .read_text()
                        .lower()
                    )
                except OSError:
                    label = ""
                cpu.append(
                    (any(word in label for word in ("package", "tctl", "tdie")), value)
                )
    for source in sorted((root / "class/drm").glob("card*/device/gpu_busy_percent")):
        value = number(source)
        if value is not None and 0 <= value <= 100:
            usage.append(value / 100)
    preferred = [value for package, value in cpu if package]
    return {
        "cpuTemp": max(preferred or [value for _, value in cpu], default=None),
        "gpuTemp": max(gpu, default=None),
        "gpuPerc": max(usage, default=None),
        "gpuUsageAvailable": bool(usage),
    }


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
