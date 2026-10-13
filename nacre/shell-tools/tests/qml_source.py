"""Formatting-independent adaptation of production QML for plain Qt fixtures."""

import re


def remove_objects(source, pattern):
    while match := re.search(pattern, source):
        start = source.index("{", match.start())
        depth = 0
        quote = None
        escaped = False
        for end in range(start, len(source)):
            char = source[end]
            if quote:
                if escaped:
                    escaped = False
                elif char == "\\":
                    escaped = True
                elif char == quote:
                    quote = None
            elif char in '"\x27`':
                quote = char
            elif char == "{":
                depth += 1
            elif char == "}":
                depth -= 1
                if depth == 0:
                    source = source[: match.start()] + source[end + 1 :]
                    break
        else:
            raise ValueError("Unclosed fixture object")
    return source


def install_foundation_interaction(fixtures, widgets):
    """Use the actual interaction and tokens while adapting only service providers."""
    from pathlib import Path

    fixtures, widgets = Path(fixtures), Path(widgets)
    manifest = fixtures / "qmldir"
    text = manifest.read_text()
    for name in ("NacreTokens", "NacreInteraction"):
        source = (widgets / (name + ".qml")).read_text()
        source = source.replace("import qs.services", 'import "."')
        (fixtures / (name + ".qml")).write_text(source)
        if name not in text:
            text += (
                "\n"
                + ("singleton " if name == "NacreTokens" else "")
                + name
                + " 1.0 "
                + name
                + ".qml\n"
            )
    if not (fixtures / "DesktopSettings.qml").exists():
        (fixtures / "DesktopSettings.qml").write_text(
            "pragma Singleton\nimport QtQuick\nQtObject {property var data:({animations:true})}\n"
        )
        text += "\nsingleton DesktopSettings 1.0 DesktopSettings.qml\n"
    manifest.write_text(text)


def install_i18n(directory):
    """Stage the real message engine for native UI tests without host preferences."""
    from pathlib import Path
    import shutil

    source = Path(__file__).resolve().parents[2] / "shell/i18n/translation.js"
    import json

    options = json.loads(source.with_name("options.json").read_text())
    messages = json.loads(source.with_name("en.json").read_text())
    shutil.copy2(source, directory / "translation.js")
    (directory / "NacreI18n.qml").write_text(
        """pragma Singleton
import QtQuick
import "translation.js" as Translation
QtObject {
    property string language: "en"
    property bool developerMode: false
    property var locale: Qt.locale("en_US")
    property var messages: MESSAGES
    property var options: OPTIONS
    function option(domain, key) { const row = options[domain]?.[key]; return row ? tr(row.id, row.fallback) : key; }
    function modelLabel(row, key) { return row[key + "Id"] ? tr(row[key + "Id"], row[key]) : row[key]; }
    function applicationName(name) { const row=options["application-name"]?.[name]; return row?tr(row.id,row.fallback):name; }
    function applicationRow(row) { const copy=Object.assign({},row);if(row.brand&&/^nacre-/.test(row.id)){copy.name=applicationName(row.name);copy.label=copy.name}return copy; }
    function windowCaption(value, className) {
        if(className!=="org.quickshell"&&!String(className||"").startsWith("nacre-"))return value;
        const pearl=/^Nacre Pearl (Launcher|Dashboard|AI|Controls|Media) ([0-9]+)$/.exec(value);
        return pearl?tr("pearl.window.title","Nacre Pearl {panel} {number}",{panel:tr("pearl.kind."+pearl[1].toLowerCase(),pearl[1]),number:pearl[2]}):applicationName(value);
    }
    function tr(id, fallback, args = {}) { return Translation.translate(messages, language, id, fallback, args); }
    function localize(value) { return Translation.localize(value, tr); }
    function number(value, decimals = 0) { return Number(value).toLocaleString(locale, "f", decimals); }
    function displayValue(value) { return language === "pseudo" && /[A-Za-z]/.test(value) ? Translation.pseudo(value) : value; }
    function dayName(day, format = Locale.ShortFormat) { return displayValue(locale.dayName(day, format)); }
    function dateTime(value, format = Locale.ShortFormat) { return new Date(value).toLocaleString(locale, format); }
    function date(value, format = Locale.ShortFormat) { return displayValue(new Date(value).toLocaleDateString(locale, format)); }
    function time(value, format = Locale.ShortFormat) { return new Date(value).toLocaleTimeString(locale, format); }
}
""".replace("OPTIONS", json.dumps(options)).replace("MESSAGES", json.dumps(messages))
    )
    manifest = directory / "qmldir"
    value = manifest.read_text() if manifest.exists() else ""
    if "singleton NacreI18n " not in value:
        manifest.write_text(value + "\nsingleton NacreI18n 1.0 NacreI18n.qml\n")
