"""Temporary native-only client for Python helpers retired in later waves.

No color, image, template or publication logic lives here. Installed palette
commands use the Rust libraries directly; this only bridges old module callers.
"""

from contextlib import contextmanager
import json
import ast
import os
from pathlib import Path
import subprocess
import threading
from types import SimpleNamespace

from nacre_native_config import _binary, _encode

_gate = threading.RLock()
_owner = threading.local()


def _call(module, operation, *args, **kwargs):
    request = dict(args=_encode(list(args)), kwargs=_encode(kwargs))
    env = dict(os.environ)
    inherited = bool(getattr(_owner, "path", None))
    if not inherited and env.get("NACRE_PALETTE_LOCKED") == "1":
        try:
            os.fstat(9)
            inherited = True
        except OSError:
            pass
    if inherited:
        env["NACRE_PALETTE_LOCKED"] = "1"
    process = subprocess.Popen(
        [str(_binary()), "palette-api", module, operation],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=None,
        text=True,
        env=env,
        pass_fds=(9,) if inherited else (),
    )
    try:
        stdout, _ = process.communicate(input=json.dumps(request), timeout=120)
    except BaseException:
        process.kill()
        process.communicate()
        raise
    reply = None
    for line in stdout.splitlines():
        value = json.loads(line)
        if isinstance(value, dict) and ("result" in value or "error" in value):
            reply = value
        else:
            print(line, flush=True)
    if reply is None:
        raise OSError("Native palette helper returned no result")
    if process.returncode:
        if reply.get("type") == "CalledProcessError":
            raise subprocess.CalledProcessError(
                reply["code"], reply["command"], stderr=reply.get("stderr")
            )
        kind = {
            "KeyError": KeyError,
            "TypeError": TypeError,
            "ValueError": ValueError,
        }.get(reply.get("type"), OSError)
        message = reply.get("error", "Native palette operation failed")
        if kind is KeyError:
            message = ast.literal_eval(message)
        raise kind(message)
    return reply["result"]


@contextmanager
def publication_lock(home):
    import fcntl

    path = Path(_call("classic-state", "lock_path", Path(home)))
    path.parent.mkdir(parents=True, exist_ok=True)
    with _gate:
        previous = getattr(_owner, "path", None)
        if previous == path:
            yield
            return
        with path.open("a") as stream:
            fcntl.flock(stream, fcntl.LOCK_EX)
            try:
                saved = os.dup(9)
            except OSError:
                saved = None
            os.dup2(stream.fileno(), 9)
            _owner.path = path
            try:
                yield
            finally:
                _owner.path = previous
                if saved is None:
                    os.close(9)
                else:
                    os.dup2(saved, 9)
                    os.close(saved)


class Module(SimpleNamespace):
    def __init__(self, name):
        super().__init__()
        self._name = name

    def __getattr__(self, operation):
        if self._name == "classic-state" and operation == "DEMO_PREFERENCES":
            return tuple(_call(self._name, operation))

        def native(*args, **kwargs):
            return _call(self._name, operation, *args, **kwargs)

        return native


classic_state = Module("classic-state")
classic_state.publication_lock = publication_lock
branding = Module("branding")
file_icons = Module("file-icons")
lock_config = Module("lock-config")
lock_dashboard = Module("lock-dashboard")
wallpaper_media = Module("wallpaper-media")
