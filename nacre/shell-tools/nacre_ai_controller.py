"""Load Python controller code, independently of user-facing command wrappers."""

from pathlib import Path
import runpy


def load(name="nacre-ai", home=None):
    if name not in ("nacre-ai", "nacre-ai-claude"):
        raise ValueError("Unknown AI controller")
    # Both the source tree and the packaged /usr/share/nacre tree keep the
    # Python controllers here. ~/.local/bin may instead hold shell launchers.
    home = Path.home() if home is None else Path(home)
    launcher = home / ".local/bin" / name
    wrapper = f'#!/bin/sh\nexec /usr/share/nacre/bin/{name} "$@"\n'
    if launcher.is_file() and launcher.read_text() == wrapper:
        candidates = (
            Path(__file__).resolve().parents[2] / "bin" / name,
            home / ".local/share/nacre/ai/application/bin" / name,
        )
        for source in candidates:
            if source.is_file():
                with source.open() as stream:
                    header = stream.readline()
                if header.startswith("#!") and "python" in header:
                    return runpy.run_path(str(source.resolve()))
        raise RuntimeError(
            "Nacre AI controller is missing; install the optional AI tools"
        )
    # Source installs retain their Python launcher and its original missing-file
    # behavior. A checkout alone must not silently enable an optional component.
    return runpy.run_path(str(launcher.resolve()))
