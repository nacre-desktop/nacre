"""Developer-only system-icon fixture boundary; all theme behavior is native."""

from pathlib import Path
from native_lock_dashboard import invoke

SYSTEM_ICONS = Path("/usr/share/icons")


def theme(home, primary, mode):
    return invoke(
        "file_icons_theme",
        home=str(home),
        system_icons=str(SYSTEM_ICONS),
        primary=primary,
        mode=mode,
    )
