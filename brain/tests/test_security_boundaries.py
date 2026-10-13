"""Brain HTTP checks use only ephemeral loopback listeners and fixture vaults."""

import http.client
import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch
from urllib.parse import urlencode

SPEC = importlib.util.spec_from_file_location(
    "brain_security", Path(__file__).parents[1] / "control.py"
)
brain = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(brain)


class BrainSecurityBoundaryTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.vault = self.root / "vault"
        self.vault.mkdir()
        (self.vault / "allowed.md").write_text("# Public fixture")
        (self.root / "outside.md").write_text("outside fixture")
        (self.vault / "linked.md").symlink_to(self.root / "outside.md")
        (self.vault / "directory-link").symlink_to(self.root, target_is_directory=True)
        (self.vault / ".hidden.md").write_text("hidden fixture")
        vault_patch = patch.object(brain, "VAULT", self.vault)
        vault_patch.start()
        self.addCleanup(vault_patch.stop)
        self.server = brain.BrainServer(("127.0.0.1", 0), self.root / "state")
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()
        self.addCleanup(self.stop_server)

    def stop_server(self):
        self.server.shutdown()
        self.server.server_close()
        self.thread.join(timeout=3)

    def request(self, method, path, body=None, headers=None):
        connection = http.client.HTTPConnection(
            "127.0.0.1", self.server.server_port, timeout=3
        )
        connection.request(method, path, body=body, headers=headers or {})
        response = connection.getresponse()
        result = response.status, response.read()
        connection.close()
        return result

    def authenticated_headers(self):
        return {
            "Cookie": "nacre_brain=" + self.server.auth.cookie,
            "Origin": "http://127.0.0.1:" + str(self.server.server_port),
            "Content-Type": "application/json",
        }

    def test_note_routes_reject_traversal_hidden_files_and_symlink_escapes(self):
        for path in (
            "../outside.md",
            str(self.root / "outside.md"),
            "linked.md",
            "directory-link/outside.md",
            ".hidden.md",
            "missing.md",
        ):
            with self.subTest(path=path):
                status, body = self.request(
                    "GET",
                    "/api/note?" + urlencode({"path": path}),
                    headers=self.authenticated_headers(),
                )
                self.assertEqual(status, 404)
                self.assertNotIn(b"fixture", body)
        status, body = self.request(
            "GET", "/api/note?path=allowed.md", headers=self.authenticated_headers()
        )
        self.assertEqual(status, 200)
        self.assertEqual(json.loads(body), {"text": "# Public fixture"})

    def test_static_file_allowlist_does_not_serve_repository_or_vault_files(self):
        for path in ("/auth.py", "/control.py", "/web/../auth.py", "/allowed.md"):
            with self.subTest(path=path):
                self.assertEqual(self.request("GET", path)[0], 404)

    def test_action_rejects_invalid_lengths_and_cross_origin_forms_before_launch(self):
        with (
            patch.object(brain, "launch_app") as launch,
            patch.object(brain, "action") as action,
        ):
            for length in ("0", "-1", "4097", "invalid"):
                with self.subTest(length=length):
                    headers = self.authenticated_headers()
                    headers["Content-Length"] = length
                    self.assertEqual(
                        self.request("POST", "/api/action", b"", headers)[0], 400
                    )
            for key, value in (
                ("Origin", "null"),
                ("Origin", "https://example.invalid"),
                ("Host", "example.invalid"),
                ("Content-Type", "text/plain"),
                ("Content-Type", "application/x-www-form-urlencoded"),
                ("Cookie", "nacre_brain=invalid"),
            ):
                headers = self.authenticated_headers()
                headers[key] = value
                with self.subTest(key=key, value=value):
                    self.assertEqual(
                        self.request("POST", "/api/action", "{}", headers)[0], 403
                    )
            self.assertEqual(
                self.request(
                    "POST",
                    "/api/action",
                    '{"name":"exec"}',
                    self.authenticated_headers(),
                )[0],
                400,
            )
            launch.assert_not_called()
            action.assert_not_called()

    def test_authentication_directory_is_private_and_linked_credentials_are_refused(
        self,
    ):
        self.assertEqual((self.root / "state/auth").stat().st_mode & 0o777, 0o700)
        for path in (self.root / "state/auth").iterdir():
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)
        session = self.root / "state/auth/session.json"
        session.unlink()
        session.symlink_to(self.root / "outside.md")
        with self.assertRaises(RuntimeError):
            brain.auth_module().BrowserAuth(
                self.root / "state", self.server.server_port
            )
