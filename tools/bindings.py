"""Use the same restricted contract reader as the installed shortcut guide."""

import importlib.util
import sys
from pathlib import Path

SOURCE = Path(__file__).resolve().parents[1] / "nacre/shell-tools/binding-contracts.py"
sys.path.insert(0, str(SOURCE.parent))
spec = importlib.util.spec_from_file_location("nacre_binding_contracts", SOURCE)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
capture = module.capture
