#!/usr/bin/env python3
"""Hand over to `nacre-helper updates` (cached count, refresh, preflight, host check)."""

import os
import sys
from pathlib import Path

if __name__ == "__main__":
    helper = Path(
        os.environ.get("NACRE_HELPER_BINARY", Path(__file__).with_name("nacre-helper"))
    )
    if not helper.is_file():
        raise SystemExit("nacre-helper is not installed beside updates.py")
    os.execv(helper, [str(helper), "updates", *sys.argv[1:]])
