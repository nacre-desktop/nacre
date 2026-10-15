#!/usr/bin/env python3
"""Developer-only regeneration of existing branding sources; never installed."""

import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys

HERE = Path(__file__).resolve().parent
if not (HERE / "logo-data.js.in").is_file():
    HERE = HERE.parent / "nacre/shell-tools"
sys.path.insert(0, str(HERE))
from nacre_native_palette import _call  # noqa: E402
from nacre_i18n import tr  # noqa: E402

os.environ["NACRE_HELPER_TOOLS"] = str(HERE)
APPLICATIONS = ("ai", "brain", "settings", "colors", "notifications")
VIEWBOX = "2.8 3 58 58"


def templates():
    return _call("branding", "templates")


def asset_names():
    return _call("branding", "asset_names")


def text_logo():
    return _call("branding", "text_logo")


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
    login_i18n = """property string language: "system"
    function tr(id, fallback, args = {}) {
        return Translation.translate(Messages.english, language, id, fallback, args);
    }"""
    widget = root / "shell/widgets/BrandLogo.qml"
    widget.write_text(
        'import QtQuick\nimport qs.services\nimport "../branding/LogoData.js" as LogoData\n'
        + common.replace("I18N_HELPER", "")
        .replace("I18N_TR", "NacreI18n.tr")
        .replace("DEFAULT_PRIMARY", "NacreColours.palette.m3primary")
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
        'import QtQuick\nimport "LogoData.js" as LogoData\nimport "translation.js" as Translation\nimport "messages.js" as Messages\n'
        + common.replace("I18N_HELPER", login_i18n)
        .replace("I18N_TR", "root.tr")
        .replace("DEFAULT_PRIMARY", '"#dda1ba"')
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
    build()
