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
