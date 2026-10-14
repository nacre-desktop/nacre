import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock, patch
from types import SimpleNamespace
from test_workflow import module


class ChatTitles(unittest.TestCase):
    def test_titles_keep_identity_and_match_running_worktree(self):
        chat = module("nacre-ai-chat")
        records = chat.chat_records(
            [
                {"id": "one", "name": "Fix stream lag", "cwd": "/repo/one"},
                {"id": "two", "name": "Fix stream lag", "cwd": "/repo/two"},
                {"id": "three", "preview": "Check settings", "cwd": "/repo/three"},
            ],
            {"ai-project-default-20260930T140000-123": "/repo/one"},
        )
        self.assertEqual(records[0]["state"], "running")
        self.assertEqual(records[1]["state"], "saved")
        self.assertEqual(records[2]["title"], "Check settings")
        self.assertEqual([r["id"] for r in records], ["one", "two", "three"])

    def test_account_filter_cannot_select_longer_account_prefix(self):
        chat = module("nacre-ai-chat")
        valid = "ai-project-profile-work-20260930T140000-123"
        data = (
            valid
            + "\t/repo\nai-project-profile-work-personal-20260930T140000-123\t/other\n"
        )
        with (
            patch("shutil.which", return_value="/usr/bin/tmux"),
            patch.object(
                chat.subprocess, "run", return_value=SimpleNamespace(stdout=data)
            ),
        ):
            self.assertEqual(chat.running_sessions("project", "work"), {valid: "/repo"})

    def test_title_control_characters_cannot_escape_terminal(self):
        chat = module("nacre-ai-chat")
        self.assertEqual(chat.label("hello\r\nworld\x1b"), "helloworld")
        self.assertLessEqual(len(chat.label("x" * 200)), 100)

    def test_derived_title_cache_is_private_and_strips_controls(self):
        import json

        chat = module("nacre-ai-chat")
        with (
            tempfile.TemporaryDirectory() as home,
            patch.dict(os.environ, {"HOME": home}),
        ):
            chat.cache_title("thread-one", "A title\x1b\n")
            file = Path(home) / ".local/state/nacre/ai/chat-titles/thread-one.json"
            self.assertEqual(
                json.loads(file.read_text()), {"title": "A title", "version": 1}
            )
            self.assertEqual(file.stat().st_mode & 0o777, 0o600)
            with self.assertRaises(ValueError):
                chat.cache_title("../escape", "no")

    def test_new_chat_window_uses_selected_default_assistant(self):
        workflow = module("nacre-ai")
        args = SimpleNamespace(
            worker_command="new", project=None, account=None, agent=None
        )
        with (
            patch.object(workflow, "default_assistant", return_value="claude"),
            patch.object(
                workflow.subprocess, "run", return_value=SimpleNamespace(returncode=0)
            ) as run,
        ):
            workflow.worker_window(args)
            command = run.call_args.args[0]
            self.assertEqual(command[command.index("--agent") + 1], "claude")

    def test_account_home_stays_isolated(self):
        chat = module("nacre-ai-chat")
        with (
            tempfile.TemporaryDirectory() as home,
            patch.dict(os.environ, {"HOME": home}),
        ):
            self.assertEqual(
                chat.account_env("work")["CODEX_HOME"],
                str(Path(home) / ".local/share/nacre/ai/accounts/work"),
            )
            with self.assertRaises(ValueError):
                chat.account_env("../other")

    def test_named_thread_caches_metadata_without_reading_turns(self):
        import json

        chat = module("nacre-ai-chat")
        api = Mock()
        api.call.return_value = {"thread": {"cwd": "/chat"}}
        with (
            tempfile.TemporaryDirectory() as home,
            patch.dict(os.environ, HOME=home),
        ):
            chat.cache_named_thread(api, "thread-one", "Title")
            api.call.assert_called_once_with(
                "thread/read", {"threadId": "thread-one", "includeTurns": False}
            )
            file = Path(home) / ".local/state/nacre/ai/chat-titles/thread-one.json"
            self.assertEqual(
                json.loads(file.read_text()),
                {"title": "Title", "cwd": "/chat", "version": 1},
            )

    def test_directory_identity_survives_older_title_only_cache_writers(self):
        import json

        chat = module("nacre-ai-chat")
        thread = "11111111-1111-1111-1111-111111111111"
        with tempfile.TemporaryDirectory() as home, patch.dict(os.environ, HOME=home):
            directory = Path(home) / ".local/share/nacre/ai/chats" / ("a" * 32)
            chat.cache_title(thread, "First name", str(directory))
            chat.cache_title(thread, "Renamed chat")
            root = Path(home) / ".local/state/nacre/ai/chat-titles"
            identity = root / "directories" / f"{directory.name}.json"
            self.assertEqual(
                json.loads(identity.read_text()), {"threadId": thread, "version": 1}
            )
            self.assertEqual(identity.stat().st_mode & 0o777, 0o600)
            self.assertEqual(
                json.loads((root / f"{thread}.json").read_text()),
                {"title": "Renamed chat", "version": 1},
            )

    def test_failed_handshake_always_cleans_child(self):
        chat = module("nacre-ai-chat")
        with (
            patch.object(chat.subprocess, "Popen"),
            patch.object(chat.Codex, "call", side_effect=ValueError("unavailable")),
            patch.object(chat.Codex, "close") as close,
        ):
            with self.assertRaises(ValueError):
                with chat.Codex({}):
                    pass
            close.assert_called_once()

    def test_mcp_only_exposes_naming_and_preserves_thread_identity(self):
        import io
        import json

        chat = module("nacre-ai-chat")
        requests = [
            {"jsonrpc": "2.0", "id": 1, "method": "tools/list"},
            {
                "jsonrpc": "2.0",
                "id": 2,
                "method": "tools/call",
                "params": {
                    "name": "set_chat_title",
                    "arguments": {"thread_id": "thread-one", "title": "Fix camera lag"},
                },
            },
            {
                "jsonrpc": "2.0",
                "id": 3,
                "method": "tools/call",
                "params": {"name": "run_command"},
            },
        ]
        output = io.StringIO()
        with (
            patch.object(
                chat.sys, "stdin", io.StringIO("\n".join(map(json.dumps, requests)))
            ),
            patch.object(chat.sys, "stdout", output),
            patch.object(chat, "set_title") as rename,
        ):
            chat.serve({"CODEX_HOME": "/account"})
            rename.assert_called_once_with(
                "thread-one", "Fix camera lag", {"CODEX_HOME": "/account"}
            )
        replies = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertEqual(
            [t["name"] for t in replies[0]["result"]["tools"]],
            ["set_chat_title", "resolve_context", "find_knowledge", "remember"],
        )
        self.assertEqual(replies[2]["error"]["code"], -32601)

    def test_configure_is_idempotent_and_keeps_other_config(self):
        chat = module("nacre-ai-chat")
        with tempfile.TemporaryDirectory() as temp:
            config = Path(temp) / "config.toml"
            config.write_text('model = "existing"\n')
            env = {"CODEX_HOME": temp}
            chat.configure(env)
            first = config.read_text()
            chat.configure(env)
            self.assertEqual(config.read_text(), first)
            self.assertEqual(__import__("tomllib").loads(first)["model"], "existing")
            self.assertEqual(first.count("[mcp_servers.nacre_workflow]"), 1)


class GeneralHistoryTests(unittest.TestCase):
    def test_general_history_accepts_only_chat_directories_without_git(self):
        chat = module("nacre-ai-chat")
        with (
            tempfile.TemporaryDirectory() as temp,
            patch.object(chat.subprocess, "run") as git,
        ):
            root = Path(temp)
            directory = root / ("a" * 32)
            directory.mkdir()
            (root / "unrelated").mkdir()
            (root / ("b" * 32)).symlink_to(directory, target_is_directory=True)
            self.assertEqual(chat.history_paths(temp, True), [temp, str(directory)])
            git.assert_not_called()
            self.assertEqual(chat.history_paths(str(root / "absent"), True), [])


class AccessDefaultsTests(unittest.TestCase):
    def test_configure_sets_full_access_without_changing_nested_settings(self):
        import tomllib

        chat = module("nacre-ai-chat")
        with tempfile.TemporaryDirectory() as temp:
            config = Path(temp) / "config.toml"
            config.write_text(
                'sandbox_mode = "workspace-write"\napproval_policy = "on-request"\nmodel = "keep-model"\n[profiles.special]\napproval_policy = "on-request"\n'
            )
            chat.configure({"CODEX_HOME": temp})
            text = config.read_text()
            data = tomllib.loads(text)
            self.assertEqual(data["sandbox_mode"], "danger-full-access")
            self.assertEqual(data["approval_policy"], "never")
            self.assertEqual(data["model"], "keep-model")
            self.assertEqual(
                data["profiles"]["special"]["approval_policy"], "on-request"
            )
            chat.configure({"CODEX_HOME": temp})
            self.assertEqual(config.read_text(), text)

    def test_invalid_config_is_not_overwritten(self):
        chat = module("nacre-ai-chat")
        with tempfile.TemporaryDirectory() as temp:
            config = Path(temp) / "config.toml"
            config.write_text("[broken")
            with self.assertRaises(ValueError):
                chat.configure({"CODEX_HOME": temp})
            self.assertEqual(config.read_text(), "[broken")
