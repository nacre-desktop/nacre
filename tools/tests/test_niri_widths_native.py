"""Production observer against private authenticated Xvfb and Niri sockets."""

import json
import os
from pathlib import Path
import select
import shutil
import socket
import subprocess
import tempfile
import threading
import time
import unittest
from unittest.mock import patch

from widths_x11_fixture import X11

ROOT = Path(__file__).resolve().parents[2]


class WidthObserverNativeTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(self.enterContext(tempfile.TemporaryDirectory()))
        self.auth = self.root / "Xauthority"
        fields = [b"", b"", b"MIT-MAGIC-COOKIE-1", os.urandom(16)]
        self.auth.write_bytes(
            b"\xff\xff"
            + b"".join(len(field).to_bytes(2, "big") + field for field in fields)
        )
        self.auth.chmod(0o600)
        self.xlog = self.enterContext((self.root / "xvfb.log").open("w+"))
        reader, writer = os.pipe()
        self.xvfb = subprocess.Popen(
            [
                shutil.which("Xvfb") or "Xvfb",
                "-displayfd",
                str(writer),
                "-auth",
                str(self.auth),
                "-screen",
                "0",
                "800x600x24",
                "-nolisten",
                "tcp",
            ],
            pass_fds=(writer,),
            stdout=subprocess.DEVNULL,
            stderr=self.xlog,
        )
        os.close(writer)
        self.addCleanup(self.stop_process, self.xvfb)
        try:
            if not select.select([reader], [], [], 5)[0]:
                raise RuntimeError("Private Xvfb did not become ready")
            display = ":" + os.read(reader, 64).decode().strip()
            if display == ":":
                raise RuntimeError("Private Xvfb exited before allocating a display")
        finally:
            os.close(reader)
        self.enterContext(patch.dict(os.environ, {"XAUTHORITY": str(self.auth)}))
        self.x11 = X11(display)
        self.addCleanup(self.x11.close)
        self.socket = socket.socket(socket.AF_UNIX)
        self.socket.bind(str(self.root / "niri.sock"))
        self.socket.listen()
        self.socket.settimeout(0.1)
        self.addCleanup(self.socket.close)
        self.finished = threading.Event()
        self.event_ready = threading.Event()
        self.event_connection = None
        self.requests = []
        self.errors = []
        self.connections = 0
        self.send_lock = threading.Lock()
        self.threads = []
        self.acceptor = threading.Thread(target=self.accept, daemon=True)
        self.acceptor.start()
        self.addCleanup(self.stop_peer)
        self.env = {
            **os.environ,
            "HOME": str(self.root / "home"),
            "DISPLAY": display,
            "XAUTHORITY": str(self.auth),
            "NIRI_SOCKET": str(self.root / "niri.sock"),
            "NACRE_COMPOSITOR": "niri",
            "XDG_CONFIG_HOME": str(self.root / "home/.config"),
            "XDG_STATE_HOME": str(self.root / "home/.local/state"),
            "XDG_CACHE_HOME": str(self.root / "home/.cache"),
            "XDG_RUNTIME_DIR": str(self.root / "runtime"),
            "PATH": "/nonexistent",
            "DBUS_SESSION_BUS_ADDRESS": "unix:path="
            + str(self.root / "absent-session"),
            "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + str(self.root / "absent-system"),
        }
        for key in (
            "WAYLAND_DISPLAY",
            "HYPRLAND_INSTANCE_SIGNATURE",
            "XDG_CURRENT_DESKTOP",
        ):
            self.env.pop(key, None)

    @staticmethod
    def stop_process(process):
        if process.poll() is None:
            process.terminate()
        process.wait(timeout=5)

    def stop_peer(self):
        self.finished.set()
        if self.event_connection is not None:
            try:
                self.event_connection.shutdown(socket.SHUT_RDWR)
            except OSError:
                pass  # Test cleanup after helper-owned socket closure.
        self.acceptor.join(timeout=2)
        for thread in self.threads:
            thread.join(timeout=2)
        self.assertEqual(self.errors, [])

    def accept(self):
        while not self.finished.is_set():
            try:
                connection, _ = self.socket.accept()
            except TimeoutError:
                continue
            self.connections += 1
            thread = threading.Thread(target=self.peer, args=(connection,), daemon=True)
            self.threads.append(thread)
            thread.start()

    def peer(self, connection):
        try:
            with connection, connection.makefile("rb") as stream:
                for line in stream:
                    request = json.loads(line)
                    self.requests.append(request)
                    if request == "EventStream":
                        self.event_connection = connection
                        self.event_ready.set()
                    else:
                        connection.sendall(b'{"Ok":"Handled"}\n')
        except (OSError, ValueError) as error:
            if not self.finished.is_set():
                self.errors.append(str(error))

    def start(self):
        self.log = self.enterContext((self.root / "helper.log").open("w+"))
        binary = os.environ.get("NACRE_HELPER_BINARY") or str(
            ROOT / "target/release/nacre-helper"
        )
        self.helper = subprocess.Popen(
            [binary, "niri_widths"], env=self.env, stdout=self.log, stderr=self.log
        )
        self.addCleanup(self.stop_process, self.helper)
        self.assertTrue(
            self.event_ready.wait(5), "Observer never subscribed to private Niri"
        )

    def event(self, event):
        with self.send_lock:
            self.event_connection.sendall(json.dumps(event).encode() + b"\n")

    @staticmethod
    def window(identifier, title, floating=False):
        return {
            "id": identifier,
            "app_id": "Fixture",
            "title": title,
            "is_floating": floating,
        }

    def await_value(self, read, expected):
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            value = read()
            if value == expected:
                return value
            if self.helper.poll() is not None:
                self.log.seek(0)
                self.fail("Observer exited: " + self.log.read()[-800:])
            time.sleep(0.01)
        self.fail(f"Private observer did not reach {expected!r}; last value {value!r}")

    def test_constraints_restore_on_floating_and_shutdown_without_changing_other_fields(
        self,
    ):
        window = self.x11.create("Existing", 1600)
        original = bytes(self.x11.hints(window))
        self.start()
        self.event({"WindowsChanged": {"windows": [self.window(1, "Existing")]}})
        self.await_value(lambda: self.x11.hints(window).min_width, 1)
        self.event(
            {"WindowOpenedOrChanged": {"window": self.window(1, "Existing", True)}}
        )
        self.await_value(lambda: self.x11.hints(window).min_width, 1600)
        self.assertEqual(bytes(self.x11.hints(window)), original)
        self.event({"WindowOpenedOrChanged": {"window": self.window(1, "Existing")}})
        self.await_value(lambda: self.x11.hints(window).min_width, 1)
        self.helper.terminate()
        self.assertEqual(self.helper.wait(timeout=5), 0)
        self.assertEqual(bytes(self.x11.hints(window)), original)
        self.assertEqual(self.requests, ["EventStream"])

    def test_new_windows_resize_once_over_one_persistent_action_connection(self):
        self.x11.create("Existing", 1600)
        self.start()
        self.event({"WindowsChanged": {"windows": [self.window(1, "Existing")]}})
        self.await_value(lambda: len(self.requests), 1)
        for identifier in (2, 3):
            title = f"New {identifier}"
            window = self.x11.create(title, 2400)
            self.event(
                {"WindowOpenedOrChanged": {"window": self.window(identifier, title)}}
            )
            self.await_value(lambda window=window: self.x11.hints(window).min_width, 1)
            expected = {
                "Action": {
                    "SetWindowWidth": {
                        "id": identifier,
                        "change": {"SetProportion": 50.0},
                    }
                }
            }
            self.await_value(lambda: self.requests[-1], expected)
            hints = self.x11.hints(window)
            hints.min_width = 2800
            self.x11.set_hints(window, hints)
            self.await_value(lambda window=window: self.x11.hints(window).min_width, 1)
        self.helper.terminate()
        self.assertEqual(self.helper.wait(timeout=5), 0)
        self.assertEqual(len(self.requests), 3)
        self.assertEqual(self.connections, 2)  # Event stream plus one action stream.

    def test_event_disconnect_is_visible_and_restores_constraints(self):
        window = self.x11.create("Existing", 1600)
        self.start()
        self.event({"WindowsChanged": {"windows": [self.window(1, "Existing")]}})
        self.await_value(lambda: self.x11.hints(window).min_width, 1)
        self.event_connection.shutdown(socket.SHUT_RDWR)
        self.assertEqual(self.helper.wait(timeout=5), 1)
        self.assertEqual(self.x11.hints(window).min_width, 1600)
        self.log.seek(0)
        self.assertIn("Niri width event stream disconnected", self.log.read())
