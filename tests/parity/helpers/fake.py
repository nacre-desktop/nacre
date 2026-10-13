#!/usr/bin/python3
"""Fixture executable: record argv/stdin, then return only scripted responses."""

import base64
import fcntl
import json
import os
from pathlib import Path
import sys
import signal
import threading
import subprocess

root = Path(os.environ.get("NACRE_PARITY_ROOT", "/case"))
name = Path(sys.argv[0]).name
rules = json.loads((root / "stubs.json").read_text()).get(name, [])
# Only read a pipe when the case specifies input; inherited stdin must not block.
rule = next((item for item in rules if item.get("argv") == sys.argv[1:]), None)
if rule is None:
    rule = next((item for item in rules if "argv" not in item), None)
if rule is None:
    rule = {"code": 127, "stderr": "Unscripted fixture invocation\n"}


def record(raw):
    try:
        payload = {"stdin": raw.decode()}
    except UnicodeDecodeError:
        payload = {"stdin_base64": base64.b64encode(raw).decode()}
    with (root / "effects.jsonl").open("a") as stream:
        fcntl.flock(stream, fcntl.LOCK_EX)
        stream.write(
            json.dumps({"command": name, "argv": sys.argv[1:], **payload}) + "\n"
        )


if "forward_stream" in rule:
    process = subprocess.Popen(
        [rule["forward_stream"], *sys.argv[1:]],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        bufsize=0,
    )

    def copy(source, destination):
        while chunk := source.read(65536):
            destination.write(chunk)
            destination.flush()

    threads = [
        threading.Thread(target=copy, args=pair)
        for pair in (
            (process.stdout, sys.stdout.buffer),
            (process.stderr, sys.stderr.buffer),
        )
    ]
    for thread in threads:
        thread.start()

    def cancel(signum, frame):
        if process.poll() is None:
            process.send_signal(signum)
        raise SystemExit(128 + signum)

    signal.signal(signal.SIGINT, cancel)
    signal.signal(signal.SIGTERM, cancel)
    try:
        for line in sys.stdin.buffer:
            record(line)
            process.stdin.write(line)
            process.stdin.flush()
    finally:
        process.stdin.close()
        try:
            process.wait(timeout=3)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()
        for thread in threads:
            thread.join(timeout=1)
    raise SystemExit(process.returncode)

raw = sys.stdin.buffer.read() if rule.get("read_stdin") else b""
record(raw)
if "forward" in rule:
    result = subprocess.run(
        [rule["forward"], *sys.argv[1:]], input=raw, capture_output=True
    )
    sys.stdout.buffer.write(result.stdout)
    sys.stderr.buffer.write(result.stderr)
    raise SystemExit(result.returncode)
for name, text in rule.get("writes", {}).items():
    path = Path(
        name.format(
            **{f"arg{index}": value for index, value in enumerate(sys.argv[1:])}
        )
    )
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text)
sys.stdout.buffer.write(
    base64.b64decode(rule["stdout_base64"])
    if "stdout_base64" in rule
    else rule.get("stdout", "").encode()
)
sys.stderr.write(rule.get("stderr", ""))
raise SystemExit(rule.get("code", 0))
