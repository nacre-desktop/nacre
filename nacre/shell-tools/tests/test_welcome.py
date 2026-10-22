"""Welcome preference ownership, login deduplication and single-shell activation."""

import importlib.machinery
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from concurrent.futures import ThreadPoolExecutor

ROOT = Path(__file__).parents[3]


def load(name, path):
    loader = importlib.machinery.SourceFileLoader(name, str(path))
    spec = importlib.util.spec_from_loader(name, loader)
    module = importlib.util.module_from_spec(spec)
    loader.exec_module(module)
    return module


welcome = load("welcome_owner", ROOT / "nacre/shell-tools/welcome.py")
configure = load("welcome_configure", ROOT / "tools/configure.py")


class WelcomeTests(unittest.TestCase):
    def test_disabled_login_and_manual_reads_do_not_reenable(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            self.assertTrue(welcome.preferences(home)["showAtLogin"])
            self.assertFalse(welcome.paths(home)[0].exists())
            welcome.set_startup(home, False)
            before = welcome.paths(home)[0].read_bytes()
            self.assertFalse(welcome.claim_login(home, "session-1")["show"])
            self.assertFalse(welcome.state(home)["preferences"]["showAtLogin"])
            self.assertEqual(before, welcome.paths(home)[0].read_bytes())
            self.assertEqual(welcome.paths(home)[0].stat().st_mode & 0o777, 0o600)

    def test_claim_once_per_session_and_failed_activation_retry(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            first = welcome.claim_login(home, "one")
            self.assertTrue(first["show"])
            self.assertFalse(welcome.claim_login(home, "one")["show"])
            welcome.release_login(home, "one", "other-claim")
            self.assertFalse(welcome.claim_login(home, "one")["show"])
            welcome.release_login(home, "one", first["claim"])
            self.assertTrue(welcome.claim_login(home, "one")["show"])
            self.assertTrue(welcome.claim_login(home, "two")["show"])
            self.assertFalse(welcome.claim_login(home, "")["show"])

    def test_first_run_is_consumed_only_after_success_and_stale_claim_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            first = welcome.claim_login(home, "one")
            with self.assertRaises(ValueError):
                welcome.acknowledge_login(home, "one", "not-ours")
            self.assertNotIn("firstRunShown", welcome.preferences(home))
            welcome.acknowledge_login(home, "one", first["claim"])
            self.assertTrue(welcome.preferences(home)["firstRunShown"])
            self.assertFalse(welcome.claim_login(home, "two")["show"])
            self.assertFalse(welcome.claim_login(home, "one")["show"])

    def test_completed_manual_tour_also_suppresses_automatic_startup(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            welcome.atomic(
                welcome.paths(home)[0], {"version": 1, "tourCompleted": True}
            )
            self.assertFalse(welcome.claim_login(home, "next-login")["show"])

    def test_unknown_fields_preserved_and_bad_configuration_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            path, _ = welcome.paths(home)
            path.parent.mkdir(parents=True)
            path.write_text(json.dumps({"version": 1, "custom": "preserve"}))
            welcome.set_startup(home, False)
            self.assertEqual(welcome.preferences(home)["custom"], "preserve")
            for text in ('{"showAtLogin":"no"}', '{"version":2}'):
                path.write_text(text)
                with self.assertRaises((ValueError, RuntimeError)):
                    welcome.set_startup(home, True)
                self.assertEqual(path.read_text(), text)
            path.write_text("broken json")
            welcome.set_startup(home, True)
            self.assertEqual(
                path.with_name(path.name + ".corrupt.bak").read_text(), "broken json"
            )
            self.assertTrue(welcome.preferences(home)["showAtLogin"])
            with self.assertRaises(ValueError):
                welcome.set_startup(home, "false")

    def test_concurrent_login_claims_activate_only_once(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            with ThreadPoolExecutor(max_workers=4) as pool:
                results = list(
                    pool.map(lambda _: welcome.claim_login(home, "same"), range(4))
                )
            self.assertEqual(sum(item["show"] for item in results), 1)

    def test_live_shortcuts_follow_remaps_and_omit_removed_actions(self):
        bindings = [
            {"key": "E", "modmask": 68, "description": "Nacre:files", "submap": ""},
            {"key": "F", "modmask": 64, "description": "", "arg": "private command"},
            {
                "key": "R",
                "modmask": 64,
                "description": "Nacre:files",
                "submap": "resize",
            },
        ]
        rows = welcome.shortcut_rows(bindings)
        self.assertEqual(len(rows), 1)
        self.assertEqual(rows[0]["key"], "Super + Ctrl + E")
        self.assertNotIn("private", json.dumps(rows))
        self.assertEqual(welcome.shortcut_rows([]), [])

    def test_pseudo_preserves_grouping_and_keys_with_translatable_title_metadata(self):
        with patch.dict(welcome.os.environ, {"NACRE_LANGUAGE": "pseudo"}):
            rows = welcome.shortcut_rows(
                [{"key": "3", "modmask": 64, "description": "Nacre:workspace"}]
            )
        self.assertEqual(rows[0]["group"], "Workspaces and focus")
        self.assertEqual(rows[0]["key"], "Super + 3")
        self.assertEqual(rows[0]["titleId"], "welcome.shortcut.workspace.title")

    def test_all_current_shortcuts_have_actual_catalog_titles_and_stable_keys(self):
        bindings = [
            dict(key="F" + str(index + 1), modmask=64, description="Nacre:" + action)
            for index, (action, _, _) in enumerate(welcome.SHORTCUTS)
        ]
        canonical_titles = {action: title for action, title, _ in welcome.SHORTCUTS}
        results = []
        for language in ("en", "pseudo"):
            with patch.dict(welcome.os.environ, {"NACRE_LANGUAGE": language}):
                rows = welcome.shortcut_rows(bindings)
                self.assertEqual({row["id"] for row in rows}, set(canonical_titles))
                for row in rows:
                    caption = welcome.tr(row["titleId"], "wrong fallback")
                    self.assertEqual(row["title"], canonical_titles[row["id"]])
                    if language == "en":
                        self.assertEqual(caption, canonical_titles[row["id"]])
                    else:
                        self.assertTrue(caption.startswith("["), caption)
                results.append(rows)
        self.assertEqual(results[0], results[1])
        self.assertEqual(canonical_titles["put-away"], "Close with undo")

    def test_live_shortcut_ranges_aliases_and_related_keys(self):
        binds = [
            {"key": str(n), "modmask": 64, "description": "Nacre:workspace"}
            for n in range(2, 6)
        ]
        binds += [
            {"key": str(n), "modmask": 65, "description": "Nacre:move-workspace"}
            for n in range(2, 6)
        ]
        binds += [
            {"key": key, "modmask": 64, "description": "Nacre:focus"}
            for key in ["left", "right", "up", "down"]
        ]
        binds += [
            {"key": "Q", "modmask": 64, "description": "Nacre:put-away"},
            {"key": "X", "modmask": 65, "description": "Nacre:restore"},
            {"key": "C", "modmask": 68, "description": "Nacre:close"},
        ]
        rows = {row["id"]: row for row in welcome.shortcut_rows(binds)}
        self.assertEqual(rows["workspace"]["key"], "Super + 2–5")
        self.assertIn("Super + Shift + 2–5", rows["workspace"]["detail"])
        self.assertEqual(rows["focus"]["key"], "Super + arrows")
        self.assertIn("Super + Shift + X", rows["put-away"]["detail"])
        self.assertNotIn("Shift + Q", rows["put-away"]["detail"])
        binds.pop(0)
        self.assertEqual(welcome.binding_labels(binds)["workspace"], "Super + 3–5")
        with self.assertRaises(ValueError):
            welcome.shortcut_rows({})

    def test_no_compositor_has_an_explicit_unavailable_state(self):
        with patch.object(welcome.compositor, "query", side_effect=FileNotFoundError()):
            rows, error = welcome.live_shortcuts()
        self.assertEqual(rows, [])
        self.assertIn("Hyprland", error)

    def test_tenth_workspace_has_a_readable_label(self):
        native = [
            dict(key=str(n), modmask=64, description="Nacre:workspace")
            for n in range(10)
        ]
        rows = {row["id"]: row for row in welcome.shortcut_rows(native)}
        self.assertEqual(rows["workspace"]["key"], "Super + 1–9, 0 (workspace 10)")

    def test_deployment_and_login_owner_are_declared(self):
        mapping = configure.files(ROOT)
        runtime = load("welcome_package_runtime", ROOT / "tools/package_runtime.py")
        links = runtime.links(ROOT, {"components": []})
        self.assertEqual(
            links[Path(".local/bin/nacre-welcome")],
            Path("../share/nacre/shell/bin/nacre-welcome"),
        )
        self.assertEqual(
            links[Path(".local/share/nacre/shell/bin/nacre-welcome")],
            Path("../tools/nacre-helper"),
        )
        self.assertIn(Path(".local/share/applications/nacre-welcome.desktop"), mapping)
        entry = (ROOT / "nacre/desktop/nacre-welcome.desktop").read_text()
        self.assertIn("Icon=nacre\n", entry)
        self.assertIn("Terminal=false", entry)
        startup = (ROOT / "hypr/conf/autostart.lua").read_text()
        self.assertEqual(startup.count('"--login"'), 1)
        self.assertNotIn("--login", (ROOT / "nacre/shell/shell.qml").read_text())
