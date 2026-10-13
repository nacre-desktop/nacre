#!/usr/bin/env python3
"""Render the Nacre Hyprlock layout; PAM and session locking stay Hyprlock-owned."""

import json
from nacre_i18n import tr
import shlex
import subprocess
from pathlib import Path

import compositor


def block(widget, monitor="", **options):
    return tr(
        "helper.lock-config.value0-monitor-value1-value2",
        "{value0} {\n    monitor = {value1}\n{value2}}\n",
        {
            "value0": widget,
            "value1": monitor,
            "value2": "".join(
                (
                    "    " + key + " = " + str(value) + "\n"
                    for key, value in options.items()
                )
            ),
        },
    )


def panel_key(monitor):
    import hashlib

    return hashlib.sha256(
        json.dumps(
            {k: monitor.get(k) for k in ("name", "width", "height", "transform")},
            sort_keys=True,
        ).encode()
    ).hexdigest()[:12]


def panel_scale(monitor):
    width, height = monitor.get("width", 1920), monitor.get("height", 1080)
    if monitor.get("transform", 0) % 2:
        width, height = height, width
    return min(width / 2048, height / 1152)


def render(
    colors, wallpaper, preferences, helper, monitors=None, ready=None, artwork=None
):
    if any(c in wallpaper for c in "\n\r"):
        raise ValueError(
            tr(
                "helper.lock-config.unsupported-wallpaper-path",
                "Unsupported wallpaper path",
            )
        )
    colors = {key: value.lstrip("#") for key, value in colors.items()}
    ready = ready or Path.home() / ".cache/nacre/lock-ready"
    result = (
        Path(__file__)
        .with_name("hyprlock.conf.in")
        .read_text()
        .replace("{{wallpaper}}", wallpaper)
    )
    for key, value in colors.items():
        result = result.replace("{{" + key + "}}", value)
    for monitor in monitors or [dict(name="", width=1920, height=1080)]:
        name = monitor.get("name", "")
        scale = panel_scale(monitor)
        pointer = ready / ("dashboard-" + panel_key(monitor) + ".txt")
        initial = ready / ("dashboard-" + panel_key(monitor) + "-initial.png")
        path = (
            str(initial)
            if initial.is_file()
            else (
                artwork
                or str(Path.home() / ".local/share/nacre/branding/nacre-lock.png")
            )
        )

        def position(x, y, scale=scale):
            return str(round((x - 720) * scale)) + ", " + str(round((405 - y) * scale))

        def rgba(key, alpha="ff"):
            return tr(
                "helper.lock-config.rgba-value0-value1",
                "rgba({value0}{value1})",
                {"value0": colors[key], "value1": alpha},
            )

        def label(text, x, y, size, role="onSurface", name=name, scale=scale, **extra):
            return block(
                "label",
                name,
                text=text,
                position=position(x, y),
                halign="center",
                valign="center",
                color=rgba(role),
                font_size=round(size * scale * 0.75),
                font_family="IBM Plex Sans",
                zindex=2,
                **extra,
            )

        result += block(
            "image",
            name,
            path=path,
            size=round(810 * scale),
            rounding=0,
            border_size=0,
            position="0, 0",
            halign="center",
            valign="center",
            reload_time=2,
            reload_cmd="/usr/bin/cat " + shlex.quote(str(pointer)),
            zindex=0,
        )
        # Independent native labels keep time current without rebuilding the panel.
        result += label("cmd[update:1000] date '+%I'", 671, 120, 104, "primary")
        result += label("cmd[update:1000] date '+%M'", 766, 105, 54, "primary")
        result += label("cmd[update:1000] date '+%p'", 766, 151, 27)
        result += label(
            "cmd[update:60000] date '+%A · %d %b' | tr '[:lower:]' '[:upper:]'",
            720,
            193,
            20,
        )
        result += block(
            "input-field",
            name,
            size=f"{round(260 * scale)}, {round(46 * scale)}",
            position=position(720, 625),
            halign="center",
            valign="center",
            inner_color=rgba("surfaceContainer", "b0"),
            outer_color=rgba("primary", "50"),
            font_color=rgba("onSurface"),
            font_family="IBM Plex Sans",
            outline_thickness=1,
            rounding=round(23 * scale),
            dots_size=0.2,
            dots_spacing=0.3,
            fade_on_empty="false",
            placeholder_text="Enter your password  →",
            fail_text="$FAIL",
            check_color=rgba("primary"),
            fail_color=rgba("error"),
            zindex=3,
        )
        result += block(
            "label",
            name,
            text="bedtime",
            position=position(672, 690),
            halign="center",
            valign="center",
            color=rgba("primary"),
            font_family="Material Symbols Rounded",
            font_size=round(24 * scale * 0.75),
            onclick="systemctl suspend",
            zindex=3,
        )
        result += block(
            "label",
            name,
            text="lock",
            position=position(768, 690),
            halign="center",
            valign="center",
            color=rgba("primary"),
            font_family="Material Symbols Rounded",
            font_size=round(24 * scale * 0.75),
            zindex=3,
        )
        if preferences.get("lockMedia", True):
            for x, icon, action in [
                (165, "skip_previous", "previous"),
                (
                    238,
                    "cmd[update:2000] /usr/bin/cat "
                    + shlex.quote(str(ready / "play-icon.txt")),
                    "toggle",
                ),
                (311, "skip_next", "next"),
            ]:
                action_command = "python3 " + shlex.quote(str(helper)) + " " + action
                result += block(
                    "shape",
                    name,
                    size=f"{round((72 if action == 'toggle' else 38) * scale)}, {round(42 * scale)}",
                    position=position(x, 687),
                    halign="center",
                    valign="center",
                    rounding=round(21 * scale),
                    color="rgba(00000000)",
                    onclick=action_command,
                    zindex=2,
                )
                result += block(
                    "label",
                    name,
                    text=icon,
                    position=position(x, 687),
                    halign="center",
                    valign="center",
                    color=rgba("onPrimary" if action == "toggle" else "onSurface"),
                    font_size=round((28 if action == "toggle" else 22) * scale * 0.75),
                    font_family="Material Symbols Rounded",
                    onclick=action_command,
                    zindex=3,
                )
    return result


def monitor_layout(home=None):
    home = Path.home() if home is None else Path(home)
    cache = home / ".cache/nacre/lock-outputs.json"
    try:
        monitors = compositor.query("monitors", timeout=2)
        if not isinstance(monitors, list) or not monitors:
            raise ValueError(
                tr(
                    "helper.lock-config.no-current-lock-outputs",
                    "No current lock outputs",
                )
            )
        cache.parent.mkdir(parents=True, exist_ok=True)
        temporary = cache.with_suffix(".next")
        temporary.write_text(json.dumps(monitors))
        temporary.chmod(0o600)
        temporary.replace(cache)
        return monitors
    except (OSError, ValueError, subprocess.SubprocessError):
        try:
            return json.loads(cache.read_text())
        except (OSError, ValueError):
            return []


def prepare_config(colors, wallpaper, preferences, helper, home=None):
    home = Path.home() if home is None else Path(home)
    ready = home / ".cache/nacre/lock-ready"
    artwork = str(ready / "initial-art.png")
    if not Path(artwork).is_file():
        artwork = str(home / ".local/share/nacre/branding/nacre-lock.png")
    monitors = monitor_layout(home)
    # Generate presentation now if deployment/geometry changed. Lock startup only
    # reads ready files; no network, font lookup or rendering runs in Hyprlock.
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "lock_dashboard", Path(__file__).with_name("lock-dashboard.py")
    )
    dashboard = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(dashboard)
    try:
        data = json.loads((ready / "snapshot.json").read_text())
    except (OSError, ValueError):
        data = {}
    data["preferences"] = preferences
    try:
        prior = json.loads((ready / "dashboard.json").read_text())
    except (OSError, ValueError):
        prior = {}
    # Routine palette publication must stay fast. The desktop's event-coalesced
    # writer prepares new pixels independently; geometry/privacy need a ready
    # safe panel before a new native config can be used.
    missing = any(
        not (ready / ("dashboard-" + panel_key(m) + "-initial.png")).is_file()
        for m in (monitors or [dict(name="", width=1920, height=1080)])
    )
    settings_changed = any(
        prior.get("preferences", {}).get(key, default) != preferences.get(key, default)
        for key, default in (
            ("lockNotificationContents", False),
            ("lockMedia", True),
            ("lockWeather", True),
            ("lockNotifications", True),
        )
    )
    if missing or settings_changed:
        dashboard.publish(data, colors, wallpaper, artwork, home, monitors)
    return render(colors, wallpaper, preferences, helper, monitors, ready, artwork)


def publish(home=None):
    home = Path.home() if home is None else Path(home)
    state = home / ".local/state/nacre"
    scheme = json.loads((state / "scheme.json").read_text())
    colors = scheme["colours"]
    try:
        preferences = json.loads((home / ".config/nacre/desktop.json").read_text())
    except (OSError, ValueError):
        preferences = {}
    wallpaper = (
        (state / "wallpaper/last.txt").read_text().strip()
        if (state / "wallpaper/last.txt").exists()
        else ""
    )
    target = home / ".config/hypr/hyprlock.conf"
    target.parent.mkdir(parents=True, exist_ok=True)
    body = prepare_config(
        colors,
        wallpaper,
        preferences,
        home / ".local/share/nacre/shell/tools/lock-info.py",
        home,
    )
    temp = target.with_suffix(".next")
    temp.write_text(body)
    temp.chmod(0o600)
    temp.replace(target)


if __name__ == "__main__":
    publish()
