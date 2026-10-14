"""Developer-only callback mapping; hide/restore and file writes run in Rust."""

import builtins
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[2]
BINARY = Path(
    os.environ.get("NACRE_WINDOW_HIDE_PARITY")
    or ROOT / "target/debug/examples/window-hide-parity"
)
HIDDEN = "special:hidden"


def compositor():
    return Service()


class Service:
    def valid_window_id(self, value):
        return invoke({"operation": "valid", "id": value})


def query(name):
    return invoke({"operation": "query", "name": name})


def dispatch(method, window, **options):
    return invoke(
        {"operation": "dispatch", "method": method, "window": window, "values": options}
    )


def notify(message):
    raise RuntimeError("Test must supply notification input")


def invoke(request):
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
                kind = message.get("callback")
                if kind is None:
                    result = message["result"]
                    if "error" in result:
                        raise getattr(builtins, result["type"])(result["error"])
                    return result["value"]
                try:
                    if kind == "valid":
                        value = bool(compositor().valid_window_id(message["id"]))
                    elif kind == "query":
                        value = query(message["name"])
                    elif kind == "dispatch":
                        dispatch(
                            message["method"], message["window"], **message["values"]
                        )
                        value = None
                    elif kind == "notify":
                        notify(message["message"])
                        value = None
                    elif kind == "service-query":
                        value = compositor().query(
                            message["name"],
                            timeout=message["timeout"],
                            max_bytes=message["max_bytes"],
                        )
                    elif kind == "service":
                        result = getattr(compositor(), message["name"])(
                            message["window"], **message["kwargs"]
                        )
                        value = {"stdout": result.stdout}
                    else:
                        raise RuntimeError("Unknown callback")
                    response = {"value": value}
                except (
                    OSError,
                    RuntimeError,
                    ValueError,
                    subprocess.SubprocessError,
                ) as error:
                    response = {"error": str(error)}
                process.stdin.write(json.dumps(response) + "\n")
                process.stdin.flush()
            raise RuntimeError(process.stderr.read())
        finally:
            process.stdin.close()
            process.wait(timeout=5)


def action(name, folder=None, window=None, quiet=False):
    return invoke(
        {
            "operation": "action",
            "name": name,
            "folder": str(folder),
            "window": window,
            "quiet": quiet,
        }
    )


def destination(workspace):
    return invoke({"operation": "destination", "workspace": workspace})


def records(folder, clients):
    return invoke({"operation": "records", "folder": str(folder), "clients": clients})
