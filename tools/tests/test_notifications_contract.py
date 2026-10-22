"""Validate the exported notification contract as an external JSON consumer."""

import json
from pathlib import Path
import unittest

from jsonschema import Draft202012Validator

ROOT = Path(__file__).resolve().parents[2]


def validator(name):
    schema = json.loads((ROOT / "protocol/schema" / (name + ".json")).read_text())
    Draft202012Validator.check_schema(schema)
    return Draft202012Validator(schema)


class NotificationsContractTests(unittest.TestCase):
    def test_goldens_use_published_wire_and_event_schemas(self):
        wire = validator("message")
        event = validator("notification-event")
        state = validator("core-service-state")
        fixtures = sorted((ROOT / "protocol/fixtures").glob("notifications-*.jsonl"))
        self.assertEqual(len(fixtures), 6)
        for fixture in fixtures:
            for line in fixture.read_text().splitlines():
                frame = json.loads(line)
                with self.subTest(fixture=fixture.name, frame=frame):
                    wire.validate(frame)
                    if frame["t"] == "snap":
                        state.validate({"topic": frame["topic"], "data": frame["data"]})
                    elif frame["t"] == "ev":
                        event.validate({"name": frame["name"], "data": frame["data"]})
                        self.assertLessEqual(
                            set(frame["data"]),
                            {
                                "id",
                                "revision",
                                "action",
                                "reason",
                                "enabled",
                                "state_seq",
                            },
                        )

    def test_method_schema_rejects_unsafe_revision_and_extra_parameters(self):
        calls = validator("core-service-call")
        valid = {
            "m": "notifications.dismiss",
            "p": {"id": "notice:boot:1:1", "revision": 1, "forget": True},
        }
        calls.validate(valid)
        self.assertFalse(
            calls.is_valid({"m": valid["m"], "p": {**valid["p"], "forget": None}})
        )
        for revision in (0, -1, 9007199254740992, 1.5, "1"):
            bad = {"m": valid["m"], "p": {**valid["p"], "revision": revision}}
            with self.subTest(revision=revision):
                self.assertFalse(calls.is_valid(bad))
        for key in ("path", "destination", "command"):
            bad = {"m": valid["m"], "p": {**valid["p"], key: "untrusted"}}
            with self.subTest(key=key):
                self.assertFalse(calls.is_valid(bad))
        for scope in ("history", "retained", "popups"):
            calls.validate({"m": "notifications.clear", "p": {"scope": scope}})
        self.assertFalse(
            calls.is_valid({"m": "notifications.clear", "p": {"scope": "all"}})
        )

    def test_event_schema_rejects_unknown_reason_and_unsafe_sequences(self):
        events = validator("notification-event")
        for reason in ("expired", "dismissed", "application", "other"):
            events.validate(
                {
                    "name": "closed",
                    "data": {"id": "n1", "reason": reason, "state_seq": 0},
                }
            )
        self.assertFalse(
            events.is_valid(
                {
                    "name": "closed",
                    "data": {"id": "n1", "reason": "secret", "state_seq": 1},
                }
            )
        )
        for seq in (-1, 9007199254740992, 0.5):
            self.assertFalse(
                events.is_valid({"name": "history_changed", "data": {"state_seq": seq}})
            )
