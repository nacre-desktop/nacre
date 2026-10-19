import fcntl
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest



MODULE = Path(__file__).parents[1] / "writer_lease.py"
SPEC = importlib.util.spec_from_file_location("writer_lease", MODULE)
writer_lease = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(writer_lease)


class WriterLeaseTests(unittest.TestCase):
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
                        self.fail("closing inherited duplicate released the parent's lock")

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
