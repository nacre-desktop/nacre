import importlib.machinery
import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


def load(name):
    loader = importlib.machinery.SourceFileLoader(name, str(ROOT / "bin" / name))
    spec = importlib.util.spec_from_loader(loader.name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


sync = load("nacre-brain-sync")
maintenance = load("nacre-brain-maintain")


class SyncTests(unittest.TestCase):
    def test_one_sided_edit_propagates_and_old_version_restores(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "vault"
            root.mkdir()
            sync.merge(root, {"wiki/project.md": "old"}, {})
            archive = sync.backup(root)
            sync.merge(root, {"wiki/project.md": "new"}, {"wiki/project.md": "old"})
            self.assertEqual(sync.snapshot(root)["wiki/project.md"], "new")
            dest = Path(tmp) / "restore"
            maintenance.restore(archive, dest)
            self.assertEqual(sync.snapshot(dest)["wiki/project.md"], "old")

    def test_divergent_edits_preserve_both_and_baseline_remote_edit(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sync.merge(root, {"wiki/p.md": "local"}, {})
            changed, conflicts = sync.merge(
                root, {"wiki/p.md": "remote"}, {"wiki/p.md": "base"}
            )
            self.assertEqual(conflicts, ["wiki/p.md"])
            self.assertEqual(changed, [])
            self.assertEqual(sync.snapshot(root)["wiki/p.md"], "local")
            self.assertEqual(
                len(list((root / ".brain-state/conflicts").glob("*.json"))), 1
            )
            self.assertEqual(
                sync.merge(root, {"wiki/p.md": "base"}, {"wiki/p.md": "base"}), ([], [])
            )

    def test_deletion_not_propagated_and_extended_formats_sync(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            notes = {
                "wiki/views/current.base": "views: []",
                "wiki/map.canvas": "{}",
                "raw/article.txt": "source",
                "AGENTS.md": "rules",
            }
            sync.merge(root, notes, {})
            sync.merge(root, {}, notes)
            self.assertEqual(sync.snapshot(root), notes)

    def test_all_input_validated_before_writes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for name, body in [
                ("../outside.md", "x"),
                (".codex/auth.json", "x"),
                ("inbox/private.md", "password: secret"),
            ]:
                with self.assertRaises(ValueError):
                    sync.merge(root, {"inbox/first.md": "ok", name: body}, {})
                self.assertFalse((root / "inbox/first.md").exists())

    def test_symlink_rejected(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "vault"
            root.mkdir()
            outside = Path(tmp) / "outside"
            outside.mkdir()
            (root / "wiki").symlink_to(outside)
            with self.assertRaises(ValueError):
                sync.merge(root, {"wiki/page.md": "x"}, {})

    def test_conflict_reply_does_not_overwrite_local(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sync.merge(root, {"wiki/p.md": "mine"}, {})
            sync.merge(
                root, {"wiki/p.md": "theirs"}, {"wiki/p.md": "mine"}, ["wiki/p.md"]
            )
            self.assertEqual(sync.snapshot(root)["wiki/p.md"], "mine")

    def test_restore_rejects_existing_destination(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive = sync.backup(root)
            with self.assertRaises(ValueError):
                maintenance.restore(archive, root)

    def test_check_missing_sources_and_links(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sync.merge(root, {"wiki/p.md": "[[missing]] [broken](absent.md)"}, {})
            found = maintenance.check(root)
            self.assertTrue(any("Source:" in x for x in found))
            self.assertTrue(any("wiki link" in x for x in found))
            self.assertTrue(any("relative link" in x for x in found))


class CheckpointTests(unittest.TestCase):
    def test_dated_checkpoint_uses_evidence_metadata(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            sync.merge(
                root,
                {
                    "checkpoints/task.md": "# Task\nRecorded: 2026-10-01\nConfidence: verified\nSource: test\nCompleted task."
                },
                {},
            )
            self.assertEqual(maintenance.check(root), [])


class SnapshotEnumerationTests(unittest.TestCase):
    def test_pending_and_unmanaged_files_do_not_block_managed_notes(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "inbox").mkdir()
            (root / "raw").mkdir()
            (root / "inbox/.note-pending").write_text("partial")
            (root / "raw/article.pdf").write_bytes(b"pdf")
            (root / "inbox/note.md").write_text("published")
            self.assertEqual(sync.snapshot(root), {"inbox/note.md": "published"})


class HealthTests(unittest.TestCase):
    def test_network_failure_is_distinct_from_authentication_and_protocol(self):
        self.assertEqual(sync.exchange_error(255, "Connection timed out")[0], "offline")
        self.assertEqual(
            sync.exchange_error(255, "Permission denied (publickey)")[0],
            "authentication",
        )
        self.assertEqual(sync.exchange_error(127, "not found")[0], "protocol")

    def test_backoff_preserves_last_success(self):
        from unittest.mock import patch

        with tempfile.TemporaryDirectory() as directory:
            status = Path(directory) / "status.json"
            with (
                patch.object(sync, "STATUS", status),
                patch.object(sync.time, "time", return_value=1000),
            ):
                sync.report_status("success", "done")
                first = sync.report_status("offline", "unavailable")
                second = sync.report_status("offline", "unavailable")
            self.assertEqual(second["lastSuccess"], 1000)
            self.assertGreater(second["nextRetry"], first["nextRetry"])
