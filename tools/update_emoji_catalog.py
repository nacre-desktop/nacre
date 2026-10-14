#!/usr/bin/env python3
"""Rebuild licensed emoji data from explicitly supplied Unicode and CLDR files."""

import argparse
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[1]
EMOJI_URL = "https://unicode.org/Public/17.0.0/emoji/emoji-test.txt"
CLDR_URL = "https://raw.githubusercontent.com/unicode-org/cldr-json/48.0.0/cldr-json/cldr-annotations-full/annotations/en/annotations.json"


def generate(emoji, annotations):
    keywords = json.loads(annotations)["annotations"]["annotations"]
    group = subgroup = ""
    rows = []
    for line in emoji.decode().splitlines():
        if line.startswith("# group:"):
            group = line.split(":", 1)[1].strip()
        if line.startswith("# subgroup:"):
            subgroup = line.split(":", 1)[1].strip()
        if "; fully-qualified" not in line:
            continue
        points = [int(code, 16) for code in line.split(";", 1)[0].split()]
        value = "".join(chr(code) for code in points)
        name = re.split(r" E[0-9.]+ ", line.split("#", 1)[1], maxsplit=1)[1]
        annotation = keywords.get(value, keywords.get(value.replace("\ufe0f", ""), {}))
        aliases = [subgroup.replace("-", " "), *annotation.get("default", [])]
        if len(points) == 2 and all(0x1F1E6 <= code <= 0x1F1FF for code in points):
            country = "".join(chr(code - 0x1F1E6 + ord("A")) for code in points)
            aliases.extend([country, "UK"] if country == "GB" else [country])
        rows.append(
            dict(
                value=value,
                name=name,
                category=group,
                aliases=list(dict.fromkeys(aliases)),
            )
        )
    if len(rows) != 3944 or len({row["value"] for row in rows}) != len(rows):
        raise ValueError("Expected all 3944 distinct fully qualified Unicode 17 emoji")
    return dict(
        version="17.0",
        source=EMOJI_URL,
        sha256=hashlib.sha256(emoji).hexdigest(),
        annotations=dict(
            version="48.0.0",
            source=CLDR_URL,
            sha256=hashlib.sha256(annotations).hexdigest(),
        ),
        entries=rows,
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--emoji", type=Path, required=True)
    parser.add_argument("--annotations", type=Path, required=True)
    parser.add_argument(
        "--output", type=Path, default=ROOT / "nacre/shell-tools/emoji-catalog.json"
    )
    args = parser.parse_args()
    value = generate(args.emoji.read_bytes(), args.annotations.read_bytes())
    args.output.write_text(
        json.dumps(value, ensure_ascii=False, separators=(",", ":")) + "\n"
    )


if __name__ == "__main__":
    main()
