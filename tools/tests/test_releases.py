import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "release", Path(__file__).parents[1] / "releases.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class ReleaseTests(unittest.TestCase):
    def test_failed_release_restores_complete_software_and_preserves_account_state(
        self,
    ):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            repo = home / "repo"
            repo.mkdir()
            source = home / ".local/share/nacre/shell/source"
            source.mkdir(parents=True)
            (source / "shell.qml").write_text("before")
            account = home / ".codex/auth.json"
            account.parent.mkdir()
            account.write_text("fixture never snapshot")
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", home / "releases"),
                patch.object(m.subprocess, "run"),
                patch.object(m, "compositor") as service,
            ):
                release, record = m.capture(repo, "new")
                (source / "shell.qml").write_text("broken")
                self.assertFalse(any(".codex" in e["path"] for e in record["entries"]))
                m.restore(release, record, force=True)
                service.return_value.reload.assert_called_once_with()
                self.assertEqual((source / "shell.qml").read_text(), "before")
                self.assertEqual(account.read_text(), "fixture never snapshot")

    def test_controller_is_part_of_the_snapshot(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            repo = home / "repo"
            repo.mkdir()
            with patch.object(m, "HOME", home):
                owned = m.paths(repo)
            self.assertIn(home / ".local/bin/nacre", owned)
            self.assertIn(home / ".local/share/nacre/control", owned)

    def test_rollback_refuses_later_edits(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            repo = home / "repo"
            repo.mkdir()
            source = home / ".local/bin/nacre-shell"
            source.parent.mkdir(parents=True)
            source.write_text("before")
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", home / "releases"),
            ):
                release, record = m.capture(repo, "new")
                source.write_text("after")
                for e in record["entries"]:
                    e["after"] = m.fingerprint(Path(e["path"]))
                source.write_text("personal edit")
                with self.assertRaisesRegex(RuntimeError, "Later edit preserved"):
                    m.restore(release, record)
                self.assertEqual(source.read_text(), "personal edit")


class RuntimeRetirementTests(unittest.TestCase):
    def test_retirement_preserves_palette_environment_and_is_repeatable(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            base = home / ".local/share/nacre"
            native = base / "palette-runtime/usr/bin/quickshell"
            native.parent.mkdir(parents=True)
            native.write_text("old binary")
            palette = base / "palette-runtime/venv/keep.txt"
            palette.parent.mkdir()
            palette.write_text("palette environment")
            release = home / "release"
            with patch.object(m, "HOME", home):
                retired = m.retire_native_runtimes(release)
                self.assertEqual(len(retired), 1)
                self.assertFalse(native.exists())
                self.assertEqual(palette.read_text(), "palette environment")
                self.assertEqual(
                    Path(retired[0]["backup"]).joinpath("bin/quickshell").read_text(),
                    "old binary",
                )
                self.assertEqual(m.retire_native_runtimes(release), [])


class RetentionTests(unittest.TestCase):
    def test_newest_current_previous_and_retired_are_protected(self):
        import json

        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory)
            for i in range(15):
                p = state / f"release-{i:02}"
                p.mkdir()
                (p / "release.json").write_text(
                    json.dumps({"status": "good", "revision": str(i), "previous": "1"})
                )
            (state / "release-00/retired-native").mkdir()
            (state / "current.json").write_text(
                json.dumps({"release": str(state / "release-02")})
            )
            with patch.object(m, "STATE", state):
                removed = m.prune_releases()
            self.assertEqual(
                {Path(p).name for p in removed}, {"release-03", "release-04"}
            )
            self.assertTrue((state / "release-00").exists())
            self.assertTrue((state / "release-01").exists())
            self.assertTrue((state / "release-02").exists())


class OptionalServiceGateTests(unittest.TestCase):
    def test_stopped_sidebar_is_not_required_or_started_by_desktop_deployment(self):
        active = {"nacre-shell.service", "nacre-brain.service"}
        self.assertNotIn(
            "nacre-sidebar-ai.service",
            m.required_live_services(["configs", "shell", "brain"], active),
        )
        self.assertIn(
            "nacre-sidebar-ai.service",
            m.required_live_services(["shell"], active | {"nacre-sidebar-ai.service"}),
        )
        self.assertIn(
            "nacre-brain.service",
            m.required_live_services(["brain"], set()),
        )


class OrientRuntimeSnapshotTests(unittest.TestCase):
    def test_palette_runtime_link_snapshots_bytes_and_restores_after_target_loss(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            repo = home / "repo"
            repo.mkdir()
            runtime = home / ".local/share/nacre/palette-runtime"
            target = home / "engine"
            (target / "venv").mkdir(parents=True)
            (target / "venv/keep").write_text("known-good-engine")
            runtime.parent.mkdir(parents=True)
            runtime.symlink_to(target)
            with (
                patch.object(m, "HOME", home),
                patch.object(m, "STATE", home / "releases"),
                patch.object(m.subprocess, "run"),
                patch.object(m, "compositor") as service,
            ):
                release, record = m.capture(repo, "orient")
                entry = next(e for e in record["entries"] if e["path"] == str(runtime))
                self.assertEqual(entry["kind"], "runtime")
                (target / "venv/keep").unlink()
                m.restore(release, record, force=True)
                service.return_value.reload.assert_called_once_with()
                self.assertEqual(
                    (runtime / "venv/keep").read_text(), "known-good-engine"
                )
