import json
import os
from pathlib import Path
import subprocess
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import patch
from test_workflow import module


class ClaudeWorkflowTests(unittest.TestCase):
    def test_title_hook_prefers_ai_title_and_preserves_custom_title(self):
        hook = ROOT / "bin/nacre-ai-claude-title-hook"
        with tempfile.TemporaryDirectory() as temp:
            transcript = Path(temp) / "session.jsonl"
            transcript.write_text(
                json.dumps(
                    {"type": "user", "message": {"content": "First prompt"}}
                )
                + "\n"
                + json.dumps({"type": "ai-title", "aiTitle": "AI summary title"})
                + "\n"
            )
            event = json.dumps(
                {"prompt": "Later prompt", "transcript_path": str(transcript)}
            )
            result = subprocess.run(
                [sys.executable, str(hook)], input=event, text=True, capture_output=True
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(
                json.loads(result.stdout)["hookSpecificOutput"]["sessionTitle"],
                "AI summary title",
            )
            with transcript.open("a") as stream:
                stream.write(json.dumps({"type": "custom-title", "customTitle": "Owner rename"}))
            result = subprocess.run(
                [sys.executable, str(hook)], input=event, text=True, capture_output=True
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")

    def test_title_hook_uses_clean_first_prompt_and_fails_open(self):
        hook = ROOT / "bin/nacre-ai-claude-title-hook"
        with tempfile.TemporaryDirectory() as temp:
            transcript = Path(temp) / "session.jsonl"
            transcript.write_text("")
            event = json.dumps(
                {
                    "prompt": "  Explain\x00   tmux persistence.  ",
                    "transcript_path": str(transcript),
                }
            )
            result = subprocess.run(
                [sys.executable, str(hook)], input=event, text=True, capture_output=True
            )
            self.assertEqual(
                json.loads(result.stdout)["hookSpecificOutput"]["sessionTitle"],
                "Explain tmux persistence",
            )
            result = subprocess.run(
                [sys.executable, str(hook)], input="not json", text=True, capture_output=True
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")

    def test_title_hook_skips_oversized_transcript_to_protect_old_rename(self):
        hook = ROOT / "bin/nacre-ai-claude-title-hook"
        with tempfile.TemporaryDirectory() as temp:
            transcript = Path(temp) / "session.jsonl"
            transcript.write_text(
                json.dumps({"type": "custom-title", "customTitle": "Owner rename"})
                + "\n"
                + (" " * (2 * 1024 * 1024))
            )
            event = json.dumps(
                {"prompt": "Later prompt", "transcript_path": str(transcript)}
            )
            result = subprocess.run(
                [sys.executable, str(hook)], input=event, text=True, capture_output=True
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")

    def test_new_chat_uses_default_without_project_or_provider_picker(self):
        ai = module("nacre-ai")
        with (
            patch.object(ai, "default_assistant", return_value="claude"),
            patch.object(ai, "select", side_effect=["New chat", "Close"]) as select,
            patch.object(ai, "project_for") as project,
            patch.object(ai, "launch_background") as launch,
        ):
            ai.dashboard(SimpleNamespace(account="work"))
            self.assertEqual(select.call_count, 2)
            argv = launch.call_args.args[0]
            self.assertEqual(argv[argv.index("--agent") + 1], "claude")
            self.assertEqual(argv[argv.index("--account") + 1], "work")
            self.assertEqual(argv[argv.index("--project") + 1], "general-chat")
            project.assert_not_called()

    def test_escape_at_dashboard_does_not_launch(self):
        ai = module("nacre-ai")
        with (
            patch.object(ai, "select", side_effect=[None, "Close"]),
            patch.object(ai, "launch_background") as launch,
        ):
            ai.dashboard(SimpleNamespace(account=None))
            launch.assert_not_called()

    def test_combined_history_contains_both_agents_and_old_remote_sources(self):
        ai = module("nacre-ai")
        p = {
            "id": "p",
            "path": "/repo",
            "task_sources": [{"host": "dev", "path": "/remote"}],
        }
        with (
            patch.object(
                ai,
                "source_chats",
                side_effect=lambda p, a: [{"agent": "codex", "project": p}],
            ),
            patch.object(
                ai,
                "claude_chats",
                side_effect=lambda p, a: [{"agent": "claude", "project": p}],
            ),
        ):
            rows = ai.project_chats(p, None)
            self.assertEqual(
                [r["agent"] for r in rows], ["codex", "claude", "codex", "claude"]
            )
            self.assertEqual(rows[3]["project"]["host"], "dev")

    def test_latest_across_agents_resumes_exact_claude_id_without_touching_codex(self):
        ai = module("nacre-ai")
        p = {"id": "p", "path": "/repo"}
        rows = [
            {"id": "codex-id", "agent": "codex", "updated_at": 1, "project": p},
            {
                "id": "claude-id",
                "agent": "claude",
                "updated_at": 2,
                "project": p,
                "cwd": "/worktree",
            },
        ]
        with (
            patch.object(ai, "chat_projects", return_value=[p]),
            patch.object(ai, "project_chats", return_value=rows),
            patch.object(ai, "launch_claude") as launch,
            patch.object(ai, "local_environment") as codex,
        ):
            ai.load_task(
                SimpleNamespace(command="latest", project=None, account="work")
            )
            self.assertEqual(
                launch.call_args.args,
                ({"id": "p", "path": "/worktree"}, "resume", "work", "claude-id"),
            )
            codex.assert_not_called()

    def test_history_rows_show_assistant_even_for_identical_titles(self):
        ai = module("nacre-ai")
        p = {"id": "p", "path": "/repo", "label": "Project"}
        rows = [
            {
                "id": agent,
                "title": "Same title",
                "agent": agent,
                "updated_at": 1,
                "project": p,
                "state": "saved",
            }
            for agent in ("codex", "claude")
        ]
        with (
            patch.object(ai, "chat_projects", return_value=[p]),
            patch.object(ai, "project_chats", return_value=rows),
            patch.object(ai, "select", return_value=None) as select,
        ):
            ai.load_task(
                SimpleNamespace(command="sessions", project=None, account=None)
            )
            details = list(select.call_args.kwargs["details"].values())
            self.assertIn("Codex", details[0])
            self.assertIn("Claude Code", details[1])

    def test_claude_time_units_and_deduplication(self):
        c = module("nacre-ai-claude")
        row = SimpleNamespace(
            session_id="one",
            custom_title="Native title",
            summary="Summary",
            first_prompt="Prompt",
            last_modified=1700000000123,
            cwd="/tree",
        )
        records = c.records([row, row], {"session": "/tree"}, "/repo")
        self.assertEqual(len(records), 1)
        self.assertEqual(records[0]["updated_at"], 1700000000.123)
        self.assertEqual(records[0]["title"], "Native title")
        self.assertEqual(records[0]["state"], "running")

    def test_claude_running_sessions_match_thread_id_before_shared_cwd(self):
        c = module("nacre-ai-claude")
        rows = [
            SimpleNamespace(
                session_id="11111111-1111-4111-8111-111111111111",
                custom_title="One",
                summary="",
                first_prompt="",
                last_modified=1,
                cwd="/general",
            ),
            SimpleNamespace(
                session_id="22222222-2222-4222-8222-222222222222",
                custom_title="Two",
                summary="",
                first_prompt="",
                last_modified=2,
                cwd="/general",
            ),
        ]
        running = {
            "ai-general-chat-claude-default-11111111111141118111111111111111": "/general",
            "ai-general-chat-claude-default-22222222222242228222222222222222": "/general",
        }
        records = c.records(rows, running, "/general")
        self.assertEqual([row["session"] for row in records], list(running))
        self.assertEqual([row["state"] for row in records], ["running", "running"])

    def test_claude_legacy_cwd_match_requires_one_chat_and_one_tmux_session(self):
        c = module("nacre-ai-claude")
        rows = [
            SimpleNamespace(
                session_id=ident,
                custom_title=None,
                summary="",
                first_prompt="",
                last_modified=1,
                cwd="/general",
            )
            for ident in ("one", "two")
        ]
        records = c.records(rows, {"legacy": "/general"}, "/general")
        self.assertTrue(all(row["state"] == "saved" for row in records))

    def test_local_claude_launch_uses_tmux_when_available(self):
        ai = module("nacre-ai")
        with patch.object(ai.shutil, "which", return_value="/usr/bin/tmux"):
            argv = ai.claude_command({"id": "p", "path": "/repo"}, "new", "")
        self.assertIn("--tmux", argv)

    def test_claude_resume_carries_title_but_custom_title_is_marked(self):
        ai = module("nacre-ai")
        argv = ai.claude_command(
            {"id": "p", "path": "/repo"}, "resume", "", "thread", "Nacre title", True
        )
        self.assertIn("--title", argv)
        self.assertIn("--custom-title", argv)

    def test_named_accounts_have_distinct_claude_home_not_codex_auth(self):
        c = module("nacre-ai-claude")
        with (
            tempfile.TemporaryDirectory() as temp,
            patch.dict(os.environ, {"HOME": temp, "CODEX_HOME": "/codex"}),
        ):
            env = c.environment("work")
            self.assertEqual(
                env["CLAUDE_CONFIG_DIR"],
                str(Path(temp) / ".local/share/nacre/ai/claude-accounts/work"),
            )
            self.assertEqual(env["CODEX_HOME"], "/codex")
            self.assertFalse(Path(env["CLAUDE_CONFIG_DIR"]).exists())
            with self.assertRaises(ValueError):
                c.environment("../../bad")

    def test_new_claude_general_chat_does_not_create_git_worktree(self):
        c = module("nacre-ai-claude")
        with (
            tempfile.TemporaryDirectory() as temp,
            patch.object(c.subprocess, "run") as git,
        ):
            p = c.prepare_directory(
                SimpleNamespace(chat_directory=True, path=temp, project="general-chat")
            )
            self.assertTrue(p.is_dir())
            git.assert_not_called()

    def test_claude_project_creates_independent_worktree_in_empty_repo(self):
        c = module("nacre-ai-claude")
        with (
            tempfile.TemporaryDirectory() as temp,
            patch.dict(os.environ, {"HOME": temp}),
        ):
            repo = Path(temp) / "repo"
            repo.mkdir()
            subprocess.run(["git", "init", "-q", str(repo)], check=True)
            p = c.prepare_directory(
                SimpleNamespace(chat_directory=False, path=str(repo), project="repo")
            )
            self.assertNotEqual(p, repo)
            self.assertTrue((p / ".git").is_file())

    def test_remote_claude_uses_tmux_and_quotes_paths(self):
        ai = module("nacre-ai")
        argv = ai.claude_command(
            {"id": "p", "path": "/my repo", "host": "dev"}, "new", "work"
        )
        self.assertIn("-t", argv)
        self.assertIn("--path '/my repo'", argv[-1])
        self.assertIn("--tmux", argv[-1])

    def test_absent_claude_history_does_not_require_sdk_or_login(self):
        c = module("nacre-ai-claude")
        with (
            tempfile.TemporaryDirectory() as temp,
            patch.dict(os.environ, {"CLAUDE_CONFIG_DIR": temp}),
            patch.object(c, "sdk") as sdk,
        ):
            self.assertEqual(c.list_chats(SimpleNamespace()), [])
            sdk.assert_not_called()


class AccountSettingsTests(unittest.TestCase):
    def test_selections_are_independent_private_and_explicit_default_wins(self):
        ai = module("nacre-ai")
        with (
            tempfile.TemporaryDirectory() as temp,
            patch.dict(
                os.environ, {"NACRE_AI_SETTINGS": str(Path(temp) / "settings.json")}
            ),
        ):
            ai.select_account("codex", "work")
            ai.select_account("claude", "personal")
            self.assertEqual(ai.effective_account("codex"), "work")
            self.assertEqual(ai.effective_account("claude"), "personal")
            self.assertEqual(ai.effective_account("codex", ""), "")
            self.assertEqual(ai.effective_account("claude", "other"), "other")
            self.assertEqual(
                (Path(temp) / "settings.json").stat().st_mode & 0o777, 0o600
            )
            with self.assertRaises(ValueError):
                ai.select_account("codex", "../../bad")

    def test_failed_login_keeps_selected_accounts_unchanged(self):
        ai = module("nacre-ai")
        with (
            patch.object(
                ai.subprocess, "run", return_value=SimpleNamespace(returncode=1)
            ),
            patch.object(ai, "select_account") as activate,
            patch("builtins.input", return_value=""),
        ):
            ai.account_login(SimpleNamespace(agent="claude", account="new"))
            activate.assert_not_called()

    def test_successful_login_activates_only_chosen_provider(self):
        ai = module("nacre-ai")
        with (
            patch.object(
                ai.subprocess, "run", return_value=SimpleNamespace(returncode=0)
            ),
            patch.object(ai, "select_account") as activate,
            patch("builtins.input", return_value=""),
        ):
            ai.account_login(SimpleNamespace(agent="claude", account="new"))
            activate.assert_called_once_with("claude", "new")

    def test_settings_back_does_not_change_account_or_launch_login(self):
        ai = module("nacre-ai")
        with (
            patch.object(ai, "select", return_value=None),
            patch.object(ai, "select_account") as activate,
            patch.object(ai, "open_account_login") as login,
        ):
            ai.account_settings()
            activate.assert_not_called()
            login.assert_not_called()

    def test_history_resume_pins_the_account_recorded_when_listed(self):
        ai = module("nacre-ai")
        p = {"id": "p", "path": "/repo"}
        row = {
            "id": "abc",
            "agent": "claude",
            "account": "original",
            "updated_at": 1,
            "project": p,
        }
        with (
            patch.object(ai, "chat_projects", return_value=[p]),
            patch.object(ai, "project_chats", return_value=[row]),
            patch.object(ai, "launch_claude") as launch,
        ):
            ai.load_task(
                SimpleNamespace(command="latest", project=None, account="different")
            )
            self.assertEqual(launch.call_args.args[2], "original")

    def test_default_assistant_change_keeps_account_selections(self):
        ai = module("nacre-ai")
        with (
            tempfile.TemporaryDirectory() as temp,
            patch.dict(
                os.environ, {"NACRE_AI_SETTINGS": str(Path(temp) / "settings.json")}
            ),
        ):
            self.assertEqual(ai.default_assistant(), "codex")
            ai.select_account("codex", "work")
            ai.set_default_assistant("claude")
            self.assertEqual(ai.default_assistant(), "claude")
            self.assertEqual(ai.effective_account("codex"), "work")
            ai.select_account("claude", "other")
            self.assertEqual(ai.default_assistant(), "claude")

    def test_setup_status_never_displays_auth_command_output(self):
        ai = module("nacre-ai")
        with (
            patch.object(ai.shutil, "which", return_value="/tool"),
            patch.object(ai, "effective_account", return_value=""),
            patch.object(
                ai.subprocess,
                "run",
                return_value=SimpleNamespace(
                    returncode=0, stdout="private account data", stderr="private data"
                ),
            ),
        ):
            rows = ai.setup_status()
            self.assertTrue(all("private" not in row for row in rows))
            self.assertIn("signed in", rows[0])
            self.assertIn("signed in", rows[1])


class PartialHistoryTests(unittest.TestCase):
    def test_provider_failure_keeps_available_chats(self):
        ai = module("nacre-ai")
        errors = []
        with (
            patch.object(ai, "source_chats", return_value=[{"id": "local"}]),
            patch.object(ai, "claude_chats", side_effect=ValueError("unavailable")),
        ):
            self.assertEqual(
                ai.project_chats({"id": "p"}, None, errors), [{"id": "local"}]
            )
            self.assertEqual(errors, ["unavailable"])


class ClaudeAccessPolicyTests(unittest.TestCase):
    def test_new_and_resumed_workers_keep_standard_permissions_and_account_identity(
        self,
    ):
        c = module("nacre-ai-claude")
        for command in ("new", "resume"):
            with self.subTest(command=command), tempfile.TemporaryDirectory() as temp:
                directory = Path(temp) / "chat"
                directory.mkdir()
                args = SimpleNamespace(
                    account="work",
                    command=command,
                    tmux=False,
                    path=str(directory),
                    project="general-chat",
                    chat_directory=True,
                    thread_id="7b7db928-7582-4a1e-91b2-e6b601ca8a2d",
                    title="Nacre title",
                    custom_title=False,
                )
                with (
                    patch.dict(os.environ, {"HOME": temp}),
                    patch.object(c.subprocess, "run"),
                    patch.object(c, "executable", return_value="/fake/claude"),
                    patch.object(c, "prepare_directory", return_value=directory),
                    patch.object(c.os, "chdir"),
                    patch.object(
                        c.os, "execvpe", side_effect=RuntimeError("intercept")
                    ) as launch,
                ):
                    with self.assertRaisesRegex(RuntimeError, "intercept"):
                        c.run(args)
                executable, argv, env = launch.call_args.args
                self.assertEqual(executable, "/fake/claude")
                self.assertNotIn("--dangerously-skip-permissions", argv)
                self.assertEqual(
                    env["CLAUDE_CONFIG_DIR"],
                    str(Path(temp) / ".local/share/nacre/ai/claude-accounts/work"),
                )
                if command == "resume":
                    self.assertEqual(argv[argv.index("--resume") + 1], args.thread_id)
                    self.assertEqual(argv[argv.index("--name") + 1], args.title)
                else:
                    self.assertIn("--session-id", argv)
                    self.assertEqual(
                        argv[argv.index("--session-id") + 1], args.thread_id
                    )
                    overlay = json.loads(argv[argv.index("--settings") + 1])
                    hook_command = overlay["hooks"]["UserPromptSubmit"][0]["hooks"][0]["command"]
                    self.assertIn("nacre-ai-claude-title-hook", hook_command)

    def test_resume_attaches_to_chat_handed_off_to_background(self):
        c = module("nacre-ai-claude")
        original = "9563ba8c-801e-494d-9446-ed9417ebd6e5"
        fork = "e7c81fbd-082d-41a7-b6f0-6b1d2f795b24"
        agents = json.dumps(
            [
                {"id": "e7c81fbd", "kind": "background", "sessionId": fork},
                {"id": "", "kind": "interactive", "sessionId": original},
            ]
        )
        roster = {
            "workers": {
                "e7c81fbd": {
                    "dispatch": {
                        "launch": {"sessionId": f"/projects/chat/{original}.jsonl"}
                    }
                }
            }
        }
        for thread_id, expected in (
            (original, "e7c81fbd"),
            (fork, "e7c81fbd"),
            ("7b7db928-7582-4a1e-91b2-e6b601ca8a2d", None),
        ):
            with (
                self.subTest(thread_id=thread_id),
                tempfile.TemporaryDirectory() as temp,
            ):
                (Path(temp) / "daemon").mkdir()
                (Path(temp) / "daemon/roster.json").write_text(json.dumps(roster))
                with patch.object(
                    c.subprocess,
                    "run",
                    return_value=SimpleNamespace(stdout=agents),
                ):
                    self.assertEqual(
                        c.background_session(
                            "/fake/claude", {"CLAUDE_CONFIG_DIR": temp}, thread_id
                        ),
                        expected,
                    )
        with patch.object(c.subprocess, "run", side_effect=OSError("no daemon")):
            self.assertIsNone(
                c.background_session("/fake/claude", {"CLAUDE_CONFIG_DIR": "/x"}, fork)
            )
