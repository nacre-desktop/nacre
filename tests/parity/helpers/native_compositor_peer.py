"""Developer-only Hyprland socket provider for unchanged caller-effect goldens."""

import fcntl
import json
import os
import socket
import threading


class Peer:
    def __init__(self, root, rules):
        self.root = root
        self.rules = rules
        self.errors = []
        self.stop = threading.Event()
        self.path = root / "runtime/hypr/fixture/.socket.sock"
        self.path.parent.mkdir(parents=True, exist_ok=True)
        self.server = socket.socket(socket.AF_UNIX)
        self.server.bind(str(self.path))
        if os.getuid() == 0:
            os.chown(self.path, 1000, 1000)
        self.path.chmod(0o600)
        self.server.listen()
        self.server.settimeout(0.1)
        self.thread = threading.Thread(target=self.serve, daemon=True)
        self.thread.start()

    def serve(self):
        while not self.stop.is_set():
            try:
                connection, _ = self.server.accept()
            except TimeoutError:
                continue
            except OSError:
                if self.stop.is_set():
                    return
                raise
            with connection:
                connection.settimeout(5)
                raw = b""
                while part := connection.recv(8192):
                    raw += part
                text = raw.decode()
                flags, command = text.split("/", 1)
                name, _, tail = command.partition(" ")
                if name == "eval":
                    argv = [name, tail]
                else:
                    argv = [name, *tail.split()] if tail else [name]
                if flags == "j":
                    argv.append("-j")
                rule = next(
                    (rule for rule in self.rules if rule.get("argv") == argv), None
                )
                rule = rule or next(
                    (rule for rule in self.rules if "argv" not in rule), None
                )
                if rule is None:
                    self.errors.append(
                        "Unscripted native compositor query: " + repr(argv)
                    )
                    connection.sendall(b"error: unscripted fixture")
                    continue
                with (self.root / "effects.jsonl").open("a") as stream:
                    fcntl.flock(stream, fcntl.LOCK_EX)
                    stream.write(
                        json.dumps({"command": "hyprctl", "argv": argv, "stdin": ""})
                        + "\n"
                    )
                response = rule.get("stdout", "")
                # Native transport adds hyprctl's presentation newline itself.
                if response.endswith("\n"):
                    response = response[:-1]
                if rule.get("code", 0):
                    response = "error: " + rule.get("stderr", response).strip()
                connection.sendall(response.encode())

    def close(self):
        self.stop.set()
        self.server.close()
        self.thread.join(timeout=2)
        if self.errors:
            raise RuntimeError("; ".join(self.errors))
