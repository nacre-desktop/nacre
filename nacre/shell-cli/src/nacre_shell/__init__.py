"""Desktop compatibility adapter for the standalone Orient engine."""

from orient import ENGINE_ID as ENGINE_ID, __version__ as __version__


def main():
    from .cli import main as run

    return run()
