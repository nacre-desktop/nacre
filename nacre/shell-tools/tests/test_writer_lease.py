import importlib.util
import os
from pathlib import Path
import stat
import tempfile
from types import SimpleNamespace
import unittest
from unittest import mock


MODULE = Path(__file__).parents[1] / "writer_lease.py"
SPEC = importlib.util.spec_from_file_location("writer_lease", MODULE)
writer_lease = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(writer_lease)


class WriterLeaseTests(unittest.TestCase):
    def test_readlink_returns_the_target_without_following_it(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "private" / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                target = root / "target-file"
                target.write_text("link target contents")
                (root / "socket-link").symlink_to(target.name)
                self.assertEqual(lease.readlink("socket-link"), target.name)
                self.assertIsNone(lease.readlink("missing-link"))

    def test_readlink_rejects_replacement_during_read(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "private" / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                link = root / "socket-link"
                link.symlink_to("first-target")
                original_readlink = os.readlink

                def replace_then_read(name, *, dir_fd=None):
                    os.unlink(name, dir_fd=dir_fd)
                    os.symlink("replacement-target", name, dir_fd=dir_fd)
                    return original_readlink(name, dir_fd=dir_fd)

                with mock.patch.object(
                    writer_lease.os, "readlink", side_effect=replace_then_read
                ):
                    with self.assertRaises(writer_lease.WriterLeaseError):
                        lease.readlink("socket-link")
                self.assertEqual(os.readlink(link), "replacement-target")

    def test_readlink_rejects_changed_ctime_during_read(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "private" / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                link = root / "socket-link"
                link.symlink_to("target")
                original_readlink = os.readlink

                def touch_then_read(name, *, dir_fd=None):
                    current = os.stat(name, dir_fd=dir_fd, follow_symlinks=False)
                    os.utime(
                        name,
                        ns=(current.st_atime_ns, current.st_mtime_ns + 1_000_000_000),
                        dir_fd=dir_fd,
                        follow_symlinks=False,
                    )
                    return original_readlink(name, dir_fd=dir_fd)

                with mock.patch.object(
                    writer_lease.os, "readlink", side_effect=touch_then_read
                ):
                    with self.assertRaises(writer_lease.WriterLeaseError):
                        lease.readlink("socket-link")

    def test_readlink_rejects_foreign_owner_or_unsafe_link_mode(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "private" / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                (root / "socket-link").symlink_to("target")
                original_stat = os.stat
                root_fd = lease._dirs[-1]

                for overrides in (
                    {"st_uid": os.getuid() + 1},
                    {"st_mode": stat.S_IFLNK | 0o755},
                ):
                    with self.subTest(overrides=overrides):

                        def unsafe_link_stat(name, *args, expected=overrides, **kwargs):
                            meta = original_stat(name, *args, **kwargs)
                            if (
                                name == "socket-link"
                                and kwargs.get("dir_fd") == root_fd
                            ):
                                values = {
                                    "st_dev": meta.st_dev,
                                    "st_ino": meta.st_ino,
                                    "st_uid": meta.st_uid,
                                    "st_mode": meta.st_mode,
                                    "st_ctime_ns": meta.st_ctime_ns,
                                    "st_nlink": meta.st_nlink,
                                }
                                values.update(expected)
                                return SimpleNamespace(**values)
                            return meta

                        with mock.patch.object(
                            writer_lease.os, "stat", side_effect=unsafe_link_stat
                        ):
                            with self.assertRaises(writer_lease.WriterLeaseError):
                                lease.readlink("socket-link")

    def test_readlink_missing_result_requires_stable_absence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "private" / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                original_stat = os.stat
                inserted = False
                root_fd = lease._dirs[-1]

                def insert_after_missing(name, *args, **kwargs):
                    nonlocal inserted
                    try:
                        return original_stat(name, *args, **kwargs)
                    except FileNotFoundError:
                        if (
                            name == "appearing-link"
                            and kwargs.get("dir_fd") == root_fd
                            and not inserted
                        ):
                            inserted = True
                            os.symlink("target", name, dir_fd=root_fd)
                        raise

                with mock.patch.object(
                    writer_lease.os, "stat", side_effect=insert_after_missing
                ):
                    with self.assertRaises(writer_lease.WriterLeaseError):
                        lease.readlink("appearing-link")

    def test_readlink_rejects_nonlink_and_invalid_basename(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "private" / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                (root / "regular-file").write_text("not a link")
                with self.assertRaises(writer_lease.WriterLeaseError):
                    lease.readlink("regular-file")
                for name in (
                    "nested/link",
                    "..",
                    ".",
                    "",
                    b"socket-link",
                    "bad\0name",
                ):
                    with self.assertRaises(writer_lease.WriterLeaseError):
                        lease.readlink(name)

    def test_readlink_rejects_unsafe_parent_and_replaced_ancestry(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            parent = base / "parent"
            parent.mkdir(mode=0o700)
            root = parent / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                (root / "socket-link").symlink_to("target")
                parent.chmod(0o770)
                with self.assertRaises(writer_lease.WriterLeaseError):
                    lease.readlink("socket-link")

        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            parent = base / "parent"
            parent.mkdir(mode=0o700)
            config = parent / "config"
            root = config / "nacre"
            with writer_lease.writer_lease(root) as lease:
                (root / "socket-link").symlink_to("target")
                original_readlink = os.readlink

                def replace_ancestor_then_read(name, *, dir_fd=None):
                    retired = base / "retired-config"
                    config.rename(retired)
                    config.mkdir(mode=0o700)
                    (config / "nacre").mkdir(mode=0o700)
                    return original_readlink(name, dir_fd=dir_fd)

                with mock.patch.object(
                    writer_lease.os,
                    "readlink",
                    side_effect=replace_ancestor_then_read,
                ):
                    with self.assertRaises(writer_lease.WriterLeaseError):
                        lease.readlink("socket-link")
                self.assertFalse((config / "nacre" / "socket-link").exists())

    def test_lease_publishes_and_excludes_another_writer(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "private" / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                lease.publish("profile.lua", b"managed\n")
                self.assertEqual((root / "profile.lua").read_bytes(), b"managed\n")
                with self.assertRaises(writer_lease.WriterLeaseError):
                    with writer_lease.writer_lease(root):
                        self.fail("second writer acquired a held destination")

    def test_inherited_fd_must_be_the_lock_owner(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                duplicate = os.dup(lease.fd)
                with writer_lease.writer_lease(root, inherited_fd=duplicate):
                    pass
                with self.assertRaises(writer_lease.WriterLeaseError):
                    with writer_lease.writer_lease(root):
                        self.fail(
                            "closing inherited duplicate released the parent's lock"
                        )

                unlocked = os.open(root / ".writer.lock", os.O_RDWR | os.O_CLOEXEC)
                try:
                    with self.assertRaises(writer_lease.WriterLeaseError):
                        with writer_lease.writer_lease(root, inherited_fd=unlocked):
                            self.fail("unlocked descriptor was accepted")
                finally:
                    os.close(unlocked)

    def test_inherited_fd_cannot_borrow_an_unrelated_holders_lock(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "config" / "nacre"
            with writer_lease.writer_lease(root):
                unrelated = os.open(root / ".writer.lock", os.O_RDWR | os.O_CLOEXEC)
                try:
                    with self.assertRaises(writer_lease.WriterLeaseError):
                        with writer_lease.writer_lease(root, inherited_fd=unrelated):
                            self.fail("descriptor without the lock was accepted")
                finally:
                    os.close(unrelated)

    def test_inherited_fd_must_match_the_requested_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            first = base / "first" / "nacre"
            second = base / "second" / "nacre"
            with writer_lease.writer_lease(second):
                pass
            second_lock = second / ".writer.lock"
            before = (second_lock.stat().st_ino, second_lock.read_bytes())
            with writer_lease.writer_lease(first) as lease:
                duplicate = os.dup(lease.fd)
                try:
                    with self.assertRaises(writer_lease.WriterLeaseError):
                        with writer_lease.writer_lease(second, inherited_fd=duplicate):
                            self.fail("a lock fd for another destination was accepted")
                finally:
                    os.close(duplicate)
            self.assertEqual(
                (second_lock.stat().st_ino, second_lock.read_bytes()), before
            )
            self.assertFalse((second / "profile.lua").exists())

    def test_symlink_ancestor_refusal_leaves_foreign_tree_unchanged(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            parent = base / "parent"
            parent.mkdir(mode=0o700)
            foreign = base / "foreign"
            foreign.mkdir(mode=0o700)
            sentinel = foreign / "sentinel"
            sentinel.write_bytes(b"foreign")
            identity = sentinel.stat().st_ino
            (parent / "config").symlink_to(foreign, target_is_directory=True)
            with self.assertRaises(writer_lease.WriterLeaseError):
                with writer_lease.writer_lease(parent / "config" / "nacre"):
                    self.fail("a symlink ancestor was accepted")
            self.assertEqual(sentinel.read_bytes(), b"foreign")
            self.assertEqual(sentinel.stat().st_ino, identity)
            self.assertFalse((foreign / "nacre").exists())

    def test_replaced_destination_path_refusal_does_not_touch_replacement(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            parent = base / "parent"
            parent.mkdir(mode=0o700)
            root = parent / "nacre"
            with writer_lease.writer_lease(root) as lease:
                retired = base / "retired"
                root.rename(retired)
                root.mkdir(mode=0o700)
                sentinel = root / "sentinel"
                sentinel.write_bytes(b"replacement")
                identity = sentinel.stat().st_ino
                with self.assertRaises(writer_lease.WriterLeaseError):
                    lease.publish("profile.lua", b"must not reach replacement")
                self.assertEqual(sentinel.read_bytes(), b"replacement")
                self.assertEqual(sentinel.stat().st_ino, identity)
                self.assertFalse((root / "profile.lua").exists())
                self.assertFalse((retired / "profile.lua").exists())

    def test_ancestry_replacement_never_redirects_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory) / "parent"
            parent.mkdir(mode=0o700)
            root = parent / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                replacement = Path(directory) / "replacement"
                replacement.mkdir(mode=0o700)
                (parent / "config").rename(replacement / "config")
                (parent / "config").mkdir(mode=0o700)
                with self.assertRaises(writer_lease.WriterLeaseError):
                    lease.publish("profile.lua", b"must not redirect")
                self.assertFalse((parent / "config" / "nacre" / "profile.lua").exists())

    def test_changed_ancestry_mode_is_rejected_before_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            parent = Path(directory) / "parent"
            parent.mkdir(mode=0o700)
            root = parent / "config" / "nacre"
            with writer_lease.writer_lease(root) as lease:
                parent.chmod(0o770)
                with self.assertRaises(writer_lease.WriterLeaseError):
                    lease.publish("profile.lua", b"must not write")
                self.assertFalse((root / "profile.lua").exists())


if __name__ == "__main__":
    unittest.main()
