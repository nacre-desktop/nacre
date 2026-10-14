"""Developer-only native pearl-window boundary retaining historical tests."""

import builtins
import json
import os
from pathlib import Path
import subprocess
import sys
from types import SimpleNamespace

ROOT = Path(__file__).resolve().parents[3]
BINARY = Path(
    os.environ.get("NACRE_PEARL_WINDOW_PARITY")
    or ROOT / "target/debug/examples/pearl-window-parity"
)


class NativeWindow:
    subprocess = SimpleNamespace(run=subprocess.run)

    def read(self, kind):
        raise RuntimeError("Legacy test must supply snapshot input")

    def dispatch(self, method, window, **values):
        # The direct invalid-address assertion uses the production validator.
        result = subprocess.run(
            [
                str(BINARY),
                json.dumps({"operation": "dispatch-validation", "window": window}),
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        response = json.loads(result.stdout)["result"]
        if "error" in response:
            raise getattr(builtins, response["type"])(response["error"])

    def invoke(self, request):
        with subprocess.Popen(
            [str(BINARY), json.dumps(request)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ) as process:
            try:
                while line := process.stdout.readline():
                    message = json.loads(line)
                    if message.get("callback") == "read":
                        value = self.read(message["kind"])
                    elif message.get("callback") == "dispatch":
                        self.dispatch(
                            message["method"], message["window"], **message["values"]
                        )
                        value = None
                    else:
                        result = message["result"]
                        if "error" in result:
                            raise getattr(builtins, result["type"])(result["error"])
                        return result["value"]
                    process.stdin.write(json.dumps(value) + "\n")
                    process.stdin.flush()
                raise RuntimeError(process.stderr.read())
            finally:
                process.stdin.close()
                process.wait(timeout=5)

    def owned(self, title, pid, clients):
        return self.invoke(
            {"operation": "owned", "title": title, "pid": pid, "clients": clients}
        )

    def main(self):
        self.invoke({"operation": "execute", "argv": sys.argv[1:]})
        return 0
