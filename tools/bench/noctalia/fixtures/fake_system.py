#!/usr/bin/env python3
# SPDX-License-Identifier: GPL-3.0-only
"""Fixed audio, battery and power-profile values for the private benchmark sessions.

Trimmed from the Bead parity fixture (B0083). `bench.py` puts a wrapper for this
file first on PATH as `pactl`, `wpctl` and `parec`, so the Nacre daemon sees the
same machine Noctalia reads from the private PipeWire: two sinks and one source
at 45 %. `fake-power.py` serves BATTERY / PROFILES on the private bus.

`pactl subscribe` sleeps in-process instead of exec'ing `tail -F`: the old tail
child woke once a second inside Nacre's measured tree (N0305).
"""

import json
import os
import sys
import time

DEFAULT_LEVEL = 0.45
SINKS = [
    ("fixture-speakers", "fixture-speakers", "Fixture speakers"),
    ("fixture-headphones", "fixture-headphones", "Fixture headphones"),
]
SOURCES = [("fixture-microphone", "fixture-microphone", "Fixture microphone")]
DEFAULTS = {"sink": SINKS[0][0], "source": SOURCES[0][0]}
STREAMS = {
    "sink-inputs": [(70, "Firefox", "firefox", 0.8), (71, "Spotify", "spotify", 0.5)],
    "source-outputs": [(80, "OBS Studio", "com.obsproject.Studio", 0.6)],
}
BATTERY = {
    "percentage": 63.0,
    "state": 2,
    "time_to_empty": 2 * 3600 + 10 * 60,
    "time_to_full": 0,
    "on_battery": True,
}
PROFILES = {
    "active": "balanced",
    "profiles": ["power-saver", "balanced", "performance"],
}


def entries(kind):
    nodes = SINKS if kind == "sink" else SOURCES
    channels = ("front-left", "front-right") if kind == "sink" else ("mono",)
    value = round(DEFAULT_LEVEL * 65536)
    volume = {
        side: {"value": value, "value_percent": f"{round(DEFAULT_LEVEL * 100)}%"}
        for side in channels
    }
    return [
        {
            "index": 50 + index,
            "state": "RUNNING" if name == DEFAULTS[kind] else "IDLE",
            "name": name,
            "description": description,
            "mute": False,
            "volume": volume,
            "properties": {"node.name": node, "device.class": "sound"},
        }
        for index, (name, node, description) in enumerate(nodes)
    ]


def stream_entries(kind):
    out = []
    for index, name, icon, level in STREAMS[kind]:
        value = round(level * 65536)
        out.append(
            {
                "index": index,
                "mute": False,
                "volume": {
                    "mono": {"value": value, "value_percent": f"{round(level * 100)}%"}
                },
                "properties": {
                    "application.name": name,
                    "application.icon_name": icon,
                    "media.name": f"{name} audio",
                },
            }
        )
    return out


def pipewire_conf():
    """A pipewire.conf.d drop-in: the same nodes as null devices plus default metadata."""

    def node(name, description, media_class, position):
        return (
            "  { factory = adapter\n"
            f'    args = {{ factory.name = support.null-audio-sink node.name = "{name}" '
            f'node.description = "{description}" media.class = {media_class} '
            f"object.linger = true audio.position = [ {position} ] }} }}\n"
        )

    nodes = [node(n, d, "Audio/Sink", "FL FR") for _, n, d in SINKS]
    nodes += [node(n, d, "Audio/Source", "MONO") for _, n, d in SOURCES]
    default = "  { factory = metadata\n    args = { metadata.name = default } }\n"
    return "context.objects = [\n" + "".join(nodes) + default + "]\n"


def default_metadata():
    return [
        ("default.audio.sink", json.dumps({"name": SINKS[0][1]})),
        ("default.audio.source", json.dumps({"name": SOURCES[0][1]})),
    ]


def pactl(argv, out=sys.stdout):
    args = [a for a in argv if not a.startswith("--format=")]
    if args[:1] == ["list"] and args[1:2] in (["sink-inputs"], ["source-outputs"]):
        json.dump(stream_entries(args[1]), out)
        out.write("\n")
    elif args[:1] == ["list"] and args[1:2] in (["sinks"], ["sources"]):
        json.dump(entries("sink" if args[1] == "sinks" else "source"), out)
        out.write("\n")
    elif args in (["get-default-sink"], ["get-default-source"]):
        out.write(DEFAULTS[args[0].rsplit("-", 1)[1]] + "\n")
    elif args == ["subscribe"]:
        while True:
            time.sleep(3600)
    return 0


def wpctl(argv, out=sys.stdout):
    if argv[:1] == ["get-volume"]:
        out.write(f"Volume: {DEFAULT_LEVEL:.2f}\n")
    elif argv[:1] == ["inspect"]:
        name = "microphone" if "SOURCE" in " ".join(argv[1:]) else "speakers"
        out.write(f'  node.name = "fixture-{name}"\n')
    return 0


def parec():
    """The daemon's peak capture: silence, one 66 ms window at a time."""
    while True:
        sys.stdout.buffer.write(b"\0" * 1056)
        sys.stdout.buffer.flush()
        time.sleep(0.066)


if __name__ == "__main__":
    # bench.py writes `pactl`/`wpctl`/`parec` wrappers that set FAKE_TOOL.
    tool = os.environ.get("FAKE_TOOL", "pactl")
    if tool == "parec":
        parec()
    elif tool == "wpctl":
        sys.exit(wpctl(sys.argv[1:]))
    sys.exit(pactl(sys.argv[1:]))
