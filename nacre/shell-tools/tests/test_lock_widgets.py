import importlib.util
import json
import tempfile
import unittest
import io
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def module(name, file):
    spec = importlib.util.spec_from_file_location(name, ROOT / file)
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


info = module("lockinfo", "lock-info.py")
import native_lock_config as config

devices = module("devices", "device-actions.py")
weather = module("weather", "weather.py")


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
            source.write_bytes(b"fixture")
            with (
                patch.object(info, "HOME", home),
                patch.object(info, "CACHE", home / "lock-widgets.json"),
                patch.object(
                    info.Image,
                    "open",
                    side_effect=info.Image.DecompressionBombError("Unsafe image"),
                ),
            ):
                result = info.artwork({"media": {"art": source.as_uri()}})
            self.assertEqual(
                result, str(home / ".local/share/nacre/branding/nacre-lock.png")
            )
            self.assertFalse(
                json.loads((home / "lock-art-source.json").read_text())["ok"]
            )

    def test_local_artwork_read_is_bounded_before_decoding(self):
        class BoundedStream(io.BytesIO):
            def read(self, size=-1):
                self.observed.append(size)
                return super().read(size)

        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            cache = home / "lock-widgets.json"
            stream = BoundedStream(b"x" * 4000002)
            stream.observed = []
            original = Path.open

            def open_file(path, *args, **kwargs):
                return (
                    stream
                    if str(path) == "/isolated-art.bin"
                    else original(path, *args, **kwargs)
                )

            with (
                patch.object(info, "HOME", home),
                patch.object(info, "CACHE", cache),
                patch.object(Path, "open", open_file),
                patch.object(info.Image, "open") as decoder,
            ):
                result = info.artwork({"media": {"art": "file:///isolated-art.bin"}})
            self.assertEqual(stream.observed, [4000001])
            decoder.assert_not_called()
            self.assertEqual(
                result, str(home / ".local/share/nacre/branding/nacre-lock.png")
            )

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

    def test_weather_prepares_feels_like_and_daily_range(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            settings = home / ".config/nacre/desktop.json"
            settings.parent.mkdir(parents=True)
            settings.write_text(json.dumps(dict(weatherLocation="Test city")))
            forecast = {
                "current": dict(
                    weather_code=0,
                    time="2099-10-11T09:00",
                    temperature_2m=28,
                    apparent_temperature=32,
                    relative_humidity_2m=50,
                    wind_speed_10m=8,
                    wind_direction_10m=90,
                    surface_pressure=1010,
                    is_day=1,
                ),
                "daily": dict(
                    time=["2099-10-11"],
                    temperature_2m_min=[27],
                    temperature_2m_max=[34],
                    weather_code=[0],
                    precipitation_probability_max=[0],
                    sunrise=["2099-10-11T07:00"],
                    sunset=["2099-10-11T18:00"],
                    uv_index_max=[2],
                ),
                "hourly": dict(
                    time=["2099-10-11T09:00"],
                    temperature_2m=[28],
                    precipitation_probability=[0],
                    weather_code=[0],
                ),
            }
            with (
                patch.object(weather, "HOME", home),
                patch.object(weather, "CACHE", home / "weather.json"),
                patch.object(
                    weather,
                    "fetch_json",
                    side_effect=[
                        {"results": [{"latitude": 59, "longitude": 10}]},
                        forecast,
                        {"current": {"us_aqi": 20}},
                    ],
                ),
            ):
                data = weather.refresh()
            self.assertEqual(data["feelsLike"], 32)
            self.assertEqual((data["high"], data["low"]), (34, 27))
            self.assertFalse(data["stale"])
            self.assertEqual(data["description"], "Clear")

    def test_weather_failure_preserves_cache_timestamp(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            cache = home / "weather.json"
            cache.write_text(
                json.dumps(
                    {"checked": 123, "description": "Overcast", "temperature": 10}
                )
            )
            with (
                patch.object(weather, "HOME", home),
                patch.object(weather, "CACHE", cache),
                patch.object(weather, "fetch_json", side_effect=TimeoutError),
            ):
                data = weather.refresh()
            self.assertEqual(data["checked"], 123)
            self.assertTrue(data["stale"])
            self.assertEqual(data["temperature"], 10)
