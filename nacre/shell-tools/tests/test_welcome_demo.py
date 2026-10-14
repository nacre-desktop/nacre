"""Fresh-install artwork, meaningful mode contrast and reversible publication."""

import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
from PIL import Image

ROOT = Path(__file__).parents[1]


def load(name):
    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), ROOT / (name + ".py")
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


art = load("demo-wallpapers")
media = load("wallpaper-media")
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

    def test_private_scene_choices_and_missing_file_fallback(self):
        welcome = load("welcome")
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            paths = []
            for index in range(4):
                path = home / f"scene-{index}.png"
                Image.new("RGB", (32, 32), (index * 60, 40, 80)).save(path)
                paths.append(str(path))
            welcome.set_demo_wallpapers(home, paths)
            self.assertEqual(welcome.preferences(home)["demoWallpapers"], paths)
            self.assertEqual(
                (home / ".config/nacre/welcome.json").stat().st_mode & 0o777, 0o600
            )
            with (
                patch.object(media, "HOME", home),
                patch.object(
                    media,
                    "describe",
                    side_effect=lambda path: (
                        dict(path=path, name=Path(path).stem)
                        if Path(path).is_file()
                        else (_ for _ in ()).throw(ValueError("missing"))
                    ),
                ),
            ):
                self.assertEqual([item["path"] for item in media.demo_scenes()], paths)
                Path(paths[0]).unlink()
                self.assertTrue(all(item["license"] for item in media.demo_scenes()))
            with self.assertRaises(ValueError):
                welcome.set_demo_wallpapers(home, [paths[1]] * 4)

    def test_snapshot_and_restore_exact_palette_keep_unrelated_preferences(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            state = home / ".local/state/nacre"
            prefs = home / ".config/nacre/wallpaper-picker.json"
            poster = home / "original.png"
            video = home / "original.mp4"
            Image.new("RGB", (40, 30), "purple").save(poster)
            video.write_bytes(b"original animated source")
            scheme = json.loads((ROOT / "reference-style.json").read_text())
            original = {
                "path": str(video),
                "poster": str(poster),
                "thumbnail": str(poster),
                "preview": str(poster),
                "name": "Original",
                "dynamic": True,
                "animated": False,
                "appliedAtMs": 123,
            }
            media.atomic(state / "scheme.json", scheme)
            media.atomic(state / "wallpaper/media.json", original)
            media.atomic(
                prefs,
                {
                    "palettePreset": "super-red",
                    "paletteMode": "light",
                    "rotationMinutes": 30,
                    "extra": "keep",
                },
            )
            with (
                patch.multiple(
                    media,
                    HOME=home,
                    STATE=state / "wallpaper",
                    CACHE=home / ".cache/nacre/wallpaper-media",
                    PREFS=prefs,
                ),
                patch.object(
                    art,
                    "ensure",
                    return_value=[
                        {"path": str(poster), "name": "Sample", "license": "CC0-1.0"}
                    ],
                ),
                patch.object(
                    media,
                    "demo_module",
                    side_effect=lambda name: (
                        publisher
                        if name == "classic-state"
                        else load("welcome")
                        if name == "welcome"
                        else art
                    ),
                ),
            ):
                start = media.demo_start()
                self.assertEqual(media.read(state / "scheme.json", {}), scheme)
                self.assertEqual(
                    media.read(state / "wallpaper/media.json", {}), original
                )
                media.atomic(
                    prefs,
                    {
                        "palettePreset": "wallpaper",
                        "paletteMode": "dark",
                        "rotationMinutes": 90,
                        "extra": "keep",
                    },
                )
                media.atomic(state / "scheme.json", dict(scheme, mode="light"))
                with patch.object(publisher, "apply_palette") as publish:
                    result = media.demo_restore(start["snapshot"])
                self.assertEqual(result["media"], original)
                self.assertEqual(media.read(state / "scheme.json", {}), scheme)
                restored = json.loads(prefs.read_text())
                self.assertEqual(restored["palettePreset"], "super-red")
                self.assertEqual(restored["paletteMode"], "light")
                self.assertEqual(restored["rotationMinutes"], 90)
                self.assertEqual(restored["extra"], "keep")
                publish.assert_called_once_with(home, str(poster), live=True)
                self.assertEqual((state / "wallpaper/current").resolve(), poster)
                current_image = home / "demo.png"
                Image.new("RGB", (30, 30), "cyan").save(current_image)
                publisher.atomic_write(state / "wallpaper/last.txt", str(current_image))
                publisher.atomic_symlink(state / "wallpaper/current", current_image)
                publisher.atomic_symlink(
                    state / "wallpaper/thumbnail.jpg", current_image
                )
                media.atomic(state / "scheme.json", dict(scheme, mode="light"))
                before_scheme = (state / "scheme.json").read_bytes()
                before_prefs = prefs.read_bytes()
                with patch.object(
                    publisher,
                    "apply_palette",
                    side_effect=[RuntimeError("publication failed"), None],
                ):
                    with self.assertRaisesRegex(RuntimeError, "publication failed"):
                        media.demo_restore(start["snapshot"])
                self.assertEqual((state / "scheme.json").read_bytes(), before_scheme)
                self.assertEqual(prefs.read_bytes(), before_prefs)
                self.assertEqual((state / "wallpaper/current").resolve(), current_image)
                self.assertEqual(
                    (state / "wallpaper/thumbnail.jpg").resolve(), current_image
                )
                poster.unlink()
                before = prefs.read_bytes()
                with self.assertRaisesRegex(ValueError, "unavailable"):
                    media.demo_restore(start["snapshot"])
                self.assertEqual(prefs.read_bytes(), before)
                with self.assertRaises(ValueError):
                    media.demo_restore("../../scheme")

    def test_empty_install_captures_flat_start_without_applying(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            with (
                patch.multiple(
                    media,
                    HOME=home,
                    STATE=home / ".local/state/nacre/wallpaper",
                    CACHE=home / ".cache/media",
                    PREFS=home / ".config/nacre/wallpaper-picker.json",
                ),
                patch.object(art, "ensure", return_value=[]),
                patch.object(
                    media,
                    "demo_module",
                    side_effect=lambda name: (
                        publisher
                        if name == "classic-state"
                        else load("welcome")
                        if name == "welcome"
                        else art
                    ),
                ),
                patch.object(publisher, "apply_palette") as publish,
            ):
                result = media.demo_start()
                snapshot = json.loads(
                    (
                        home
                        / ".local/state/nacre/welcome/demo"
                        / (result["snapshot"] + ".json")
                    ).read_text()
                )
                self.assertTrue(Path(snapshot["poster"]).exists())
                self.assertFalse((home / ".local/state/nacre/scheme.json").exists())
                self.assertFalse(media.PREFS.exists())
                publish.assert_not_called()
                self.assertEqual(
                    (
                        home
                        / ".local/state/nacre/welcome/demo"
                        / (result["snapshot"] + ".json")
                    )
                    .stat()
                    .st_mode
                    & 0o777,
                    0o600,
                )
