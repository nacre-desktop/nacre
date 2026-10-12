#!/usr/bin/env python3
"""Generate backend configuration from the versioned neutral compositor contract."""

import argparse
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / "nacre/shell-tools"
sys.path.insert(0, str(DIRECTORY))
from compositor_config import load, render  # noqa: E402 - source module lives with installed helpers


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Fail on generated output drift without writing files",
    )
    args = parser.parse_args()
    generated = render(load())
    different = [
        path
        for path, text in generated.items()
        if not (ROOT / path).exists() or (ROOT / path).read_text() != text
    ]
    if args.check and different:
        parser.exit(
            1,
            "Generated compositor config differs; run tools/generate_compositor_config.py:\n"
            + "\n".join(different)
            + "\n",
        )
    if not args.check:
        for path in different:
            (ROOT / path).write_text(generated[path])
    print(
        "Compositor configuration matches its neutral source."
        if args.check
        else "Generated compositor configuration."
    )


if __name__ == "__main__":
    main()
