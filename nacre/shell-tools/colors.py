# Use the checkout engine in tests and the immutable runtime engine when installed.
#!/usr/bin/env python3
"""Nacre Colors: private previews, saved choices and the existing publisher."""

import argparse
from nacre_i18n import tr
from contextlib import contextmanager
import fcntl
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import re
import sys
import signal
import time

# Local source tests use the same package as the immutable deployed environment.
source = Path(__file__).parents[1] / "shell-cli/src"
if source.is_dir():
    sys.path.insert(0, str(source))
from orient import ENGINE_ID, FORMAT_VERSION  # noqa: E402
from orient.engine import from_image  # noqa: E402
from orient.accessibility import audit  # noqa: E402
from orient.extract import sample_color  # noqa: E402
from orient.colour import lch  # noqa: E402
from orient.export import export as export_templates  # noqa: E402
from orient.palette import PERSONALITIES, background_policy, validate  # noqa: E402
from PIL import Image, ImageDraw, ImageFont  # noqa: E402

HOME = Path.home()
HERE = Path(__file__).parent
CACHE = HOME / ".cache/nacre/colors"
STORE = HOME / ".config/nacre/colors.json"
EXPORTS = HOME / "Documents/Nacre Colors"


def module(name):
    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), HERE / (name + ".py")
    )
    item = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(item)
    return item


media = module("wallpaper-media")


def load(path, default):
    try:
        return json.loads(path.read_text())
    except FileNotFoundError:
        return default


def stored():
    return {
        "version": 1,
        "wallpapers": {},
        "favorites": [],
        "history": [],
        **load(STORE, {}),
    }


@contextmanager
def store_lock():
    STORE.parent.mkdir(parents=True, exist_ok=True)
    with STORE.with_suffix(".lock").open("a") as stream:
        os.chmod(stream.name, 0o600)
        fcntl.flock(stream, fcntl.LOCK_EX)
        yield


def signature(path):
    path = Path(path).resolve(strict=True)
    stat = path.stat()
    return [
        str(path),
        stat.st_dev,
        stat.st_ino,
        stat.st_size,
        stat.st_mtime_ns,
        stat.st_ctime_ns,
    ]


def validate_request(request):
    if not isinstance(request, dict):
        raise ValueError(
            tr("helper.colors.invalid-color-request", "Invalid color request")
        )
    result = {
        "mode": "dark",
        "personality": "natural",
        "overrides": {},
        "brightness": 0.0,
        "saturation": 1.0,
        "hour": time.localtime().tm_hour,
        "autoMode": False,
        "tideAutomatic": False,
        "timeTint": False,
        "workspaceColors": False,
        **request,
    }
    result["personality"], result["backgroundFromWallpaper"] = background_policy(
        result["personality"], result.get("backgroundFromWallpaper")
    )
    if (
        result["mode"] not in ("dark", "light")
        or result["personality"] not in PERSONALITIES
    ):
        raise ValueError(
            tr(
                "helper.colors.choose-a-supported-color-personality-and-mode",
                "Choose a supported color personality and mode",
            )
        )
    if not isinstance(result.get("image"), str) or not Path(result["image"]).is_file():
        raise ValueError(
            tr("helper.colors.choose-a-local-wallpaper", "Choose a local wallpaper")
        )
    if type(result["hour"]) is not int or not 0 <= result["hour"] <= 23:
        raise ValueError(
            tr(
                "helper.colors.choose-an-hour-from-zero-to-23",
                "Choose an hour from zero to 23",
            )
        )
    for key in ("brightness", "saturation"):
        if not isinstance(result[key], (int, float)) or not math.isfinite(result[key]):
            raise ValueError(
                tr("helper.colors.invalid-color-adjustment", "Invalid color adjustment")
            )
    for key in ("autoMode", "tideAutomatic", "workspaceColors", "timeTint"):
        if type(result[key]) is not bool:
            raise ValueError(
                tr(
                    "helper.colors.expected-a-boolean-option",
                    "Expected a boolean option",
                )
            )
    if result["tideAutomatic"]:
        result["hour"] = time.localtime().tm_hour
    if (result["personality"] == "tide" or result["timeTint"]) and result["autoMode"]:
        result["mode"] = "light" if 6 <= result["hour"] < 19 else "dark"
    result["image"] = str(Path(result["image"]).resolve())
    return result


def palette_args(request):
    return {
        key: request[key]
        for key in (
            "mode",
            "personality",
            "overrides",
            "brightness",
            "saturation",
            "hour",
        )
    } | {
        "accent": request.get("accent"),
        "background_from_wallpaper": request["backgroundFromWallpaper"],
        "time_tint": request["timeTint"],
    }


def image_colors(data, limit=8):
    """Show distinct observed families alongside major shadows, without double counting.

    Family members identify the underlying bins; their coverage and regions are
    already summed by Orient. Tiny contrasting details get room before repeated
    neutral shades, while the percentages remain actual sampled image area.
    """
    source = data.get("source", {})
    families = source.get("families", [])
    chosen = []
    for family in sorted(families, key=lambda item: -item.get("score", 0)):
        if family["coverage"] < 0.003:
            continue
        hue = lch(family["hex"])[2]
        if all(
            abs((hue - lch(item["hex"])[2] + 180) % 360 - 180) >= 35 for item in chosen
        ):
            chosen.append(family)
        if len(chosen) == 5:
            break
    # All family members are disjoint from the remaining raw neutral/shadow bins.
    members = {value for item in families for value in item.get("members", [])}
    leftovers = [
        item for item in source.get("clusters", []) if item["hex"] not in members
    ]
    neutrals = []
    for item in sorted(leftovers, key=lambda item: -item["coverage"]):
        light = lch(item["hex"])[0]
        if all(abs(light - lch(other["hex"])[0]) >= 0.10 for other in neutrals):
            neutrals.append(item)
        if len(neutrals) == limit - len(chosen):
            break
    selected = sorted(chosen + neutrals, key=lambda item: -item["coverage"])
    return selected[:limit]


def stage(request):
    request = validate_request(request)
    if request.get("pick"):
        point = request.pop("pick")
        item = media.describe(request["image"])
        request["accent"] = sample_color(item["poster"], point["x"], point["y"])
    item = media.describe(request["image"])
    stamp = signature(request["image"])
    saved = stored()
    favorite = next(
        (f for f in saved["favorites"] if f["id"] == request.get("favoriteId")), None
    )
    if request.get("favoriteId") and not favorite:
        raise ValueError(
            tr(
                "helper.colors.that-saved-palette-no-longer-exists",
                "That saved palette no longer exists",
            )
        )
    identity = hashlib.sha256(
        json.dumps([ENGINE_ID, stamp, request, favorite], sort_keys=True).encode()
    ).hexdigest()
    try:
        cached = load(CACHE / (identity + ".json"), {})
    except (OSError, ValueError):
        cached = {}
    if (
        isinstance(cached, dict)
        and cached.get("engine") == ENGINE_ID
        and cached.get("signature") == stamp
        and cached.get("request") == request
        and "comparisons" in cached
    ):
        try:
            validate(cached["palette"]["colours"])
            return preview_result(identity, cached)
        except (KeyError, TypeError, ValueError):
            pass
    if favorite:
        data = dict(
            favorite["palette"],
            mode=request["mode"],
            colours=favorite["modes"][request["mode"]],
            input={
                **favorite["palette"].get("input", {}),
                "mode": request["mode"],
                "personality": request["personality"],
                "background_from_wallpaper": request["backgroundFromWallpaper"],
                "time_tint": request["timeTint"],
            },
        )
        validate(data["colours"])
        data["accessibility"] = audit(data["colours"])
        data["source"] = dict(data["source"], options=[])
        comparisons = []
    else:
        data = from_image(request["image"], **palette_args(request))
        comparisons = []
        for personality in PERSONALITIES if request.get("compare") else ():
            comparison = (
                data
                if personality == request["personality"]
                else from_image(
                    request["image"],
                    **(palette_args(request) | {"personality": personality}),
                )
            )
            comparisons.append(
                {
                    "personality": personality,
                    "mode": comparison["mode"],
                    "colours": comparison["colours"],
                    "minimumContrast": comparison["accessibility"][
                        "minimumTextContrast"
                    ],
                }
            )
    record = {
        "version": 1,
        "engine": ENGINE_ID,
        "signature": stamp,
        "request": request,
        "palette": data,
        "poster": item["poster"],
        "media": item,
        "comparisons": comparisons,
        "imageColors": image_colors(data),
        "favorite": bool(favorite),
    }
    if signature(request["image"]) != stamp:
        raise ValueError(
            tr(
                "helper.colors.wallpaper-changed-while-previewing-try-again",
                "Wallpaper changed while previewing; try again",
            )
        )
    media.atomic(CACHE / (identity + ".json"), record)
    # Staging is private and bounded; never a recurring cleanup timer.
    for stale in sorted(
        CACHE.glob("*.json"), key=lambda p: p.stat().st_mtime, reverse=True
    )[64:]:
        stale.unlink(missing_ok=True)
    return preview_result(identity, record)


def preview_result(identity, record):
    data, item = record["palette"], record["media"]
    swatches = record["imageColors"]
    return {
        "id": identity,
        "request": record["request"],
        "palette": data,
        "comparisons": record["comparisons"],
        "imageColors": swatches,
        "otherCoverage": max(0, 1 - sum(color["coverage"] for color in swatches)),
        "thumbnail": item["preview"],
        "image": item["path"],
        "name": item["name"],
        "favorite": record["favorite"],
        "frames": data.get("source", {}).get("sample", {}).get("frames", 1),
    }


def record(identity):
    if not isinstance(identity, str) or not re.fullmatch("[0-9a-f]{64}", identity):
        raise ValueError(
            tr("helper.colors.preview-is-not-valid", "Preview is not valid")
        )
    data = load(CACHE / (identity + ".json"), {})
    if data.get("version") != 1 or data.get("engine") != ENGINE_ID:
        raise ValueError(
            tr(
                "helper.colors.preview-expired-preview-it-again",
                "Preview expired; preview it again",
            )
        )
    if signature(data["request"]["image"]) != data["signature"]:
        raise ValueError(
            tr(
                "helper.colors.wallpaper-changed-preview-it-again",
                "Wallpaper changed; preview it again",
            )
        )
    if data["palette"].get("formatVersion") != FORMAT_VERSION:
        raise ValueError(
            tr("helper.colors.unsupported-palette-format", "Unsupported palette format")
        )
    validate(data["palette"]["colours"])
    return data


def catalog():
    data = media.catalog()
    values = stored()
    scheme = load(HOME / ".local/state/nacre/scheme.json", {})
    for item in data["entries"]:
        prepared = load(
            media.palette_marker(
                item["poster"], scheme.get("flavour", "default"), source=item["path"]
            ),
            {},
        )
        item["families"] = sorted(
            {
                o["name"].split(" ")[0]
                for o in prepared.get("source", {}).get("options", [])
            }
        )
        item["colors"] = prepared.get("source", {}).get("options", [])
    return {
        "current": data["media"].get("path", ""),
        "entries": data["entries"],
        "favorites": [
            {k: f[k] for k in ("id", "name", "swatches")} for f in values["favorites"]
        ],
        "history": values["history"],
        "choices": values["wallpapers"],
        "exports": str(EXPORTS),
    }


def favorite(identity, name=""):
    data = record(identity)
    modes = {data["palette"]["mode"]: data["palette"]["colours"]}
    other = "light" if data["palette"]["mode"] == "dark" else "dark"
    existing = next(
        (
            f
            for f in stored()["favorites"]
            if f["id"] == data["request"].get("favoriteId")
        ),
        None,
    )
    modes[other] = (
        existing["modes"][other]
        if existing
        else from_image(
            data["request"]["image"],
            **(palette_args(data["request"]) | {"mode": other}),
        )["colours"]
    )
    key = hashlib.sha256(json.dumps(modes, sort_keys=True).encode()).hexdigest()[:16]
    value = {
        "id": key,
        "name": str(name).strip()[:64]
        or data["request"]["personality"].title()
        + " "
        + data["palette"]["source"]["selected"],
        "modes": modes,
        "palette": data["palette"],
        "swatches": [
            data["palette"]["colours"][k] for k in ("primary", "secondary", "tertiary")
        ],
    }
    with store_lock():
        values = stored()
        values["favorites"] = [
            value,
            *[f for f in values["favorites"] if f["id"] != key],
        ][:64]
        media.atomic(STORE, values)
    return {"saved": value["name"]}


def apply(identity, live=True):
    data = record(identity)
    request = data["request"]
    palette = data["palette"]
    publisher = module("classic-state")
    image = request["image"]
    item = data["media"]
    thumbnail = (
        HOME / ".cache/nacre/wallpapers" / palette["source"]["digest"] / "thumbnail.jpg"
    )
    if not thumbnail.is_file():
        thumbnail.parent.mkdir(parents=True, exist_ok=True)
        with Image.open(item["preview"]) as original:
            original.convert("RGB").save(thumbnail, quality=85)
    # Preference writes and publication share the established publisher's lock.
    with publisher.publication_lock(HOME), store_lock():
        values = stored()
        previous_values = json.loads(json.dumps(values))
        previous_preferences = media.settings()
        choice = {
            key: request[key]
            for key in (
                "personality",
                "backgroundFromWallpaper",
                "overrides",
                "brightness",
                "saturation",
                "hour",
                "autoMode",
                "tideAutomatic",
                "timeTint",
                "workspaceColors",
            )
        }
        if request.get("accent"):
            choice["accent"] = request["accent"]
        choice["mode"] = palette["mode"]
        if request.get("favoriteId"):
            choice["favoriteId"] = request["favoriteId"]
        choice["poster"] = item["poster"]
        values["wallpapers"][image] = choice
        values["wallpapers"][str(Path(item["poster"]).resolve())] = choice
        media.atomic(STORE, values)
        patch = {
            "palettePreset": "favorite:" + request["favoriteId"]
            if request.get("favoriteId")
            else "wallpaper",
            "paletteMode": palette["mode"],
            "paletteHarmony": request["personality"] == "harmony",
            "palettePersonality": request["personality"],
            "paletteBackgroundFromWallpaper": request["backgroundFromWallpaper"],
        }
        try:
            media.preference(patch)
            if not publisher.commit_prepared(
                HOME,
                item["poster"],
                palette,
                thumbnail,
                live=live,
                allow_mode_change=True,
            ):
                raise ValueError(
                    tr(
                        "helper.colors.the-desktop-cannot-apply-that-prepared-palette-choose-a-wallpaper",
                        "The desktop cannot apply that prepared palette; choose a wallpaper palette first",
                    )
                )
            item["appliedAtMs"] = int(time.time() * 1000)
            media.atomic(media.STATE / "media.json", item)
        except BaseException:
            media.atomic(STORE, previous_values)
            media.preference({key: previous_preferences[key] for key in patch})
            raise
        history = {
            "id": identity[:16],
            "name": item["name"],
            "time": int(time.time()),
            "request": request,
        }
        values["lastApplied"] = identity
        values["history"] = [
            history,
            *[h for h in values["history"] if h["request"] != request],
        ][:20]
        media.atomic(STORE, values)
    return {"applied": True, "name": item["name"]}


def output_folder(identity):
    record(identity)
    folder = EXPORTS / (time.strftime("%Y%m%d-%H%M%S") + "-" + identity[:8])
    index = 1
    while folder.exists():
        folder = folder.with_name(folder.name + "-" + str(index))
        index += 1
    folder.mkdir(parents=True)
    return folder


def export(identity):
    data = record(identity)
    folder = output_folder(identity)
    favorite = next(
        (
            f
            for f in stored()["favorites"]
            if f["id"] == data["request"].get("favoriteId")
        ),
        None,
    )
    schemes = favorite["modes"] if favorite else None
    paths = export_templates(data["palette"], folder, schemes=schemes)
    media.atomic(folder / "palette.json", data["palette"])
    (folder / "README.txt").write_text(
        "Generated by Nacre Colors / Orient. These are exports, not installed themes.\nVS Code: import the color-theme JSON with your extension/theme setup.\nFirefox: load the theme manifest as a temporary add-on for testing; distribution requires Mozilla signing.\nObsidian: enable the CSS snippet. Neovim: load the Lua file. btop: put the theme in its themes directory.\nSpotify requires optional Spicetify; Discord requires an optional user-theme client. Neither is installed or modified here.\nKitty/Foot/Alacritty/GTK/rofi/Hyprland: load explicitly from your own configuration.\nAdd custom .in templates to ~/.config/nacre/colors-templates, using {{role}} or Matugen {{ colors.primary.default.hex }} tokens. Filters/blocks are not supported.\n"
    )
    custom = HOME / ".config/nacre/colors-templates"
    if custom.is_dir():
        paths += export_templates(
            data["palette"], folder / "custom", custom, schemes=schemes
        )
    return {"directory": str(folder), "files": paths}


def card(identity):
    data = record(identity)
    folder = output_folder(identity)
    colors = data["palette"]["colours"]
    output = Image.new("RGB", (1200, 850), "#" + colors["surface"])
    with Image.open(data["media"]["preview"]) as original:
        from PIL import ImageOps

        photo = ImageOps.fit(original.convert("RGB"), (1200, 540))
        output.paste(photo)
    draw = ImageDraw.Draw(output)
    font = ImageFont.truetype("DejaVuSans.ttf", 27)
    for index, role in enumerate(
        ("primary", "secondary", "tertiary", "surface", "onSurface")
    ):
        x = 30 + index * 235
        draw.rounded_rectangle((x, 570, x + 205, 650), 15, fill="#" + colors[role])
        draw.text((x, 675), role, font=font, fill="#" + colors["onSurface"])
        draw.text(
            (x, 716), "#" + colors[role], font=font, fill="#" + colors["onSurface"]
        )
    draw.text(
        (30, 786),
        "Made with Nacre · " + data["request"]["personality"].title() + " · Orient",
        font=font,
        fill="#" + colors["onSurface"],
    )
    target = folder / "palette-card.png"
    output.save(target)
    return {"directory": str(folder), "card": str(target)}


def serve(input_stream=None, output_stream=None):
    """Read-only previews, one request at a time; no publication or idle polling."""
    input_stream = input_stream or sys.stdin
    output_stream = output_stream or sys.stdout
    print(json.dumps({"ready": True}), file=output_stream, flush=True)
    while True:
        line = input_stream.readline(65537)
        if not line:
            return 0
        sequence = -1
        try:
            if len(line) > 65536:
                raise ValueError(
                    tr(
                        "helper.colors.preview-request-is-too-large",
                        "Preview request is too large",
                    )
                )
            request = json.loads(line)
            sequence = request["sequence"]
            if type(sequence) is not int or sequence < 0:
                raise ValueError(
                    tr(
                        "helper.colors.invalid-preview-sequence",
                        "Invalid preview sequence",
                    )
                )
            result = stage(request["request"])
            response = {"sequence": sequence, "result": result}
        except (OSError, ValueError, KeyError, TypeError) as error:
            response = {"sequence": sequence, "error": str(error)[:240]}
        print(json.dumps(response), file=output_stream, flush=True)
        if len(line) > 65536:
            return 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "action",
        choices=(
            "catalog",
            "preview",
            "serve",
            "apply",
            "favorite",
            "export",
            "card",
            "delete",
        ),
    )
    args = parser.parse_args()
    if args.action == "serve":
        return serve()
    try:
        request = (
            json.loads(sys.stdin.readline(65537)) if args.action != "catalog" else {}
        )
        if args.action == "catalog":
            result = catalog()
        elif args.action == "preview":
            result = stage(request)
        elif args.action == "apply":
            result = apply(request["id"])
        elif args.action == "favorite":
            result = favorite(request["id"], request.get("name", ""))
        elif args.action == "export":
            result = export(request["id"])
        elif args.action == "card":
            result = card(request["id"])
        else:
            with store_lock():
                values = stored()
                values["favorites"] = [
                    f for f in values["favorites"] if f["id"] != request["id"]
                ]
                media.atomic(STORE, values)
            result = {"deleted": True}
        print(json.dumps(result))
    except (OSError, ValueError, KeyError, TypeError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":

    def cancelled(signum, frame):
        raise SystemExit(143)

    signal.signal(signal.SIGTERM, cancelled)
    raise SystemExit(main())
