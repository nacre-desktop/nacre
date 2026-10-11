#!/usr/bin/env python3
"""Install the optional local CPU model without touching project/system Python."""

import subprocess
import sys
from pathlib import Path

root = Path.home() / ".local/share/nacre/brain-model"
python = root / "bin/python"
if not python.exists():
    subprocess.run([sys.executable, "-m", "venv", str(root)], check=True)
subprocess.run(
    [
        str(python),
        "-m",
        "pip",
        "install",
        "fastembed==0.8.1",
        "onnxruntime==1.30.0",
        "tokenizers==0.23.2",
    ],
    check=True,
)
subprocess.run(
    [str(python), str(Path(__file__).with_name("semantic.py")), "--download"],
    check=True,
)
