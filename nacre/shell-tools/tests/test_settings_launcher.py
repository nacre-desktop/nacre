"""Settings activation shares one shell and never shells out page arguments."""

import importlib.machinery
import importlib.util
from pathlib import Path
import subprocess
import unittest
from unittest.mock import Mock

ROOT = Path(__file__).resolve().parents[3]
loader = importlib.machinery.SourceFileLoader(
    "settings_launcher", str(ROOT / "bin/nacre-settings")
)
spec = importlib.util.spec_from_loader(loader.name, loader)
settings = importlib.util.module_from_spec(spec)
loader.exec_module(settings)


class SettingsLauncherTests(unittest.TestCase):
    def test_warm_open_is_one_ipc_without_service_restart(self):
        runner = Mock(return_value=subprocess.CompletedProcess([], 0, "", ""))
        settings.open_settings("sound", runner=runner)
        self.assertEqual(runner.call_count, 1)
        self.assertEqual(
            runner.call_args.args[0][-3:], ["settingsApp", "open", "sound"]
        )

    def test_cold_open_starts_service_once_and_retries(self):
        failed = subprocess.CompletedProcess([], 1, "", "")
        good = subprocess.CompletedProcess([], 0, "", "")
        runner = Mock(side_effect=[failed, good, failed, good])
        sleep = Mock()
        settings.open_settings("displays", runner=runner, sleep=sleep)
        self.assertEqual(
            runner.call_args_list[1].args[0],
            ["systemctl", "--user", "start", "--no-block", "nacre-shell.service"],
        )
        self.assertEqual(sleep.call_count, 2)
        self.assertEqual(runner.call_args.args[0][-1], "displays")

    def test_failure_is_bounded_and_reported(self):
        runner = Mock(return_value=subprocess.CompletedProcess([], 1, "", ""))
        with self.assertRaisesRegex(RuntimeError, "Could not start"):
            settings.open_settings(runner=runner, sleep=Mock())
        self.assertEqual(runner.call_count, 2)

    def test_desktop_entry_and_all_page_routes_exist(self):
        entry = (ROOT / "nacre/desktop/nacre-settings.desktop").read_text()
        self.assertIn("Exec=nacre-settings\n", entry)
        self.assertIn("Terminal=false", entry)
        registry = settings.navigation()
        for page in [row["id"] for row in registry["pages"]] + list(
            registry["aliases"]
        ):
            self.assertTrue(settings.valid_route(page))
        self.assertTrue(settings.valid_route("panels/dashboard#pages"))
        for route in ["../desktop", "missing", "colors;bad", "panels#x$(id)"]:
            self.assertFalse(settings.valid_route(route))
