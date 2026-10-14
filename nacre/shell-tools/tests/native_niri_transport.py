#!/usr/bin/env python3
"""Original socket/stream assertions against native code; private sockets only."""

import builtins
import json
import os
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
BINARY = Path(
    os.environ.get("NACRE_NIRI_TRANSPORT_PARITY")
    or ROOT / "target/debug/examples/niri-transport-parity"
)


def error(message):
    if "error" in message:
        raise getattr(builtins, message["type"], RuntimeError)(message["error"])


class NativeBackend:
    def _connection(self):
        raise RuntimeError("Test must supply a private socket")

    def _response(self, name):
        raise RuntimeError("Test must supply output metadata")

    def exchange(self, request, sockets):
        descriptors = tuple(sock.fileno() for sock in sockets)
        return subprocess.Popen(
            [str(BINARY), json.dumps(request)],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            pass_fds=descriptors,
            text=True,
        )

    def close_window(self, window):
        connection = self._connection()
        try:
            with self.exchange(
                {
                    "mode": "request",
                    "fd": connection.fileno(),
                    "request": {
                        "Action": {
                            "CloseWindow": {
                                "id": int(window.removeprefix("niri-window:"))
                            }
                        }
                    },
                },
                [connection],
            ) as process:
                stdout, stderr = process.communicate(timeout=10)
                if process.returncode or stderr:
                    raise RuntimeError(stderr)
                error(json.loads(stdout))
        finally:
            connection.close()

    def event_socket(self):
        # Retain both supplied socket identities for the original assertIs.
        connections = list(self._connection.side_effect)
        with self.exchange(
            {
                "mode": "handshake",
                "fds": [sock.fileno() for sock in connections],
                "hover": os.environ.get("NACRE_NIRI_HOVER_STREAM", "1") != "0",
            },
            connections,
        ) as process:
            stdout, stderr = process.communicate(timeout=10)
            if process.returncode or stderr:
                raise RuntimeError(stderr)
            response = json.loads(stdout)
            error(response)
        # Rust owns and consumes the inherited stream. Restore only the first
        # event bytes at the test socket boundary; no backend logic is replayed.
        selected = next(
            index
            for index, sock in enumerate(connections)
            if sock.fileno() == response["selected_fd"]
        )
        selected_socket = connections[selected]
        from socket import socketpair

        client, server = socketpair()
        try:
            server.sendall(bytes(response["remaining"]))
            os.dup2(client.fileno(), selected_socket.fileno())
        finally:
            client.close()
            server.close()
        for connection in connections[:selected]:
            connection.close()
        return selected_socket

    def events(self):
        connection = self.event_socket()
        try:
            with self.exchange(
                {"mode": "events", "fd": connection.fileno()}, [connection]
            ) as process:
                try:
                    while line := process.stdout.readline():
                        response = json.loads(line)
                        if response.get("callback") == "outputs":
                            process.stdin.write(
                                json.dumps(self._response("Outputs")) + "\n"
                            )
                            process.stdin.flush()
                        else:
                            error(response)
                            yield response["event"]
                    raise RuntimeError(process.stderr.read())
                finally:
                    process.stdin.close()
                    process.wait(timeout=10)
        finally:
            connection.close()
