"""Transactional refresh for the generated private Niri session."""

import shutil
import tempfile
from pathlib import Path


def backend_root(*, home=None, package_root=None):
    """Find the installed backend tree used to generate a session config."""
    if (
        package_root is not None
        and (Path(package_root) / "package.json.installed").is_file()
    ):
        return Path(package_root)
    home = Path.home() if home is None else Path(home)
    return home / ".local/share/nacre/shell/tools/backend"


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
