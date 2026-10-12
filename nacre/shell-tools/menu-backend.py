#!/usr/bin/env python3
"""Bounded menu discovery and explicit user-selected tools; no search-text execution."""

import argparse
import hashlib
import fcntl
from functools import wraps
from datetime import datetime
import json
import os
from pathlib import Path
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import time
from urllib.parse import quote, urlparse
from urllib.request import Request, urlopen
import uuid

HOME = Path.home()
STATE = HOME / ".local/state/nacre/menu"
CACHE = HOME / ".cache/nacre/menu"
PACKAGE = re.compile(r"[A-Za-z0-9][A-Za-z0-9@._+:-]{0,199}\Z")
UNIT = re.compile(r"nacre-reminder-[0-9a-f]{32}\.timer\Z")


def registry_lock(name):
    def decorate(function):
        @wraps(function)
        def wrapped(*args, **kwargs):
            STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
            with (STATE / (name + ".lock")).open("w") as guard:
                os.chmod(guard.name, 0o600)
                fcntl.flock(guard, fcntl.LOCK_EX)
                return function(*args, **kwargs)

        return wrapped

    return decorate


def run(args, timeout=5, check=False):
    return subprocess.run(
        args, capture_output=True, text=True, timeout=timeout, check=check
    )


def atomic(path, data):
    path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, name = tempfile.mkstemp(dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(data, stream)
        os.chmod(name, 0o600)
        os.replace(name, path)
    finally:
        Path(name).unlink(missing_ok=True)


def read(path, fallback):
    try:
        if path.stat().st_size > 2097152:
            return fallback
        return json.loads(path.read_text())
    except (OSError, ValueError):
        return fallback


def package_name(value):
    if not isinstance(value, str) or not PACKAGE.fullmatch(value):
        raise ValueError("Invalid package name")
    return value


def installed():
    if not shutil.which("pacman"):
        return []
    result = run(["pacman", "-Q"], timeout=10)
    return [
        {"name": line.split()[0], "version": line.split()[1]}
        for line in result.stdout.splitlines()
        if len(line.split()) == 2
    ][:10000]


def active(unit):
    return run(["systemctl", "--user", "is-active", "--quiet", unit]).returncode == 0


def fonts():
    if not shutil.which("fc-list"):
        return []
    result = run(["fc-list", ":lang=en", "--format", "%{family}\n"])
    return sorted(
        {
            name.strip()
            for line in result.stdout.splitlines()
            for name in line.split(",")
            if name.strip()
        },
        key=str.casefold,
    )[:1000]


def webapps():
    data = read(STATE / "webapps.json", [])
    if not isinstance(data, list):
        return []
    return [
        row
        for row in data
        if isinstance(row, dict)
        and re.fullmatch("[0-9a-f]{32}", str(row.get("id", "")))
        and isinstance(row.get("label"), str)
    ][:500]


def reminders():
    data = read(STATE / "reminders.json", [])
    if not isinstance(data, list):
        return []
    now = time.time()
    return [
        row
        for row in data
        if isinstance(row, dict)
        and UNIT.fullmatch(str(row.get("unit", "")))
        and isinstance(row.get("due"), (int, float))
        and row["due"] > now
    ][:100]


def package_stamp():
    try:
        return Path("/var/lib/pacman/local").stat().st_mtime_ns
    except OSError:
        return 0


def snapshot(force=False):
    stamp = package_stamp()
    cached = read(CACHE / "state.json", {})
    if (
        not force
        and isinstance(cached, dict)
        and cached.get("version") == 1
        and cached.get("packageStamp") == stamp
        and isinstance(cached.get("created"), (int, float))
        and isinstance(cached.get("capabilities"), dict)
        and isinstance(cached.get("packages"), list)
        and 0 <= time.time() - cached.get("created", 0) < 30
    ):
        return cached
    tools = {
        name: bool(shutil.which(name))
        for name in (
            "grim",
            "slurp",
            "hyprpicker",
            "wf-recorder",
            "tesseract",
            "zbarimg",
            "pacman",
            "paru",
            "yay",
            "nacre-ai",
            "nacre-brain",
            "notify-send",
            "fc-list",
        )
    }
    caps = {
        "capture": tools["grim"] and tools["slurp"],
        "record": tools["wf-recorder"] and tools["slurp"],
        "ocr": tools["tesseract"] and tools["grim"] and tools["slurp"],
        "qr": tools["zbarimg"] and tools["grim"] and tools["slurp"],
        "color-picker": tools["hyprpicker"],
        "packages": tools["pacman"],
        "aur": tools["paru"] or tools["yay"],
        "ai": tools["nacre-ai"],
        "brain": (HOME / ".local/share/nacre/brain/control.py").is_file(),
        "fonts": tools["fc-list"],
    }
    value = dict(
        version=1,
        packageStamp=stamp,
        created=time.time(),
        capabilities=caps,
        packages=installed(),
        fonts=fonts(),
        stayAwake=active("nacre-stay-awake.service"),
        recording=active("nacre-screen-recording.service"),
        reminders=reminders(),
        webapps=webapps(),
    )
    atomic(CACHE / "state.json", value)
    return value


def search_packages(query):
    query = str(query).strip()[:120]
    if len(query) < 2:
        return {
            "rows": [],
            "message": "Type at least two characters to search packages",
        }
    rows = []
    if shutil.which("pacman"):
        result = run(["pacman", "-Ss", "--", re.escape(query)], timeout=8)
        lines = result.stdout.splitlines()
        for index, line in enumerate(lines):
            if line.startswith(" ") or "/" not in line:
                continue
            parts = line.split()
            repo, name = parts[0].split("/", 1)
            if PACKAGE.fullmatch(name):
                rows.append(
                    dict(
                        name=name,
                        repository=repo,
                        description=lines[index + 1].strip()
                        if index + 1 < len(lines) and lines[index + 1].startswith(" ")
                        else "",
                        source="repo",
                    )
                )
    message = ""
    if shutil.which("paru") or shutil.which("yay"):
        try:
            request = Request(
                "https://aur.archlinux.org/rpc/v5/search/"
                + quote(query, safe="")
                + "?by=name-desc",
                headers={"User-Agent": "Nacre-menu/1"},
            )
            with urlopen(request, timeout=3) as response:
                data = json.loads(response.read(2097152))
            seen = {row["name"] for row in rows}
            for row in data.get("results", []):
                name = row.get("Name", "")
                if PACKAGE.fullmatch(name) and name not in seen:
                    rows.append(
                        dict(
                            name=name,
                            repository="AUR",
                            description=str(row.get("Description") or ""),
                            source="aur",
                        )
                    )
                    seen.add(name)
        except (OSError, ValueError):
            message = (
                "Community repository unavailable; official results are still shown"
            )
    rows.sort(
        key=lambda row: (
            row["name"].casefold() != query.casefold(),
            not row["name"].casefold().startswith(query.casefold()),
            row["source"] == "aur",
            row["name"],
        )
    )
    return {"rows": rows[:60], "message": message}


def ownership(ident):
    if (
        not isinstance(ident, str)
        or not re.fullmatch(r"[A-Za-z0-9_.-]{1,200}", ident)
        or not shutil.which("pacman")
    ):
        return {"package": ""}
    name = ident if ident.endswith(".desktop") else ident + ".desktop"
    if (HOME / ".local/share/applications" / name).exists():
        return {"package": ""}
    path = Path("/usr/share/applications") / name
    result = run(["pacman", "-Qoq", "--", str(path)])
    value = result.stdout.strip()
    return {
        "package": value if result.returncode == 0 and PACKAGE.fullmatch(value) else ""
    }


def package_command(action, value):
    value = package_name(value)
    manager = shutil.which("paru") or shutil.which("yay")
    if manager:
        return [
            manager,
            "-S" if action == "install" else "-Rns",
            *(["--needed"] if action == "install" else []),
            *(
                ["--skipreview"]
                if action == "install" and Path(manager).name == "paru"
                else []
            ),
            "--",
            value,
        ]
    if not shutil.which("pacman"):
        raise ValueError("Package management requires an Arch-compatible system")
    return [
        "pkexec",
        "pacman",
        "-S" if action == "install" else "-Rns",
        *(["--needed"] if action == "install" else []),
        "--",
        value,
    ]


@registry_lock("webapps")
def create_webapp(label, url):
    if (
        not isinstance(label, str)
        or not 1 <= len(label.strip()) <= 80
        or any(ord(c) < 32 for c in label)
    ):
        raise ValueError("Choose a short application name")
    if not isinstance(url, str) or len(url) > 4096 or any(ord(c) < 32 for c in url):
        raise ValueError("Enter a valid website URL")
    parsed = urlparse(url)
    if (
        parsed.scheme not in ("https", "http")
        or not parsed.hostname
        or parsed.username
        or parsed.password
    ):
        raise ValueError("Use an HTTP or HTTPS website URL without credentials")
    ident = uuid.uuid4().hex
    rows = webapps()
    row = dict(id=ident, label=label.strip(), url=url)
    rows.append(row)
    atomic(STATE / "webapps.json", rows)
    folder = HOME / ".local/share/applications"
    folder.mkdir(parents=True, exist_ok=True)
    # The generated Exec contains only a fixed launcher and a generated identifier.
    display_name = label.strip().replace("\\", "\\\\")
    body = f"[Desktop Entry]\nType=Application\nName={display_name}\nIcon=web-browser\nExec=nacre-menu webapp-launch {ident}\nCategories=Network;\nTerminal=false\n"
    path = folder / ("nacre-web-" + ident + ".desktop")
    path.write_text(body)
    path.chmod(0o600)
    CACHE.joinpath("state.json").unlink(missing_ok=True)
    return {"message": "Web app added to All apps"}


@registry_lock("webapps")
def remove_webapp(ident):
    if not re.fullmatch("[0-9a-f]{32}", str(ident)):
        raise ValueError("Invalid web app")
    rows = webapps()
    if not any(row["id"] == ident for row in rows):
        raise ValueError("Web app no longer exists")
    atomic(STATE / "webapps.json", [row for row in rows if row["id"] != ident])
    (HOME / ".local/share/applications" / ("nacre-web-" + ident + ".desktop")).unlink(
        missing_ok=True
    )
    CACHE.joinpath("state.json").unlink(missing_ok=True)
    return {"message": "Web app removed"}


def launch_webapp(ident):
    row = next((row for row in webapps() if row["id"] == ident), None)
    if not row:
        raise ValueError("Web app no longer exists")
    url = row["url"]
    if urlparse(url).scheme not in ("http", "https"):
        raise ValueError("Invalid saved website")
    browser = next(
        (
            shutil.which(name)
            for name in ("google-chrome-stable", "google-chrome", "chromium")
            if shutil.which(name)
        ),
        None,
    )
    command = [browser, "--app=" + url] if browser else ["nacre-app", "browser", url]
    subprocess.Popen(["uwsm", "app", "--", *command], start_new_session=True)
    return {"message": "Web app opened"}


@registry_lock("reminders")
def set_reminder(minutes, message):
    minutes = float(minutes)
    if (
        not 0.1 <= minutes <= 1440
        or not isinstance(message, str)
        or not 1 <= len(message.strip()) <= 240
        or "\0" in message
    ):
        raise ValueError(
            "Use a duration between 0.1 and 1440 minutes and a short message"
        )
    if not shutil.which("notify-send"):
        raise ValueError("Desktop notifications are unavailable")
    unit = "nacre-reminder-" + uuid.uuid4().hex
    run(
        [
            "systemd-run",
            "--user",
            "--collect",
            "--unit=" + unit,
            "--on-active=" + str(minutes * 60) + "s",
            "--timer-property=AccuracySec=1s",
            "notify-send",
            "-a",
            "Nacre Reminders",
            "-i",
            "nacre-notifications",
            "Reminder",
            message.strip(),
        ],
        check=True,
    )
    rows = reminders()
    rows.append(
        dict(
            unit=unit + ".timer", label=message.strip(), due=time.time() + minutes * 60
        )
    )
    atomic(STATE / "reminders.json", rows)
    CACHE.joinpath("state.json").unlink(missing_ok=True)
    return {"message": "Reminder set"}


@registry_lock("reminders")
def cancel_reminder(unit):
    if (
        not isinstance(unit, str)
        or not UNIT.fullmatch(unit)
        or not any(row["unit"] == unit for row in reminders())
    ):
        raise ValueError("Unknown Nacre reminder")
    run(["systemctl", "--user", "stop", unit], check=True)
    atomic(
        STATE / "reminders.json", [row for row in reminders() if row["unit"] != unit]
    )
    CACHE.joinpath("state.json").unlink(missing_ok=True)
    return {"message": "Reminder canceled"}


def capture_window():
    current = json.loads(run(["hyprctl", "activewindow", "-j"], check=True).stdout)
    position = current.get("at", [])
    size = current.get("size", [])
    if (
        len(position) != 2
        or len(size) != 2
        or any(type(v) is not int for v in position + size)
        or any(v <= 0 or v > 32768 for v in size)
    ):
        raise ValueError("Focus the window you want to capture first")
    folder = HOME / "Pictures/Screenshots"
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / (
        "screenshot_" + datetime.now().strftime("%Y%m%d_%H%M%S_%f") + ".png"
    )
    geometry = f"{position[0]},{position[1]} {size[0]}x{size[1]}"
    subprocess.run(["grim", "-g", geometry, str(path)], check=True)
    subprocess.run(
        ["wl-copy", "--type", "image/png"], input=path.read_bytes(), check=True
    )
    return "Window saved and copied"


def read_screen(kind):
    selection = run(["slurp"], timeout=180)
    if selection.returncode or not selection.stdout.strip():
        return ""
    CACHE.mkdir(parents=True, exist_ok=True, mode=0o700)
    fd, name = tempfile.mkstemp(suffix=".png", dir=CACHE)
    os.close(fd)
    try:
        subprocess.run(["grim", "-g", selection.stdout.strip(), name], check=True)
        command = (
            ["tesseract", name, "stdout"]
            if kind == "text"
            else ["zbarimg", "--quiet", "--raw", name]
        )
        result = run(command, timeout=30)
        text = result.stdout.strip()
        if not text:
            raise ValueError("No text found" if kind == "text" else "No QR code found")
        subprocess.run(
            ["wl-copy", "--type", "text/plain;charset=utf-8"],
            input=text.encode(),
            check=True,
        )
        return "Text copied" if kind == "text" else "QR content copied"
    finally:
        Path(name).unlink(missing_ok=True)


def unit_owner(unit):
    result = run(
        [
            "systemctl",
            "--user",
            "show",
            unit,
            "-p",
            "MainPID",
            "-p",
            "InvocationID",
            "-p",
            "ActiveState",
        ]
    )
    fields = dict(
        line.split("=", 1) for line in result.stdout.splitlines() if "=" in line
    )
    return (
        fields.get("MainPID", "0") + ":" + fields.get("InvocationID", "")
        if fields.get("ActiveState") == "active"
        and fields.get("MainPID", "0") != "0"
        and fields.get("InvocationID")
        else ""
    )


@registry_lock("recording")
def record_start():
    if active("nacre-screen-recording.service"):
        raise ValueError("A Nacre recording is already running")
    selection = run(["slurp"], timeout=180)
    if selection.returncode or not selection.stdout.strip():
        return ""
    folder = HOME / "Videos/Recordings"
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / (
        "recording_" + datetime.now().strftime("%Y%m%d_%H%M%S_%f") + ".mp4"
    )
    run(
        [
            "systemd-run",
            "--user",
            "--collect",
            "--unit=nacre-screen-recording",
            "wf-recorder",
            "-g",
            selection.stdout.strip(),
            "-f",
            str(path),
        ],
        check=True,
    )
    owner = ""
    for _ in range(20):
        owner = unit_owner("nacre-screen-recording.service")
        if owner:
            break
        time.sleep(0.1)
    if not owner:
        raise ValueError("Recording could not start")
    atomic(STATE / "recording.json", dict(owner=owner, path=str(path)))
    CACHE.joinpath("state.json").unlink(missing_ok=True)
    return "Recording started; use Actions › Capture › Stop recording"


@registry_lock("recording")
def record_stop():
    data = read(STATE / "recording.json", {})
    owner = unit_owner("nacre-screen-recording.service")
    if not owner:
        return "No Nacre recording is running"
    if not isinstance(data, dict) or data.get("owner") != owner:
        raise ValueError("This recording belongs to another controller")
    run(
        [
            "systemctl",
            "--user",
            "kill",
            "--signal=SIGINT",
            "nacre-screen-recording.service",
        ],
        check=True,
    )
    for _ in range(50):
        if not active("nacre-screen-recording.service"):
            break
        time.sleep(0.1)
    else:
        raise ValueError("Recording is still finishing; try again shortly")
    CACHE.joinpath("state.json").unlink(missing_ok=True)
    return "Recording saved in Videos/Recordings"


@registry_lock("stay-awake")
def stay_awake():
    unit = "nacre-stay-awake.service"
    owner = unit_owner(unit)
    record = read(STATE / "stay-awake.json", {})
    if owner:
        if not isinstance(record, dict) or record.get("owner") != owner:
            raise ValueError("Stay awake belongs to another controller")
        run(["systemctl", "--user", "stop", unit], check=True)
        message = "Normal idle behavior restored"
    else:
        run(
            [
                "systemd-run",
                "--user",
                "--collect",
                "--unit=nacre-stay-awake",
                "systemd-inhibit",
                "--what=idle",
                "--mode=block",
                "--who=Nacre",
                "--why=Nacre stay awake",
                "sleep",
                "infinity",
            ],
            check=True,
        )
        for _ in range(20):
            owner = unit_owner(unit)
            if owner:
                break
            time.sleep(0.1)
        if not owner:
            raise ValueError("Stay awake could not start")
        atomic(STATE / "stay-awake.json", dict(owner=owner))
        message = "Stay awake enabled until you turn it off or log out"
    CACHE.joinpath("state.json").unlink(missing_ok=True)
    return message


def tool(value):
    if value == "support-report":
        from diagnostics import support_report

        subprocess.run(
            ["wl-copy", "--type", "text/plain;charset=utf-8"],
            input=support_report(),
            text=True,
            check=True,
            timeout=5,
        )
        return "Support report copied"
    if value in ("capture-area", "capture-screen"):
        import screenshot

        result = screenshot.capture("region" if value == "capture-area" else "full")
        if result and value == "capture-area":
            folder = HOME / "Pictures/Screenshots"
            folder.mkdir(parents=True, exist_ok=True)
            path = folder / (
                "screenshot_" + datetime.now().strftime("%Y%m%d_%H%M%S_%f") + ".png"
            )
            png = subprocess.run(
                ["wl-paste", "--type", "image/png"], check=True, capture_output=True
            ).stdout
            path.write_bytes(png)
            result = "Selection saved and copied"
        return result or ""
    if value == "capture-window":
        return capture_window()
    if value == "capture-text":
        return read_screen("text")
    if value == "capture-qr":
        return read_screen("qr")
    if value == "record-start":
        return record_start()
    if value == "record-stop":
        return record_stop()
    if value == "color-picker":
        subprocess.run(["hyprpicker", "-a"], check=True)
        return "Color copied"
    if value == "stay-awake":
        return stay_awake()
    if value == "focus-timer":
        return set_reminder(25, "Time for a break")["message"]
    if value == "updates":
        return subprocess.call(["bash", str(Path(__file__).with_name("updates.sh"))])
    if value == "logs":
        return subprocess.call(
            [
                "journalctl",
                "--user",
                "-u",
                "nacre-shell.service",
                "--since",
                "30 minutes ago",
                "--no-pager",
            ]
        )
    raise ValueError("Unknown menu tool")


def edit_config(value):
    if value not in ("defaults", "shortcuts"):
        raise ValueError("Unknown configuration route")
    path = HOME / (
        ".config/nacre/apps.json"
        if value == "defaults"
        else ".config/nacre/shortcuts.lua"
    )
    if not path.exists():
        path.parent.mkdir(parents=True, exist_ok=True)
        if value == "defaults":
            path.write_text(
                json.dumps(
                    {
                        "browser": ["google-chrome-stable"],
                        "files": ["nacre-shell", "dolphin"],
                        "mail": [
                            "google-chrome-stable",
                            "--app=https://mail.google.com/",
                        ],
                        "emoji": ["nacre-menu", "actions.emoji"],
                    },
                    indent=2,
                )
                + "\n"
            )
        else:
            path.write_text("-- Personal Hyprland shortcut overrides.\n")
        path.chmod(0o600)
    preferred = shlex.split(os.environ.get("EDITOR", ""))
    command = (
        preferred
        if preferred and shutil.which(preferred[0])
        else next(
            ([name] for name in ("nvim", "nano", "vim") if shutil.which(name)), None
        )
    )
    if not command:
        raise ValueError("Install a text editor such as Neovim to edit configuration")
    return subprocess.call([*command, str(path)])


def search_files(query):
    if (
        not isinstance(query, str)
        or len(query) > 160
        or any(ord(c) < 32 for c in query)
    ):
        raise ValueError("Invalid file search")
    if len(query.strip()) < 2:
        return {"rows": [], "message": "Type two or more characters to find files"}
    roots = [
        str(HOME / name)
        for name in ("Documents", "Downloads", "Pictures", "Desktop", "Projects")
        if (HOME / name).is_dir()
    ]
    if not roots:
        return {"rows": []}
    if not shutil.which("fd"):
        return {"rows": [], "message": "Install fd to search files"}
    result = run(
        [
            "fd",
            "--type",
            "f",
            "--fixed-strings",
            "--ignore-case",
            "--max-results",
            "50",
            "--print0",
            "--",
            query,
            *roots,
        ],
        timeout=2,
    )
    rows = []
    for value in result.stdout.split("\0"):
        if not value or any(ord(c) < 32 for c in value):
            continue
        path = Path(value)
        if not any(path.is_relative_to(Path(root)) for root in roots):
            continue
        rows.append(
            {
                "path": str(path),
                "name": path.name,
                "id": hashlib.sha256(value.encode()).hexdigest()[:20],
            }
        )
    return {"rows": rows[:50]}


def convert_units(expression):
    if (
        not isinstance(expression, str)
        or len(expression) > 256
        or not re.fullmatch(r"[+\-]?[0-9][A-Za-z0-9µ°% .+*/^()\-]*", expression)
        or not re.search(r"\b(to|in)\b", expression)
    ):
        return {"ok": False, "error": "Try a conversion such as 10 km to miles"}
    if not shutil.which("qalc"):
        return {
            "ok": False,
            "error": "Install libqalculate for unit and currency conversions",
        }
    result = subprocess.run(
        [
            "qalc",
            "--terse",
            "--defaults",
            "--nodatasets",
            "--time",
            "400",
            "--set",
            "update exchange rates 0",
            "--set",
            "save config 0",
            "--set",
            "save definitions 0",
            "--set",
            "save mode 0",
            "--set",
            "clear history 1",
            expression,
        ],
        stdin=subprocess.DEVNULL,
        capture_output=True,
        text=True,
        timeout=2,
    )
    value = result.stdout.strip()
    if (
        result.returncode
        or not value
        or len(value) > 2048
        or "error" in result.stderr.lower()
    ):
        return {"ok": False, "error": "Could not convert these units"}
    return {
        "ok": True,
        "value": value,
        "description": "Enter to copy · currencies use stored Qalculate rates",
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "action",
        choices=[
            "state",
            "files",
            "convert",
            "packages",
            "installed",
            "fonts",
            "ownership",
            "tool",
            "install",
            "remove",
            "webapp-create",
            "webapp-remove",
            "webapp-launch",
            "reminder",
            "cancel-reminder",
            "config",
        ],
    )
    parser.add_argument("value", nargs="?", default="")
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args()
    try:
        if args.action == "config":
            return edit_config(args.value)
        if args.action in ("install", "remove"):
            return subprocess.call(package_command(args.action, args.value))
        if args.action == "files":
            result = search_files(args.value)
        elif args.action == "convert":
            result = convert_units(args.value)
        elif args.action == "state":
            result = snapshot(args.force)
        elif args.action == "packages":
            result = search_packages(args.value)
        elif args.action == "installed":
            result = {"rows": installed()}
        elif args.action == "fonts":
            result = {"rows": fonts()}
        elif args.action == "ownership":
            result = ownership(args.value)
        elif args.action == "tool":
            message = tool(args.value)
            if isinstance(message, int):
                return message
            result = {"message": message}
            if message and args.value not in ("logs", "updates"):
                subprocess.run(
                    [
                        "notify-send",
                        "-a",
                        "Nacre Capture"
                        if args.value.startswith(("capture-", "record-"))
                        or args.value == "color-picker"
                        else "Nacre",
                        "-i",
                        "nacre-notifications",
                        "Nacre",
                        message,
                    ],
                    check=False,
                )
        elif args.action in ("webapp-create", "reminder"):
            data = json.loads(sys.stdin.readline(65537))
            result = (
                create_webapp(data.get("label"), data.get("url"))
                if args.action == "webapp-create"
                else set_reminder(data.get("minutes"), data.get("message"))
            )
        elif args.action == "webapp-remove":
            result = remove_webapp(args.value)
        elif args.action == "webapp-launch":
            result = launch_webapp(args.value)
        else:
            result = cancel_reminder(args.value)
        print(json.dumps(result))
    except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
        print(json.dumps({"error": str(error)[:240]}))
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
