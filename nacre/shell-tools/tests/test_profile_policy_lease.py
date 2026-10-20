import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


TOOLS = Path(__file__).parents[1]
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location(
    "profile_policy", TOOLS / "profile-policy.py"
)
profile_policy = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(profile_policy)
from writer_lease import Lease, WriterLeaseError, writer_lease


class ProfilePolicyLeaseTests(unittest.TestCase):
    def setUp(self):
        self.selected = profile_policy.selected
        self.lua = profile_policy.lua
        profile_policy.selected = lambda: dict(profile_policy.DEFAULT)
        profile_policy.lua = lambda value: "-- test profile\n"

    def tearDown(self):
        profile_policy.selected = self.selected
        profile_policy.lua = self.lua

    def test_install_publishes_both_artifacts(self):
        with tempfile.TemporaryDirectory() as home:
            value = profile_policy.install(Path(home))
            config = Path(home) / ".config"
            root = config / "nacre"
            self.assertEqual(json.loads((root / "profile.json").read_text()), value)
            self.assertTrue((root / "profile.lua").is_file())
            self.assertEqual(config.stat().st_mode & 0o777, 0o700)
            self.assertEqual(root.stat().st_mode & 0o777, 0o700)
            lock = root / ".writer.lock"
            self.assertEqual(lock.read_bytes(), b"")
            self.assertEqual(lock.stat().st_mode & 0o777, 0o600)
            self.assertEqual((root / "profile.json").stat().st_mode & 0o777, 0o600)
            self.assertEqual((root / "profile.lua").stat().st_mode & 0o777, 0o600)

    def test_install_preserves_existing_safe_directory_modes(self):
        with tempfile.TemporaryDirectory() as home:
            config = Path(home) / ".config"
            root = config / "nacre"
            root.mkdir(mode=0o755, parents=True)
            config.chmod(0o755)
            root.chmod(0o755)

            profile_policy.install(Path(home))

            self.assertEqual(config.stat().st_mode & 0o777, 0o755)
            self.assertEqual(root.stat().st_mode & 0o777, 0o755)
            self.assertEqual((root / ".writer.lock").stat().st_mode & 0o777, 0o600)

    def test_direct_install_contention_refuses_before_profile_read_or_write(self):
        with tempfile.TemporaryDirectory() as home:
            root = Path(home) / ".config/nacre"
            root.mkdir(mode=0o700, parents=True)
            profile = root / "profile.json"
            original = b"foreign profile\n"
            profile.write_bytes(original)
            identity = profile.stat().st_ino
            selected_calls = []
            profile_policy.selected = lambda: selected_calls.append(True)
            with writer_lease(root):
                with self.assertRaises(WriterLeaseError):
                    profile_policy.install(Path(home))
            self.assertEqual(selected_calls, [])
            self.assertEqual(profile.read_bytes(), original)
            self.assertEqual(profile.stat().st_ino, identity)
            self.assertFalse((root / "profile.lua").exists())

    def test_install_reuses_explicit_parent_lease(self):
        with tempfile.TemporaryDirectory() as home:
            root = Path(home) / ".config/nacre"
            with writer_lease(root) as lease:
                value = profile_policy.install(Path(home), lease=lease)
                self.assertEqual(json.loads((root / "profile.json").read_text()), value)
                with self.assertRaises(WriterLeaseError):
                    with writer_lease(root):
                        self.fail("profile installation dropped its parent lease")

    def test_future_profile_is_refused_before_either_publication(self):
        with tempfile.TemporaryDirectory() as home:
            root = Path(home) / ".config/nacre"
            root.mkdir(mode=0o700, parents=True)
            original = b'{"version":99,"id":"future"}\n'
            (root / "profile.json").write_bytes(original)
            with self.assertRaises(profile_policy._state.NewerVersionError):
                profile_policy.install(Path(home))
            self.assertEqual((root / "profile.json").read_bytes(), original)
            self.assertFalse((root / "profile.lua").exists())

    def test_corrupt_profile_is_backed_up_before_replacement(self):
        with tempfile.TemporaryDirectory() as home:
            root = Path(home) / ".config/nacre"
            root.mkdir(mode=0o700, parents=True)
            original = b"not json\n"
            (root / "profile.json").write_bytes(original)
            profile_policy.install(Path(home))
            self.assertEqual((root / "profile.json.corrupt.bak").read_bytes(), original)

    def test_partial_publication_restores_both_original_files(self):
        with tempfile.TemporaryDirectory() as home:
            root = Path(home) / ".config/nacre"
            root.mkdir(mode=0o700, parents=True)
            profile = b'{"version":1,"id":"before"}\n'
            lua = b"before lua\n"
            (root / "profile.json").write_bytes(profile)
            (root / "profile.lua").write_bytes(lua)
            (root / "profile.json").chmod(0o640)
            (root / "profile.lua").chmod(0o644)
            publish = Lease.publish
            failed = False

            def fail_once(lease, name, content, mode=0o600):
                nonlocal failed
                if name == "profile.lua" and not failed:
                    failed = True
                    raise WriterLeaseError("injected publication failure")
                return publish(lease, name, content, mode)

            Lease.publish = fail_once
            try:
                with self.assertRaises(WriterLeaseError):
                    profile_policy.install(Path(home))
            finally:
                Lease.publish = publish
            self.assertEqual((root / "profile.json").read_bytes(), profile)
            self.assertEqual((root / "profile.lua").read_bytes(), lua)
            self.assertEqual((root / "profile.json").stat().st_mode & 0o777, 0o640)
            self.assertEqual((root / "profile.lua").stat().st_mode & 0o777, 0o644)

    def test_partial_publication_preserves_external_json_edit(self):
        with tempfile.TemporaryDirectory() as home:
            root = Path(home) / ".config/nacre"
            root.mkdir(mode=0o700, parents=True)
            original = b'{"version":1,"id":"before"}\n'
            external = b'{"version":1,"id":"external"}\n'
            lua = b"before lua\n"
            (root / "profile.json").write_bytes(original)
            (root / "profile.lua").write_bytes(lua)
            publish = Lease.publish

            def edit_after_json(lease, name, content, mode=0o600):
                if name == "profile.lua":
                    (root / "profile.json").write_bytes(external)
                    (root / "profile.json").chmod(0o600)
                    raise WriterLeaseError("injected Lua publication failure")
                return publish(lease, name, content, mode)

            Lease.publish = edit_after_json
            try:
                with self.assertRaisesRegex(RuntimeError, "restoration incomplete"):
                    profile_policy.install(Path(home))
            finally:
                Lease.publish = publish
            self.assertEqual((root / "profile.json").read_bytes(), external)
            self.assertEqual((root / "profile.lua").read_bytes(), lua)

    def test_unreceipted_post_replace_failure_fails_closed(self):
        with tempfile.TemporaryDirectory() as home:
            root = Path(home) / ".config/nacre"
            root.mkdir(mode=0o700, parents=True)
            original = b'{"version":1,"id":"before"}\n'
            (root / "profile.json").write_bytes(original)
            publish = Lease.publish

            def publish_then_fail(lease, name, content, mode=0o600):
                publish(lease, name, content, mode)
                if name == "profile.json":
                    raise WriterLeaseError("injected post-replace failure")

            Lease.publish = publish_then_fail
            try:
                with self.assertRaisesRegex(RuntimeError, "restoration incomplete"):
                    profile_policy.install(Path(home))
            finally:
                Lease.publish = publish
            self.assertNotEqual((root / "profile.json").read_bytes(), original)
            self.assertFalse((root / "profile.lua").exists())

    def test_identical_external_replacement_has_no_publication_receipt(self):
        with tempfile.TemporaryDirectory() as home:
            root = Path(home) / ".config/nacre"
            root.mkdir(mode=0o700, parents=True)
            original = b'{"version":1,"id":"before"}\n'
            (root / "profile.json").write_bytes(original)
            expected = (json.dumps(profile_policy.DEFAULT, indent=2) + "\n").encode()
            publish = Lease.publish

            def replace_after_json(lease, name, content, mode=0o600):
                publish(lease, name, content, mode)
                if name == "profile.json":
                    replacement = root / "external.next"
                    replacement.write_bytes(expected)
                    replacement.chmod(0o600)
                    replacement.replace(root / "profile.json")
                    raise WriterLeaseError("injected post-publication edit")

            Lease.publish = replace_after_json
            try:
                with self.assertRaisesRegex(RuntimeError, "restoration incomplete"):
                    profile_policy.install(Path(home))
            finally:
                Lease.publish = publish
            self.assertEqual((root / "profile.json").read_bytes(), expected)
            self.assertFalse((root / "profile.lua").exists())


if __name__ == "__main__":
    unittest.main()
