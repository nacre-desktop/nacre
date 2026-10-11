"""Local Codex startup recovers without restarting peers or changing accounts."""

import subprocess
import unittest
from types import SimpleNamespace
from unittest.mock import patch
from test_workflow import module


class CodexStartupTests(unittest.TestCase):
    def setUp(self):
        self.ai = module("nacre-ai")
        self.argv = ["/local/codex", "-C", "/chat", "--add-dir", "/brain"]
        self.env = {"CODEX_HOME": "/accounts/selected"}

    def test_ready_server_executes_original_arguments_and_account(self):
        with (
            patch.object(
                self.ai.subprocess, "run", return_value=SimpleNamespace(returncode=0)
            ) as start,
            patch.object(self.ai.os, "execvpe") as execute,
        ):
            self.ai.exec_local_codex(self.argv, self.env)
        self.assertEqual(
            start.call_args.args[0], ["/local/codex", "app-server", "daemon", "start"]
        )
        self.assertEqual(start.call_args.kwargs["env"], self.env)
        execute.assert_called_once_with(self.argv[0], self.argv, self.env)

    def test_draining_transition_retries_without_restart_or_stop(self):
        with (
            patch.object(
                self.ai.subprocess,
                "run",
                side_effect=[
                    SimpleNamespace(
                        returncode=1,
                        stderr="Server is draining; retry after reconnecting",
                    ),
                    SimpleNamespace(returncode=0),
                ],
            ) as start,
            patch("time.sleep"),
            patch.object(self.ai.os, "execvpe") as execute,
        ):
            self.ai.exec_local_codex(self.argv, self.env)
        self.assertEqual(start.call_count, 2)
        self.assertTrue(
            all(call.args[0][-1] == "start" for call in start.call_args_list)
        )
        execute.assert_called_once()

    def test_persistent_transition_is_bounded_and_does_not_open_duplicates(self):
        with (
            patch.object(
                self.ai.subprocess,
                "run",
                side_effect=subprocess.TimeoutExpired("codex", 10),
            ) as start,
            patch("time.sleep"),
            patch.object(self.ai.os, "execvpe") as execute,
        ):
            with self.assertRaisesRegex(ValueError, "still transitioning"):
                self.ai.exec_local_codex(self.argv, self.env)
        self.assertEqual(start.call_count, 3)
        execute.assert_not_called()

    def test_unrelated_errors_are_not_retried_or_hidden(self):
        with (
            patch.object(
                self.ai.subprocess,
                "run",
                return_value=SimpleNamespace(
                    returncode=1, stderr="Configuration permission denied"
                ),
            ) as start,
            patch.object(self.ai.os, "execvpe") as execute,
        ):
            with self.assertRaisesRegex(ValueError, "doctor"):
                self.ai.exec_local_codex(self.argv, self.env)
        self.assertEqual(start.call_count, 1)
        execute.assert_not_called()


class LauncherDeploymentTests(unittest.TestCase):
    def test_plan_and_apply_touch_only_the_private_launcher(self):
        import importlib.util
        from pathlib import Path
        import tempfile
        from test_workflow import ROOT

        spec = importlib.util.spec_from_file_location(
            "install_ai", ROOT / "ai/install.py"
        )
        installer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(installer)
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            target = (
                home / ".local/share/nacre/ai/conversation-runtime/test/bin/nacre-ai"
            )
            target.parent.mkdir(parents=True)
            target.write_text("prior launcher")
            target.chmod(0o755)
            link = home / ".local/bin/nacre-ai"
            link.parent.mkdir(parents=True)
            link.symlink_to(target)
            auth = home / ".codex/auth.json"
            auth.parent.mkdir()
            auth.write_text("existing private data")
            installer.install_launcher(ROOT, home)
            self.assertEqual(target.read_text(), "prior launcher")
            self.assertFalse((home / ".local/state").exists())
            installer.install_launcher(ROOT, home, True)
            self.assertEqual(target.read_bytes(), (ROOT / "bin/nacre-ai").read_bytes())
            self.assertEqual(target.stat().st_mode & 0o777, 0o755)
            self.assertTrue(link.is_symlink())
            backups = list((home / ".local/state/nacre/ai/backups").glob("*/nacre-ai"))
            self.assertEqual(len(backups), 1)
            self.assertEqual(backups[0].read_text(), "prior launcher")
            self.assertEqual(auth.read_text(), "existing private data")
            with self.assertRaisesRegex(RuntimeError, "recognized"):
                installer.install_launcher(ROOT, home / "unknown", True)


class ControllerDeploymentTests(unittest.TestCase):
    def test_claude_only_update_preserves_other_controller_and_private_state(self):
        import importlib.util
        from pathlib import Path
        import tempfile
        from test_workflow import ROOT

        spec = importlib.util.spec_from_file_location(
            "controller_install", ROOT / "ai/install.py"
        )
        installer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(installer)
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            runtime = home / ".local/share/nacre/ai/conversation-runtime/test/bin"
            runtime.mkdir(parents=True)
            target = runtime / "nacre-ai-claude"
            target.write_text("prior controller")
            target.chmod(0o755)
            other = runtime / "nacre-ai"
            other.write_text("running peer")
            link = home / ".local/bin/nacre-ai-claude"
            link.parent.mkdir(parents=True)
            link.symlink_to(target)
            private = home / ".claude/private-fixture"
            private.parent.mkdir()
            private.write_text("preserve")
            installer.install_launcher(ROOT, home, name="nacre-ai-claude")
            self.assertEqual(target.read_text(), "prior controller")
            self.assertFalse((home / ".local/state").exists())
            installer.install_launcher(ROOT, home, True, "nacre-ai-claude")
            self.assertEqual(
                target.read_bytes(), (ROOT / "bin/nacre-ai-claude").read_bytes()
            )
            self.assertEqual(target.stat().st_mode & 0o777, 0o755)
            self.assertEqual(other.read_text(), "running peer")
            self.assertEqual(private.read_text(), "preserve")
            self.assertTrue(link.is_symlink())
            backup = list(
                (home / ".local/state/nacre/ai/backups").glob("*/nacre-ai-claude")
            )
            self.assertEqual(len(backup), 1)
            self.assertEqual(backup[0].read_text(), "prior controller")
            with self.assertRaisesRegex(RuntimeError, "Unrecognized"):
                installer.install_launcher(ROOT, home, True, "auth.json")
