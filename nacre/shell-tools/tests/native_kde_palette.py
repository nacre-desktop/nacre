"""Developer-only adapter retaining historical KDE publisher assertions."""

import json
import subprocess

from nacre_native_config import _binary


def publish(home, colors, icon_theme):
    subprocess.run(
        [str(_binary()), "kde-palette", "--publish"],
        input=json.dumps(
            {"home": str(home), "colors": colors, "icon_theme": icon_theme}
        ),
        text=True,
        capture_output=True,
        check=True,
        timeout=10,
    )
