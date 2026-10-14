"""Developer-only Niri daily controls use Rust rendering and action callbacks."""

from pathlib import Path as Path
import subprocess as subprocess
import shutil as shutil
from native_niri_actions import NativeBackend
import nacre_native_compositor as client
import os
import json
import builtins


class Backend(NativeBackend):
    night_light_service = "nacre-night-light.service"

    @property
    def capabilities(self):
        binary = client._binary()
        result = subprocess.run(
            [str(binary), "compositor", "capabilities"],
            env={**os.environ, "NACRE_COMPOSITOR": "niri"},
            capture_output=True,
            text=True,
            check=True,
        )
        return json.loads(result.stdout)["capabilities"]

    def set_desktop_options(self, preferences, *, timeout=10):
        binary = Path(
            os.environ.get("NACRE_NIRI_DESKTOP_PARITY")
            or Path(__file__).resolve().parents[3]
            / "target/debug/examples/niri-desktop-parity"
        )
        request = {
            "home": str(Path.home()),
            "loader": os.environ.get("NACRE_NIRI_CONFIG"),
            "target": os.environ.get("NACRE_NIRI_DESKTOP_CONFIG"),
            "preferences": preferences,
            "timeout": timeout,
        }
        with subprocess.Popen(
            [str(binary), json.dumps(request)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ) as process:
            try:
                while line := process.stdout.readline():
                    message = json.loads(line)
                    try:
                        if message.get("callback") == "query":
                            value = self.query("monitors", timeout=message["timeout"])
                        elif message.get("callback") == "validate":
                            subprocess.run(
                                ["niri", "validate", "-c", message["path"]],
                                check=True,
                                capture_output=True,
                                timeout=message["timeout"],
                            )
                            value = None
                        elif message.get("callback") == "reload":
                            self.reload(timeout=message["timeout"])
                            value = None
                        else:
                            result = message["result"]
                            if "error" in result:
                                raise getattr(builtins, result["type"], RuntimeError)(
                                    result["error"]
                                )
                            return result["value"]
                    except RuntimeError as error:
                        if "callback" not in message:
                            raise
                        value = {"error": str(error)}
                    process.stdin.write(json.dumps(value) + "\n")
                    process.stdin.flush()
                raise RuntimeError(process.stderr.read())
            finally:
                process.stdin.close()
                process.wait(timeout=5)

    def query(self, name, *, timeout=10):
        if name == "bindings" and "binding_source_files" in vars(self):
            binary = Path(
                os.environ.get("NACRE_NIRI_DESKTOP_PARITY")
                or Path(__file__).resolve().parents[3]
                / "target/debug/examples/niri-desktop-parity"
            )
            result = subprocess.run(
                [
                    str(binary),
                    json.dumps(
                        {
                            "operation": "binding_query",
                            "paths": [
                                str(path)
                                for path in self.binding_source_files(Path.home())
                            ],
                        }
                    ),
                ],
                capture_output=True,
                text=True,
                check=True,
            )
            return json.loads(result.stdout)["result"]["value"]
        return client._call("query", name, timeout=timeout)

    def render_desktop_options(self, preferences):
        return client.render_for_backend("niri", "render_desktop_options", preferences)

    def render_monitors(self, monitors):
        return client.render_for_backend("niri", "render_monitors", monitors)

    def render_keyboard(self, values):
        return client.render_for_backend("niri", "render_keyboard", values)

    def binding_source_files(self, home):
        return client._call("binding_source_files", home)

    def capture_bindings(self, path, kind="bindings"):
        binary = Path(
            os.environ.get("NACRE_NIRI_DESKTOP_PARITY")
            or Path(__file__).resolve().parents[3]
            / "target/debug/examples/niri-desktop-parity"
        )
        result = subprocess.run(
            [
                str(binary),
                json.dumps(
                    {"operation": "binding_capture", "paths": [str(path)], "kind": kind}
                ),
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        response = json.loads(result.stdout)["result"]
        if "error" in response:
            raise getattr(builtins, response["type"], RuntimeError)(response["error"])
        return response["value"]

    def set_night_light_temperature(self, enabled, *, timeout=1):
        return self._night_call(
            {"operation": "temperature", "enabled": enabled}, timeout
        )

    def night_light_available(self):
        installed = bool(shutil.which("wlsunset") and shutil.which("wayland-info"))
        result = (
            subprocess.run(["wayland-info"], capture_output=True, text=True, timeout=3)
            if installed
            else None
        )
        return self._night_call(
            {
                "operation": "available",
                "installed": installed,
                "code": result.returncode if result else 1,
                "stdout": result.stdout if result else "",
            },
            3,
        )

    def _night_call(self, request, timeout):
        binary = Path(
            os.environ.get("NACRE_NIRI_NIGHT_PARITY")
            or Path(__file__).resolve().parents[3]
            / "target/debug/examples/niri-night-light-parity"
        )
        with subprocess.Popen(
            [str(binary), json.dumps(request)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ) as process:
            try:
                while line := process.stdout.readline():
                    message = json.loads(line)
                    if message.get("callback") == "stop":
                        subprocess.run(
                            ["systemctl", "--user", "stop", self.night_light_service],
                            check=True,
                            capture_output=True,
                            timeout=timeout,
                        )
                        process.stdin.write("null\n")
                        process.stdin.flush()
                    else:
                        result = message["result"]
                        if "error" in result:
                            raise RuntimeError(result["error"])
                        value = result["value"]
                        if isinstance(value, dict) and set(value) == {
                            "code",
                            "stdout",
                            "stderr",
                        }:
                            return subprocess.CompletedProcess(
                                [], value["code"], value["stdout"], value["stderr"]
                            )
                        return value
                raise RuntimeError(process.stderr.read())
            finally:
                process.stdin.close()
                process.wait(timeout=5)
