"""Developer-only synthetic boundary for the pre-port support-report pipeline."""

from contextlib import ExitStack, redirect_stdout
import datetime
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
CHECKED = "2026-10-01T12:00:00+00:00"


def module(name):
    spec = importlib.util.spec_from_file_location(
        name, ROOT / "nacre/shell-tools" / (name + ".py")
    )
    value = importlib.util.module_from_spec(spec)
    sys.modules[name] = value
    spec.loader.exec_module(value)
    return value


def execute(request):
    if os.environ.get("NACRE_PARITY_ROOT") != "/case" or Path.home() != Path(
        "/case/home"
    ):
        raise RuntimeError("Diagnostics reference requires the isolated parity sandbox")
    diagnostics = module("diagnostics")
    maintenance = module("maintenance")
    fixed = datetime.datetime.fromisoformat(CHECKED)

    class Clock(datetime.datetime):
        @classmethod
        def now(cls, tz=None):
            return fixed.astimezone(tz) if tz else fixed.replace(tzinfo=None)

    commands = {
        name: ["fixture-compositor", name]
        for name in (
            "runningVersion",
            "installedVersion",
            "allMonitors",
            "configErrors",
        )
    }
    rules = {tuple(value): name for name, value in commands.items()}
    rules.update(
        {
            ("fixture-python", "--version"): "python-version",
            ("quickshell", "--version"): "quickshell-version",
            ("/usr/lib/qt6/bin/qtpaths", "--qt-version"): "qt-version",
            ("pacman", "-Q"): "packages",
            ("pw-dump",): "audio",
            (
                "systemctl",
                "--user",
                "show",
                *diagnostics.NACRE_UNITS,
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
            ): "services",
            (
                "systemctl",
                "--user",
                "show",
                *maintenance.SERVICES,
                "-p",
                "Id",
                "-p",
                "ActiveState",
                "-p",
                "Result",
                "-p",
                "ExecMainStatus",
            ): "maintenance-services",
            (
                "systemctl",
                "--user",
                "--failed",
                "--no-legend",
                "--plain",
            ): "failed-services",
            (
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
                *[
                    argument
                    for unit in diagnostics.NACRE_UNITS
                    for argument in ("-u", unit)
                ],
            ): "journal",
            (
                "quickshell",
                "log",
                "--config",
                "nacre",
                "--no-color",
                "--tail",
                "60",
            ): "shell-log",
            (
                "du",
                "-s",
                "-B1",
                "/case/home/.local/state/nacre/releases",
            ): "release-size",
            (
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
            ): "ssids",
            (
                "nmcli",
                "--terse",
                "--escape",
                "no",
                "--fields",
                "NAME",
                "connection",
                "show",
            ): "connection-names",
            ("fixture-helper", "resource-state", "inventory", "/case/sys"): "gpus",
            ("test",): "test-probe",
            ("systemctl", "--user", "show"): "test-probe",
        }
    )
    for gpu in request.get("gpus", []):
        if gpu.get("id"):
            rules[("lspci", "-s", gpu["id"])] = "gpu-model"
        if gpu.get("driver"):
            rules[("modinfo", "-F", "version", gpu["driver"])] = "gpu-driver-version"
    calls = []

    def scripted(args, **kwargs):
        args = tuple(str(value) for value in args)
        if args not in rules:
            raise RuntimeError("Unscripted diagnostics reference probe")
        calls.append(
            {
                "args": list(args),
                "timeout": kwargs.get("timeout"),
                "stdin": kwargs.get("stdin"),
                "capture_output": kwargs.get("capture_output", False),
                "text": kwargs.get("text", False),
                "locale": kwargs.get("env", {}).get("LC_ALL"),
                "noColor": kwargs.get("env", {}).get("NO_COLOR"),
            }
        )
        rule = request.get("outputs", {}).get(rules[args], {"error": "missing"})
        if rules[args] == "gpus":
            rule = {"stdout": json.dumps(request.get("gpus", []))}
        if rule.get("error") == "missing":
            raise FileNotFoundError("Synthetic missing probe")
        if rule.get("error") == "timeout":
            raise subprocess.TimeoutExpired(args, kwargs.get("timeout", 3))
        stdout = rule.get("stdout", "")
        if "stdout_repeat" in rule:
            stdout = rule["stdout_repeat"]["text"] * rule["stdout_repeat"]["count"]
        return subprocess.CompletedProcess(
            args, rule.get("code", 0), stdout, rule.get("stderr", "")
        )

    def checked(args, **kwargs):
        result = scripted(args, **kwargs)
        if result.returncode:
            raise subprocess.CalledProcessError(
                result.returncode, args, output=result.stdout
            )
        return result.stdout

    def configuration_errors(timeout):
        if timeout != 5:
            raise RuntimeError("Unexpected configuration deadline")
        if request.get("config_timeout"):
            raise subprocess.TimeoutExpired("fixture-compositor", timeout)
        return request.get("config_errors", "")

    with ExitStack() as stack:
        for owner, name, value in (
            (datetime, "datetime", Clock),
            (sys, "executable", "fixture-python"),
            (diagnostics.platform, "release", lambda: "6.12.4-fixture"),
            (diagnostics.platform, "machine", lambda: "x86_64"),
            (diagnostics.os, "cpu_count", lambda: 8),
            (diagnostics.getpass, "getuser", lambda: "fixture-user"),
            (diagnostics.socket, "gethostname", lambda: "fixture-host"),
            (diagnostics.shutil, "which", lambda name: None),
            (diagnostics.subprocess, "run", scripted),
            (diagnostics.subprocess, "check_output", checked),
            (diagnostics.compositor, "diagnostic_commands", lambda: commands),
            (
                diagnostics.compositor,
                "version_command",
                lambda: commands["installedVersion"],
            ),
            (diagnostics.compositor, "backend_display_name", "Fixture"),
            (diagnostics.compositor, "config_errors", configuration_errors),
            (maintenance.time, "time", lambda: fixed.timestamp()),
        ):
            stack.enter_context(patch.object(owner, name, value))
        stack.enter_context(
            patch.dict(os.environ, {"NACRE_HELPER_BINARY": "fixture-helper"})
        )
        maintenance.HOME = Path.home()
        maintenance.STATE = Path.home() / ".local/state/nacre"
        operation = request["operation"]
        if operation == "redact":
            result = diagnostics.redact(request["value"], request.get("identities", []))
        elif operation == "hardware-fixture":
            result = {
                "hardware": diagnostics.hardware(Path("/case/sys")),
                "monitors": diagnostics.monitors(
                    {"value": json.dumps(request["monitors"])}
                ),
            }
        elif operation == "monitors":
            result = diagnostics.monitors({"value": request["value"]})
        elif operation == "audio":
            result = diagnostics.audio_devices({"value": request["value"]})
        elif operation == "probe":
            result = diagnostics.probe(request.get("args", ["test"]))
        elif operation == "maintenance":
            result = diagnostics.maintenance_facts(Path.home())
        elif operation == "maintenance-state":
            result = maintenance.state()
        elif operation == "configuration-errors":
            result = maintenance.configuration_errors()
        elif operation in (
            "collect",
            "markdown",
            "support-json",
            "support-markdown",
            "doctor-json",
            "doctor-report",
        ):
            report = diagnostics.collect(
                Path("/case/sys"), Path("/case/etc"), Path("/case/proc"), Path.home()
            )
            if operation == "collect":
                result = report
            elif operation == "markdown":
                result = diagnostics.markdown(report)
            elif operation in ("support-json", "support-markdown"):
                with patch.object(diagnostics, "collect", return_value=report):
                    result = diagnostics.support_report(operation == "support-json")
            else:
                output = io.StringIO()
                with (
                    patch.object(diagnostics, "collect", return_value=report),
                    patch.object(
                        sys,
                        "argv",
                        [
                            "maintenance.py",
                            "doctor",
                            "--json" if operation == "doctor-json" else "--report",
                        ],
                    ),
                    redirect_stdout(output),
                ):
                    maintenance.main()
                result = output.getvalue()
        else:
            raise ValueError("Unsupported diagnostics reference operation")
    return {"value": result, "calls": calls}


if __name__ == "__main__":
    print(json.dumps(execute(json.loads(sys.argv[1])), allow_nan=False))
