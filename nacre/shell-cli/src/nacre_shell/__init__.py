"""Desktop adapter for the pinned standalone Rust Orient engine."""

from ._backend import ENGINE_ID as ENGINE_ID, FORMAT_VERSION as FORMAT_VERSION, __version__ as __version__


def main():
    from .cli import main as run

    return run()
