#!/usr/bin/env python3
"""Local wallpaper catalog, video posters, private picker preferences and selection."""

import argparse
from nacre_i18n import tr
import functools
import fcntl
import hashlib
import json
import nacre_state as _state
import os
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.parse
import time
from pathlib import Path
from PIL import Image, ImageStat

HOME = Path.home()
try:
    wallpaper_config = _state.read_json_file(HOME / ".config/nacre/cli.json").get(
        "wallpaper", {}
    )
    library_value = wallpaper_config.get("directory", str(HOME / "Pictures/Wallpapers"))
    LIBRARY = (
        Path(library_value).expanduser()
        if isinstance(library_value, str)
        and library_value
        and "\0" not in library_value
        else HOME / "Pictures/Wallpapers"
    )
except (OSError, ValueError, TypeError):
    LIBRARY = HOME / "Pictures/Wallpapers"
CACHE = HOME / ".cache/nacre/wallpaper-media"
STATE = HOME / ".local/state/nacre/wallpaper"
PREFS = HOME / ".config/nacre/wallpaper-picker.json"
IMAGES = {".jpg", ".jpeg", ".png", ".webp", ".gif", ".tif", ".tiff"}
VIDEOS = {".mp4", ".webm", ".mkv", ".mov", ".avi", ".m4v"}


def atomic(path, data):
    if _state.schema(path):
        _state.write(path, data)
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(dir=path.parent)
    try:
        with os.fdopen(fd, "w") as stream:
            json.dump(data, stream)
        os.chmod(name, 0o600)
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def read(path, default):
    try:
        return _state.read_json_file(path)
    except FileNotFoundError:
        return default


def settings():
    from nacre_native_palette import wallpaper_media as native

    return native.settings()


def media_state():
    from nacre_native_palette import wallpaper_media as native

    return native.media_state()


def palette_presets():
    from nacre_native_palette import wallpaper_media as native

    return native.palette_presets()


def wallpaper_mode(poster):
    """Choose from the image, independent of the previously selected UI mode."""
    with Image.open(poster) as image:
        image.thumbnail((64, 64))
        brightness = ImageStat.Stat(image.convert("L")).mean[0]
    return "light" if brightness >= 140 else "dark"


def theme(value):
    if (
        not isinstance(value, dict)
        or not value
        or set(value)
        - {
            "palettePreset",
            "paletteMode",
            "paletteAutoMode",
            "paletteAccent",
            "paletteHarmony",
            "palettePersonality",
            "paletteBackgroundFromWallpaper",
        }
    ):
        raise ValueError(
            tr(
                "helper.wallpaper-media.unknown-appearance-preference",
                "Unknown appearance preference",
            )
        )
    accent = value.get("paletteAccent")
    if accent is not None and (
        not isinstance(accent, str) or not re.fullmatch(r"auto|[0-9a-fA-F]{6}", accent)
    ):
        raise ValueError(
            tr(
                "helper.wallpaper-media.choose-a-valid-wallpaper-accent",
                "Choose a valid wallpaper accent",
            )
        )
    previous = settings()
    changes = {key: item for key, item in value.items() if key != "paletteAccent"}
    if "paletteMode" in value and "paletteAutoMode" not in value:
        changes["paletteAutoMode"] = False
    if "paletteHarmony" in value:
        changes["palettePersonality"] = (
            "harmony" if value["paletteHarmony"] else "natural"
        )
    if "palettePersonality" in value:
        changes["paletteHarmony"] = value["palettePersonality"] == "harmony"
    if accent is not None:
        changes["palettePreset"] = "wallpaper"
    poster = read(STATE / "media.json", {}).get("poster")
    if not poster:
        path = STATE / "last.txt"
        poster = path.read_text().strip() if path.exists() else ""
    if not poster or not Path(poster).is_file():
        raise ValueError(
            tr(
                "helper.wallpaper-media.choose-a-wallpaper-before-changing-its-palette",
                "Choose a wallpaper before changing its palette",
            )
        )
    if changes.get("paletteAutoMode", previous.get("paletteAutoMode", False)):
        changes["paletteMode"] = wallpaper_mode(poster)
    result = preference(changes)
    try:
        cli = HOME / ".local/share/nacre/shell/bin/nacre_shell"
        if (
            result["palettePreset"] == "wallpaper"
            and accent is None
            and prepared_theme(poster)
        ):
            return result
        if accent is not None:
            args = [str(cli), "scheme", "set", "-n", "dynamic"]
            if "paletteMode" in changes:
                args += ["-m", changes["paletteMode"]]
            args += ["--auto-accent"] if accent == "auto" else ["--accent", accent]
            subprocess.run(args, check=True, capture_output=True, timeout=60)
        elif "paletteMode" in changes:
            # A mode change generates and publishes once, with the saved harmony.
            subprocess.run(
                [str(cli), "scheme", "set", "-m", changes["paletteMode"]],
                check=True,
                capture_output=True,
                timeout=60,
            )
        else:
            subprocess.run(
                [str(cli), "wallpaper", "-f", poster, "--no-smart"],
                check=True,
                capture_output=True,
                timeout=60,
            )
    except Exception:  # noqa: BLE001 — Rollback must run for any failure before the original exception is re-raised.
        preference({key: previous[key] for key in changes})
        raise
    return result


def preference(value):
    from nacre_native_palette import wallpaper_media as native

    return native.preference(value)


def previews(poster):
    from nacre_native_palette import wallpaper_media as native

    return native.previews(poster)


def describe(path):
    from nacre_native_palette import wallpaper_media as native

    return native.describe(path)


def catalog():
    from nacre_native_palette import wallpaper_media as native

    return native.catalog()


def palette_marker(
    poster, flavour, harmony=None, source=None, mode=None, personality=None
):
    from nacre_native_palette import wallpaper_media as native

    return Path(
        native.palette_marker(poster, flavour, harmony, source, mode, personality)
    )


def prepared_theme(poster, live=True):
    """Apply a fresh prepared treatment through the same locked publisher."""
    scheme = read(HOME / ".local/state/nacre/scheme.json", {})
    marker = palette_marker(
        poster,
        scheme.get("flavour", "default"),
        mode=settings().get("paletteMode", scheme.get("mode", "dark")),
    )
    if not marker.exists():
        return False
    try:
        data = read(marker, {})
        if (
            not isinstance(data, dict)
            or not isinstance(data.get("input"), dict)
            or not isinstance(data.get("source"), dict)
        ):
            return False
        if data.get("input", {}).get("harmony") is not settings().get(
            "paletteHarmony", False
        ):
            return False
        digest = data.get("source", {}).get("digest", "")
        if not isinstance(digest, str) or not re.fullmatch(r"[0-9a-f]{64}", digest):
            return False
        thumbnail = HOME / ".cache/nacre/wallpapers" / digest / "thumbnail.jpg"
        palette = demo_module("classic-state")
        return palette.commit_prepared(
            HOME, poster, data, thumbnail, live=live, allow_mode_change=True
        )
    except (OSError, ValueError, KeyError, TypeError):
        return False


def warm(include_demo=True):
    # Read-only preparation never publishes a wallpaper, palette or login theme.
    # One low-priority worker survives overlapping startup requests via a lock.
    CACHE.mkdir(parents=True, exist_ok=True)
    with (CACHE / "warm.lock").open("w") as lock:
        os.chmod(lock.name, 0o600)
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return {"busy": True}
        cli = HOME / ".local/share/nacre/shell/bin/nacre_shell"
        from nacre_native_config import _binary

        def prepare_login(poster):
            subprocess.run(
                [str(_binary()), "login-appearance", "--prepare", str(poster)],
                stdout=subprocess.DEVNULL,
                check=True,
                timeout=60,
            )

        brand = demo_module("branding")
        current_media = media_state()
        scenes = [current_media] if current_media.get("poster") else []
        # Welcome's immutable demo collection is not part of the personal library.
        # Prepare it too, once per engine/preferences revision, never on a timer.
        if include_demo and Path(__file__).with_name("demo-wallpapers").is_dir():
            scenes.extend(describe(item["path"]) for item in demo_scenes())
        flavour = read(HOME / ".local/state/nacre/scheme.json", {}).get(
            "flavour", "default"
        )
        for scene in scenes:
            poster = scene.get("poster")
            if not poster or not cli.exists() or not Path(poster).is_file():
                continue
            prepare_login(poster)
            for mode in ("dark", "light"):
                for personality in (
                    "natural",
                    "pigment",
                    "harmony",
                    "pop",
                    "mist",
                    "vivid",
                    "pearl",
                    "tide",
                ):
                    marker = palette_marker(
                        poster,
                        flavour,
                        source=scene.get("path", poster),
                        harmony=personality == "harmony",
                        mode=mode,
                        personality=personality,
                    )
                    if marker.exists():
                        try:
                            # Palette and raster caches have separate lifetimes.
                            # Repair missing art even when color extraction is warm.
                            brand.prepare(read(marker, {})["colours"], HOME)
                            continue
                        except (
                            OSError,
                            ValueError,
                            KeyError,
                            subprocess.SubprocessError,
                        ):
                            pass
                    try:
                        result = subprocess.run(
                            [
                                str(cli),
                                "wallpaper",
                                "-p",
                                poster,
                                "--source",
                                scene.get("path", poster),
                                "--mode",
                                mode,
                                "--personality",
                                personality,
                            ],
                            capture_output=True,
                            text=True,
                            check=True,
                            timeout=30,
                        )
                        candidate = json.loads(result.stdout)
                        atomic(marker, candidate)
                        brand.prepare(candidate["colours"], HOME)
                        digest = candidate.get("source", {}).get("digest", "")
                        if re.fullmatch(r"[0-9a-f]{64}", digest):
                            thumbnail = (
                                HOME
                                / ".cache/nacre/wallpapers"
                                / digest
                                / "thumbnail.jpg"
                            )
                            if not thumbnail.exists():
                                thumbnail.parent.mkdir(parents=True, exist_ok=True)
                                with Image.open(scene.get("preview", poster)) as image:
                                    image.convert("RGB").save(thumbnail, quality=85)
                    except (OSError, ValueError, KeyError, subprocess.SubprocessError):
                        continue
        prepared = 0
        computed = 0
        flavour = read(HOME / ".local/state/nacre/scheme.json", {}).get(
            "flavour", "default"
        )
        for item in catalog()["entries"]:
            try:
                poster = Path(item["poster"])
                marker = palette_marker(poster, flavour, source=item["path"])
                if cli.exists() and not marker.exists():
                    result = subprocess.run(
                        [
                            str(cli),
                            "wallpaper",
                            "-p",
                            str(poster),
                            "--source",
                            item["path"],
                        ],
                        capture_output=True,
                        text=True,
                        check=True,
                        timeout=30,
                    )
                    atomic(marker, json.loads(result.stdout))
                    computed += 1
                prepare_login(item["poster"])
                prepared += 1
            except (OSError, ValueError, subprocess.SubprocessError):
                continue
        return {"prepared": prepared, "computed": computed}


def select(path):
    previous = settings()
    try:
        return _select(path)
    except Exception:  # noqa: BLE001 — Restore auto mode on any failed scene publication.
        if previous.get("paletteAutoMode"):
            preference({"paletteMode": previous["paletteMode"]})
        raise


def _select(path):
    item = describe(path)
    if settings().get("paletteAutoMode"):
        preference({"paletteMode": wallpaper_mode(item["poster"])})
    # Prepared colors still pass through the single publisher and its shared lock.
    # Missing/stale/invalid cache or custom CLI hooks use the normal CLI pipeline.
    prepared = False
    scheme = read(HOME / ".local/state/nacre/scheme.json", {})
    marker = palette_marker(
        item["poster"],
        scheme.get("flavour", "default"),
        source=item["path"],
        mode=settings()["paletteMode"] if settings().get("paletteAutoMode") else None,
    )
    if marker.exists():
        try:
            data = read(marker, {})
            digest = hashlib.sha256(Path(item["poster"]).read_bytes()).hexdigest()
            thumbnail = HOME / ".cache/nacre/wallpapers" / digest / "thumbnail.jpg"
            if not thumbnail.exists():
                thumbnail.parent.mkdir(parents=True, exist_ok=True)
                with Image.open(item["preview"]) as image:
                    image.convert("RGB").save(thumbnail, quality=85)
            palette = demo_module("classic-state")
            prepared = palette.commit_prepared(
                HOME,
                item["poster"],
                data,
                thumbnail,
                allow_mode_change=settings().get("paletteAutoMode", False),
            )
        except (OSError, ValueError, KeyError):
            prepared = False
    if not prepared:
        legacy = HOME / ".local/bin/nacre-shell"
        bridge = HOME / ".local/share/nacre/shell/bin/nacre_shell"
        if not bridge.is_file():
            bridge = Path(__file__).with_name("cli-bridge.sh")
        command = (
            [str(legacy), "wallpaper-image", item["poster"]]
            if legacy.is_file() and not settings().get("paletteAutoMode")
            else [
                str(bridge),
                "wallpaper",
                "--file",
                item["poster"],
            ]
        )
        if settings().get("paletteAutoMode"):
            command += ["--mode", settings()["paletteMode"]]
        subprocess.run(
            command,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            timeout=60,
        )
    item["appliedAtMs"] = int(time.time() * 1000)
    atomic(STATE / "media.json", item)
    return item


def import_files(paths):
    copied = []
    for value in paths:
        source = (
            Path(
                urllib.parse.unquote(urllib.parse.urlparse(value).path)
                if str(value).startswith("file:")
                else value
            )
            .expanduser()
            .resolve()
        )
        item = describe(source)
        if source.is_relative_to(LIBRARY.resolve()):
            copied.append(str(source))
            continue
        folder = LIBRARY / ("Dynamic" if item["dynamic"] else "Static")
        folder.mkdir(parents=True, exist_ok=True)
        target = folder / source.name
        index = 1
        while target.exists():
            target = folder / (source.stem + "-" + str(index) + source.suffix)
            index += 1
        temporary = target.with_name(target.name + ".importing")
        try:
            shutil.copy2(source, temporary)
            temporary.chmod(0o600)
            temporary.replace(target)
        finally:
            temporary.unlink(missing_ok=True)
        copied.append(str(target))
    return copied


@functools.lru_cache(maxsize=8)
def demo_module(name):
    if name in {
        "classic-state",
        "branding",
        "file-icons",
        "lock-config",
        "lock-dashboard",
    }:
        import nacre_native_palette

        return getattr(nacre_native_palette, name.replace("-", "_"))
    import importlib.util

    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), Path(__file__).with_name(name + ".py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def demo_scenes():
    """Honor private scene choices without distributing personal artwork."""
    values = demo_module("welcome").preferences(HOME).get("demoWallpapers", [])
    if (
        isinstance(values, list)
        and len(values) == 4
        and all(isinstance(value, str) for value in values)
        and len(set(values)) == 4
    ):
        try:
            return [
                dict(describe(value), license="", artist="", source="", licenseUrl="")
                for value in values
            ]
        except (OSError, ValueError, subprocess.SubprocessError):
            pass
    from nacre_native_config import _binary

    result = subprocess.run(
        [str(_binary()), "demo-wallpapers", "--home", str(HOME), "--fast"],
        check=True,
        stdout=subprocess.PIPE,
        text=True,
        timeout=60,
    )
    return json.loads(result.stdout)


def demo_start():
    """Capture before any choices; preparing images never publishes a palette."""
    import uuid

    palette = demo_module("classic-state")
    folder = HOME / ".local/state/nacre/welcome/demo"
    folder.mkdir(parents=True, exist_ok=True)
    with palette.publication_lock(HOME):
        scheme = read(HOME / ".local/state/nacre/scheme.json", None)
        if scheme is None:
            scheme = read(Path(__file__).with_name("reference-style.json"), {})
        palette.validated_colors(scheme)
        original = media_state()
        last = STATE / "last.txt"
        poster = original.get("poster") or (
            last.read_text().strip() if last.exists() else ""
        )
        if not poster:
            # A pristine desktop can have only the reference surface. Preserve
            # that flat starting scene as an image understood by the same publisher.
            blank = (
                HOME
                / ".cache/nacre/demo-wallpapers"
                / ("starting-" + scheme["colours"]["background"] + ".png")
            )
            blank.parent.mkdir(parents=True, exist_ok=True)
            if not blank.exists():
                Image.new("RGB", (16, 16), "#" + scheme["colours"]["background"]).save(
                    blank
                )
            poster = str(blank)
        if not Path(poster).is_file():
            raise ValueError(
                tr(
                    "helper.wallpaper-media.your-starting-wallpaper-is-unavailable-restore-its-file-before-tr",
                    "Your starting wallpaper is unavailable. Restore its file before trying the demo.",
                )
            )
        if not original:
            original = describe(poster)
        pref = read(PREFS, {})
        snapshot = {
            "version": 1,
            "scheme": scheme,
            "poster": poster,
            "media": original,
            "preferences": {
                key: pref[key] for key in palette.DEMO_PREFERENCES if key in pref
            },
        }
        token = uuid.uuid4().hex
        atomic(folder / (token + ".json"), snapshot)
        # Transient private baselines: bound retained history, never release trees.
        for old in sorted(
            folder.glob("*.json"),
            key=lambda path: path.stat().st_mtime_ns,
            reverse=True,
        )[10:]:
            old.unlink()
        scenes = demo_scenes()
    entries = [
        dict(
            describe(item["path"]),
            name=item["name"],
            license=item["license"],
            artist=item.get("artist", ""),
            licenseUrl=item.get("licenseUrl", ""),
            source=item.get("source", ""),
        )
        for item in scenes
    ]
    return {"demoEntries": entries, "snapshot": token}


def demo_restore(token):
    if not isinstance(token, str) or not re.fullmatch(r"[0-9a-f]{32}", token):
        raise ValueError(
            tr(
                "helper.wallpaper-media.reopen-welcome-to-capture-a-starting-point",
                "Reopen Welcome to capture a starting point.",
            )
        )
    snapshot = read(HOME / ".local/state/nacre/welcome/demo" / (token + ".json"), {})
    if snapshot.get("version") != 1:
        raise ValueError(
            tr(
                "helper.wallpaper-media.the-demo-starting-point-expired-reopen-welcome-to-try-again",
                "The demo starting point expired. Reopen Welcome to try again.",
            )
        )
    item = demo_module("classic-state").restore_demo(HOME, snapshot)
    return {"media": item, "preferences": settings()}


def serve(input_stream=None, output_stream=None):
    """One serial publisher transport; blocking stdin consumes no idle CPU."""
    input_stream = sys.stdin if input_stream is None else input_stream
    output_stream = sys.stdout if output_stream is None else output_stream
    import contextlib

    os.environ["NACRE_PRESENTATION_STREAM"] = "1"
    print(json.dumps({"ready": True}), file=output_stream, flush=True)
    for line in input_stream:
        try:
            if len(line) > 65536:
                raise ValueError(
                    tr(
                        "helper.wallpaper-media.palette-request-is-too-large",
                        "Palette request is too large",
                    )
                )
            request = json.loads(line)
            action, value = request["action"], request["value"]
            with contextlib.redirect_stdout(output_stream):
                if action == "select" and isinstance(value, str):
                    result = select(value)
                elif action in ("theme", "preferences") and isinstance(value, dict):
                    result = theme(value) if action == "theme" else preference(value)
                else:
                    raise ValueError(
                        tr(
                            "helper.wallpaper-media.invalid-palette-request",
                            "Invalid palette request",
                        )
                    )
            response = {"done": True, "code": 0, "result": result}
        except (
            OSError,
            ValueError,
            KeyError,
            TypeError,
            subprocess.SubprocessError,
        ) as error:
            response = {"done": True, "code": 1, "result": {"error": str(error)[:240]}}
        print(json.dumps(response), file=output_stream, flush=True)
    return 0


def main():
    p = argparse.ArgumentParser()
    p.add_argument(
        "action",
        choices=[
            "catalog",
            "demo-start",
            "demo-restore",
            "select",
            "preferences",
            "theme",
            "import",
            "pick",
            "session",
            "warm",
            "serve",
        ],
    )
    p.add_argument("path", nargs="?")
    p.add_argument("--stream-presentation", action="store_true")
    a = p.parse_args()
    if a.stream_presentation:
        os.environ["NACRE_PRESENTATION_STREAM"] = "1"
    try:
        if a.action == "serve":
            return serve()
        if a.action == "session":
            display = subprocess.check_output(
                [
                    "loginctl",
                    "show-user",
                    str(os.getuid()),
                    "--property=Display",
                    "--value",
                ],
                text=True,
                timeout=3,
            ).strip()
            if not display:
                result = {"locked": False, "path": ""}
            else:
                locked = (
                    subprocess.check_output(
                        [
                            "loginctl",
                            "show-session",
                            display,
                            "--property=LockedHint",
                            "--value",
                        ],
                        text=True,
                        timeout=3,
                    ).strip()
                    == "yes"
                )
                raw = subprocess.check_output(
                    [
                        "busctl",
                        "--system",
                        "call",
                        "org.freedesktop.login1",
                        "/org/freedesktop/login1",
                        "org.freedesktop.login1.Manager",
                        "GetSession",
                        "s",
                        display,
                    ],
                    text=True,
                    timeout=3,
                ).strip()
                result = {"locked": locked, "path": raw.split(" ", 1)[1].strip('"')}
        elif a.action == "demo-start":
            result = demo_start()
        elif a.action == "demo-restore":
            result = demo_restore(a.path)
        elif a.action == "catalog":
            result = catalog()
        elif a.action == "warm":
            result = warm()
        elif a.action == "select":
            result = select(a.path)
        elif a.action in ("preferences", "theme"):
            value = json.loads(sys.stdin.readline())
            result = theme(value) if a.action == "theme" else preference(value)
        else:
            if a.action == "pick":
                picker = subprocess.run(
                    [
                        "zenity",
                        "--file-selection",
                        "--multiple",
                        "--separator=\n",
                        "--title=Add wallpapers",
                        "--file-filter=Wallpapers | *.png *.jpg *.jpeg *.webp *.gif *.mp4 *.webm *.mkv *.mov *.m4v",
                    ],
                    capture_output=True,
                    text=True,
                )
                if picker.returncode:
                    print(json.dumps({"cancelled": True}))
                    return
                paths = picker.stdout.strip().splitlines()
            else:
                paths = json.loads(sys.stdin.readline())
            result = {"imported": import_files(paths)}
        print(json.dumps(result))
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        RuntimeError,
        subprocess.SubprocessError,
        Image.DecompressionBombError,
    ) as e:
        print(json.dumps({"error": str(e)}))
        sys.exit(1)


if __name__ == "__main__":
    main()
