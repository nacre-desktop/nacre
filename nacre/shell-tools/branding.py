#!/usr/bin/env python3
"""One user-approved shell geometry for Nacre web, Qt, lock and terminal branding."""

import io
from nacre_i18n import tr
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import tempfile
from xml.etree import ElementTree as ET

from PIL import Image

HERE = Path(__file__).resolve().parent
MASTER = next(
    p
    for p in (
        HERE.parent / "shell/branding/nacre-master.svg",
        HERE.parent / "source/branding/nacre-master.svg",
    )
    if p.is_file()
)
GEOMETRY = MASTER
VIEWBOX = "2.8 3 58 58"
NS = "{http://www.w3.org/2000/svg}"
APPLICATIONS = ("ai", "brain", "settings", "colors", "notifications")


def asset_names():
    for application in ("", *APPLICATIONS):
        stem = "nacre" + ("-" + application if application else "")
        prefix = application + "-" if application else ""
        for variant in ("full", "compact", "symbolic"):
            yield (
                prefix + variant,
                stem + ("-" + variant if variant != "full" else "") + ".svg",
            )


def templates():
    original = MASTER.read_text()
    root = ET.fromstring(original)
    chambers = [
        node
        for node in root
        if node.tag == NS + "path" and node.get("fill", "").startswith("@")
    ]
    pearl = next(
        node
        for node in root.iter(NS + "circle")
        if node.get("fill") == "url(#nacre-logo-pearl-body)"
    )
    opening = f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{VIEWBOX}">'
    compact = opening + "".join(
        f'<path d="{node.get("d")}" fill="{node.get("fill")}" fill-rule="evenodd"/>'
        for node in chambers
    )
    dot = f'<circle cx="{pearl.get("cx")}" cy="{pearl.get("cy")}" r="{pearl.get("r")}" fill="@HIGHLIGHT@"/>'
    compact += dot + "</svg>"
    symbolic = (
        opening
        + '<path fill="currentColor" fill-rule="evenodd" d="'
        + " ".join(n.get("d") for n in chambers)
        + '"/>'
        + dot.replace("@HIGHLIGHT@", "currentColor")
        + "</svg>"
    )
    full = re.sub(r'viewBox="[^"]+"', f'viewBox="{VIEWBOX}"', original, count=1)
    values = {"full": full, "compact": compact, "symbolic": symbolic}
    for application in APPLICATIONS:
        label = (HERE / (application + "-label.svg.in")).read_text()
        for variant in ("full", "compact", "symbolic"):
            suffix = label
            if variant == "symbolic":
                suffix = re.sub(r"@[A-Z_]+@", "currentColor", suffix)
            values[application + "-" + variant] = values[variant].replace(
                "</svg>", suffix + "</svg>"
            )
    return values


def luminance(value):
    values = [int(value.lstrip("#")[i : i + 2], 16) / 255 for i in (0, 2, 4)]
    values = [
        v / 12.92 if v <= 0.04045 else ((v + 0.055) / 1.055) ** 2.4 for v in values
    ]
    return sum(v * w for v, w in zip(values, (0.2126, 0.7152, 0.0722), strict=True))


def role_colors(colors):
    bg = colors.get("frame", colors.get("surface", "101014")).lstrip("#")

    def clean(value):
        value = str(value).lstrip("#")
        if not re.fullmatch(r"[0-9a-fA-F]{6}", value):
            raise ValueError(
                tr("helper.branding.invalid-logo-color", "Invalid logo color")
            )
        return "#" + value

    foreground = clean(
        colors.get("onSurface", "ffffff" if luminance(bg) < 0.5 else "101014")
    )

    def ratio(a):
        x, y = sorted((luminance(a), luminance(bg)))
        return (y + 0.05) / (x + 0.05)

    def lift(value, minimum=3.4):
        value = clean(value)
        if ratio(value) >= minimum:
            return value
        a = [int(value[i : i + 2], 16) for i in (1, 3, 5)]
        target = (
            foreground
            if ratio(foreground) >= minimum
            else ("#ffffff" if ratio("#ffffff") >= ratio("#000000") else "#000000")
        )
        b = [int(target[i : i + 2], 16) for i in (1, 3, 5)]
        for step in range(1, 21):
            result = "#" + "".join(
                f"{round(x + (y - x) * step / 20):02x}"
                for x, y in zip(a, b, strict=True)
            )
            if ratio(result) >= minimum:
                return result
        return target

    primary = lift(colors.get("primary", "47a99a"))
    return {
        "PRIMARY": primary,
        "ON_PRIMARY": "#ffffff" if luminance(primary) < 0.179 else "#000000",
        "SECONDARY": lift(colors.get("secondary", primary)),
        "AI_PRIMARY": lift(colors.get("primary", primary), 4.5),
        "AI_SECONDARY": lift(colors.get("secondary", primary), 4.5),
        "TERTIARY": lift(colors.get("tertiary", colors.get("secondary", primary))),
        "HIGHLIGHT": primary
        if luminance(bg) > 0.5
        else clean(colors.get("primaryFixed", colors.get("onSurface", "f4f1ef"))),
    }


def svg(template=True, colors=None, variant="full"):
    text = templates()[variant]
    if template:
        return text
    return paint_svg(text, role_colors(colors))


def paint_svg(text, roles):
    for name, value in roles.items():
        text = text.replace("@" + name + "@", value)
    return text.replace("currentColor", roles["PRIMARY"])


def raster(text, size=512):
    binary = shutil.which("rsvg-convert")
    if not binary:
        raise RuntimeError(
            tr(
                "helper.branding.nacre-logo-rasterization-needs-rsvg-convert-from-librsvg",
                "Nacre logo rasterization needs rsvg-convert from librsvg",
            )
        )
    result = subprocess.run(
        [binary, "--format=png", f"--width={size}", f"--height={size}"],
        input=text.encode(),
        capture_output=True,
        check=True,
        timeout=10,
    )
    return Image.open(io.BytesIO(result.stdout)).convert("RGBA")


def cached_png(text, home, size=512):
    """Cache immutable raster art by geometry, palette, size and renderer build."""
    binary = shutil.which("rsvg-convert")
    if not binary:
        raise RuntimeError(
            tr(
                "helper.branding.nacre-logo-rasterization-needs-rsvg-convert-from-librsvg",
                "Nacre logo rasterization needs rsvg-convert from librsvg",
            )
        )
    stat = Path(binary).stat()
    key = hashlib.sha256(
        (text + str(size) + str(stat.st_size) + str(stat.st_mtime_ns)).encode()
    ).hexdigest()
    folder = Path(home) / ".cache/nacre/branding"
    path = folder / (key + ".png")
    try:
        raw = path.read_bytes()
        with Image.open(io.BytesIO(raw)) as image:
            if image.size != (size, size) or image.mode != "RGBA":
                raise ValueError(
                    tr(
                        "helper.branding.invalid-branding-cache",
                        "Invalid branding cache",
                    )
                )
            image.verify()
        return raw
    except (OSError, ValueError):
        pass
    image = raster(text, size)
    stream = io.BytesIO()
    image.save(stream, format="PNG")
    raw = stream.getvalue()
    try:
        atomic(path, raw)
        path.chmod(0o600)
        # Bound disk use; cache misses only prune, never a recurring idle job.
        for stale in sorted(
            folder.glob("*.png"), key=lambda p: p.stat().st_mtime, reverse=True
        )[1024:]:
            stale.unlink(missing_ok=True)
    except OSError:
        pass
    return raw


def prepare(colors, home):
    """Prepare art without publishing app icons or changing any live consumer."""
    for variant in ("full", *(app + "-full" for app in APPLICATIONS)):
        cached_png(svg(False, colors, variant), home)
    for variant in ("symbolic", "ai-symbolic"):
        cached_png(templates()[variant].replace("currentColor", "#ffffff"), home, 96)


def text_logo(variant="symbolic", home=None):
    text = templates()[variant].replace("currentColor", "#ffffff")
    image = (
        raster(text, 96)
        if home is None
        else Image.open(io.BytesIO(cached_png(text, home, 96)))
    )
    image = image.resize((24, 12), Image.Resampling.LANCZOS)
    dots = [
        (0, 0, 0),
        (0, 1, 1),
        (0, 2, 2),
        (1, 0, 3),
        (1, 1, 4),
        (1, 2, 5),
        (0, 3, 6),
        (1, 3, 7),
    ]
    return (
        "\n".join(
            "".join(
                chr(
                    0x2800
                    + sum(
                        1 << bit
                        for dx, dy, bit in dots
                        if image.getpixel((x + dx, y + dy))[3] >= 100
                    )
                )
                for x in range(0, 24, 2)
            )
            for y in range(0, 12, 4)
        )
        + "\n"
    )


def atomic(path, data):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, tmp = tempfile.mkstemp(dir=path.parent)
    with os.fdopen(fd, "wb") as f:
        f.write(data)
    os.chmod(tmp, 0o644)
    os.replace(tmp, path)


def refresh_terminal_menus(home, proc=Path("/proc")):
    """Wake the logo renderer with ncurses' normal resize event; never restart it."""
    command = Path(home) / ".local/bin/nacre-ai"
    expected = {str(command), str(command.resolve())}
    for entry in proc.iterdir():
        if not entry.name.isdigit():
            continue
        descriptor = None
        try:
            if entry.stat().st_uid != os.getuid():
                continue
            arguments = [
                part.decode()
                for part in (entry / "cmdline").read_bytes().split(b"\0")
                if part
            ]
            # Kitty also includes this command in its argv. Only notify Python's
            # menu controller, never its terminal server or assistant workers.
            if (
                len(arguments) < 2
                or not Path(arguments[0]).name.startswith("python")
                or arguments[1] not in expected
                or (
                    len(arguments) > 2 and arguments[2] not in ("dashboard", "settings")
                )
            ):
                continue
            descriptor = os.pidfd_open(int(entry.name))
            # Recheck after opening the stable process handle in case the PID
            # exited/recycled between discovery and notification.
            current = [
                part.decode()
                for part in (entry / "cmdline").read_bytes().split(b"\0")
                if part
            ]
            if current != arguments:
                continue
            signal.pidfd_send_signal(descriptor, signal.SIGWINCH)
        except (OSError, UnicodeError):
            continue
        finally:
            if descriptor is not None:
                os.close(descriptor)


def publish(colors, home=None):
    home = Path.home() if home is None else Path(home)
    folder = home / ".local/share/nacre/branding"
    patterns, roles = templates(), role_colors(colors)

    def render(variant):
        return paint_svg(patterns[variant], roles)

    raw = cached_png(render("full"), home)
    atomic(folder / "nacre-lock.png", raw)
    # Kitty composites alpha over its own background, including opacity effects.
    # Baking a palette surface into the image produces a visible square.
    atomic(folder / "nacre.png", raw)
    for application in APPLICATIONS:
        raw = cached_png(render(application + "-full"), home)
        atomic(folder / ("nacre-" + application + ".png"), raw)
    for variant, name in asset_names():
        atomic(folder / name, render(variant).encode())
    atomic(folder / "nacre-text.txt", text_logo(home=home).encode())
    atomic(folder / "nacre-ai-text.txt", text_logo("ai-symbolic", home).encode())

    for application in ("", *APPLICATIONS):
        name = "nacre" + ("-" + application if application else "")
        variant = application + "-full" if application else "full"
        atomic(
            home / ".local/share/icons/hicolor/scalable/apps" / (name + ".svg"),
            render(variant).encode(),
        )
    # Already-open old menus read the previous filename; it contains the new art.
    for legacy, current in [
        ("sh.png", "nacre.png"),
        ("sh-lock.png", "nacre-lock.png"),
        ("sh.svg", "nacre.svg"),
    ]:
        atomic(folder / legacy, (folder / current).read_bytes())
    refresh_terminal_menus(home)
    live = HERE / "terminal-branding.py"
    if live.is_file():
        spec = importlib.util.spec_from_file_location("terminal_branding", live)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        module.refresh(home)


def build():
    formatter = (
        "/usr/lib/qt6/bin/qmlformat"
        if Path("/usr/lib/qt6/bin/qmlformat").exists()
        else shutil.which("qmlformat")
    )
    if not formatter:
        raise RuntimeError(
            tr(
                "helper.branding.regenerating-qml-branding-requires-qmlformat",
                "Regenerating QML branding requires qmlformat",
            )
        )
    root = HERE.parent
    values = templates()
    for variant, name in asset_names():
        (root / "shell/branding" / name).write_text(values[variant])
    (root / "shell/branding/nacre-text.txt").write_text(text_logo())
    helper = (HERE / "logo-data.js.in").read_text()
    data = ".pragma library\n"
    for application in ("", *APPLICATIONS):
        for variant in ("full", "compact"):
            name = application + variant.capitalize() if application else variant
            key = application + "-" + variant if application else variant
            data += "var " + name + "Template=" + json.dumps(values[key]) + ";\n"
    data += helper
    for target in [root / "shell/branding/LogoData.js", root / "login/LogoData.js"]:
        target.write_text(data)
    common = (HERE / "logo-widget.qml.in").read_text()
    widget = root / "shell/widgets/BrandLogo.qml"
    widget.write_text(
        'import QtQuick\nimport qs.services\nimport "../branding/LogoData.js" as LogoData\n'
        + common.replace("DEFAULT_PRIMARY", "NacreColours.palette.m3primary")
        .replace("DEFAULT_SECONDARY", "NacreColours.palette.m3secondary")
        .replace("DEFAULT_TERTIARY", "NacreColours.palette.m3tertiary || secondary")
        .replace("DEFAULT_HIGHLIGHT", "NacreColours.palette.m3primaryFixed || primary")
        .replace(
            "DEFAULT_BACKGROUND",
            "NacreColours.palette.m3frame || NacreColours.palette.m3surface",
        )
        .replace("DEFAULT_FOREGROUND", "NacreColours.palette.m3onSurface")
        .replace("DEFAULT_MOTION", "DesktopSettings.data.animations !== false")
    )
    login = root / "login/Logo.qml"
    login.write_text(
        'import QtQuick\nimport "LogoData.js" as LogoData\n'
        + common.replace("DEFAULT_PRIMARY", '"#dda1ba"')
        .replace("DEFAULT_SECONDARY", '"#afa2df"')
        .replace("DEFAULT_TERTIARY", '"#8cc9cd"')
        .replace("DEFAULT_HIGHLIGHT", '"#eee9f4"')
        .replace("DEFAULT_BACKGROUND", '"#151310"')
        .replace("DEFAULT_FOREGROUND", '"#f4f1ef"')
        .replace("DEFAULT_MOTION", "false")
    )
    for file in [widget, login]:
        for _ in range(3):
            formatted = subprocess.check_output(
                [formatter, str(file.resolve())], text=True
            )
            if formatted == file.read_text():
                break
            file.write_text(formatted)
    web = root.parent / "brain/web/index.html"
    text = web.read_text()
    symbol_match = re.search(r'<symbol id="(?:sh|nacre)".*?</symbol>', text, re.S)
    if not symbol_match:
        raise RuntimeError(
            tr("helper.branding.brain-logo-symbol-missing", "Brain logo symbol missing")
        )
    inner = values["brain-compact"].split(">", 1)[1].rsplit("</svg>", 1)[0]
    for role, var in [
        ("PRIMARY", "accent"),
        ("SECONDARY", "secondary"),
        ("TERTIARY", "tertiary"),
        ("HIGHLIGHT", "logo-highlight"),
        ("ON_PRIMARY", "on-accent"),
    ]:
        inner = inner.replace("@" + role + "@", "var(--" + var + ")")
    text = (
        text[: symbol_match.start()]
        + f'<symbol id="nacre" viewBox="{VIEWBOX}" stroke="none">'
        + inner
        + "</symbol>"
        + text[symbol_match.end() :]
    )
    text = text.replace('href="#sh"', 'href="#nacre"')
    web.write_text(text)


if __name__ == "__main__":
    import sys

    if "--build" in sys.argv:
        build()
    else:
        colors = json.loads(
            (Path.home() / ".local/state/nacre/scheme.json").read_text()
        )["colours"]
        publish(colors)
