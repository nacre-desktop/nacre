#!/usr/bin/env python3
"""Generate backend configuration from the versioned neutral compositor contract."""

import argparse
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]
DIRECTORY = ROOT / "nacre/shell-tools"
sys.path.insert(0, str(DIRECTORY))
from nacre_native_config import load, render  # noqa: E402 - thin native client


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--check",
        action="store_true",
        help="Fail on generated output drift without writing files",
    )
    parser.add_argument("--backend", choices=("hyprland", "niri", "all"), default="all")
    args = parser.parse_args()
    generated = {}
    for name in ("hyprland", "niri") if args.backend == "all" else (args.backend,):
        generated.update(render(load(), backend=name))
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
            (ROOT / path).parent.mkdir(parents=True, exist_ok=True)
            (ROOT / path).write_text(generated[path])
    print(
        "Compositor configuration matches its neutral source."
        if args.check
        else "Generated compositor configuration."
    )


if __name__ == "__main__":
    main()
