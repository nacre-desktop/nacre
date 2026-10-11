import importlib.util
import time
import unittest
from pathlib import Path
from types import SimpleNamespace

spec = importlib.util.spec_from_file_location(
    "location", Path(__file__).resolve().parents[1] / "device-location.py"
)
location = importlib.util.module_from_spec(spec)
spec.loader.exec_module(location)


class DeviceLocationTests(unittest.TestCase):
    def fix(self, **values):
        return dict(
            latitude=29.76,
            longitude=-95.37,
            accuracy=200,
            timestamp=time.time(),
            description="",
            **values,
        )

    def test_rejects_coarse_ip_and_stale_or_invalid_fix(self):
        for change in [
            {"accuracy": 50000},
            {"timestamp": time.time() - 1000},
            {"latitude": float("nan")},
            {"description": "GeoIP location"},
            {"longitude": 181},
        ]:
            fix = self.fix()
            fix.update(change)
            with self.assertRaises(ValueError):
                location.validate_fix(fix)

    def test_offline_zone_mapping_requires_uncertainty_region_agreement(self):
        class City:
            def __init__(self, zone):
                self.zone = zone

            def get_timezone_str(self):
                return self.zone

            def get_city_name(self):
                return "Example city"

        world = SimpleNamespace(
            find_nearest_city=lambda lat, lon: City("America/Chicago")
        )
        self.assertEqual(
            location.zone_for_fix(self.fix(), world)["timezone"], "America/Chicago"
        )
        world = SimpleNamespace(
            find_nearest_city=lambda lat, lon: City(
                "America/Chicago" if lat < 29.78 else "America/New_York"
            )
        )
        with self.assertRaises(ValueError):
            location.zone_for_fix(self.fix(), world)
