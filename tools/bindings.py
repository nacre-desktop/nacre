"""Use the same restricted contract reader as the installed shortcut guide."""

import importlib.util
from pathlib import Path

SOURCE = Path(__file__).resolve().parents[1] / "nacre/shell-tools/binding-contracts.py"
spec = importlib.util.spec_from_file_location("nacre_binding_contracts", SOURCE)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
capture = module.capture
