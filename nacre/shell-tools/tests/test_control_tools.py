"""Optional control actions respect external ownership and publish after success."""

import importlib.util
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "control_tools", Path(__file__).resolve().parents[1] / "control-tools.py"
)
tools = importlib.util.module_from_spec(spec)
spec.loader.exec_module(tools)


original_socket_probe = tools.unmanaged_socket


class ControlToolsTests(unittest.TestCase):
    def setUp(self):
        # Fixtures never discover sockets in the user's real display namespace.
        self.enterContext(
            patch.dict(
                tools.os.environ,
                {"HYPRLAND_INSTANCE_SIGNATURE": "", "XDG_RUNTIME_DIR": ""},
            )
        )

    def test_dead_socket_does_not_lock_out_control_but_live_listener_does(self):
        import socket
        import os

        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "hypr/test/.hyprsunset.sock"
            path.parent.mkdir(parents=True)
            with patch.dict(
                os.environ,
                {"XDG_RUNTIME_DIR": folder, "HYPRLAND_INSTANCE_SIGNATURE": "test"},
            ):
                with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as server:
                    server.bind(str(path))
                    server.listen(1)
                    self.assertTrue(original_socket_probe(False))
                self.assertTrue(path.exists())
                self.assertFalse(original_socket_probe(False))

    def test_new_owned_service_retries_initial_ipc_readiness(self):
        with (
            patch.object(
                tools.subprocess,
                "run",
                side_effect=[subprocess.CalledProcessError(1, ["hyprctl"]), None],
            ) as run,
            patch.object(tools, "service", return_value=(True, "owned")),
            patch.object(tools.time, "sleep"),
        ):
            tools.apply_temperature(
                ["hyprctl", "hyprsunset", "temperature", "4500"], "owned", True
            )
            self.assertEqual(run.call_count, 2)

    def test_state_discovery_is_readonly_and_external_owner_is_visible(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(tools.shutil, "which", return_value="/usr/bin/tool"),
            patch.object(tools, "service", return_value=(True, 123)),
            patch.object(tools.subprocess, "run") as run,
        ):
            home = Path(folder)
            status = tools.state(home)
            self.assertTrue(status["nightLightExternal"])
            self.assertFalse(status["nightLightEnabled"])
            self.assertFalse(tools.record_path(home).exists())
            run.assert_not_called()
            with self.assertRaisesRegex(ValueError, "externally"):
                tools.night_light(True, home)
            run.assert_not_called()

    def test_night_light_claims_only_service_it_started_then_can_disable(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(tools.shutil, "which", return_value="/usr/bin/tool"),
            patch.object(
                tools,
                "service",
                side_effect=[
                    (False, 0),
                    (True, 321),
                    (True, 321),
                    (True, 321),
                    (True, 321),
                ],
            ),
            patch.object(tools.subprocess, "run") as run,
        ):
            home = Path(folder)
            self.assertTrue(tools.night_light(True, home)["nightLightEnabled"])
            self.assertEqual(
                run.call_args_list[0].args[0],
                ["systemctl", "--user", "start", "hyprsunset.service"],
            )
            self.assertEqual(
                run.call_args_list[1].args[0],
                ["hyprctl", "hyprsunset", "temperature", "4500"],
            )
            self.assertEqual(tools.record_path(home).stat().st_mode & 0o777, 0o600)
            self.assertFalse(tools.night_light(False, home)["nightLightEnabled"])
            self.assertEqual(
                run.call_args.args[0], ["hyprctl", "hyprsunset", "identity"]
            )

    def test_private_schedule_and_unmanaged_socket_are_never_taken_over(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(tools.shutil, "which", return_value="/usr/bin/tool"),
            patch.object(tools, "service", return_value=(False, "")),
            patch.object(tools.subprocess, "run") as run,
        ):
            home = Path(folder)
            config = home / ".config/hypr/hyprsunset.conf"
            config.parent.mkdir(parents=True)
            config.write_text("profile { identity = true }\n")
            self.assertTrue(tools.state(home)["nightLightExternal"])
            with self.assertRaises(ValueError):
                tools.night_light(True, home)
            run.assert_not_called()
            config.unlink()
            with patch.object(tools, "unmanaged_socket", return_value=True):
                with self.assertRaises(ValueError):
                    tools.night_light(True, home)
                run.assert_not_called()

    def test_failed_temperature_request_never_publishes_enabled(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(tools.shutil, "which", return_value="/usr/bin/tool"),
            patch.object(
                tools, "service", side_effect=[(False, 0)] + [(True, 321)] * 20
            ),
            patch.object(
                tools.subprocess,
                "run",
                side_effect=lambda command, **kw: (
                    (_ for _ in ()).throw(subprocess.CalledProcessError(1, command))
                    if command[0] == "hyprctl"
                    else None
                ),
            ),
        ):
            home = Path(folder)
            with (
                patch.object(tools.time, "sleep"),
                self.assertRaises(subprocess.CalledProcessError),
            ):
                tools.night_light(True, home)
            self.assertFalse(tools.load(home)["enabled"])
