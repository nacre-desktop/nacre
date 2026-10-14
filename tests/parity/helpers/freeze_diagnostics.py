"""Freeze bounded synthetic inputs for the current support-report implementation."""

import copy
import datetime
import hashlib
import json

import run

SOURCES = {
    "diagnostics.py": "0176fe8eca21315c075f1aabbe6d3be48dd20fadf594fc8f0c0fa73b08c3a9fd",
    "maintenance.py": "ef0a0d85ea966a69a3cb5f83c53cbef6e2c89a31c1175c143f2ea53fe77a0ae3",
}


def complete():
    units = "Id=nacre-shell.service\nLoadState=loaded\nActiveState=active\nResult=success\nExecMainStatus=0\n\nId=xdg-desktop-portal.service\nActiveState=inactive\nResult=success\nExecMainStatus=0"
    outputs = {
        "runningVersion": {"stdout": "Fixture running version"},
        "installedVersion": {"stdout": "Fixture installed version"},
        "quickshell-version": {"stdout": "Quickshell 0.3.2"},
        "qt-version": {"stdout": "6.10.0"},
        "python-version": {"stdout": "Python 3.14.7"},
        "packages": {"stdout": "python 3.14\nquickshell 0.3.2\nbroken row ignored\n"},
        "allMonitors": {
            "stdout": json.dumps(
                [
                    {
                        "name": "DP-1",
                        "width": 3840,
                        "height": 2160,
                        "scale": 2,
                        "refreshRate": 60,
                        "serial": "synthetic serial",
                    },
                    {
                        "name": "DP-2",
                        "width": 1920,
                        "height": 1080,
                        "scale": 1.25,
                        "refreshRate": 144,
                        "disabled": False,
                    },
                ]
            )
        },
        "audio": {
            "stdout": json.dumps(
                [
                    {
                        "info": {
                            "props": {
                                "media.class": "Audio/Sink",
                                "node.description": "fixture-user Headphones",
                                "device.api": "alsa",
                                "device.serial": "synthetic serial",
                            },
                            "state": "running",
                        }
                    }
                ]
            )
        },
        "services": {
            "stdout": units,
            "code": 1,
            "stderr": "Synthetic private service error",
        },
        "maintenance-services": {"stdout": units, "code": 1},
        "failed-services": {"stdout": "fixture.service failed\n"},
        "journal": {
            "stdout": "fixture-user@fixture-host /case/home/file 192.0.2.1\nTOKEN=synthetic-short\n<warning>*literal*\n"
        },
        "shell-log": {"stdout": "SSID=Fixture Network\nGPU 0% / 42 C kernel 6.12.4\n"},
        "configErrors": {"stdout": "bad <option> at /case/home/config"},
        "release-size": {"stdout": "12345 /case/home/.local/state/nacre/releases"},
        "ssids": {"stdout": "Fixture Network\n"},
        "connection-names": {"stdout": "Fixture Connection\n"},
        "gpu-model": {"stdout": "Fixture GPU model"},
        "gpu-driver-version": {"stdout": ""},
    }
    request = {
        "operation": "collect",
        "outputs": outputs,
        "config_errors": "bad option",
        "gpus": [
            {
                "id": "0000:01:00.0",
                "name": "GPU 0",
                "vendor": "nvidia",
                "driver": "fixture_driver",
                "deviceId": "0x1234",
            }
        ],
    }
    epoch = datetime.datetime.fromisoformat("2026-10-01T12:00:00+00:00").timestamp()
    files = {
        "etc/os-release": 'ID=fixture\nVERSION_ID="1"\nPRETTY_NAME="Fixture Linux"',
        "etc/hostname": "fixture-host\n",
        "proc/cpuinfo": "model name : Fixture CPU\n",
        "sys/class/dmi/id/chassis_type": "9",
        "sys/class/power_supply/BAT0/type": "Battery",
        "sys/class/backlight/panel": {"directory": True},
        "sys/class/bluetooth/hci0": {"directory": True},
        "sys/class/net/wlan0/wireless": {"directory": True},
        "sys/class/net/eth0/type": "1",
        "home/.local/state/nacre/releases/current.json": json.dumps(
            {
                "revision": "fixture-revision",
                "status": "ready",
                "checked": "fixed release check",
                "label": "omit arbitrary profile label",
            }
        ),
        "home/.local/state/nacre/configuration.json": json.dumps(
            {
                ".config/managed.txt": hashlib.sha256(b"expected").hexdigest(),
                ".config/missing.txt": "missing",
            }
        ),
        "home/.config/managed.txt": "changed",
        "home/.cache/nacre/updates.json": {"text": "{}", "mtime": epoch - 3600},
        "home/.local/state/nacre/brain-sync.json": '{"privateLabel":"not exported"}',
        "home/.local/state/nacre/session/latest.json": '{"privateSession":"not exported"}',
        "home/.local/state/nacre/performance/latest.json": '{"privateProfile":"not exported"}',
    }
    return request, files


def cases():
    full, files = complete()
    for path in sorted(
        (run.ROOT / "nacre/shell-tools/tests/fixtures/hardware").glob("*.json")
    ):
        fixture = json.loads(path.read_text())
        yield (
            "support-hardware-" + path.stem,
            {"operation": "hardware-fixture", "monitors": fixture["monitors"]},
            {"sys/" + name: value for name, value in fixture["files"].items()},
        )
    for operation in (
        "collect",
        "markdown",
        "support-json",
        "support-markdown",
        "maintenance",
        "maintenance-state",
        "doctor-json",
        "doctor-report",
    ):
        request = copy.deepcopy(full)
        request["operation"] = operation
        yield "support-" + operation, request, copy.deepcopy(files)
    yield "support-unavailable", {"operation": "collect"}, {}
    yield "support-monitors-object", {"operation": "monitors", "value": "{}"}, {}
    malformed = copy.deepcopy(full)
    malformed["outputs"]["allMonitors"] = {
        "stdout": '[{"name":"DP-1","scale":NaN,"width":"bad","height":true}]'
    }
    malformed["outputs"]["audio"] = {
        "stdout": '[{"info":"bad"},{"info":{"props":null}},{}]'
    }
    yield "support-malformed-devices", malformed, files
    yield (
        "support-corrupt-maintenance",
        full,
        {**files, "home/.local/state/nacre/configuration.json": "not json"},
    )
    for label, rule in [
        ("ok", {"stdout": "value", "stderr": "synthetic-private"}),
        ("missing", {"error": "missing"}),
        ("timeout", {"error": "timeout"}),
        (
            "nonunit-error",
            {
                "code": 1,
                "stdout": "Id=example\nActiveState=active",
                "stderr": "synthetic-private",
            },
        ),
        ("oversized", {"stdout_repeat": {"text": "x", "count": 2 * 1024 * 1024 + 1}}),
    ]:
        yield (
            "support-probe-" + label,
            {"operation": "probe", "outputs": {"test-probe": rule}},
            {},
        )
    yield (
        "support-probe-partial-unit",
        {
            "operation": "probe",
            "args": ["systemctl", "--user", "show"],
            "outputs": {
                "test-probe": {
                    "code": 1,
                    "stdout": "Id=nacre-shell.service\nActiveState=active",
                    "stderr": "synthetic-private",
                }
            },
        },
        {},
    )
    for label, request in [
        (
            "redact-full",
            {
                "operation": "redact",
                "value": {
                    "username": "fixture-user",
                    "hostname": "fixture-host",
                    "SSID": "Fixture Network",
                    "accessToken": "fake-access",
                    "private_key": "fake-key",
                    "message": "fixture-user@fixture-host /home/fixture-user/file /Users/fixture-other/file 192.0.2.1 2001:db8::42 fe80::2%eth0 AA:BB:CC:DD:EE:FF\nFixture Network\nBearer a.short.synthetic\npassword=two synthetic words\nhttps://demo:fake@example.invalid/?code=synthetic\nabcDEF0123456789abcdefghijklmnop",
                    "nested": [{"/home/fixture-user/key": "fixture-host"}],
                },
                "identities": ["fixture-user", "fixture-host", "Fixture Network"],
            },
        ),
        (
            "redact-nested",
            {
                "operation": "redact",
                "value": {
                    "username": "fixture-user",
                    "SSID": "Fixture Network",
                    "accessToken": "synthetic-short",
                    "nested": [{"/case/home/key": "fixture-host"}],
                    "message": "fixture-user@fixture-host 192.0.2.1 2001:db8::42 fe80::2%eth0 AA:BB:CC:DD:EE:FF\nFixture Network\npassword=synthetic words\nhttps://example.invalid/a",
                },
                "identities": [
                    "fixture-user",
                    "fixture-host",
                    "Fixture Network",
                    "/case/home",
                ],
            },
        ),
        (
            "redact-short",
            {
                "operation": "redact",
                "value": "API_KEY=short-synthetic\nclientSecret: sample\ncookie='synthetic-cookie'\npostgres://demo:fake@db.invalid/test",
            },
        ),
        (
            "redact-public",
            {"operation": "redact", "value": "GPU 0% / 42 C kernel 6.12.4"},
        ),
        (
            "config-error",
            {"operation": "configuration-errors", "config_errors": "bad option"},
        ),
        (
            "config-timeout",
            {"operation": "configuration-errors", "config_timeout": True},
        ),
    ]:
        yield "support-" + label, request, {}


def capture():
    for name, digest in SOURCES.items():
        path = run.ROOT / "nacre/shell-tools" / name
        if (
            not path.is_file()
            or hashlib.sha256(path.read_bytes()).hexdigest() != digest
        ):
            raise ValueError(
                "Current Python diagnostics/maintenance source changed; refusing capture"
            )
    entries = list(cases())
    folder = run.HERE / "diagnostics"
    if any(
        (folder / f"{prefix}-{label}.json").exists()
        for label, _, _ in entries
        for prefix in ("case", "expected")
    ):
        raise ValueError("Reference destination exists; refusing overwrite")
    snapshots = []
    for label, request, files in entries:
        case = {
            "helper": "diagnostics",
            "command": "python3 tests/parity/helpers/diagnostics_reference.py",
            "args": [json.dumps(request)],
            "fixtures": files,
            "reference_source_sha256": SOURCES,
        }
        result = run.execute(case, None)
        if result["exit"] or result["files"] or result["deleted"]:
            raise ValueError("Synthetic reference failed or wrote files: " + label)
        snapshots.append((label, case, result))
    for label, case, result in snapshots:
        for prefix, data in [("case", case), ("expected", result)]:
            with (folder / f"{prefix}-{label}.json").open("x") as output:
                output.write(json.dumps(data, indent=2, ensure_ascii=True) + "\n")
    return len(snapshots)


if __name__ == "__main__":
    print(
        f"Captured {capture()} synthetic diagnostics boundaries; no live collection or writes"
    )
