"""Wallpaper resolution filtering queries the common compositor and degrades safely."""

from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).parents[2]
sys.path.insert(0, str(ROOT / "nacre/shell-cli/src"))
from PIL import Image
from nacre_shell import cli


class WallpaperCompositorFilterTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.folder = Path(folder.name)
        for name, size in (("small.png", (128, 80)), ("large.png", (256, 160))):
            Image.new("RGB", size).save(self.folder / name)

    def test_monitor_dimensions_keep_existing_filter_behavior(self):
        with patch.object(cli, "compositor") as service:
            service.return_value.query.return_value = [
                {"width": 256, "height": 160},
                {"width": 192, "height": 108},
            ]
            self.assertEqual(
                cli.suitable_images(self.folder, 1, False), [self.folder / "large.png"]
            )
        service.return_value.query.assert_called_once_with("monitors", timeout=3)

    def test_optional_service_failures_use_existing_1080p_fallback(self):
        for failure in (
            OSError("offline"),
            RuntimeError("not installed"),
            ValueError("bad data"),
            subprocess.TimeoutExpired("compositor", 3),
        ):
            with self.subTest(failure=failure):
                with patch.object(cli, "compositor", side_effect=failure):
                    self.assertEqual(
                        cli.suitable_images(self.folder, 0.1, False),
                        [self.folder / "large.png"],
                    )

    def test_no_filter_is_read_only_and_never_queries_the_compositor(self):
        with patch.object(cli, "compositor") as service:
            self.assertEqual(len(cli.suitable_images(self.folder, 1, True)), 2)
        service.assert_not_called()
