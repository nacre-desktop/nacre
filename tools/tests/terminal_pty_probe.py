"""Single-process PTY driver: do not inherit a test suite's Popen objects/threads."""

import base64
import json
import os
from pathlib import Path
import pty
import re
import select
import signal
import sys
import tempfile
import time


def probe(binary, prepare):
    with tempfile.TemporaryDirectory(prefix="nacre-native-terminal-") as folder:
        home = Path(folder)
        art = home / ".local/share/nacre/branding"
        art.mkdir(parents=True)
        (art / "nacre.png").write_bytes(b"fixture-image")
        pid, master = pty.fork()
        if pid == 0:
            environment = {
                "HOME": folder,
                "PATH": str(home / "absent-path"),
                "LANG": "C.UTF-8",
                "TERM": "xterm-kitty",
                "TMUX": "",
                "NO_COLOR": "",
                "KITTY_PID": "100",
                "KITTY_WINDOW_ID": "1",
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + folder + "/absent-session",
                "DBUS_SYSTEM_BUS_ADDRESS": "unix:path=" + folder + "/absent-system",
            }
            os.execve(
                "/bin/bash",
                [
                    "bash",
                    "--noprofile",
                    "--norc",
                    "-c",
                    '"$1" terminal-branding "$2"; read -r -t 10 fixture',
                    "fixture",
                    binary,
                    "--prepare" if prepare else "ignored",
                ],
                environment,
            )
        output = bytearray()
        cache = home / ".cache/nacre/terminal-logos"
        try:
            until = time.monotonic() + 4
            while time.monotonic() < until:
                if select.select([master], [], [], 0.03)[0]:
                    output.extend(os.read(master, 65536))
                if (prepare and (cache / "100-1.raw").is_file()) or (
                    not prepare and b"\x1b\\" in output
                ):
                    break
            records = list(cache.glob("*.json"))
            if len(records) != 1:
                raise AssertionError(
                    f"Expected one private registration: {bytes(output)!r}"
                )
            record = json.loads(records[0].read_text())
            actual = base64.b64encode(os.fsencode(art / "nacre.png"))
            fixed = base64.b64encode(
                b"/fixture/home/.local/share/nacre/branding/nacre.png"
            )

            def normalized(data):
                return list(
                    re.sub(rb"i=\d+,q=2", b"i=@IMAGE@,q=2", data.replace(actual, fixed))
                )

            return {
                "shell_pid": pid,
                "record": record,
                "record_mode": records[0].stat().st_mode & 0o777,
                "cache_mode": cache.stat().st_mode & 0o777,
                "stdout": normalized(bytes(output)),
                "raw": normalized((cache / "100-1.raw").read_bytes())
                if prepare
                else None,
                "raw_mode": (cache / "100-1.raw").stat().st_mode & 0o777
                if prepare
                else None,
            }
        finally:
            try:
                os.killpg(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            os.waitpid(pid, 0)
            os.close(master)


if __name__ == "__main__":
    print(
        json.dumps(probe(sys.argv[1], len(sys.argv) > 2 and sys.argv[2] == "--prepare"))
    )
