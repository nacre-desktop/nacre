"""Frozen tool providers cross native sockets without replaying helper outputs."""

import json
from pathlib import Path
import socket
import tempfile
import unittest
from native_compositor_peer import Peer


class NativeCompositorPeerTests(unittest.TestCase):
    def test_exact_query_and_eval_bytes_record_original_effects(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "effects.jsonl").touch()
            peer = Peer(
                root,
                [
                    {"argv": ["clients", "-j"], "stdout": "[]"},
                    {"argv": ["eval", "hl.config({})"], "stdout": "ok\n"},
                ],
            )
            try:
                for command, reply in [
                    (b"j/clients", b"[]"),
                    (b"/eval hl.config({})", b"ok"),
                ]:
                    with socket.socket(socket.AF_UNIX) as client:
                        client.connect(str(peer.path))
                        client.sendall(command)
                        client.shutdown(socket.SHUT_WR)
                        self.assertEqual(client.recv(1024), reply)
            finally:
                peer.close()
            effects = [
                json.loads(line)
                for line in (root / "effects.jsonl").read_text().splitlines()
            ]
            self.assertEqual(
                effects,
                [
                    {"command": "hyprctl", "argv": ["clients", "-j"], "stdin": ""},
                    {
                        "command": "hyprctl",
                        "argv": ["eval", "hl.config({})"],
                        "stdin": "",
                    },
                ],
            )

    def test_unscripted_native_query_fails_closed(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "effects.jsonl").touch()
            peer = Peer(root, [])
            with socket.socket(socket.AF_UNIX) as client:
                client.connect(str(peer.path))
                client.sendall(b"j/clients")
                client.shutdown(socket.SHUT_WR)
                self.assertEqual(client.recv(1024), b"error: unscripted fixture")
            with self.assertRaisesRegex(
                RuntimeError, "Unscripted native compositor query"
            ):
                peer.close()
            self.assertEqual((root / "effects.jsonl").read_text(), "")


if __name__ == "__main__":
    unittest.main()
