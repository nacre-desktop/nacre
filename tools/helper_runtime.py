"""Resolve benchmark helper commands across the staged runtime migration."""

from pathlib import Path
import sys


def helper_command(tools, name, binary=None):
    tools = Path(tools)
    if binary is None:
        script = tools / (name + ".py")
        if script.is_file():
            return [sys.executable, str(script)], "Python"
        binary = tools / "nacre-helper"
        if not binary.is_file() and tools.parts[-2:] == ("nacre", "shell-tools"):
            binary = tools.parents[1] / "target/release/nacre-helper"
    binary = Path(binary).resolve(strict=True)
    if not binary.is_file():
        raise ValueError("Build or select the native nacre-helper binary")
    command = "serve" if name == "helper-service" else name
    return [str(binary), command], "Rust"
