"""Expose the shared guard when running the palette adapter from source."""

from pathlib import Path

_state_source = Path(__file__).resolve().parents[2]
_state_source = _state_source / "shell-tools" / "nacre_state.py"
__file__ = str(_state_source)
exec(compile(_state_source.read_bytes(), __file__, "exec"), globals())
