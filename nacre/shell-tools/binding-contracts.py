"""Compatibility contract reader; native declaration parsing belongs to the backend."""

import importlib.util
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "nacre_compositor_contract_reader", Path(__file__).with_name("compositor.py")
)
compositor = importlib.util.module_from_spec(spec)
spec.loader.exec_module(compositor)
capture = compositor.capture_bindings
