"""Restore the asset input set used by pre-bundle immutable palette captures."""

import hashlib
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
    # Captures used the original publisher metadata, before the Dolphin reader
    # correction. Supply that immutable input rather than normalize its output.
    manifest = (
        ROOT / "crates/nacre-helper/tests/fixtures/theme-targets-original-v1.json"
    )
    assert (
        hashlib.sha256(manifest.read_bytes()).hexdigest()
        == "744f98c10e07fe12f2b09ab3464dbbe89ca39ba03e85351e6a3026b0f4cc81fa"
    ), "Historical publisher asset input changed"
    shutil.copy2(manifest, tools / "theme-targets.json")
    (root / "shell").symlink_to(ROOT / "nacre/shell", target_is_directory=True)
    return str(tools)
