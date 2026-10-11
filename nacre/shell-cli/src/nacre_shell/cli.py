"""Compatible palette/wallpaper CLI, newly implemented from consumer contracts."""

import argparse
import json
import os
from pathlib import Path
import random
import subprocess
import sys

from PIL import Image, ImageOps

from . import ENGINE_ID
from .colour import clean
from .engine import default_palette, from_image
from .palette import VARIANTS, PERSONALITIES, background_policy, validate
from .storage import atomic, commit_lock, link, read, roots, write_json

IMAGE_EXTENSIONS = {".jpg", ".jpeg", ".png", ".webp", ".gif", ".tif", ".tiff"}


def parser():
    cli = argparse.ArgumentParser(description="Orient: Nacre wallpaper and palette engine")
    cli.add_argument("-v", "--version", action="version", version=ENGINE_ID)
    commands = cli.add_subparsers(dest="command", required=True)
    scheme = commands.add_parser("scheme", help="manage the colour scheme")
    actions = scheme.add_subparsers(dest="action", required=True)
    listing = actions.add_parser("list")
    for short, long in (("n", "names"), ("f", "flavours"), ("m", "modes"), ("v", "variants")):
        listing.add_argument("-" + short, "--" + long, action="store_true")
    get = actions.add_parser("get")
    for short, long in (("n", "name"), ("f", "flavour"), ("m", "mode"), ("v", "variant")):
        get.add_argument("-" + short, "--" + long, action="store_true")
    get.add_argument("--json", action="store_true")
    setting = actions.add_parser("set")
    setting.add_argument("--notify", action="store_true")
    setting.add_argument("-r", "--random", action="store_true")
    setting.add_argument("-n", "--name", choices=("default", "dynamic"))
    setting.add_argument("-f", "--flavour", choices=("default", "hard"))
    setting.add_argument("-m", "--mode", choices=("dark", "light"))
    setting.add_argument("-v", "--variant", choices=VARIANTS)
    setting.add_argument("--accent", type=clean, help="explicit six-digit accent; 'auto' is accepted separately")
    setting.add_argument("--auto-accent", action="store_true")
    wallpaper = commands.add_parser("wallpaper", help="manage the wallpaper")
    choose = wallpaper.add_mutually_exclusive_group()
    choose.add_argument("-p", "--print", nargs="?", const="")
    choose.add_argument("-r", "--random", nargs="?", const="")
    choose.add_argument("-f", "--file")
    wallpaper.add_argument("-n", "--no-filter", action="store_true")
    wallpaper.add_argument("-t", "--threshold", type=float, default=0.5)
    wallpaper.add_argument("-N", "--no-smart", action="store_true")
    wallpaper.add_argument("--accent", type=clean)
    wallpaper.add_argument("--source", help="optional animated analysis source for a prepared poster")
    wallpaper.add_argument("--mode", choices=("dark", "light"))
    wallpaper.add_argument(
        "--harmony",
        action=argparse.BooleanOptionalAction,
        default=None,
        help="prefer related, substantial supporting colors",
    )
    wallpaper.add_argument("--personality", choices=(*PERSONALITIES, "source"))
    wallpaper.add_argument("--background-from-wallpaper", action=argparse.BooleanOptionalAction, default=None)
    return cli


def current():
    data = read(roots()[1] / "scheme.json", {})
    return {"name": "dynamic", "mode": "dark", "variant": "tonalspot", "flavour": "default", **data}


def current_image():
    state = roots()[1]
    for name in ("wallpaper/path.txt", "wallpaper/last.txt"):
        path = state / name
        if path.exists():
            return path.read_text().strip()
    return None


def harmony_setting(args):
    explicit = getattr(args, "harmony", None)
    if explicit is not None:
        return explicit
    return read(roots()[0] / "wallpaper-picker.json", {}).get("paletteHarmony") is True


def personality_setting(args):
    explicit = getattr(args, "personality", None)
    if explicit:
        return explicit
    harmony = getattr(args, "harmony", None)
    if harmony is not None:
        return "harmony" if harmony else "natural"
    preferences = read(roots()[0] / "wallpaper-picker.json", {})
    return preferences.get("palettePersonality") or ("harmony" if preferences.get("paletteHarmony") else "natural")


def image_settings(path, args):
    preferences = read(roots()[0] / "colors.json", {})
    choice = preferences.get("wallpapers", {}).get(str(Path(path).resolve()), {})
    overrides = dict(choice.get("overrides", {}))
    if getattr(args, "accent", None) or getattr(args, "auto_accent", False):
        overrides.pop("primary", None)
    desktop = read(roots()[0] / "wallpaper-picker.json", {})
    background = getattr(args, "background_from_wallpaper", None)
    if background is None:
        background = choice.get(
            "backgroundFromWallpaper",
            True if choice.get("personality") == "source" else desktop.get("paletteBackgroundFromWallpaper"),
        )
    personality, background = background_policy(personality_setting(args), background)
    return {
        "personality": personality,
        "background_from_wallpaper": background,
        "overrides": overrides,
        "brightness": choice.get("brightness", 0.0),
        "time_tint": choice.get("timeTint", False),
        "saturation": choice.get("saturation", 1.0),
        "hour": __import__("datetime").datetime.now().hour if choice.get("tideAutomatic") else choice.get("hour", 12),
    }


def analysis_path(path, args):
    explicit = getattr(args, "source", None)
    if explicit:
        return explicit
    media = read(roots()[1] / "wallpaper/media.json", {})
    if media.get("poster") == str(Path(path).resolve()) and Path(media.get("path", "")).is_file():
        return media["path"]
    return path


def options(path, args, data):
    config = read(roots()[0] / "cli.json", {})
    overrides = config.get("orient", {}).get("accents", {})
    accent = getattr(args, "accent", None) or overrides.get(str(Path(path).resolve()))
    choice = read(roots()[0] / "colors.json", {}).get("wallpapers", {}).get(str(Path(path).resolve()), {})
    accent = getattr(args, "accent", None) or choice.get("accent") or accent
    return from_image(
        analysis_path(path, args),
        mode=getattr(args, "mode", None) or data["mode"],
        variant=data["variant"],
        flavour=data["flavour"],
        accent=accent,
        smart=False,
        **image_settings(path, args),
    )


def save_scheme(data):
    state = roots()[1]
    validate(data["colours"])
    write_json(state / "scheme.json", data)
    atomic(state / "scheme/current-mode.txt", data["mode"])
    atomic(state / "scheme/current.txt", "".join(f"{key} {value}\n" for key, value in data["colours"].items()))


def scheme(args):
    if args.action == "list":
        values = {
            "names": ["default", "dynamic"],
            "flavours": ["default", "hard"],
            "modes": ["light", "dark"],
            "variants": list(VARIANTS),
        }
        selected = [key for key in values if getattr(args, key)]
        print("\n".join(v for key in (selected or values) for v in values[key]))
        return
    data = current()
    if args.action == "get":
        selected = [key for key in ("name", "flavour", "mode", "variant") if getattr(args, key)]
        if selected:
            print("\n".join(str(data[key]) for key in selected))
        elif args.json:
            print(json.dumps(data))
        else:
            print("Current scheme:")
            for key in ("name", "flavour", "mode", "variant"):
                print(f"    {key.title()}: {data[key]}")
            for key, value in data.get("colours", {}).items():
                print(f"    {key}: {value}")
        return
    if args.accent and args.auto_accent:
        raise ValueError("Choose an explicit accent or automatic selection, not both")
    with commit_lock():
        data = current()
        for key in ("name", "flavour", "mode", "variant"):
            if getattr(args, key) is not None:
                data[key] = getattr(args, key)
        if args.random:
            data["flavour"] = random.choice(("default", "hard"))
        path = current_image()
        if data["name"] == "dynamic":
            if not path:
                raise ValueError("Choose a wallpaper before selecting a dynamic palette")
            config_path = roots()[0] / "cli.json"
            config = read(config_path, {})
            accents = dict(config.get("orient", {}).get("accents", {}))
            if args.auto_accent:
                accents.pop(str(Path(path).resolve()), None)
            elif args.accent:
                accents[str(Path(path).resolve())] = args.accent
            # Generate before committing either preference or palette.
            accent = accents.get(str(Path(path).resolve()))
            generated = from_image(
                analysis_path(path, args),
                data["mode"],
                data["variant"],
                data["flavour"],
                accent,
                **image_settings(path, args),
            )
            if args.accent or args.auto_accent:
                config["orient"] = {**config.get("orient", {}), "accents": accents}
                write_json(config_path, config)
                choices_path = roots()[0] / "colors.json"
                if choices_path.exists():
                    saved = read(choices_path, {})
                    for key, choice in saved.get("wallpapers", {}).items():
                        if key == str(Path(path).resolve()) or choice.get("poster") == str(Path(path).resolve()):
                            choice["overrides"] = {
                                k: v for k, v in choice.get("overrides", {}).items() if k != "primary"
                            }
                            if args.auto_accent:
                                choice.pop("accent", None)
                            else:
                                choice["accent"] = args.accent
                    write_json(choices_path, saved)
        else:
            generated = default_palette(data["mode"], data["variant"], data["flavour"])
        save_scheme(generated)


def suitable_images(directory, threshold, no_filter):
    paths = sorted(p for p in directory.rglob("*") if p.is_file() and p.suffix.lower() in IMAGE_EXTENSIONS)
    if no_filter:
        return paths
    if not 0 < threshold <= 1:
        raise ValueError("Wallpaper size threshold must be above 0 and at most 1")
    try:
        result = subprocess.run(["hyprctl", "-j", "monitors"], capture_output=True, text=True, timeout=3, check=True)
        monitors = json.loads(result.stdout)
        width = max(m["width"] for m in monitors)
        height = max(m["height"] for m in monitors)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError):
        width, height = 1920, 1080
    valid = []
    for path in paths:
        try:
            with Image.open(path) as image:
                if image.width >= width * threshold and image.height >= height * threshold:
                    valid.append(path)
        except (OSError, ValueError, Image.DecompressionBombError):
            continue
    return valid


def wallpaper(args):
    data = current()
    if args.print is not None:
        path = args.print or current_image()
        if not path:
            raise ValueError("No current wallpaper")
        print(json.dumps(options(path, args, data)))
        return
    if args.random is not None:
        config = read(roots()[0] / "cli.json", {})
        directory = Path(
            args.random or config.get("wallpaper", {}).get("directory", str(Path.home() / "Pictures/Wallpapers"))
        ).expanduser()
        choices = suitable_images(directory, args.threshold, args.no_filter)
        if not choices:
            raise ValueError("No suitable local wallpapers found")
        path = random.choice(choices)
    else:
        path = args.file or current_image()
    if not path:
        raise ValueError("Provide --file, --random or --print")
    path = Path(path).expanduser().resolve(strict=True)
    with commit_lock():
        data = current()
        generated = options(path, args, data)
        config = read(roots()[0] / "cli.json", {})
        # Explicit mode and the desktop's saved paletteMode always win over inference.
        fixed_mode = read(roots()[0] / "wallpaper-picker.json", {}).get("paletteMode")
        if not args.no_smart and not args.mode and fixed_mode not in ("light", "dark"):
            generated = from_image(
                analysis_path(path, args),
                data["mode"],
                data["variant"],
                data["flavour"],
                generated.get("input", {}).get("accent"),
                smart=True,
                **image_settings(path, args),
            )
        if data["name"] == "default":
            generated = default_palette(generated["mode"], data["variant"], data["flavour"])
        digest = generated.get("source", {}).get("digest")
        if not digest:
            import hashlib

            digest = hashlib.sha256(path.read_bytes()).hexdigest()
        thumbnail = roots()[2] / "wallpapers" / digest / "thumbnail.jpg"
        if not thumbnail.exists():
            thumbnail.parent.mkdir(parents=True, exist_ok=True)
            with Image.open(path) as original:
                original.draft("RGB", (640, 640))
                image = ImageOps.exif_transpose(original).convert("RGB")
                image.thumbnail((640, 640), Image.Resampling.LANCZOS)
                temp = thumbnail.with_suffix(".tmp.jpg")
                image.save(temp, quality=85)
                os.replace(temp, thumbnail)
        save_scheme(generated)
        state = roots()[1]
        atomic(state / "wallpaper/path.txt", str(path))
        link(state / "wallpaper/current", path)
        link(state / "wallpaper/thumbnail.jpg", thumbnail)
        hook = config.get("wallpaper", {}).get("postHook")
        if hook:
            subprocess.run(
                ["/bin/sh", "-c", hook, "nacre-wallpaper", str(path)],
                env={**os.environ, "WALLPAPER": str(path), "NACRE_WALLPAPER": str(path)},
                check=True,
            )


def main(argv=None):
    args = parser().parse_args(argv)
    try:
        if args.command == "scheme":
            scheme(args)
        else:
            wallpaper(args)
        return 0
    except (
        OSError,
        ValueError,
        KeyError,
        TypeError,
        subprocess.SubprocessError,
        Image.DecompressionBombError,
        Image.DecompressionBombWarning,
    ) as error:
        print(f"Orient: {error}", file=sys.stderr)
        if getattr(args, "notify", False):
            try:
                subprocess.run(["notify-send", "Orient", str(error)], check=False, timeout=3)
            except (OSError, subprocess.SubprocessError):
                pass
        return 1
