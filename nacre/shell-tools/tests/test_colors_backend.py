"""Real preview/export/favorite/apply behavior in an isolated fake desktop home."""

import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import select
import tempfile
import unittest
from unittest.mock import patch
from PIL import Image

TOOLS = Path(__file__).parents[1]
sys.path.insert(0, str(TOOLS.parent / "shell-cli/src"))
import native_colors as colors


class ColorsBackendTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.home = Path(temporary.name)
        changes = patch.multiple(
            colors,
            HOME=self.home,
            CACHE=self.home / ".cache/nacre/colors",
            STORE=self.home / ".config/nacre/colors.json",
            EXPORTS=self.home / "exports",
        )
        changes.start()
        self.addCleanup(changes.stop)
        changes = patch.multiple(
            colors.media,
            HOME=self.home,
            LIBRARY=self.home / "Pictures/Wallpapers",
            CACHE=self.home / ".cache/nacre/wallpaper-media",
            STATE=self.home / ".local/state/nacre/wallpaper",
            PREFS=self.home / ".config/nacre/wallpaper-picker.json",
        )
        changes.start()
        self.addCleanup(changes.stop)
        changes = patch.dict(
            os.environ,
            {
                "HOME": str(self.home),
                "XDG_CACHE_HOME": str(self.home / ".cache"),
                "XDG_CONFIG_HOME": str(self.home / ".config"),
                "XDG_STATE_HOME": str(self.home / ".local/state"),
            },
        )
        changes.start()
        self.addCleanup(changes.stop)
        self.image = self.home / "scene.png"
        image = Image.new("RGB", (64, 40), "#172b45")
        image.paste("#db788f", (24, 12, 40, 28))
        image.save(self.image)

    def preview(self, **extra):
        return colors.stage({"image": str(self.image), **extra})

    def test_cached_stage_skips_generation_and_recovers_corrupt_record(self):
        data = self.preview()
        with patch.object(
            colors, "from_image", side_effect=AssertionError("warm preview regenerated")
        ):
            self.assertEqual(self.preview(), data)
        (colors.CACHE / (data["id"] + ".json")).write_text("broken json")
        self.assertEqual(self.preview()["id"], data["id"])

    def test_preview_stream_preserves_sequence_and_never_publishes(self):
        import io

        requests = [
            dict(sequence=4, request={"image": str(self.image)}),
            dict(sequence=9, request={"image": str(self.image), "personality": "pop"}),
        ]
        output = io.StringIO()
        self.assertEqual(
            colors.serve(
                io.StringIO("\n".join(json.dumps(r) for r in requests) + "\n"), output
            ),
            0,
        )
        replies = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertTrue(replies[0]["ready"])
        self.assertEqual([r["sequence"] for r in replies[1:]], [4, 9])
        self.assertFalse(colors.STORE.exists())
        self.assertFalse((self.home / ".local/state/nacre/scheme.json").exists())

    def test_preview_is_read_only_and_staged_record_detects_changed_image(self):
        data = self.preview(personality="pop")
        self.assertFalse(colors.STORE.exists())
        self.assertFalse(colors.media.PREFS.exists())
        self.assertFalse((self.home / ".local/state/nacre/scheme.json").exists())
        self.assertEqual(data["comparisons"], [])
        compared = self.preview(personality="pop", compare=True)
        self.assertEqual(len(compared["comparisons"]), 8)
        self.assertTrue(data["palette"]["accessibility"]["textPasses"])
        self.assertEqual(colors.record(data["id"])["request"]["personality"], "pop")
        Image.new("RGB", (64, 40), "#ee8844").save(self.image)
        with self.assertRaisesRegex(ValueError, "changed"):
            colors.record(data["id"])

    def test_swatches_keep_small_blue_detail_and_skin_among_dominant_shadows(self):
        image = Image.new("RGB", (120, 80), "#101114")
        image.paste("#292a2d", (0, 0, 100, 35))
        image.paste("#c39978", (35, 40, 65, 75))
        image.paste("#269bc8", (65, 40, 75, 75))
        image.save(self.image)
        data = self.preview()
        from nacre_shell.colour import lch

        hues = [
            lch(item["hex"])[2]
            for item in data["imageColors"]
            if lch(item["hex"])[1] > 0.04
        ]
        self.assertTrue(any(200 < hue < 270 for hue in hues), hues)
        self.assertTrue(any(40 < hue < 90 for hue in hues), hues)
        self.assertAlmostEqual(
            sum(item["coverage"] for item in data["imageColors"])
            + data["otherCoverage"],
            1,
            places=5,
        )
        self.assertGreater(data["otherCoverage"], -0.00001)
        members = [
            value
            for item in data["imageColors"]
            for value in item.get("members", [item["hex"]])
        ]
        self.assertEqual(len(members), len(set(members)))

    def test_source_migrates_to_read_only_background_switch(self):
        old = self.preview(personality="source")
        current = self.preview(personality="natural", backgroundFromWallpaper=True)
        self.assertEqual(old["id"], current["id"])
        self.assertEqual(current["request"]["personality"], "natural")
        self.assertTrue(current["request"]["backgroundFromWallpaper"])
        self.assertNotEqual(
            self.preview(backgroundFromWallpaper=False)["palette"]["colours"][
                "surface"
            ],
            current["palette"]["colours"]["surface"],
        )
        self.assertEqual(
            old["palette"]["colours"]["primary"],
            current["palette"]["colours"]["primary"],
        )
        self.assertFalse(colors.STORE.exists())
        self.assertFalse(colors.media.PREFS.exists())
        with self.assertRaises(ValueError):
            self.preview(backgroundFromWallpaper="yes")

    @unittest.skipUnless(shutil.which("rsvg-convert"), "SVG renderer unavailable")
    def test_favorite_both_modes_apply_history_and_new_home(self):
        data = self.preview(personality="harmony", backgroundFromWallpaper=True)
        result = colors.favorite(data["id"], "Evening")
        self.assertEqual(result["saved"], "Evening")
        saved = colors.stored()["favorites"][0]
        self.assertEqual(set(saved["modes"]), {"dark", "light"})
        result = colors.apply(data["id"], live=False)
        self.assertTrue(result["applied"])
        published = json.loads(
            (self.home / ".local/state/nacre/presentation.json").read_text()
        )
        self.assertEqual(published["palettePersonality"], "harmony")
        self.assertTrue(published["paletteBackgroundFromWallpaper"])
        self.assertTrue(
            colors.stored()["wallpapers"][str(self.image)]["backgroundFromWallpaper"]
        )
        self.assertEqual(len(colors.stored()["history"]), 1)
        self.assertEqual(colors.stored()["lastApplied"], data["id"])
        item = self.preview(favoriteId=saved["id"], mode="light")
        self.assertEqual(item["palette"]["colours"], saved["modes"]["light"])
        colors.apply(item["id"], live=False)
        self.assertEqual(
            colors.media.settings()["palettePreset"], "favorite:" + saved["id"]
        )
        colors.catalog()  # A saved preset must satisfy the existing catalogue schema.

    def test_export_card_and_custom_template_stay_in_output_folder(self):
        data = self.preview()
        result = colors.export(data["id"])
        directory = Path(result["directory"])
        self.assertTrue((directory / "palette.json").is_file())
        self.assertTrue((directory / "firefox/manifest.json").is_file())
        result = colors.card(data["id"])
        self.assertTrue(Path(result["card"]).is_file())
        self.assertFalse((self.home / ".config/kitty").exists())
        self.assertFalse((self.home / ".config/gtk-4.0").exists())

    def test_picked_point_and_pinned_roles_preserve_readability(self):
        data = self.preview(
            pick={"x": 0.5, "y": 0.5},
            overrides={"secondary": "47a87f"},
            brightness=0.08,
            saturation=1.3,
        )
        self.assertRegex(data["request"]["accent"], r"^[0-9a-f]{6}$")
        self.assertTrue(data["palette"]["accessibility"]["textPasses"])
        with self.assertRaises(ValueError):
            self.preview(hour=27)
        with self.assertRaises(ValueError):
            colors.record("../other")

    def test_worker_responds_without_waiting_for_stdin_eof(self):
        process = subprocess.Popen(
            [
                os.environ.get("NACRE_HELPER_BINARY", str(TOOLS / "nacre-helper")),
                "colors",
                "preview",
            ],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            env={**os.environ, "PYTHONPATH": str(TOOLS.parent / "shell-cli/src")},
        )

        def cleanup():
            if process.poll() is None:
                process.kill()
            process.wait()
            for stream in (process.stdin, process.stdout, process.stderr):
                stream.close()

        self.addCleanup(cleanup)
        process.stdin.write(json.dumps({"image": str(self.image)}) + "\n")
        process.stdin.flush()
        self.assertTrue(
            select.select([process.stdout], [], [], 10)[0],
            "worker must reply before stdin closes",
        )
        response = process.stdout.readline()
        self.assertEqual(process.wait(timeout=10), 0, process.stderr.read())
        self.assertIn("id", json.loads(response))
        process.stdin.close()
        process.stdout.close()
        process.stderr.close()
