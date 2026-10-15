#!/usr/bin/env python3
"""Developer-only terminal color witness, controlled through a private socket.

The terminal reports its live OSC 11 background. This is a state acknowledgement,
not a presented frame: the harness must confirm the rendered pixels separately.
No escape sequence in this program changes terminal colors.
"""

import argparse
import json
import os
from pathlib import Path
import re
import select
import socket
import termios
import time
import tty


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--socket", type=Path, required=True)
    args = parser.parse_args()
    server = socket.socket(socket.AF_UNIX)
    server.bind(str(args.socket))
    args.socket.chmod(0o600)
    server.listen(1)
    fd = os.open("/dev/tty", os.O_RDWR | os.O_NOCTTY)
    previous = termios.tcgetattr(fd)
    try:
        tty.setraw(fd)
        # Ordinary terminal content uses ANSI bright blue. No OSC color writes;
        # the publisher's live palette must recolor these existing glyphs.
        os.write(
            fd, ("\x1b[2J\x1b[H\x1b[94m" + ("█" * 30 + "\r\n") * 5 + "\x1b[0m").encode()
        )
        with server.accept()[0] as peer:
            stream = peer.makefile("rwb", buffering=0)
            stream.write(b'{"ready":true}\n')
            for line in stream:
                request = json.loads(line)
                if request.get("action") == "quit":
                    break
                if request.get("action") != "query":
                    raise ValueError("Unknown witness action")
                os.write(fd, b"\x1b]11;?\x1b\\")
                deadline = time.monotonic() + 2
                response = b""
                while b"\x1b\\" not in response and b"\x07" not in response:
                    remaining = deadline - time.monotonic()
                    if remaining <= 0 or not select.select([fd], [], [], remaining)[0]:
                        raise TimeoutError("Terminal color reply missing")
                    response += os.read(fd, 256)
                    if len(response) > 4096:
                        raise ValueError("Terminal color reply too large")
                match = re.search(
                    rb"rgb:([0-9a-fA-F]+)/([0-9a-fA-F]+)/([0-9a-fA-F]+)", response
                )
                if not match:
                    raise ValueError("Malformed terminal color reply")
                channels = [
                    round(int(value, 16) * 255 / (16 ** len(value) - 1))
                    for value in match.groups()
                ]
                result = dict(
                    consumer="kitty",
                    background="".join(f"{v:02x}" for v in channels),
                    monotonic_ns=time.monotonic_ns(),
                    realtime_ns=time.time_ns(),
                    boundary="OSC11 live color acknowledgement; pixels still required",
                )
                stream.write((json.dumps(result) + "\n").encode())
    finally:
        termios.tcsetattr(fd, termios.TCSANOW, previous)
        os.close(fd)
        server.close()
        args.socket.unlink(missing_ok=True)


if __name__ == "__main__":
    main()
