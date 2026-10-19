"""Desktop terminals escape the Niri code overlay through the user manager."""

import importlib.util
from importlib.machinery import SourceFileLoader
import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


class TerminalRouteTests(unittest.TestCase):
    def setUp(self):
        loader = SourceFileLoader("terminal_app_route", str(ROOT / "bin/nacre-app"))
        spec = importlib.util.spec_from_loader(loader.name, loader)
        self.app = importlib.util.module_from_spec(spec)
        loader.exec_module(self.app)

    def test_restriction_detection_is_scoped_to_niri(self):
        for compositor, uid_map, status, expected in (
            ("niri", "1000 1000 1", "NoNewPrivs:\t1", True),
            ("niri", "0 0 4294967295", "NoNewPrivs:\t1", True),
            ("niri", "0 0 4294967295", "NoNewPrivs:\t0", False),
            ("hyprland", "1000 1000 1", "NoNewPrivs:\t1", False),
            ("", "1000 1000 1", "NoNewPrivs:\t1", False),
        ):
            with (
                self.subTest(compositor=compositor, uid_map=uid_map, status=status),
                patch.dict(os.environ, {"NACRE_COMPOSITOR": compositor}, clear=True),
                patch.object(Path, "read_text", side_effect=[uid_map, status]),
            ):
                self.assertEqual(self.app.restricted_niri_session(), expected)

    def test_unreadable_proc_does_not_assume_niri_overlay(self):
        with (
            patch.dict(os.environ, {"NACRE_COMPOSITOR": "niri"}, clear=True),
            patch.object(Path, "read_text", side_effect=OSError("unavailable")),
        ):
            self.assertFalse(self.app.restricted_niri_session())

    def test_host_service_preserves_argv_connections_and_cwd(self):
        environment = {
            "PATH": "/overlay/bin:/usr/bin",
            "WAYLAND_DISPLAY": "wayland-1",
            "DISPLAY": ":1",
            "NIRI_SOCKET": "/run/user/1000/niri.sock",
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=/run/user/1000/bus",
            "NACRE_COMPOSITOR": "niri",
            "SSH_AUTH_SOCK": "/run/user/1000/ssh-agent",
        }
        argv = ["custom-terminal", "--title", "With spaces", "-e", "echo", "$HOME"]
        with (
            patch.dict(os.environ, environment, clear=True),
            patch.object(self.app.shutil, "which", return_value="/usr/bin/systemd-run"),
            patch.object(Path, "cwd", return_value=Path("/home/test/a directory")),
            patch.object(self.app.subprocess, "run") as run,
        ):
            run.return_value.returncode = 0
            self.app.host_terminal(argv)
        command = run.call_args.args[0]
        self.assertEqual(command[0], "/usr/bin/systemd-run")
        for option in (
            "--user",
            "--wait",
            "--collect",
            "--expand-environment=no",
            "--property=Type=exec",
        ):
            self.assertIn(option, command)
        self.assertNotIn("--scope", command)
        self.assertIn("--working-directory=/home/test/a directory", command)
        for name, value in environment.items():
            if name != "PATH":
                self.assertIn(f"--setenv={name}={value}", command)
        self.assertFalse(any(part.startswith("--setenv=PATH=") for part in command))
        self.assertEqual(
            command[command.index("--") :], ["--", "/usr/bin/env", "--", *argv]
        )

    def test_bridge_failure_is_clear_and_never_falls_back(self):
        for result in (
            subprocess.CompletedProcess([], 5),
            OSError("manager unavailable"),
        ):
            with (
                self.subTest(result=result),
                patch.object(
                    self.app.shutil, "which", return_value="/usr/bin/systemd-run"
                ),
                patch.object(self.app.subprocess, "run") as run,
                patch.object(self.app.os, "execvp") as execute,
            ):
                if isinstance(result, OSError):
                    run.side_effect = result
                else:
                    run.return_value = result
                with self.assertRaisesRegex(
                    SystemExit, "Cannot open a normal terminal"
                ):
                    self.app.host_terminal(["kitty"])
                execute.assert_not_called()

    def test_missing_bridge_is_clear(self):
        with patch.object(self.app.shutil, "which", return_value=None):
            with self.assertRaisesRegex(SystemExit, "systemd-run is unavailable"):
                self.app.host_terminal(["kitty"])

    def test_routes_preserve_custom_arguments_and_only_bridge_terminal(self):
        for role, restricted in (
            ("terminal", True),
            ("terminal", False),
            ("browser", True),
        ):
            with (
                self.subTest(role=role, restricted=restricted),
                tempfile.TemporaryDirectory() as directory,
            ):
                home = Path(directory)
                config = home / ".config/nacre/apps.json"
                config.parent.mkdir(parents=True)
                config.write_text(
                    json.dumps({role: ["custom-app", "--title", "A title"]})
                )
                with (
                    patch.object(Path, "home", return_value=home),
                    patch.object(
                        self.app.sys, "argv", ["route", role, "literal $HOME"]
                    ),
                    patch.object(
                        self.app.shutil, "which", return_value="/usr/bin/custom-app"
                    ),
                    patch.object(
                        self.app, "restricted_niri_session", return_value=restricted
                    ),
                    patch.object(self.app, "host_terminal") as bridge,
                    patch.object(self.app.os, "execvp") as execute,
                ):
                    self.app.main()
                argv = ["custom-app", "--title", "A title", "literal $HOME"]
                if role == "terminal" and restricted:
                    bridge.assert_called_once_with(argv)
                    execute.assert_not_called()
                else:
                    execute.assert_called_once_with(argv[0], argv)
                    bridge.assert_not_called()

    def test_default_kitty_config_is_preserved_through_bridge(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            config = home / ".config/nacre/terminal/kitty.conf"
            config.parent.mkdir(parents=True)
            config.write_text("font_size 11\n")
            with (
                patch.object(Path, "home", return_value=home),
                patch.object(
                    self.app.sys, "argv", ["route", "terminal", "--title", "Kitty"]
                ),
                patch.object(self.app.shutil, "which", return_value="/usr/bin/kitty"),
                patch.object(self.app, "restricted_niri_session", return_value=True),
                patch.object(self.app, "host_terminal") as bridge,
            ):
                self.app.main()
            bridge.assert_called_once_with(
                ["kitty", "--config", str(config), "--title", "Kitty"]
            )


if __name__ == "__main__":
    unittest.main()
