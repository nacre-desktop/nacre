"""Locate Nacre's desktop adapter for helpers launched by the system Python."""

from pathlib import Path
import sys

# Candidate and active venvs must not borrow an older deployed SDK while staging.
environment_sites = (
    tuple((Path(sys.prefix) / "lib").glob("python*/site-packages"))
    if sys.prefix != sys.base_prefix
    else ()
)
for folder in (
    Path(__file__).parents[1] / "shell-cli/src",
    *environment_sites,
    *(Path.home() / ".local/share/nacre/palette-runtime/venv/lib").glob(
        "python*/site-packages"
    ),
):
    if (folder / "nacre_shell/engine-pin.json").is_file():
        sys.path.insert(0, str(folder))
        break

from nacre_shell.template_engine import render_values as render_values  # noqa: E402
