import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from test_workflow import module, ROOT


class Conversation(unittest.TestCase):
    def test_context_exposes_build_routing_and_ambiguity_without_running(self):
        ctx = module("nacre-ai-context")
        with tempfile.TemporaryDirectory() as tmp:
            registry = Path(tmp) / "registry.json"
            registry.write_text(
                json.dumps(
                    {
                        "projects": [
                            {
                                "id": "game",
                                "label": "First game",
                                "aliases": ["camera"],
                                "path": "/repo",
                                "build_host": "dev-1",
                                "build_path": "/build",
                            },
                            {"id": "other", "path": "/other"},
                        ]
                    }
                )
            )
            with patch.dict(os.environ, {"NACRE_AI_PROJECTS": str(registry)}):
                one = ctx.resolve("fix camera")
                self.assertEqual(one["matched"][0]["build_host"], "dev-1")
                self.assertFalse(one["ambiguous"])
                self.assertTrue(ctx.resolve("game and other")["ambiguous"])
                self.assertEqual(ctx.resolve("what is bouldering")["matched"], [])

    def test_capture_sanitizes_native_input(self):
        usage = module("nacre-ai-usage")
        data = usage.snapshot(
            {
                "rate_limits": {
                    "five_hour": {
                        "used_percentage": 25,
                        "resets_at": 1700000000,
                        "secret": "not retained",
                    }
                },
                "cost": {"total_cost_usd": 1.2},
                "transcript_path": "private",
                "api_key": "private",
            }
        )
        self.assertNotIn("private", json.dumps(data))
        self.assertNotIn("secret", json.dumps(data))
        self.assertEqual(data["five_hour"]["used_percentage"], 25)

    def test_claude_missing_usage_is_explicit_and_account_scoped(self):
        usage = module("nacre-ai-usage")
        with (
            tempfile.TemporaryDirectory() as tmp,
            patch.dict(os.environ, {"HOME": tmp}),
        ):
            self.assertIn("Not yet reported", usage.read("claude", "work")["rows"][1])
            self.assertNotEqual(usage.cache("work"), usage.cache(""))
            with self.assertRaises(ValueError):
                usage.cache("../escape")

    def test_memory_creates_topics_worlds_and_deduplicates_evidence(self):
        with tempfile.TemporaryDirectory() as tmp:
            env = dict(os.environ, HOME=tmp, NACRE_BRAIN=tmp + "/brain")
            command = [
                sys.executable,
                str(ROOT / "bin/nacre-ai-memory"),
                "--title",
                "A useful result",
                "--source",
                "user statement",
                "--confidence",
                "reported",
                "--world",
                "Climbing",
                "--world",
                "Personal",
                "--topic",
                "Training",
            ]
            first = subprocess.run(
                command,
                input="Prefers bouldering.",
                text=True,
                capture_output=True,
                env=env,
                check=True,
            )
            second = subprocess.run(
                command,
                input="Prefers bouldering.",
                text=True,
                capture_output=True,
                env=env,
                check=True,
            )
            self.assertEqual(
                json.loads(first.stdout)["path"], json.loads(second.stdout)["path"]
            )
            root = Path(tmp) / "brain"
            self.assertEqual(len(list((root / "wiki").glob("*.md"))), 2)
            body = Path(json.loads(first.stdout)["path"]).read_text()
            self.assertIn("Worlds: Climbing, Personal", body)
            self.assertIn("Topics: Training", body)
            refused = subprocess.run(
                command,
                input="password: hunter2",
                text=True,
                capture_output=True,
                env=env,
            )
            self.assertNotEqual(refused.returncode, 0)

    def test_memory_subject_creation_is_independent_of_coding_registry(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "brain"
            registry = Path(tmp) / "projects.json"
            registry.write_text(
                json.dumps(
                    {
                        "projects": [
                            {
                                "id": "gardensense",
                                "label": "GardenSense",
                                "path": "/repo",
                            }
                        ]
                    }
                )
            )
            subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "bin/nacre-ai-memory"),
                    "--title",
                    "A useful observation",
                    "--source",
                    "user",
                    "--world",
                    "GardenSense",
                ],
                input="Soil moisture readings are useful.",
                text=True,
                capture_output=True,
                env=dict(
                    os.environ,
                    HOME=tmp,
                    NACRE_BRAIN=str(root),
                    NACRE_AI_PROJECTS=str(registry),
                ),
                check=True,
            )
            self.assertTrue((root / "wiki/entity-gardensense.md").exists())

    def test_memory_existing_alias_preserves_curated_page(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp) / "brain"
            (root / "wiki").mkdir(parents=True)
            page = root / "wiki/existing.md"
            original = "# My subject\nEntity: world\nName: Climbing\nAliases: Bouldering\n\nCurated evidence\n"
            page.write_text(original)
            subprocess.run(
                [
                    sys.executable,
                    str(ROOT / "bin/nacre-ai-memory"),
                    "--title",
                    "Training",
                    "--source",
                    "user",
                    "--world",
                    "Bouldering",
                ],
                input="A preference.",
                text=True,
                capture_output=True,
                env=dict(os.environ, HOME=tmp, NACRE_BRAIN=str(root)),
                check=True,
            )
            self.assertEqual(page.read_text(), original)
            self.assertFalse((root / "wiki/entity-bouldering.md").exists())


if __name__ == "__main__":
    unittest.main()


class OwnedChatTrust(unittest.TestCase):
    def test_only_created_chat_is_trusted_and_other_settings_survive(self):
        ai = module("nacre-ai")
        with (
            tempfile.TemporaryDirectory() as tmp,
            patch.dict(os.environ, {"HOME": tmp}),
        ):
            home = Path(tmp)
            chat = home / ".local/share/nacre/ai/chats" / ("a" * 32)
            chat.mkdir(parents=True)
            (chat / "AGENTS.md").write_text("Owned launcher guidance")
            config = home / ".codex/config.toml"
            config.parent.mkdir()
            original = (
                'model = "keep"\n[projects."/unrelated"]\ntrust_level = "untrusted"\n'
            )
            config.write_text(original)
            ai.trust_created_chat(chat, {"CODEX_HOME": str(config.parent)})
            data = __import__("tomllib").loads(config.read_text())
            self.assertEqual(data["model"], "keep")
            self.assertEqual(data["projects"]["/unrelated"]["trust_level"], "untrusted")
            self.assertEqual(data["projects"][str(chat)]["trust_level"], "trusted")
            with self.assertRaises(ValueError):
                ai.trust_created_chat(home, {"CODEX_HOME": str(config.parent)})
            other = chat.with_name("b" * 32)
            other.mkdir()
            (other / "AGENTS.md").write_text("x")
            (other / "unexpected").write_text("x")
            with self.assertRaises(ValueError):
                ai.trust_created_chat(other, {"CODEX_HOME": str(config.parent)})
