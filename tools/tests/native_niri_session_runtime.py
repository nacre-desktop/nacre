"""Developer provider mapping preserving the historical runtime assertions."""

import json
import math
import os
from pathlib import Path
import select
import subprocess

ROOT = Path(__file__).resolve().parents[2]
HOVER_SOURCE = "8ed0da44d974c32c6877d2f4630c314da0717ecb"
PACKAGED_HOVER = Path("/usr/lib/nacre/niri")


def wire(value):
    # JSON has no NaN/Infinity. Null retains the validator's rejection branch;
    # this is an invalid input provider mapping, never an expected-output replay.
    if isinstance(value, float) and not math.isfinite(value):
        return None
    if isinstance(value, dict):
        return {key: wire(item) for key, item in value.items()}
    if isinstance(value, list):
        return [wire(item) for item in value]
    return value


def invoke(request):
    binary = os.environ.get("NACRE_SESSION_RUNTIME_PARITY") or str(
        ROOT / "target/debug/examples/niri-session-runtime-parity"
    )
    process = subprocess.Popen(
        [binary, json.dumps(wire(request), allow_nan=False)],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        text=True,
    )
    try:
        while True:
            if not select.select([process.stdout], [], [], 10)[0]:
                raise RuntimeError("Native runtime provider timed out")
            line = process.stdout.readline()
            if not line:
                raise RuntimeError("Native runtime provider exited without a result")
            message = json.loads(line)
            if "callback" not in message:
                process.wait(timeout=10)
                if "error" in message:
                    raise ValueError(message["error"])
                return message["result"]
            provider = message["callback"]
            try:
                operation = provider["operation"]
                if operation == "version":
                    result = subprocess.check_output(
                        [provider["binary"], "--version"], text=True, timeout=2
                    )
                elif operation == "verify":
                    result = verified_hover_binary(
                        Path(provider["root"]),
                        provider["owner"],
                        packaged=provider["packaged"],
                    )
                elif operation == "companion":
                    result = overview_hover_binary(Path(provider["home"]))
                elif operation == "build":
                    subprocess.run(
                        [
                            str(ROOT / "target/release/nacre-helper"),
                            "build-niri-overview-hover",
                            "--backend",
                            provider["backend"],
                            "--install",
                        ],
                        check=True,
                    )
                    result = None
                else:
                    raise ValueError("Unknown provider operation")
                reply = {"value": str(result) if isinstance(result, Path) else result}
            except (OSError, ValueError, subprocess.SubprocessError) as failure:
                reply = {"error": str(failure)}
            process.stdin.write(json.dumps(reply) + "\n")
            process.stdin.flush()
    finally:
        if process.poll() is None:
            process.terminate()
            process.wait(timeout=5)
        process.stdin.close()
        process.stdout.close()


def command(action, environment=None, home=None):
    argv, environment = invoke(
        {
            "operation": "command",
            "action": action,
            "environment": dict(os.environ if environment is None else environment),
            "home": str(Path.home() if home is None else home),
        }
    )
    return argv, environment


def host_preferences(desktop):
    return invoke({"operation": "host_preferences", "desktop": desktop})


def palette_include(home, config):
    return Path(
        invoke(
            {"operation": "palette_include", "home": str(home), "config": str(config)}
        )
    )


def verified_hover_binary(root, owner, *, packaged=False):
    try:
        result = invoke(
            {
                "operation": "verified_hover_binary",
                "root": str(root),
                "owner": owner,
                "packaged": packaged,
            }
        )
    except ValueError:
        return None
    return Path(result) if result else None


def overview_hover_binary(home):
    result = invoke({"operation": "overview_hover_binary", "home": str(home)})
    return Path(result) if result else None


def ensure_overview_hover(backend, home):
    try:
        invoke(
            {
                "operation": "ensure_overview_hover",
                "backend": str(backend),
                "home": str(home),
            }
        )
    except ValueError as error:
        raise RuntimeError(str(error)) from error
