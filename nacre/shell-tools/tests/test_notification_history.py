import importlib.util
import tempfile
import unittest
import json
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "history", Path(__file__).resolve().parents[1] / "notification-history.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class HistoryTests(unittest.TestCase):
    def test_private_roundtrip_keeps_only_display_fields_and_clear_is_durable(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory) / "notifications/history.json"
            row = dict(
                key="fixture",
                time="2026-10-03T21:00:00Z",
                summary="Test",
                body="Body",
                appName="Fixture",
                actions=["stale native action"],
                notification=5,
            )
            m.save([row], p)
            loaded = m.load(p)
            self.assertEqual(loaded[0]["body"], "Body")
            self.assertNotIn("actions", loaded[0])
            self.assertNotIn("notification", loaded[0])
            self.assertEqual(p.stat().st_mode & 0o777, 0o600)
            self.assertEqual(p.parent.stat().st_mode & 0o777, 0o700)
            m.save([], p)
            self.assertEqual(m.load(p), [])

    def test_feedback_is_removed_but_messages_and_completion_are_kept(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "history.json"
            rows = [
                dict(key="shot", appName="nacre_shell-cli", summary="Screenshot saved"),
                dict(
                    key="trash", appName="notify-send", summary="Window moved to trash"
                ),
                dict(
                    key="transient", appName="Desktop", summary="Volume", transient=True
                ),
                dict(key="message", appName="Discord", summary="Screenshot saved"),
                dict(key="done", appName="Nacre AI", summary="Task complete"),
            ]
            path.write_text(json.dumps(rows))
            self.assertEqual([r["key"] for r in m.load(path)], ["message", "done"])
            m.save(rows, path)
            self.assertEqual([r["key"] for r in m.load(path)], ["message", "done"])

    def test_invalid_input_cannot_overwrite_existing_history(self):
        with tempfile.TemporaryDirectory() as directory:
            p = Path(directory) / "notifications/history.json"
            m.save([dict(key="saved", summary="Kept")], p)
            before = p.read_bytes()
            for bad in ({}, [None]):
                with self.assertRaises(ValueError):
                    m.save(bad, p)
                self.assertEqual(p.read_bytes(), before)


if __name__ == "__main__":
    unittest.main()
