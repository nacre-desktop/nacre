"""Frozen stream references protect readiness, ordering and hover negotiation."""

import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import freeze_niri_events as events
import freeze_niri_handshake as handshake


class NiriEventReferenceTests(unittest.TestCase):
    def test_source_guard_rejects_capture_before_execution(self):
        for reference in (events, handshake):
            with self.subTest(reference=reference.__name__):
                with tempfile.TemporaryDirectory() as folder:
                    with (
                        patch.object(
                            reference.reference, "SOURCE", Path(folder) / "absent.py"
                        ),
                        patch.object(reference.subprocess, "run") as run,
                    ):
                        with self.assertRaisesRegex(ValueError, "absent or changed"):
                            reference.capture(Path(folder) / "new.json")
                        run.assert_not_called()

    def test_capture_never_overwrites_either_reference(self):
        for reference in (events, handshake):
            with self.subTest(reference=reference.__name__):
                with tempfile.TemporaryDirectory() as folder:
                    destination = Path(folder) / "frozen.json"
                    destination.write_bytes(b"frozen\n")
                    with (
                        patch.object(reference.reference, "verify_source"),
                        patch.object(reference.subprocess, "run") as run,
                    ):
                        with self.assertRaisesRegex(
                            ValueError, "Refusing to overwrite"
                        ):
                            reference.capture(destination)
                        run.assert_not_called()
                    self.assertEqual(destination.read_bytes(), b"frozen\n")

    def corpus(self, reference, count):
        data = json.loads(reference.DESTINATION.read_text())
        self.assertEqual(data["source_sha256"], reference.reference.SOURCE_SHA)
        cases = {case["request"]["name"]: case for case in data["cases"]}
        self.assertEqual(len(cases), count)
        for case in cases.values():
            self.assertEqual(case["files"], {})
            self.assertEqual(case["stderr"], "")
        return cases

    def test_readiness_event_order_and_terminal_errors_are_preserved(self):
        cases = self.corpus(events, 11)
        self.assertEqual(sum(len(c["observed"]["emitted"]) for c in cases.values()), 45)
        self.assertEqual(cases["workspace-first-only"]["observed"]["emitted"], [])
        self.assertEqual(len(cases["windows-first"]["observed"]["emitted"]), 2)
        self.assertEqual(
            len(cases["multi-field-window-change"]["observed"]["emitted"]), 6
        )
        self.assertEqual(
            [
                json.loads(line)["type"]
                for line in cases["multi-field-window-change"]["observed"]["emitted"]
            ],
            [
                "snapshot",
                "windows-changed",
                "snapshot",
                "window-title",
                "window-workspace",
                "window-floating",
            ],
        )
        self.assertEqual(
            len(cases["outputs-changed-on-config"]["observed"]["queries"]), 3
        )
        for case in cases.values():
            observed = case["observed"]
            self.assertTrue(observed["connection_closed"])
            for line in observed["emitted"]:
                self.assertTrue(line.endswith("\n"))
                json.loads(line)
        self.assertEqual(
            cases["malformed-event"]["observed"]["error"],
            {"type": "ValueError", "error": "Malformed niri event payload"},
        )

    def test_negotiation_retains_first_event_and_closes_failed_connections(self):
        cases = self.corpus(handshake, 8)
        for name in (
            "hover-accepted",
            "stock-fallback",
            "hover-disabled",
            "unexpected-ok-fallback",
        ):
            observed = cases[name]["observed"]
            self.assertIsNone(observed["error"])
            self.assertFalse(observed["blocking"])
            self.assertEqual(
                bytes.fromhex(observed["remaining_hex"]).decode(), handshake.EVENT
            )
        for name in ("stock-fallback", "unexpected-ok-fallback"):
            observed = cases[name]["observed"]
            self.assertEqual(observed["selected"], 1)
            self.assertEqual(observed["closed"], [True, False])
            self.assertEqual(
                [json.loads(bytes.fromhex(w)) for w in observed["wire_hex"]],
                ["NacreOverviewHoverStream", "EventStream"],
            )
        for name in ("stock-rejected", "bad-json", "truncated", "oversized"):
            observed = cases[name]["observed"]
            self.assertIsNotNone(observed["error"])
            self.assertTrue(all(observed["closed"]))


if __name__ == "__main__":
    unittest.main()
