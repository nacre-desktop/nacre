import importlib.util
import unittest
import tempfile
from unittest.mock import patch
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "startup", Path(__file__).resolve().parents[1] / "startup-apps.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class StartupTests(unittest.TestCase):
    def test_generic_profile_starts_no_personal_apps(self):
        self.assertEqual(m.plan([], {"version": 1}), [])

    def test_empty_home_startup_has_no_process_or_workspace_side_effects(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(m, "HOME", Path(directory)),
                patch.object(
                    m.Path, "open", side_effect=AssertionError("Unexpected lock")
                ),
                patch.object(m.subprocess, "check_output") as query,
                patch.object(m.subprocess, "run") as dispatch,
                patch.object(m.subprocess, "Popen") as launch,
            ):
                m.main()
                query.assert_not_called()
                dispatch.assert_not_called()
                launch.assert_not_called()

    def test_configured_apps_keep_workspaces_and_do_not_launch_twice(self):
        profile = {
            "version": 1,
            "startup": [
                {
                    "label": "Browser",
                    "workspace": 3,
                    "argv": ["example-browser"],
                    "classes": ["Example-browser"],
                },
                {
                    "label": "Brain",
                    "workspace": 2,
                    "argv": ["nacre-brain-ui"],
                    "classes": ["nacre-brain"],
                },
            ],
        }
        rows = m.plan([], profile)
        self.assertEqual(
            [(r[0], r[1], r[3]) for r in rows],
            [("Browser", 3, False), ("Brain", 2, False)],
        )
        clients = [
            {"class": "example-browser", "title": ""},
            {"class": "chrome-127.0.0.1__-Default", "title": "Nacre Brain"},
        ]
        self.assertTrue(all(r[3] for r in m.plan(clients, profile)))
