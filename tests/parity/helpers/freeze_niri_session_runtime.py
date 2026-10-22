#!/usr/bin/env python3
"""Freeze current prepared-session rendering, commands and trust outcomes."""

import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from contextlib import redirect_stdout, redirect_stderr
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "uwsm/tools/niri-session-runtime.py"
SOURCE_SHA = "25689b48f666258342145a44651865f18fdd6e1e7c40a49ea51d40c77a5104f7"
DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/niri-session-runtime-reference.json"
)


def verify_source():
    if (
        not SOURCE.is_file()
        or hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA
    ):
        raise ValueError("Prepared runtime source is absent or changed")


def requests():
    desktops = [
        {},
        {"naturalScroll": False},
        {"naturalScroll": 1},
        {"gapsOut": 0, "borderSize": 0},
        {"gapsOut": 1.5, "borderSize": 2.0},
        {"gapsOut": -1},
        {"borderSize": 101},
        {"gapsOut": True},
        {"displays": [{"name": "eDP-1", "scale": 1.5}]},
        {"displays": [{"name": "DP-1", "scale": 2}, {"name": "DP-2", "scale": 1.25}]},
        {"displays": [{"name": 'DP-"\\ø', "scale": 1}]},
        {"displays": [{"name": "DP-1", "scale": 0}]},
        {"displays": [{"name": "", "scale": 1}]},
        {"displays": [{"name": "DP-1", "scale": True}]},
        {"displays": [{"disabled": True, "scale": "bad"}]},
    ]
    for index, desktop in enumerate(desktops):
        yield {
            "name": f"host-{index}",
            "operation": "host_preferences",
            "desktop": desktop,
        }
    contexts = [
        {},
        {"XDG_CURRENT_DESKTOP": "Hyprland", "NIRI_SOCKET": "/stale"},
        {
            "XDG_CURRENT_DESKTOP": "niri",
            "NIRI_SOCKET": "/private/niri.sock",
            "DISPLAY": ":fixture",
        },
        {"NIRI_SOCKET": "/private/niri.sock", "HYPRLAND_INSTANCE_SIGNATURE": "stale"},
    ]
    for index, environment in enumerate(contexts):
        for action in ("shell", "brain", "compositor", "widths", "widths-check"):
            for ready in (False, True):
                yield {
                    "name": f"command-{index}-{action}-{ready}",
                    "operation": "command",
                    "action": action,
                    "environment": environment,
                    "ready": ready,
                }
    for action in ("shell", "compositor", "widths", "widths-check"):
        for stock in (False, True):
            yield {
                "name": f"companion-{action}-{stock}",
                "operation": "command",
                "action": action,
                "environment": contexts[2],
                "ready": True,
                "companion": True,
                "stock": stock,
            }
    for live in (False, True):
        yield {"name": f"palette-{live}", "operation": "palette_include", "live": live}
    for variant in (
        "valid",
        "changed-binary",
        "no-nested-proof",
        "wrong-source",
        "wrong-protocol",
        "stock-new-version",
        "candidate-new-version",
        "binary-symlink",
        "receipt-symlink",
        "disabled",
        "not-executable",
        "packaged",
        "packaged-writable",
    ):
        yield {
            "name": f"trust-{variant}",
            "operation": "verified_hover_binary",
            "variant": variant,
        }


def child(request):
    verify_source()
    spec = importlib.util.spec_from_file_location("current_prepared_runtime", SOURCE)
    runtime = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(runtime)
    home = Path(os.environ["HOME"])
    stdout, stderr = io.StringIO(), io.StringIO()
    calls = []
    result = None
    error = None
    try:
        with redirect_stdout(stdout), redirect_stderr(stderr):
            if request["operation"] == "host_preferences":
                result = runtime.host_preferences(request["desktop"])
            elif request["operation"] == "palette_include":
                config = home / "stage/config"
                if request["live"]:
                    path = home / ".config/nacre/niri/palette.kdl"
                    path.parent.mkdir(parents=True)
                    path.write_text("palette\n")
                result = str(runtime.palette_include(home, config))
            elif request["operation"] == "command":
                shell = (
                    home
                    / ".local/share/nacre/niri-on-pc/session/home-view/.local/share/nacre/shell"
                )
                if request["ready"]:
                    for name in (
                        "source/shell.qml",
                        "tools/nacre_native_compositor.py",
                        "bin/qs",
                    ):
                        path = shell / name
                        path.parent.mkdir(parents=True, exist_ok=True)
                        path.touch()
                if request.get("stock"):
                    (shell.parents[4] / "stock-niri").write_text("stock\n")

                def companion(_home):
                    calls.append({"method": "overview_hover_binary"})
                    return Path("/companion/niri") if request.get("companion") else None

                with patch.object(
                    runtime, "overview_hover_binary", side_effect=companion
                ):
                    argv, environment = runtime.command(
                        request["action"], request["environment"], home
                    )
                    result = {"argv": argv, "environment": environment}
            else:
                variant = request["variant"]
                root = home / "companion"
                root.mkdir()
                binary = root / "niri"
                binary.write_text("verified companion\n")
                binary.chmod(0o700)
                manifest = {
                    "schema": 1,
                    "enabled": True,
                    "nestedVerified": True,
                    "sourceRevision": runtime.HOVER_SOURCE,
                    "protocol": "NacreOverviewHoverStream",
                    "binarySha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
                }
                if variant == "changed-binary":
                    binary.write_text("changed\n")
                if variant == "no-nested-proof":
                    manifest["nestedVerified"] = False
                if variant == "wrong-source":
                    manifest["sourceRevision"] = "untrusted"
                if variant == "wrong-protocol":
                    manifest["protocol"] = "Other"
                if variant == "disabled":
                    manifest["enabled"] = False
                if variant == "not-executable":
                    binary.chmod(0o600)
                receipt = root / "manifest.json"
                receipt.write_text(json.dumps(manifest))
                receipt.chmod(0o600)
                if variant == "binary-symlink":
                    binary.rename(root / "target")
                    binary.symlink_to(root / "target")
                if variant == "receipt-symlink":
                    receipt.rename(root / "receipt")
                    receipt.symlink_to(root / "receipt")
                if variant == "packaged-writable":
                    receipt.chmod(0o622)

                def version(args, **kwargs):
                    calls.append(
                        {"method": "check_output", "args": args, "kwargs": kwargs}
                    )
                    return (
                        "niri 26.12 (new)"
                        if (
                            variant == "stock-new-version"
                            and args[0] == "/usr/bin/niri"
                        )
                        or (
                            variant == "candidate-new-version"
                            and args[0] == str(binary)
                        )
                        else "niri 26.04 (fixture)"
                    )

                with patch.object(
                    runtime.subprocess, "check_output", side_effect=version
                ):
                    result = runtime.verified_hover_binary(
                        root, os.getuid(), packaged=variant.startswith("packaged")
                    )
                    result = str(result) if result else None
    except (OSError, ValueError, RuntimeError, AttributeError, TypeError) as failure:
        error = {"type": type(failure).__name__, "message": str(failure)}

    def normalize(value):
        if isinstance(value, str):
            return value.replace(str(home), "{home}")
        if isinstance(value, list):
            return [normalize(item) for item in value]
        if isinstance(value, dict):
            return {key: normalize(item) for key, item in value.items()}
        return value

    return normalize(
        {
            "value": result,
            "error": error,
            "calls": calls,
            "stdout": stdout.getvalue(),
            "stderr": stderr.getvalue(),
        }
    )


def capture(destination=DESTINATION):
    verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite prepared-runtime reference")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="nacre-runtime-reference-") as folder:
            env = dict(
                os.environ,
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_CACHE_HOME=folder + "/.cache",
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
                NACRE_COMPOSITOR="hyprland",
                NACRE_HELPER_BINARY=str(ROOT / "target/release/nacre-helper"),
            )
            for key in (
                "DISPLAY",
                "WAYLAND_DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
                "XDG_CURRENT_DESKTOP",
            ):
                env.pop(key, None)
            process = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--child",
                    json.dumps(request),
                ],
                env=env,
                capture_output=True,
                text=True,
                check=True,
            )
            cases.append(
                {
                    "request": request,
                    "observed": json.loads(process.stdout),
                    "exit": process.returncode,
                    "stderr": process.stderr,
                }
            )
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as stream:
        json.dump({"source_sha256": SOURCE_SHA, "cases": cases}, stream, indent=2)
        stream.write("\n")
    print(f"Froze {len(cases)} prepared-runtime behavior cases")


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--child":
        print(json.dumps(child(json.loads(sys.argv[2]))))
    else:
        capture()
