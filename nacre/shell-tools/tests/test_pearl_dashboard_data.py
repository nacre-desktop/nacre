"""Provider boundaries: persistent deadlines, private config and cached forecasts."""

import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]


def module(name):
    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), ROOT / (name + ".py")
    )
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


weather = module("weather")


class PearlDashboardDataTests(unittest.TestCase):
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
