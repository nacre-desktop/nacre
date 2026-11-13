"""Settings activation shares one shell and never shells out page arguments."""

import json
import importlib.machinery
import importlib.util
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tools"))
import ui_shell_switch

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
            runner.call_args.args[0],
            ["nacrectl", "ui", "nacre", "settings", "sound"],
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

        def no_launcher(_home):
            return None

        registry = settings.navigation(home=ROOT, launcher_resolver=no_launcher)
        for page in [row["id"] for row in registry["pages"]] + list(
            registry["aliases"]
        ):
            self.assertTrue(
                settings.valid_route(page, home=ROOT, launcher_resolver=no_launcher)
            )
        self.assertTrue(
            settings.valid_route(
                "panels/dashboard#pages", home=ROOT, launcher_resolver=no_launcher
            )
        )
        for route in ["../desktop", "missing", "colors;bad", "panels#x$(id)"]:
            self.assertFalse(
                settings.valid_route(route, home=ROOT, launcher_resolver=no_launcher)
            )

    def test_source_install_uses_active_snapshot_in_temporary_home(self):
        with tempfile.TemporaryDirectory() as temporary:
            home = Path(temporary)
            builds = home / ".local/share/nacre/bead"
            active = builds / "aaa-active" / "launch"
            newer = builds / "zzz-newer" / "launch"
            active.parent.mkdir(parents=True)
            newer.parent.mkdir(parents=True)
            active.touch()
            newer.touch()

            for launcher, page in ((active, "source-active"), (newer, "source-newer")):
                registry = launcher.parent / "shell/settings/registry/navigation.json"
                registry.parent.mkdir(parents=True)
                registry.write_text(
                    json.dumps({"pages": [{"id": page}], "aliases": {}})
                )

            env = {
                "XDG_CONFIG_HOME": str(home / ".config"),
                "XDG_STATE_HOME": str(home / ".local/state"),
            }
            exec_start = (
                f"{{ path={active}; argv[]={active} start ; ignore_errors=no ; }}"
            )
            with (
                patch.object(ui_shell_switch.UiShell, "loaded", return_value=True),
                patch.object(
                    ui_shell_switch.UiShell,
                    "props",
                    return_value={"ExecStart": exec_start},
                ),
            ):

                def resolve(selected_home):
                    return settings.active_launcher(selected_home, environ=env)

                self.assertTrue(
                    settings.valid_route(
                        "source-active", home=home, launcher_resolver=resolve
                    )
                )
                self.assertFalse(
                    settings.valid_route(
                        "source-newer", home=home, launcher_resolver=resolve
                    )
                )
