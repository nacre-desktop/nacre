import concurrent.futures
import os
from pathlib import Path
import stat
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


class IsolatedWorkflowTest(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory(prefix="nacre-workflow-test-")
        self.addCleanup(temporary.cleanup)
        home = Path(temporary.name)
        environment = dict(os.environ)
        for key in (
            "CODEX_HOME",
            "CLAUDE_CONFIG_DIR",
            "NACRE_BRAIN",
            "NACRE_BRAIN_DIR",
            "NACRE_AI_PROJECTS",
            "NACRE_AI_SETTINGS",
        ):
            environment.pop(key, None)
        environment.update(
            HOME=str(home),
            XDG_CONFIG_HOME=str(home / ".config"),
            XDG_STATE_HOME=str(home / ".local/state"),
            XDG_CACHE_HOME=str(home / ".cache"),
        )
        isolated = patch.dict(os.environ, environment, clear=True)
        isolated.start()
        self.addCleanup(isolated.stop)


class BrainTests(IsolatedWorkflowTest):
    def test_note_metadata_cannot_bypass_common_secret_checks(self):
        with tempfile.TemporaryDirectory() as tmp:
            for field in ("--title", "--source"):
                argv = [
                    str(ROOT / "bin/nacre-brain"),
                    "note",
                    "--title",
                    "Title",
                    "--source",
                    "Source",
                ]
                argv[argv.index(field) + 1] = "password: example-private-value"
                result = subprocess.run(
                    argv,
                    input="Ordinary body",
                    text=True,
                    env=dict(os.environ, NACRE_BRAIN=tmp),
                    capture_output=True,
                )
                self.assertNotEqual(result.returncode, 0)
            self.assertEqual(list((Path(tmp) / "inbox").glob("*.md")), [])

    def test_concurrent_notes_are_private_and_distinct(self):
        with tempfile.TemporaryDirectory() as tmp:
            vault = Path(tmp) / "private"
            env = dict(os.environ, NACRE_BRAIN=str(vault))
            # Initialization is done by installation before concurrent writers.
            subprocess.run(
                [str(ROOT / "bin/nacre-brain"), "init"],
                env=env,
                check=True,
                capture_output=True,
            )

            def write(index):
                return subprocess.run(
                    [
                        str(ROOT / "bin/nacre-brain"),
                        "note",
                        "--title",
                        "Concurrent fact",
                        "--source",
                        "test evidence",
                        "--confidence",
                        "verified",
                    ],
                    input=f"Observation {index}",
                    text=True,
                    env=env,
                    check=True,
                    capture_output=True,
                ).stdout.strip()

            with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
                paths = list(pool.map(write, range(16)))
            self.assertEqual(len(set(paths)), 16)
            self.assertEqual(stat.S_IMODE(vault.stat().st_mode), 0o700)
            for index, name in enumerate(paths):
                self.assertIn(f"Observation {index}", Path(name).read_text())
                self.assertEqual(stat.S_IMODE(Path(name).stat().st_mode), 0o600)

    def test_failed_note_does_not_create_a_note_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = dict(os.environ, NACRE_BRAIN=tmp)
            result = subprocess.run(
                [
                    str(ROOT / "bin/nacre-brain"),
                    "note",
                    "--title",
                    "Blocked",
                    "--source",
                    "test",
                ],
                input="refresh_token: test-private-value",
                text=True,
                env=env,
                capture_output=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(list((Path(tmp) / "inbox").glob("*.md")), [])


class WorkflowTests(IsolatedWorkflowTest):
    def test_remote_wrapper_protects_legacy_home_and_honors_separate_accounts(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            binary = home / ".local/share/nacre/ai/codex/packages/0.159.2/bin/codex"
            binary.parent.mkdir(parents=True)
            binary.write_text('#!/bin/sh\nprintf "%s\\n" "$CODEX_HOME"\n')
            binary.chmod(0o755)
            for selected, expected in [
                (home / ".codex", home / ".local/share/nacre/ai/codex-home"),
                (
                    home / ".local/share/nacre/ai/accounts/second",
                    home / ".local/share/nacre/ai/accounts/second",
                ),
            ]:
                result = subprocess.run(
                    ["bash", str(ROOT / "ai/remote-codex-wrapper.sh")],
                    env=dict(os.environ, HOME=tmp, CODEX_HOME=str(selected)),
                    capture_output=True,
                    text=True,
                    check=True,
                )
                self.assertEqual(result.stdout.strip(), str(expected))

    def test_account_helper_preserves_default_auth_and_uses_isolated_home(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            (home / ".local/bin").mkdir(parents=True)
            (home / ".local/bin/nacre-ai-chat").symlink_to(ROOT / "bin/nacre-ai-chat")
            (home / ".local/bin/nacre-ai-skills").symlink_to(
                ROOT / "bin/nacre-ai-skills"
            )
            (home / ".codex").mkdir()
            (home / ".codex/auth.json").write_text("default authentication")
            (home / ".codex/AGENTS.md").write_text("shared guidance")
            fake = home / "fake"
            fake.mkdir()
            (fake / "codex").write_text('#!/bin/sh\nprintf "%s\\n" "$CODEX_HOME"\n')
            (fake / "codex").chmod(0o755)
            result = subprocess.run(
                [
                    str(ROOT / "bin/nacre-ai-account"),
                    "second",
                    "login",
                ],
                env=dict(
                    os.environ,
                    HOME=tmp,
                    PATH=str(fake) + ":" + os.environ["PATH"],
                    NACRE_HELPER_BINARY=os.environ.get(
                        "NACRE_HELPER_BINARY", str(ROOT / "target/release/nacre-helper")
                    ),
                ),
                text=True,
                capture_output=True,
                check=True,
            )
            profile = home / ".local/share/nacre/ai/accounts/second"
            self.assertEqual(result.stdout.strip(), str(profile))
            self.assertEqual(
                (home / ".codex/auth.json").read_text(), "default authentication"
            )
            self.assertFalse((profile / "auth.json").exists())
            self.assertTrue((profile / "AGENTS.md").is_symlink())

    def test_install_preserves_existing_guidance_credentials_and_registry(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            (home / ".codex").mkdir()
            (home / ".codex/AGENTS.md").write_text("Existing user rule\n")
            (home / ".codex/auth.json").write_text("Existing account data")
            (home / ".config/nacre/ai").mkdir(parents=True)
            registry = home / ".config/nacre/ai/projects.json"
            registry.write_text('{"projects": []}')
            env = dict(os.environ, HOME=tmp)
            for _ in range(2):
                subprocess.run(
                    [
                        sys.executable,
                        str(ROOT / "ai/install.py"),
                        "--apply",
                        "--allow-worktree",
                    ],
                    env=env,
                    check=True,
                    capture_output=True,
                )
            guidance = (home / ".codex/AGENTS.md").read_text()
            self.assertTrue(guidance.startswith("Existing user rule"))
            self.assertEqual(guidance.count("<!-- nacre-ai-guidance -->"), 1)
            self.assertEqual(
                (home / ".codex/auth.json").read_text(), "Existing account data"
            )
            self.assertEqual(registry.read_text(), '{"projects": []}')
            self.assertTrue((home / ".agents/skills/nacre-brain").is_symlink())
            self.assertFalse((home / ".local/bin/nacre-brain-sync").exists())
            self.assertFalse(
                (home / ".config/systemd/user/nacre-brain-sync.timer").exists()
            )
            self.assertFalse(
                (home / ".config/systemd/user/nacre-brain-sync.service").exists()
            )

    def test_full_install_defaults_to_plan_and_apply_mutates_temp_home(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp) / "home"
            home.mkdir()
            (home / ".codex").mkdir()
            guidance = home / ".codex/AGENTS.md"
            guidance.write_text("User guidance\n")
            (home / ".codex/auth.json").write_text("credentials fixture")

            def snapshot():
                return {
                    str(path.relative_to(home)): (
                        "link:" + os.readlink(path)
                        if path.is_symlink()
                        else "dir"
                        if path.is_dir()
                        else "file:" + path.read_bytes().hex()
                    )
                    for path in home.rglob("*")
                }

            before = snapshot()
            env = dict(os.environ, HOME=str(home))
            planned = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "ai/install.py"),
                    "--repo",
                    str(ROOT),
                    "--allow-worktree",
                ],
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )
            self.assertIn("Full install plan only", planned.stdout)
            self.assertIn("LINK", planned.stdout)
            self.assertEqual(snapshot(), before)
            applied = subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "ai/install.py"),
                    "--repo",
                    str(ROOT),
                    "--apply",
                    "--allow-worktree",
                ],
                env=env,
                check=True,
                capture_output=True,
                text=True,
            )
            self.assertIn("Personal workflow installed", applied.stdout)
            self.assertNotEqual(snapshot(), before)
            self.assertEqual(
                (home / ".codex/auth.json").read_text(), "credentials fixture"
            )

    def test_full_install_refuses_non_main_worktree(self):
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            main = base / "main"
            worktree = base / "worktree"
            home = base / "home"
            main.mkdir()
            home.mkdir()
            subprocess.run(["git", "init", str(main)], check=True, capture_output=True)
            subprocess.run(
                [
                    "git",
                    "-C",
                    str(main),
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "fixture",
                ],
                check=True,
                capture_output=True,
            )
            subprocess.run(
                ["git", "-C", str(main), "worktree", "add", str(worktree)],
                check=True,
                capture_output=True,
            )
            env = dict(os.environ, HOME=str(home))
            refused = subprocess.run(
                [sys.executable, str(ROOT / "ai/install.py"), "--repo", str(worktree)],
                env=env,
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(refused.returncode, 0)
            self.assertIn("Refusing install from Git worktree", refused.stderr)
            self.assertEqual(list(home.iterdir()), [])

    def test_installer_preserves_foreign_guidance_symlink_target(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            foreign = home / "other-public-repository/AGENTS.md"
            foreign.parent.mkdir()
            foreign.write_text("Existing external rule\n")
            (home / ".codex").mkdir()
            guidance = home / ".codex/AGENTS.md"
            guidance.symlink_to(foreign)
            subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "ai/install.py"),
                    "--apply",
                    "--allow-worktree",
                ],
                env=dict(os.environ, HOME=tmp),
                check=True,
                capture_output=True,
            )
            self.assertEqual(foreign.read_text(), "Existing external rule\n")
            self.assertFalse(guidance.is_symlink())
            self.assertTrue(guidance.read_text().startswith("Existing external rule"))
            backups = list(
                (home / ".local/state/nacre/ai/backups").glob("*/.codex/AGENTS.md")
            )
            self.assertEqual(len(backups), 1)
            self.assertTrue(backups[0].is_symlink())

    def test_reinstall_refreshes_owned_guidance_pointer_after_checkout_move(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            (home / ".codex").mkdir()
            (home / ".codex/AGENTS.md").write_text("Existing user rule\n")
            env = dict(os.environ, HOME=tmp)
            subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "ai/install.py"),
                    "--apply",
                    "--allow-worktree",
                ],
                env=env,
                check=True,
                capture_output=True,
            )
            moved = home / "moved-checkout"
            shutil.copytree(ROOT, moved, ignore=shutil.ignore_patterns("target"))
            subprocess.run(
                [
                    sys.executable,
                    str(moved / "ai/install.py"),
                    "--apply",
                    "--allow-worktree",
                ],
                env=env,
                check=True,
                capture_output=True,
            )
            guidance = (home / ".codex/AGENTS.md").read_text()
            self.assertIn(str(moved / "ai/AGENTS.md"), guidance)
            self.assertNotIn(str(ROOT / "ai/AGENTS.md"), guidance)
            self.assertEqual(guidance.count("<!-- nacre-ai-guidance -->"), 1)


if __name__ == "__main__":
    unittest.main()
