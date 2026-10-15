import importlib.util
import json
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

ROOT = Path(__file__).parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(
        name,
        ROOT
        / (
            "tests/native_session_watch.py" if name == "session-watch" else name + ".py"
        ),
    )
    m = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(m)
    return m


class SessionTests(unittest.TestCase):
    def test_readiness_records_lock_and_duplicates_without_claiming_cold_boot(self):
        m = load("session-watch")
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(m, "STATE", Path(folder)),
        ):

            def run(args, *rest, **kw):
                if args[0] == "busctl":
                    return "b true"
                if args[0] == "hyprctl":
                    return json.dumps([]) if args[1] != "configerrors" else ""
                return '{"launcherMode":"apps"}'

            with patch.object(m, "run", side_effect=run):
                value = m.check("manual")
            self.assertEqual(value["event"], "manual")
            self.assertEqual(value["wallet"], "locked")
            self.assertTrue(value["desktopReady"])

    def test_missing_audio_devices_are_not_applied(self):
        m = load("workflow-profiles")
        with (
            patch.object(
                m,
                "load",
                return_value={
                    "meeting": {
                        "sink": "unplugged",
                        "source": "gone",
                        "displays": [],
                        "roles": [],
                    }
                },
            ),
            patch.object(m, "devices", return_value={"sinks": [], "sources": []}),
            patch.object(m, "run", return_value="[]"),
            patch.object(m.compositor, "query", return_value=[]),
            patch.object(m.subprocess, "run") as action,
        ):
            m.apply("meeting")
            action.assert_not_called()
