"""Lexical message coverage for QML: distinguish UI bindings from protocol strings."""

import ast
from dataclasses import dataclass
import re

VISIBLE = {
    "text",
    "label",
    "title",
    "description",
    "detail",
    "heading",
    "placeholderText",
    "placeholder",
    "subtitle",
    "hint",
    "tooltipText",
    "loadingText",
    "errorText",
    "statusText",
    "footerText",
    "displayTitle",
    "caption",
    "accessibleName",
    "accessibleLabel",
    "tooltip",
    "toolTip",
    "buttonText",
    "buttonLabel",
    "emptyText",
    "error",
    "message",
    "summary",
    "body",
    "status",
    "reason",
    "estimate",
    "profileName",
    "rotationStatus",
    "textContent",
}
ICONS = {"NacreIcon", "MaterialIcon", "IconImage", "BrandLogo"}


@dataclass
class Token:
    text: str
    start: int
    end: int
    kind: str = "code"


def tokens(source):
    result = []
    pos = 0
    while pos < len(source):
        start = pos
        char = source[pos]
        if char.isspace():
            pos += 1
            continue
        if source.startswith("//", pos):
            end = source.find("\n", pos)
            pos = len(source) if end < 0 else end
            continue
        if source.startswith("/*", pos):
            end = source.find("*/", pos + 2)
            pos = len(source) if end < 0 else end + 2
            continue
        if char in "\"'`":
            quote = char
            pos += 1
            while pos < len(source):
                if source[pos] == "\\":
                    pos += 2
                elif source[pos] == quote:
                    pos += 1
                    break
                else:
                    pos += 1
            result.append(Token(source[start:pos], start, pos, "string"))
            continue
        if char == "/" and (
            not result
            or result[-1].text in ("=", "(", "[", ":", ",", "return", "?", "!")
        ):
            pos += 1
            bracket = False
            while pos < len(source):
                if source[pos] == "\\":
                    pos += 2
                    continue
                if source[pos] == "[":
                    bracket = True
                if source[pos] == "]":
                    bracket = False
                if source[pos] == "/" and not bracket:
                    pos += 1
                    while pos < len(source) and source[pos].isalpha():
                        pos += 1
                    break
                pos += 1
            result.append(Token(source[start:pos], start, pos, "regexp"))
            continue
        match = re.match(
            r"===|!==|==|!=|=>|\?\?|\?\.|\+\+|--|>=|<=|\+=|-=|&&|\|\||[A-Za-z_$][A-Za-z0-9_$]*|\d+(?:\.\d+)?",
            source[pos:],
        )
        pos += len(match[0]) if match else 1
        result.append(Token(source[start:pos], start, pos))
    return result


def literal(token):
    if token.kind != "string" or token.text.startswith("`"):
        return None
    try:
        return ast.literal_eval(token.text)
    except (ValueError, SyntaxError):
        return None


def human(text):
    return bool(
        text
        and not re.match(r"(?:https?|file)://", text)
        and not re.fullmatch(r"T\d{2}:\d{2}:\d{2}", text)
        and re.search(r"[A-Za-z]", text)
        and not re.fullmatch(r"[a-z0-9_./:#-]+", text)
        and not re.fullmatch(r"(?:[yMdHhmsAP]+|[ ·:/,\-])+", text)
    )


def bindings(source):
    stream = tokens(source)
    stack = []
    for index, token in enumerate(stream):
        field = literal(token) if token.kind == "string" else token.text
        if token.text == "{":
            prior = stream[index - 1].text if index else ""
            stack.append(prior if re.match(r"[A-Z]\w*$", prior) else "")
        elif token.text == "}":
            if stack:
                stack.pop()
        if (
            field not in VISIBLE
            and not (field == "name" and token.kind == "string")
            and not (
                field == "name"
                and index >= 2
                and stream[index - 2].text == "Accessible"
            )
        ) or index + 1 >= len(stream):
            continue
        if stream[index + 1].text not in (":", "="):
            continue
        if (
            field == "text"
            and next((name for name in reversed(stack) if name), "") in ICONS
        ):
            continue
        previous = stream[max(0, index - 4) : index]
        if any(item.text == field + "Id" for item in previous):
            continue
        begin = index + 2
        if begin >= len(stream):
            continue
        depth = 0
        end = begin
        for current in range(begin, len(stream)):
            value = stream[current]
            if depth == 0 and (
                value.text in (",", ";", "}")
                or (
                    current > begin
                    and "\n" in source[stream[current - 1].end : value.start]
                )
            ):
                break
            if value.text in ("{", "[", "("):
                depth += 1
            elif value.text in ("}", "]", ")"):
                depth -= 1
                if depth < 0:
                    break
            end = current + 1
        if end > begin:
            yield field, stream[begin].start, stream[end - 1].end
    # Known rendering factories carry messages in arguments rather than bindings.
    for index, token in enumerate(stream):
        if (
            token.text not in ("valueRow", "el", "button")
            or index + 1 >= len(stream)
            or stream[index + 1].text != "("
        ):
            continue
        depth, argument, start = 0, 0, index + 2
        for current in range(index + 2, len(stream)):
            value = stream[current]
            if depth == 0 and value.text in (",", ")"):
                if (
                    argument
                    in ({"valueRow": (2, 6), "el": (1,), "button": (0,)}[token.text])
                    and current > start
                ):
                    yield (
                        "label" if argument == 2 else "description",
                        stream[start].start,
                        stream[current - 1].end,
                    )
                if value.text == ")":
                    break
                argument, start = argument + 1, current + 1
                continue
            if value.text in ("(", "[", "{"):
                depth += 1
            elif value.text in (")", "]", "}"):
                depth -= 1


def translated_ranges(source):
    ranges = []
    stream = tokens(source)
    for index, token in enumerate(stream):
        if (
            token.text != "tr"
            or index + 1 >= len(stream)
            or stream[index + 1].text != "("
        ):
            continue
        depth = 0
        for current in stream[index + 1 :]:
            if current.text == "(":
                depth += 1
            elif current.text == ")":
                depth -= 1
                if depth == 0:
                    ranges.append((token.start, current.end))
                    break
    return ranges


def uncovered(source):
    covered = translated_ranges(source)
    seen = set()
    for field, start, end in bindings(source):
        for token in tokens(source[start:end]):
            absolute = token.start + start
            value = literal(token)
            if (
                human(value)
                and absolute not in seen
                and not any(a <= absolute < b for a, b in covered)
            ):
                seen.add(absolute)
                yield field, absolute, value
