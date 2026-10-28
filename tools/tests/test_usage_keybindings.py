"""User docs must follow supported bindings, not stale hand-written tables."""

import contextlib
import copy
import importlib.util
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "usage_keybindings", ROOT / "tools/generate_usage_keybindings.py"
)
docs = importlib.util.module_from_spec(spec)
spec.loader.exec_module(docs)


class UsageKeybindingsTests(unittest.TestCase):
    def test_native_ui_transport_keeps_existing_labels(self):
        for command, text in (
            ("nacrectl ui nacre hide", "Hide or show the shell frame and panels"),
            ("nacrectl ui drawers toggle dashboard", "Toggle the main panel"),
            ("nacrectl ui nacre menu actions.capture ''", "Capture menu"),
        ):
            self.assertEqual(
                docs.label(
                    {
                        "keys": "SUPER + Z",
                        "action": "application.launch",
                        "parameters": {"command": command},
                    },
                    "niri",
                ),
                text,
            )

    def test_backend_differences_and_opt_in(self):
        text = docs.generate(docs.load())
        hyprland, rest = text.split("## Niri shortcuts")
        niri, personal = rest.split("## Personal trash shortcuts")
        self.assertIn("Super + Q | Close the window", hyprland)
        self.assertIn("Super + Q | Close the window", niri)
        self.assertIn("Toggle scratchpad", hyprland)
        self.assertNotIn("Toggle scratchpad", niri)
        self.assertNotIn("resize mode", niri)
        self.assertIn("screenshot picker", niri)
        self.assertNotIn("Put away the window", hyprland + niri)
        self.assertEqual(personal.count("(opt-in) |"), 4)
        self.assertIn("Super + Ctrl + Q", personal)

    def test_widgets_shortcut_is_labelled_in_the_generated_shortcut_list(self):
        text = docs.generate(docs.load())
        self.assertIn("Super + Shift + W | Widgets", text)

    def test_contract_change_changes_docs(self):
        contract = copy.deepcopy(docs.load())
        for rows in contract["outputs"].values():
            for row in rows:
                if row.get("action") == "window.close":
                    row["keys"] = "SUPER + F12"
        self.assertNotEqual(docs.generate(contract), docs.generate(docs.load()))
        self.assertIn("Super + F12 | Close the window", docs.generate(contract))

    def test_unknown_action_is_not_silently_documented(self):
        with self.assertRaisesRegex(ValueError, "user-facing label"):
            docs.label({"keys": "SUPER + F12", "action": "new.action"}, "hyprland")

    def test_workspace_compaction_keeps_changed_targets_visible(self):
        contract = copy.deepcopy(docs.load())
        for rows in contract["outputs"].values():
            for row in rows:
                if (
                    row.get("action") == "workspace.switch"
                    and row["keys"] == "SUPER + 1"
                ):
                    row["parameters"]["workspace"]["number"] = 2
        text = docs.generate(contract)
        self.assertIn("Super + 1 / Super + 2 | Switch to workspace 2", text)
        self.assertNotIn("Switch to workspace 1–10", text)

    def test_block_replacement_preserves_prose_and_requires_markers(self):
        source = "Before\n" + docs.START + "\nstale\n" + docs.END + "\nAfter\n"
        self.assertEqual(docs.replace_block(source, "fresh"), "Before\nfresh\nAfter\n")
        for source in ("no markers", docs.START + docs.START + docs.END):
            with self.assertRaisesRegex(ValueError, "exactly one"):
                docs.replace_block(source, "fresh")

    def test_check_fails_on_drift_without_writing(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "docs").mkdir()
            path = root / "docs/usage.md"
            source = docs.START + "\nstale\n" + docs.END
            path.write_text(source)
            with (
                patch.object(docs, "ROOT", root),
                patch.object(sys, "argv", ["generator", "--check"]),
                contextlib.redirect_stderr(io.StringIO()),
                self.assertRaises(SystemExit) as result,
            ):
                docs.main()
            self.assertEqual(result.exception.code, 1)
            self.assertEqual(path.read_text(), source)


if __name__ == "__main__":
    unittest.main()
