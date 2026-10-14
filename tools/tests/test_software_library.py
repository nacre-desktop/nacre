"""Exercise extension imports/updates through actual HTTPS Git and native CLI."""

import unittest
import shutil
import http.server
import threading
import ssl
import tempfile
import subprocess
import json
import os
import urllib.parse
from pathlib import Path

binary = Path(
    os.environ.get(
        "NACRE_HELPER_BINARY",
        str(Path(__file__).resolve().parents[2] / "target/release/nacre-helper"),
    )
)


class SoftwareLibraryIntegrationTests(unittest.TestCase):
    def test_https_lifecycle_preserves_edits_and_rejects_invalid_updates(self):
        if not shutil.which("openssl") or not binary.is_file():
            self.skipTest("OpenSSL and a compiled candidate helper are required")
        with tempfile.TemporaryDirectory(prefix="nacre-extension-check-") as name:
            root = Path(name)
            repo = root / "extension.git"
            repo.mkdir()
            identity = [
                "-c",
                "user.name=Nacre Fixture",
                "-c",
                "user.email=fixture@example.com",
            ]

            def git(*args):
                return subprocess.check_output(
                    ["git", *identity, *args],
                    cwd=repo,
                    text=True,
                    stderr=subprocess.DEVNULL,
                ).strip()

            git("init", "-q", "--initial-branch=main")

            def commit(version, hooks=False):
                value = {
                    "schemaVersion": 1,
                    "id": "example.integration",
                    "name": "Integration pack",
                    "description": "Private deterministic test fixture",
                    "version": version,
                    "kinds": ["launcher"],
                    "actions": [
                        {
                            "id": "sample",
                            "name": "Sample action",
                            "command": ["echo", "literal argument; $(never run)"],
                        }
                    ],
                }
                if hooks:
                    value["hooks"] = {"install": "echo NOT EXECUTED"}
                (repo / "nacre.extension.json").write_text(json.dumps(value))
                git("add", "nacre.extension.json")
                git("commit", "-qm", "Update fixture")
                return git("rev-parse", "HEAD")

            first = commit("1.0")
            key = root / "private.key"
            cert = root / "certificate.pem"
            subprocess.run(
                [
                    "openssl",
                    "req",
                    "-x509",
                    "-newkey",
                    "rsa:2048",
                    "-nodes",
                    "-keyout",
                    str(key),
                    "-out",
                    str(cert),
                    "-days",
                    "1",
                    "-subj",
                    "/CN=localhost",
                    "-addext",
                    "subjectAltName=DNS:localhost",
                ],
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
                check=True,
            )

            class Handler(http.server.BaseHTTPRequestHandler):
                protocol_version = "HTTP/1.1"

                def log_message(self, *args):
                    pass

                def do_GET(self):
                    self.serve()

                def do_POST(self):
                    self.serve()

                def serve(self):
                    u = urllib.parse.urlsplit(self.path)
                    body = self.rfile.read(int(self.headers.get("Content-Length", "0")))
                    env = {
                        **os.environ,
                        "GIT_PROJECT_ROOT": str(root),
                        "GIT_HTTP_EXPORT_ALL": "1",
                        "PATH_INFO": u.path,
                        "QUERY_STRING": u.query,
                        "REQUEST_METHOD": self.command,
                        "CONTENT_TYPE": self.headers.get("Content-Type", ""),
                        "CONTENT_LENGTH": str(len(body)),
                    }
                    r = subprocess.run(
                        ["git", "http-backend"],
                        env=env,
                        input=body,
                        capture_output=True,
                        check=True,
                    )
                    header, payload = r.stdout.split(b"\r\n\r\n", 1)
                    headers = [
                        line.decode().split(":", 1) for line in header.split(b"\r\n")
                    ]
                    status = next(
                        (
                            int(v.strip().split()[0])
                            for k, v in headers
                            if k.lower() == "status"
                        ),
                        200,
                    )
                    self.send_response(status)
                    self.send_header("Content-Length", str(len(payload)))
                    for k, v in headers:
                        if k.lower() != "status":
                            self.send_header(k, v.strip())
                    self.end_headers()
                    self.wfile.write(payload)

            server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Handler)
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.load_cert_chain(cert, key)
            server.socket = context.wrap_socket(server.socket, server_side=True)
            threading.Thread(target=server.serve_forever, daemon=True).start()
            home = root / "home"
            home.mkdir()
            data = root / "xdg-data"
            config = root / "xdg-config"
            state = root / "xdg-state"
            runtime = root / "runtime"
            runtime.mkdir(mode=0o700)
            env = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(config),
                "XDG_DATA_HOME": str(data),
                "XDG_STATE_HOME": str(state),
                "XDG_CACHE_HOME": str(root / "cache"),
                "XDG_RUNTIME_DIR": str(runtime),
                "GIT_SSL_CAINFO": str(cert),
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(runtime / "absent-bus"),
                "DBUS_SYSTEM_BUS_ADDRESS": "unix:path="
                + str(runtime / "absent-system-bus"),
            }
            for keyname in [
                "WAYLAND_DISPLAY",
                "DISPLAY",
                "NIRI_SOCKET",
                "HYPRLAND_INSTANCE_SIGNATURE",
            ]:
                env.pop(keyname, None)

            def call(action, value="", payload=None, success=True):
                r = subprocess.run(
                    [str(binary), "software-library", action, value],
                    env=env,
                    input=json.dumps(payload or {}),
                    capture_output=True,
                    text=True,
                    timeout=20,
                )
                value = json.loads(r.stdout)
                assert (r.returncode == 0) == success, (action, value, r.stderr)
                return value

            try:
                url = "https://localhost:" + str(server.server_port) + "/extension.git"
                call("import", payload={"url": url})
                folder = data / "nacre/software/extensions/example.integration"
                assert folder.is_dir()
                assert not (
                    home / ".local/share/nacre/software/extensions/example.integration"
                ).exists()
                cfg = config / "nacre/software-library.json"
                record = json.loads(cfg.read_text())["extensions"][0]
                assert record["revision"] == first and not record["enabled"]
                call("enable", "example.integration")
                assert json.loads(cfg.read_text())["extensions"][0]["enabled"]
                second = commit("2.0")
                call("update", "example.integration")
                record = json.loads(cfg.read_text())["extensions"][0]
                assert record["revision"] == second and not record["enabled"]
                (folder / "nacre.extension.json").write_text(
                    (folder / "nacre.extension.json").read_text() + "\n"
                )
                before = cfg.read_bytes()
                commit("3.0")
                call("update", "example.integration", success=False)
                assert cfg.read_bytes() == before
                subprocess.run(
                    [
                        "git",
                        "-C",
                        str(folder),
                        "checkout",
                        "--",
                        "nacre.extension.json",
                    ],
                    check=True,
                )
                commit("4.0", hooks=True)
                call("update", "example.integration", success=False)
                assert cfg.read_bytes() == before
                assert (
                    json.loads((folder / "nacre.extension.json").read_text())["version"]
                    == "2.0"
                )
                call("remove", "example.integration")
                assert not folder.exists()
                assert not json.loads(cfg.read_text())["extensions"]
                print(
                    json.dumps(
                        {
                            "httpsGitImport": True,
                            "isolatedXdgData": True,
                            "updateDisablesNewCode": True,
                            "dirtyCheckoutPreserved": True,
                            "invalidUpdatePreservedOldVersion": True,
                            "managedRemoval": True,
                            "noHooksExecuted": True,
                        }
                    )
                )
            finally:
                server.shutdown()
                server.server_close()
