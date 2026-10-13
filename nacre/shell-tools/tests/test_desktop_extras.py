import importlib.util
import json
import copy
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from types import SimpleNamespace

spec = importlib.util.spec_from_file_location(
    "extras", Path(__file__).resolve().parents[1] / "desktop-extras.py"
)
extras = importlib.util.module_from_spec(spec)
spec.loader.exec_module(extras)


class DesktopExtrasTests(unittest.TestCase):
    def test_actual_english_and_pseudo_bindings_preserve_dispatch_and_external_labels(
        self,
    ):
        records = [
            dict(
                keys="SUPER + SPACE",
                submap="",
                action=dict(method="exec_cmd", args=["nacre-menu apps"]),
                flags=dict(description="Nacre:launcher"),
            ),
            dict(
                keys="ESCAPE",
                submap="resize",
                action=dict(method="submap", args=["reset"]),
                flags={},
            ),
            dict(
                keys="CTRL + ALT + F12",
                submap="",
                action=dict(method="exec_cmd", args=["public-TEST_APP --flag"]),
                flags={},
            ),
            dict(
                keys="XF86MonBrightnessUp",
                submap="",
                action=dict(method="exec_cmd", args=["nacre-shell brightness +"]),
                flags={},
            ),
        ]
        native = [
            dict(key="space", modmask=64, dispatcher="__lua", arg="", submap=""),
            dict(
                key="ESCAPE",
                modmask=0,
                dispatcher="__lua",
                arg="reset",
                submap="resize",
            ),
            dict(key="F12", modmask=12, dispatcher="__lua", arg="", submap=""),
            dict(
                key="XF86MonBrightnessUp",
                modmask=0,
                dispatcher="__lua",
                arg="",
                submap="",
            ),
            dict(
                key="L",
                modmask=64,
                dispatcher="exec",
                arg="loginctl lock-session",
                submap="",
            ),
            dict(key="Q", modmask=64, dispatcher="exec", arg="", submap=""),
            dict(key="0", modmask=64, dispatcher="workspace", arg="10", submap=""),
            dict(
                key="0", modmask=65, dispatcher="movetoworkspace", arg="10", submap=""
            ),
        ]
        original = copy.deepcopy((records, native))
        results = []
        for language in ("en", "pseudo"):
            with (
                patch.dict("os.environ", {"NACRE_LANGUAGE": language}),
                patch.object(
                    extras.compositor,
                    "binding_source_files",
                    return_value=[Path("public-fixture.lua")],
                ),
                patch.object(
                    extras.compositor, "capture_bindings", return_value=records
                ),
                patch.object(extras.compositor, "query", return_value=native),
            ):
                results.append(
                    {(row["submap"], row["key"]): row for row in extras.bindings()}
                )
        english, pseudo = results
        expected = {
            ("", "SUPER+SPACE"): "Find an application",
            ("resize", "ESCAPE"): "resize · Exit resize mode",
            ("", "CTRL+ALT+F12"): "public TEST APP",
            ("", "XF86MonBrightnessUp"): "Screen brightness +",
            ("", "SUPER+L"): "Lock screen",
            ("", "SUPER+Q"): "Close (undo with Super+Shift+Q within 30 seconds)",
            ("", "SUPER+0"): "Go to workspace 10",
            ("", "SUPER+SHIFT+0"): "Move window to workspace 10",
        }
        self.assertEqual(
            {key: row["description"] for key, row in english.items()}, expected
        )
        for key in english:
            self.assertEqual(
                {
                    field: value
                    for field, value in english[key].items()
                    if field != "description"
                },
                {
                    field: value
                    for field, value in pseudo[key].items()
                    if field != "description"
                },
            )
            if key == ("", "CTRL+ALT+F12"):
                self.assertEqual(
                    pseudo[key]["description"], english[key]["description"]
                )
            else:
                self.assertNotEqual(
                    pseudo[key]["description"], english[key]["description"]
                )
                self.assertIn("[", pseudo[key]["description"])
        self.assertEqual(pseudo[("", "SUPER+SPACE")]["menu"], "apps")
        self.assertIn("Super+Shift+Q", pseudo[("", "SUPER+Q")]["description"])
        self.assertTrue(
            pseudo[("resize", "ESCAPE")]["description"].startswith("resize · [")
        )
        self.assertEqual((records, native), original)

    def test_brain_search_excludes_hidden_files_and_symlinks_and_requires_all_terms(
        self,
    ):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            vault = home / "Documents/Nacre-Brain"
            vault.mkdir(parents=True)
            (vault / "camera.md").write_text(
                "# Camera controller\nFrontend camera controls\n"
            )
            (vault / "other.md").write_text("# Camera\nHardware\n")
            (vault / ".private").mkdir()
            (vault / ".private/hidden.md").write_text("camera frontend")
            (vault / "alias.md").symlink_to(vault / "camera.md")
            with (
                patch.object(extras, "HOME", home),
                patch.dict("os.environ", {"NACRE_BRAIN": str(vault)}),
            ):
                results = extras.brain_search("camera frontend")
                self.assertEqual([r["path"] for r in results], ["camera.md"])
                self.assertEqual(extras.brain_search(""), [])

    def test_note_open_rejects_paths_outside_the_vault(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            vault = home / "Documents/Nacre-Brain"
            vault.mkdir(parents=True)
            (home / "external.md").write_text("# Outside")
            (vault / "escape.md").symlink_to(home / "external.md")
            with (
                patch.object(extras, "HOME", home),
                patch.object(extras.subprocess, "Popen") as launch,
            ):
                for path in ("../../external.md", "escape.md"):
                    with self.assertRaises(ValueError):
                        extras.open_note(path)
                launch.assert_not_called()

    def test_pinned_clipboard_entries_remain_visible_beyond_recent_limit(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(extras, "STATE", Path(folder)),
        ):
            (Path(folder) / "pins.json").write_text('["200"]')
            rows = "\n".join(f"{i}\ttext {i}" for i in range(201)).encode()
            with patch.object(extras, "run", return_value=SimpleNamespace(stdout=rows)):
                result = extras.clips()
                self.assertEqual(result[0]["id"], "200")
                self.assertTrue(result[0]["pinned"])
                self.assertEqual(len(result), 161)

    def test_clipboard_copy_preserves_image_mime_type(self):
        data = b"\x89PNG\r\n\x1a\nfixture"
        with patch.object(
            extras,
            "run",
            side_effect=[
                SimpleNamespace(stdout=b"42\timage"),
                SimpleNamespace(stdout=data),
                SimpleNamespace(stdout=b""),
            ],
        ) as run:
            extras.clip_action("copy", "42")
            self.assertEqual(run.call_args.args[0], ["wl-copy", "--type", "image/png"])
            self.assertEqual(run.call_args.kwargs["input"], data)
        self.assertEqual(extras.mime(b"hello"), "text/plain;charset=utf-8")

    def test_clipboard_pin_stores_only_fingerprints_and_can_be_reversed(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(extras, "STATE", Path(folder)),
            patch.object(
                extras, "run", return_value=SimpleNamespace(stdout=b"7\tprivate text")
            ),
        ):
            extras.clip_action("pin", "7")
            pins = json.loads((Path(folder) / "pins.json").read_text())
            self.assertEqual(len(pins), 1)
            self.assertNotIn("private text", (Path(folder) / "pins.json").read_text())
            self.assertEqual((Path(folder) / "pins.json").stat().st_mode & 0o777, 0o600)
            extras.clip_action("pin", "7")
            self.assertEqual(json.loads((Path(folder) / "pins.json").read_text()), [])

    def test_rebound_actions_and_tenth_workspace_have_correct_labels(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            folder = home / ".config/hypr/conf"
            folder.mkdir(parents=True)
            (folder / "map.lua").write_text(
                'hl.bind("SUPER + A",hl.dsp.exec_cmd("~/.local/bin/nacre-shell launcher"),{description="Nacre:launcher"})\nhl.bind("SUPER + F1",hl.dsp.exec_cmd("~/.local/bin/nacre-menu learn.shortcuts"),{description="Nacre:shortcuts"})'
            )
            native = [
                dict(key="A", modmask=64, dispatcher="__lua"),
                dict(key="F1", modmask=64, dispatcher="__lua"),
                dict(key="0", modmask=64, dispatcher="__lua"),
            ]
            with (
                patch.object(extras, "HOME", home),
                patch.object(extras.compositor, "query", return_value=native),
            ):
                rows = extras.bindings()
            labels = {row["key"]: row for row in rows}
            self.assertEqual(labels["SUPER+A"]["description"], "Find an application")
            help_row = labels["SUPER+F1"]
            self.assertEqual(help_row["menu"], "learn.shortcuts")
            self.assertEqual(help_row["description"], "Keyboard shortcuts")
            self.assertEqual(labels["SUPER+0"]["description"], "Go to workspace 10")

    def test_generated_shortcut_guide_keeps_actions_and_submap_labels(self):
        import shutil

        if not shutil.which("lua"):
            self.skipTest("Lua unavailable")
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            folder = home / ".config/hypr/conf"
            folder.mkdir(parents=True)
            (folder / "map.lua").write_text("""
for n=1,2 do hl.bind("SUPER + "..n,hl.dsp.focus({workspace=n})) end
hl.bind("SUPER + K",hl.dsp.exec_cmd("~/.local/bin/nacre-menu learn.shortcuts"))
hl.bind("XF86MonBrightnessDown",hl.dsp.exec_cmd("~/.local/bin/nacre-shell brightness down"))
hl.define_submap("resize",function() hl.bind("SUPER + R",hl.dsp.submap("reset")) end)
""")
            native = [
                dict(key="1", modmask=64, submap="", dispatcher="__lua"),
                dict(key="K", modmask=64, submap="", dispatcher="__lua", arg="17"),
                dict(
                    key="XF86MonBrightnessDown",
                    modmask=0,
                    submap="",
                    dispatcher="__lua",
                ),
                dict(key="R", modmask=64, submap="resize", dispatcher="__lua"),
            ]
            with (
                patch.object(extras, "HOME", home),
                patch.object(
                    extras.compositor,
                    "query",
                    return_value=native,
                ),
            ):
                result = extras.bindings()
            self.assertEqual(
                next(r["menu"] for r in result if r["key"] == "SUPER+K"),
                "learn.shortcuts",
            )
            descriptions = {(r["submap"], r["key"]): r["description"] for r in result}
            self.assertEqual(descriptions[("", "SUPER+1")], "Go to workspace 1")
            self.assertEqual(
                descriptions[("", "XF86MonBrightnessDown")], "Screen brightness down"
            )
            self.assertEqual(
                descriptions[("resize", "SUPER+R")], "resize · Exit resize mode"
            )


if __name__ == "__main__":
    unittest.main()
