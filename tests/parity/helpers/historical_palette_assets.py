"""Restore the asset input set used by pre-bundle immutable palette captures."""

from pathlib import Path
import shutil

ROOT = Path(__file__).resolve().parents[3]


def stage(base):
    root = Path(base) / "historical-assets"
    if root.exists():
        return str(root / "shell-tools")
    root.mkdir()
    tools = root / "shell-tools"
    tools.mkdir()
    for source in (ROOT / "nacre/shell-tools").iterdir():
        if source.name != "theme-bundles":
            if source.is_dir():
                (tools / source.name).symlink_to(source, target_is_directory=True)
            else:
                shutil.copy2(source, tools / source.name)
    (root / "shell").symlink_to(ROOT / "nacre/shell", target_is_directory=True)
    return str(tools)
