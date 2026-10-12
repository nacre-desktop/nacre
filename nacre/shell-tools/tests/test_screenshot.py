import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import Mock
from types import SimpleNamespace

spec = importlib.util.spec_from_file_location(
    "screenshot", Path(__file__).parents[1] / "screenshot.py"
)
shot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(shot)


class ScreenshotTests(unittest.TestCase):
    def test_capture_notification_identifies_nacre(self):
        from unittest.mock import patch

        with patch.object(shot.subprocess, "run") as run:
            shot.notify("Saved")
        command = run.call_args.args[0]
        self.assertEqual(command[command.index("-a") + 1], "Nacre Capture")
        self.assertEqual(command[command.index("-i") + 1], "nacre-notifications")

    def test_cancelled_selection_never_captures_or_clears_clipboard(self):
        for result in (
            SimpleNamespace(returncode=1, stdout=""),
            SimpleNamespace(returncode=0, stdout=""),
        ):
            runner = Mock(return_value=result)
            self.assertIsNone(shot.capture("region", runner=runner))
            runner.assert_called_once_with(["slurp"], capture_output=True, text=True)

    def test_region_copies_the_selected_png(self):
        runner = Mock(
            side_effect=[
                SimpleNamespace(returncode=0, stdout="10,20 400x300\n"),
                SimpleNamespace(stdout=b"png"),
                SimpleNamespace(),
            ]
        )
        self.assertIn("selection", shot.capture("region", runner=runner))
        self.assertEqual(
            runner.call_args_list[1].args[0], ["grim", "-g", "10,20 400x300", "-"]
        )
        self.assertEqual(runner.call_args_list[2].kwargs["input"], b"png")

    def test_full_capture_creates_directory_and_unique_saved_pngs(self):
        paths = []

        def runner(args, **kwargs):
            self.assertNotEqual(args[0], "slurp")
            if args[0] == "grim":
                path = Path(args[1])
                paths.append(path)
                self.assertTrue(path.parent.is_dir())
                path.write_bytes(b"png")
            else:
                self.assertEqual(args, ["wl-copy", "--type", "image/png"])
                self.assertEqual(kwargs["input"], b"png")
            return SimpleNamespace()

        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            shot.capture("full", home=home, runner=runner)
            shot.capture("full", home=home, runner=runner)
            self.assertEqual(len(set(paths)), 2)
            self.assertTrue(
                all(p.parent == home / "Pictures/Screenshots" for p in paths)
            )
