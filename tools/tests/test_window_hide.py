"""Real hide/restore implementation against fake compositor data and old ledgers."""

import os
import subprocess
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).parents[2]
import native_window_hide as hide


def client(address, workspace=3, name="3"):
    return dict(address=address, workspace=dict(id=workspace, name=name))


class WindowHideTests(unittest.TestCase):
    def test_explicit_quiet_target_does_not_hide_the_active_other_window(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(
                    hide, "query", return_value=[client("0xaaa", 2), client("0xbbb", 7)]
                ),
                patch.object(hide, "dispatch") as dispatch,
                patch.object(hide, "notify") as notify,
            ):
                hide.action("hide", directory, window="0xbbb", quiet=True)
            dispatch.assert_called_once_with(
                "window.move", "0xbbb", workspace="special:hidden", follow=False
            )
            notify.assert_not_called()
            self.assertEqual((Path(directory) / "0xbbb").read_text().strip(), "7")

    def test_hide_writes_compatible_private_record_and_moves_only_active_window(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory) / "ledger"
            with (
                patch.object(hide, "query", return_value=client("0xaaa")),
                patch.object(hide, "dispatch") as dispatch,
                patch.object(hide, "notify"),
            ):
                hide.action("hide", folder)
            record = folder / "0xaaa"
            self.assertEqual(record.read_text(), "3\n")
            self.assertEqual(record.stat().st_mode & 0o777, 0o600)
            dispatch.assert_called_once_with(
                "window.move", "0xaaa", workspace="special:hidden", follow=False
            )

    def test_empty_or_already_hidden_window_does_not_move(self):
        for value in ({}, client("0xaaa", -99, "special:hidden")):
            with tempfile.TemporaryDirectory() as directory:
                with (
                    patch.object(hide, "query", return_value=value),
                    patch.object(hide, "dispatch") as dispatch,
                    patch.object(hide, "notify"),
                ):
                    hide.action("hide", directory)
                    dispatch.assert_not_called()

    def test_restore_last_uses_newest_old_format_record_and_focuses_it(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            (folder / "0xaaa").write_text("3\n")
            (folder / "0xbbb").write_text("4\n")
            os.utime(folder / "0xaaa", ns=(1, 1))
            os.utime(folder / "0xbbb", ns=(2, 2))
            rows = [
                client("0xaaa", -99, hide.HIDDEN),
                client("0xbbb", -99, hide.HIDDEN),
            ]
            with (
                patch.object(hide, "query", return_value=rows),
                patch.object(hide, "dispatch") as dispatch,
                patch.object(hide, "notify"),
            ):
                hide.action("restore-last", folder)
            self.assertEqual(dispatch.call_args_list[0].args, ("window.move", "0xbbb"))
            self.assertEqual(dispatch.call_args_list[0].kwargs["workspace"], "4")
            self.assertEqual(dispatch.call_args_list[1].args, ("focus", "0xbbb"))
            self.assertTrue((folder / "0xaaa").exists())
            self.assertFalse((folder / "0xbbb").exists())

    def test_restore_current_keeps_other_workspaces_and_never_focuses(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            (folder / "0xaaa").write_text("3\n")
            (folder / "0xbbb").write_text("4\n")

            def query(name):
                return (
                    {"id": 3}
                    if name == "active_workspace"
                    else [
                        client("0xaaa", -99, hide.HIDDEN),
                        client("0xbbb", -99, hide.HIDDEN),
                    ]
                )

            with (
                patch.object(hide, "query", side_effect=query),
                patch.object(hide, "dispatch") as dispatch,
                patch.object(hide, "notify"),
            ):
                hide.action("restore-current", folder)
            dispatch.assert_called_once_with(
                "window.move", "0xaaa", workspace="3", follow=False
            )
            self.assertTrue((folder / "0xbbb").exists())
            self.assertFalse((folder / "0xaaa").exists())

    def test_malformed_symlink_and_unrelated_records_are_never_executed(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            (folder / "0xaaa").write_text("$(arbitrary command)")
            (folder / "custom.txt").write_text("user data")
            (folder / "0xbbb").symlink_to(folder / "custom.txt")
            self.assertEqual(hide.records(folder, []), [])
            self.assertEqual((folder / "custom.txt").read_text(), "user data")
            self.assertTrue((folder / "0xaaa").exists())
            self.assertTrue((folder / "0xbbb").is_symlink())

    def test_failed_restore_keeps_record_and_stale_client_is_pruned(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            record = folder / "0xaaa"
            record.write_text("3\n")
            with (
                patch.object(
                    hide, "query", return_value=[client("0xaaa", -99, hide.HIDDEN)]
                ),
                patch.object(hide, "dispatch", side_effect=RuntimeError("fixture")),
                patch.object(hide, "notify"),
            ):
                with self.assertRaises(RuntimeError):
                    hide.action("restore-last", folder)
            self.assertTrue(record.exists())
            self.assertEqual(hide.records(folder, []), [])
            self.assertFalse(record.exists())

    def test_special_workspace_resolution_and_service_target_arguments(self):
        with patch.object(
            hide, "query", return_value=[{"id": -2, "name": "special:notes"}]
        ):
            self.assertEqual(hide.destination(-2), "special:notes")
        with patch.object(hide, "compositor") as service:
            service.return_value.move_window.return_value = subprocess.CompletedProcess(
                [], 0, "ok\n"
            )
            hide.dispatch(
                "window.move", "0xaaa", workspace='name:quote"\\ø', follow=False
            )
        service.return_value.move_window.assert_called_once_with(
            "0xaaa", workspace='name:quote"\\ø', follow=False, timeout=5
        )
        with patch.object(hide, "compositor") as service:
            service.return_value.focus_window.side_effect = RuntimeError(
                "invalid window"
            )
            with self.assertRaises(RuntimeError):
                hide.dispatch("focus", "0xaaa")

    def test_query_uses_canonical_service_with_bounded_timeout(self):
        with patch.object(hide, "compositor") as service:
            service.return_value.query.return_value = {"address": "0x123"}
            self.assertEqual(hide.query("active_window"), {"address": "0x123"})
        service.return_value.query.assert_called_once_with(
            "active_window", timeout=5, max_bytes=4 * 1024 * 1024
        )

    def test_invalid_address_does_not_reach_the_service(self):
        with patch.object(hide, "compositor") as service:
            service.return_value.valid_window_id.return_value = False
            with self.assertRaises(ValueError):
                hide.dispatch("focus", "0x123;bad")
            service.return_value.valid_window_id.assert_called_once_with("0x123;bad")
            service.return_value.focus_window.assert_not_called()

    def test_backend_owned_ids_can_be_opaque_without_changing_hide_records(self):
        with tempfile.TemporaryDirectory() as directory:
            with (
                patch.object(hide, "compositor") as service,
                patch.object(hide, "query", return_value=client("window-42", 3)),
                patch.object(hide, "dispatch") as dispatch,
                patch.object(hide, "notify"),
            ):
                service.return_value.valid_window_id.side_effect = lambda value: (
                    value == "window-42"
                )
                hide.action("hide", directory)
            self.assertEqual((Path(directory) / "window-42").read_text(), "3\n")
            dispatch.assert_called_once_with(
                "window.move", "window-42", workspace="special:hidden", follow=False
            )
        with patch.object(hide, "compositor") as service:
            service.return_value.valid_window_id.return_value = True
            service.return_value.focus_window.return_value = (
                subprocess.CompletedProcess([], 0, "ok")
            )
            hide.dispatch("focus", "window-42")
        service.return_value.focus_window.assert_called_once_with(
            "window-42", timeout=5
        )
