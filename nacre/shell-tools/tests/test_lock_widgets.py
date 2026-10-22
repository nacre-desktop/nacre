import importlib.util
import json
import tempfile
import unittest
import io
import struct
import zlib
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / file)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


import native_lock_info as info
import native_lock_config as config

devices = module("devices", "device-actions.py")


class LockAndDeviceTests(unittest.TestCase):
    def test_future_snapshot_timestamp_does_not_reuse_private_messages(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory) / "lock-widgets.json"
            cache.write_text(
                json.dumps(
                    {
                        "time": 500,
                        "data": {"notifications": [{"summary": "old private message"}]},
                    }
                )
            )
            with (
                patch.object(info, "CACHE", cache),
                patch.object(info.time, "time", return_value=100),
                patch.object(info, "ipc", return_value='{"notifications":[]}') as fetch,
            ):
                self.assertEqual(info.snapshot(), {"notifications": []})
                fetch.assert_called_once_with("lockWidgets", "state")

    def test_decoder_safety_failure_keeps_the_lock_artwork_fallback(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            source = home / "art.bin"

            # A valid, highly compressed 24 MP PNG exercises the native 20 MP
            # guard without allocating that image in the test driver.
            def chunk(kind, data):
                return (
                    struct.pack(">I", len(data))
                    + kind
                    + data
                    + struct.pack(">I", zlib.crc32(kind + data))
                )

            compressor = zlib.compressobj()
            compressed = (
                b"".join(compressor.compress(b"\0" * 18001) for _ in range(4000))
                + compressor.flush()
            )
            source.write_bytes(
                b"\x89PNG\r\n\x1a\n"
                + chunk(b"IHDR", struct.pack(">IIBBBBB", 6000, 4000, 8, 2, 0, 0, 0))
                + chunk(b"IDAT", compressed)
                + chunk(b"IEND", b"")
            )
            with (
                patch.object(info, "HOME", home),
                patch.object(info, "CACHE", home / "lock-widgets.json"),
            ):
                result = info.artwork({"media": {"art": source.as_uri()}})
            self.assertEqual(
                result, str(home / ".local/share/nacre/branding/nacre-lock.png")
            )
            self.assertFalse(
                json.loads((home / "lock-art-source.json").read_text())["ok"]
            )

    def test_local_artwork_read_is_bounded_before_decoding(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            source = home / "oversized.png"
            raw = io.BytesIO()
            info.Image.new("RGB", (1, 1), "black").save(raw, "PNG")
            # This PNG would decode successfully if the byte budget were absent.
            source.write_bytes(raw.getvalue() + b"x" * (4000002 - len(raw.getvalue())))
            with (
                patch.object(info, "HOME", home),
                patch.object(info, "CACHE", home / "lock-widgets.json"),
            ):
                result = info.artwork({"media": {"art": source.as_uri()}})
            self.assertEqual(
                result, str(home / ".local/share/nacre/branding/nacre-lock.png")
            )
            self.assertFalse(
                json.loads((home / "lock-art-source.json").read_text())["ok"]
            )
            self.assertFalse(list(home.glob("lock-art-*.png")))

    def test_notification_privacy_is_enforced_again_after_cache_read(self):
        data = {
            "count": 1,
            "notifications": [
                {"app": "Mail", "summary": "Private title", "body": "Private body"}
            ],
        }
        text = info.label("notifications", data, {})
        self.assertNotIn("Private", text)
        self.assertIn("Mail", text)
        self.assertIn(
            "Private title",
            info.label("notifications", data, {"lockNotificationContents": True}),
        )
        self.assertEqual(
            info.label("notifications", data, {"lockNotifications": False}), ""
        )

    def test_untrusted_metadata_is_escaped_for_pango(self):
        text = info.label(
            "media",
            {
                "media": {
                    "title": '<span foreground="red">Oops</span>',
                    "artist": "A & B",
                }
            },
            {},
        )
        self.assertNotIn("<span", text)
        self.assertIn("&lt;span", text)
        self.assertIn("A &amp; B", text)

    def test_render_respects_preferences_and_has_no_unlock_action(self):
        colors = json.loads((ROOT / "reference-style.json").read_text())["colours"]
        output = config.render(
            colors,
            "/tmp/wallpaper.png",
            {"lockMedia": False, "lockWeather": False},
            Path("/tmp/lock-info.py"),
        )
        self.assertNotIn("skip_next", output)
        self.assertNotIn("weather\n", output)
        self.assertIn("Notifications", info.label("notifications", {}, {}))
        self.assertIn("input-field", output)
        self.assertNotIn("{{", output)
        self.assertNotIn("unlock-session", output)
        with self.assertRaises(ValueError):
            config.render(colors, "/tmp/evil\nlabel {}", {}, Path("/tmp/helper.py"))

    def test_long_labels_fit_the_card_and_are_bounded(self):
        import html

        for font in (14, 16):
            text = html.unescape(info.plain("WWWW" * 200, lines=2, font=font))
            self.assertLessEqual(len(text.splitlines()), 2)
            self.assertTrue(text.endswith("…"))
            self.assertTrue(
                all(
                    info.text_font(font).getlength(line) <= 288
                    for line in text.splitlines()
                )
            )

    def test_media_controls_scale_with_the_shared_panel_on_4k(self):
        colors = json.loads((ROOT / "reference-style.json").read_text())["colours"]
        for width, height in ((1920, 1080), (2880, 1800), (3840, 2160), (7680, 4320)):
            output = config.render(
                colors,
                "/tmp/wall.png",
                {},
                Path("/tmp/helper"),
                [dict(name="test", width=width, height=height)],
            )
            scale = min(width / 2048, height / 1152)
            # Each control must be centered on its prepared rounded button.
            for x, _action in ((165, "previous"), (238, "toggle"), (311, "next")):
                pattern = f"position = {round((x - 720) * scale)}, {round((405 - 687) * scale)}"
                self.assertIn(pattern, output)
            self.assertEqual(output.count("input-field {"), 1)
            self.assertNotIn("unlock-session", output)

    def test_device_names_are_arguments_and_actions_are_bounded(self):
        self.assertEqual(
            devices.command("wifi-connect", "$(anything); space")[-1],
            "$(anything); space",
        )
        self.assertIn("--ask", devices.command("wifi-connect", "WiFi"))
        with self.assertRaises(ValueError):
            devices.command("bluetooth-connect", "AA:BB:CC;anything")
        with self.assertRaises(ValueError):
            devices.command("shell", "anything")
