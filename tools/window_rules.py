"""Detect conflicting class-wide Lua window routes and floating rules.

Window rules and binding declarations use a restricted Lua environment without
native actions, processes or file/environment access.
It compares equal selectors and witnesses overlaps using finite class names,
including case pairs and alternations. Arbitrary PCRE intersection is not assumed.
"""

import ast
import itertools
import re
from pathlib import Path

STRING = r'"(?:[^"\\]|\\.)*"|\x27(?:[^\x27\\]|\\.)*\x27'


def balanced(source, start):
    opening = source[start]
    closing = {"{": "}", "(": ")"}[opening]
    depth = 0
    quote = None
    escaped = False
    for index in range(start, len(source)):
        char = source[index]
        if quote:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == quote:
                quote = None
        elif char in '"\x27':
            quote = char
        elif char == opening:
            depth += 1
        elif char == closing:
            depth -= 1
            if depth == 0:
                return index + 1
    raise ValueError("Unclosed window rule table")


def fields(table):
    values = {}
    for match in re.finditer(r"\b(\w+)\s*=\s*(" + STRING + r"|true|false)", table):
        value = match[2]
        values[match[1]] = (
            value == "true" if value in ("true", "false") else ast.literal_eval(value)
        )
    return values


def lua_sources(folder, extra=()):
    return sorted(set(Path(folder).glob("*.lua")) | {Path(p) for p in extra})


def without_comments(source):
    return re.sub(
        STRING + r"|--[^\n]*", lambda m: "" if m[0].startswith("--") else m[0], source
    )


def rules(folder, extra=()):
    from bindings import capture

    result = []
    for path in lua_sources(folder, extra):
        source = without_comments(path.read_text())
        if not re.search(r"\bhl\.window_rule\s*\(", source):
            continue
        for rule in capture(path, "rules"):
            selectors = dict(rule.get("match", {}))
            pattern = selectors.pop("class", None)
            if not pattern:
                continue
            values = {key: value for key, value in rule.items() if key != "match"}
            if "workspace" in values:
                values["workspace"] = str(values["workspace"]).split()[0]
            result.append((path.name, pattern, selectors, values))
    return result


def witnesses(pattern):
    body = pattern.removeprefix("^").removesuffix("$")
    if body.startswith("(") and body.endswith(")"):
        body = body[1:-1]
    values = []
    for branch in body.split("|"):
        pieces = re.split(r"(\[[^\]]+\])", branch)
        options = []
        for piece in pieces:
            if piece.startswith("["):
                chars = piece[1:-1]
                if "-" in chars or "^" in chars:
                    break
                options.append(list(chars))
            elif any(char in piece for char in "*+?(){}\\"):
                break
            else:
                options.append([piece])
        else:
            values.extend("".join(parts) for parts in itertools.product(*options))
    return values


def conflicts(folder, extra=()):
    found = []
    rows = rules(folder, extra)
    for left, right in itertools.combinations(rows, 2):
        if left[2] != right[2]:
            continue
        overlapping = left[1] == right[1]
        if not overlapping:
            try:
                overlapping = any(
                    re.fullmatch(left[1], name) and re.fullmatch(right[1], name)
                    for name in witnesses(left[1]) + witnesses(right[1])
                )
            except re.error:
                continue  # The compositor validates PCRE-specific syntax.
        if overlapping:
            for field in ("workspace", "float"):
                if (
                    field in left[3]
                    and field in right[3]
                    and left[3][field] != right[3][field]
                ):
                    found.append(
                        f"{left[0]} {left[1]} conflicts with {right[0]} {right[1]}: {field}"
                    )
    return found


def bind_conflicts(folder, extra=()):
    from bindings import capture

    found, seen = [], {}
    for path in lua_sources(folder, extra):
        source = without_comments(path.read_text())
        if not re.search(r"\bhl\.bind\s*\(", source):
            continue
        for record in capture(path):
            if record["submap"]:
                continue
            key = record["keys"]
            parts = [part.strip().upper() for part in key.split("+")]
            modifiers = {"CONTROL": "CTRL", "META": "SUPER", "MOD4": "SUPER"}
            canonical = (
                "+".join(sorted(modifiers.get(part, part) for part in parts[:-1]))
                + "+"
                + parts[-1]
            )
            if canonical in seen:
                found.append(f"{path.name} {key} duplicates {seen[canonical]}")
            else:
                seen[canonical] = f"{path.name} {key}"
    return found
