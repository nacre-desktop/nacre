#!/usr/bin/env python3
"""Local maintenance facts and explicitly requested recovery actions."""

import argparse
import datetime as dt
import hashlib
import json
import subprocess
import sys
import time
from pathlib import Path

import nacre_native_compositor as compositor

HOME = Path.home()
STATE = HOME / ".local/state/nacre"
SERVICES = [
    "nacre-shell.service",
    "nacre-sidebar-ai.service",
    "nacre-brain.service",
    "nacre-session-watch.service",
    "hypridle.service",
    "nacre-brain-sync.timer",
    "xdg-desktop-portal.service",
    compositor.portal_service,
    "xdg-document-portal.service",
]


def run(args, default="", timeout=5):
    try:
        return subprocess.check_output(
            args, text=True, stderr=subprocess.DEVNULL, timeout=timeout
        ).strip()
    except (OSError, RuntimeError, subprocess.SubprocessError):
        return default


def configuration_errors():
    try:
        return compositor.config_errors(timeout=5)
    except (OSError, RuntimeError, subprocess.SubprocessError) as error:
        print(
            "nacre-helper: compositor configuration unavailable: " + str(error),
            file=sys.stderr,
        )
        return ""


def service_health():
    result = subprocess.run(
        [
            "systemctl",
            "--user",
            "show",
            *SERVICES,
            "-p",
            "Id",
            "-p",
            "ActiveState",
            "-p",
            "Result",
            "-p",
            "ExecMainStatus",
        ],
        capture_output=True,
        text=True,
        timeout=5,
    )
    # systemctl can return failure for a missing optional unit while still
    # reporting every installed unit; keep those valid results.
    output = result.stdout
    states, details = {}, {}
    for block in output.split("\n\n"):
        values = dict(line.split("=", 1) for line in block.splitlines() if "=" in line)
        name = values.get("Id")
        if not name:
            continue
        state = values.get("ActiveState", "unavailable")
        if (
            name.startswith("xdg-")
            and state == "inactive"
            and values.get("Result", "success") != "exit-code"
        ):
            state = "ready on demand"
        states[name] = state
        details[name] = values
    return {name: states.get(name, "unavailable") for name in SERVICES}, details


def state():
    release = STATE / "releases/current.json"
    managed = STATE / "configuration.json"
    drift = []
    for name, expected in (
        json.loads(managed.read_text()).items() if managed.exists() else []
    ):
        path = HOME / name
        if (
            not path.is_file()
            or hashlib.sha256(path.read_bytes()).hexdigest() != expected
        ):
            drift.append(name)
    cache = HOME / ".cache/nacre/updates.json"
    current = json.loads(release.read_text()) if release.exists() else {}
    session = STATE / "session/latest.json"
    profile = STATE / "performance/latest.json"
    versions = {
        name: run(args)
        for name, args in {
            compositor.backend_display_name: compositor.version_command(),
            "Python": [sys.executable, "--version"],
        }.items()
    }
    services, details = service_health()
    return dict(
        release=current,
        services=services,
        brainSync=json.loads((STATE / "brain-sync.json").read_text())
        if (STATE / "brain-sync.json").exists()
        else {"state": "unrecorded", "message": "No synchronization attempt recorded"},
        serviceDetails=details,
        failedServices=run(
            ["systemctl", "--user", "--failed", "--no-legend", "--plain"]
        ).splitlines(),
        drift=drift,
        configErrors=configuration_errors(),
        updatesAgeSeconds=int(time.time() - cache.stat().st_mtime)
        if cache.exists()
        else None,
        session=json.loads(session.read_text()) if session.exists() else {},
        performance=json.loads(profile.read_text()) if profile.exists() else {},
        versions=versions,
        checked=dt.datetime.now(dt.timezone.utc).isoformat(),
    )


def sample(seconds, label):
    if not 1 <= seconds <= 30:
        raise ValueError("Sample duration must be 1–30 seconds")

    def read():
        return {
            k: int(v)
            for k, v in (
                line.split("=", 1)
                for line in run(
                    [
                        "systemctl",
                        "--user",
                        "show",
                        SERVICES[0],
                        "-p",
                        "CPUUsageNSec",
                        "-p",
                        "MemoryCurrent",
                    ]
                ).splitlines()
            )
            if v.isdigit()
        }

    before = read()
    start = time.monotonic()
    time.sleep(seconds)
    after = read()
    duration = time.monotonic() - start
    value = dict(
        label=label,
        seconds=round(duration, 2),
        cpuCorePercent=round(
            (after["CPUUsageNSec"] - before["CPUUsageNSec"]) / duration / 1e7, 2
        ),
        memoryMiB=round(after["MemoryCurrent"] / 1048576, 1),
        checked=dt.datetime.now(dt.timezone.utc).isoformat(),
    )
    path = STATE / "performance/latest.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value))
    return value


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "action",
        choices=[
            "state",
            "doctor",
            "check",
            "profile",
            "restart",
            "repair-portals",
            "sync-now",
            "rollback",
            "session",
        ],
    )
    p.add_argument("--seconds", type=int, default=5)
    p.add_argument("--label", default="manual")
    output = p.add_mutually_exclusive_group()
    output.add_argument(
        "--report", action="store_true", help="Redacted Markdown support report"
    )
    output.add_argument(
        "--json", action="store_true", help="Redacted JSON support report"
    )
    a = p.parse_args()
    if (a.report or a.json) and a.action != "doctor":
        p.error("--report and --json are only valid for doctor")
    if a.action == "doctor":
        helper = Path(__file__).with_name("nacre-helper")
        if not helper.is_file():
            helper = Path(__file__).resolve().parents[2] / "target/release/nacre-helper"
        subprocess.run(
            [
                str(helper),
                "diagnostics",
                "report",
                *(["--json"] if not a.report else []),
            ],
            check=True,
        )
        return
    if a.action == "repair-portals":
        for unit in (
            "xdg-document-portal.service",
            "xdg-desktop-portal.service",
            compositor.portal_service,
        ):
            if run(["systemctl", "--user", "is-failed", unit]) == "failed":
                subprocess.run(["systemctl", "--user", "restart", unit], check=True)
    elif a.action == "sync-now":
        subprocess.run(
            [str(HOME / ".local/bin/nacre-brain-sync"), "--force"],
            check=False,
            timeout=75,
        )
    elif a.action == "restart":
        subprocess.run(["systemctl", "--user", "restart", SERVICES[0]], check=True)
    elif a.action == "rollback":
        subprocess.run([str(HOME / ".local/bin/nacre"), "rollback"], check=True)
    elif a.action == "profile":
        sample(a.seconds, a.label)
    elif a.action == "check":
        subprocess.run(
            [
                sys.executable,
                str(HOME / ".local/share/nacre/control/check-overlays.py"),
                "--keyboard",
                "/usr/bin/wtype",
            ],
            check=True,
        )
    elif a.action == "session":
        subprocess.run(
            [
                sys.executable,
                str(Path(__file__).with_name("session-watch.py")),
                "check",
                "--event",
                "manual",
            ],
            check=True,
            stdout=subprocess.DEVNULL,
        )
    print(json.dumps(state()))


if __name__ == "__main__":
    try:
        main()
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        RuntimeError,
        subprocess.SubprocessError,
    ) as e:
        if len(sys.argv) > 1 and sys.argv[1] == "doctor":
            # Collection failures must not bypass the support export's privacy
            # boundary by printing an exception containing a local path or value.
            print(
                "# Nacre support report\n\nSupport report unavailable."
                if "--report" in sys.argv
                else json.dumps(dict(error="Support report unavailable."))
            )
            sys.exit(1)
        print(json.dumps(dict(error=str(e))))
