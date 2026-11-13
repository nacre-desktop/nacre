import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from test_workflow import ROOT


class ClaudeWorkflowTests(unittest.TestCase):
    def test_title_hook_uses_latest_summary_without_ai_title(self):
        hook = ROOT / "bin/nacre-ai-claude-title-hook"
        with tempfile.TemporaryDirectory() as temp:
            transcript = Path(temp) / "session.jsonl"
            transcript.write_text(
                "\n".join(
                    json.dumps(row)
                    for row in [
                        {"type": "summary", "summary": "Old summary"},
                        {"type": "summary", "summary": "Latest summary"},
                    ]
                )
            )
            result = subprocess.run(
                [sys.executable, str(hook)],
                input=json.dumps({"transcript_path": str(transcript)}),
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(
                json.loads(result.stdout)["hookSpecificOutput"]["sessionTitle"],
                "Latest summary",
            )

    def test_title_hook_prefers_ai_title_and_preserves_custom_title(self):
        hook = ROOT / "bin/nacre-ai-claude-title-hook"
        with tempfile.TemporaryDirectory() as temp:
            transcript = Path(temp) / "session.jsonl"
            transcript.write_text(
                json.dumps({"type": "user", "message": {"content": "First prompt"}})
                + "\n"
                + json.dumps({"type": "ai-title", "aiTitle": "Old AI title"})
                + "\n"
                + json.dumps({"type": "ai-title", "aiTitle": "AI summary title"})
                + "\n"
                + json.dumps({"type": "summary", "summary": "Newer summary"})
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
                stream.write(
                    json.dumps({"type": "custom-title", "customTitle": "Owner rename"})
                )
            result = subprocess.run(
                [sys.executable, str(hook)], input=event, text=True, capture_output=True
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")

    def test_title_hook_waits_for_ai_title_and_fails_open(self):
        hook = ROOT / "bin/nacre-ai-claude-title-hook"
        with tempfile.TemporaryDirectory() as temp:
            transcript = Path(temp) / "session.jsonl"
            transcript.write_text(
                json.dumps(
                    {"type": "user", "message": {"content": "First prompt text"}}
                )
            )
            event = json.dumps(
                {
                    "prompt": "  Explain\x00   tmux persistence.  ",
                    "transcript_path": str(transcript),
                }
            )
            result = subprocess.run(
                [sys.executable, str(hook)], input=event, text=True, capture_output=True
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")
            result = subprocess.run(
                [sys.executable, str(hook)],
                input="not json",
                text=True,
                capture_output=True,
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

    def test_title_hook_skips_excessive_record_count(self):
        hook = ROOT / "bin/nacre-ai-claude-title-hook"
        with tempfile.TemporaryDirectory() as temp:
            transcript = Path(temp) / "session.jsonl"
            transcript.write_text("{}\n" * 5000)
            result = subprocess.run(
                [sys.executable, str(hook)],
                input=json.dumps(
                    {"prompt": "Title", "transcript_path": str(transcript)}
                ),
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")

    def test_title_hook_ignores_synthetic_prompt_and_meta_records(self):
        hook = ROOT / "bin/nacre-ai-claude-title-hook"
        with tempfile.TemporaryDirectory() as temp:
            transcript = Path(temp) / "session.jsonl"
            transcript.write_text(
                json.dumps(
                    {
                        "type": "user",
                        "message": {
                            "content": "<local-command-stdout>output</local-command-stdout>"
                        },
                    }
                )
                + "\n"
                + json.dumps(
                    {
                        "type": "user",
                        "isMeta": True,
                        "message": {"content": "do not use this"},
                    }
                )
                + "\n"
            )
            event = json.dumps(
                {
                    "prompt": "<local-command-stdout>output</local-command-stdout>",
                    "transcript_path": str(transcript),
                }
            )
            result = subprocess.run(
                [sys.executable, str(hook)], input=event, text=True, capture_output=True
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")

    def test_title_hook_fails_open_on_malformed_transcript_and_oversized_event(self):
        hook = ROOT / "bin/nacre-ai-claude-title-hook"
        with tempfile.TemporaryDirectory() as temp:
            transcript = Path(temp) / "session.jsonl"
            transcript.write_text(
                json.dumps({"type": "user", "message": {"content": "First prompt"}})
                + "\n{malformed custom-title metadata\n"
            )
            event = json.dumps(
                {"prompt": "Later prompt", "transcript_path": str(transcript)}
            )
            result = subprocess.run(
                [sys.executable, str(hook)], input=event, text=True, capture_output=True
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")
            result = subprocess.run(
                [sys.executable, str(hook)],
                input=" " * (64 * 1024 + 1),
                text=True,
                capture_output=True,
            )
            self.assertEqual(result.returncode, 0)
            self.assertEqual(result.stdout, "")
