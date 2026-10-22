"""Fresh-install artwork, meaningful mode contrast and reversible publication."""

import importlib.util
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
from PIL import Image

ROOT = Path(__file__).parents[1]


def load(name):
    if name == "wallpaper-media":
        import native_wallpaper_media

        return native_wallpaper_media
    if name == "classic-state":
        import native_classic_state

        return native_classic_state
    if name == "demo-wallpapers":
        import native_demo_wallpapers

        return native_demo_wallpapers
    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), ROOT / (name + ".py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


art = load("demo-wallpapers")
from native_media_subset import install

media = install(load("wallpaper-media"))
publisher = load("classic-state")


class WelcomeDemoTests(unittest.TestCase):
    def test_curated_art_has_distinct_pop_and_readable_modes(self):
        sys.path.insert(0, str(ROOT.parent / "shell-cli/src"))
        self.addCleanup(sys.path.pop, 0)
        from nacre_shell.engine import from_image
        from nacre_shell.colour import hue_distance, lch
        from nacre_shell.palette import validate

        with tempfile.TemporaryDirectory() as directory:
            for item in art.collection():
                path = art.ASSETS / item["file"]
                for mode in ("dark", "light"):
                    palettes = [
                        from_image(
                            path,
                            mode=mode,
                            personality=name,
                            cache_dir=Path(directory) / "palettes",
                        )
                        for name in ("natural", "pop", "pearl")
                    ]
                    for palette in palettes:
                        validate(palette["colours"])
                    natural, pop, pearl = palettes
                    distance = hue_distance(
                        lch(natural["colours"]["primary"])[2],
                        lch(pop["colours"]["primary"])[2],
                    )
                    self.assertGreater(distance, 70, (item["name"], mode, distance))
                    self.assertNotEqual(pop["colours"], pearl["colours"])

    def test_offline_bundled_resolution_cache_preserves_bytes_and_repairs_damage(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            entries = art.ensure(home)
            stamps = [Path(item["path"]).stat().st_mtime_ns for item in entries]
            self.assertEqual(entries, art.ensure(home))
            self.assertEqual(
                stamps, [Path(item["path"]).stat().st_mtime_ns for item in entries]
            )
            with patch.object(
                art,
                "collection",
                side_effect=AssertionError("unchanged bytes were rehashed"),
            ):
                self.assertEqual(entries, art.ensure(home, fast=True))
            self.assertFalse((home / "Pictures").exists())
            self.assertEqual(len(entries), 4)
            for item in entries:
                self.assertEqual(
                    Path(item["path"]).read_bytes(),
                    (art.ASSETS / item["file"]).read_bytes(),
                )
                self.assertIn(item["license"], ("CC0",))
                self.assertTrue(
                    item["artist"] and item["source"] and item["licenseUrl"]
                )
                with Image.open(item["path"]) as image:
                    self.assertEqual(list(image.size), item["size"])
                    self.assertEqual(max(image.size), 3840)
            Path(entries[0]["path"]).write_bytes(b"broken cache")
            art.ensure(home, fast=True)
            self.assertEqual(
                Path(entries[0]["path"]).read_bytes(),
                (art.ASSETS / entries[0]["file"]).read_bytes(),
            )
            Path(entries[1]["path"]).unlink()
            art.ensure(home, fast=True)
            self.assertTrue(Path(entries[1]["path"]).is_file())
