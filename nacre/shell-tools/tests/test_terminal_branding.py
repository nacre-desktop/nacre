"""Refresh terminal graphics on palette publication without restarting workers."""

import base64
import fcntl
import importlib.util
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
spec = importlib.util.spec_from_file_location(
    "branding", ROOT / "nacre/shell-tools/branding.py"
)
branding = importlib.util.module_from_spec(spec)
spec.loader.exec_module(branding)


class TerminalBrandingTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("rsvg-convert"), "SVG renderer unavailable")
    def test_prepared_branding_reuses_pixels_and_invalidates_changed_geometry(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            text = '<svg xmlns="http://www.w3.org/2000/svg" width="16" height="16"><circle cx="8" cy="8" r="6" fill="red"/></svg>'
            with patch.object(branding, "raster", wraps=branding.raster) as render:
                first = branding.cached_png(text, home, 16)
                self.assertEqual(branding.cached_png(text, home, 16), first)
                self.assertEqual(render.call_count, 1)
                second = branding.cached_png(text.replace("red", "blue"), home, 16)
                self.assertNotEqual(second, first)
                self.assertEqual(render.call_count, 2)
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
                shutil.copyfile(Path(branding.__file__).with_name(name), tools / name)
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
            with patch.object(branding, "refresh_terminal_menus"):
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
            proc = Path(folder) / "proc"
            proc.mkdir()
            menu = str(home / ".local/bin/nacre-ai")
            for pid, argv in [
                (111, ["python3", menu, "dashboard"]),
                (112, ["python3", menu]),
                (222, ["kitty", menu, "dashboard"]),
                (333, ["python3", menu, "new"]),
                (444, ["python3", "/other/nacre-ai", "dashboard"]),
            ]:
                path = proc / str(pid)
                path.mkdir()
                (path / "cmdline").write_bytes(b"\0".join(a.encode() for a in argv))
            with (
                patch.object(branding.os, "pidfd_open", return_value=7) as opened,
                patch.object(branding.signal, "pidfd_send_signal") as notified,
                patch.object(branding.os, "close") as closed,
            ):
                branding.refresh_terminal_menus(home, proc)
            self.assertCountEqual(
                [call.args[0] for call in opened.call_args_list], [111, 112]
            )
            self.assertEqual(notified.call_count, 2)
            notified.assert_any_call(7, signal.SIGWINCH)
            self.assertEqual(closed.call_count, 2)

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
