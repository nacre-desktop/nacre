"""Nacre-owned helper messages, sharing the shell's English message catalog."""

import json
import os
from pathlib import Path
import re
from functools import lru_cache

ACCENTS = str.maketrans(
    dict(
        zip(
            "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ",
            "áƀçďëƒğħïĵķľḿñöþɋŕšţüṽŵẋÿžÁɃÇĎËƑĞĦÏĴĶĽḾÑÖÞɊŔŠŢÜṼŴẊŸŽ",
            strict=True,
        )
    )
)


@lru_cache(maxsize=1)
def english():
    paths = (
        Path(__file__).resolve().parent.parent / "source/i18n/en.json",
        Path(__file__).resolve().parent.parent / "shell/i18n/en.json",
    )
    for path in paths:
        try:
            value = json.loads(path.read_text())
            if isinstance(value, dict):
                return value
        except (OSError, ValueError):
            continue
    return {}


@lru_cache(maxsize=8)
def _preference_language(path, modified):
    try:
        value = json.loads(Path(path).read_text())
        return value.get("language", "system") if isinstance(value, dict) else "system"
    except (OSError, ValueError):
        return "system"


def language():
    if os.environ.get("NACRE_LANGUAGE"):
        return os.environ["NACRE_LANGUAGE"]
    path = Path.home() / ".config/nacre/desktop.json"
    try:
        modified = path.stat().st_mtime_ns
    except OSError:
        return "system"
    return _preference_language(str(path), modified)


def pseudo(text):
    parts = re.split(r"(<[^>]+>|\{[A-Za-z_][A-Za-z0-9_]*\})", text)
    length = sum(
        len(re.findall(r"[A-Za-z]", part))
        for part in parts
        if not part.startswith(("{", "<"))
    )
    return (
        "["
        + "".join(
            part if part.startswith(("{", "<")) else part.translate(ACCENTS)
            for part in parts
        )
        + " "
        + "~" * ((length * 4 + 9) // 10)
        + "]"
    )


def tr(identifier, fallback, args=None):
    args = args or {}
    message = english().get(identifier, fallback)
    if isinstance(message, dict):
        message = message.get(
            "one" if str(args.get("count")) in ("1", "1.0") else "other",
            message.get("other", fallback),
        )
    if not isinstance(message, str):
        message = fallback.get("other", "") if isinstance(fallback, dict) else fallback
    if language() == "pseudo":
        message = pseudo(message)
    return re.sub(
        r"\{([A-Za-z_][A-Za-z0-9_]*)\}",
        lambda match: str(args[match[1]]) if match[1] in args else match[0],
        message,
    )


def main():
    """Small notification-message bridge for compositor shell scripts."""
    import argparse

    parser = argparse.ArgumentParser(description=main.__doc__)
    parser.add_argument("identifier")
    parser.add_argument("fallback")
    parser.add_argument("arguments", nargs="?", default="{}")
    values = parser.parse_args()
    print(tr(values.identifier, values.fallback, json.loads(values.arguments)))


if __name__ == "__main__":
    main()
