"""Cooperative, path-pinned lease for one Nacre config destination.

The lock is advisory: older Nacre versions and external editors are not fenced.
"""

from __future__ import annotations

from contextlib import contextmanager
import errno
import fcntl
import os
from pathlib import Path
import secrets
import stat


class WriterLeaseError(RuntimeError):
    """The destination is busy or its filesystem ownership is unsafe."""


def _fail():
    # Keep the destination and filesystem details out of user-facing errors.
    raise WriterLeaseError("Nacre destination busy or unsafe")


def _components(path: Path) -> list[str]:
    if not path.is_absolute() or ".." in path.parts or "." in path.parts:
        _fail()
    return [part for part in path.parts if part != "/"]


def _validate_file(meta) -> None:
    if meta is not None and (
        not stat.S_ISREG(meta.st_mode)
        or meta.st_uid != os.getuid()
        or meta.st_nlink != 1
        or meta.st_mode & 0o022
    ):
        _fail()


def _validate_chain(
    dirs: list[int], names: list[str], *, final_private: bool = True
) -> None:
    uid = os.getuid()
    root_uid = os.fstat(dirs[0]).st_uid
    anchor_next = False
    for index, fd in enumerate(dirs):
        meta = os.fstat(fd)
        if not stat.S_ISDIR(meta.st_mode):
            _fail()
        if anchor_next:
            if meta.st_uid != uid or stat.S_IMODE(meta.st_mode) != 0o700:
                _fail()
            anchor_next = False
        elif meta.st_uid not in (root_uid, uid):
            _fail()
        elif meta.st_mode & 0o022:
            if meta.st_uid != root_uid or stat.S_IMODE(meta.st_mode) != 0o1777:
                _fail()
            anchor_next = True
        if (
            final_private
            and index == len(dirs) - 1
            and (meta.st_uid != uid or meta.st_mode & 0o022)
        ):
            _fail()
        if index < len(names):
            try:
                named = os.stat(names[index], dir_fd=fd, follow_symlinks=False)
                child = os.fstat(dirs[index + 1])
            except OSError:
                _fail()
            if not stat.S_ISDIR(named.st_mode) or (named.st_dev, named.st_ino) != (
                child.st_dev,
                child.st_ino,
            ):
                _fail()
    if anchor_next and final_private:
        _fail()


def _directory(fd: int, name: str) -> int:
    try:
        return os.open(
            name,
            os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC | os.O_NOFOLLOW,
            dir_fd=fd,
        )
    except OSError:
        _fail()


class PinnedDirectory:
    """Validated directory fd; every operation stays relative to this inode."""

    def __init__(self, root: Path, dirs: list[int], names: list[str]):
        self.root = root
        self._dirs = dirs
        self._names = names
        self._closed = False
        self.ensure_valid()

    def ensure_valid(self) -> None:
        if self._closed:
            _fail()
        _validate_chain(self._dirs, self._names)

    def stat(self, name: str):
        if not name or "/" in name or name in (".", ".."):
            _fail()
        self.ensure_valid()
        try:
            return os.stat(name, dir_fd=self._dirs[-1], follow_symlinks=False)
        except FileNotFoundError:
            return None
        except OSError:
            _fail()

    def readlink(self, name: str) -> str | None:
        """Read one owned symlink target without following the link."""
        if (
            not isinstance(name, str)
            or not name
            or "/" in name
            or "\0" in name
            or name in (".", "..")
        ):
            _fail()

        def link_stat():
            try:
                return os.stat(name, dir_fd=self._dirs[-1], follow_symlinks=False)
            except FileNotFoundError:
                return None
            except OSError:
                _fail()

        def validate_link(meta) -> None:
            if meta is None or (
                not stat.S_ISLNK(meta.st_mode)
                or meta.st_uid != os.getuid()
                or stat.S_IMODE(meta.st_mode) != 0o777
                or meta.st_nlink != 1
            ):
                _fail()

        def identity(meta):
            return (
                meta.st_dev,
                meta.st_ino,
                meta.st_uid,
                stat.S_IMODE(meta.st_mode),
                meta.st_ctime_ns,
                meta.st_mtime_ns,
                meta.st_size,
                meta.st_nlink,
            )

        self.ensure_valid()
        before = link_stat()
        if before is None:
            self.ensure_valid()
            if link_stat() is not None:
                _fail()
            self.ensure_valid()
            return None
        validate_link(before)
        self.ensure_valid()
        path_flag = getattr(os, "O_PATH", None)
        if path_flag is None:
            _fail()
        link_fd = None
        try:
            link_fd = os.open(
                name,
                path_flag | os.O_NOFOLLOW | os.O_CLOEXEC,
                dir_fd=self._dirs[-1],
            )
            pinned_before = os.fstat(link_fd)
            validate_link(pinned_before)
            if identity(before) != identity(pinned_before):
                _fail()
            self.ensure_valid()
            target = os.readlink("", dir_fd=link_fd)
            pinned_after = os.fstat(link_fd)
            validate_link(pinned_after)
            if identity(pinned_before) != identity(pinned_after):
                _fail()
            named_after = link_stat()
            validate_link(named_after)
            if identity(pinned_after) != identity(named_after):
                _fail()
            self.ensure_valid()
            return target
        except WriterLeaseError:
            raise
        except OSError:
            _fail()
        finally:
            if link_fd is not None:
                os.close(link_fd)

    def identity(self) -> tuple[int, int]:
        self.ensure_valid()
        meta = os.fstat(self._dirs[-1])
        return meta.st_dev, meta.st_ino

    def read(self, name: str, limit: int | None = None) -> bytes | None:
        before = self.stat(name)
        if before is None:
            return None
        _validate_file(before)
        try:
            fd = os.open(
                name,
                os.O_RDONLY | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK,
                dir_fd=self._dirs[-1],
            )
            opened = os.fstat(fd)
            if (before.st_dev, before.st_ino) != (opened.st_dev, opened.st_ino):
                _fail()
            with os.fdopen(fd, "rb", closefd=True) as stream:
                data = stream.read() if limit is None else stream.read(limit + 1)
            if limit is not None and len(data) > limit:
                _fail()
            self.ensure_valid()
            return data
        except OSError:
            _fail()

    def open_file(
        self,
        name: str,
        *,
        flags: int = os.O_RDWR,
        mode: int = 0o600,
        create: bool = False,
    ) -> int:
        """Open one checked regular file relative to the pinned directory fd."""
        if not name or "/" in name or name in (".", "..") or mode & 0o022:
            _fail()
        self.ensure_valid()
        open_flags = flags | os.O_CLOEXEC | os.O_NOFOLLOW | os.O_NONBLOCK
        if create:
            open_flags |= os.O_CREAT
        try:
            fd = os.open(name, open_flags, mode, dir_fd=self._dirs[-1])
            opened = os.fstat(fd)
            named = os.stat(name, dir_fd=self._dirs[-1], follow_symlinks=False)
            _validate_file(opened)
            if (opened.st_dev, opened.st_ino) != (named.st_dev, named.st_ino):
                _fail()
            self.ensure_valid()
            return fd
        except OSError:
            _fail()

    def unlink(self, name: str) -> None:
        current = self.stat(name)
        if current is None:
            return
        _validate_file(current)
        self.ensure_valid()
        try:
            os.unlink(name, dir_fd=self._dirs[-1])
            os.fsync(self._dirs[-1])
        except OSError:
            _fail()
        self.ensure_valid()

    def backup_once(self, name: str, content: bytes) -> bool:
        """Create a private, no-clobber backup below this pinned directory."""
        if not name or "/" in name or name in (".", ".."):
            _fail()
        current = self.stat(name)
        if current is not None:
            _validate_file(current)
            return False
        temp = ".backup-" + secrets.token_hex(12) + ".next"
        root_fd = self._dirs[-1]
        fd = -1
        try:
            fd = os.open(
                temp,
                os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC | os.O_NOFOLLOW,
                0o600,
                dir_fd=root_fd,
            )
            view = memoryview(content)
            while view:
                view = view[os.write(fd, view) :]
            os.fsync(fd)
            os.close(fd)
            fd = -1
            self.ensure_valid()
            try:
                os.link(
                    temp,
                    name,
                    src_dir_fd=root_fd,
                    dst_dir_fd=root_fd,
                    follow_symlinks=False,
                )
                created = True
            except FileExistsError:
                created = False
            if not created:
                existing = self.stat(name)
                _validate_file(existing)
            os.fsync(root_fd)
            self.ensure_valid()
            return created
        except OSError:
            _fail()
        finally:
            if fd >= 0:
                os.close(fd)
            try:
                os.unlink(temp, dir_fd=root_fd)
            except OSError as error:
                if error.errno != errno.ENOENT:
                    pass

    def sync(self) -> None:
        self.ensure_valid()
        try:
            os.fsync(self._dirs[-1])
        except OSError:
            _fail()
        self.ensure_valid()

    def publish(self, name: str, content: bytes, mode: int = 0o600) -> None:
        """Atomically publish a private regular file below the pinned root."""
        if (
            not name
            or "/" in name
            or name in (".", "..")
            or mode & 0o022
            or mode & ~0o777
        ):
            _fail()
        self.ensure_valid()
        root_fd = self._dirs[-1]
        try:
            existing = os.stat(name, dir_fd=root_fd, follow_symlinks=False)
        except FileNotFoundError:
            existing = None
        except OSError:
            _fail()
        _validate_file(existing)

        temp = ".writer-" + secrets.token_hex(12) + ".next"
        fd = -1
        try:
            fd = os.open(
                temp,
                os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_CLOEXEC | os.O_NOFOLLOW,
                mode,
                dir_fd=root_fd,
            )
            view = memoryview(content)
            while view:
                written = os.write(fd, view)
                if written <= 0:
                    _fail()
                view = view[written:]
            os.fsync(fd)
            os.close(fd)
            fd = -1
            self.ensure_valid()
            try:
                current = os.stat(name, dir_fd=root_fd, follow_symlinks=False)
            except FileNotFoundError:
                current = None
            _validate_file(current)
            if (existing is None) != (current is None) or (
                existing is not None
                and current is not None
                and (existing.st_dev, existing.st_ino)
                != (current.st_dev, current.st_ino)
            ):
                _fail()
            os.replace(temp, name, src_dir_fd=root_fd, dst_dir_fd=root_fd)
            os.fsync(root_fd)
            self.ensure_valid()
        except OSError:
            _fail()
        finally:
            if fd >= 0:
                os.close(fd)
            try:
                os.unlink(temp, dir_fd=root_fd)
            except OSError as error:
                if error.errno != errno.ENOENT:
                    pass

    def close(self) -> None:
        if self._closed:
            return
        self._closed = True
        for fd in reversed(self._dirs):
            os.close(fd)


class Lease(PinnedDirectory):
    """Pinned config directory plus exclusive cooperative writer lock."""

    def __init__(self, root: Path, dirs: list[int], names: list[str], lock_fd: int):
        self.fd = lock_fd
        super().__init__(root, dirs, names)

    def ensure_valid(self) -> None:
        super().ensure_valid()
        uid = os.getuid()
        try:
            named = os.stat(
                ".writer.lock", dir_fd=self._dirs[-1], follow_symlinks=False
            )
            opened = os.fstat(self.fd)
        except OSError:
            _fail()
        if (
            not stat.S_ISREG(named.st_mode)
            or named.st_dev != opened.st_dev
            or named.st_ino != opened.st_ino
            or opened.st_uid != uid
            or opened.st_nlink != 1
            or stat.S_IMODE(opened.st_mode) != 0o600
        ):
            _fail()

    def close(self) -> None:
        if self._closed:
            return
        os.close(self.fd)
        super().close()


def _open_root(path: Path, *, create: bool = False) -> tuple[list[int], list[str]]:
    uid = os.getuid()
    # In an unprivileged user namespace, root-owned host ancestors can map to
    # an unmapped UID. Match the filesystem root's observed ownership, as the
    # daemon socket path policy does.
    root_uid = os.stat("/").st_uid
    components = _components(path)
    dirs = [os.open("/", os.O_RDONLY | os.O_DIRECTORY | os.O_CLOEXEC)]
    names: list[str] = []
    sticky_anchor_required = False
    try:
        for index, name in enumerate(components):
            _validate_chain(dirs, names, final_private=False)
            parent = os.fstat(dirs[-1])
            if parent.st_uid not in (root_uid, uid) or (
                parent.st_mode & 0o022
                and not (
                    parent.st_uid == root_uid and stat.S_IMODE(parent.st_mode) == 0o1777
                )
            ):
                _fail()
            if parent.st_uid == root_uid and parent.st_mode & 0o022:
                # A sticky shared directory is allowed only as the parent of a
                # pre-existing, caller-owned private anchor.
                sticky_anchor_required = True
            try:
                child = _directory(dirs[-1], name)
            except WriterLeaseError:
                if sticky_anchor_required or not create:
                    _fail()
                try:
                    os.mkdir(name, 0o700, dir_fd=dirs[-1])
                    child = _directory(dirs[-1], name)
                except OSError:
                    _fail()
            meta = os.fstat(child)
            if not stat.S_ISDIR(meta.st_mode):
                os.close(child)
                _fail()
            if sticky_anchor_required:
                if meta.st_uid != uid or stat.S_IMODE(meta.st_mode) != 0o700:
                    os.close(child)
                    _fail()
                sticky_anchor_required = False
            elif meta.st_uid not in (root_uid, uid) or (
                meta.st_mode & 0o022
                and not (
                    meta.st_uid == root_uid and stat.S_IMODE(meta.st_mode) == 0o1777
                )
            ):
                os.close(child)
                _fail()
            # The destination itself is always caller-owned.
            if index + 1 == len(components) and (
                meta.st_uid != uid or meta.st_mode & 0o022
            ):
                os.close(child)
                _fail()
            names.append(name)
            dirs.append(child)
            _validate_chain(dirs, names, final_private=index + 1 == len(components))
        return dirs, names
    except BaseException:
        for fd in reversed(dirs):
            os.close(fd)
        raise


def _lock(
    root: Path, dirs: list[int], names: list[str], inherited_fd: int | None
) -> int:
    uid = os.getuid()
    root_fd = dirs[-1]
    try:
        if inherited_fd is None:
            fd = os.open(
                ".writer.lock",
                os.O_RDWR | os.O_CREAT | os.O_CLOEXEC | os.O_NOFOLLOW,
                0o600,
                dir_fd=root_fd,
            )
        else:
            fd = inherited_fd
        named = os.stat(".writer.lock", dir_fd=root_fd, follow_symlinks=False)
        opened = os.fstat(fd)
        if (
            not stat.S_ISREG(named.st_mode)
            or not stat.S_ISREG(opened.st_mode)
            or opened.st_uid != uid
            or opened.st_nlink != 1
            or stat.S_IMODE(opened.st_mode) != 0o600
            or (named.st_dev, named.st_ino) != (opened.st_dev, opened.st_ino)
        ):
            _fail()
        if inherited_fd is None:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        else:
            # Prove both that the inode is held and that this inherited open
            # description owns the lock. flock locks follow Linux open-file
            # descriptions across fork/exec, so reacquiring on that same fd
            # succeeds while an independent probe remains blocked.
            probe = os.open(
                ".writer.lock",
                os.O_RDWR | os.O_CLOEXEC | os.O_NOFOLLOW,
                dir_fd=root_fd,
            )
            try:
                try:
                    fcntl.flock(probe, fcntl.LOCK_EX | fcntl.LOCK_NB)
                except BlockingIOError:
                    pass
                else:
                    _fail()
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            finally:
                os.close(probe)
        return fd
    except WriterLeaseError:
        raise
    except OSError:
        _fail()


@contextmanager
def pinned_directory(
    path: Path,
    *,
    create: bool = False,
    expected_identity: tuple[int, int] | None = None,
):
    """Pin a safe directory for fd-relative reads, replacement and removal.

    `create=True` creates missing descendants at 0700, except that an anchor
    immediately below a root-owned 01777 sticky directory must already exist
    as caller-owned mode 0700.
    """
    root = Path(path)
    dirs, names = _open_root(root, create=create)
    try:
        directory = PinnedDirectory(root, dirs, names)
        if expected_identity is not None and directory.identity() != expected_identity:
            _fail()
        try:
            yield directory
        finally:
            directory.close()
    except BaseException:
        for fd in reversed(dirs):
            try:
                os.close(fd)
            except OSError:
                pass
        raise


@contextmanager
def writer_lease(config_dir: Path, inherited_fd: int | None = None):
    """Acquire an exclusive nonblocking lease on an actual Nacre config root."""
    root = Path(config_dir)
    dirs, names = _open_root(root, create=True)
    lock_fd = -1
    lease = None
    try:
        lock_fd = _lock(root, dirs, names, inherited_fd)
        lease = Lease(root, dirs, names, lock_fd)
        yield lease
    except BaseException:
        if lease is not None:
            lease.close()
        else:
            if lock_fd >= 0:
                os.close(lock_fd)
            elif inherited_fd is not None:
                # A failed child validation must not close a parent's descriptor;
                # the inherited process duplicate closes at process exit.
                pass
            for fd in reversed(dirs):
                try:
                    os.close(fd)
                except OSError:
                    pass
        raise
    else:
        if lease is not None:
            lease.close()


def inherited_fd_from_env() -> int | None:
    value = os.environ.get("NACRE_WRITER_LEASE_FD")
    if value is None:
        return None
    try:
        fd = int(value)
        if fd < 3:
            _fail()
        return fd
    except ValueError:
        _fail()
