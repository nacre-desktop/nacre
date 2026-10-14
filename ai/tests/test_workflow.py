import concurrent.futures
import importlib.machinery
import importlib.util
import json
import os
from pathlib import Path
import stat
import shutil
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def module(name):
    loader = importlib.machinery.SourceFileLoader(
        name.replace("-", "_"), str(ROOT / "bin" / name)
    )
    spec = importlib.util.spec_from_loader(loader.name, loader)
    loaded = importlib.util.module_from_spec(spec)
    loader.exec_module(loaded)
    return loaded


class AgentProfilePolicyTests(unittest.TestCase):
    def test_full_access_requires_an_explicit_personal_profile(self):
        with (
            tempfile.TemporaryDirectory() as directory,
            patch.dict(os.environ, {"HOME": directory}),
        ):
            for name in ("nacre-ai", "nacre-ai-claude"):
                agent = module(name)
                self.assertFalse(agent.agent_full_access())
                path = Path(directory) / ".config/nacre/profile.json"
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(json.dumps({"aiFullAccess": True}))
                self.assertTrue(agent.agent_full_access())
                path.unlink()


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
                    sys.executable,
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
                [sys.executable, str(ROOT / "bin/nacre-brain"), "init"],
                env=env,
                check=True,
                capture_output=True,
            )

            def write(index):
                return subprocess.run(
                    [
                        sys.executable,
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

    def test_common_secret_values_are_rejected_but_references_allowed(self):
        brain = module("nacre-brain")
        for text in [
            "password: example-private-value",
            "-----BEGIN OPENSSH PRIVATE KEY-----",
            "sk-proj-" + "x" * 30,
        ]:
            with self.assertRaises(ValueError):
                brain.safe_note(text)
        brain.safe_note(
            "Credential reference: secret-service://nacre-brain/development"
        )

    def test_failed_note_does_not_create_a_note_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = dict(os.environ, NACRE_BRAIN=tmp)
            result = subprocess.run(
                [
                    sys.executable,
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
    def test_dashboard_forwards_selected_account_to_worker(self):
        ai = module("nacre-ai")
        with (
            patch.object(sys, "argv", ["nacre-ai", "dashboard", "--account", "second"]),
            patch.object(ai, "select", side_effect=["New chat", "Close"]),
            patch.object(ai, "project_for", return_value={"id": "example-project"}),
            patch.object(ai.subprocess, "Popen") as launch,
            patch.object(ai, "default_assistant", return_value="codex"),
        ):
            ai.main()
            argv = launch.call_args.args[0]
            self.assertEqual(argv[argv.index("--project") + 1], "general-chat")
            self.assertEqual(argv[argv.index("--agent") + 1], "codex")
            self.assertEqual(argv[-2:], ["--account", "second"])

    def test_gui_warnings_do_not_corrupt_the_menu(self):
        ai = module("nacre-ai")
        with (
            tempfile.TemporaryDirectory() as tmp,
            patch.dict(os.environ, {"HOME": tmp}),
        ):
            process = ai.launch_background(
                [
                    sys.executable,
                    "-c",
                    'import sys; print("window diagnostic", file=sys.stderr)',
                ]
            )
            self.assertEqual(process.wait(timeout=5), 0)
            self.assertIn(
                "window diagnostic",
                (Path(tmp) / ".local/state/nacre/ai/launcher.log").read_text(),
            )

    def test_escape_at_root_and_project_cancel_keep_dashboard_open(self):
        ai = module("nacre-ai")
        with (
            patch.object(ai, "select", side_effect=[None, None, "Close"]) as menu,
            patch.object(ai, "project_for", return_value=None),
            patch.object(ai.subprocess, "Popen") as launch,
        ):
            ai.dashboard(SimpleNamespace(account=None))
            self.assertEqual(menu.call_count, 3)
            launch.assert_not_called()

    def test_menu_uses_alternate_screen_without_fzf_scrollback(self):
        ai = module("nacre-ai")
        with patch.object(ai.curses, "wrapper", return_value=None) as wrapper:
            self.assertIsNone(ai.select(["New task"], "Workspace"))
            wrapper.assert_called_once()
        self.assertEqual(
            ai.clean_label("Title\x1b\n with\t spacing"), "Title with spacing"
        )

    def test_menu_only_has_requested_choices(self):
        ai = module("nacre-ai")
        with patch.object(ai, "select", return_value="Close") as menu:
            ai.dashboard(SimpleNamespace(account=None))
            self.assertEqual(
                menu.call_args.args[0],
                [
                    "New chat",
                    "Resume latest chat",
                    "Load chat",
                    "Open brain",
                    "Settings",
                    "Close",
                ],
            )

    def test_remote_shell_uses_supported_terminal_without_changing_parent(self):
        ai = module("nacre-ai")
        with (
            patch.dict(os.environ, {"TERM": "xterm-kitty"}),
            patch.object(
                ai,
                "project_for",
                return_value={"id": "example", "host": "dev", "path": "/tmp/repo"},
            ),
            patch.object(ai.os, "execvpe", side_effect=RuntimeError("exec")) as execute,
        ):
            with self.assertRaises(RuntimeError):
                ai.run_project(
                    SimpleNamespace(project="example", account=None, command="shell")
                )
            self.assertEqual(execute.call_args.args[2]["TERM"], "xterm-256color")
            self.assertEqual(os.environ["TERM"], "xterm-kitty")

    def test_worker_failure_stays_visible_until_enter(self):
        ai = module("nacre-ai")
        with (
            patch.object(
                ai, "project_for", return_value={"id": "example", "host": "dev"}
            ),
            patch.object(
                ai.subprocess, "run", return_value=SimpleNamespace(returncode=255)
            ),
            patch("builtins.input") as wait,
        ):
            ai.worker_window(
                SimpleNamespace(project="example", account=None, worker_command="new")
            )
            wait.assert_called_once()

    def test_local_worker_has_writable_worktree_and_brain(self):
        ai = module("nacre-ai")
        with (
            patch.object(
                ai, "project_for", return_value={"id": "example", "path": "/tmp/repo"}
            ),
            patch.object(
                ai.subprocess, "run", return_value=SimpleNamespace(returncode=0)
            ),
            patch.object(ai.os, "execvpe", side_effect=RuntimeError("exec")) as execute,
        ):
            with self.assertRaises(RuntimeError):
                ai.run_project(
                    SimpleNamespace(project="example", account=None, command="new")
                )
            command = execute.call_args.args[1]
            self.assertEqual(command[command.index("--sandbox") + 1], "workspace-write")
            self.assertEqual(command[command.index("--ask-for-approval") + 1], "never")
            self.assertIn("--worktree", command)
            self.assertIn("--add-dir", command)

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

    def test_remote_resume_displays_names_but_attaches_exact_session(self):
        ai = module("nacre-ai")
        project = {"id": "project", "host": "my-server", "path": "/tmp/project"}
        session = "ai-project-profile-work-20260930T140000-123"
        listing = SimpleNamespace(
            returncode=0,
            stdout=json.dumps(
                [
                    {
                        "id": "thread-1",
                        "title": "Fix Wi-Fi setup",
                        "state": "running",
                        "session": session,
                    }
                ]
            ),
        )
        with (
            patch.object(ai, "project_for", return_value=project),
            patch.object(ai.subprocess, "run", return_value=listing) as run,
            patch.object(ai, "select", side_effect=lambda lines, *_, **kw: lines[0]),
            patch.object(ai, "ssh_exec") as execute,
        ):
            ai.run_project(
                SimpleNamespace(command="sessions", project="project", account="work")
            )
            self.assertIn("--account work", run.call_args.args[0][-1])
            self.assertIn("=" + session, execute.call_args.args[1])

    def test_saved_chat_resumes_exact_id_and_account(self):
        ai = module("nacre-ai")
        item = {
            "id": "thread-1",
            "title": "Camera work",
            "state": "saved",
            "session": None,
        }
        with (
            patch.object(
                ai,
                "project_for",
                return_value={"id": "example", "host": "dev", "path": "/repo"},
            ),
            patch.object(
                ai.subprocess,
                "run",
                return_value=SimpleNamespace(returncode=0, stdout=json.dumps([item])),
            ),
            patch.object(ai, "select", side_effect=lambda lines, *_, **kw: lines[0]),
            patch.object(ai, "ssh_exec") as execute,
        ):
            ai.run_project(
                SimpleNamespace(command="sessions", project="example", account="second")
            )
            self.assertIn(
                "--thread-id thread-1 --project example --account second",
                execute.call_args.args[1],
            )

    def test_remote_account_is_forwarded_without_shell_injection(self):
        ai = module("nacre-ai")
        project = {
            "id": "project",
            "host": "my-server",
            "path": "/tmp/project with spaces",
        }
        with (
            patch.object(ai, "project_for", return_value=project),
            patch.object(
                ai.os, "execvpe", side_effect=RuntimeError("exec intercepted")
            ) as call,
        ):
            with self.assertRaisesRegex(RuntimeError, "exec intercepted"):
                ai.run_project(
                    SimpleNamespace(command="new", project="project", account="second")
                )
            args = call.call_args.args[1]
            self.assertEqual(args[:3], ["ssh", "-t", "my-server"])
            self.assertIn("'/tmp/project with spaces' second", args[3])
            with self.assertRaises(ValueError):
                ai.run_project(
                    SimpleNamespace(
                        command="new", project="project", account="../../escape"
                    )
                )

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
                    sys.executable,
                    str(ROOT / "bin/nacre-ai-account"),
                    "second",
                    "login",
                ],
                env=dict(
                    os.environ, HOME=tmp, PATH=str(fake) + ":" + os.environ["PATH"]
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

    def test_separate_account_home_keeps_shared_guidance_without_auth_copy(self):
        ai = module("nacre-ai")
        with (
            tempfile.TemporaryDirectory() as tmp,
            patch.dict(os.environ, {"HOME": tmp}),
        ):
            (Path(tmp) / ".local/bin").mkdir(parents=True)
            (Path(tmp) / ".local/bin/nacre-ai-chat").symlink_to(
                ROOT / "bin/nacre-ai-chat"
            )
            codex = Path(tmp) / ".codex"
            codex.mkdir()
            (codex / "AGENTS.md").write_text("Personal guidance")
            (codex / "auth.json").write_text("existing account data")
            first = Path(ai.local_environment("first")["CODEX_HOME"])
            second = Path(ai.local_environment("second")["CODEX_HOME"])
            self.assertNotEqual(first, second)
            self.assertEqual((first / "AGENTS.md").resolve(), codex / "AGENTS.md")
            self.assertFalse((first / "auth.json").exists())
            self.assertEqual((codex / "auth.json").read_text(), "existing account data")
            with self.assertRaises(ValueError):
                ai.local_environment("../escape")

    def test_project_registry_rejects_shell_host_syntax(self):
        ai = module("nacre-ai")
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "projects.json"
            path.write_text(
                json.dumps(
                    {
                        "projects": [
                            {
                                "id": "project",
                                "path": "/tmp/repo",
                                "host": "server; command",
                            }
                        ]
                    }
                )
            )
            with (
                patch.dict(os.environ, {"NACRE_AI_PROJECTS": str(path)}),
                self.assertRaises(ValueError),
            ):
                ai.projects()

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
                    [sys.executable, str(ROOT / "ai/install.py")],
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
                [sys.executable, str(ROOT / "ai/install.py")],
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
                [sys.executable, str(ROOT / "ai/install.py")],
                env=env,
                check=True,
                capture_output=True,
            )
            moved = home / "moved-checkout"
            shutil.copytree(ROOT, moved, ignore=shutil.ignore_patterns("target"))
            subprocess.run(
                [sys.executable, str(moved / "ai/install.py")],
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


class LatestTaskTests(IsolatedWorkflowTest):
    def test_latest_uses_most_recent_across_projects_without_picker(self):
        ai = module("nacre-ai")
        projects = [
            {"id": "one", "host": "dev", "path": "/one"},
            {"id": "two", "host": "prod", "path": "/two"},
        ]

        def chats(p, account, errors=None):
            return [
                dict(
                    id=p["id"],
                    title="A task",
                    state="running",
                    session="session-" + p["id"],
                    updated_at=10 if p["id"] == "one" else 20,
                    project=p,
                )
            ]

        with (
            patch.object(ai, "chat_projects", return_value=projects),
            patch.object(ai, "project_chats", side_effect=chats),
            patch.object(ai, "select") as picker,
            patch.object(ai, "ssh_exec") as execute,
        ):
            ai.load_task(
                SimpleNamespace(command="latest", project=None, account="work")
            )
            picker.assert_not_called()
            self.assertEqual(
                execute.call_args.args, ("prod", "tmux attach-session -t =session-two")
            )

    def test_latest_dashboard_does_not_ask_for_project(self):
        ai = module("nacre-ai")
        with (
            patch.object(ai, "select", side_effect=["Resume latest chat", "Close"]),
            patch.object(ai, "project_for") as picker,
            patch.object(ai, "launch_background") as launch,
        ):
            ai.dashboard(SimpleNamespace(account=None))
            picker.assert_not_called()
            self.assertIn("latest", launch.call_args.args[0])

    def test_local_saved_task_resumes_exact_thread_and_worktree(self):
        ai = module("nacre-ai")
        project = {"id": "local", "path": "/repo"}
        item = {
            "id": "thread-xyz",
            "title": "Fix settings",
            "state": "saved",
            "updated_at": 30,
            "cwd": "/repo-worktree",
            "project": project,
        }
        with (
            patch.object(ai, "chat_projects", return_value=[project]),
            patch.object(ai, "project_chats", return_value=[item]),
            patch.object(ai, "local_environment", return_value={}),
            patch.object(ai, "prepare_local_codex"),
            patch.object(ai.os, "execvpe") as execute,
        ):
            ai.load_task(SimpleNamespace(command="latest", project=None, account=None))
            argv = execute.call_args.args[1]
            self.assertEqual(argv[1:3], ["resume", "thread-xyz"])
            self.assertEqual(argv[argv.index("--sandbox") + 1], "workspace-write")
            self.assertEqual(argv[argv.index("--ask-for-approval") + 1], "never")
            self.assertEqual(argv[argv.index("-C") + 1], "/repo-worktree")

    def test_no_tasks_returns_to_dashboard_without_starting_new_task(self):
        ai = module("nacre-ai")
        with (
            patch.object(ai, "chat_projects", return_value=[{"id": "one"}]),
            patch.object(ai, "project_chats", return_value=[]),
            patch.object(ai, "select", return_value="Back") as picker,
            patch.object(ai.os, "execvpe") as execute,
        ):
            ai.load_task(SimpleNamespace(command="latest", project=None, account=None))
            picker.assert_called_once()
            execute.assert_not_called()


class NewProjectTests(IsolatedWorkflowTest):
    def test_new_project_registered_without_overwriting_other_projects(self):
        ai = module("nacre-ai")
        with (
            tempfile.TemporaryDirectory() as temp,
            patch.dict(os.environ, {"HOME": temp}, clear=False),
        ):
            config = Path(temp) / "projects.json"
            config.write_text(
                json.dumps(
                    {"projects": [{"id": "old", "label": "Old", "path": "/existing"}]}
                )
            )
            with patch.dict(os.environ, {"NACRE_AI_PROJECTS": str(config)}):
                p = ai.create_project("My New Game")
                self.assertTrue((Path(p["path"]) / ".git").is_dir())
                self.assertEqual(
                    [x["id"] for x in json.loads(config.read_text())["projects"]],
                    ["old", "my-new-game"],
                )
                with self.assertRaises(ValueError):
                    ai.create_project("My New Game")
                with (
                    patch.object(ai, "prepare_local_codex"),
                    patch.object(ai.os, "execvpe") as execute,
                ):
                    ai.run_project(
                        SimpleNamespace(command="new", project=p["id"], account=None)
                    )
                    argv = execute.call_args.args[1]
                    worktree = Path(argv[argv.index("-C") + 1])
                    self.assertTrue((worktree / ".git").is_file())
                    self.assertNotEqual(worktree, Path(p["path"]))
                head = subprocess.run(
                    ["git", "-C", p["path"], "rev-parse", "--verify", "HEAD"],
                    capture_output=True,
                )
                self.assertNotEqual(head.returncode, 0)

    def test_new_project_never_overwrites_existing_folder(self):
        ai = module("nacre-ai")
        with (
            tempfile.TemporaryDirectory() as temp,
            patch.dict(
                os.environ,
                {
                    "HOME": temp,
                    "NACRE_AI_PROJECTS": str(Path(temp) / "projects.json"),
                },
            ),
        ):
            folder = Path(temp) / "Projects/keep-me"
            folder.mkdir(parents=True)
            (folder / "important").write_text("keep")
            with self.assertRaises(FileExistsError):
                ai.create_project("Keep me")
            self.assertEqual((folder / "important").read_text(), "keep")

    def test_project_wizard_back_does_not_create_folder(self):
        ai = module("nacre-ai")
        with (
            patch.object(ai, "project_name", return_value=None),
            patch.object(ai, "create_project") as create,
        ):
            self.assertIsNone(ai.project_wizard())
            create.assert_not_called()


class ProjectBuildHostTests(IsolatedWorkflowTest):
    def test_build_host_does_not_move_local_tasks_to_ssh(self):
        ai = module("nacre-ai")
        p = {
            "id": "example",
            "path": "/local/repo",
            "build_host": "builder",
            "build_path": "/remote/repo",
        }
        with (
            patch.object(ai, "project_for", return_value=p),
            patch.object(
                ai.subprocess, "run", return_value=SimpleNamespace(returncode=0)
            ),
            patch.object(ai.os, "execvpe") as execute,
            patch.object(ai, "ssh_exec") as ssh,
        ):
            ai.run_project(
                SimpleNamespace(command="new", project="example", account=None)
            )
            ssh.assert_not_called()
            self.assertIn("/local/repo", execute.call_args.args[1])

    def test_load_keeps_previous_remote_chat_source(self):
        ai = module("nacre-ai")
        p = {
            "id": "example",
            "path": "/local/repo",
            "task_sources": [{"host": "dev", "path": "/remote/repo"}],
        }
        with (
            patch.object(
                ai,
                "source_chats",
                side_effect=lambda source, account: [{"project": source}],
            ),
            patch.object(ai, "claude_chats", return_value=[]),
        ):
            rows = ai.project_chats(p, None)
            self.assertEqual(len(rows), 2)
            self.assertNotIn("host", rows[0]["project"])
            self.assertEqual(rows[1]["project"]["host"], "dev")


class GeneralChatTests(IsolatedWorkflowTest):
    def test_general_chat_launch_is_not_a_git_task_and_has_separate_folders(self):
        ai = module("nacre-ai")
        with (
            tempfile.TemporaryDirectory() as temp,
            patch.dict(os.environ, {"HOME": temp}),
            patch.object(ai.os, "execvpe") as execute,
            patch.object(ai.subprocess, "run") as run,
            patch.object(ai, "prepare_local_codex"),
        ):
            folders = []
            for _ in range(2):
                ai.run_project(
                    SimpleNamespace(command="new", project="general-chat", account=None)
                )
                argv = execute.call_args.args[1]
                self.assertNotIn("--worktree", argv)
                folder = Path(argv[argv.index("-C") + 1])
                folders.append(folder)
                self.assertTrue(folder.is_dir())
                guidance = (folder / "AGENTS.md").read_text()
                self.assertIn("projects.json", guidance)
                self.assertIn("Retain this conversation", guidance)
                self.assertIn("selected profile", guidance)
                self.assertEqual(argv[argv.index("--sandbox") + 1], "workspace-write")
                self.assertEqual(argv[argv.index("--ask-for-approval") + 1], "never")
            self.assertNotEqual(folders[0], folders[1])
            run.assert_not_called()

    def test_general_option_is_only_in_new_task_picker(self):
        ai = module("nacre-ai")
        with (
            patch.object(
                ai,
                "projects",
                return_value=[{"id": "os", "label": "Custom OS", "path": "/os"}],
            ),
            patch.object(
                ai, "select", side_effect=lambda lines, *_, **kw: lines[0]
            ) as menu,
        ):
            self.assertEqual(
                ai.project_for(None, include_general=True)["id"], "general-chat"
            )
            self.assertIn("General chat", menu.call_args.args[0][0])
            self.assertEqual(ai.project_for(None)["id"], "os")

    def test_general_history_is_included_without_registered_projects(self):
        ai = module("nacre-ai")
        with patch.object(ai, "projects", return_value=[]):
            self.assertEqual([p["id"] for p in ai.chat_projects()], ["general-chat"])

    def test_general_history_flag_and_account_forwarded(self):
        ai = module("nacre-ai")
        with (
            patch.object(ai, "local_environment", return_value={}),
            patch.object(ai, "prepare_local_codex"),
            patch.object(
                ai.subprocess,
                "run",
                return_value=SimpleNamespace(returncode=0, stdout="[]"),
            ) as run,
        ):
            ai.source_chats(ai.general_chat(), "personal")
            argv = run.call_args.args[0]
            self.assertIn("--chat-directory", argv)
            self.assertEqual(argv[argv.index("--account") + 1], "personal")

    def test_general_name_cannot_shadow_chat_entry(self):
        ai = module("nacre-ai")
        with self.assertRaises(ValueError):
            ai.create_project("General chat")


class WorkflowIsolationTests(unittest.TestCase):
    def test_mocked_workflow_leaves_caller_home_and_account_settings_untouched(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            settings = home / ".config/nacre/ai/settings.json"
            settings.parent.mkdir(parents=True)
            settings.write_text('{"accounts":{"codex":"host-account"}}')
            original = settings.read_bytes()
            before = sorted(str(p.relative_to(home)) for p in home.rglob("*"))
            with patch.dict(
                os.environ,
                {
                    "HOME": directory,
                    "NACRE_AI_SETTINGS": str(settings),
                    "CODEX_HOME": str(home / "existing-codex"),
                },
            ):
                for method in (
                    "test_dashboard_forwards_selected_account_to_worker",
                    "test_local_worker_has_writable_worktree_and_brain",
                ):
                    result = unittest.TestResult()
                    WorkflowTests(method).run(result)
                    self.assertTrue(
                        result.wasSuccessful(), result.errors + result.failures
                    )
            self.assertEqual(settings.read_bytes(), original)
            self.assertEqual(
                sorted(str(p.relative_to(home)) for p in home.rglob("*")), before
            )
