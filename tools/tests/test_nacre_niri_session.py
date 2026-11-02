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
spec = importlib.util.spec_from_file_location("package_niri_session", ROOT / "tools/package_niri.py")
package_niri = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package_niri)
session_spec = importlib.util.spec_from_file_location(
    "package_sessions_under_test", ROOT / "tools/package_sessions.py"
)
package_sessions = importlib.util.module_from_spec(session_spec)
session_spec.loader.exec_module(package_sessions)


class NacreNiriSessionTests(unittest.TestCase):
    def test_nacre_niri_command_uses_fork_binary_and_shared_config(self):
        config = Path("/home/test/.local/share/nacre/niri-on-pc/session/config/config.kdl")
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
            with (
                patch.object(package_niri, "validate"),
                patch.object(package_niri.subprocess, "run"),
                patch.object(package_niri.os, "execvpe") as execute,
                patch.dict(package_niri.os.environ, {"NACRE_NIRI_FORK": "stale"}, clear=True),
            ):
                package_niri.session(home)
            environment = execute.call_args.args[2]
            self.assertEqual(environment["NACRE_NIRI_FORK"], "0")
            self.assertEqual(execute.call_args.args[1][6], "niri")
            self.assertEqual(environment["XDG_CURRENT_DESKTOP"], "niri")
            self.assertIn("NACRE_NIRI_FORK", environment["UWSM_FINALIZE_VARNAMES"])

    def test_fork_session_prefers_existing_prepared_config_and_clears_stock_leak(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            prepared = home / ".local/share/nacre/niri-on-pc/session"
            config = prepared / "config/config.kdl"
            config.parent.mkdir(parents=True)
            config.write_text("// shared generated configuration\n")
            (prepared / "source-revision").write_text("a" * 40)
            with (
                patch.object(package_niri, "validate") as validate,
                patch.object(package_niri.subprocess, "run") as run,
                patch.object(package_niri.os, "execvpe") as execute,
                patch.dict(package_niri.os.environ, {"NACRE_NIRI_FORK": "stale"}, clear=True),
            ):
                package_niri.session(home, compositor="nacre-niri")
            validate.assert_called_once_with(config, "nacre-niri")
            args, environment = execute.call_args.args[1:]
            self.assertEqual(args[6], "/usr/bin/nacre-niri")
            self.assertEqual(args[-3:], ["--session", "--config", str(config)])
            self.assertEqual(environment["NACRE_COMPOSITOR"], "niri")
            self.assertEqual(environment["XDG_CURRENT_DESKTOP"], "niri")
            self.assertEqual(environment["NACRE_NIRI_FORK"], "1")
            self.assertEqual(environment["NIRI_CONFIG"], str(config))
            self.assertIn("NIRI_SOCKET", environment["UWSM_WAIT_VARNAMES"])
            self.assertIn("XCURSOR_THEME", environment["UWSM_FINALIZE_VARNAMES"])
            self.assertIn("NACRE_NIRI_FORK", environment["UWSM_FINALIZE_VARNAMES"])
            run.assert_called_once_with(["systemctl", "--user", "daemon-reload"], check=True)

    def test_fork_session_falls_back_to_nacre_generated_config(self):
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

    def test_uwsm_env_exports_shared_config_and_keeps_stock_override_separate(self):
        script = ROOT / "uwsm/env-niri"
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            prepared = home / ".local/share/nacre/niri-on-pc/session"
            (prepared / "config").mkdir(parents=True)
            (prepared / "source-revision").write_text("fixture\n")

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
                        '. "$1"; printf "NIRI_CONFIG=%s\\nPATH=%s\\nNACRE_COMPOSITOR=%s\\n" "$NIRI_CONFIG" "$PATH" "$NACRE_COMPOSITOR"',
                        "probe",
                        str(script),
                    ],
                    env=environment,
                    text=True,
                )
                return dict(line.split("=", 1) for line in output.splitlines() if "=" in line)

            stock = source(False)
            fork = source(True)
            shared = str(prepared / "config/config.kdl")
            self.assertEqual(stock["NIRI_CONFIG"], shared)
            self.assertTrue(stock["PATH"].startswith(str(home / ".config/uwsm/niri-session-bin")))
            self.assertEqual(fork["NIRI_CONFIG"], shared)
            self.assertEqual(fork["PATH"], "/fixture/bin")


if __name__ == "__main__":
    unittest.main()
