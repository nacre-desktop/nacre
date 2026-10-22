import importlib.util
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "sidebar", Path(__file__).resolve().parents[1] / "sidebar-chat.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class SidebarTests(unittest.TestCase):
    def manager(self, root):
        return patch.multiple(module, HOME=root, STATE=root / "state"), patch.object(
            module, "preferences", return_value=("codex", "")
        )

    def test_history_preserves_roles_and_updates_streamed_message_in_place(self):
        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                manager.delta("reply", "Hello")
                manager.delta("reply", " world")
                manager.complete_message("reply", "Hello world!")
                self.assertEqual(len(manager.messages), 1)
                self.assertEqual(manager.messages[0]["text"], "Hello world!")
                manager.restore(
                    [
                        dict(
                            items=[
                                dict(
                                    id="u",
                                    type="userMessage",
                                    content=[dict(type="text", text="Question")],
                                ),
                                dict(id="a", type="agentMessage", text="Answer"),
                            ]
                        )
                    ]
                )
                self.assertEqual(
                    [m["role"] for m in manager.messages], ["user", "assistant"]
                )

    def test_questions_require_matching_request_id(self):
        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                from unittest.mock import Mock

                manager = module.Manager()
                peer = Mock()
                manager.ask(peer, 17, [dict(id="choice", question="Choose")], "input")
                manager.handle(dict(action="answer", id="old", answers={}))
                peer.answer.assert_not_called()
                manager.handle(
                    dict(
                        action="answer", id="17", answers={"choice": {"answers": ["A"]}}
                    )
                )
                peer.answer.assert_called_once()

    def test_session_metadata_is_private_and_does_not_copy_messages(self):
        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                manager.messages = [
                    dict(id="a", role="assistant", text="Private conversation body")
                ]
                manager.save()
                self.assertNotIn("Private conversation body", manager.path.read_text())
                self.assertEqual(manager.path.stat().st_mode & 0o777, 0o600)

    def test_workspace_releases_peer_before_launch_and_blocks_reconnect(self):
        from unittest.mock import Mock

        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                manager.metadata.update(id="fixture-thread", title="Fixture")
                peer = Mock()
                manager.peer = peer
                events = []
                peer.close.side_effect = lambda: events.append("closed")
                launch = Mock(
                    side_effect=lambda *args, **kwargs: events.append("launched")
                )
                with patch.object(module.subprocess, "Popen", launch):
                    manager.handle(dict(action="workspace"))
                self.assertEqual(events, ["closed", "launched"])
                self.assertEqual(
                    launch.call_args.args[0][1:3], ["desktop-extras", "resume"]
                )
                self.assertIsNone(manager.peer)
                self.assertTrue(manager.state()["inWorkspace"])
                with self.assertRaises(RuntimeError):
                    manager.ensure_peer()
                manager.handle(dict(action="send", text="Must not reopen"))
                self.assertFalse(manager.busy)
                manager.handle(dict(action="new"))
                self.assertFalse(manager.state()["inWorkspace"])

    def test_busy_workspace_handoff_does_not_stop_worker(self):
        from unittest.mock import Mock

        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                manager.metadata["id"] = "fixture"
                manager.peer = Mock()
                manager.busy = True
                manager.handle(dict(action="workspace"))
                manager.peer.close.assert_not_called()

    def test_provider_choice_changes_default_without_converting_current_chat(self):
        from unittest.mock import Mock

        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                setter = Mock()
                with patch.object(
                    __import__("nacre_ai_controller"),
                    "load",
                    return_value={"set_default_assistant": setter},
                ):
                    manager.handle(dict(action="provider", provider="claude"))
                    setter.assert_called_once_with("claude")
                    manager.handle(dict(action="provider", provider="unexpected"))
                    self.assertEqual(setter.call_count, 1)
                self.assertEqual(manager.metadata["agent"], "codex")

    def test_error_messages_do_not_echo_credentials_or_prompts(self):
        self.assertEqual(
            module.friendly_error("Invalid secret_key sensitive-value"),
            "The assistant could not finish this request. Try again or open AI settings.",
        )

    def test_busy_codex_followup_steers_same_turn_and_deduplicates_native_echo(self):
        from unittest.mock import Mock

        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                manager.metadata.update(id="fixture", title="Fixture")
                manager.busy = True
                peer = module.CodexPeer.__new__(module.CodexPeer)
                peer.turn = "active-turn"
                peer.metadata = manager.metadata
                peer.call = Mock(return_value={"turnId": "active-turn"})
                manager.peer = peer
                manager.send("Correction one")
                manager.send("Correction two")
                self.assertEqual(
                    [c.args[0] for c in peer.call.call_args_list],
                    ["turn/steer", "turn/steer"],
                )
                self.assertEqual(
                    peer.call.call_args.args[1]["expectedTurnId"], "active-turn"
                )
                manager.native_user("native-one", "Correction one")
                manager.native_user("native-two", "Correction two")
                self.assertEqual(len(manager.messages), 2)
                self.assertTrue(manager.busy)
                self.assertEqual(manager.pending_users, [])

    def test_busy_send_is_accepted_at_command_boundary(self):

        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                manager.busy = True
                with patch.object(module.threading, "Thread") as worker:
                    manager.handle({"action": "send", "text": "While working"})
                    worker.assert_called_once()
                    worker.return_value.start.assert_called_once()

    def test_completed_turn_race_starts_followup_once(self):
        from unittest.mock import Mock

        peer = module.CodexPeer.__new__(module.CodexPeer)
        peer.metadata = {"id": "fixture"}
        peer.turn = "old"
        peer.send = Mock()

        def complete(*args):
            peer.turn = ""
            raise RuntimeError("No active turn")

        peer.call = Mock(side_effect=complete)
        peer.steer("Follow up")
        peer.send.assert_called_once_with("Follow up")

    def test_failed_steer_restores_draft_without_marking_active_turn_finished(self):
        from unittest.mock import Mock

        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                manager.metadata.update(id="fixture", title="Fixture")
                manager.busy = True
                peer = module.CodexPeer.__new__(module.CodexPeer)
                peer.turn = "active"
                peer.metadata = manager.metadata
                peer.call = Mock(side_effect=RuntimeError("Unavailable"))
                manager.peer = peer
                manager.broadcast = Mock()
                manager.send("Keep this draft")
                self.assertTrue(manager.busy)
                self.assertEqual(manager.messages, [])
                self.assertEqual(manager.pending_users, [])
                self.assertTrue(
                    any(
                        c.args[0].get("type") == "sendFailed"
                        and c.args[0]["text"] == "Keep this draft"
                        for c in manager.broadcast.call_args_list
                    )
                )

    def test_async_questions_survive_finish_and_answer_with_active_turn_steer(self):
        from unittest.mock import Mock

        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                manager.metadata.update(id="fixture", title="Fixture")
                manager.busy = True
                peer = module.CodexPeer.__new__(module.CodexPeer)
                peer.turn = "active-turn"
                peer.metadata = manager.metadata
                peer.call = Mock()
                manager.peer = peer
                manager.ask_async(
                    "question-card", [{"title": "Which option?", "options": ["A", "B"]}]
                )
                manager.ask_async("question-card", [{"title": "Which option?"}])
                self.assertEqual(len(manager.async_questions), 1)
                self.assertEqual(
                    manager.state()["question"]["questions"][0]["options"][0]["label"],
                    "A",
                )
                manager.finish()
                self.assertIsNotNone(manager.question)
                manager.handle(
                    {"action": "answer", "id": "async-question-card", "answers": {}}
                )
                self.assertIsNotNone(manager.question)
                with patch.object(module.threading, "Thread") as worker:
                    manager.handle(
                        {
                            "action": "answer",
                            "id": "async-question-card",
                            "answers": {"0": {"answers": ["B"]}},
                        }
                    )
                    self.assertEqual(
                        worker.call_args.kwargs["args"], ("Which option?\nAnswer: B",)
                    )
                self.assertIsNone(manager.question)
                manager.ask_async("question-card", [{"title": "Which option?"}])
                self.assertIsNone(manager.question)

    def test_async_questions_restore_only_since_last_user_message(self):
        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                manager.restore(
                    [
                        {
                            "items": [
                                {
                                    "id": "old",
                                    "type": "agentMessage",
                                    "text": "",
                                    "questions": [{"title": "Old question"}],
                                },
                                {
                                    "id": "answer",
                                    "type": "userMessage",
                                    "content": [{"type": "text", "text": "Answered"}],
                                },
                                {
                                    "id": "new",
                                    "type": "agentMessage",
                                    "text": "",
                                    "questions": [{"title": "New question"}],
                                },
                            ]
                        }
                    ]
                )
                self.assertEqual(
                    manager.question["questions"][0]["question"], "New question"
                )
                self.assertEqual(len(manager.async_questions), 1)

    def test_async_question_events_and_blocking_questions_coexist(self):
        from unittest.mock import Mock

        with tempfile.TemporaryDirectory() as directory:
            p, q = self.manager(Path(directory))
            with p, q:
                manager = module.Manager()
                peer = module.CodexPeer.__new__(module.CodexPeer)
                peer.manager = manager
                peer.generation = manager.generation
                peer.metadata = manager.metadata
                peer.event(
                    {
                        "method": "item/completed",
                        "params": {
                            "item": {
                                "id": "card",
                                "type": "agentMessage",
                                "text": "",
                                "questions": [
                                    {"title": "Please choose", "options": ["A"]}
                                ],
                            }
                        },
                    }
                )
                manager.ask(
                    Mock(),
                    3,
                    [{"id": "blocking", "question": "Blocking question"}],
                    "input",
                )
                self.assertEqual(manager.question["kind"], "input")
                manager.handle(
                    {
                        "action": "answer",
                        "id": "3",
                        "answers": {"blocking": {"answers": ["Yes"]}},
                    }
                )
                self.assertEqual(manager.question["kind"], "async")


if __name__ == "__main__":
    unittest.main()
