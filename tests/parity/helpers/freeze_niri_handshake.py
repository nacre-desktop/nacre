#!/usr/bin/env python3
"""Freeze hover negotiation/fallback and retained events before porting."""

import argparse
import importlib.util
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import tempfile
from unittest.mock import patch

import freeze_niri_state as reference

DESTINATION = (
    reference.ROOT / "crates/nacre-helper/tests/fixtures/niri-handshake-reference.json"
)
EVENT = '{"OverviewOpenedOrClosed":{"is_open":true}}\n'


def requests():
    for name, replies, hover, maximum in (
        ("hover-accepted", ['{"Ok":"Handled"}\n' + EVENT], True, 256),
        (
            "stock-fallback",
            ['{"Err":"unknown request"}\n', '{"Ok":"Handled"}\n' + EVENT],
            True,
            256,
        ),
        ("hover-disabled", ['{"Ok":"Handled"}\n' + EVENT], False, 256),
        (
            "unexpected-ok-fallback",
            ['{"Ok":"Unexpected"}\n', '{"Ok":"Handled"}\n' + EVENT],
            True,
            256,
        ),
        ("stock-rejected", ['{"Err":"unsupported"}\n'], False, 256),
        ("bad-json", ["invalid\n"], True, 256),
        ("truncated", ['{"Ok":'], True, 256),
        ("oversized", ['{"Ok":"' + "x" * 60 + '"}\n'], True, 32),
    ):
        yield {"name": name, "replies": replies, "hover": hover, "max_bytes": maximum}


def child(request):
    reference.verify_source()
    spec = importlib.util.spec_from_file_location(
        "current_niri_handshake", reference.SOURCE
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    clients, servers = [], []
    for reply in request["replies"]:
        client, server = socket.socketpair()
        server.sendall(reply.encode())
        server.shutdown(socket.SHUT_WR)
        clients.append(client)
        servers.append(server)
    backend = module.Backend()
    selected, error, remaining, blocking = None, None, "", None
    with (
        patch.dict(
            os.environ,
            {"NACRE_NIRI_HOVER_STREAM": "1" if request["hover"] else "0"},
            clear=True,
        ),
        patch.object(module, "MAX_BYTES", request["max_bytes"]),
        patch.object(backend, "_connection", side_effect=clients),
    ):
        try:
            connection = backend.event_socket()
            selected = clients.index(connection)
            blocking = connection.getblocking()
            remaining = connection.recv(4096).hex()
        except (ValueError, RuntimeError, OSError) as failure:
            error = {"type": type(failure).__name__, "error": str(failure)}
    wire = []
    for server in servers:
        server.settimeout(0.1)
        wire.append(server.recv(4096).hex())
    observed = {
        "selected": selected,
        "error": error,
        "remaining_hex": remaining,
        "blocking": blocking,
        "wire_hex": wire,
        "closed": [client.fileno() == -1 for client in clients],
    }
    for client in clients:
        client.close()
    for server in servers:
        server.close()
    print(json.dumps(observed))
    return 0


def capture(destination):
    reference.verify_source()
    if destination.exists():
        raise ValueError("Refusing to overwrite frozen niri handshake outputs")
    cases = []
    for request in requests():
        with tempfile.TemporaryDirectory(prefix="niri-handshake-reference-") as folder:
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
                    "Unexpected handshake capture effect: " + request["name"]
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
    print("Frozen niri handshakes:", capture(args.output))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
