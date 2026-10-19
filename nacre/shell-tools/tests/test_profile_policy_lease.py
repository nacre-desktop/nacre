import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest


TOOLS = Path(__file__).parents[1]
sys.path.insert(0, str(TOOLS))
SPEC = importlib.util.spec_from_file_location("profile_policy", TOOLS / "profile-policy.py")
profile_policy = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(profile_policy)
from writer_lease import Lease, WriterLeaseError


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
            root = Path(home) / ".config/nacre"
            self.assertEqual(json.loads((root / "profile.json").read_text()), value)
            self.assertTrue((root / "profile.lua").is_file())
            self.assertEqual((root / "profile.json").stat().st_mode & 0o777, 0o600)
            self.assertEqual((root / "profile.lua").stat().st_mode & 0o777, 0o600)

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


if __name__ == "__main__":
    unittest.main()
