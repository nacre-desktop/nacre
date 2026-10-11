import http.client
import importlib.util
import json
from pathlib import Path
import tempfile
import threading
import unittest
from unittest.mock import patch
from urllib.parse import urlencode

spec = importlib.util.spec_from_file_location(
    "authenticated_brain", Path(__file__).parents[1] / "control.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class BrowserAuthenticationTests(unittest.TestCase):
    def test_cookie_is_required_bootstrap_is_private_and_restart_keeps_session(self):
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            server = m.BrainServer(("127.0.0.1", 0), state)
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()

            def get(path, cookie=None, host=None):
                connection = http.client.HTTPConnection(
                    "127.0.0.1", server.server_port, timeout=3
                )
                headers = {}
                if cookie:
                    headers["Cookie"] = cookie
                if host:
                    headers["Host"] = host
                connection.request("GET", path, headers=headers)
                response = connection.getresponse()
                result = response.status, dict(response.getheaders()), response.read()
                connection.close()
                return result

            try:
                self.assertEqual(get("/api/health")[0], 200)
                self.assertEqual(get("/api/brain")[0], 403)
                self.assertEqual(get("/api/note?path=anything")[0], 403)
                self.assertEqual(get("/?token=invalid")[0], 403)
                boot = get("/?" + urlencode({"token": server.auth.token}))
                self.assertEqual(boot[0], 303)
                cookie = boot[1]["Set-Cookie"].split(";", 1)[0]
                self.assertIn("HttpOnly", boot[1]["Set-Cookie"])
                self.assertIn("SameSite=Strict", boot[1]["Set-Cookie"])
                self.assertEqual(boot[1]["Location"], "/")
                with patch.object(m, "graph", return_value={"fixture": True}):
                    self.assertEqual(get("/api/brain", cookie)[0], 200)
                    self.assertEqual(get("/api/brain", cookie, "evil.invalid")[0], 403)
                old = server.auth.token
                server.auth = m.auth_module().BrowserAuth(state, server.server_port)
                self.assertNotEqual(old, server.auth.token)
                self.assertEqual(get("/?" + urlencode({"token": old}))[0], 403)
                with patch.object(m, "graph", return_value={}):
                    self.assertEqual(get("/api/brain", cookie)[0], 200)
                for file in (state / "auth").iterdir():
                    self.assertEqual(file.stat().st_mode & 0o777, 0o600)
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=3)

    def test_post_needs_cookie_and_same_origin(self):
        with tempfile.TemporaryDirectory() as directory:
            server = m.BrainServer(("127.0.0.1", 0), Path(directory))
            thread = threading.Thread(target=server.serve_forever, daemon=True)
            thread.start()
            try:
                for cookie, origin, expected in (
                    (None, True, 403),
                    (server.auth.cookie, False, 403),
                    (server.auth.cookie, True, 200),
                ):
                    connection = http.client.HTTPConnection(
                        "127.0.0.1", server.server_port, timeout=3
                    )
                    headers = {
                        "Content-Type": "application/json",
                        "Origin": f"http://127.0.0.1:{server.server_port}"
                        if origin
                        else "http://evil.invalid",
                    }
                    if cookie:
                        headers["Cookie"] = "nacre_brain=" + cookie
                    with patch.object(m, "action"):
                        connection.request(
                            "POST",
                            "/api/action",
                            json.dumps({"name": "assign-note", "value": "{}"}),
                            headers,
                        )
                        response = connection.getresponse()
                        self.assertEqual(response.status, expected)
                        response.read()
                    connection.close()
            finally:
                server.shutdown()
                server.server_close()
                thread.join(timeout=3)
