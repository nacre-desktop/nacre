from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


class PrivateLuaTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
    def test_broken_desktop_does_not_block_later_overrides(self):
        source = (Path(__file__).parents[2] / "hypr/hyprland.lua").read_text()
        source = source[source.index("local function load_private") :]
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            private = home / ".config/nacre"
            private.mkdir(parents=True)
            (private / "desktop.lua").write_text('error("broken setting")')
            (private / "shortcuts.lua").write_text('table.insert(loaded, "shortcuts")')
            (private / "host.lua").write_text('table.insert(loaded, "host")')
            script = home / "probe.lua"
            script.write_text(
                "loaded = {}; failures = 0; hl = {notification = {create = function() failures = failures + 1 end}}\n"
                + source
                + '\nassert(failures == 1); assert(table.concat(loaded, ",") == "shortcuts,host")'
            )
            import os

            subprocess.run(
                ["lua", str(script)], env=dict(os.environ, HOME=str(home)), check=True
            )
