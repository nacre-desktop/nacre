#!/usr/bin/env python3
"""Automatic travel timezone updates. The UTC clock remains NTP-owned."""

import argparse
from nacre_i18n import tr
import datetime as dt
import json
import os
import re
import shutil
import subprocess
import time
import fcntl
import contextlib
from pathlib import Path
from zoneinfo import ZoneInfo

CONFIG = Path("/etc/nacre/timezone.json")
STATE = Path("/var/lib/nacre/timezone")
PROGRAM = Path("/usr/local/libexec/nacre-timezone.py")
LOCATION = Path("/usr/local/libexec/nacre-device-location.py")
GEOCONFIG = Path("/etc/geoclue/conf.d/80-nacre-timezone.conf")
GEODESKTOP = Path("/usr/share/applications/nacre-timezone.desktop")
GEO_PERMISSION = "[nacre-timezone]\nallowed=true\nsystem=true\nusers=0\n"
GEO_DESKTOP = "[Desktop Entry]\nType=Application\nName=Nacre travel timezone\nExec=/usr/local/libexec/nacre-timezone.py update\nNoDisplay=true\nX-Geoclue-Reason=Set the local timezone while travelling\n"
SERVICE = """[Unit]
Description=Nacre automatic travel timezone
After=network-online.target
Wants=network-online.target
StartLimitIntervalSec=60
StartLimitBurst=2
[Service]
Type=oneshot
ExecStart=/usr/bin/python3 /usr/local/libexec/nacre-timezone.py update
TimeoutStartSec=30
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
PrivateTmp=yes
ReadWritePaths=/var/lib/nacre/timezone /etc/nacre
RestrictAddressFamilies=AF_INET AF_INET6 AF_UNIX
CapabilityBoundingSet=
"""
TIMER = """[Unit]
Description=Check the travel timezone every15minutes
[Timer]
OnBootSec=1min
OnCalendar=*:0/15
Persistent=true
RandomizedDelaySec=20
[Install]
WantedBy=timers.target
"""
DISPATCH = """#!/bin/sh
case "$2" in
 up|dhcp4-change|dhcp6-change|connectivity-change)
  /usr/bin/touch /var/lib/nacre/timezone/network-change
  /usr/bin/systemctl start --no-block nacre-timezone.service ;;
esac
"""


def run(*args):
    return subprocess.check_output(
        ["/usr/bin/" + args[0], *args[1:]],
        text=True,
        stderr=subprocess.PIPE,
        timeout=15,
    ).strip()


def valid_zone(zone):
    if (
        not isinstance(zone, str)
        or len(zone) > 100
        or not re.fullmatch(r"[A-Za-z0-9_+-]+(?:/[A-Za-z0-9_+-]+)*", zone)
    ):
        raise ValueError(
            tr(
                "helper.timezone.use-a-valid-iana-timezone-name",
                "Use a valid IANA timezone name",
            )
        )
    ZoneInfo(zone)
    return zone


def read(path, default):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return default


def atomic(path, data, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    temp = path.with_name(path.name + ".next")
    temp.write_text(json.dumps(data, indent=2) + "\n")
    temp.chmod(mode)
    temp.replace(path)


def state():
    zone = run("timedatectl", "show", "--property=Timezone", "--value")
    data = read(STATE / "status.json", {})
    return dict(
        installed=PROGRAM.exists(),
        installedProgram=str(PROGRAM),
        automatic=read(CONFIG, {}).get("automatic", False),
        confirmedTimezone=read(CONFIG, {}).get("confirmedTimezone", ""),
        deviceLocation=LOCATION.exists() and GEOCONFIG.exists(),
        timezone=zone,
        localTime=dt.datetime.now(ZoneInfo(zone)).strftime("%A, %d %B · %H:%M %Z"),
        **{
            k: v
            for k, v in data.items()
            if k in ("checked", "detected", "error", "source", "city", "accuracyMeters")
        },
    )


@contextlib.contextmanager
def operation_lock():
    STATE.mkdir(parents=True, exist_ok=True)
    with (STATE / "operation.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        yield


def device_detect():
    result = subprocess.run(
        ["/usr/bin/python3", str(LOCATION)],
        capture_output=True,
        text=True,
        timeout=16,
        check=True,
    )
    data = json.loads(result.stdout)
    data["timezone"] = valid_zone(data["timezone"])
    return data


def update(force=False):
    with operation_lock():
        return update_locked(force)


def update_locked(force=False):
    config = read(CONFIG, {})
    if not config.get("automatic", False):
        return
    old = read(STATE / "status.json", {})
    network_change = STATE / "network-change"
    changed = network_change.exists() and network_change.stat().st_mtime > old.get(
        "epoch", 0
    )
    if not force and not changed and time.time() - old.get("epoch", 0) < 300:
        return
    try:
        fix = device_detect()
        zone = fix["timezone"]
        current = run("timedatectl", "show", "--property=Timezone", "--value")
        if current != zone:
            run("timedatectl", "set-timezone", zone)
        # Last trustworthy zone becomes the fallback; precise coordinates are never stored.
        config["lastTrustedTimezone"] = zone
        atomic(CONFIG, config)
        atomic(
            STATE / "status.json",
            dict(
                epoch=time.time(),
                checked=dt.datetime.now(dt.timezone.utc).isoformat(),
                detected=zone,
                source=fix["source"],
                city=fix.get("city", ""),
                accuracyMeters=fix.get("accuracyMeters"),
                error="",
            ),
        )
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        RuntimeError,
        subprocess.SubprocessError,
    ):
        current = run("timedatectl", "show", "--property=Timezone", "--value")
        fallback = (
            config.get("lastTrustedTimezone")
            or config.get("confirmedTimezone")
            or current
        )
        zone = valid_zone(fallback)
        if current != zone:
            run("timedatectl", "set-timezone", zone)
        atomic(
            STATE / "status.json",
            dict(
                epoch=time.time(),
                checked=dt.datetime.now(dt.timezone.utc).isoformat(),
                detected=zone,
                source="Last confirmed timezone",
                error=tr(
                    "helper.timezone.device-location-unavailable-or-imprecise-kept-the-last-confirmed",
                    "Device location unavailable or imprecise; kept the last confirmed timezone. IP location is not used to change the clock.",
                ),
            ),
        )


def confirm(zone):
    require_root()
    zone = valid_zone(zone)
    with operation_lock():
        config = read(CONFIG, {})
        config.update(confirmedTimezone=zone, lastTrustedTimezone=zone)
        atomic(CONFIG, config)
        run("timedatectl", "set-timezone", zone)
        atomic(
            STATE / "status.json",
            dict(
                epoch=time.time(),
                checked=dt.datetime.now(dt.timezone.utc).isoformat(),
                detected=zone,
                source="Confirmed location",
                error="",
            ),
        )


def require_root():
    if os.geteuid() != 0:
        raise PermissionError("Administrator authentication is required")


def install(zone=None):
    require_root()
    with operation_lock():
        return install_locked(zone)


def install_locked(zone=None):
    require_root()
    STATE.mkdir(parents=True, exist_ok=True)
    zone = zone or None
    if zone:
        valid_zone(zone)
    backup = STATE / (
        "backup-" + dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%SZ")
    )
    backup.mkdir(mode=0o700)
    paths = [
        PROGRAM,
        CONFIG,
        Path("/etc/systemd/system/nacre-timezone.service"),
        Path("/etc/systemd/system/nacre-timezone.timer"),
        Path("/etc/NetworkManager/dispatcher.d/80-nacre-timezone"),
        LOCATION,
        GEOCONFIG,
        GEODESKTOP,
    ]
    entries = []
    for index, path in enumerate(paths):
        entry = {"path": str(path), "existed": path.exists()}
        if path.exists():
            shutil.copy2(path, backup / str(index))
            entry["backup"] = str(backup / str(index))
        entries.append(entry)
    atomic(
        backup / "manifest.json",
        dict(
            previousTimezone=run(
                "timedatectl", "show", "--property=Timezone", "--value"
            ),
            entries=entries,
        ),
        0o600,
    )
    PROGRAM.parent.mkdir(parents=True, exist_ok=True)
    if Path(__file__).resolve() != PROGRAM.resolve():
        shutil.copyfile(Path(__file__).resolve(), PROGRAM)
    PROGRAM.chmod(0o755)
    for path, body in [(paths[2], SERVICE), (paths[3], TIMER), (paths[4], DISPATCH)]:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(body)
        path.chmod(0o755 if path == paths[4] else 0o644)
    LOCATION.parent.mkdir(parents=True, exist_ok=True)
    location_source = Path(__file__).with_name("device-location.py")
    if not location_source.exists():
        location_source = LOCATION
    if location_source.resolve() != LOCATION.resolve():
        shutil.copyfile(location_source, LOCATION)
    LOCATION.chmod(0o755)
    GEOCONFIG.parent.mkdir(parents=True, exist_ok=True)
    GEOCONFIG.write_text(GEO_PERMISSION)
    GEODESKTOP.parent.mkdir(parents=True, exist_ok=True)
    GEODESKTOP.write_text(GEO_DESKTOP)
    config = read(CONFIG, {})
    config["automatic"] = True
    if zone:
        config.update(confirmedTimezone=zone, lastTrustedTimezone=zone)
    atomic(CONFIG, config)
    if zone:
        run("timedatectl", "set-timezone", zone)
    run("systemctl", "try-restart", "geoclue.service")
    run("systemctl", "daemon-reload")
    run("systemctl", "enable", "--now", "nacre-timezone.timer")
    update_locked(force=True)


def automatic(value):
    require_root()
    if value not in ("on", "off"):
        raise ValueError(tr("helper.timezone.choose-on-or-off", "Choose on or off"))
    with operation_lock():
        config = read(CONFIG, {})
        config["automatic"] = value == "on"
        atomic(CONFIG, config)
        run(
            "systemctl",
            "enable" if value == "on" else "disable",
            "--now",
            "nacre-timezone.timer",
        )
        if value == "on":
            update_locked(force=True)


def manual(zone):
    require_root()
    zone = valid_zone(zone)
    with operation_lock():
        config = read(CONFIG, {})
        config.update(automatic=False, confirmedTimezone=zone, lastTrustedTimezone=zone)
        atomic(CONFIG, config)
        run("systemctl", "disable", "--now", "nacre-timezone.timer")
        run("timedatectl", "set-timezone", zone)
        atomic(
            STATE / "status.json",
            dict(source="Manual timezone", detected=zone, error=""),
        )


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "action",
        choices=["state", "install", "update", "automatic", "manual", "confirm"],
    )
    p.add_argument("value", nargs="?")
    a = p.parse_args()
    try:
        if a.action == "install":
            install(a.value)
        elif a.action == "confirm":
            confirm(a.value)
        elif a.action == "update":
            require_root()
            if a.value not in (None, "force"):
                raise ValueError(
                    tr(
                        "helper.timezone.unsupported-location-refresh",
                        "Unsupported location refresh",
                    )
                )
            update(force=a.value == "force")
        elif a.action == "automatic":
            automatic(a.value)
        elif a.action == "manual":
            manual(a.value)
        print(json.dumps(state()))
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        RuntimeError,
        subprocess.SubprocessError,
    ) as e:
        print(json.dumps(dict(error=str(e))))
        raise SystemExit(1) from None


if __name__ == "__main__":
    main()
