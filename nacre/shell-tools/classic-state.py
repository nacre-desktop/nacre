#!/usr/bin/env python3
"""Commit one wallpaper palette to the shell, window frame and native choosers."""

import nacre_native_compositor as compositor

import fcntl
from nacre_i18n import tr
import json
import nacre_state as _state
import os
import re
import signal
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path
from contextlib import contextmanager
import threading


_publication_gate = threading.RLock()
_publication_owner = threading.local()


@contextmanager
def publication_lock(home):
    """Serialize direct callers, prepared commits and the inherited CLI lock."""
    path = (Path(home) / ".local/state/nacre/palette-commit.lock").resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    with _publication_gate:
        previous = getattr(_publication_owner, "path", None)
        if previous == path:
            yield
            return
        inherited = False
        if os.environ.get("NACRE_PALETTE_LOCKED") == "1":
            try:
                descriptor = os.fstat(9)
                expected = path.stat()
                inherited = (descriptor.st_dev, descriptor.st_ino) == (
                    expected.st_dev,
                    expected.st_ino,
                )
            except OSError:
                pass
        if inherited:
            # Same open description as the CLI: reacquire safely, never unlock its owner.
            fcntl.flock(9, fcntl.LOCK_EX)
            _publication_owner.path = path
            try:
                yield
            finally:
                _publication_owner.path = previous
        else:
            with path.open("a") as owner:
                fcntl.flock(owner, fcntl.LOCK_EX)
                _publication_owner.path = path
                try:
                    yield
                finally:
                    _publication_owner.path = previous


def validated_colors(data):
    """Reject incomplete or malformed public roles before any consumer output."""
    if not isinstance(data, dict) or data.get("mode") not in ("light", "dark"):
        raise ValueError(
            tr("helper.classic-state.invalid-palette-mode", "Invalid palette mode")
        )
    values = data.get("colours")
    roles = _state.read_json_file(Path(__file__).with_name("reference-style.json"))[
        "colours"
    ]
    if not isinstance(values, dict) or not roles.keys() <= values.keys():
        raise ValueError(
            tr(
                "helper.classic-state.palette-is-missing-required-color-roles",
                "Palette is missing required color roles",
            )
        )
    for key, value in values.items():
        if not isinstance(key, str) or not re.fullmatch(r"[A-Za-z][A-Za-z0-9_]*", key):
            raise ValueError(
                tr(
                    "helper.classic-state.invalid-palette-role-name",
                    "Invalid palette role name",
                )
            )
        if not isinstance(value, str) or not re.fullmatch(r"#?[0-9a-fA-F]{6}", value):
            raise ValueError(
                tr(
                    "helper.classic-state.invalid-palette-color",
                    "Invalid palette color",
                )
            )
    for name in ("source", "input"):
        if name in data and not isinstance(data[name], dict):
            raise ValueError(
                tr(
                    "helper.classic-state.invalid-palette-metadata",
                    "Invalid palette metadata",
                )
            )
    return {key: value.lstrip("#") for key, value in values.items()}


def atomic_write(path, text):
    if _state.write_text_if_managed(path, text):
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, temp = tempfile.mkstemp(prefix="." + path.name, dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            stream.write(text)
        os.replace(temp, path)
    finally:
        if os.path.exists(temp):
            os.unlink(temp)

    if (
        path.name == "presentation.json"
        and os.environ.get("NACRE_PRESENTATION_STREAM") == "1"
    ):
        print(json.dumps({"presentation": json.loads(text)}), flush=True)


def atomic_symlink(path, target):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(dir=path.parent)
    os.close(fd)
    os.unlink(name)
    try:
        Path(name).symlink_to(target)
        os.replace(name, path)
    finally:
        Path(name).unlink(missing_ok=True)


def commit_prepared(
    home, wallpaper, data, thumbnail, live=True, allow_mode_change=False
):
    """Use a validated read-only cache through this same palette publisher."""
    home = Path(home)
    state = home / ".local/state/nacre"
    image = Path(wallpaper).resolve()
    thumbnail = Path(thumbnail).resolve()
    if not image.is_file() or not thumbnail.is_file():
        return False
    if not thumbnail.is_relative_to((home / ".cache/nacre/wallpapers").resolve()):
        return False
    try:
        validated_colors(data)
        if data["name"] != "dynamic":
            return False
        if any(
            not isinstance(data[k], str)
            or not re.fullmatch("[a-zA-Z0-9_-]{1,40}", data[k])
            for k in ("flavour", "variant")
        ):
            return False
        config = home / ".config/nacre/cli.json"
        if config.exists() and _state.read_json_file(config).get("wallpaper", {}).get(
            "postHook"
        ):
            return False
    except (OSError, ValueError, KeyError, TypeError):
        return False
    state.mkdir(parents=True, exist_ok=True)
    with publication_lock(home):
        try:
            current = _state.read_json_file(state / "scheme.json")
        except FileNotFoundError:
            if not allow_mode_change:
                return False
            current = {
                "name": "dynamic",
                "mode": data["mode"],
                "flavour": data["flavour"],
                "variant": data["variant"],
            }
        except (OSError, ValueError):
            return False
        if (
            not isinstance(current, dict)
            or (not allow_mode_change and current.get("name") != "dynamic")
            or (not allow_mode_change and current.get("flavour") != data["flavour"])
            or (not allow_mode_change and current.get("mode") != data["mode"])
            or (
                not allow_mode_change
                and current.get("variant", "tonalspot")
                != data.get("variant", "tonalspot")
            )
        ):
            return False
        atomic_write(state / "scheme.json", json.dumps(data))
        atomic_write(state / "wallpaper/path.txt", str(image))
        atomic_symlink(state / "wallpaper/current", image)
        atomic_symlink(state / "wallpaper/thumbnail.jpg", thumbnail)
        apply_palette(home, str(image), live=live)
    return True


DEMO_PREFERENCES = (
    "palettePreset",
    "paletteMode",
    "paletteHarmony",
    "palettePersonality",
    "paletteBackgroundFromWallpaper",
)


def restore_demo(home, snapshot, live=True):
    """Publish a captured demo baseline through the ordinary palette owner.

    Restore only the palette preferences touched by Welcome; preserve unrelated
    changes (rotation, motion, layout, unknown preference fields). All inputs are
    validated before writes and publication uses the same cross-process lock.
    """
    home = Path(home)
    state = home / ".local/state/nacre"
    data = snapshot["scheme"]
    validated_colors(data)
    poster = Path(snapshot["poster"])
    media = snapshot["media"]
    if not poster.is_absolute() or not poster.is_file():
        raise ValueError(
            tr(
                "helper.classic-state.the-starting-wallpaper-is-unavailable-restore-its-file-and-try-ag",
                "The starting wallpaper is unavailable. Restore its file and try again.",
            )
        )
    if media.get("poster") != str(poster) or not Path(media.get("path", "")).is_file():
        raise ValueError(
            tr(
                "helper.classic-state.the-starting-wallpaper-source-is-unavailable-restore-its-file-and",
                "The starting wallpaper source is unavailable. Restore its file and try again.",
            )
        )
    if set(snapshot["preferences"]) - set(DEMO_PREFERENCES):
        raise ValueError(
            tr(
                "helper.classic-state.invalid-demo-preference-snapshot",
                "Invalid demo preference snapshot",
            )
        )
    pref_path = home / ".config/nacre/wallpaper-picker.json"
    with publication_lock(home):
        files = [
            pref_path,
            state / "scheme.json",
            state / "wallpaper/media.json",
            state / "wallpaper/last.txt",
            state / "wallpaper/path.txt",
        ]
        links = {}
        for path in (state / "wallpaper/current", state / "wallpaper/thumbnail.jpg"):
            links[path] = (
                ("link", os.readlink(path))
                if path.is_symlink()
                else ("file", path.read_bytes())
                if path.is_file()
                else ("absent", None)
            )
        previous = {path: path.read_text() if path.exists() else None for path in files}
        options = json.loads(previous[pref_path] or "{}")
        for key in DEMO_PREFERENCES:
            options.pop(key, None)
        options.update(snapshot["preferences"])
        try:
            atomic_write(pref_path, json.dumps(options))
            atomic_write(state / "scheme.json", json.dumps(data))
            atomic_write(state / "wallpaper/media.json", json.dumps(media))
            atomic_write(state / "wallpaper/path.txt", str(poster))
            atomic_symlink(state / "wallpaper/current", poster)
            thumbnail = Path(media.get("thumbnail", ""))
            if thumbnail.is_file():
                atomic_symlink(state / "wallpaper/thumbnail.jpg", thumbnail)
            apply_palette(home, str(poster), live=live)
        except Exception:  # noqa: BLE001 — Restore all published state before re-raising any failure.
            for path, text in previous.items():
                if text is None:
                    path.unlink(missing_ok=True)
                else:
                    atomic_write(path, text)
            for path, (kind, value) in links.items():
                if kind == "link":
                    atomic_symlink(path, value)
                elif kind == "file":
                    fd, name = tempfile.mkstemp(dir=path.parent)
                    with os.fdopen(fd, "wb") as stream:
                        stream.write(value)
                    os.replace(name, path)
                else:
                    path.unlink(missing_ok=True)
            old_poster = previous[state / "wallpaper/last.txt"]
            if old_poster and Path(old_poster.strip()).is_file():
                atomic_symlink(state / "wallpaper/current", old_poster.strip())
            if previous[state / "scheme.json"]:
                apply_palette(
                    home, old_poster.strip() if old_poster else None, live=live
                )
            raise
    return media


def luminance(value):
    rgb = [int(value.lstrip("#")[i : i + 2], 16) / 255 for i in (0, 2, 4)]
    rgb = [v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4 for v in rgb]
    return sum(v * w for v, w in zip(rgb, (0.2126, 0.7152, 0.0722), strict=True))


def readable(value, background, minimum=4.5):
    rgb = [int(value.lstrip("#")[i : i + 2], 16) for i in (0, 2, 4)]
    bg = luminance(background)
    for _ in range(100):
        result = "".join(f"{v:02x}" for v in rgb)
        fg = luminance(result)
        if (max(bg, fg) + 0.05) / (min(bg, fg) + 0.05) >= minimum:
            return result
        rgb = (
            [max(0, int(v * 0.92)) for v in rgb]
            if bg > 0.5
            else [min(255, int(v + max(1, (255 - v) * 0.08))) for v in rgb]
        )
    return "000000" if bg > 0.5 else "ffffff"


def render_template(home, name, colors):
    """Use Orient's pure renderer; publication remains owned by this module."""
    from palette_engine import render_values

    here = Path(__file__).parent
    return render_values(
        (
            compositor.palette_template_path()
            if name == "compositor-palette"
            else here.joinpath(name)
        ).read_text(),
        colors,
    )


def apply_palette(home, wallpaper=None, live=True):
    home = Path(home)
    with publication_lock(home):
        return _apply_palette(home, wallpaper, live=live)


def publish_compositor_palette(home, colors):
    """Apply the decorative-gradient preference only to desktop borders."""
    values = dict(colors)
    desktop = Path(home) / ".config/nacre/desktop.json"
    preferences = _state.read_json_file(desktop) if desktop.exists() else {}
    if preferences.get("interfaceGradients", True) is False:
        values["secondary"] = values["primary"]
    text = render_template(home, "compositor-palette", values)
    atomic_write(compositor.config_path(home, "palette"), text)
    return text


def _publish_presentation(home, state, data, colors, wallpaper, fixed, preset, options):
    selected = (
        str(Path(wallpaper).expanduser().resolve())
        if wallpaper
        else (
            (state / "wallpaper/last.txt").read_text().strip()
            if (state / "wallpaper/last.txt").exists()
            else ""
        )
    )
    if selected:
        presentation = state / "presentation.json"
        previous = _state.read_json_file(presentation) if presentation.exists() else {}
        changed_at = (
            previous.get("changedAtMs", 0) if previous.get("poster") == selected else 0
        )
        if not changed_at and previous.get("poster") == selected:
            media_path = state / "wallpaper/media.json"
            media = _state.read_json_file(media_path) if media_path.exists() else {}
            if media.get("poster") == selected:
                changed_at = media.get("appliedAtMs") or int(
                    media_path.stat().st_mtime * 1000
                )
            else:
                changed_at = int(presentation.stat().st_mtime * 1000)
        if not changed_at:
            changed_at = int(time.time() * 1000)
        atomic_write(
            state / "presentation.json",
            json.dumps(
                {
                    "version": 1,
                    "changedAtMs": changed_at,
                    "publishedAtMs": max(
                        int(time.time() * 1000), previous.get("publishedAtMs", 0) + 1
                    ),
                    "mode": data["mode"],
                    "colours": colors,
                    "poster": selected,
                    "paletteOptions": data.get("source", {}).get("options", []),
                    "selectedAccent": None
                    if fixed
                    else data.get("source", {}).get("selected")
                    or data.get("input", {}).get("accent"),
                    "palettePreset": preset,
                    "paletteHarmony": data.get("input", {}).get(
                        "harmony", options.get("paletteHarmony", False)
                    ),
                    "palettePersonality": data.get("input", {}).get(
                        "personality", options.get("palettePersonality", "natural")
                    ),
                    "paletteBackgroundFromWallpaper": data.get("input", {}).get(
                        "background_from_wallpaper",
                        options.get("paletteBackgroundFromWallpaper", False),
                    ),
                    "workspaceColors": _state.read_json_file(
                        home / ".config/nacre/colors.json"
                    )
                    .get("wallpapers", {})
                    .get(selected, {})
                    .get("workspaceColors", False)
                    if (home / ".config/nacre/colors.json").exists()
                    else False,
                }
            ),
        )


def _publish_target_manifest(home, state):
    manifest = json.loads(Path(__file__).with_name("theme-targets.json").read_text())
    targets = []
    for target in manifest["targets"]:
        available = not target.get("tool") or shutil.which(target["tool"]) is not None
        published = any((home / output).is_file() for output in target["outputs"])
        targets.append(
            dict(
                target,
                status="tool-missing"
                if not available
                else "current"
                if published
                else "not-published",
                available=available,
            )
        )
    _state.write(state / "theme-targets.json", dict(version=1, targets=targets))


def _publish_application_colors(home, colors, data, live):
    # GTK and qtct use the same committed palette, rather than the old blue files.
    import importlib.util

    icon_spec = importlib.util.spec_from_file_location(
        "file_icons", Path(__file__).with_name("file-icons.py")
    )
    icon_module = importlib.util.module_from_spec(icon_spec)
    icon_spec.loader.exec_module(icon_module)
    icon_theme = icon_module.theme(home, colors["primary"], data["mode"])
    for version in ("3.0", "4.0"):
        directory = home / (".config/gtk-" + version)
        css = render_template(home, "gtk" + version[0] + "-colors.css.in", colors)
        atomic_write(directory / "gtk.css", css)
        settings = directory / "settings.ini"
        if settings.exists():
            text = settings.read_text()
            text = re.sub(
                r"(?m)^gtk-application-prefer-dark-theme\s*=.*$",
                "gtk-application-prefer-dark-theme="
                + ("1" if data["mode"] == "dark" else "0"),
                text,
            )
            text = re.sub(
                r"(?m)^gtk-theme-name\s*=.*$",
                "gtk-theme-name=Adwaita" + ("-dark" if data["mode"] == "dark" else ""),
                text,
            )
            if icon_theme:
                text = re.sub(
                    r"(?m)^gtk-icon-theme-name\s*=.*$",
                    "gtk-icon-theme-name=" + icon_theme,
                    text,
                )
                if not re.search(r"(?m)^gtk-icon-theme-name=", text):
                    text += "\ngtk-icon-theme-name=" + icon_theme + "\n"
            atomic_write(settings, text)
    from nacre_native_config import _binary

    subprocess.run(
        [str(_binary()), "kde-palette", "--publish"],
        input=json.dumps(
            {"home": str(home), "colors": colors, "icon_theme": icon_theme}
        ),
        text=True,
        capture_output=True,
        check=True,
        timeout=10,
    )
    # Numeric order follows Qt QPalette::ColorRole, including Accent in Qt 6.
    qt_roles = [
        "onSurface",
        "surfaceContainer",
        "surfaceBright",
        "surfaceContainerHigh",
        "surfaceDim",
        "outlineVariant",
        "onSurface",
        "onPrimary",
        "onSurface",
        "surface",
        "surface",
        "shadow",
        "primary",
        "onPrimary",
        "primary",
        "tertiary",
        "surfaceContainerLow",
        "onSurface",
        "surfaceContainer",
        "onSurface",
        "onSurfaceVariant",
        "primary",
    ]
    qt = "[ColorScheme]\n" + "".join(
        group + "_colors=" + ", ".join("#ff" + colors[role] for role in qt_roles) + "\n"
        for group in ("active", "inactive", "disabled")
    )
    qt_path = home / ".config/nacre/qt.conf"
    atomic_write(qt_path, qt)
    for version in (5, 6):
        settings = home / f".config/qt{version}ct/qt{version}ct.conf"
        if settings.exists():
            text = re.sub(
                r"(?m)^color_scheme_path\s*=.*$",
                "color_scheme_path=" + str(qt_path),
                settings.read_text(),
            )
            text = re.sub(r"(?m)^custom_palette\s*=.*$", "custom_palette=true", text)
            if icon_theme:
                text = re.sub(
                    r"(?m)^icon_theme\s*=.*$", "icon_theme=" + icon_theme, text
                )
            atomic_write(settings, text)
    if live:
        # Native KDE apps react in-place; no polling or application restarts.
        for change in (0, 4):
            subprocess.run(
                [
                    "dbus-send",
                    "--session",
                    "--type=signal",
                    "/KGlobalSettings",
                    "org.kde.KGlobalSettings.notifyChange",
                    "int32:" + str(change),
                    "int32:0",
                ],
                capture_output=True,
                timeout=3,
            )
    return icon_theme


def _publish_terminal_colors(home, colors, data):
    # Existing updater prompts and terminal apps share the committed colors too.
    for name, role in {
        "primary": "primary",
        "secondary": "secondary",
        "onsurface": "onSurface",
        "onprimary": "onPrimary",
        "surface": "surface",
        "surfacecontainer": "surfaceContainer",
    }.items():
        atomic_write(home / (".config/nacre/colors/" + name), "#" + colors[role])
    # Terminal TUIs often paint dark input panels regardless of the desktop mode.
    # Use the same palette's inverse roles on light wallpapers, preserving its hue.
    term_bg = (
        colors["inverseSurface"]
        if data["mode"] == "light"
        else colors["surfaceContainerLow"]
    )
    term_fg = (
        colors["inverseOnSurface"] if data["mode"] == "light" else colors["onSurface"]
    )
    term_accent = readable(
        colors["inversePrimary"] if data["mode"] == "light" else colors["primary"],
        term_bg,
    )
    term_muted = readable(colors["onSurfaceVariant"], term_bg)
    terminal_roles = {
        "foreground": term_fg,
        "background": term_bg,
        "cursor": term_accent,
        "cursor_text_color": term_bg,
        "selection_foreground": term_bg,
        "selection_background": term_accent,
        "url_color": readable(colors["tertiary"], term_bg),
        "active_border_color": colors["primary"],
        "inactive_border_color": colors["outlineVariant"],
        "active_tab_foreground": term_bg,
        "active_tab_background": term_accent,
        "inactive_tab_foreground": term_muted,
        "inactive_tab_background": term_bg,
    }
    ansi = [
        "onSurface",
        "error",
        "green",
        "yellow",
        "blue",
        "mauve",
        "teal",
        "onSurface",
    ]
    for i, role in enumerate(ansi):
        value = (
            term_bg
            if i == 0
            else term_fg
            if i == 7
            else term_accent
            if i == 4
            else readable(colors["secondary"], term_bg)
            if i == 5
            else readable(colors.get(role, colors["primary"]), term_bg)
        )
        bright = (
            term_muted
            if i == 0
            else readable(colors["secondary"], term_bg)
            if i == 6
            else value
        )
        terminal_roles[f"color{i}"] = value
        terminal_roles[f"color{i + 8}"] = bright
    atomic_write(
        home / ".config/nacre/kitty-colors.conf",
        render_template(home, "kitty-colors.conf.in", terminal_roles),
    )


def _publish_lock_login(home, state, colors, wallpaper):
    selected = (
        str(Path(wallpaper).expanduser().resolve())
        if wallpaper
        else (
            (state / "wallpaper/last.txt").read_text().strip()
            if (state / "wallpaper/last.txt").exists()
            else ""
        )
    )
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        "lock_config", Path(__file__).with_name("lock-config.py")
    )
    lock_config = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(lock_config)
    try:
        preferences = _state.read_json_file(home / ".config/nacre/desktop.json")
    except (OSError, ValueError):
        preferences = {}
    atomic_write(
        home / ".config/hypr/hyprlock.conf",
        lock_config.prepare_config(
            colors,
            selected,
            preferences,
            home / ".local/share/nacre/shell/tools/lock-info.py",
            home,
        ),
    )
    # Login appearance is public wallpaper/color data; authentication remains SDDM-owned.
    if selected:
        from nacre_native_config import _binary

        try:
            result = subprocess.run(
                [str(_binary()), "login-appearance", "--publish"],
                input=json.dumps(
                    {
                        "home": str(home),
                        "colors": colors,
                        "wallpaper": selected,
                        "preview": str(home / ".local/state/nacre/shell/login-preview"),
                    }
                ),
                text=True,
                stdout=subprocess.DEVNULL,
                timeout=60,
                check=False,
            )
            if result.returncode:
                print(
                    "Login appearance publication failed; desktop palette remains active",
                    file=sys.stderr,
                )
        except (OSError, ValueError, RuntimeError, subprocess.SubprocessError) as error:
            print(f"Login appearance publication failed: {error}", file=sys.stderr)


def _reload_desktop_consumers(data, icon_theme, lua, live):
    if live:
        # Kitty documents SIGUSR1 as a config reload; it leaves terminal sessions running.
        for proc in Path("/proc").iterdir():
            if not proc.name.isdigit():
                continue
            try:
                if (
                    proc.stat().st_uid == os.getuid()
                    and (proc / "comm").read_text().strip() == "kitty"
                ):
                    os.kill(int(proc.name), signal.SIGUSR1)
            except (OSError, ProcessLookupError):
                pass
        subprocess.run(
            [
                "gsettings",
                "set",
                "org.gnome.desktop.interface",
                "cursor-theme",
                os.environ.get("XCURSOR_THEME", "breeze_cursors"),
            ],
            capture_output=True,
        )
        subprocess.run(
            [
                "gsettings",
                "set",
                "org.gnome.desktop.interface",
                "cursor-size",
                os.environ.get("XCURSOR_SIZE", "24"),
            ],
            capture_output=True,
        )
        subprocess.run(
            [
                "gsettings",
                "set",
                "org.gnome.desktop.interface",
                "color-scheme",
                "prefer-" + data["mode"],
            ],
            capture_output=True,
        )
        subprocess.run(
            [
                "gsettings",
                "set",
                "org.gnome.desktop.interface",
                "gtk-theme",
                "Adwaita" + ("-dark" if data["mode"] == "dark" else ""),
            ],
            capture_output=True,
        )
        if icon_theme:
            subprocess.run(
                [
                    "gsettings",
                    "set",
                    "org.gnome.desktop.interface",
                    "icon-theme",
                    icon_theme,
                ],
                capture_output=True,
            )
        if compositor.backend_name == "hyprland":
            result = compositor.apply_config(lua, check=False, timeout=None)
            if result.returncode:
                print(
                    "Palette saved; live window-border update failed: "
                    + result.stderr.strip(),
                    file=sys.stderr,
                )
        else:
            # Niri reads the generated KDL include through its configuration API.
            compositor.reload()


def _apply_palette(home, wallpaper=None, live=True):
    state = home / ".local/state/nacre"
    data = _state.read_json_file(state / "scheme.json")
    preferences = home / ".config/nacre/wallpaper-picker.json"
    options = _state.read_json_file(preferences) if preferences.exists() else {}
    preset = options.get("palettePreset", "wallpaper")
    fixed = preset != "wallpaper"
    if fixed:
        presets = _state.read_json_file(
            Path(__file__).with_name("palette-presets.json")
        )
        for favorite in (
            _state.read_json_file(home / ".config/nacre/colors.json").get(
                "favorites", []
            )
            if (home / ".config/nacre/colors.json").exists()
            else []
        ):
            presets.append(dict(favorite, id="favorite:" + favorite["id"]))
        chosen = next((item for item in presets if item["id"] == preset), None)
        mode = options.get("paletteMode", "dark")
        if chosen is None or mode not in ("dark", "light"):
            raise ValueError(
                tr(
                    "helper.classic-state.invalid-fixed-palette-preference",
                    "Invalid fixed palette preference",
                )
            )
        data = dict(data, mode=mode, colours=chosen["modes"][mode])
    colors = validated_colors(data)
    if fixed:
        # Resolve preferences only after the complete candidate passes validation.
        atomic_write(state / "scheme.json", json.dumps(data))
    # Validate required roles before publishing any state.
    primary, secondary, inactive, shadow = (
        colors[k] for k in ("primary", "secondary", "outlineVariant", "shadow")
    )
    _publish_presentation(home, state, data, colors, wallpaper, fixed, preset, options)
    brand = Path(__file__).with_name("branding.py")
    if brand.exists():
        import importlib.util

        spec = importlib.util.spec_from_file_location("brand", brand)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module.publish(colors, home)
    # Publish the native shell palette before slower compatibility/login assets.
    atomic_write(state / "scheme/current-mode.txt", data["mode"])
    atomic_write(
        state / "scheme/current.txt",
        "\n".join(k + " " + v for k, v in colors.items()) + "\n",
    )
    lua = publish_compositor_palette(home, colors)
    atomic_write(
        home / ".config/nacre/rofi.rasi",
        render_template(home, "rofi-colors.rasi.in", colors),
    )
    icon_theme = _publish_application_colors(home, colors, data, live)
    _publish_terminal_colors(home, colors, data)
    _publish_lock_login(home, state, colors, wallpaper)
    _publish_target_manifest(home, state)
    if wallpaper is not None:
        atomic_write(
            state / "wallpaper/last.txt", str(Path(wallpaper).expanduser().resolve())
        )
    _reload_desktop_consumers(data, icon_theme, lua, live)


if __name__ == "__main__":
    apply_palette(Path.home(), sys.argv[1] if len(sys.argv) > 1 else None)
