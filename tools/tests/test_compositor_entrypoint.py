"""Execute the compositor entry point with isolated dependency/private owners."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
MODULES = [
    "monitor",
    "cursor",
    "cursor-behavior",
    "keyboard",
    "autostart",
    "window",
    "decoration",
    "layout",
    "misc",
    "keybinding",
    "windowrule",
    "animation",
    "nacre",
    "brain",
]
PRIVATE = ["monitor", "palette", "desktop", "shortcuts", "host"]


@unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
class CompositorEntrypointTests(unittest.TestCase):
    def execute(self, files):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            private = home / ".config/nacre"
            private.mkdir(parents=True)
            for name, body in files.items():
                (private / f"{name}.lua").write_text(body)
            script = home / "probe.lua"
            script.write_text(
                "modules = {}; loaded = {}; failures = {}\n"
                "require = function(name)\n"
                "  table.insert(modules, name)\n"
                "  if name == 'conf.window' then\n"
                "    assert(var_primary == 'rgba(808080ff)')\n"
                "    assert(var_on_primary == 'rgba(f0f0f0ff)')\n"
                "  end\n"
                "end\n"
                "hl = { notification = { create = function(item)\n"
                "  assert(item.icon == 'error' and item.timeout == 15000)\n"
                "  table.insert(failures, item.text)\n"
                "end }}\n"
                f"dofile({json.dumps(str(ROOT / 'hypr/hyprland.lua'))})\n"
                "print(table.concat(modules, ','))\n"
                "print(table.concat(loaded, ','))\n"
                "print(#failures)\n"
                "for _, message in ipairs(failures) do print(message) end\n"
            )
            return subprocess.check_output(
                ["lua", str(script)],
                text=True,
                env=dict(os.environ, HOME=str(home)),
            ).splitlines()

    def test_missing_private_files_still_load_all_managed_modules(self):
        lines = self.execute({})
        self.assertEqual(lines[:3], [",".join(f"conf.{n}" for n in MODULES), "", "0"])

    def test_loader_markers_remain_available_to_managed_helpers(self):
        source = (ROOT / "hypr/hyprland.lua").read_text()
        for label in [
            "committed wallpaper palette",
            "desktop settings",
            "native desktop shortcuts",
        ]:
            self.assertIn(f"-- Nacre {label}", source)

    def test_private_order_and_last_override_wins(self):
        files = {
            name: f"table.insert(loaded, '{name}'); setting = '{name}'"
            for name in PRIVATE
        }
        files["host"] += "; assert(setting == 'host'); assert(#modules == 14)"
        lines = self.execute(files)
        self.assertEqual(lines[1:3], [",".join(PRIVATE), "0"])

    def test_every_broken_private_file_is_contained(self):
        for broken in PRIVATE:
            with self.subTest(broken=broken):
                files = {name: f"table.insert(loaded, '{name}')" for name in PRIVATE}
                files[broken] = "error('isolated failure')"
                lines = self.execute(files)
                self.assertEqual(lines[1], ",".join(n for n in PRIVATE if n != broken))
                self.assertEqual(lines[2], "1")
                self.assertIn(f"Nacre: {broken} failed:", lines[3])
                self.assertIn("isolated failure", lines[3])
