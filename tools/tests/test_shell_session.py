"""Exercise graphical environment and alias routes without user configuration."""

import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]


class GraphicalEnvironmentTests(unittest.TestCase):
    def test_real_posix_shell_exports_toolkit_and_shared_cursor_contract(self):
        result = subprocess.check_output(
            [
                "sh",
                "-c",
                '. "$1"; . "$2"; env',
                "probe",
                str(ROOT / "uwsm/env"),
                str(ROOT / "uwsm/env-hyprland"),
            ],
            env={"PATH": os.defpath},
            text=True,
        )
        exported = dict(
            line.split("=", 1) for line in result.splitlines() if "=" in line
        )
        expected = {
            "QT_QPA_PLATFORM": "wayland;xcb",
            "QT_QPA_PLATFORMTHEME": "qt6ct",
            "QT_WAYLAND_DISABLE_WINDOWDECORATION": "1",
            "GDK_BACKEND": "wayland,x11,*",
            "GDK_SCALE": "1",
            "OZONE_PLATFORM": "wayland",
            "ELECTRON_OZONE_PLATFORM_HINT": "wayland",
            "XCURSOR_THEME": "breeze_cursors",
            "XCURSOR_SIZE": "24",
            "HYPRCURSOR_SIZE": "24",
        }
        self.assertEqual({name: exported.get(name) for name in expected}, expected)
        for retired in [
            "XDG_CURRENT_DESKTOP",
            "XDG_SESSION_TYPE",
            "XDG_SESSION_DESKTOP",
            "MOZ_ENABLE_WAYLAND",
            "CLUTTER_BACKEND",
            "QT_AUTO_SCREEN_SCALE_FACTOR",
            "SDL_VIDEODRIVER",
            "LD_LIBRARY_PATH",
            "QT_PLUGIN_PATH",
            "QML_IMPORT_PATH",
        ]:
            self.assertNotIn(retired, exported)


@unittest.skipUnless(shutil.which("fish"), "Fish unavailable")
class FishAliasTests(unittest.TestCase):
    def test_aliases_forward_arguments_without_launching_during_startup(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            binary = home / "bin"
            binary.mkdir()
            calls = home / "calls"
            for name in ["nacre-shell", "figlet"]:
                fake = binary / name
                fake.write_text(
                    "#!/bin/sh\n"
                    f"printf '%s\\n' '__{name}' >> \"$HOME/calls\"\n"
                    'printf \'[%s]\\n\' "$@" >> "$HOME/calls"\n'
                )
                fake.chmod(0o755)
            env = {
                "HOME": str(home),
                "PATH": str(binary) + ":" + os.defpath,
                "XDG_CONFIG_HOME": str(home / "config"),
                "XDG_DATA_HOME": str(home / "data"),
                "TERM": "xterm-256color",
            }
            for interactive in [False, True]:
                with self.subTest(interactive=interactive):
                    calls.unlink(missing_ok=True)
                    command = [shutil.which("fish"), "--no-config"]
                    if interactive:
                        command.append("-i")
                    for script in [
                        "source $argv[1]; functions -q nacre-update ascii",
                        "source $argv[1]; nacre-update $argv[2]; ascii $argv[2]",
                    ]:
                        subprocess.run(
                            [
                                *command,
                                "-c",
                                script,
                                str(ROOT / "fish/conf.d/90-nacre.fish"),
                                "sample text",
                            ],
                            env=env,
                            text=True,
                            capture_output=True,
                            check=True,
                        )
                        if "functions -q" in script:
                            self.assertFalse(calls.exists())
                    self.assertEqual(
                        calls.read_text(),
                        "__nacre-shell\n[updates]\n[sample text]\n__figlet\n[sample text]\n",
                    )
