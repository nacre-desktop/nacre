import importlib.util
import json
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
                    extras,
                    "run",
                    return_value=SimpleNamespace(stdout=json.dumps(native)),
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
