"""Selector protocol validates private sessions and returns values without execution."""

import importlib.util
import json
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "menu_cli", Path(__file__).parents[1] / "menu-cli.py"
)
cli = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cli)


class MenuCLITests(unittest.TestCase):
    def test_tabbed_choices_preserve_distinct_values(self):
        rows = cli.choices("Same\tone\tFirst\nSame\ttwo\tSecond\n")
        self.assertEqual([row["value"] for row in rows], ["one", "two"])
        self.assertEqual(rows[1]["description"], "Second")

    def test_choice_validation_rejects_unbounded_and_nonstring_values(self):
        for value in (
            [{"label": "x", "value": 42}],
            [{"label": "", "value": "x"}],
            [{"label": "x", "value": "\0"}],
        ):
            with self.assertRaises(ValueError):
                cli.choices(json.dumps(value), True)
        with self.assertRaises(ValueError):
            cli.choices("x" * 1048577)

    def test_arbitrary_paths_are_never_selector_sessions(self):
        for ident in ("../../file", "/tmp/file", "not-a-session", "a" * 31):
            with self.assertRaises(ValueError):
                cli.session_path(ident)

    def test_response_can_only_choose_an_enabled_member(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            ident = "a" * 32
            path = root / ident
            path.mkdir(mode=0o700)
            request = {
                "version": 1,
                "id": ident,
                "kind": "pick",
                "expires": time.time() + 30,
                "choices": [
                    {"label": "One", "value": "one"},
                    {"label": "Two", "value": "two", "disabled": True},
                ],
            }
            cli.atomic(path / "request.json", request)
            with patch.object(cli, "ROOT", root):
                for value in ("missing", "two"):
                    with self.assertRaises(ValueError):
                        cli.respond(ident, {"cancelled": False, "value": value})
                cli.respond(ident, {"cancelled": False, "value": "one"})
                self.assertEqual(
                    json.loads((path / "response.json").read_text())["value"], "one"
                )
                self.assertEqual((path / "response.json").stat().st_mode & 0o777, 0o600)
                cli.respond(ident, {"cancelled": True, "value": ""})
                self.assertTrue(
                    json.loads((path / "response.json").read_text())["cancelled"]
                )

    def test_world_readable_or_symlinked_session_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            ident = "a" * 32
            path = root / ident
            path.mkdir(mode=0o755)
            with patch.object(cli, "ROOT", root):
                with self.assertRaises(ValueError):
                    cli.session_path(ident)
                path.rmdir()
                path.symlink_to(root, target_is_directory=True)
                with self.assertRaises(ValueError):
                    cli.session_path(ident)

    def test_expired_request_is_rejected(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            ident = "a" * 32
            path = root / ident
            path.mkdir(mode=0o700)
            cli.atomic(
                path / "request.json",
                {
                    "version": 1,
                    "id": ident,
                    "kind": "input",
                    "expires": time.time() - 1,
                },
            )
            with patch.object(cli, "ROOT", root):
                with self.assertRaisesRegex(ValueError, "expired"):
                    cli.load_request(ident)
