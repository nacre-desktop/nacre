"""Host-independent menu discovery, package arguments and owned tool lifecycle."""

import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "menu_backend", Path(__file__).parents[1] / "menu-backend.py"
)
menu = importlib.util.module_from_spec(spec)
spec.loader.exec_module(menu)


class MenuBackendTests(unittest.TestCase):
    def test_support_report_copies_only_the_redacted_markdown(self):
        import sys
        from types import SimpleNamespace

        with (
            patch.dict(
                sys.modules,
                {
                    "diagnostics": SimpleNamespace(
                        support_report=lambda: "# Redacted fixture\n"
                    )
                },
            ),
            patch.object(menu.subprocess, "run") as run,
        ):
            self.assertEqual(menu.tool("support-report"), "Support report copied")
        self.assertEqual(
            run.call_args.args[0], ["wl-copy", "--type", "text/plain;charset=utf-8"]
        )
        self.assertEqual(run.call_args.kwargs["input"], "# Redacted fixture\n")
        self.assertTrue(run.call_args.kwargs["check"])
        self.assertEqual(run.call_args.kwargs["timeout"], 5)

    def test_files_are_scoped_and_query_is_literal(self):
        import subprocess

        (menu.HOME / "Documents").mkdir()
        target = menu.HOME / "Documents/report.pdf"
        target.touch()
        with (
            patch.object(menu.shutil, "which", return_value="fd"),
            patch.object(
                menu,
                "run",
                return_value=subprocess.CompletedProcess(
                    [], 0, str(target) + "\0/etc/passwd\0", ""
                ),
            ) as run,
        ):
            result = menu.search_files("report")
            self.assertEqual([row["path"] for row in result["rows"]], [str(target)])
            self.assertIn("--fixed-strings", run.call_args.args[0])
        with patch.object(menu, "run") as run:
            with self.assertRaises(ValueError):
                menu.search_files("bad\nname")
            run.assert_not_called()

    def test_conversions_are_bounded_and_never_update_rates_implicitly(self):
        import subprocess

        with (
            patch.object(menu.shutil, "which", return_value="qalc"),
            patch.object(
                menu.subprocess,
                "run",
                return_value=subprocess.CompletedProcess([], 0, "6.2137 mi", ""),
            ) as run,
        ):
            self.assertTrue(menu.convert_units("10 km to miles")["ok"])
            self.assertIn("update exchange rates 0", run.call_args.args[0])
            self.assertEqual(run.call_args.kwargs["stdin"], subprocess.DEVNULL)
        with patch.object(menu.subprocess, "run") as run:
            self.assertFalse(menu.convert_units("10 km to miles; injected")["ok"])
            run.assert_not_called()

    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        home = Path(directory.name)
        for name, value in (
            ("HOME", home),
            ("STATE", home / "state"),
            ("CACHE", home / "cache"),
        ):
            guard = patch.object(menu, name, value)
            guard.start()
            self.addCleanup(guard.stop)

    def test_package_names_cannot_be_options_or_shell_fragments(self):
        for value in ("--help", "foo;rm -rf /", "foo\nbar", "", "../file", None):
            with self.subTest(value=value), self.assertRaises(ValueError):
                menu.package_name(value)
        self.assertEqual(menu.package_name("lib32-mesa"), "lib32-mesa")

    def test_install_uses_interactive_manager_and_existing_skipreview_policy(self):
        with patch.object(
            menu.shutil,
            "which",
            side_effect=lambda name: "/usr/bin/paru" if name == "paru" else None,
        ):
            command = menu.package_command("install", "neovim")
            self.assertEqual(
                command,
                ["/usr/bin/paru", "-S", "--needed", "--skipreview", "--", "neovim"],
            )
            self.assertNotIn("--noconfirm", command)
            self.assertEqual(
                menu.package_command("remove", "neovim"),
                ["/usr/bin/paru", "-Rns", "--", "neovim"],
            )

    def test_pacman_fallback_keeps_authorization_and_confirmation(self):
        with patch.object(
            menu.shutil,
            "which",
            side_effect=lambda name: "/usr/bin/pacman" if name == "pacman" else None,
        ):
            self.assertEqual(
                menu.package_command("install", "btop"),
                ["pkexec", "pacman", "-S", "--needed", "--", "btop"],
            )

    def test_package_commit_invalidates_capabilities_without_waiting_for_cache(self):
        with (
            patch.object(menu, "package_stamp", side_effect=[1, 2]),
            patch.object(menu, "installed", return_value=[]),
            patch.object(menu, "fonts", return_value=[]),
            patch.object(menu, "active", return_value=False),
            patch.object(menu, "shutil") as utils,
        ):
            utils.which.return_value = None
            first = menu.snapshot(True)
            self.assertFalse(first["capabilities"]["record"])
            utils.which.return_value = "/usr/bin/tool"
            second = menu.snapshot()
            self.assertTrue(second["capabilities"]["record"])
            self.assertEqual(second["packageStamp"], 2)

    def test_snapshot_cache_avoids_discovery_on_repeated_queries(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            with (
                patch.object(menu, "CACHE", base),
                patch.object(menu, "installed", return_value=[]) as packages,
                patch.object(menu, "fonts", return_value=["IBM Plex Sans"]) as fonts,
                patch.object(menu, "active", return_value=False),
                patch.object(menu, "reminders", return_value=[]),
                patch.object(menu, "webapps", return_value=[]),
            ):
                first = menu.snapshot(True)
                second = menu.snapshot()
                self.assertEqual(first, second)
                packages.assert_called_once()
                fonts.assert_called_once()
                self.assertEqual((base / "state.json").stat().st_mode & 0o777, 0o600)

    def test_webapp_id_drives_exec_never_the_entered_url(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            with (
                patch.object(menu, "HOME", home),
                patch.object(menu, "STATE", home / "state"),
                patch.object(menu, "CACHE", home / "cache"),
            ):
                menu.create_webapp("My site", "https://example.org/?q=$(touch%20x)")
                entry = next(
                    (home / ".local/share/applications").glob("*.desktop")
                ).read_text()
                self.assertIn("Exec=nacre-menu webapp-launch ", entry)
                self.assertNotIn("example.org", entry)
                self.assertNotIn("touch", entry)
                row = menu.webapps()[0]
                menu.remove_webapp(row["id"])
                self.assertEqual(menu.webapps(), [])
                self.assertEqual(
                    list((home / ".local/share/applications").glob("*.desktop")), []
                )

    def test_webapp_rejects_credentials_control_characters_and_nonweb_urls(self):
        for label, url in [
            ("Bad\nExec=x", "https://example.org"),
            ("Name", "file:///etc/passwd"),
            ("Name", "https://user:password@example.org"),
            ("Name", "javascript:alert(1)"),
        ]:
            with self.subTest(url=url), self.assertRaises(ValueError):
                menu.create_webapp(label, url)

    def test_stop_recording_never_signals_another_invocation(self):
        with (
            patch.object(menu, "unit_owner", return_value="7:new"),
            patch.object(menu, "read", return_value={"owner": "7:old"}),
            patch.object(menu, "run") as run,
        ):
            with self.assertRaisesRegex(ValueError, "another controller"):
                menu.record_stop()
            run.assert_not_called()

    def test_stay_awake_never_stops_an_external_owner(self):
        with (
            patch.object(menu, "unit_owner", return_value="7:new"),
            patch.object(menu, "read", return_value={}),
            patch.object(menu, "run") as run,
        ):
            with self.assertRaisesRegex(ValueError, "another controller"):
                menu.stay_awake()
            run.assert_not_called()

    def test_timer_cancel_checks_owned_metadata_and_unit_namespace(self):
        with (
            patch.object(menu, "reminders", return_value=[]),
            patch.object(menu, "run") as run,
        ):
            for unit in (
                "other.timer",
                "nacre-reminder-aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa.timer",
            ):
                with self.assertRaises(ValueError):
                    menu.cancel_reminder(unit)
            run.assert_not_called()

    def test_reminder_values_are_bounded_before_starting_a_unit(self):
        with patch.object(menu, "run") as run:
            for minutes in (-1, 0, float("nan"), 1441):
                with self.assertRaises(ValueError):
                    menu.set_reminder(minutes, "Reminder")
            run.assert_not_called()
