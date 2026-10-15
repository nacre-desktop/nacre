"""Verify real output geometry, ready presentation privacy and transparent fallback."""

import importlib.util
import json
import os
import sys
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from PIL import Image

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    if name in {"branding", "lock-config"}:
        import native_branding
        import native_lock_config

        return native_branding if name == "branding" else native_lock_config
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
            monitors = [dict(name="eDP-1", width=2880, height=1800, scale=1.5)]
            sys.path.insert(0, str(ROOT.parents[1] / "tests/parity/helpers"))
            from native_compositor_peer import Peer

            (home / "effects.jsonl").touch()
            peer = Peer(
                home, [{"argv": ["monitors", "-j"], "stdout": json.dumps(monitors)}]
            )
            environment = {
                "XDG_RUNTIME_DIR": str(home / "runtime"),
                "HYPRLAND_INSTANCE_SIGNATURE": "fixture",
                "NACRE_COMPOSITOR": "hyprland",
            }
            try:
                with patch.dict(os.environ, environment):
                    text = config.prepare_config(
                        colors, "/tmp/wall.jpg", {}, ROOT / "lock-info.py", home
                    )
            finally:
                peer.close()
                sys.path.pop(0)
            effects = [
                json.loads(line)
                for line in (home / "effects.jsonl").read_text().splitlines()
            ]
            self.assertEqual(
                effects,
                [{"command": "hyprctl", "argv": ["monitors", "-j"], "stdin": ""}],
            )
            self.assertIn("monitor = eDP-1", text)
            self.assertIn("position = -678, -397", text)
            self.assertNotIn("position = -452, -280", text)
            with patch.dict(os.environ, environment):
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
            brand.publish(colors, home)
            root = home / ".local/share/nacre/branding"
            with Image.open(root / "nacre-lock.png") as image:
                self.assertEqual(image.mode, "RGBA")
                self.assertEqual(image.getpixel((0, 0))[3], 0)
                self.assertGreater(image.getchannel("A").getextrema()[1], 0)
            with Image.open(root / "nacre.png") as image:
                self.assertEqual(image.mode, "RGBA")
                self.assertEqual(image.getpixel((0, 0))[3], 0)

    def test_paused_lock_theme_retains_colors_while_widgets_update(self):
        prepare = load("lock-prepare")
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            ready = home / ".cache/nacre/lock-ready"
            ready.mkdir(parents=True)
            retained = json.loads((ROOT / "reference-style.json").read_text())[
                "colours"
            ]
            (ready / "snapshot.json").write_text(json.dumps({"colors": retained}))
            prefs = home / ".config/nacre/colors.json"
            prefs.parent.mkdir(parents=True)
            prefs.write_text(json.dumps({"version": 1, "appThemes": {"lock": False}}))
            changed = dict(retained, primary="abcdef")
            with patch.object(prepare.dashboard, "publish") as publish:
                prepare.prepare(
                    {
                        "colors": changed,
                        "preferences": {},
                        "weather": {"description": "New weather"},
                    },
                    home,
                )
            self.assertEqual(publish.call_args.args[1], retained)
            snapshot = json.loads((ready / "snapshot.json").read_text())
            self.assertEqual(snapshot["colors"], retained)
            self.assertEqual(snapshot["weather"]["description"], "New weather")
            prefs.write_text(json.dumps({"version": 1, "appThemes": {"lock": True}}))
            with patch.object(prepare.dashboard, "publish") as publish:
                prepare.prepare({"colors": changed, "preferences": {}}, home)
            self.assertEqual(publish.call_args.args[1], changed)

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
