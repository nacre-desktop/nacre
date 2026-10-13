import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch


def load():
    path = Path(__file__).parents[1] / "window-chat-title.py"
    spec = importlib.util.spec_from_file_location("window_chat_title", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class WindowChatTitleTests(unittest.TestCase):
    def test_new_chat_resolves_from_named_metadata_without_resume_argument(self):
        helper = load()
        with tempfile.TemporaryDirectory() as home, patch.dict(os.environ, HOME=home):
            cwd = str(Path(home) / ".local/share/nacre/ai/chats" / ("a" * 32))
            cache = Path(home) / ".local/state/nacre/ai/chat-titles"
            cache.mkdir(parents=True)
            (cache / "11111111-1111-1111-1111-111111111111.json").write_text(
                json.dumps({"title": "Fix Niri Titles", "cwd": cwd})
            )
            proc = Path(home) / "proc/123"
            (proc / "task/123").mkdir(parents=True)
            (proc / "task/123/children").write_text("")
            (proc / "cmdline").write_bytes(f"/usr/bin/codex\0-C\0{cwd}\0".encode())
            with (
                patch.object(helper, "PROC", proc.parent),
                patch.object(helper.os, "write") as publish,
            ):
                self.assertEqual(helper.resolve_info(123)["title"], "Fix Niri Titles")
                publish.assert_not_called()

    def test_shared_project_directory_and_ambiguous_chat_never_guess_identity(self):
        helper = load()
        with tempfile.TemporaryDirectory() as home, patch.dict(os.environ, HOME=home):
            cache = Path(home) / ".local/state/nacre/ai/chat-titles"
            cache.mkdir(parents=True)
            cwd = str(Path(home) / ".local/share/nacre/ai/chats" / ("a" * 32))
            for name in ("one", "two"):
                (cache / f"{name}.json").write_text(
                    json.dumps({"title": name, "cwd": cwd})
                )
            self.assertEqual(helper.cached_info("", cwd)["title"], "")
            self.assertEqual(helper.cached_info("", "/repo/project")["title"], "")

    def test_resumed_chat_uses_exact_id_and_strips_terminal_control_sequences(self):
        helper = load()
        thread = "11111111-1111-1111-1111-111111111111"
        with tempfile.TemporaryDirectory() as home, patch.dict(os.environ, HOME=home):
            cache = Path(home) / ".local/state/nacre/ai/chat-titles"
            cache.mkdir(parents=True)
            (cache / f"{thread}.json").write_text(
                json.dumps({"title": "Named\x1b\x07\n chat"})
            )
            self.assertEqual(helper.cached_info(thread, "")["title"], "Named chat")
            self.assertEqual(
                helper.chat_identity(["/usr/bin/codex", "resume", thread]),
                (thread, ""),
            )
            self.assertEqual(
                helper.chat_identity(["/usr/bin/echo", "resume", thread]), ("", "")
            )
