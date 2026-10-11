"""Local browser authentication: rotated bootstrap tokens and restart-safe cookies."""

import hashlib
import json
import os
from pathlib import Path
import secrets
import tempfile
from urllib.parse import urlencode


def private_write(path, text):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    if path.is_symlink() or path.parent.is_symlink():
        raise RuntimeError("Refusing linked authentication files")
    fd, temporary = tempfile.mkstemp(dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            stream.write(text)
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        Path(temporary).unlink(missing_ok=True)


class BrowserAuth:
    def __init__(self, state, port):
        self.directory = Path(state) / "auth"
        self.directory.mkdir(parents=True, exist_ok=True, mode=0o700)
        if self.directory.is_symlink():
            raise RuntimeError("Refusing linked authentication directory")
        self.directory.chmod(0o700)
        session = self.directory / "session.json"
        if session.is_symlink():
            raise RuntimeError("Refusing linked browser credential")
        if session.exists():
            self.cookie = json.loads(session.read_text())["cookie"]
            if not isinstance(self.cookie, str) or len(self.cookie) < 40:
                raise ValueError("Invalid private browser credential")
            session.chmod(0o600)
        else:
            self.cookie = secrets.token_urlsafe(32)
            private_write(session, json.dumps({"cookie": self.cookie}))
        self.token = secrets.token_urlsafe(32)
        private_write(
            self.directory / "bootstrap.json", json.dumps({"token": self.token})
        )
        for handoff in (False, True):
            url = (
                "http://127.0.0.1:"
                + str(port)
                + "/?"
                + urlencode({"token": self.token, "handoff": "1" if handoff else "0"})
            )
            name = "handoff.html" if handoff else "open.html"
            private_write(
                self.directory / name,
                '<!doctype html><meta charset="utf-8"><script>location.replace('
                + json.dumps(url)
                + ")</script>",
            )

    def token_valid(self, value):
        return isinstance(value, str) and secrets.compare_digest(value, self.token)

    def cookie_valid(self, value):
        return isinstance(value, str) and secrets.compare_digest(value, self.cookie)

    def mark_browser(self):
        private_write(
            self.directory / "browser.json",
            json.dumps({"session": hashlib.sha256(self.cookie.encode()).hexdigest()}),
        )

    def browser_ready(self):
        try:
            value = json.loads((self.directory / "browser.json").read_text())["session"]
            return secrets.compare_digest(
                value, hashlib.sha256(self.cookie.encode()).hexdigest()
            )
        except (OSError, ValueError, KeyError):
            return False
