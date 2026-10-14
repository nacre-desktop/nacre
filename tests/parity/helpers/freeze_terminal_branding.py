#!/usr/bin/env python3
"""Freeze terminal branding before retiring its Python implementation."""

import base64
from contextlib import redirect_stdout
import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import pty
import select
import stat
import sys
import tempfile
from types import SimpleNamespace
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / "nacre/shell-tools/terminal-branding.py"
SOURCE_SHA = "87b2bd02c82d8abb87aa0f329562ac7237347ad5cac222c230d61d3013795f90"
DESTINATION = (
    ROOT / "crates/nacre-helper/tests/fixtures/terminal-branding-reference.json"
)


def proc_files(root, processes):
    for row in processes:
        folder = root / str(row["pid"])
        folder.mkdir(parents=True)
        fields = ["0"] * 25
        fields[0], fields[1], fields[4], fields[19] = (
            "S",
            str(row["parent"]),
            str(row["device"]),
            row["started"],
        )
        (folder / "stat").write_text(
            f"{row['pid']} ({row['name']}) " + " ".join(fields)
        )
        (folder / "comm").write_text(row["name"] + "\n")


def snapshot(home):
    result = {}
    for path in sorted(home.rglob("*")):
        if path.is_file():
            result[str(path.relative_to(home))] = {
                "bytes": list(path.read_bytes()),
                "mode": path.stat().st_mode & 0o777,
            }
    return result


def capture():
    if DESTINATION.exists():
        raise ValueError("Refusing to overwrite terminal reference")
    if hashlib.sha256(SOURCE.read_bytes()).hexdigest() != SOURCE_SHA:
        raise ValueError("Terminal reference source changed")
    sys.path.insert(0, str(SOURCE.parent))
    spec = importlib.util.spec_from_file_location("terminal_reference", SOURCE)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    cases = []
    for name in ("bash", "name ) with spaces"):
        with tempfile.TemporaryDirectory(prefix="nacre-terminal-identity-") as folder:
            proc = Path(folder)
            row = dict(pid=500, parent=499, device=1234, started="41", name=name)
            proc_files(proc, [row])
            cases.append(
                {
                    "operation": "identity",
                    "input": row,
                    "observed": list(module.identity(500, proc)),
                }
            )
    for image in (0, 1, 234, 2**32 - 1, True, -3):
        for edit in (False, True):
            path = "/fixture/home/a logo λ.png"
            cases.append(
                {
                    "operation": "packet",
                    "image": image,
                    "path": path,
                    "edit": edit,
                    "observed": list(module.packet(image, path, edit)),
                }
            )
    processes = [
        dict(pid=500, parent=499, device=1234, started="41", name="bash"),
        dict(pid=499, parent=498, device=1234, started="40", name="fish"),
        dict(pid=498, parent=1, device=0, started="39", name="kitty"),
    ]
    for name, changes in [
        ("outer-shell", {}),
        ("zero-image", {"image": 0}),
        ("non-pty", {"tty": "/dev/tty1"}),
        ("foreign-tty", {"owned": False}),
        ("regular-file", {"character": False}),
        ("no-shell", {"processes": [dict(row, name="wrapper") for row in processes]}),
        (
            "mismatched-device",
            {"processes": [dict(row, device=0) for row in processes]},
        ),
        ("parent-ended", {"processes": [dict(processes[0], parent=1)]}),
    ]:
        request = dict(
            tty="/dev/pts/987",
            owned=True,
            character=True,
            inode=55,
            device=1234,
            parent=500,
            image=7,
            processes=processes,
        )
        request.update(changes)
        with tempfile.TemporaryDirectory(prefix="nacre-terminal-register-") as folder:
            home, proc = Path(folder) / "home", Path(folder) / "proc"
            home.mkdir()
            proc_files(proc, request["processes"])
            original_stat = os.stat
            tty_info = SimpleNamespace(
                st_uid=os.getuid() if request["owned"] else os.getuid() + 1,
                st_mode=(stat.S_IFCHR if request["character"] else stat.S_IFREG)
                | 0o600,
                st_ino=request["inode"],
                st_rdev=request["device"],
            )

            def metadata(
                path,
                *args,
                tty_info=tty_info,
                request=request,
                original_stat=original_stat,
                **kwargs,
            ):
                return (
                    tty_info
                    if str(path) == request["tty"]
                    else original_stat(path, *args, **kwargs)
                )

            with (
                patch.object(module.os, "ttyname", return_value=request["tty"]),
                patch.object(module.os, "getppid", return_value=request["parent"]),
                patch.object(
                    module.os,
                    "urandom",
                    return_value=request["image"].to_bytes(4, "big"),
                ),
                patch.object(module.os, "stat", side_effect=metadata),
            ):
                record = module.register(home, proc)
            cases.append(
                {
                    "operation": "register",
                    "name": name,
                    "input": request,
                    "observed": record,
                    "files": snapshot(home),
                }
            )
    for name, change in [
        ("valid", {}),
        ("reused", {"started": "42"}),
        ("integer-start", {"started": 41}),
        ("inode-changed", {"inode": "increment"}),
        ("device-string", {"device": "string"}),
        ("zero-image", {"image": 0}),
        ("big-image", {"image": 2**32}),
        ("negative-image", {"image": -1}),
        ("string-image", {"image": "234"}),
        ("bool-image", {"image": True}),
        ("float-image", {"image": 234.0}),
        ("foreign-path", {"tty": "/tmp/anything"}),
        ("string-pid", {"pid": "123"}),
    ]:
        master, slave = pty.openpty()
        try:
            tty, meta = os.ttyname(slave), os.fstat(slave)
            with tempfile.TemporaryDirectory(
                prefix="nacre-terminal-refresh-"
            ) as folder:
                home = Path(folder)
                cache = home / ".cache/nacre/terminal-logos"
                cache.mkdir(parents=True)
                record = dict(
                    pid=123,
                    started="41",
                    tty=tty,
                    inode=meta.st_ino,
                    device=meta.st_rdev,
                    image=234,
                )
                for key, value in change.items():
                    record[key] = (
                        record[key] + 1
                        if value == "increment"
                        else str(record[key])
                        if value == "string"
                        else value
                    )
                target = cache / "test.json"
                target.write_text(json.dumps(record))
                with patch.object(
                    module, "identity", return_value=("1", str(meta.st_rdev), "41")
                ):
                    module.refresh(home)
                output = (
                    os.read(master, 4096)
                    if select.select([master], [], [], 0.02)[0]
                    else b""
                )
                output = output.replace(
                    base64.b64encode(
                        os.fsencode(home / ".local/share/nacre/branding/nacre.png")
                    ),
                    base64.b64encode(
                        b"/fixture/home/.local/share/nacre/branding/nacre.png"
                    ),
                )
                cases.append(
                    {
                        "operation": "refresh",
                        "name": name,
                        "changes": change,
                        "retained": target.exists(),
                        "observed": list(output),
                    }
                )
        finally:
            os.close(master)
            os.close(slave)
    with tempfile.TemporaryDirectory(prefix="nacre-terminal-bound-") as folder:
        home = Path(folder)
        cache = home / ".cache/nacre/terminal-logos"
        cache.mkdir(parents=True)
        for index in range(260):
            (cache / f"{index:03}.json").write_text("{}")
        module.refresh(home)
        cases.append(
            {
                "operation": "bounded",
                "observed": sorted(path.name for path in cache.iterdir()),
            }
        )
    for term, tmux, no_color, text, prepare, key in [
        ("", "", "", None, False, "100-1"),
        ("xterm-kitty", "tmux", "", "Public logo\n\n", False, "100-1"),
        ("xterm-kitty", "", "1", "λ logo \n", False, "100-1"),
        ("xterm-kitty", "", "", None, False, "100-1"),
        ("xterm-kitty", "", "", None, True, "100-1"),
        ("xterm-kitty", "", "", None, True, "bad-key"),
    ]:
        with tempfile.TemporaryDirectory(prefix="nacre-terminal-render-") as folder:
            home = Path(folder)
            art = home / ".local/share/nacre/branding"
            art.mkdir(parents=True)
            (art / "nacre.png").write_bytes(b"fixture-image")
            if text is not None:
                (art / "nacre-text.txt").write_text(text)
            (home / ".cache/nacre/terminal-logos").mkdir(parents=True)
            env = dict(
                TERM=term,
                TMUX=tmux,
                NO_COLOR=no_color,
                KITTY_PID=key.split("-", 1)[0],
                KITTY_WINDOW_ID=key.split("-", 1)[-1],
            )
            output = io.BytesIO()
            stream = io.TextIOWrapper(output, encoding="utf-8", write_through=True)
            with (
                patch.dict(os.environ, env, clear=True),
                patch.object(module, "register", return_value={"image": 234}),
                redirect_stdout(stream),
            ):
                module.render(home, prepare)
            raw = output.getvalue()
            actual = base64.b64encode(os.fsencode(art / "nacre.png"))
            fixed = base64.b64encode(
                b"/fixture/home/.local/share/nacre/branding/nacre.png"
            )
            files = snapshot(home)
            for row in files.values():
                row["bytes"] = list(bytes(row["bytes"]).replace(actual, fixed))
            cases.append(
                {
                    "operation": "render",
                    "input": dict(environment=env, text=text, prepare=prepare),
                    "observed": list(raw.replace(actual, fixed)),
                    "files": files,
                }
            )
            stream.detach()
    DESTINATION.write_text(
        json.dumps(
            {"source_sha256": SOURCE_SHA, "cases": cases}, indent=2, ensure_ascii=False
        )
        + "\n"
    )
    print(f"Frozen {len(cases)} terminal branding cases")


if __name__ == "__main__":
    capture()
