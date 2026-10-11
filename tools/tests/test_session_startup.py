from pathlib import Path
import shlex
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[2]


@unittest.skipUnless(shutil.which("lua"), "Lua unavailable")
class SessionStartupTests(unittest.TestCase):
    def commands(self, home):
        source = """
local callback = nil
os.getenv = function(name)
    assert(name == "HOME")
    return arg[1]
end
hl = {
    on = function(event, action)
        assert(event == "hyprland.start" and callback == nil)
        callback = action
    end,
    exec_cmd = function(command) error("Unexpected launch on load: " .. command) end,
}
dofile(arg[2])
assert(callback ~= nil)
-- Loading/verification/reload merely registers the handler. Execute it with a
-- safe command capture owner only; no process or service operation is performed.
hl.exec_cmd = function(command) io.write(command .. "\\n") end
callback()
"""
        result = subprocess.run(
            ["lua", "-", home, str(ROOT / "hypr/conf/autostart.lua")],
            input=source,
            capture_output=True,
            text=True,
            check=True,
        )
        return [shlex.split(line) for line in result.stdout.splitlines()]

    def test_startup_order_and_ownership(self):
        self.assertEqual(
            self.commands("/home/example"),
            [
                ["uwsm", "app", "-t", "scope", "--", "/usr/lib/pam_kwallet_init"],
                [
                    "uwsm",
                    "app",
                    "--",
                    "/usr/lib/polkit-gnome/polkit-gnome-authentication-agent-1",
                ],
                ["uwsm", "app", "--", "wl-paste", "--watch", "cliphist", "store"],
                [
                    "uwsm",
                    "app",
                    "--",
                    "/home/example/.config/hypr/scripts/startup-apps.sh",
                ],
                [
                    "uwsm",
                    "app",
                    "--",
                    "/home/example/.local/bin/nacre-welcome",
                    "--login",
                ],
            ],
        )

    def test_home_path_is_one_literal_argument(self):
        for home in ("/home/space name", "/home/quote'home", "/home/$(not-executed)"):
            commands = self.commands(home)
            self.assertEqual(len(commands), 5)
            self.assertEqual(
                commands[-2][-1], home + "/.config/hypr/scripts/startup-apps.sh"
            )
            self.assertEqual(len(commands[-2]), 4)
            self.assertEqual(
                commands[-1][-2:], [home + "/.local/bin/nacre-welcome", "--login"]
            )
