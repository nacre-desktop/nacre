"""Provider boundaries: persistent deadlines, private config and cached forecasts."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from zoneinfo import ZoneInfoNotFoundError
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def module(name):
    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), ROOT / (name + ".py")
    )
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


data = module("dashboard-data")
weather = module("weather")
media = module("dashboard-media")
system = module("dashboard-system")


class PearlDashboardDataTests(unittest.TestCase):
    def test_tabs_and_preferences_reject_invalid_values(self):
        self.assertEqual(
            data.validate({"tabs": ["focus", "today"]})["tabs"], ["focus", "today"]
        )
        for value in ([], ["today", "today"], ["unknown"], [{}]):
            with self.assertRaises((ValueError, TypeError)):
                data.validate({"tabs": value})
        for value in (0, 181, True, "25"):
            with self.assertRaises(ValueError):
                data.validate({"focusMinutes": value})
        with self.assertRaises(ValueError):
            data.validate({"calendarFile": "relative.ics"})
        with self.assertRaises(ZoneInfoNotFoundError):
            data.validate({"homeZone": "bad/zone"})

    def test_private_writes_and_focus_deadline_survive_pause_and_resume(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.object(data, "STATE", Path(directory) / "focus.json"),
        ):
            state = data.focus_action("start", 25, False, now=100)
            self.assertEqual(state["deadline"], 1600)
            self.assertTrue(state["ownsDnd"])
            state = data.focus_action("pause", now=400)
            self.assertEqual(state["remaining"], 1200)
            state = data.focus_action("resume", now=900)
            self.assertEqual(state["deadline"], 2100)
            state = data.focus_action("release", now=1000)
            self.assertFalse(state["ownsDnd"])
            state = data.focus_action("complete", now=2200)
            self.assertEqual(state["status"], "done")
            self.assertFalse(state["ownsDnd"])
            self.assertEqual(data.STATE.stat().st_mode & 0o777, 0o600)
            self.assertEqual(list(Path(directory).glob(".focus.json*")), [])

    def test_local_calendar_unfolds_titles_dates_and_excludes_cancelled(self):
        with tempfile.TemporaryDirectory() as directory:
            file = Path(directory) / "events.ics"
            file.write_text(
                "BEGIN:VCALENDAR\nBEGIN:VEVENT\nDTSTART:20991011T100000Z\nSUMMARY:Project\\,\n review\nEND:VEVENT\nBEGIN:VEVENT\nDTSTART:20991011T120000Z\nSTATUS:CANCELLED\nSUMMARY:Cancelled\nEND:VEVENT\nEND:VCALENDAR"
            )
            now = data.dt.datetime(2099, 10, 11, tzinfo=data.dt.timezone.utc)
            result = data.calendar(str(file), now)
            self.assertEqual(len(result["events"]), 1)
            self.assertEqual(result["events"][0]["title"], "Project,review")
            file.write_text("x" * 2_000_001)
            self.assertIn("2MB", data.calendar(str(file), now)["error"])

    def test_forecast_uses_current_day_hours_and_ten_day_shape(self):
        source = {
            "current": dict(
                weather_code=61,
                time="2099-10-11T09:00",
                temperature_2m=14,
                apparent_temperature=13,
                relative_humidity_2m=75,
                wind_speed_10m=8,
                wind_direction_10m=90,
                surface_pressure=1010,
                is_day=1,
            ),
            "daily": dict(
                time=["2099-10-11"],
                temperature_2m_min=[9],
                temperature_2m_max=[18],
                weather_code=[61],
                precipitation_probability_max=[85],
                sunrise=["2099-10-11T07:00"],
                sunset=["2099-10-11T18:00"],
                uv_index_max=[2],
            ),
            "hourly": dict(
                time=["2099-10-11T08:00", "2099-10-11T09:00", "2099-10-11T10:00"],
                temperature_2m=[12, 14, 15],
                precipitation_probability=[90, 85, 70],
                weather_code=[61, 61, 63],
            ),
            "timezone": "Europe/Oslo",
        }
        result = weather.normalize(source, "Oslo", 59, 10, now=100)
        self.assertEqual(result["temperature"], 14)
        self.assertEqual(result["code"], "296")
        self.assertEqual(len(result["hours"]), 2)
        self.assertEqual(result["days"][0]["low"], 9)
        self.assertEqual(result["checked"], 100)

    def test_failed_refresh_keeps_original_check_time_and_marks_stale(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.object(weather, "HOME", Path(directory)),
            patch.object(weather, "CACHE", Path(directory) / "weather.json"),
            patch.object(weather, "fetch_json", side_effect=OSError("offline")),
        ):
            weather.CACHE.write_text(
                json.dumps(dict(checked=42, temperature=10, location="Oslo"))
            )
            result = weather.refresh()
            self.assertEqual(result["checked"], 42)
            self.assertTrue(result["stale"])

    def test_lrc_orders_multiple_timestamps_and_ignores_tags(self):
        result = media.lyrics("[ar:artist]\n[00:02.50][00:10.000]Two\n[00:01.1]One")
        self.assertEqual([p["time"] for p in result], [1.1, 2.5, 10])
        self.assertEqual(result[1]["text"], "Two")

    def test_cached_media_does_not_fetch_or_publish(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.object(media, "CACHE", Path(directory)),
            patch.object(media, "get") as request,
        ):
            meta = dict(title="Test", artist="Test", lyrics=False, palette=False)
            first = media.track_data(meta)
            second = media.track_data(meta)
            self.assertEqual(first, second)
            request.assert_not_called()
            self.assertEqual(len(list(Path(directory).glob("*.json"))), 1)

    def test_pid_reuse_and_other_owners_cannot_receive_signals(self):
        with (
            patch.object(
                system,
                "process_info",
                return_value=dict(start="new", uid=system.os.getuid()),
            ),
            patch.object(system.os, "pidfd_open") as descriptor,
        ):
            with self.assertRaises(ValueError):
                system.terminate(5000, "old")
            descriptor.assert_not_called()
        with (
            patch.object(
                system,
                "process_info",
                return_value=dict(start="1", uid=system.os.getuid() + 1),
            ),
            patch.object(system.os, "pidfd_open") as descriptor,
        ):
            with self.assertRaises(ValueError):
                system.terminate(5000, "1")
            descriptor.assert_not_called()
