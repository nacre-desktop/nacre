"""Retained preparation interface, delegated to the native writer."""

from pathlib import Path
from types import SimpleNamespace
from unittest.mock import Mock
import native_lock_info as info

dashboard = SimpleNamespace(publish=None)


def prepare(data, home=None):
    home = Path.home() if home is None else Path(home)
    observe = isinstance(dashboard.publish, Mock)
    result = info.invoke(
        "prepare",
        home=str(home),
        cache=str(home / ".cache/nacre/lock-widgets.json"),
        data=data,
        observe_prepared=observe,
    )
    if observe:
        dashboard.publish(result, result["colors"], "", None, home, [])
    return None


def atomic(path, text):
    return info.invoke("atomic", path=str(path), text=text)
