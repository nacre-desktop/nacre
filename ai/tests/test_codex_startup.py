"""Local Codex startup recovers without restarting peers or changing accounts."""

import subprocess
import unittest


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
            self.assertFalse((target.parent / "nacre_ai_core.py").exists())
            # Exercise the deployed file in a clean interpreter, so an older
            # private runtime cannot accidentally rely on the test's imports.
            result = subprocess.run(
                [str(link), "--help"],
                env={
                    "HOME": str(home),
                    "PATH": __import__("os").environ["PATH"],
                    "NACRE_HELPER_BINARY": __import__("os").environ[
                        "NACRE_HELPER_BINARY"
                    ],
                },
                capture_output=True,
                text=True,
                check=True,
            )
            self.assertIn("Codex accounts share chats", result.stdout)
            with self.assertRaisesRegex(RuntimeError, "recognized"):
                installer.install_launcher(ROOT, home / "unknown", True)


class ControllerDeploymentTests(unittest.TestCase):
    def test_linked_native_controllers_dispatch_to_helper(self):
        import os
        from pathlib import Path
        import subprocess
        import tempfile
        from test_workflow import ROOT

        with tempfile.TemporaryDirectory() as folder:
            bin_dir = Path(folder) / "bin"
            bin_dir.mkdir()
            for name in ("nacre-ai-chat", "nacre-ai-claude"):
                link = bin_dir / name
                link.symlink_to(ROOT / "bin" / name)
                result = subprocess.run(
                    [str(link), "--help"],
                    env={
                        "HOME": folder,
                        "PATH": os.environ["PATH"],
                        "NACRE_HELPER_BINARY": os.environ.get(
                            "NACRE_HELPER_BINARY",
                            str(ROOT / "target/release/nacre-helper"),
                        ),
                    },
                    capture_output=True,
                    text=True,
                )
                self.assertEqual(result.returncode, 0, result.stderr)

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
