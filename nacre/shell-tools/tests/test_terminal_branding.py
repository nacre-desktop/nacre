"""Refresh terminal graphics on palette publication without restarting workers."""

import base64
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import signal
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[3]
import native_branding as branding


class TerminalBrandingTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("fish"), "Fish unavailable")
    def test_nacre_terminal_overrides_distro_greeting_without_changing_user_config(
        self,
    ):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            commands = home / "bin"
            commands.mkdir()
            (commands / "nacre-terminal-logo").write_text(
                '#!/bin/sh\ntest "$1" = --prepare\n'
            )
            (commands / "fastfetch").write_text(
                '#!/bin/sh\ntest "$1" = --config && test "$2" = "$HOME/.config/nacre/terminal/fastfetch.jsonc" || exit 1\nprintf "NACRE_GREETING_OK\\n"\n'
            )
            for path in commands.iterdir():
                path.chmod(0o755)
            result = subprocess.run(
                [
                    "fish",
                    "--no-config",
                    "-c",
                    "function fish_greeting; echo CACHYOS_GREETING; end; source $argv[1]; fish_greeting",
                    str(ROOT / "kitty/greeting.fish"),
                ],
                env={
                    **os.environ,
                    "HOME": str(home),
                    "PATH": str(commands) + os.pathsep + os.environ["PATH"],
                },
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(result.stdout.strip(), "NACRE_GREETING_OK")
            self.assertFalse((home / ".config/fish").exists())

    @unittest.skipUnless(shutil.which("rsvg-convert"), "SVG renderer unavailable")
    def test_prepared_branding_reuses_pixels_and_invalidates_changed_geometry(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            text = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><circle cx="8" cy="8" r="6" fill="red"/></svg>'
            commands = home / "bin"
            commands.mkdir()
            calls = home / "raster-calls"
            wrapper = commands / "rsvg-convert"
            renderer = shutil.which("rsvg-convert")
            wrapper.write_text(
                "#!/usr/bin/env python3\nimport os,sys\n"
                + f"with open({str(calls)!r},'a') as stream: stream.write('call\\n')\n"
                + f"os.execv({renderer!r},[{renderer!r},*sys.argv[1:]])\n"
            )
            wrapper.chmod(0o755)
            with patch.dict(
                os.environ, PATH=str(commands) + os.pathsep + os.environ["PATH"]
            ):
                first = branding.cached_png(text, home, 16)
                self.assertEqual(branding.cached_png(text, home, 16), first)
                self.assertEqual(len(calls.read_text().splitlines()), 1)
                second = branding.cached_png(text.replace("red", "blue"), home, 16)
                self.assertNotEqual(second, first)
                self.assertEqual(len(calls.read_text().splitlines()), 2)
            self.assertFalse((home / ".local/share/nacre/branding").exists())
            self.assertTrue(
                all(
                    (p.stat().st_mode & 0o777) == 0o600
                    for p in (home / ".cache/nacre/branding").glob("*.png")
                )
            )

    @unittest.skipUnless(shutil.which("fastfetch"), "Fastfetch unavailable")
    def test_real_fastfetch_accepts_shipped_config(self):
        result = subprocess.run(
            [
                "fastfetch",
                "--config",
                str(ROOT / "fastfetch/config.jsonc"),
                "--format",
                "json",
            ],
            capture_output=True,
            text=True,
            timeout=15,
            check=True,
        )
        self.assertIsInstance(json.loads(result.stdout), list)
        self.assertNotIn("JsonConfig Error", result.stderr)

    @unittest.skipUnless(shutil.which("fastfetch"), "Fastfetch unavailable")
    def test_fastfetch_labels_use_the_wallpaper_accent_slot(self):
        result = subprocess.run(
            [
                "fastfetch",
                "--config",
                str(ROOT / "fastfetch/config.jsonc"),
                "--logo",
                "none",
                "--pipe",
                "false",
            ],
            capture_output=True,
            text=True,
            timeout=15,
            check=True,
        )
        self.assertIn("\033[34mDesktop", result.stdout)
        self.assertIn("\033[34mTerminal", result.stdout)
        self.assertNotIn("JsonConfig Error", result.stderr)

    @unittest.skipUnless(
        Path("/usr/lib/qt6/bin/qmlformat").exists() or shutil.which("qmlformat"),
        "QML formatter unavailable",
    )
    def test_build_only_is_reproducible_and_does_not_publish_into_home(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "project"
            home = Path(directory) / "home"
            tools = root / "nacre/shell-tools"
            for folder in [
                tools,
                root / "nacre/shell/branding",
                root / "nacre/shell/widgets",
                root / "nacre/login",
                root / "brain/web",
                home,
            ]:
                folder.mkdir(parents=True, exist_ok=True)
            script = tools / "branding.py"
            shutil.copyfile(branding.__file__, script)
            for name in (
                "logo-data.js.in",
                "logo-widget.qml.in",
                *(app + "-label.svg.in" for app in branding.APPLICATIONS),
            ):
                shutil.copyfile(ROOT / "nacre/shell-tools" / name, tools / name)
            shutil.copyfile(
                branding.GEOMETRY, root / "nacre/shell/branding/nacre-master.svg"
            )
            (root / "brain/web/index.html").write_text(
                '<svg><symbol id="sh" viewBox="0 0 34 28"></symbol></svg>'
            )
            env = {**os.environ, "HOME": str(home)}
            paths = [
                root / "nacre/shell/branding/nacre.svg",
                root / "nacre/shell/widgets/BrandLogo.qml",
                root / "nacre/login/Logo.qml",
                root / "brain/web/index.html",
                root / "nacre/shell/branding/nacre-ai.svg",
                root / "nacre/shell/branding/nacre-ai-compact.svg",
                root / "nacre/shell/branding/nacre-ai-symbolic.svg",
                root / "nacre/shell/branding/nacre-notifications.svg",
                root / "nacre/shell/branding/nacre-notifications-compact.svg",
                root / "nacre/shell/branding/nacre-notifications-symbolic.svg",
            ]
            subprocess.run(
                [sys.executable, str(script), "--build"],
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )
            first = {p: p.read_bytes() for p in paths}
            subprocess.run(
                [sys.executable, str(script), "--build"],
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )
            self.assertEqual({p: p.read_bytes() for p in paths}, first)
            self.assertEqual(list(home.rglob("*")), [])
            widget = paths[1].read_text()
            self.assertIn("NacreColours.palette", widget)
            self.assertNotIn("root:/", widget)

    def test_published_lock_logo_is_transparent_and_roles_remain_distinct(self):
        from PIL import Image

        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            branding.publish(
                {"primary": "ff0000", "secondary": "0000ff", "surface": "121212"},
                home,
            )
            folder = home / ".local/share/nacre/branding"
            image = Image.open(folder / "nacre-lock.png")
            self.assertEqual(image.mode, "RGBA")
            self.assertEqual(image.getpixel((0, 0))[3], 0)
            self.assertEqual(image.size, (512, 512))
            self.assertEqual(
                (
                    home / ".local/share/icons/hicolor/scalable/apps/nacre-ai.svg"
                ).read_bytes(),
                (folder / "nacre-ai.svg").read_bytes(),
            )
            pixels = (
                list(image.get_flattened_data())
                if hasattr(image, "get_flattened_data")
                else list(image.getdata())
            )
            self.assertTrue(any(r > b + 60 and a > 240 for r, g, b, a in pixels))
            self.assertTrue(any(b > r + 40 and a > 240 for r, g, b, a in pixels))
            with Image.open(folder / "nacre.png") as terminal:
                self.assertEqual(terminal.mode, "RGBA")
                self.assertEqual(terminal.getpixel((0, 0))[3], 0)
                self.assertEqual(terminal.tobytes(), image.tobytes())
            with Image.open(folder / "nacre-ai.png") as ai:
                self.assertEqual(ai.mode, "RGBA")
                self.assertEqual(ai.getpixel((0, 0))[3], 0)
                self.assertNotEqual(ai.tobytes(), image.tobytes())

    def test_notification_targets_only_exact_menu_controller(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder) / "home"
            menu = home / ".local/bin/nacre-ai"
            menu.parent.mkdir(parents=True)
            marker = Path(folder) / "signals"
            body = (
                "import os,signal,time\n"
                + f"def received(*_):\n with open({str(marker)!r},'a') as stream: stream.write(str(os.getpid())+'\\n')\n"
                + "signal.signal(signal.SIGWINCH, received)\nprint('ready',flush=True)\n"
                + "while True: time.sleep(1)\n"
            )
            menu.write_text(body)
            other = Path(folder) / "other/nacre-ai"
            other.parent.mkdir()
            other.write_text(body)
            children = []
            try:
                for argv in (
                    ["python3", str(menu), "dashboard"],
                    ["python3", str(menu)],
                    ["kitty", str(menu), "dashboard"],
                    ["python3", str(menu), "new"],
                    ["python3", str(other), "dashboard"],
                ):
                    child = subprocess.Popen(
                        argv,
                        executable=sys.executable,
                        env={**os.environ, "HOME": str(home)},
                        stdout=subprocess.PIPE,
                        text=True,
                    )
                    children.append(child)
                    self.assertEqual(child.stdout.readline().strip(), "ready")
                branding.refresh_terminal_menus(home, Path("/proc"))
                deadline = time.monotonic() + 3
                observed = []
                while time.monotonic() < deadline:
                    observed = (
                        marker.read_text().splitlines() if marker.exists() else []
                    )
                    if len(observed) >= 2:
                        break
                    time.sleep(0.01)
                self.assertCountEqual(
                    [int(pid) for pid in observed],
                    [child.pid for child in children[:2]],
                )
            finally:
                for child in children:
                    child.terminate()
                    child.communicate(timeout=5)

    def test_real_idle_curses_menu_reloads_png_after_redraw_signal(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            logo = home / ".local/share/nacre/branding/nacre-ai.png"
            logo.parent.mkdir(parents=True)
            # The PTY captures Kitty's graphics packets; no terminal decoder or
            # authentication/session state is used in this isolated test.
            first, second = b"first-image", b"updated-image"
            logo.write_bytes(first)
            code = """
import importlib.machinery,importlib.util,os,sys
from pathlib import Path
loader=importlib.machinery.SourceFileLoader('menu',sys.argv[1])
spec=importlib.util.spec_from_loader(loader.name,loader)
menu=importlib.util.module_from_spec(spec);loader.exec_module(menu)
menu.Path.home=lambda:Path(sys.argv[2])
original=menu.curses.wrapper
def wrapper(callback):
 def graphical(screen):
  os.environ['TERM']='xterm-kitty'
  return callback(screen)
 return original(graphical)
menu.curses.wrapper=wrapper
menu.select(['First','Second'],'Workspace')
"""
            master, slave = pty.openpty()
            fcntl.ioctl(
                slave,
                __import__("termios").TIOCSWINSZ,
                struct.pack("HHHH", 30, 90, 0, 0),
            )
            process = subprocess.Popen(
                [sys.executable, "-c", code, str(ROOT / "bin/nacre-ai"), str(home)],
                stdin=slave,
                stdout=slave,
                stderr=slave,
                env=dict(os.environ, TERM="xterm-256color", TMUX=""),
            )
            os.close(slave)
            output = bytearray()

            def wait_for(packet):
                until = time.monotonic() + 3
                while time.monotonic() < until:
                    ready, _, _ = select.select([master], [], [], 0.1)
                    if ready:
                        try:
                            output.extend(os.read(master, 65536))
                        except OSError:
                            break
                    if packet in output:
                        return True
                return False

            try:
                self.assertTrue(
                    wait_for(base64.b64encode(first)), output.decode(errors="replace")
                )
                logo.write_bytes(second)
                started = time.monotonic()
                os.kill(process.pid, signal.SIGWINCH)
                self.assertTrue(
                    wait_for(base64.b64encode(second)),
                    "Idle menu did not send updated image",
                )
                self.assertLess(time.monotonic() - started, 2)
                # Only this final exit sends keyboard input; recoloring did not.
                os.write(master, b"\x1b")
                process.wait(timeout=3)
                self.assertEqual(process.returncode, 0)
            finally:
                if process.poll() is None:
                    process.terminate()
                    process.wait(timeout=3)
                os.close(master)
