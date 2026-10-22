import importlib.util
import io
from pathlib import Path
import tempfile
import unittest
from types import SimpleNamespace
from contextlib import redirect_stdout
from unittest.mock import patch


INSTALL = Path(__file__).resolve().parents[1] / "install.py"
SPEC = importlib.util.spec_from_file_location("brain_install", INSTALL)
install = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(install)


class InstallTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.home = Path(self.temp.name) / "home"
        self.home.mkdir()
        self.root = Path(self.temp.name) / "repo/brain"
        self.root.mkdir(parents=True)
        for name in install.FILES:
            (self.root / name).write_text("new " + name)
        (self.root / "web").mkdir()
        (self.root / "web/index.html").write_text("new web")
        (self.root / "launch.sh").write_text("new wrapper")
        (self.root / "brain.service").write_text("new service")
        self.templates = self.root.parent / "ai/brain"
        self.templates.mkdir(parents=True)
        (self.templates / "template.md").write_text("new template")
        self.dest = self.home / ".local/share/nacre/brain"
        self.dest.mkdir(parents=True)
        (self.dest / "control.py").write_text("old control")
        self.wrapper = self.home / ".local/bin/nacre-brain-ui"
        self.wrapper.parent.mkdir(parents=True)
        self.wrapper.write_text("old wrapper")
        self.service = self.home / ".config/systemd/user/nacre-brain.service"
        self.service.parent.mkdir(parents=True)
        self.service.write_text("old service")

    def tearDown(self):
        self.temp.cleanup()

    def snapshot(self):
        result = {}
        for path in self.home.rglob("*"):
            if path.is_file():
                result[str(path.relative_to(self.home))] = path.read_bytes()
        return result

    def test_plan_only_does_not_change_files(self):
        before = self.snapshot()
        output = io.StringIO()
        with (
            patch.object(install, "ROOT", self.root),
            patch.object(install, "_validate_sources"),
            patch("pathlib.Path.home", return_value=self.home),
            redirect_stdout(output),
        ):
            install.main([])
        self.assertEqual(before, self.snapshot())
        self.assertIn("Plan only", output.getvalue())

    def test_apply_backs_up_every_replaced_path(self):
        commands = []
        backup = install.deploy(
            self.root,
            self.home,
            systemctl=lambda *a, **k: commands.append(a),
            readiness=lambda: True,
            helper_probe=lambda _: (True, ""),
        )
        self.assertEqual("new control.py", (self.dest / "control.py").read_text())
        self.assertEqual("new wrapper", self.wrapper.read_text())
        self.assertEqual("new service", self.service.read_text())
        self.assertEqual(
            "old control",
            (backup / self.dest.relative_to(self.home) / "control.py").read_text(),
        )
        self.assertEqual(
            "old wrapper", (backup / self.wrapper.relative_to(self.home)).read_text()
        )
        self.assertEqual(
            "old service", (backup / self.service.relative_to(self.home)).read_text()
        )
        for path in (self.dest, self.wrapper, self.service):
            saved = backup / path.relative_to(self.home)
            self.assertTrue(saved.exists())
        self.assertIn(("stop", *install.STOP_UNITS), commands)
        self.assertIn(("enable", "--now", "nacre-brain.service"), commands)

    def test_capability_probe_requires_expected_usage_response(self):
        helper = self.home / ".local/share/nacre/shell/tools/nacre-helper"
        helper.parent.mkdir(parents=True)
        helper.write_text("test stub")
        response = SimpleNamespace(
            returncode=2,
            stdout="",
            stderr="Usage: nacre-brain-ui [brain|capture|notes]",
        )
        with patch.object(install.subprocess, "run", return_value=response) as run:
            self.assertEqual((True, ""), install._helper_supports_web(self.home))
        self.assertEqual(
            [str(helper), "brain-launch", "__nacre_capability_probe__"],
            run.call_args.args[0],
        )

    def test_helper_capability_refusal_precedes_changes(self):
        before = self.snapshot()
        with self.assertRaisesRegex(RuntimeError, "does not support"):
            install.deploy(
                self.root,
                self.home,
                helper_probe=lambda _: (False, "helper does not support web"),
            )
        self.assertEqual(before, self.snapshot())

    def test_failed_readiness_restores_backup_and_restarts(self):
        commands = []
        with patch.object(install.time, "sleep"):
            with self.assertRaisesRegex(RuntimeError, "did not become ready"):
                install.deploy(
                    self.root,
                    self.home,
                    systemctl=lambda *a, **k: commands.append(a),
                    readiness=lambda: False,
                    helper_probe=lambda _: (True, ""),
                )
        self.assertEqual("old control", (self.dest / "control.py").read_text())
        self.assertEqual("old wrapper", self.wrapper.read_text())
        self.assertEqual("old service", self.service.read_text())
        self.assertIn(("restart", "nacre-brain.service"), commands)

    def test_worktree_refusal(self):
        root = self.home / ".local/share/nacre/ai/worktrees/task"
        root.mkdir(parents=True)
        self.assertTrue(install._is_worktree(root, self.home))
        with (
            patch.object(install, "ROOT", root),
            patch("pathlib.Path.home", return_value=self.home),
        ):
            with self.assertRaises(SystemExit):
                install.main([])
            output = io.StringIO()
            with redirect_stdout(output):
                with patch.object(install, "_validate_sources"):
                    self.assertEqual(0, install.main(["--allow-worktree"]))

    def test_apply_flag_dispatches_deployment(self):
        with (
            patch.object(install, "ROOT", self.root),
            patch("pathlib.Path.home", return_value=self.home),
            patch.object(install, "_is_worktree", return_value=False),
            patch.object(install, "deploy") as deploy,
            patch.object(install, "_validate_sources"),
        ):
            self.assertEqual(0, install.main(["--apply"]))
        deploy.assert_called_once_with(self.root, self.home)


if __name__ == "__main__":
    unittest.main()
