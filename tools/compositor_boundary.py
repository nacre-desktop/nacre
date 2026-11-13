#!/usr/bin/env python3
"""Reject native compositor transport outside the explicit backend boundaries."""

import ast
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
BACKENDS = (
    "nacre/shell-tools/compositor_backends/",
    "config/hypr/conf/",
)
BACKEND_FILES = {
    "config/hypr/hyprland.lua",
    "nacre/shell-tools/shortcuts.lua",
    "tools/compositor_boundary.py",
    # Session lifecycle owns starting/validating the native compositor, not IPC.
    # Packaged session lifecycle validates/starts Niri; it performs no native IPC.
    "tools/package_niri.py",
}
EXCLUDED = {
    ".git",
    "deps",
    "node_modules",
    "tests",
    "__pycache__",
    "build",
    "dist",
    ".venv",
    ".research",
}
NATIVE_PROGRAMS = {"hyprctl", "Hyprland", "hyprshutdown", "niri", "wlr-randr"}
TEXT_NATIVE = re.compile(r"\bhl\.(?:dsp|dispatch|config|monitor|window_rule|bind)\b")
SHELL_NATIVE = re.compile(
    r"(?<![\w.-])(?:hyprctl|Hyprland|hyprshutdown|niri|wlr-randr)(?=$|[\s\"'])"
)


def allowed(relative):
    return relative in BACKEND_FILES or relative.startswith(BACKENDS)


def native_program(value):
    return isinstance(value, str) and Path(value).name in NATIVE_PROGRAMS


def excluded(path, root):
    relative = path.relative_to(root)
    # Frozen reference captures are not installed dispatchers.
    return relative.parts[:3] == ("docs", "research", "n0156-reference") or bool(
        set(relative.parts) & EXCLUDED
    )


def violations(path, root=ROOT):
    relative = path.relative_to(root).as_posix()
    if allowed(relative) or excluded(path, root):
        return []
    if (
        path.suffix not in (".py", ".qml", ".js", ".lua", ".json", ".sh", ".fish")
        and path.parent != root / "bin"
    ):
        return []
    text = path.read_text()
    errors = []
    if path.suffix == ".py" or (
        path.parent == root / "bin" and text.startswith("#!/usr/bin/env python3")
    ):
        tree = ast.parse(text)
        for node in ast.walk(tree):
            if isinstance(node, (ast.List, ast.Tuple)) and node.elts:
                first = node.elts[0]
                if isinstance(first, ast.Constant) and native_program(first.value):
                    errors.append((node.lineno, "native command construction"))
            if (
                isinstance(node, ast.Constant)
                and isinstance(node.value, str)
                and TEXT_NATIVE.search(node.value)
            ):
                errors.append((node.lineno, "native expression"))
    elif path.suffix in (".sh", ".fish") or path.parent == root / "bin":
        for index, line in enumerate(text.splitlines(), 1):
            if not line.lstrip().startswith("#") and SHELL_NATIVE.search(line):
                errors.append((index, "native shell command"))
    else:
        for index, line in enumerate(text.splitlines(), 1):
            if TEXT_NATIVE.search(line) or re.search(
                r"[\"\'](?:[^\"\']*/)?hyprctl[\"\']", line
            ):
                errors.append((index, "native import, command or expression"))
    return [f"{relative}:{line}: {reason}" for line, reason in sorted(set(errors))]


def check(root=ROOT):
    failures = []
    for path in sorted(root.rglob("*")):
        if path.is_file() and not excluded(path, root):
            failures.extend(violations(path, root))
    return failures


if __name__ == "__main__":
    failures = check()
    if failures:
        raise SystemExit(
            "Direct compositor access must move behind the compositor interface:\n"
            + "\n".join(failures)
        )
    print(
        "All compositor transport and native expressions are behind backend boundaries."
    )
