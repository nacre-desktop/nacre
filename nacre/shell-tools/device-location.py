#!/usr/bin/env python3
"""Bounded system location lookup; timezone mapping stays offline, coordinates ephemeral."""

import json
import math
import time
import threading


def validate_fix(fix, now=None):
    now = time.time() if now is None else now
    lat, lon, accuracy = fix["latitude"], fix["longitude"], fix["accuracy"]
    if not all(
        isinstance(v, (int, float)) and math.isfinite(v) for v in (lat, lon, accuracy)
    ):
        raise ValueError("Invalid device location")
    if not -90 <= lat <= 90 or not -180 <= lon <= 180 or not 0 <= accuracy <= 5000:
        raise ValueError("Device location is too imprecise")
    if not -60 <= now - fix["timestamp"] <= 300:
        raise ValueError("Device location is stale")
    if "geoip" in fix.get("description", "").lower():
        raise ValueError("IP-only location is not trusted")
    return fix


def zone_for_fix(fix, world):
    validate_fix(fix)
    lat, lon = fix["latitude"], fix["longitude"]
    # Nearest-city mapping is conservative near differing timezone regions.
    # Require all points in a 5km-or-larger uncertainty envelope to agree.
    radius = max(5000, fix["accuracy"])
    latitude_delta = radius / 111000
    longitude_delta = latitude_delta / max(0.1, math.cos(math.radians(lat)))
    zones = set()
    city = None
    for north, east in [(0, 0), (1, 0), (-1, 0), (0, 1), (0, -1)]:
        latitude = max(-90, min(90, lat + north * latitude_delta))
        longitude = ((lon + east * longitude_delta + 180) % 360) - 180
        place = world.find_nearest_city(latitude, longitude)
        if place is None or not place.get_timezone_str():
            raise ValueError("No reliable local timezone")
        zones.add(place.get_timezone_str())
        if city is None:
            city = place.get_city_name() or place.get_name()
    if len(zones) != 1:
        raise ValueError("Location is near an uncertain timezone boundary")
    return {
        "timezone": zones.pop(),
        "city": city,
        "accuracyMeters": round(fix["accuracy"]),
        "source": "Device location (GeoClue)",
    }


def detect():
    import gi

    gi.require_version("Geoclue", "2.0")
    gi.require_version("GWeather", "4.0")
    from gi.repository import Geoclue, Gio, GLib, GWeather

    cancellable = Gio.Cancellable()
    timer = threading.Timer(12, cancellable.cancel)
    timer.daemon = True
    timer.start()
    simple = None
    loop = GLib.MainLoop()
    result = {}
    timeout = None
    started = time.monotonic()
    try:
        simple = Geoclue.Simple.new_sync(
            "nacre-timezone", Geoclue.AccuracyLevel.NEIGHBORHOOD, cancellable
        )

        def check(*unused):
            location = simple.get_location()
            if location is None:
                return

            def value(name):
                variant = location.get_cached_property(name)
                if variant is None:
                    raise ValueError("Incomplete device fix")
                return variant.unpack()

            try:
                stamp = value("Timestamp")
                fix = dict(
                    latitude=value("Latitude"),
                    longitude=value("Longitude"),
                    accuracy=value("Accuracy"),
                    timestamp=stamp[0] + stamp[1] / 1000000,
                    description=value("Description"),
                )
                result.update(zone_for_fix(fix, GWeather.Location.get_world()))
                loop.quit()
            except (ValueError, KeyError, TypeError):
                return

        simple.connect("notify::location", check)
        check()
        if not result:
            remaining = max(1, int((12 - (time.monotonic() - started)) * 1000))

            def expired():
                loop.quit()
                return False

            timeout = GLib.timeout_add(remaining, expired)
            loop.run()
        if not result:
            raise RuntimeError("No fresh, accurate device location available")
        return result
    finally:
        timer.cancel()
        if timeout and GLib.MainContext.default().find_source_by_id(timeout):
            GLib.source_remove(timeout)
        if simple is not None:
            try:
                simple.get_client().call_stop_sync(None)
            except Exception:  # noqa: BLE001 — GI/service boundary may raise backend-specific errors; location is optional.
                pass


if __name__ == "__main__":
    try:
        print(json.dumps(detect()))
    except Exception:  # noqa: BLE001 — GI/service boundary may raise backend-specific errors; location is optional.
        print(json.dumps({"error": "No trustworthy device location available"}))
        raise SystemExit(1) from None
