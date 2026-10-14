#!/usr/bin/env python3
"""Freeze current Python request framing/errors on private socket pairs."""

import argparse
import importlib.util
import json
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
import threading
from unittest.mock import patch

import freeze_niri_state as reference

DESTINATION = (
    reference.ROOT / "crates/nacre-helper/tests/fixtures/niri-transport-reference.json"
)


def requests():
    frames = (
        ("handled", '{"Ok":"Handled"}\n', 64),
        ("u64", '{"Ok":{"FocusedWindow":{"id":18446744073709551615}}}\n', 128),
        ("ipc-error", '{"Err":"window no longer exists"}\n', 64),
        ("empty", "", 64),
        ("truncated", '{"Ok":"Handled"}', 64),
        ("oversized", '{"Ok":"' + "x" * 100 + '"}\n', 32),
        ("non-object", "[]\n", 64),
        ("two-statuses", '{"Ok":"Handled","Err":"bad"}\n', 64),
        ("missing-status", '{"Unknown":"Handled"}\n', 64),
        ("null-result", '{"Ok":null}\n', 64),
        ("malformed-json", "invalid\n", 64),
        ("event-after-reply", '{"Ok":"Handled"}\n{"FutureEvent":null}\n', 128),
    )
    for name, reply, maximum in frames:
        yield {
            "name": name,
            "reply": reply,
            "max_bytes": maximum,
            "request": {"Action": {"CloseWindow": {"id": 9007199254740995}}},
        }


def child(request):
    reference.verify_source()
    spec = importlib.util.spec_from_file_location(
        "current_niri_transport", reference.SOURCE
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    client, server = socket.socketpair()
    wire = []

    def reply():
        with server, server.makefile("rb") as stream:
            wire.append(stream.readline().hex())
            data = request["reply"].encode()
            server.sendall(data[:4])
            server.sendall(data[4:])

    worker = threading.Thread(target=reply)
    worker.start()
    backend = module.Backend()
    with patch.object(backend, "_connection", return_value=client):
        try:
            result = backend._request(
                request["request"], timeout=1, max_bytes=request["max_bytes"]
            )
            outcome, code = result, 0
        except (ValueError, RuntimeError, OSError) as error:
            outcome, code = {"error": str(error), "type": type(error).__name__}, 1
    worker.join(timeout=2)
    if worker.is_alive():
        raise RuntimeError("Private reference peer did not finish")
    print(
        json.dumps(
            {
                "stdout": json.dumps(outcome, allow_nan=False) + "\n",
                "exit": code,
                "wire_hex": wire[0],
                "connection_closed": client.fileno() == -1,
            }
        )
    )
    return 0


def capture(destination):
    reference.verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen niri transport outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="niri-transport-reference-") as folder:
            env = {
                "HOME": folder,
                "PATH": "",
                "PYTHONDONTWRITEBYTECODE": "1",
                "XDG_CONFIG_HOME": folder + "/.config",
                "XDG_STATE_HOME": folder + "/.local/state",
                "XDG_CACHE_HOME": folder + "/.cache",
            }
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(__file__).resolve()),
                    "--child",
                    json.dumps(request),
                ],
                env=env,
                stdin=subprocess.DEVNULL,
                capture_output=True,
                text=True,
                timeout=5,
                check=False,
            )
            if result.returncode or result.stderr or list(Path(folder).rglob("*")):
                raise ValueError(
                    "Unexpected niri transport reference effect: " + request["name"]
                )
            cases.append(
                {
                    "request": request,
                    "observed": json.loads(result.stdout),
                    "stderr": "",
                    "files": {},
                }
            )
    reference.verify_source()
    destination.parent.mkdir(parents=True, exist_ok=True)
    with destination.open("x") as output:
        output.write(
            json.dumps(
                {"source_sha256": reference.SOURCE_SHA, "cases": cases}, indent=2
            )
            + "\n"
        )
    return len(cases)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=DESTINATION)
    parser.add_argument("--child")
    args = parser.parse_args()
    if args.child:
        return child(json.loads(args.child))
    print("Frozen niri transport cases:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
