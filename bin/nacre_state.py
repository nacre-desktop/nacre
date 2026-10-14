"""Resolve the shared state module for source and packaged command entrypoints."""

from pathlib import Path

for _candidate in (
    Path(__file__).resolve().parents[1] / "nacre/shell-tools/nacre_state.py",
    Path.home() / ".local/share/nacre/shell/tools/nacre_state.py",
    Path("/usr/share/nacre/nacre/shell-tools/nacre_state.py"),
):
    if _candidate.is_file():
        __file__ = str(_candidate)
        exec(compile(_candidate.read_bytes(), str(_candidate), "exec"), globals())
        break
else:
    raise ImportError("The shared Nacre state module is not installed")
