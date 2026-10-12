#!/usr/bin/env python3
"""Private dashboard preferences, bounded local context and persistent focus state."""

import argparse
import datetime as dt
import fcntl
from itertools import islice
import shutil
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
from zoneinfo import ZoneInfo

HOME = Path.home()
BRAIN = shutil.which("nacre-brain") or str(HOME / ".local/bin/nacre-brain")
CONFIG = HOME / ".config/nacre/dashboard.json"
STATE = HOME / ".local/state/nacre/dashboard-focus.json"
PAGES = ["today", "weather", "media", "system", "focus"]
DEFAULTS = dict(
    tabs=PAGES,
    homeZone="",
    homeCity="",
    calendarFile="",
    lyrics=True,
    albumPalette=True,
    focusMinutes=25,
)


def read(path, fallback):
    try:
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return fallback


def write(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temp = tempfile.mkstemp(prefix="." + path.name, dir=path.parent)
    try:
        with os.fdopen(fd, "w") as output:
            json.dump(value, output, allow_nan=False)
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)


def validate(data):
    result = dict(DEFAULTS)
    for key, value in data.items():
        if key == "tabs":
            if (
                not isinstance(value, list)
                or not value
                or len(set(value)) != len(value)
                or any(v not in PAGES for v in value)
            ):
                raise ValueError("Keep at least one unique dashboard tab")
        elif key in ("lyrics", "albumPalette"):
            if type(value) is not bool:
                raise ValueError("Use true or false")
        elif key == "focusMinutes":
            if type(value) is not int or not 1 <= value <= 180:
                raise ValueError("Focus duration must be1–180minutes")
        elif key in ("homeZone", "homeCity", "calendarFile"):
            if (
                not isinstance(value, str)
                or len(value) > 1024
                or any(ord(c) < 32 for c in value)
            ):
                raise ValueError("Invalid text")
            if key == "homeZone" and value:
                ZoneInfo(value)
            if (
                key == "calendarFile"
                and value
                and not Path(value).expanduser().is_absolute()
            ):
                raise ValueError("Use an absolute local calendar path")
        else:
            raise ValueError("Unknown dashboard preference")
        result[key] = value
    return result


def calendar(path, now=None):
    now = now or dt.datetime.now(dt.timezone.utc)
    if not path:
        return dict(events=[], error="", configured=False)
    try:
        source = Path(path).expanduser()
        if source.stat().st_size > 2_000_000:
            raise ValueError("Calendar exceeds2MB")
        text = re.sub(r"\r?\n[ \t]", "", source.read_text())
        events = []
        recurring = False
        for block in text.split("BEGIN:VEVENT")[1:501]:
            fields = {}
            for line in block.split("END:VEVENT")[0].splitlines():
                if ":" in line:
                    key, value = line.split(":", 1)
                    fields[key] = value
            stamp = next((k for k in fields if k.startswith("DTSTART")), None)
            if not stamp:
                continue
            if fields.get("STATUS") == "CANCELLED":
                continue
            raw = fields[stamp]
            all_day = "VALUE=DATE" in stamp or len(raw) == 8
            try:
                event = dt.datetime.strptime(
                    raw.rstrip("Z"), "%Y%m%d" if all_day else "%Y%m%dT%H%M%S"
                )
                zone = (
                    ZoneInfo(stamp.split("TZID=", 1)[1].split(";")[0])
                    if "TZID=" in stamp
                    else dt.timezone.utc
                    if raw.endswith("Z")
                    else dt.datetime.now().astimezone().tzinfo
                )
                event = event.replace(tzinfo=zone)
            except (ValueError, KeyError):
                continue
            recurring |= "RRULE" in fields
            if (
                event.date() >= now.astimezone(zone).date()
                and event.timestamp() <= now.timestamp() + 31 * 86400
            ):
                events.append(
                    dict(
                        title=fields.get("SUMMARY", "Untitled event")
                        .replace("\\n", " ")
                        .replace("\\,", ",")[:180],
                        start=event.timestamp(),
                        time="All day"
                        if all_day
                        else event.astimezone().strftime("%H:%M"),
                        date=event.astimezone().strftime("%a %d %b"),
                    )
                )
        return dict(
            events=sorted(events, key=lambda e: e["start"])[:8],
            configured=True,
            error="Recurring events require an expanded calendar export."
            if recurring
            else "",
        )
    except (OSError, ValueError) as error:
        return dict(events=[], configured=True, error=str(error)[:180])


def brain_context():
    try:
        root = Path(
            subprocess.check_output([BRAIN, "path"], text=True, timeout=3).strip()
        )
        notes = sorted(
            (
                p
                for p in islice(root.rglob("*.md"), 2000)
                if not any(part.startswith(".") for part in p.relative_to(root).parts)
            ),
            key=lambda p: p.stat().st_mtime,
            reverse=True,
        )[:10]
        recent = []
        tasks = []
        for note in notes:
            if note.stat().st_size > 128_000:
                continue
            text = note.read_text()
            title = next(
                (
                    line.lstrip("# ").strip()
                    for line in text.splitlines()
                    if line.startswith("# ")
                ),
                note.stem,
            )
            recent.append(dict(title=title[:100], path=str(note)))
            tasks.extend(
                dict(text=line[6:].strip()[:160], path=str(note))
                for line in text.splitlines()
                if line.startswith("- [ ] ")
            )
        return dict(available=True, recent=recent[:5], tasks=tasks[:6], root=str(root))
    except (OSError, subprocess.SubprocessError):
        return dict(available=False, recent=[], tasks=[])


def snapshot(include_brain=False):
    prefs = validate(read(CONFIG, {}))
    focus = read(STATE, {})
    zone = prefs["homeZone"] or read(Path("/etc/nacre/timezone.json"), {}).get(
        "confirmedTimezone", ""
    )
    home = {}
    try:
        current = dt.datetime.now().astimezone()
        at_home = current.astimezone(ZoneInfo(zone))
        if current.utcoffset() != at_home.utcoffset():
            home = dict(
                time=at_home.strftime("%H:%M"),
                label=prefs["homeCity"] or zone.split("/")[-1].replace("_", " "),
            )
    except (ValueError, KeyError):
        pass
    return dict(
        preferences=prefs,
        focus=focus,
        home=home,
        calendar=calendar(prefs["calendarFile"]),
        brain=brain_context() if include_brain else {},
        checked=time.time(),
    )


def focus_action(kind, minutes=25, dnd=False, now=None):
    now = time.time() if now is None else now
    state = read(STATE, {})
    if kind == "start":
        if not 1 <= minutes <= 180:
            raise ValueError("Choose1–180minutes")
        state = dict(
            status="running",
            total=minutes * 60,
            remaining=minutes * 60,
            deadline=now + minutes * 60,
            previousDnd=dnd,
            ownsDnd=not dnd,
        )
    elif kind == "pause" and state.get("status") == "running":
        state.update(status="paused", remaining=max(0, state["deadline"] - now))
    elif kind == "resume" and state.get("status") == "paused":
        state.update(status="running", deadline=now + state["remaining"])
    elif kind in ("stop", "complete"):
        state.update(
            status="done" if kind == "complete" else "idle",
            remaining=0,
            deadline=0,
            ownsDnd=False,
        )
    elif kind == "release":
        state["ownsDnd"] = False
    else:
        raise ValueError("Focus action unavailable")
    write(STATE, state)
    return state


def execute(action, value=""):
    CONFIG.parent.mkdir(parents=True, exist_ok=True)
    with CONFIG.with_suffix(".lock").open("w") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        if action == "preferences":
            write(CONFIG, validate(json.loads(value)))
        elif action.startswith("focus-"):
            data = json.loads(value or "{}")
            focus_action(action[6:], data.get("minutes", 25), data.get("dnd", False))
        elif action == "capture":
            if not value.strip() or len(value) > 16000:
                raise ValueError("Write a note up to16000characters")
            subprocess.run(
                [
                    BRAIN,
                    "note",
                    "--kind",
                    "inbox",
                    "--title",
                    value.splitlines()[0][:100],
                    "--source",
                    "Nacre Focus quick capture",
                    "--confidence",
                    "reported",
                ],
                input=value,
                text=True,
                check=True,
                capture_output=True,
                timeout=8,
            )
        elif action not in ("read", "context"):
            raise ValueError("Unknown dashboard action")
        result = snapshot(action in ("context", "capture"))
    return result


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("action")
    parser.add_argument("value", nargs="?", default="")
    args = parser.parse_args()
    try:
        result = execute(args.action, args.value)
        print(json.dumps(result, allow_nan=False))
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        subprocess.SubprocessError,
    ) as error:
        print(json.dumps(dict(error=str(error)[:240])))
        raise SystemExit(1) from None


if __name__ == "__main__":
    main()
