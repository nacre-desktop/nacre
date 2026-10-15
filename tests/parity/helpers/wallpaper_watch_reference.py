"""Disposable event dialogue shared by the frozen producer and native replay."""

import json
import os
from pathlib import Path
import select
import subprocess
import tempfile
import time


def subscribed(process, directory):
    deadline = time.monotonic() + 10
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise AssertionError("Watcher exited before subscribing")
        if directory.exists():
            inode = directory.stat().st_ino
            for path in (Path("/proc") / str(process.pid) / "fdinfo").iterdir():
                try:
                    text = path.read_text()
                except FileNotFoundError:
                    continue
                if any(
                    int(field[4:], 16) == inode
                    for line in text.splitlines()
                    if line.startswith("inotify ")
                    for field in line.split()
                    if field.startswith("ino:")
                ):
                    return
        time.sleep(0.01)
    raise AssertionError("Watcher did not subscribe to the fixture directory")


def capture(command):
    # Frozen file modes use 022; the CI container may inherit 002.
    # Scope the fixture umask and restore it even when capture fails.
    previous = os.umask(0o022)
    try:
        return _capture(command)
    finally:
        os.umask(previous)


def _capture(command):
    with tempfile.TemporaryDirectory(prefix="nacre-watch-reference-") as folder:
        home = Path(folder) / "home"
        home.mkdir(mode=0o700)
        root, scheme = (
            home / "Pictures/Wallpapers",
            home / ".local/state/nacre/scheme.json",
        )
        engine = home / ".local/share/nacre/shell/bin/nacre_shell"
        config = home / ".config/nacre"
        config.mkdir(parents=True)
        engine.parent.mkdir(parents=True)
        env = dict(
            os.environ,
            HOME=str(home),
            XDG_CONFIG_HOME=str(home / ".config"),
            XDG_STATE_HOME=str(home / ".local/state"),
            XDG_CACHE_HOME=str(home / ".cache"),
        )
        process = subprocess.Popen(
            [*command, "--root", str(root), "--scheme", str(scheme)],
            env=env,
            cwd=home,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            bufsize=0,
        )
        buffer = b""
        steps = []

        def packet(timeout):
            nonlocal buffer
            deadline = time.monotonic() + timeout
            while b"\n" not in buffer:
                remaining = deadline - time.monotonic()
                if (
                    remaining <= 0
                    or not select.select([process.stdout], [], [], remaining)[0]
                ):
                    return None
                raw = os.read(process.stdout.fileno(), 65536)
                if not raw:
                    raise AssertionError("Watcher closed its event stream")
                buffer += raw
            line, buffer = buffer.split(b"\n", 1)
            return (line + b"\n").decode()

        def step(name, action, changed):
            action()
            reply = packet(5 if changed else 0.8)
            if bool(reply) != changed:
                raise AssertionError(f"Unexpected event at {name}: {reply!r}")
            if reply and json.loads(reply) != {"changed": True}:
                raise AssertionError("Invalid watcher packet")
            steps.append(dict(name=name, stdout=reply or ""))
            if changed and packet(0.7) is not None:
                raise AssertionError(f"Undebounced extra event at {name}")

        try:
            subscribed(process, root)
            step(
                "ignored-extension",
                lambda: (root / "readme.txt").write_text("ignored"),
                False,
            )

            def same_scheme():
                # A creation event arrives before the content. Hold that state
                # briefly to exercise the write boundary deterministically.
                with scheme.open("w") as stream:
                    time.sleep(0.05)
                    stream.write('{"mode":"dark","colours":{"primary":"ffffff"}}')

            step("same-scheme", same_scheme, False)
            step("changed-mode", lambda: scheme.write_text('{"mode":"light"}'), True)
            step(
                "changed-flavour",
                lambda: scheme.write_text('{"mode":"light","flavour":"hard"}'),
                True,
            )
            step(
                "unchanged-tuple",
                lambda: scheme.write_text(
                    '{"mode":"light","flavour":"hard","colours":{}}'
                ),
                False,
            )
            step(
                "picker-preference",
                lambda: (config / "wallpaper-picker.json").write_text("{}"),
                True,
            )
            step("cli-preference", lambda: (config / "cli.json").write_text("{}"), True)
            step(
                "unrelated-config",
                lambda: (config / "colors.json").write_text("{}"),
                False,
            )
            step(
                "engine-replacement", lambda: engine.write_text("native wrapper"), True
            )
            nested = root / "collection"
            nested.mkdir()
            subscribed(process, nested)

            def burst():
                for text in ("first", "second", "final"):
                    (nested / "scene.PNG").write_text(text)

            step("nested-burst", burst, True)
            outside = home / "unwatched"
            outside.mkdir()
            (root / "alias").symlink_to(outside)
            step(
                "symlink-not-followed",
                lambda: (outside / "scene.jpg").write_text("outside"),
                False,
            )
            step(
                "video-extension",
                lambda: (root / "scene.MP4").write_text("video"),
                True,
            )

            def atomic_image():
                temporary = root / "incoming.tmp"
                temporary.write_text("image")
                temporary.replace(root / "scene.webp")

            step("atomic-move", atomic_image, True)
            step("image-removed", lambda: (root / "scene.webp").unlink(), True)
            step(
                "library-renamed", lambda: root.rename(root.with_name("Archived")), True
            )
        finally:
            process.terminate()
            process.wait(timeout=5)
            stderr = process.stderr.read().decode()
            process.stdout.close()
            process.stderr.close()
        files = {}
        for path in sorted(home.rglob("*")):
            name = str(path.relative_to(home))
            mode = path.lstat().st_mode & 0o777
            if path.is_symlink():
                files[name] = {
                    "link": str(path.readlink()).replace(str(home), "/fixture/home")
                }
            elif path.is_dir():
                files[name] = {"directory": True, "mode": mode}
            else:
                files[name] = {"text": path.read_text(), "mode": mode}
        return dict(steps=steps, exit=process.returncode, stderr=stderr, files=files)
