#!/usr/bin/env python3
"""Entry point for the installed unit; the native worker is the only implementation."""

import os
from pathlib import Path
import sys

helper = os.environ.get("NACRE_HELPER_BINARY") or str(
    Path.home() / ".local/share/nacre/shell/tools/nacre-helper"
)
os.execv(helper, [helper, "brain-control", *sys.argv[1:]])
