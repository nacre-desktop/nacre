"""Every persistent schema: migrations, exact backups, downgrades and recovery."""

import json
from pathlib import Path
import tempfile
import unittest
from concurrent.futures import ThreadPoolExecutor
import nacre_state as state


class StateTests(unittest.TestCase):
    def path_for(self, root, entry):
        pattern = entry["paths"][0].replace("*", "fixture")
        return root / pattern

    def test_every_schema_migrates_once_and_preserves_original_bytes(self):
        with tempfile.TemporaryDirectory() as folder:
            for entry in state.SCHEMAS:
                with self.subTest(schema=entry["id"]):
                    path = self.path_for(Path(folder), entry)
                    path.parent.mkdir(parents=True, exist_ok=True)
                    legacy = [] if entry["shape"] == "array" else {"custom": "kept"}
                    original = json.dumps(legacy, indent=3).encode()
                    path.write_bytes(original)
                    current = state.read(path, entry["defaults"])
                    state.write(path, current)
                    saved = json.loads(path.read_bytes())
                    self.assertEqual(saved["version"], entry["version"])
                    backup = path.with_name(
                        path.name + f".pre-v0-to-v{entry['version']}.bak"
                    )
                    self.assertEqual(backup.read_bytes(), original)
                    self.assertEqual(backup.stat().st_mode & 0o777, 0o600)
                    state.write(path, current)
                    self.assertEqual(backup.read_bytes(), original)
                    if entry["shape"] == "array":
                        self.assertEqual(saved["items"], [])
                    else:
                        self.assertEqual(saved["custom"], "kept")

    def test_every_schema_refuses_newer_versions_on_read_and_write(self):
        with tempfile.TemporaryDirectory() as folder:
            for entry in state.SCHEMAS:
                with self.subTest(schema=entry["id"]):
                    path = self.path_for(Path(folder), entry)
                    path.parent.mkdir(parents=True, exist_ok=True)
                    original = json.dumps(
                        {"version": entry["version"] + 1, "future": "keep"}
                    ).encode()
                    path.write_bytes(original)
                    with self.assertRaises(state.NewerVersionError):
                        state.read(path, entry["defaults"])
                    with self.assertRaises(state.NewerVersionError):
                        state.write(path, entry["defaults"])
                    self.assertEqual(path.read_bytes(), original)
                    self.assertFalse(list(path.parent.glob(path.name + "*.bak")))

    def test_every_schema_recovers_corrupt_bytes_only_after_backup(self):
        with tempfile.TemporaryDirectory() as folder:
            for entry in state.SCHEMAS:
                with self.subTest(schema=entry["id"]):
                    path = self.path_for(Path(folder), entry)
                    path.parent.mkdir(parents=True, exist_ok=True)
                    original = b"{not JSON\xff"
                    path.write_bytes(original)
                    self.assertEqual(
                        state.read(path, entry["defaults"]), entry["defaults"]
                    )
                    self.assertEqual(path.read_bytes(), original)
                    state.write(path, entry["defaults"])
                    self.assertEqual(
                        path.with_name(path.name + ".corrupt.bak").read_bytes(),
                        original,
                    )
                    self.assertEqual(
                        json.loads(path.read_bytes())["version"], entry["version"]
                    )

    def test_backup_failure_and_linked_backup_prevent_overwrite(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "nacre/desktop.json"
            path.parent.mkdir()
            original = b'{"custom":"keep"}'
            path.write_bytes(original)
            backup = path.with_name(path.name + ".pre-v0-to-v2.bak")
            outside = Path(folder) / "outside"
            outside.write_text("untouched")
            backup.symlink_to(outside)
            with self.assertRaises(RuntimeError):
                state.write(path, {})
            self.assertEqual(path.read_bytes(), original)
            self.assertEqual(outside.read_text(), "untouched")

    def test_simultaneous_saves_keep_one_complete_original_backup(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "nacre/desktop.json"
            path.parent.mkdir()
            original = b'{"custom":"original"}'
            path.write_bytes(original)
            with ThreadPoolExecutor(max_workers=2) as pool:
                list(
                    pool.map(
                        lambda index: state.write(path, {"choice": index}), range(10)
                    )
                )
            self.assertEqual(
                path.with_name(path.name + ".pre-v0-to-v2.bak").read_bytes(), original
            )
            self.assertEqual(json.loads(path.read_bytes())["version"], 2)

    def test_desktop_edges_split_without_losing_profile_snapshot_or_new_keys(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "nacre/desktop.json"
            path.parent.mkdir()
            original = b'{"version":1,"rightEdge":false,"frameRightEnabled":true,"topEdge":false,"leftDrawer":false,"unknown":"keep","normalSnapshot":{"rightEdge":false,"topEdge":true}}'
            path.write_bytes(original)
            current = state.read(path, {})
            self.assertTrue(current["frameRightEnabled"])
            self.assertFalse(current["controlCenterEnabled"])
            self.assertFalse(current["topBarEnabled"])
            self.assertFalse(current["frameTopEnabled"])
            self.assertFalse(current["aiSidebarEnabled"])
            self.assertTrue(current["dashboardEnabled"])
            self.assertFalse(current["normalSnapshot"]["controlCenterEnabled"])
            self.assertTrue(current["normalSnapshot"]["topBarEnabled"])
            self.assertEqual(current["unknown"], "keep")
            self.assertNotIn("rightEdge", current)
            self.assertEqual(path.read_bytes(), original)
            state.write(path, current)
            self.assertEqual(
                path.with_name(path.name + ".pre-v1-to-v2.bak").read_bytes(), original
            )
            self.assertEqual(json.loads(path.read_bytes())["version"], 2)

    def test_control_center_v1_migration_keeps_optional_order(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "nacre/control-center.json"
            path.parent.mkdir()
            original = b'{"version":1,"optionalTiles":["vpn","mic"]}'
            path.write_bytes(original)
            migrated = state.read_json_file(path)
            self.assertEqual(migrated["tiles"][-2:], ["vpn", "mic"])
            state.write(path, migrated)
            self.assertEqual(json.loads(path.read_bytes())["version"], 2)
            self.assertEqual(
                path.with_name(path.name + ".pre-v1-to-v2.bak").read_bytes(), original
            )

    def test_path_override_uses_explicit_schema_and_refuses_downgrade(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "custom-preferences.json"
            state.write(path, {"accounts": {}}, schema_id="ai-settings")
            self.assertEqual(json.loads(path.read_bytes())["version"], 1)
            original = b'{"version":99,"future":"keep"}'
            path.write_bytes(original)
            with self.assertRaises(state.NewerVersionError):
                state.read_json_file(path, "ai-settings")
            with self.assertRaises(state.NewerVersionError):
                state.write(path, {}, schema_id="ai-settings")
            self.assertEqual(path.read_bytes(), original)

    def test_unsupported_version_metadata_is_never_recovered_as_corruption(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "nacre/desktop.json"
            path.parent.mkdir()
            for version in ["next", None, -1, 1.5]:
                original = json.dumps({"version": version, "future": "keep"}).encode()
                path.write_bytes(original)
                with self.assertRaises(state.InvalidVersionError):
                    state.write(path, {})
                self.assertEqual(path.read_bytes(), original)
