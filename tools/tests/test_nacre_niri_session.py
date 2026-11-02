"""Nacre's fork session uses the existing generated configuration safely."""

import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location(
    "package_niri_session", ROOT / "tools/package_niri.py"
)
package_niri = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package_niri)
session_spec = importlib.util.spec_from_file_location(
    "package_sessions_under_test", ROOT / "tools/package_sessions.py"
)
package_sessions = importlib.util.module_from_spec(session_spec)
session_spec.loader.exec_module(package_sessions)


class NacreNiriSessionTests(unittest.TestCase):
    def test_prepared_wrapper_keeps_fork_selected_for_offline_queries(self):
        wrapper = (ROOT / "uwsm/niri-session-bin/niri").read_text()
        self.assertIn('if [ "${NACRE_NIRI_FORK:-0}" = 1 ]', wrapper)
        self.assertIn("fork=/usr/bin/nacre-niri", wrapper)
        self.assertIn('exec "$fork" "$@"', wrapper)
        self.assertIn(
            "fork was selected but its version identity did not match", wrapper
        )

    def test_nacre_niri_command_uses_fork_binary_and_shared_config(self):
        config = Path(
            "/home/test/.local/share/nacre/niri-on-pc/session/config/config.kdl"
        )
        command = package_niri.command(config, "nacre-niri")
        self.assertEqual(command[command.index("-D") + 1], "niri")
        self.assertEqual(command[6], "/usr/bin/nacre-niri")
        self.assertEqual(command[-3:], ["--session", "--config", str(config)])
        self.assertEqual(package_niri.command(config)[6], "niri")

    def test_fork_validator_uses_packaged_binary(self):
        with patch.object(package_niri.subprocess, "run") as run:
            package_niri.validate(Path("/fixture/config.kdl"), "nacre-niri")
        self.assertEqual(
            run.call_args.args[0],
            ["/usr/bin/nacre-niri", "validate", "-c", "/fixture/config.kdl"],
        )

    def test_stock_and_fork_validate_separate_private_home_configs(self):
        fork = os.environ.get("NACRE_NIRI_BINARY")
        if not fork:
            self.skipTest("set NACRE_NIRI_BINARY to the task-built fork for this check")
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / ".local/share/nacre/niri-on-pc/session/config"
            config.mkdir(parents=True)
            shared = config / "config.kdl"
            overlay = config / "nacre-niri.kdl"
            shared.write_text('binds { Super+Ctrl+Tab { toggle-overview; } }\n')
            overlay.write_text(
                'include "config.kdl"\n'
                'binds { Super+Ctrl+Tab { focus-collection-next; } }\n'
            )
            self.assertNotIn("focus-collection-next", shared.read_text())
            subprocess.run(["/usr/bin/niri", "validate", "-c", str(shared)], check=True)
            subprocess.run([fork, "validate", "-c", str(overlay)], check=True)

    def test_nacre_session_dispatch_selects_fork_launcher(self):
        backend = SimpleNamespace(supported=lambda root: True, session=Mock())
        with (
            patch.object(package_sessions, "registry", return_value={"nacre-niri": {}}),
            patch.object(package_sessions, "integration", return_value=backend),
        ):
            package_sessions.start(Path("/fixture/nacre"), "nacre-niri")
        backend.session.assert_called_once_with(compositor="nacre-niri")

    def test_stock_session_clears_fork_selector_and_keeps_standard_niri_path(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            config = home / ".config/nacre/niri/config.kdl"
            config.parent.mkdir(parents=True)
            config.write_text("// stock session fixture\n")
            standard_bindings = home / ".config/nacre/niri/bindings.kdl"
            managed_dir = home / ".local/share/nacre/niri-on-pc/session/config"
            managed_dir.mkdir(parents=True)
            managed_bindings = managed_dir / "bindings.kdl"
            generated = "// generated stock bindings\nbinds { Super+Q { close-window; } }\n"
            standard_bindings.write_text(generated)
            managed_bindings.write_text(generated)
            custom = home / ".config/nacre/niri/custom.kdl"
            custom.write_text("binds { Super+F12 { close-window; } }\n")
            with (
                patch.object(package_niri, "validate"),
                patch.object(package_niri.subprocess, "run"),
                patch.object(package_niri.os, "execvpe") as execute,
                patch.dict(
                    package_niri.os.environ, {"NACRE_NIRI_FORK": "stale"}, clear=True
                ),
            ):
                package_niri.session(home)
            environment = execute.call_args.args[2]
            self.assertEqual(environment["NACRE_NIRI_FORK"], "0")
            self.assertEqual(execute.call_args.args[1][6], "niri")
            self.assertEqual(environment["XDG_CURRENT_DESKTOP"], "niri")
            self.assertIn("NACRE_NIRI_FORK", environment["UWSM_FINALIZE_VARNAMES"])
            self.assertEqual(standard_bindings.read_text(), generated)
            self.assertEqual(managed_bindings.read_text(), generated)
            self.assertEqual(
                custom.read_text(), "binds { Super+F12 { close-window; } }\n"
            )

    def test_fork_session_prefers_prepared_overlay_and_falls_back_to_shared_config(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            prepared = home / ".local/share/nacre/niri-on-pc/session"
            config = prepared / "config/config.kdl"
            config.parent.mkdir(parents=True)
            config.write_text("// shared generated configuration\n")
            (prepared / "source-revision").write_text("a" * 40)
            overlay = config.with_name("nacre-niri.kdl")
            overlay.write_text('include "config.kdl"\nbinds { Super+Ctrl+Tab { focus-collection-next; } }\n')
            with (
                patch.object(package_niri, "validate") as validate,
                patch.object(package_niri.subprocess, "run") as run,
                patch.object(package_niri.os, "execvpe") as execute,
                patch.dict(
                    package_niri.os.environ, {"NACRE_NIRI_FORK": "stale"}, clear=True
                ),
            ):
                package_niri.session(home, compositor="nacre-niri")
            validate.assert_called_once_with(overlay, "nacre-niri")
            args, environment = execute.call_args.args[1:]
            self.assertEqual(args[6], "/usr/bin/nacre-niri")
            self.assertEqual(args[-3:], ["--session", "--config", str(overlay)])
            self.assertEqual(environment["NACRE_COMPOSITOR"], "niri")
            self.assertEqual(environment["XDG_CURRENT_DESKTOP"], "niri")
            self.assertEqual(environment["NACRE_NIRI_FORK"], "1")
            self.assertEqual(environment["NIRI_CONFIG"], str(overlay))
            self.assertIn("NIRI_SOCKET", environment["UWSM_WAIT_VARNAMES"])
            self.assertIn("XCURSOR_THEME", environment["UWSM_FINALIZE_VARNAMES"])
            self.assertIn("NACRE_NIRI_FORK", environment["UWSM_FINALIZE_VARNAMES"])
            run.assert_called_once_with(
                ["systemctl", "--user", "daemon-reload"], check=True
            )

    def test_fork_session_falls_back_to_shared_config_without_overlay(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            config = home / ".config/nacre/niri/config.kdl"
            config.parent.mkdir(parents=True)
            config.write_text("// package setup fixture\n")
            with (
                patch.object(package_niri, "validate") as validate,
                patch.object(package_niri.subprocess, "run"),
                patch.object(package_niri.os, "execvpe") as execute,
                patch.dict(package_niri.os.environ, {}, clear=True),
            ):
                package_niri.session(home, compositor="nacre-niri")
            validate.assert_called_once_with(config, "nacre-niri")
            self.assertEqual(execute.call_args.args[2]["NIRI_CONFIG"], str(config))

    def test_fork_session_prefers_overlay_beside_nacre_generated_config(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            config = home / ".config/nacre/niri/config.kdl"
            config.parent.mkdir(parents=True)
            config.write_text("// stock config\n")
            overlay = config.with_name("nacre-niri.kdl")
            overlay.write_text('include "config.kdl"\n')
            with (
                patch.object(package_niri, "validate") as validate,
                patch.object(package_niri.subprocess, "run"),
                patch.object(package_niri.os, "execvpe") as execute,
                patch.dict(package_niri.os.environ, {}, clear=True),
            ):
                package_niri.session(home, compositor="nacre-niri")
            validate.assert_called_once_with(overlay, "nacre-niri")
            self.assertEqual(execute.call_args.args[2]["NIRI_CONFIG"], str(overlay))

    def test_uwsm_env_exports_shared_config_and_keeps_stock_override_separate(self):
        script = ROOT / "uwsm/env-niri"
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            prepared = home / ".local/share/nacre/niri-on-pc/session"
            (prepared / "config").mkdir(parents=True)
            (prepared / "source-revision").write_text("fixture\n")
            wrapper_dir = home / ".config/uwsm/niri-session-bin"
            wrapper_dir.mkdir(parents=True)
            loginctl = wrapper_dir / "loginctl"
            loginctl.write_text("#!/bin/sh\nexit 0\n")
            loginctl.chmod(0o755)

            def source(fork):
                environment = {
                    **os.environ,
                    "HOME": str(home),
                    "PATH": "/fixture/bin",
                    "NACRE_NIRI_FORK": "1" if fork else "",
                }
                output = subprocess.check_output(
                    [
                        "/bin/sh",
                        "-c",
                        '. "$1"; printf "NIRI_CONFIG=%s\\nPATH=%s\\nNACRE_COMPOSITOR=%s\\nLOGINCTL=%s\\n" "$NIRI_CONFIG" "$PATH" "$NACRE_COMPOSITOR" "$(command -v loginctl)"',
                        "probe",
                        str(script),
                    ],
                    env=environment,
                    text=True,
                )
                return dict(
                    line.split("=", 1) for line in output.splitlines() if "=" in line
                )

            stock = source(False)
            fork = source(True)
            shared = str(prepared / "config/config.kdl")
            wrapped = str(wrapper_dir)
            self.assertEqual(stock["NIRI_CONFIG"], shared)
            self.assertTrue(stock["PATH"].startswith(wrapped))
            self.assertEqual(stock["LOGINCTL"], str(loginctl))
            self.assertEqual(fork["NIRI_CONFIG"], shared)
            self.assertTrue(fork["PATH"].startswith(wrapped))
            self.assertEqual(fork["LOGINCTL"], str(loginctl))
            self.assertEqual(
                package_niri.command(Path(shared), "nacre-niri")[6],
                "/usr/bin/nacre-niri",
            )


if __name__ == "__main__":
    unittest.main()
