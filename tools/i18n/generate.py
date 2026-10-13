#!/usr/bin/env python3
"""Generate translation assets for Qt-only login and optional browser surfaces."""

import argparse
import json
from pathlib import Path
import subprocess
import tempfile
import shutil

ROOT = Path(__file__).resolve().parents[2]


def assets(root=ROOT):
    english = json.loads((root / "nacre/shell/i18n/en.json").read_text())
    login = {
        key: value
        for key, value in english.items()
        if key.startswith(("login.", "ui.brand-logo."))
    }
    browser = {
        key: value for key, value in english.items() if key.startswith("brain.ui.")
    }
    engine = (
        (root / "nacre/shell/i18n/translation.js")
        .read_text()
        .removeprefix(".pragma library\n")
    )
    return {
        Path("nacre/login/messages.js"): ".pragma library\n\nconst english = "
        + json.dumps(login, indent=2, ensure_ascii=False, sort_keys=True)
        + ";\n",
        Path("brain/web/i18n-data.js"): '"use strict";\nwindow.NacreEnglish = '
        + json.dumps(browser, indent=2, ensure_ascii=False, sort_keys=True)
        + ";\n",
        Path("brain/web/translation-engine.js"): '"use strict";\n'
        + engine
        + "\nwindow.NacreTranslation = {translate, pseudo, format};\n",
    }


def format_asset(path, content, root=ROOT):
    if path.parts[0] == "nacre":
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / path.name
            source.write_text(content)
            return subprocess.check_output(
                ["/usr/lib/qt6/bin/qmlformat", str(source)], text=True
            )
    node = shutil.which("node")
    command = [node] if node else ["nacre-ai-tools", "node"]
    return subprocess.check_output(
        [
            *command,
            str(root / "node_modules/prettier/bin/prettier.cjs"),
            "--parser",
            "babel",
        ],
        input=content,
        text=True,
    )


def verify(root=ROOT, write=False):
    for path, content in assets(root).items():
        expected = format_asset(path, content, root)
        target = root / path
        if write:
            target.write_text(expected)
        elif not target.exists() or target.read_text() != expected:
            raise ValueError("Generated translation asset is stale: " + str(path))
    print("Generated translation assets " + ("written." if write else "are current."))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--write", action="store_true")
    verify(write=parser.parse_args().write)


if __name__ == "__main__":
    main()
