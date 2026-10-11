from pathlib import Path
import shlex
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
class SessionCursorTests(unittest.TestCase):
    def command(self, theme, size):
        source = """
local theme, size, path = arg[1], arg[2], arg[3]
local fired = false
os.getenv = function(name)
    if name == "XCURSOR_THEME" then return theme end
    if name == "XCURSOR_SIZE" then return size end
end
hl = {
    on = function(event, callback)
        assert(event == "hyprland.start")
        assert(not fired)
        fired = true
        callback()
    end,
    exec_cmd = function(command) io.write(command) end,
}
dofile(path)
assert(fired)
"""
        result = subprocess.run(
            ["lua", "-", theme, size, str(ROOT / "hypr/conf/cursor.lua")],
            input=source,
            capture_output=True,
            text=True,
            check=True,
        )
        return shlex.split(result.stdout)

    def test_session_theme_remains_one_argument(self):
        for theme in (
            "breeze_cursors",
            "My Cursor Theme",
            "a'b",
            "name$(echo unexpected)",
        ):
            self.assertEqual(
                self.command(theme, "24"), ["hyprctl", "setcursor", theme, "24"]
            )

    def test_invalid_preferences_do_not_spawn_or_invent_defaults(self):
        for theme, size in (
            ("", "24"),
            ("breeze_cursors", ""),
            ("breeze_cursors", "no"),
            ("breeze_cursors", "24.5"),
            ("breeze_cursors", "0"),
            ("breeze_cursors", "-1"),
            ("breeze_cursors", "1e100"),
        ):
            self.assertEqual(self.command(theme, size), [])
