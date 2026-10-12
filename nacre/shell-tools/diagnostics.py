"""Bounded, read-only support facts with one recursive export redaction boundary."""

import datetime as dt
import getpass
import importlib.util
import ipaddress
import json
import math
import os
from pathlib import Path
import platform
import re
import shutil
import socket
import subprocess
import sys

REQUIRED_PACKAGES = (
    "hyprland",
    "uwsm",
    "python",
    "python-pip",
    "python-pillow",
    "quickshell",
    "pipewire",
    "wireplumber",
    "networkmanager",
    "bluez",
    "librsvg",
    "qt6-imageformats",
    "qt6-multimedia",
    "qt6-multimedia-ffmpeg",
    "ddcutil",
    "cpptrace",
    "libdwarf",
    "wtype",
    "papirus-icon-theme",
    "kitty",
    "dolphin",
    "kio-extras",
    "qt6ct",
    "ark",
    "hypridle",
    "hyprlock",
    "grim",
    "slurp",
    "wl-clipboard",
    "libnotify",
    "brightnessctl",
    "ffmpeg",
    "fontconfig",
)
REQUIRED_COMMANDS = (
    "Hyprland",
    "hyprctl",
    "uwsm",
    "python3",
    "quickshell",
    "systemctl",
    "wpctl",
    "nmcli",
    "bluetoothctl",
    "kitty",
    "dolphin",
    "hypridle",
    "hyprlock",
    "grim",
    "slurp",
    "wl-copy",
    "wl-paste",
    "notify-send",
    "brightnessctl",
    "ffmpeg",
    "fc-list",
    "ddcutil",
    "wtype",
)
NACRE_UNITS = (
    "nacre-shell.service",
    "nacre-sidebar-ai.service",
    "nacre-brain.service",
    "nacre-session-watch.service",
    "nacre-brain-sync.service",
)


def read(path, limit=65536):
    try:
        with path.open() as stream:
            return stream.read(limit).strip()
    except (OSError, UnicodeError):
        return ""


def probe(args, timeout=3):
    try:
        result = subprocess.run(
            args,
            capture_output=True,
            text=True,
            timeout=timeout,
            env={**os.environ, "LC_ALL": "C", "NO_COLOR": "1"},
            stdin=subprocess.DEVNULL,
        )
        partial_units = args[:3] == ["systemctl", "--user", "show"] and bool(
            result.stdout.strip()
        )
        if result.returncode and not partial_units:
            return dict(status="unavailable", value=None)
        if len(result.stdout) > 2 * 1024 * 1024:
            return dict(status="unavailable (output limit)", value=None)
        return dict(
            status="partial" if result.returncode else "ok", value=result.stdout.strip()
        )
    except (OSError, UnicodeError, subprocess.SubprocessError):
        return dict(status="unavailable", value=None)


def parse_json(result, fallback):
    try:
        return json.loads(result.get("value") or "", parse_constant=lambda value: None)
    except (ValueError, TypeError):
        return fallback


def hardware(root=Path("/sys")):
    batteries = [
        path
        for path in (root / "class/power_supply").glob("*")
        if read(path / "type") == "Battery"
    ]
    chassis = read(root / "class/dmi/id/chassis_type")
    kind = (
        "laptop"
        if chassis in {"8", "9", "10", "14", "30", "31", "32"}
        else "desktop"
        if chassis in {"3", "4", "5", "6", "7", "13", "15", "16"}
        else "unknown"
    )
    if kind == "unknown" and batteries:
        kind = "laptop (inferred from battery)"
    interfaces = list((root / "class/net").glob("*"))
    return dict(
        formFactor=kind,
        chassisType=chassis or None,
        battery=bool(batteries),
        backlight=any((root / "class/backlight").glob("*")),
        bluetooth=any((root / "class/bluetooth").glob("hci*")),
        wifi=any(
            (path / "wireless").exists() or (path / "phy80211").exists()
            for path in interfaces
        ),
        ethernet=any(
            path.name != "lo"
            and read(path / "type") == "1"
            and not (path / "wireless").exists()
            and not (path / "phy80211").exists()
            for path in interfaces
        ),
    )


def gpu_inventory(root):
    spec = importlib.util.spec_from_file_location(
        "nacre_resource_inventory", Path(__file__).with_name("resource-state.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.gpu_inventory(root)


def monitors(result):
    raw = parse_json(result, [])
    if not isinstance(raw, list):
        return []
    # Do not export monitor serials, model descriptions, titles or window state.
    result = []
    for row in raw[:32]:
        if not isinstance(row, dict):
            continue
        device = dict(
            name=row.get("name") if isinstance(row.get("name"), str) else "Unknown"
        )
        for key in ("width", "height", "scale", "refreshRate", "x", "y", "transform"):
            value = row.get(key)
            device[key] = (
                value
                if isinstance(value, (int, float))
                and not isinstance(value, bool)
                and math.isfinite(value)
                else None
            )
        device["disabled"] = (
            row.get("disabled") if isinstance(row.get("disabled"), bool) else None
        )
        result.append(device)
    return result


def audio_devices(result):
    raw = parse_json(result, [])
    if not isinstance(raw, list):
        return []
    result = []
    for row in raw:
        if not isinstance(row, dict):
            continue
        info = row.get("info")
        if not isinstance(info, dict) or not isinstance(info.get("props"), dict):
            continue
        props = info["props"]
        if props.get("media.class") not in {"Audio/Sink", "Audio/Source"}:
            continue
        result.append(
            dict(
                kind=props["media.class"],
                description=props.get("node.description")
                if isinstance(props.get("node.description"), str)
                else "Unknown",
                api=props.get("device.api")
                if isinstance(props.get("device.api"), str)
                else None,
                state=info.get("state") if isinstance(info.get("state"), str) else None,
            )
        )
    return result[:64]


def sensitive_names():
    # Names are used only to redact; they are never included in the report.
    result = {
        getpass.getuser(),
        socket.gethostname(),
        str(Path.home()),
        os.environ.get("USER", ""),
        os.environ.get("LOGNAME", ""),
        os.environ.get("HOSTNAME", ""),
    }
    result.update(read(Path("/etc/hostname")).splitlines())
    for args in (
        [
            "nmcli",
            "--terse",
            "--escape",
            "no",
            "--fields",
            "SSID",
            "device",
            "wifi",
            "list",
            "--rescan",
            "no",
        ],
        [
            "nmcli",
            "--terse",
            "--escape",
            "no",
            "--fields",
            "NAME",
            "connection",
            "show",
        ],
    ):
        value = probe(args).get("value") or ""
        result.update(value.splitlines())
    return result - {"", "--"}


def redact(value, identities=()):
    """Scrub both keys and values, including free-form errors and nested strings."""
    sensitive_key = re.compile(
        r"(?i)(?:token|secret|password|passwd|authorization|cookie|api.?key|ssid|username|hostname|serial|private.?key)"
    )

    def text(source):
        source = re.sub(r"\x1b\[[0-?]*[ -/]*[@-~]", "", source)
        source = re.sub(r"[\x00-\x08\x0b-\x1f\x7f]", "", source)
        # Whole lines with a named secret/SSID may contain spaces and punctuation.
        source = re.sub(
            r"(?im)\b(?:authorization|bearer|token|secret|password|passwd|api[_-]?key|ssid|user(?:name)?|host(?:name)?|wi-?fi (?:name|network))\b\s*[:= ]+[^\n]+",
            "[redacted sensitive value]",
            source,
        )
        source = re.sub(r"[\w.+-]+@[\w.-]+", "[account]", source)
        source = re.sub(r"(?i)https?://[^\s]+", "[redacted URL]", source)
        source = re.sub(r"(?:/home/|/Users/)[^/\s]+|/root(?=/|\s|$)", "[home]", source)
        for identity in sorted(set(identities), key=len, reverse=True):
            if not identity:
                continue
            if "/" in identity:
                source = source.replace(identity, "[home]")
            else:
                source = re.sub(
                    r"(?<![\w-])" + re.escape(identity) + r"(?![\w-])",
                    "[identity]",
                    source,
                    flags=re.I,
                )
        source = re.sub(r"(?i)\b(?:[0-9a-f]{2}[:-]){5}[0-9a-f]{2}\b", "[MAC]", source)

        def address(match):
            candidate = match[0]
            try:
                ipaddress.ip_address(candidate.split("%")[0])
                return "[IP]"
            except ValueError:
                return candidate

        source = re.sub(r"(?<![\w.])(?:\d{1,3}\.){3}\d{1,3}(?![\w.])", address, source)
        source = re.sub(
            r"(?<![\w:])(?:[0-9a-fA-F]{0,4}:){2,}[0-9a-fA-F:.]+(?:%[\w.-]+)?",
            address,
            source,
        )
        source = re.sub(
            r"\beyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+",
            "[token-like value]",
            source,
        )
        source = re.sub(
            r"(?<![\w])[A-Za-z0-9_+/=-]{24,}(?![\w])", "[token-like value]", source
        )
        return source

    def walk(item):
        if isinstance(item, str):
            return text(item)
        if isinstance(item, dict):
            return {
                text(str(key)): "[redacted]"
                if sensitive_key.search(str(key))
                else walk(child)
                for key, child in item.items()
            }
        if isinstance(item, (list, tuple)):
            return [walk(child) for child in item]
        return item

    return walk(value)


def maintenance_facts(home):
    # Keep the existing doctor's useful local health fields; omit arbitrary profile
    # labels and optional account/memory synchronization metadata from exports.
    spec = importlib.util.spec_from_file_location(
        "nacre_maintenance_facts", Path(__file__).with_name("maintenance.py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.HOME = home
    module.STATE = home / ".local/state/nacre"
    try:
        local = module.state()
        fields = (
            "services",
            "serviceDetails",
            "failedServices",
            "drift",
            "configErrors",
            "updatesAgeSeconds",
        )
        return {key: local.get(key) for key in fields}
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError):
        return dict(status="unavailable")


def collect(root=Path("/sys"), etc=Path("/etc"), proc=Path("/proc"), home=None):
    home = home or Path.home()
    distro = {}
    for line in read(etc / "os-release").splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            if key in {"ID", "VERSION_ID", "PRETTY_NAME"}:
                distro[key] = value.strip('"')
    versions = {
        "compositor": probe(["hyprctl", "version"]),
        "HyprlandInstalled": probe(["Hyprland", "--version"]),
        "Quickshell": probe(["quickshell", "--version"]),
        "Qt": probe(
            [shutil.which("qtpaths6") or "/usr/lib/qt6/bin/qtpaths", "--qt-version"]
        ),
        "Python": probe([sys.executable, "--version"]),
    }
    gpu = gpu_inventory(root)
    for record in gpu:
        if re.fullmatch(r"[0-9a-f]{4}:[0-9a-f]{2}:[0-9a-f]{2}\.[0-7]", record["id"]):
            record["model"] = probe(["lspci", "-s", record["id"]]).get("value")
        if record["driver"]:
            record["driverVersion"] = (
                probe(["modinfo", "-F", "version", record["driver"]]).get("value")
                or platform.release()
            )
    monitor_probe = probe(["hyprctl", "-j", "monitors", "all"])
    audio_probe = probe(["pw-dump"])
    installed = probe(["pacman", "-Q"])
    packages = {
        parts[0]: parts[1]
        for line in (installed["value"] or "").splitlines()
        if len(parts := line.split()) == 2
    }
    cpu = next(
        (
            line.split(":", 1)[1].strip()
            for line in read(proc / "cpuinfo").splitlines()
            if line.split(":", 1)[0].strip() in {"model name", "Hardware"}
        ),
        platform.machine(),
    )
    units = probe(
        [
            "systemctl",
            "--user",
            "show",
            *NACRE_UNITS,
            "-p",
            "Id",
            "-p",
            "LoadState",
            "-p",
            "ActiveState",
            "-p",
            "Result",
            "-p",
            "ExecMainStatus",
        ]
    )
    journal = probe(
        [
            "journalctl",
            "--user",
            "--no-pager",
            "-o",
            "cat",
            "--since",
            "1 hour ago",
            "-p",
            "warning",
            "-n",
            "80",
            *[argument for unit in NACRE_UNITS for argument in ("-u", unit)],
        ]
    )
    shell_log = probe(
        ["quickshell", "log", "--config", "nacre", "--no-color", "--tail", "60"]
    )
    config = probe(["hyprctl", "configerrors"])
    release_path = home / ".local/state/nacre/releases/current.json"
    try:
        release = json.loads(read(release_path))
        release = {key: release.get(key) for key in ("revision", "status", "checked")}
    except (ValueError, AttributeError):
        release = {}
    result = dict(
        schemaVersion=1,
        checked=dt.datetime.now(dt.timezone.utc).isoformat(),
        software=dict(
            distro=distro,
            kernel=platform.release(),
            architecture=platform.machine(),
            compositor="Hyprland"
            if versions["compositor"]["status"] == "ok"
            else "unavailable",
            versions=versions,
            release=release,
        ),
        hardware=dict(
            cpu=dict(model=cpu, logicalCpus=os.cpu_count()), gpus=gpu, **hardware(root)
        ),
        monitors=dict(status=monitor_probe["status"], devices=monitors(monitor_probe)),
        audio=dict(status=audio_probe["status"], devices=audio_devices(audio_probe)),
        dependencies=dict(
            packageManager="pacman"
            if installed["status"] == "ok"
            else "unavailable (command checks only)",
            missingPackages=[name for name in REQUIRED_PACKAGES if name not in packages]
            if installed["status"] == "ok"
            else None,
            missingCommands=[
                name for name in REQUIRED_COMMANDS if not shutil.which(name)
            ],
        ),
        services=units,
        maintenance=maintenance_facts(home),
        recentErrors=dict(journal=journal, shellLog=shell_log, compositorConfig=config),
        privacy="Redacted locally. No account files, environment dump, Wi-Fi scan, uploads or privileged probes. Review free-form logs before sharing.",
    )
    size = (
        probe(["du", "-s", "-B1", str(home / ".local/state/nacre/releases")]).get(
            "value"
        )
        or ""
    )
    result["maintenance"]["releaseDiskBytes"] = (
        int(size.split()[0]) if size.split() and size.split()[0].isdigit() else None
    )
    return redact(result, sensitive_names())


def markdown(report):
    lines = [
        "# Nacre support report",
        "",
        "Collected: " + report["checked"],
        "",
        report["privacy"],
        "",
    ]

    def label(key):
        names = {
            "cpu": "CPU",
            "gpus": "GPUs",
            "Qt": "Qt",
            "Python": "Python",
            "Quickshell": "Quickshell",
            "deviceId": "Device ID",
            "logicalCpus": "Logical CPUs",
            "refreshRate": "Refresh rate (Hz)",
            "releaseDiskBytes": "Release disk usage (bytes)",
        }
        return names.get(key, re.sub(r"(?<=[a-z0-9])(?=[A-Z])", " ", key).capitalize())

    def render(value, indent=""):
        if isinstance(value, dict):
            for key, child in value.items():
                if isinstance(child, (dict, list)):
                    lines.append(indent + "- **" + label(key) + "**:")
                    render(child, indent + "  ")
                else:
                    lines.append(
                        indent
                        + "- **"
                        + label(key)
                        + "**: "
                        + scalar(child, indent + "  ")
                    )
        elif isinstance(value, list):
            if not value:
                lines.append(indent + "- None reported")
            for child in value:
                if isinstance(child, dict):
                    lines.append(indent + "- Device:")
                    render(child, indent + "  ")
                else:
                    lines.append(indent + "- " + scalar(child, indent + "  "))

    def scalar(value, continuation="  "):
        if value is None:
            return "Unavailable"
        if isinstance(value, bool):
            return "Yes" if value else "No"
        # Escape Markdown/HTML from log and device strings. Keep multiline errors.
        value = (
            str(value).replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        )
        value = re.sub(r"([\\`*_[\]{}])", r"\\\1", value)
        return value.replace("\n", "\n" + continuation) or "None reported"

    for key in (
        "software",
        "hardware",
        "monitors",
        "audio",
        "dependencies",
        "services",
        "maintenance",
        "recentErrors",
    ):
        lines.extend(["## " + label(key), ""])
        render(report[key])
        lines.append("")
    return "\n".join(lines)


def support_report(as_json=False):
    report = collect()
    return (
        json.dumps(report, indent=2, allow_nan=False) + "\n"
        if as_json
        else markdown(report)
    )
