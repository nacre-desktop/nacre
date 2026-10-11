#!/usr/bin/env python3
"""Register Nacre's Kitty logo and recolor its existing frame on palette events."""

import base64
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile

ROOT = Path(__file__).resolve().parent


def identity(pid, proc=Path("/proc")):
    directory = proc / str(pid)
    if directory.stat().st_uid != os.getuid():
        raise ValueError("Other user process")
    # Fields after the final ')' begin at stat field 3; starttime is field 22.
    fields = (directory / "stat").read_text().rsplit(")", 1)[1].split()
    return fields[1], fields[4], fields[19]


def register(home, proc=Path("/proc")):
    tty = os.ttyname(0)
    if not re.fullmatch(r"/dev/pts/\d+", tty):
        return None
    info = os.stat(tty)
    if info.st_uid != os.getuid() or not stat.S_ISCHR(info.st_mode):
        return None
    pid = os.getppid()
    # Skip short-lived startup wrappers: retain the outer terminal shell below Kitty.
    candidate = None
    for _ in range(12):
        parent, device, started = identity(pid, proc)
        name = (proc / str(pid) / "comm").read_text().strip()
        if name == "kitty":
            break
        if name in ("fish", "bash", "zsh") and int(device) == info.st_rdev:
            candidate = pid, started
        pid = int(parent)
        if pid <= 1:
            break
    if candidate is None:
        return None
    pid, started = candidate
    record = dict(
        pid=pid,
        started=started,
        tty=tty,
        inode=info.st_ino,
        device=info.st_rdev,
        image=int.from_bytes(os.urandom(4), "big") or 1,
    )
    folder = home / ".cache/nacre/terminal-logos"
    folder.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, temporary = tempfile.mkstemp(dir=folder)
    with os.fdopen(fd, "w") as stream:
        json.dump(record, stream)
    os.chmod(temporary, 0o600)
    os.replace(temporary, folder / (tty.rsplit("/", 1)[1] + ".json"))
    return record


def packet(image, path, edit=False):
    # Editing root frame 1 preserves placements and never moves the text cursor.
    fields = "a=f,r=1,X=1" if edit else "a=T,r=16"
    source = base64.b64encode(os.fsencode(path)).decode()
    return f"\x1b_G{fields},t=f,f=100,i={image},q=2;{source}\x1b\\".encode()


def refresh(home, proc=Path("/proc")):
    folder = home / ".cache/nacre/terminal-logos"
    if not folder.is_dir():
        return
    image = home / ".local/share/nacre/branding/nacre.png"
    for file in sorted(folder.glob("*.json"))[:256]:
        fd = None
        try:
            record = json.loads(file.read_text())
            tty = record["tty"]
            if not re.fullmatch(r"/dev/pts/\d+", tty):
                raise ValueError("Unexpected terminal path")
            _, device, started = identity(int(record["pid"]), proc)
            if started != record["started"] or int(device) != record["device"]:
                raise ValueError("Terminal shell ended or process id reused")
            fd = os.open(tty, os.O_WRONLY | os.O_NONBLOCK | os.O_NOFOLLOW)
            info = os.fstat(fd)
            if (
                not stat.S_ISCHR(info.st_mode)
                or info.st_uid != os.getuid()
                or info.st_ino != record["inode"]
                or info.st_rdev != record["device"]
            ):
                raise ValueError("Terminal device changed")
            if not isinstance(record["image"], int) or not 0 < record["image"] < 2**32:
                raise ValueError("Invalid image id")
            # No image creation, placement, input injection or animation playback.
            # A removed image yields a suppressed ENOENT and stays removed.
            os.write(fd, packet(record["image"], image, edit=True))
        except (OSError, ValueError, KeyError, TypeError):
            file.unlink(missing_ok=True)
        finally:
            if fd is not None:
                os.close(fd)


def render(home=None, prepare=False):
    home = Path.home() if home is None else Path(home)
    image = home / ".local/share/nacre/branding/nacre.png"
    if (
        os.environ.get("TERM") == "xterm-kitty"
        and not os.environ.get("TMUX")
        and not os.environ.get("NO_COLOR")
        and image.is_file()
    ):
        try:
            record = register(home)
            if record:
                data = packet(record["image"], image)
                if prepare:
                    key = (
                        os.environ.get("KITTY_PID", "")
                        + "-"
                        + os.environ.get("KITTY_WINDOW_ID", "")
                    )
                    if not re.fullmatch(r"\d+-\d+", key):
                        return
                    (home / ".cache/nacre/terminal-logos" / (key + ".raw")).write_bytes(
                        data
                    )
                else:
                    sys.stdout.buffer.write(data)
                return
        except (OSError, ValueError):
            pass
    fallback = home / ".local/share/nacre/branding/nacre-text.txt"
    print(fallback.read_text().rstrip() if fallback.is_file() else "Nacre")


if __name__ == "__main__":
    render(prepare="--prepare" in sys.argv)
