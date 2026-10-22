"""Account switches preserve chats without sharing authentication."""

from types import SimpleNamespace
import unittest
from unittest.mock import patch

from test_workflow import module


class SharedChatTests(unittest.TestCase):
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
