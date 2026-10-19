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
