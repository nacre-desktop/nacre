#!/usr/bin/env python3
"""Freeze the existing session query with private loginctl/busctl peers."""

import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/wallpaper-media.py"
DEST = ROOT / "crates/nacre-helper/tests/fixtures/wallpaper-session-reference.json"
PEER = """#!/usr/bin/python3
import json,os,sys
from pathlib import Path
command=Path(sys.argv[0]).name;args=sys.argv[1:];case=os.environ["SESSION_CASE"]
if command=="loginctl" and args[0]=="show-user":args[1]="@UID@"
with (Path.home()/"calls.jsonl").open("a") as stream:stream.write(json.dumps(dict(command=command,argv=args))+"\\n")
if command=="busctl":print('o "/org/freedesktop/login1/session/_7"')
elif args[0]=="show-user":print("" if case=="empty" else "7")
else:print("yes" if case=="locked" else "no")
"""


def main():
    if DEST.exists():
        raise ValueError("Refusing to overwrite session references")
    source_sha = hashlib.sha256(SOURCE.read_bytes()).hexdigest()
    if source_sha != "74795d256c7f4e44b5d19e6ed7b58a158d3a1905b552097c99de1681abcbc4a2":
        raise ValueError("Current media source changed")
    cases = []
    for name in ("empty", "locked", "unlocked"):
        with tempfile.TemporaryDirectory(
            prefix="nacre-wallpaper-session-original-"
        ) as folder:
            home = Path(folder)
            binary = home / "bin"
            binary.mkdir()
            for command in ("loginctl", "busctl"):
                path = binary / command
                path.write_text(PEER)
                path.chmod(0o700)
            env = {
                **os.environ,
                "HOME": str(home),
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
                "XDG_CACHE_HOME": str(home / ".cache"),
                "PATH": str(binary) + ":/usr/bin",
                "SESSION_CASE": name,
            }
            reply = subprocess.run(
                [sys.executable, str(SOURCE), "session"],
                env=env,
                capture_output=True,
                text=True,
                timeout=30,
                check=True,
            )
            calls = [
                json.loads(line)
                for line in (home / "calls.jsonl").read_text().splitlines()
            ]
            cases.append(
                dict(
                    name=name,
                    stdout=reply.stdout,
                    stderr=reply.stderr,
                    exit=reply.returncode,
                    effects=calls,
                )
            )
    DEST.write_text(
        json.dumps(dict(source_sha256=source_sha, cases=cases), indent=2) + "\n"
    )
    print(
        f"Frozen {len(cases)} session states and exact tool-call effects; no system bus queried"
    )


if __name__ == "__main__":
    main()
