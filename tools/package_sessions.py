"""Session integrations share one package/setup contract across distro adapters."""

import importlib.util
import json
from pathlib import Path
import re


def backends(value):
    values = [value] if isinstance(value, str) else list(value)
    if not values or any(
        not isinstance(name, str) or not re.fullmatch(r"[a-z][a-z0-9-]*", name)
        for name in values
    ):
        raise ValueError("Invalid compositor selection")
    return tuple(dict.fromkeys(values))


def registry(root):
    path = Path(root) / "packaging/sessions.json"
    if path.exists():
        return json.loads(path.read_text())
    # Compatibility for already installed pre-session-package archives.
    return {"hyprland": {"module": None}, "niri": {"module": "package_niri"}}


def integration(root, name):
    selected = registry(root)[name]["module"]
    if selected is None:
        return None
    if not re.fullmatch(r"package_[a-z][a-z0-9_]*", selected):
        raise ValueError("Invalid packaged session integration")
    path = Path(root) / "tools" / (selected + ".py")
    if not path.is_file():
        raise RuntimeError("Session integration missing: " + name)
    spec = importlib.util.spec_from_file_location("nacre_session_" + name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


def start(root, name):
    if name not in registry(root):
        raise ValueError("Unknown packaged compositor: " + name)
    backend = integration(root, name)
    if (
        backend is None
        or not backend.supported(root)
        or not hasattr(backend, "session")
    ):
        raise RuntimeError("Packaged session launcher unavailable: " + name)
    backend.session()
