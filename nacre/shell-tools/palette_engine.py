"""Locate Nacre's desktop adapter for helpers launched by the system Python."""

from pathlib import Path
import sys

for folder in (
    Path(__file__).parents[1] / "shell-cli/src",
    *(Path.home() / ".local/share/nacre/palette-runtime/venv/lib").glob(
        "python*/site-packages"
    ),
):
    if (folder / "nacre_shell").is_dir():
        sys.path.insert(0, str(folder))
        break

from nacre_shell.template_engine import render_values as render_values  # noqa: E402
