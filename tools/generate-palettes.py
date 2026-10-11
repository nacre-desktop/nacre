#!/usr/bin/env python3
"""Reproducible fixed palettes using Orient's independent role policy."""

import argparse
import json
from pathlib import Path
from nacre_shell.engine import default_palette
from nacre_shell.palette import generate

SOFT = [
    ("amethyst", "Amethyst", "8b6bd1"),
    ("ocean", "Ocean", "2785b6"),
    ("forest", "Forest", "4c8861"),
    ("rose", "Rose", "b95879"),
    ("ember", "Ember", "bb713d"),
    ("slate", "Slate", "73808c"),
    ("soft-red", "Blush Red", "e54a54"),
    ("soft-orange", "Apricot", "ef8a47"),
    ("soft-gold", "Honey", "c59628"),
    ("soft-yellow", "Butter Yellow", "dac343"),
    ("soft-lime", "Pear", "9eaf38"),
    ("soft-green", "Sage", "6b9b67"),
    ("soft-teal", "Sea Glass", "409e91"),
    ("soft-cyan", "Glacier", "32a8c3"),
    ("soft-blue", "Cornflower", "4e78db"),
    ("soft-indigo", "Periwinkle", "6964c8"),
    ("soft-purple", "Lilac", "a05acb"),
    ("soft-pink", "Candy Pink", "de5caa"),
]
VIVID = [
    ("super-red", "Super Red", "ff3030"),
    ("flame-orange", "Flame Orange", "ff791f"),
    ("solar-gold", "Solar Gold", "ffb800"),
    ("electric-yellow", "Electric Yellow", "f5ed00"),
    ("acid-lime", "Acid Lime", "b5ed14"),
    ("emerald", "Emerald", "11d96c"),
    ("deep-teal", "Deep Teal", "00b99d"),
    ("electric-cyan", "Electric Cyan", "00d6f5"),
    ("cobalt-blue", "Cobalt Blue", "4275ff"),
    ("royal-indigo", "Royal Indigo", "7155ff"),
    ("vivid-purple", "Violet", "b147ff"),
    ("hot-pink", "Hot Pink", "ff38b8"),
]


def build():
    presets = []
    for group, choices in (("soft", SOFT), ("vivid", VIVID)):
        for ident, name, seed in choices:
            modes = {}
            for mode in ("dark", "light"):
                variant = "neutral" if group == "soft" else "vibrant"
                modes[mode] = generate(seed, mode, variant)
            presets.append(
                dict(id=ident, name=name, seed="#" + seed, group=group, modes=modes)
            )
    return presets


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--output-dir",
        type=Path,
        default=Path(__file__).resolve().parents[1] / "nacre/shell-tools",
    )
    args = parser.parse_args()
    args.output_dir.mkdir(parents=True, exist_ok=True)
    for name, value in (
        ("palette-presets.json", build()),
        ("reference-style.json", default_palette()),
    ):
        (args.output_dir / name).write_text(json.dumps(value, indent=2) + "\n")


if __name__ == "__main__":
    main()
