"""Verify real output geometry, ready presentation privacy and transparent fallback."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), ROOT / (name + ".py")
    )
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


class LockReadyTests(unittest.TestCase):
    def test_production_config_uses_live_and_cached_monitor_geometry(self):
        config = load("lock-config")
        colors = json.loads((ROOT / "reference-style.json").read_text())["colours"]
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            output = json.dumps(
                [dict(name="eDP-1", width=2880, height=1800, scale=1.5)]
            )
            real_check_output = config.subprocess.check_output

            def fetch(command, **kwargs):
                return (
                    output
                    if command[0] == "hyprctl"
                    else real_check_output(command, **kwargs)
                )

            with patch.object(config.subprocess, "check_output", side_effect=fetch):
                text = config.prepare_config(
                    colors, "/tmp/wall.jpg", {}, ROOT / "lock-info.py", home
                )
            self.assertIn("monitor = eDP-1", text)
            self.assertIn("position = -678, -397", text)
            self.assertNotIn("position = -452, -280", text)
            with patch.object(config.subprocess, "check_output", side_effect=OSError):
                self.assertEqual(
                    config.prepare_config(
                        colors, "/tmp/wall.jpg", {}, ROOT / "lock-info.py", home
                    ),
                    text,
                )

    def test_lock_and_terminal_logos_preserve_alpha(self):
        brand = load("branding")
        colors = json.loads((ROOT / "reference-style.json").read_text())["colours"]
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            with patch.object(brand, "refresh_terminal_menus"):
                brand.publish(colors, home)
            root = home / ".local/share/nacre/branding"
            with Image.open(root / "nacre-lock.png") as image:
                self.assertEqual(image.mode, "RGBA")
                self.assertEqual(image.getpixel((0, 0))[3], 0)
                self.assertGreater(image.getchannel("A").getextrema()[1], 0)
            with Image.open(root / "nacre.png") as image:
                self.assertEqual(image.mode, "RGBA")
                self.assertEqual(image.getpixel((0, 0))[3], 0)

    def test_ready_labels_separate_safe_notifications_and_preload_art(self):
        prepare = load("lock-prepare")
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            logo = home / ".local/share/nacre/branding/nacre-lock.png"
            logo.parent.mkdir(parents=True)
            Image.new("RGBA", (20, 20), (0, 0, 0, 0)).save(logo)
            data = dict(
                count=1,
                notifications=[
                    dict(app="Mail", summary="Private title", body="Private body")
                ],
                preferences={},
            )
            with patch.object(
                prepare.info,
                "ipc",
                side_effect=AssertionError("Preparation must not query IPC"),
            ):
                prepare.prepare(data, home)
            ready = home / ".cache/nacre/lock-ready"
            self.assertNotIn("Private", (ready / "notifications-safe.txt").read_text())
            self.assertIn(
                "Private title", (ready / "notifications-private.txt").read_text()
            )
            self.assertEqual(
                (ready / "initial-art.png").read_bytes(), logo.read_bytes()
            )
            self.assertEqual((ready / "media.txt").stat().st_mode & 0o777, 0o600)
