#!/usr/bin/env python3
"""Reject missing/unused message IDs and untranslated owned UI literals."""

import json
import ast
from pathlib import Path
import re
import sys

from source import uncovered, tokens, literal

ROOT = Path(__file__).resolve().parents[2]


def references(root):
    result = set()
    files = (
        list((root / "nacre/login").glob("*.qml"))
        + list((root / "nacre/shell").rglob("*.qml"))
        + list((root / "nacre/shell").rglob("*.js"))
        + list((root / "nacre/shell-tools").glob("*.py"))
        + list((root / "tools").glob("*.py"))
        + list((root / "hypr/scripts").glob("*.py"))
        + list((root / "hypr/scripts").glob("*.sh"))
        + list((root / "brain/web").glob("*.js"))
        + list((root / "brain/web").glob("*.html"))
        + ([root / "bin/nacre-ai"] if (root / "bin/nacre-ai").exists() else [])
    )
    for path in files:
        text = path.read_text()
        result.update(
            re.findall(
                r'data-i18n-(?:node|title|placeholder|aria-label)="([^"]+)"', text
            )
        )
        result.update(re.findall(r'nacre_message\s+"([^"]+)"', text))
        if path.suffix == ".py" or path.name == "nacre-ai":
            for node in ast.walk(ast.parse(text)):
                if isinstance(node, ast.Dict):
                    for key, value in zip(node.keys, node.values, strict=True):
                        if (
                            isinstance(key, ast.Constant)
                            and key.value
                            in (
                                "labelId",
                                "titleId",
                                "nameId",
                                "detailId",
                                "descriptionId",
                            )
                            and isinstance(value, ast.Constant)
                            and isinstance(value.value, str)
                        ):
                            result.add(value.value)
                if (
                    isinstance(node, ast.Call)
                    and isinstance(node.func, ast.Name)
                    and node.func.id in ("tr", "ui_label", "translate")
                    and node.args
                ):
                    if isinstance(node.args[0], ast.Constant) and isinstance(
                        node.args[0].value, str
                    ):
                        result.add(node.args[0].value)
            continue
        stream = tokens(text)
        for index, token in enumerate(stream):
            if (
                token.text not in ("tr", "ui_label", "translate")
                or index + 1 >= len(stream)
                or stream[index + 1].text != "("
            ):
                continue
            depth = 0
            first = []
            for current in stream[index + 2 :]:
                if depth == 0 and current.text in (",", ")"):
                    break
                if current.text in ("(", "[", "{"):
                    depth += 1
                elif current.text in (")", "]", "}"):
                    depth -= 1
                first.append(current)
            if not any(current.text == "+" for current in first):
                result.update(
                    value
                    for current in first
                    if (value := literal(current))
                    and re.fullmatch(r"[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)+", value)
                )
        for match in re.finditer(
            r'\b(?:label|title|name|detail|description|confirm|setup|status)Id\s*:\s*"([^"]+)"',
            text,
        ):
            result.add(match[1])
    catalog = json.loads((root / "nacre/shell-tools/launcher-menu.json").read_text())
    for rows in (catalog.get("entries", []), catalog.get("software", [])):
        for row in rows:
            for field in ("label", "description", "confirm"):
                if row.get(field + "Id"):
                    result.add(row[field + "Id"])
    for domain in json.loads(
        (root / "nacre/shell/i18n/options.json").read_text()
    ).values():
        result.update(row["id"] for row in domain.values())
    return result


def check(root=ROOT):
    english = json.loads((root / "nacre/shell/i18n/en.json").read_text())
    context = json.loads((root / "nacre/shell/i18n/context.json").read_text())
    used = references(root)
    errors = []
    for key in sorted(used - english.keys()):
        errors.append("Missing English ID: " + key)
    for key in sorted(english.keys() - used):
        errors.append("Unused English ID: " + key)
    for key in sorted(used - context.keys()):
        errors.append("Missing translator context: " + key)
    for key, value in english.items():
        if not re.fullmatch(r"[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)+", key):
            errors.append("Invalid message ID: " + key)
        if isinstance(value, dict):
            if not {"one", "other"} <= value.keys() or any(
                not isinstance(form, str) for form in value.values()
            ):
                errors.append("Invalid plural forms: " + key)
        elif not isinstance(value, str):
            errors.append("English message must be text or plural forms: " + key)
    for path in [
        *(root / "nacre/shell").rglob("*.qml"),
        *(root / "nacre/login").glob("*.qml"),
        *([root / "brain/web/app.js"] if (root / "brain/web/app.js").exists() else []),
    ]:
        if path.name == "NacreI18n.qml":
            continue
        source = path.read_text()
        for field, offset, value in uncovered(source):
            line = source[:offset].count("\n") + 1
            errors.append(
                f"{path.relative_to(root)}:{line}: unwrapped {field}: {value!r}"
            )
    catalog = json.loads((root / "nacre/shell-tools/launcher-menu.json").read_text())
    for rows in (catalog.get("entries", []), catalog.get("software", [])):
        for row in rows:
            for field in ("label", "description", "confirm"):
                if row.get(field):
                    key = row.get(field + "Id")
                    if not key or english.get(key) != row[field]:
                        errors.append(
                            "Catalog fallback/ID mismatch: "
                            + str(row.get("id", row.get("package")))
                            + "."
                            + field
                        )
    for relative in (
        "modules/settings/settings-catalog.js",
        "modules/welcome/welcome-catalog.js",
    ):
        path = root / "nacre/shell" / relative
        stream = tokens(path.read_text())
        for index, token in enumerate(stream[:-2]):
            if (
                token.text
                in ("label", "title", "detail", "description", "setup", "status")
                and stream[index + 1].text == ":"
                and stream[index + 2].kind == "string"
            ):
                value = literal(stream[index + 2])
                # Explicit companion ID is required next to each source-language catalog field.
                previous = stream[max(0, index - 4) : index]
                if not any(t.text == token.text + "Id" for t in previous):
                    errors.append(
                        str(path.relative_to(root))
                        + ": catalog field has no message ID: "
                        + str(value)
                    )
    if errors:
        raise ValueError("\n".join(errors))
    print(f"Translation coverage passed: {len(english)} owned messages.")


if __name__ == "__main__":
    try:
        check()
    except ValueError as failure:
        print(failure, file=sys.stderr)
        raise SystemExit(1) from failure
