"""Read literal Rust translation calls and finite local ID wrappers.

This is a lexer for the supported translation boundary, not a Rust compiler.
Computed IDs require explicit caller evidence instead of claiming a whole prefix.
"""

from dataclasses import dataclass
import re

ID = re.compile(r"[A-Za-z0-9_-]+(?:\.[A-Za-z0-9_-]+)+")


@dataclass
class Token:
    text: str
    start: int
    value: str | None = None


def quoted(text):
    result = []
    index = 0
    escapes = {
        "n": "\n",
        "r": "\r",
        "t": "\t",
        "0": "\0",
        "\\": "\\",
        '"': '"',
        "'": "'",
    }
    while index < len(text):
        char = text[index]
        index += 1
        if char != "\\":
            result.append(char)
            continue
        char = text[index]
        index += 1
        if char in escapes:
            result.append(escapes[char])
        elif char == "x":
            result.append(chr(int(text[index : index + 2], 16)))
            index += 2
        elif char == "u" and text[index] == "{":
            end = text.index("}", index)
            result.append(chr(int(text[index + 1 : end].replace("_", ""), 16)))
            index = end + 1
        elif char in "\r\n":
            while index < len(text) and text[index].isspace():
                index += 1
        else:
            raise ValueError("Unsupported Rust string escape")
    return "".join(result)


def tokens(source):
    result = []
    pos = 0
    while pos < len(source):
        start = pos
        if source[pos].isspace():
            pos += 1
            continue
        if source.startswith("//", pos):
            end = source.find("\n", pos)
            pos = len(source) if end < 0 else end
            continue
        if source.startswith("/*", pos):
            depth = 1
            pos += 2
            while depth and pos < len(source):
                if source.startswith("/*", pos):
                    depth += 1
                    pos += 2
                elif source.startswith("*/", pos):
                    depth -= 1
                    pos += 2
                else:
                    pos += 1
            if depth:
                raise ValueError("Unterminated Rust comment")
            continue
        if raw := re.match(r'(?:b|c)?r(#+)?"', source[pos:]):
            hashes = raw[1] or ""
            begin = pos + len(raw[0])
            end = source.find('"' + hashes, begin)
            if end < 0:
                raise ValueError("Unterminated Rust raw string")
            pos = end + 1 + len(hashes)
            result.append(
                Token(
                    source[start:pos],
                    start,
                    source[begin:end] if raw[0].startswith("r") else None,
                )
            )
            continue
        if source[pos] == '"':
            pos += 1
            while pos < len(source) and source[pos] != '"':
                pos += 2 if source[pos] == "\\" else 1
            if pos >= len(source):
                raise ValueError("Unterminated Rust string")
            pos += 1
            result.append(
                Token(source[start:pos], start, quoted(source[start + 1 : pos - 1]))
            )
            continue
        # Character literals are separate from lifetimes such as &'static str.
        if char := re.match(
            r"'(?:[^'\\\n]|\\(?:u\{[0-9a-fA-F_]+\}|x[0-9a-fA-F]{2}|.))'", source[pos:]
        ):
            pos += len(char[0])
            result.append(Token(char[0], start))
            continue
        word = re.match(r"[A-Za-z_]\w*|::", source[pos:])
        pos += len(word[0]) if word else 1
        result.append(Token(source[start:pos], start))
    return result


def arguments(stream, opening):
    """Return balanced comma-separated arguments and the closing token index."""
    depth = []
    parts = [[]]
    pairs = {"(": ")", "[": "]", "{": "}"}
    for index in range(opening + 1, len(stream)):
        token = stream[index]
        if not depth and token.text == ")":
            return parts, index
        if not depth and token.text == ",":
            parts.append([])
            continue
        parts[-1].append(token)
        if token.value is None:
            if token.text in pairs:
                depth.append(pairs[token.text])
            elif depth and token.text == depth[-1]:
                depth.pop()
    raise ValueError("Unterminated Rust call")


def literal(part):
    return part[0].value if len(part) == 1 else None


def production_tokens(stream):
    """Do not let test-only calls keep retired runtime catalog entries alive."""
    result = []
    index = 0
    while index < len(stream):
        test_attribute = None
        for spelling in (
            ["#", "[", "cfg", "(", "test", ")", "]"],
            ["#", "[", "test", "]"],
        ):
            if [
                token.text for token in stream[index : index + len(spelling)]
            ] == spelling:
                test_attribute = len(spelling)
                break
        if test_attribute is None:
            result.append(stream[index])
            index += 1
            continue
        index += test_attribute
        while index < len(stream) and stream[index].text not in ("{", ";"):
            index += 1
        if index == len(stream):
            raise ValueError("Unterminated Rust test item")
        if stream[index].text == ";":
            index += 1
            continue
        depth = 1
        index += 1
        while index < len(stream) and depth:
            if stream[index].value is None:
                depth += (stream[index].text == "{") - (stream[index].text == "}")
            index += 1
    return result


def messages(source):
    stream = production_tokens(tokens(source))
    wrappers = {}
    wrapper_calls = set()
    for index, token in enumerate(stream[:-3]):
        if token.text != "fn" or stream[index + 2].text != "(":
            continue
        parameters, close = arguments(stream, index + 2)
        names = [part[0].text for part in parameters if part]
        body = close + 1
        while body < len(stream) and stream[body].text not in ("{", ";"):
            body += 1
        if body == len(stream) or stream[body].text != "{":
            continue
        depth = 1
        end = body + 1
        while end < len(stream) and depth:
            if stream[end].value is None:
                depth += (stream[end].text == "{") - (stream[end].text == "}")
            end += 1
        for current in range(body + 1, end - 1):
            if stream[current].text != "tr" or stream[current + 1].text != "(":
                continue
            parts, _ = arguments(stream, current + 1)
            first = [part.text for part in parts[0]]
            if (
                len(parts) != 2
                or len(first) != 6
                or first[:4] != ["&", "format", "!", "("]
                or first[-1] != ")"
            ):
                continue
            template = parts[0][4].value
            captures = re.findall(r"\{([A-Za-z_]\w*)\}", template or "")
            if (
                len(captures) == 1
                and captures[0] in names
                and len(parts[1]) == 1
                and parts[1][0].text in names
            ):
                wrappers[stream[index + 1].text] = (
                    template,
                    names.index(captures[0]),
                    names.index(parts[1][0].text),
                )
                wrapper_calls.add(current)
    result = []
    for index, token in enumerate(stream[:-1]):
        if (
            stream[index + 1].text != "("
            or (index and stream[index - 1].text == "fn")
            or index in wrapper_calls
        ):
            continue
        if token.text != "tr" and token.text not in wrappers:
            continue
        parts, _ = arguments(stream, index + 1)
        if token.text in wrappers:
            template, id_index, fallback_index = wrappers[token.text]
            suffix = literal(parts[id_index]) if id_index < len(parts) else None
            identifier = (
                re.sub(
                    r"\{[A-Za-z_]\w*\}",
                    lambda _, replacement=suffix: replacement,
                    template,
                )
                if suffix is not None
                else None
            )
        else:
            identifier = literal(parts[0])
            fallback_index = 1
        fallback = (
            literal(parts[fallback_index]) if fallback_index < len(parts) else None
        )
        line = source[: token.start].count("\n") + 1
        if identifier is None or not ID.fullmatch(identifier):
            raise ValueError(f"Unresolved Rust translation ID at line {line}")
        if fallback is None:
            raise ValueError(
                f"Unresolved Rust translation fallback for {identifier} at line {line}"
            )
        result.append((identifier, fallback, line))
    return result
