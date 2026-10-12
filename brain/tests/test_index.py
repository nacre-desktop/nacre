import importlib.util
from pathlib import Path
import tempfile
import unittest
import json
import os
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "observatory", Path(__file__).parents[1] / "control.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class IndexTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.patcher = patch.object(module, "VAULT", self.root)
        self.patcher.start()
        self.semantic = patch.dict(os.environ, {"NACRE_BRAIN_SEMANTICS": "0"})
        self.semantic.start()

    def tearDown(self):
        self.patcher.stop()
        self.semantic.stop()
        self.tmp.cleanup()

    def note(self, path, body):
        p = self.root / path
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_text(body)
        return p

    def test_paths_cannot_escape_vault(self):
        with self.assertRaises(ValueError):
            module.note_path("../outside.md")
        self.note("inside.md", "# Test")
        (self.root / "escape.md").symlink_to("/etc/passwd")
        with self.assertRaises(ValueError):
            module.note_path("escape.md")
        self.assertEqual(module.graph()["noteCount"], 1)

    def test_hidden_and_instruction_files_excluded(self):
        self.note(".brain-state/private.md", "# Hidden")
        self.note("AGENTS.md", "# Instructions")
        self.note("personal/one.md", "# A personal interest\nRecorded: 2026-10-02")
        self.assertEqual(module.graph()["noteCount"], 1)

    def test_explicit_links_remain_distinct_from_grouping(self):
        self.note(
            "wiki/example-project.md",
            "# Example Project\n[Evidence](../projects/camera.md)",
        )
        self.note(
            "projects/camera.md",
            "# Camera controller\nRecorded: 2026-10-02\nExample Project camera",
        )
        graph = module.graph()
        sources = [e for e in graph["links"] if e["kind"] == "source"]
        self.assertEqual(len(sources), 1)
        self.assertTrue(any(e["kind"] == "group" for e in graph["links"]))
        ids = {n["id"] for n in graph["nodes"]}
        self.assertTrue(
            all(e["source"] in ids and e["target"] in ids for e in graph["links"])
        )

    def test_wiki_references_resolve_aliases_and_deduplicate_markdown_links(self):
        self.note(
            "wiki/start.md",
            "# Start\n[[wiki/target|Target page]]\n[[wiki/target#Details]]\n[Target](target.md)",
        )
        self.note("wiki/target.md", "# Target\nContent")
        edges = [e for e in module.graph()["links"] if e["kind"] == "source"]
        self.assertEqual(len(edges), 1)

    def test_ambiguous_wiki_basenames_do_not_invent_connections(self):
        self.note("wiki/start.md", "# Start\n[[target]]")
        self.note("projects/target.md", "# Target A")
        self.note("personal/target.md", "# Target B")
        self.assertEqual(
            [e for e in module.graph()["links"] if e["kind"] == "source"], []
        )

    def test_grouping_override_changes_only_derived_state_and_rejects_escape(self):
        p = self.note(
            "inbox/result.md",
            "# A result\nRecorded: 2026-10-03\nWorlds: Original\nGrip training",
        )
        before = p.read_bytes()
        module.action(
            "assign-note",
            json.dumps({"path": "inbox/result.md", "subjects": ["Climbing"]}),
        )
        note = next(n for n in module.graph()["nodes"] if n["kind"] == "note")
        self.assertEqual(note["memberships"][0]["subject"], "climbing")
        self.assertEqual(p.read_bytes(), before)
        cache = self.root / ".brain-state/discovery/overrides.json"
        self.assertEqual(cache.stat().st_mode & 0o777, 0o600)
        with self.assertRaises(ValueError):
            module.action(
                "assign-note", json.dumps({"path": "../outside.md", "subjects": ["X"]})
            )
        module.action(
            "assign-note", json.dumps({"path": "inbox/result.md", "reset": True})
        )
        note = next(n for n in module.graph()["nodes"] if n["kind"] == "note")
        self.assertEqual(note["memberships"][0]["subject"], "original")

    def test_full_text_search_excludes_hidden_instructions_and_external_symlinks(self):
        self.note("projects/one.md", "# Ordinary title\nA uniquely searchable detail")
        self.note(".private/one.md", "# uniquely searchable")
        self.note("AGENTS.md", "# uniquely searchable")
        (self.root / "outside.md").symlink_to("/etc/passwd")
        self.assertEqual(len(module.search_notes("uniquely searchable")), 1)
        self.assertEqual(module.search_notes("root:x"), [])

    def test_recorded_time_is_normalized_and_revision_tracks_content_not_activity(self):
        p = self.note(
            "projects/one.md",
            "# One\nRecorded: 2026-10-03T18:00:00+02:00\nExample Project camera",
        )
        first = next(n for n in module.graph()["nodes"] if n["kind"] == "note")
        self.assertEqual(first["recorded"], "2026-10-03T16:00:00+00:00")
        p.write_text(p.read_text() + "\nExtra evidence")
        second = next(n for n in module.graph()["nodes"] if n["kind"] == "note")
        self.assertNotEqual(first["revision"], second["revision"])
        self.assertEqual(first["activity"], second["activity"])

    def test_old_notes_stay_present_but_have_less_recent_weight(self):
        self.note("personal/old.md", "# Old\nRecorded: 2020-01-01\nPersonal")
        self.note("personal/new.md", "# New\nRecorded: 2026-10-02\nPersonal")
        notes = {n["label"]: n for n in module.graph()["nodes"] if n["kind"] == "note"}
        self.assertLess(notes["Old"]["activity"], notes["New"]["activity"])
        self.assertEqual(len(notes), 2)

    def test_file_modification_does_not_manufacture_activity(self):
        self.note("personal/undated.md", "# Undated personal note")
        note = next(n for n in module.graph()["nodes"] if n["kind"] == "note")
        self.assertEqual(note["activity"], 0)

    def test_unknown_actions_and_window_injection_rejected(self):
        with self.assertRaises(ValueError):
            module.action("shell", "echo anything")
        with self.assertRaises(ValueError):
            module.action("focus", '"; injected')

    def test_new_registered_project_and_annotated_topic_appear(self):
        # A note annotation creates the subject without consulting a coding registry.
        self.note(
            "projects/relay.md",
            "# First circuit\nProject: relay\nTopics: Soldering\nRecorded: 2026-10-02\nSoldering a circuit",
        )
        nodes = {n["id"]: n for n in module.graph()["nodes"]}
        self.assertEqual(nodes["relay"]["label"], "Relay")
        self.assertEqual(nodes["relay:tag-soldering"]["category"], "electronics")

    def test_declared_world_and_semi_topic_need_no_registry_entry(self):
        self.note(
            "wiki/photography.md",
            "# Photography\nEntity: world\nName: Photography\nCategory: research",
        )
        self.note(
            "wiki/lighting.md",
            "# Lighting\nEntity: topic\nParent: photography\nCategory: electronics\nLighting sensor circuit",
        )
        nodes = {n["id"]: n for n in module.graph()["nodes"]}
        self.assertEqual(nodes["photography"]["themeReason"], "declared")
        self.assertEqual(
            nodes["photography:entity-lighting"]["category"], "electronics"
        )

    def test_examples_in_code_and_raw_imports_cannot_declare_worlds(self):
        self.note(
            "wiki/example.md", "# Schema\n```\nEntity: world\nName: Imaginary\n```"
        )
        self.note("raw/import.md", "# Import\nEntity: world\nName: Untrusted")
        labels = {n["label"] for n in module.graph()["nodes"] if n["kind"] == "hub"}
        self.assertNotIn("Imaginary", labels)
        self.assertNotIn("Untrusted", labels)

    def test_subject_colors_differ_without_using_confidence(self):
        self.note(
            "projects/one.md",
            "# AI inference\nRecorded: 2026-10-02\nConfidence: reported",
        )
        self.note(
            "projects/two.md",
            "# Circuit soldering\nRecorded: 2026-10-02\nConfidence: verified",
        )
        notes = [n for n in module.graph()["nodes"] if n["kind"] == "note"]
        self.assertNotEqual(notes[0]["color"], notes[1]["color"])

    def test_desktop_actions_do_not_belong_to_brain(self):
        with self.assertRaisesRegex(ValueError, "Unknown Brain action"):
            module.action("updates")

    def test_persistent_brain_reuses_window_without_focus_or_duplicates(self):
        with (
            patch.object(module, "STATE", self.root / "state"),
            patch.object(module, "ensure_server"),
            patch.object(module, "preferred_workspace", return_value=6),
            patch.object(module, "bootstrap_existing_browser"),
            patch.object(module, "brain_window", return_value={"address": "0x123"}),
            patch.object(module, "launch") as launch,
            patch.object(module, "action") as action,
        ):
            module.ensure_brain(focus=False)
            launch.assert_not_called()
            action.assert_not_called()

    def test_background_brain_launch_restores_workspace_and_routes_to_six(self):
        browser = self.root / "chromium"
        browser.write_text("fixture; browser execution is mocked")
        with (
            patch.object(module.shutil, "which", return_value=str(browser)),
            patch.object(module, "STATE", self.root / "state"),
            patch.object(module, "ensure_server"),
            patch.object(module, "preferred_workspace", return_value=6),
            patch.object(module, "bootstrap_existing_browser"),
            patch.object(
                module, "brain_window", side_effect=[None, {"address": "0x123"}]
            ),
            patch.object(module, "brain_browser_running", return_value=False),
            patch.object(module, "compositor") as service,
            patch.object(module, "launch") as launch,
        ):
            service.return_value.query.return_value = {"id": 2}
            module.ensure_brain(focus=False)
            launch.assert_called_once()
            service.return_value.fullscreen_state.assert_called_once_with(
                "0x123", internal=0, client=0, timeout=3
            )
            service.return_value.move_window.assert_called_once_with(
                "0x123", workspace="6", follow=False, timeout=3
            )
            service.return_value.switch_workspace.assert_called_once_with(
                2, current_monitor=True, timeout=3
            )


if __name__ == "__main__":
    unittest.main()


class ConversationEntities(unittest.TestCase):
    setUp = IndexTests.setUp
    tearDown = IndexTests.tearDown
    note = IndexTests.note

    def test_nonproject_world_annotations_create_shared_connections(self):
        self.note(
            "wiki/entity-climbing.md",
            "# Climbing\nEntity: world\nName: Climbing\nReviewed: 2026-10-03\nStatus: Current\nSource: User",
        )
        self.note(
            "wiki/entity-electronics.md",
            "# Electronics\nEntity: world\nName: Electronics\nReviewed: 2026-10-03\nStatus: Current\nSource: User",
        )
        self.note(
            "inbox/related.md",
            "# A useful skill\nWorlds: Climbing, Electronics\nTopics: Learning\nRecorded: 2026-10-03\nNo project ID is needed.",
        )
        graph = module.graph()
        hubs = [n for n in graph["nodes"] if n["kind"] == "hub"]
        self.assertTrue(any(n["label"] == "Climbing" and n["count"] >= 1 for n in hubs))
        self.assertTrue(
            any(n["label"] == "Electronics" and n["count"] >= 1 for n in hubs)
        )
        topics = [
            n
            for n in graph["nodes"]
            if n["kind"] == "topic" and n["label"] == "Learning"
        ]
        self.assertEqual(len(topics), 2)
        note = next(n for n in graph["nodes"] if n["label"] == "A useful skill")
        self.assertEqual(
            sum(
                e["target"] == note["id"] and e["source"] in [n["id"] for n in topics]
                for e in graph["links"]
            ),
            2,
        )


class BrainStartupTests(unittest.TestCase):
    def test_loading_window_is_reused_and_workspace_six_is_preferred(self):
        scoped = patch.object(module, "preferred_workspace", return_value=6)
        scoped.start()
        self.addCleanup(scoped.stop)
        windows = [
            {
                "address": "0x111",
                "pid": 123,
                "class": "chrome-127.0.0.1__-Default",
                "title": "127.0.0.1_/",
                "workspace": {"id": 3},
            },
            {
                "address": "0x222",
                "pid": 123,
                "class": "chrome-127.0.0.1__-Default",
                "title": "127.0.0.1_/",
                "workspace": {"id": 6},
            },
        ]
        with (
            patch.object(module, "browser_profile_pids", return_value={123}),
            patch.object(module, "compositor") as service,
        ):
            service.return_value.query.return_value = windows
            self.assertEqual(module.brain_window()["address"], "0x222")
        with (
            tempfile.TemporaryDirectory() as tmp,
            patch.object(module, "STATE", Path(tmp)),
            patch.object(module, "ensure_server"),
            patch.object(module, "preferred_workspace", return_value=6),
            patch.object(module, "bootstrap_existing_browser"),
            patch.object(module, "browser_profile_pids", return_value={123}),
            patch.object(module, "compositor") as service,
            patch.object(module, "launch") as launch,
        ):
            service.return_value.query.return_value = windows
            for _ in range(3):
                module.ensure_brain()
            launch.assert_not_called()

    def test_unavailable_compositor_never_launches_a_browser(self):
        for response in ("", "broken", "{}"):
            with (
                tempfile.TemporaryDirectory() as tmp,
                patch.object(module, "STATE", Path(tmp)),
                patch.object(module, "ensure_server"),
                patch.object(module, "preferred_workspace", return_value=6),
                patch.object(module, "bootstrap_existing_browser"),
                patch.object(module, "browser_profile_pids", return_value={123}),
                patch.object(module, "compositor") as service,
                patch.object(module, "launch") as launch,
            ):
                if response in ("", "broken"):
                    service.return_value.query.side_effect = ValueError(
                        "Invalid compositor response"
                    )
                else:
                    service.return_value.query.return_value = {}
                with self.assertRaises(RuntimeError):
                    module.ensure_brain()
                launch.assert_not_called()

    def test_wallet_blocked_browser_does_not_get_more_launch_requests(self):
        with (
            tempfile.TemporaryDirectory() as tmp,
            patch.object(module, "STATE", Path(tmp)),
            patch.object(module, "ensure_server"),
            patch.object(module, "preferred_workspace", return_value=6),
            patch.object(module, "bootstrap_existing_browser"),
            patch.object(module, "browser_profile_pids", return_value={123}),
            patch.object(module, "brain_window", return_value=None),
            patch.object(module, "brain_browser_running", return_value=True),
            patch.object(module, "launch") as launch,
        ):
            for _ in range(10):
                module.ensure_brain(focus=False)
            launch.assert_not_called()


class BrainCompositorFailureTests(unittest.TestCase):
    def test_housekeeping_uses_common_facade_and_keeps_transient_failure_policy(self):
        with patch.object(module, "compositor") as service:
            service.return_value.fullscreen_state.side_effect = OSError("offline")
            self.assertIsNone(
                module.compositor_action(
                    "fullscreen_state", "0x123", internal=0, client=0
                )
            )
            service.return_value.fullscreen_state.assert_called_once_with(
                "0x123", internal=0, client=0, timeout=3
            )
