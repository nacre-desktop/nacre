#!/usr/bin/env python3
"""One bounded Open-Meteo snapshot for shell and lock weather consumers."""

import json
from nacre_i18n import tr
import time
import urllib.parse
import urllib.request
from pathlib import Path

HOME = Path.home()
CACHE = HOME / ".cache/nacre/weather.json"
WMO = {
    0: (tr("weather.condition.clear", "Clear"), 113),
    1: (tr("weather.condition.mostly-clear", "Mostly clear"), 116),
    2: (tr("weather.condition.partly-cloudy", "Partly cloudy"), 116),
    3: (tr("weather.condition.overcast", "Overcast"), 122),
    45: (tr("weather.condition.fog", "Fog"), 143),
    48: (tr("weather.condition.freezing-fog", "Freezing fog"), 260),
    51: (tr("weather.condition.light-drizzle", "Light drizzle"), 266),
    53: (tr("weather.condition.drizzle", "Drizzle"), 266),
    55: (tr("weather.condition.heavy-drizzle", "Heavy drizzle"), 281),
    56: (tr("weather.condition.freezing-drizzle", "Freezing drizzle"), 281),
    57: (tr("weather.condition.freezing-drizzle", "Freezing drizzle"), 284),
    61: (tr("weather.condition.light-rain", "Light rain"), 296),
    63: (tr("weather.condition.rain", "Rain"), 302),
    65: (tr("weather.condition.heavy-rain", "Heavy rain"), 308),
    66: (tr("weather.condition.freezing-rain", "Freezing rain"), 311),
    67: (tr("weather.condition.freezing-rain", "Freezing rain"), 314),
    71: (tr("weather.condition.light-snow", "Light snow"), 326),
    73: (tr("weather.condition.snow", "Snow"), 332),
    75: (tr("weather.condition.heavy-snow", "Heavy snow"), 338),
    77: (tr("weather.condition.snow-grains", "Snow grains"), 350),
    80: (tr("weather.condition.rain-showers", "Rain showers"), 353),
    81: (tr("weather.condition.rain-showers", "Rain showers"), 356),
    82: (tr("weather.condition.heavy-showers", "Heavy showers"), 359),
    85: (tr("weather.condition.snow-showers", "Snow showers"), 368),
    86: (tr("weather.condition.heavy-snow-showers", "Heavy snow showers"), 371),
    95: (tr("weather.condition.thunderstorm", "Thunderstorm"), 200),
    96: (tr("weather.condition.thunderstorm-with-hail", "Thunderstorm with hail"), 389),
    99: (tr("weather.condition.thunderstorm-with-hail", "Thunderstorm with hail"), 389),
}


def fetch_json(url):
    with urllib.request.urlopen(
        urllib.request.Request(url, headers={"User-Agent": "Nacre/1.0"}), timeout=8
    ) as response:
        data = response.read(512001)
        if len(data) > 512000:
            raise ValueError(
                tr(
                    "helper.weather.weather-response-too-large",
                    "Weather response too large",
                )
            )
        return json.loads(data)


def url(base, **query):
    return base + "?" + urllib.parse.urlencode(query)


def normalize(forecast, location, latitude, longitude, now=None):
    current = forecast["current"]
    daily = forecast["daily"]
    hourly = forecast["hourly"]
    checked = time.time() if now is None else now
    description, legacy = WMO.get(
        current["weather_code"], (tr("weather.condition.unknown", "Weather"), 122)
    )
    stamp = current["time"]
    hours = []
    days = []
    for i, value in enumerate(hourly["time"]):
        if value < stamp:
            continue
        hours.append(
            dict(
                time=value,
                temperature=hourly["temperature_2m"][i],
                rain=hourly["precipitation_probability"][i],
                code=hourly["weather_code"][i],
            )
        )
        if len(hours) == 24:
            break
    for i, value in enumerate(daily["time"][:10]):
        days.append(
            dict(
                date=value,
                low=daily["temperature_2m_min"][i],
                high=daily["temperature_2m_max"][i],
                code=daily["weather_code"][i],
                rain=daily["precipitation_probability_max"][i],
                sunrise=daily["sunrise"][i],
                sunset=daily["sunset"][i],
                uv=daily["uv_index_max"][i],
            )
        )
    return dict(
        location=location,
        latitude=latitude,
        longitude=longitude,
        code=str(legacy),
        wmo=current["weather_code"],
        description=description,
        temperature=current["temperature_2m"],
        feelsLike=current["apparent_temperature"],
        high=days[0]["high"],
        low=days[0]["low"],
        humidity=current["relative_humidity_2m"],
        wind=current["wind_speed_10m"],
        direction=current["wind_direction_10m"],
        pressure=current["surface_pressure"],
        isDay=bool(current["is_day"]),
        hours=hours,
        days=days,
        timezone=forecast.get("timezone", ""),
        localTime=current["time"],
        checked=checked,
        stale=False,
        error="",
        provider="Open-Meteo",
    )


def refresh():
    try:
        settings = json.loads((HOME / ".config/nacre/desktop.json").read_text())
    except (OSError, ValueError):
        settings = {}
    try:
        previous = json.loads(CACHE.read_text())
    except (OSError, ValueError):
        previous = {}
    location = settings.get("weatherLocation", "").strip()
    try:
        if not location:
            location = fetch_json("https://ipinfo.io/json").get("city", "")
        if not location:
            raise ValueError(
                tr(
                    "helper.weather.set-a-weather-city-in-settings",
                    "Set a weather city in Settings",
                )
            )
        if previous.get("location") == location and "latitude" in previous:
            latitude, longitude = previous["latitude"], previous["longitude"]
        else:
            choices = fetch_json(
                url(
                    "https://geocoding-api.open-meteo.com/v1/search",
                    name=location,
                    count=1,
                    language="en",
                    format="json",
                )
            ).get("results", [])
            if not choices:
                raise ValueError(
                    tr(
                        "helper.weather.weather-city-not-found",
                        "Weather city not found",
                    )
                )
            latitude, longitude = choices[0]["latitude"], choices[0]["longitude"]
        forecast = fetch_json(
            url(
                "https://api.open-meteo.com/v1/forecast",
                latitude=latitude,
                longitude=longitude,
                timezone="auto",
                forecast_days=10,
                current="temperature_2m,relative_humidity_2m,apparent_temperature,is_day,weather_code,surface_pressure,wind_speed_10m,wind_direction_10m",
                hourly="temperature_2m,precipitation_probability,weather_code",
                daily="weather_code,temperature_2m_max,temperature_2m_min,sunrise,sunset,uv_index_max,precipitation_probability_max",
            )
        )
        data = normalize(forecast, location, latitude, longitude)
        try:
            air = fetch_json(
                url(
                    "https://air-quality-api.open-meteo.com/v1/air-quality",
                    latitude=latitude,
                    longitude=longitude,
                    current="us_aqi",
                    timezone="auto",
                )
            )
            data["aqi"] = air.get("current", {}).get("us_aqi")
        except (OSError, ValueError, KeyError, TypeError, RuntimeError):
            data["aqi"] = None
        CACHE.parent.mkdir(parents=True, exist_ok=True)
        temp = CACHE.with_suffix(".tmp")
        temp.write_text(json.dumps(data, allow_nan=False))
        temp.chmod(0o600)
        temp.replace(CACHE)
        return data
    except (OSError, ValueError, KeyError, TypeError, RuntimeError):
        return dict(
            previous,
            stale=True,
            error="Weather unavailable; showing cached conditions"
            if previous
            else "Weather unavailable. Set a city or try again.",
        )


if __name__ == "__main__":
    print(json.dumps(refresh(), allow_nan=False))
