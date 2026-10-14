"""Keep pacman's transaction approval, selecting its routine provider defaults."""

import os
import pty
import re
import select
import sys
import termios
import tty


class Providers:
    def __init__(self):
        self.output = b""

    def feed(self, data):
        self.output = (self.output + data)[-32768:]
        plain = uncolored(self.output)
        match = re.search(rb"Enter a number \(default=(\d+)\):\s*$", plain)
        if match and re.search(rb"There are \d+ providers available for", plain):
            # Repository order defines the default. Prefer Qt's desktop backend
            # explicitly if the virtual multimedia dependency offers both.
            candidates = re.findall(rb"(\d+)\)\s+(\S+)", plain)
            selected = next(
                (
                    number
                    for number, name in candidates
                    if name.rsplit(b"/", 1)[-1] == b"qt6-multimedia-ffmpeg"
                ),
                match[1],
            )
            self.output = b""
            return selected + b"\n"
        return None


def uncolored(data):
    return re.sub(rb"\x1b\[[0-?]*[ -/]*[@-~]", b"", data)


def transaction(packages):
    if not packages or any(
        not re.fullmatch(r"[a-zA-Z0-9@_+.:<>=-]+", name) or name.startswith("-")
        for name in packages
    ):
        raise ValueError("Invalid package dependency")
    return run(
        [
            "/usr/bin/env",
            "LC_ALL=C",
            "sudo",
            "pacman",
            "-Syu",
            "--needed",
            "--",
            *packages,
        ]
    )


def run(command):
    providers = Providers()
    interactive = os.isatty(0)
    original = termios.tcgetattr(0) if interactive else None
    pid, fd = pty.fork()
    if pid == 0:
        os.execvp(command[0], command)
    pending = b""
    prompt = b""
    manual = False
    input_open = True

    # A real pseudo-terminal preserves sudo, pacman's preview/approval, keyboard
    # input and Ctrl+C. Piped approval is held until its actual yes/no prompt;
    # otherwise it could accidentally become the answer to a provider question.
    try:
        if interactive:
            tty.setraw(0)
        while True:
            readable, _, _ = select.select([fd, *([0] if input_open else [])], [], [])
            if fd in readable:
                try:
                    data = os.read(fd, 65536)
                except OSError:
                    break  # Linux returns EIO when the terminal child exits.
                if not data:
                    break
                os.write(1, data)
                prompt = (prompt + data)[-4096:]
                reply = providers.feed(data)
                if reply is not None:
                    os.write(fd, reply)
                    prompt = b""
                    manual = False
                else:
                    manual = bool(re.search(rb"\[[Yy]/[Nn]\]\s*$", uncolored(prompt)))
            if 0 in readable:
                data = os.read(0, 65536)
                if not data:
                    input_open = False
                elif interactive:
                    os.write(fd, data)
                else:
                    pending += data
            if not interactive and manual:
                if b"\n" in pending:
                    answer, pending = pending.split(b"\n", 1)
                    os.write(fd, answer + b"\n")
                    manual = False
                    prompt = b""
                elif not input_open:
                    # No unattended approval: an EOF cancels this question.
                    os.write(fd, b"n\n")
                    manual = False
                    prompt = b""
    finally:
        if original is not None:
            termios.tcsetattr(0, termios.TCSAFLUSH, original)
        os.close(fd)
    _, status = os.waitpid(pid, 0)
    return os.waitstatus_to_exitcode(status)


def main():
    raise SystemExit(transaction(sys.argv[1:]))
