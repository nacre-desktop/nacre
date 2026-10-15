"""Developer-only monitor fixture adapter for native resolution filtering."""

from pathlib import Path
import importlib
from native_lock_dashboard import invoke


def compositor():
    return importlib.import_module("nacre_native_compositor")


def suitable_images(directory, threshold, no_filter):
    dimensions = None
    if not no_filter:
        try:
            monitors = compositor().query("monitors", timeout=3)
            dimensions = [
                max(m["width"] for m in monitors),
                max(m["height"] for m in monitors),
            ]
        except (OSError, RuntimeError, ValueError, KeyError):
            dimensions = None
        except Exception as error:
            import subprocess

            if not isinstance(error, subprocess.SubprocessError):
                raise
    return [
        Path(path)
        for path in invoke(
            "suitable_images",
            directory=str(directory),
            threshold=threshold,
            no_filter=no_filter,
            dimensions=dimensions,
        )
    ]
