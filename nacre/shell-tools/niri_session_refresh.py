"""Transactional refresh for the generated private Niri session."""

import shutil
import tempfile
from pathlib import Path


def backend_root(source_root=None, *, home=None):
    """Find the backend tree used to generate a session config."""
    if source_root is not None:
        return Path(source_root)
    home = Path.home() if home is None else Path(home)
    marker = home / ".local/share/nacre/shell/tools/nacre-backend-root"
    if marker.is_file():
        return Path(marker.read_text().strip())
    package = Path("/usr/share/nacre")
    if (package / "package.json.installed").is_file():
        return package
    compositor = home / ".local/share/nacre/shell/tools/nacre_native_compositor.py"
    if compositor.is_file():
        return compositor.resolve().parents[2]
    raise RuntimeError("The installed Nacre backend tree is unavailable")


def restore_tree(source, target):
    """Restore files in place so running bind mounts keep directory inodes."""
    target.mkdir(parents=True, exist_ok=True)
    for item in source.iterdir():
        destination = target / item.name
        if item.is_dir():
            restore_tree(item, destination)
        else:
            with tempfile.NamedTemporaryFile(dir=target, delete=False) as temporary:
                name = Path(temporary.name)
            try:
                shutil.copy2(item, name)
                name.replace(destination)
            finally:
                name.unlink(missing_ok=True)
    for item in target.iterdir():
        if not (source / item.name).exists():
            shutil.rmtree(item) if item.is_dir() else item.unlink()
    shutil.copystat(source, target)


def refresh(session, prepare, reload=None):
    """Back up, prepare and optionally reload, restoring the tree on failure."""
    backup = session.with_name("session.rollback")
    if backup.exists():
        shutil.rmtree(backup)
    shutil.copytree(session, backup)
    try:
        prepare()
        if reload is not None:
            reload()
    except BaseException:
        restore_tree(backup, session)
        shutil.rmtree(backup)
        raise
    return backup
