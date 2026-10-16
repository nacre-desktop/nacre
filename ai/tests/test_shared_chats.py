"""Account switches preserve chats without sharing authentication."""

import os
from pathlib import Path
import tempfile
import tomllib
from types import SimpleNamespace
import unittest
from unittest.mock import Mock, patch

from test_workflow import module


class SharedChatTests(unittest.TestCase):
    def test_workflow_home_alias_resolving_to_same_account_is_preserved(self):
        chat = module("nacre-ai-chat")
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, HOME=temp):
            home = Path(temp) / ".local/share/nacre/ai/accounts/work"
            chat.configure({"CODEX_HOME": str(home)})
            alias = Path(temp) / "old-account-home"
            alias.symlink_to(home, target_is_directory=True)
            config = home / "config.toml"
            text = config.read_text().replace(
                "CODEX_HOME = " + __import__("json").dumps(str(home)),
                "CODEX_HOME = " + __import__("json").dumps(str(alias)),
            )
            config.write_text(text)
            chat.configure({"CODEX_HOME": str(home)})
            self.assertEqual(config.read_text(), text)

    def test_custom_previous_store_remains_discoverable_after_configuration(self):
        chat = module("nacre-ai-chat")
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, HOME=temp):
            old = Path(temp) / "custom-history"
            old.mkdir()
            (old / "state_5.sqlite").touch()
            home = Path(temp) / ".local/share/nacre/ai/accounts/old"
            home.mkdir(parents=True)
            (home / "config.toml").write_text(
                "sqlite_home = " + __import__("json").dumps(str(old)) + "\n"
            )
            chat.configure({"CODEX_HOME": str(home)})
            self.assertIn(("old", old), chat.history_sources())

    def test_old_remote_helper_keeps_working_until_host_is_updated(self):
        ai = module("nacre-ai")
        with (
            patch.object(
                ai.subprocess,
                "run",
                side_effect=[
                    SimpleNamespace(
                        returncode=2, stderr="unrecognized arguments: --all-accounts"
                    ),
                    SimpleNamespace(returncode=0, stdout='[{"id":"same-thread"}]'),
                ],
            ) as run,
        ):
            rows = ai.source_chats(
                {"id": "repo", "path": "/repo", "host": "host"}, "work"
            )
        self.assertEqual(rows[0]["id"], "same-thread")
        self.assertEqual(rows[0]["account"], "work")
        self.assertEqual(run.call_count, 2)
        self.assertNotIn("--all-accounts", run.call_args.args[0][-1])

    def test_profiles_share_state_but_preserve_credentials_and_config(self):
        chat = module("nacre-ai-chat")
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, HOME=temp):
            default = Path(temp) / ".codex"
            default.mkdir()
            (default / "auth.json").write_text("default fixture")
            for name in ("first", "second"):
                home = Path(temp) / ".local/share/nacre/ai/accounts" / name
                home.mkdir(parents=True)
                (home / "auth.json").write_text(name + " fixture")
                config = home / "config.toml"
                original = 'model = "keep"\n[profiles.custom]\nsqlite_home = "/keep"\n'
                config.write_text(original)
                chat.configure({"CODEX_HOME": str(home)})
                first = config.read_text()
                chat.configure({"CODEX_HOME": str(home)})
                self.assertEqual(config.read_text(), first)
                data = tomllib.loads(first)
                self.assertEqual(data["sqlite_home"], str(default))
                self.assertEqual(data["model"], "keep")
                self.assertEqual(data["profiles"]["custom"]["sqlite_home"], "/keep")
                self.assertEqual((home / "auth.json").read_text(), name + " fixture")
                backups = list(
                    (Path(temp) / ".local/state/nacre/ai/backups").glob(
                        "*/" + name + "/config.toml"
                    )
                )
                self.assertEqual(len(backups), 1)
                self.assertEqual(backups[0].read_text(), original)
                self.assertEqual(backups[0].stat().st_mode & 0o777, 0o600)
            self.assertEqual((default / "auth.json").read_text(), "default fixture")

    def test_discovery_preserves_old_stores_and_ignores_linked_profiles(self):
        chat = module("nacre-ai-chat")
        with tempfile.TemporaryDirectory() as temp, patch.dict(os.environ, HOME=temp):
            root = Path(temp) / ".local/share/nacre/ai/accounts"
            old = root / "old"
            old.mkdir(parents=True)
            (old / "state_5.sqlite").touch()
            (root / "new").mkdir()
            (root / "linked").symlink_to(old, target_is_directory=True)
            self.assertEqual(
                chat.history_sources(), [("", Path(temp) / ".codex"), ("old", old)]
            )

    def test_combined_list_keeps_ids_and_uses_newest_duplicate(self):
        chat = module("nacre-ai-chat")
        shared, old = Path("/shared"), Path("/old")
        with (
            patch.object(chat, "shared_history_home", return_value=shared),
            patch.object(
                chat, "history_sources", return_value=[("", shared), ("old", old)]
            ),
            patch.object(chat, "running_sessions", return_value={}),
            patch.object(
                chat,
                "list_threads",
                side_effect=[
                    [{"id": "same", "name": "First", "updatedAt": 1}],
                    [
                        {"id": "same", "name": "Updated", "updatedAt": 2},
                        {"id": "other", "name": "Other"},
                    ],
                ],
            ),
        ):
            result = chat.shared_chat_records({}, ["/repo"], "repo")
        self.assertEqual(len(result), 2)
        self.assertEqual(result[0]["id"], "same")
        self.assertEqual(result[0]["title"], "Updated")
        self.assertEqual(result[0]["history_home"], "/old")
        self.assertFalse(result[0]["shared_history"])

    def test_pagination_reads_more_than_five_pages(self):
        chat = module("nacre-ai-chat")
        api = Mock()
        api.call.side_effect = [
            {"data": [{"id": str(i)}], "nextCursor": str(i + 1) if i < 6 else None}
            for i in range(7)
        ]
        with patch.object(chat, "Codex") as client:
            client.return_value.__enter__.return_value = api
            self.assertEqual(len(chat.list_threads({}, ["/repo"])), 7)
        self.assertEqual(api.call.call_args.args[1]["modelProviders"], [])

    def test_resume_uses_selected_login_and_original_id_and_store(self):
        ai = module("nacre-ai")
        project = {"id": "repo", "path": "/repo"}
        for shared in (False, True):
            item = {
                "id": "existing-thread",
                "title": "Keep",
                "state": "saved",
                "project": project,
                "account": "old",
                "shared_history": shared,
                "history_home": "/old-store",
            }
            with (
                patch.object(ai, "chat_projects", return_value=[project]),
                patch.object(ai, "project_chats", return_value=[item]),
                patch.object(
                    ai,
                    "local_environment",
                    return_value={"CODEX_HOME": "/selected-login"},
                ) as environment,
                patch.object(ai, "exec_local_codex") as execute,
            ):
                ai.load_task(
                    SimpleNamespace(command="latest", project=None, account="selected")
                )
            environment.assert_called_once_with("selected")
            argv, env = execute.call_args.args
            self.assertEqual(argv[1:3], ["resume", "existing-thread"])
            self.assertEqual("--no-daemon" in argv, not shared)
            self.assertEqual(env["CODEX_HOME"], "/selected-login")
            if not shared:
                self.assertIn('sqlite_home="/old-store"', argv)
                self.assertEqual(env["NACRE_AI_HISTORY_HOME"], "/old-store")

    def test_legacy_resume_does_not_start_or_restart_a_peer_daemon(self):
        ai = module("nacre-ai")
        argv = ["/codex", "resume", "same-id", "--no-daemon"]
        with (
            patch.object(ai, "prepare_local_codex") as prepare,
            patch.object(ai.os, "execvpe") as execute,
        ):
            ai.exec_local_codex(argv, {"CODEX_HOME": "/selected"})
        prepare.assert_not_called()
        execute.assert_called_once_with("/codex", argv, {"CODEX_HOME": "/selected"})
