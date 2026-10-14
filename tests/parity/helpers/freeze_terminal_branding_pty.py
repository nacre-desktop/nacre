import base64
import hashlib
import json
import os
import pty
import re
import select
import signal
import sys
import tempfile
import time
from pathlib import Path

root = Path(__file__).resolve().parents[3]
source = root / "nacre/shell-tools/terminal-branding.py"
dest = root / "crates/nacre-helper/tests/fixtures/terminal-branding-pty-reference.json"
if dest.exists():
    raise ValueError("Refusing to overwrite PTY reference")
sha = hashlib.sha256(source.read_bytes()).hexdigest()
assert sha == "87b2bd02c82d8abb87aa0f329562ac7237347ad5cac222c230d61d3013795f90"
cases = []
for prepare in (False, True):
    with tempfile.TemporaryDirectory(prefix="nacre-terminal-real-pty-") as folder:
        home = Path(folder)
        art = home / ".local/share/nacre/branding"
        art.mkdir(parents=True)
        (art / "nacre.png").write_bytes(b"fixture-image")
        code = 'import importlib.util,sys;from pathlib import Path;sys.path.insert(0,str(Path(sys.argv[1]).parent));s=importlib.util.spec_from_file_location("terminal_reference",sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);m.os.urandom=lambda n:(234).to_bytes(4,"big");m.render(prepare=sys.argv[2]=="prepare")'
        pid, master = pty.fork()
        if pid == 0:
            env = {
                "HOME": folder,
                "PATH": "/usr/bin",
                "LANG": "C.UTF-8",
                "TERM": "xterm-kitty",
                "TMUX": "",
                "NO_COLOR": "",
                "KITTY_PID": "100",
                "KITTY_WINDOW_ID": "1",
                "XDG_CONFIG_HOME": folder + "/.config",
                "XDG_STATE_HOME": folder + "/.local/state",
                "XDG_CACHE_HOME": folder + "/.cache",
            }
            os.execve(
                "/bin/bash",
                [
                    "bash",
                    "--noprofile",
                    "--norc",
                    "-c",
                    '"$1" -c "$2" "$3" "$4"; read -r -t 10 fixture',
                    "fixture",
                    sys.executable,
                    code,
                    str(source),
                    "prepare" if prepare else "render",
                ],
                env,
            )
        output = bytearray()
        cache = home / ".cache/nacre/terminal-logos"
        until = time.monotonic() + 4
        try:
            while time.monotonic() < until:
                if select.select([master], [], [], 0.03)[0]:
                    output.extend(os.read(master, 65536))
                if (prepare and (cache / "100-1.raw").is_file()) or (
                    not prepare and b"\x1b\\" in output
                ):
                    break
            records = list(cache.glob("*.json"))
            assert len(records) == 1, (prepare, bytes(output))
            record = json.loads(records[0].read_text())
            assert record["pid"] == pid and record["image"] == 234
            for key in ["pid", "inode", "device", "started", "image"]:
                record[key] = "@" + key.upper() + "@"
            record["tty"] = "/dev/pts/@PTY@"
            actual = base64.b64encode(os.fsencode(art / "nacre.png"))
            fixed = base64.b64encode(
                b"/fixture/home/.local/share/nacre/branding/nacre.png"
            )

            def norm(data, actual=actual, fixed=fixed):
                return list(
                    re.sub(rb"i=\d+,q=2", b"i=@IMAGE@,q=2", data.replace(actual, fixed))
                )

            cases.append(
                {
                    "prepare": prepare,
                    "record": record,
                    "record_mode": records[0].stat().st_mode & 0o777,
                    "stdout": norm(bytes(output)),
                    "raw": norm((cache / "100-1.raw").read_bytes())
                    if prepare
                    else None,
                    "raw_mode": (cache / "100-1.raw").stat().st_mode & 0o777
                    if prepare
                    else None,
                }
            )
        finally:
            try:
                os.killpg(pid, signal.SIGTERM)
            except ProcessLookupError:
                pass
            os.waitpid(pid, 0)
            os.close(master)
dest = root / "crates/nacre-helper/tests/fixtures/terminal-branding-pty-reference.json"
assert not dest.exists()
dest.write_text(
    json.dumps(
        {
            "source_sha256": sha,
            "normalizations": "Only generated PID, start time, PTY device/inode, random image ID, and disposable HOME are normalized.",
            "cases": cases,
        },
        indent=2,
    )
    + "\n"
)
print("Frozen2 real Bash/private-PTY cases")
