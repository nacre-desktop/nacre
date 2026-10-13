#!/usr/bin/env python3
"""Region selection copies a PNG; full capture saves and copies without a picker."""

import argparse
from nacre_i18n import tr
from datetime import datetime
from pathlib import Path
import subprocess


def notify(message):
    subprocess.run(
        [
            "notify-send",
            "-a",
            "Nacre Capture",
            "-i",
            "nacre-notifications",
            tr("capture.notification.title", "Screenshot"),
            message,
        ],
        check=False,
    )


def capture(mode, home=None, runner=subprocess.run):
    home = home or Path.home()
    if mode == "region":
        selection = runner(["slurp"], capture_output=True, text=True)
        if selection.returncode or not selection.stdout.strip():
            return None
        png = runner(
            ["grim", "-g", selection.stdout.strip(), "-"],
            capture_output=True,
            check=True,
        ).stdout
        runner(["wl-copy", "--type", "image/png"], input=png, check=True)
        return tr(
            "helper.screenshot.copied-selection-to-clipboard",
            "Copied selection to clipboard",
        )
    if mode != "full":
        raise ValueError(
            tr("helper.screenshot.unknown-screenshot-mode", "Unknown screenshot mode")
        )
    folder = home / "Pictures/Screenshots"
    folder.mkdir(parents=True, exist_ok=True)
    path = folder / (
        "screenshot_" + datetime.now().strftime("%Y%m%d_%H%M%S_%f") + ".png"
    )
    runner(["grim", str(path)], check=True)
    runner(["wl-copy", "--type", "image/png"], input=path.read_bytes(), check=True)
    return tr(
        "helper.screenshot.saved-to-pictures-screenshots-and-copied-to-clipboard",
        "Saved to Pictures/Screenshots and copied to clipboard",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("mode", choices=("region", "full"))
    args = parser.parse_args()
    try:
        result = capture(args.mode)
    except (OSError, subprocess.SubprocessError) as error:
        notify(
            tr(
                "capture.notification.failed",
                "Capture failed: {error}",
                {"error": str(error)},
            )
        )
        return 1
    if result:
        notify(result)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
