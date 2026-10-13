import shutil
import importlib.util
import json
import os
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from PIL import Image

spec = importlib.util.spec_from_file_location(
    "wallpaper_media", Path(__file__).resolve().parents[1] / "wallpaper-media.py"
)
media = importlib.util.module_from_spec(spec)
spec.loader.exec_module(media)


class WallpaperMediaTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        home = Path(temporary.name)
        isolated = patch.multiple(
            media,
            HOME=home,
            LIBRARY=home / "pictures",
            CACHE=home / "cache",
            STATE=home / "state",
            PREFS=home / "config/picker.json",
        )
        isolated.start()
        self.addCleanup(isolated.stop)

    def test_decoder_safety_failure_does_not_break_the_wallpaper_library(self):
        media.LIBRARY.mkdir(parents=True)
        bad = media.LIBRARY / "bad.png"
        good = media.LIBRARY / "good.png"
        bad.write_bytes(b"invalid")
        good.write_bytes(b"fixture")

        def describe(path):
            if path == bad:
                raise Image.DecompressionBombError("Image is unsafe to decode")
            return {"path": str(path)}

        with patch.object(media, "describe", side_effect=describe):
            result = media.catalog()
        self.assertEqual(result["errors"], ["bad.png"])
        self.assertEqual(result["entries"], [{"path": str(good)}])

    def test_source_preference_migrates_without_changing_file_on_read(self):
        media.PREFS.parent.mkdir(parents=True)
        media.PREFS.write_text(json.dumps({"palettePersonality": "source"}))
        before = media.PREFS.read_bytes()
        current = media.settings()
        self.assertEqual(current["palettePersonality"], "natural")
        self.assertTrue(current["paletteBackgroundFromWallpaper"])
        self.assertEqual(media.PREFS.read_bytes(), before)
        media.preference({"palettePersonality": "source"})
        saved = json.loads(media.PREFS.read_text())
        self.assertEqual(saved["palettePersonality"], "natural")
        self.assertTrue(saved["paletteBackgroundFromWallpaper"])
        media.preference({"paletteBackgroundFromWallpaper": False})
        self.assertFalse(media.settings()["paletteBackgroundFromWallpaper"])

    def test_static_and_animated_gif_have_distinct_identity_and_posters(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(media, "CACHE", Path(folder) / "cache"),
        ):
            static = Path(folder) / "Still.png"
            Image.new("RGB", (32, 24), "red").save(static)
            animated = Path(folder) / "Motion.gif"
            frames = [Image.new("RGB", (32, 24), c) for c in ("red", "blue")]
            frames[0].save(
                animated, save_all=True, append_images=frames[1:], duration=100, loop=0
            )
            self.assertFalse(media.describe(static)["dynamic"])
            item = media.describe(animated)
            self.assertTrue(item["dynamic"])
            self.assertTrue(item["animated"])
            self.assertNotEqual(item["path"], item["poster"])
            self.assertTrue(Path(item["poster"]).is_file())

    def test_previews_are_small_private_reused_and_invalidated_after_change(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(media, "CACHE", Path(folder) / "cache"),
        ):
            source = Path(folder) / "Large.png"
            Image.new("RGB", (3840, 2160), "red").save(source)
            first = media.describe(source)
            stamp = Path(first["thumbnail"]).stat().st_mtime_ns
            again = media.describe(source)
            self.assertEqual(first["thumbnail"], again["thumbnail"])
            self.assertEqual(stamp, Path(again["thumbnail"]).stat().st_mtime_ns)
            for name, limit in [("thumbnail", 640), ("preview", 1600)]:
                path = Path(first[name])
                self.assertEqual(path.stat().st_mode & 0o777, 0o600)
                with Image.open(path) as image:
                    self.assertLessEqual(image.width, limit)
            Image.new("RGB", (3840, 2160), "blue").save(source)
            changed = media.describe(source)
            self.assertNotEqual(first["thumbnail"], changed["thumbnail"])

    def test_preferences_are_validated_private_and_preserve_other_fields(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(media, "PREFS", Path(folder) / "picker.json"),
        ):
            media.preference({"layout": "hexagons"})
            media.preference({"kind": "dynamic"})
            self.assertEqual(media.settings()["layout"], "hexagons")
            self.assertEqual(os.stat(media.PREFS).st_mode & 0o777, 0o600)
            with self.assertRaises(ValueError):
                media.preference({"layout": "command-line"})

    def test_rotation_and_palette_preferences_validate_without_clobbering_picker(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(media, "PREFS", Path(folder) / "picker.json"),
        ):
            media.preference(
                {
                    "layout": "hexagons",
                    "rotationEnabled": True,
                    "rotationMinutes": 30,
                    "rotationKind": "dynamic",
                    "rotationShuffle": False,
                    "palettePreset": "ocean",
                }
            )
            media.preference({"rotationMinutes": 60})
            saved = media.settings()
            self.assertEqual(saved["layout"], "hexagons")
            self.assertEqual(saved["palettePreset"], "ocean")
            self.assertEqual(saved["rotationMinutes"], 60)
            for value in [True, 0, 4, 1441, 2.5, "30"]:
                with self.assertRaises(ValueError):
                    media.preference({"rotationMinutes": value})
            for value in [
                {"palettePreset": "arbitrary-file"},
                {"paletteMode": "auto"},
                {"rotationKind": "brain"},
                {"rotationEnabled": "yes"},
            ]:
                with self.assertRaises(ValueError):
                    media.preference(value)
            self.assertEqual(media.settings(), saved)

    def test_theme_failure_restores_preferences(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            poster = base / "poster.png"
            Image.new("RGB", (20, 20), "red").save(poster)
            state = base / "state"
            state.mkdir()
            (state / "media.json").write_text(json.dumps({"poster": str(poster)}))
            with (
                patch.object(media, "STATE", state),
                patch.object(media, "PREFS", base / "picker.json"),
                patch.object(
                    media.subprocess, "run", side_effect=RuntimeError("test failure")
                ),
            ):
                with self.assertRaises(RuntimeError):
                    media.theme({"palettePreset": "ocean"})
                self.assertEqual(media.settings()["palettePreset"], "wallpaper")
                self.assertTrue(poster.exists())

    def test_rotation_anchor_survives_preferences_and_legacy_reload(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(media, "PREFS", Path(folder) / "picker.json"),
        ):
            with patch.object(media.time, "time", return_value=1000):
                media.preference({"rotationEnabled": True, "rotationMinutes": 30})
            self.assertEqual(media.settings()["rotationAnchorMs"], 1000000)
            with patch.object(media.time, "time", return_value=2000):
                media.preference({"layout": "hexagons", "palettePreset": "ocean"})
                self.assertEqual(media.settings()["rotationAnchorMs"], 1000000)
                media.preference({"rotationMinutes": 60})
                self.assertEqual(media.settings()["rotationAnchorMs"], 2000000)
            media.PREFS.write_text(
                json.dumps({"rotationEnabled": True, "rotationMinutes": 30})
            )
            os.utime(media.PREFS, (800, 800))
            self.assertEqual(media.settings()["rotationAnchorMs"], 800000)
            self.assertEqual(media.settings()["rotationAnchorMs"], 800000)

    def test_media_migration_preserves_success_time_without_rewriting(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(media, "STATE", Path(folder)),
        ):
            path = media.STATE / "media.json"
            path.write_text(json.dumps({"path": "old.mp4", "poster": "poster.png"}))
            os.utime(path, (500, 500))
            self.assertEqual(media.media_state()["appliedAtMs"], 500000)
            self.assertNotIn("appliedAtMs", json.loads(path.read_text()))

    def test_import_preserves_original_and_does_not_overwrite_names(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(media, "LIBRARY", Path(folder) / "walls"),
        ):
            source = Path(folder) / "Example.png"
            Image.new("RGB", (32, 24), "blue").save(source)
            first = media.import_files([str(source)])
            second = media.import_files([str(source)])
            self.assertNotEqual(first, second)
            self.assertTrue(source.exists())
            self.assertTrue(Path(first[0]).exists())

    def test_only_successful_selection_advances_persisted_photo_time(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            with (
                patch.object(media, "HOME", home),
                patch.object(media, "STATE", home / "state"),
                patch.object(media, "CACHE", home / "cache"),
                patch.object(
                    media, "PREFS", home / ".config/nacre/wallpaper-picker.json"
                ),
            ):
                source = home / "image.png"
                Image.new("RGB", (30, 30), "red").save(source)
                with (
                    patch.object(media.subprocess, "run"),
                    patch.object(media.time, "time", return_value=1234),
                ):
                    item = media.select(source)
                self.assertEqual(item["appliedAtMs"], 1234000)
                with (
                    patch.object(
                        media.subprocess, "run", side_effect=RuntimeError("failed")
                    ),
                    patch.object(media.time, "time", return_value=9999),
                ):
                    with self.assertRaises(RuntimeError):
                        media.select(source)
                self.assertEqual(
                    json.loads((media.STATE / "media.json").read_text())["appliedAtMs"],
                    1234000,
                )

    def test_prepared_install_uses_palette_bridge_when_desktop_wrapper_is_absent(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            source = home / "image.png"
            Image.new("RGB", (32, 24), "red").save(source)
            bridge = home / ".local/share/nacre/shell/bin/nacre_shell"
            bridge.parent.mkdir(parents=True)
            bridge.touch()
            with (
                patch.object(media, "HOME", home),
                patch.object(media, "STATE", home / "state"),
                patch.object(media, "CACHE", home / "cache"),
                patch.object(media.subprocess, "run") as run,
            ):
                media.select(source)
                self.assertEqual(
                    run.call_args.args[0],
                    [
                        str(home / ".local/share/nacre/shell/bin/nacre_shell"),
                        "wallpaper",
                        "--file",
                        str(source),
                    ],
                )
                bridge.unlink()
                media.select(source)
                self.assertEqual(
                    run.call_args.args[0],
                    [
                        str(Path(media.__file__).with_name("cli-bridge.sh")),
                        "wallpaper",
                        "--file",
                        str(source),
                    ],
                )
                legacy = home / ".local/bin/nacre-shell"
                legacy.parent.mkdir(parents=True)
                legacy.touch()
                media.select(source)
                self.assertEqual(
                    run.call_args.args[0], [str(legacy), "wallpaper-image", str(source)]
                )

    def test_failed_palette_commit_does_not_publish_media_state(self):
        with (
            tempfile.TemporaryDirectory() as folder,
            patch.object(media, "STATE", Path(folder) / "state"),
            patch.object(media, "CACHE", Path(folder) / "cache"),
            patch.object(media.subprocess, "run", side_effect=RuntimeError),
        ):
            source = Path(folder) / "image.png"
            Image.new("RGB", (32, 24), "red").save(source)
            with self.assertRaises(RuntimeError):
                media.select(source)
            self.assertFalse((media.STATE / "media.json").exists())


class OrientPreparedCacheTests(unittest.TestCase):
    @unittest.skipUnless(shutil.which("rsvg-convert"), "SVG renderer unavailable")
    def test_current_treatments_prepare_only_cache_without_publishing(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            poster = home / "poster.png"
            Image.new("RGB", (32, 24), "red").save(poster)
            cli = home / ".local/share/nacre/palette-runtime/venv/bin/nacre_shell"
            cli.parent.mkdir(parents=True)
            colors = json.loads(
                Path(media.__file__).with_name("reference-style.json").read_text()
            )["colours"]
            cli.write_text(
                "#!/usr/bin/python3\nprint("
                + repr(json.dumps({"colours": colors}))
                + ")\n"
            )
            cli.chmod(0o700)
            with (
                patch.object(media, "HOME", home),
                patch.object(media, "CACHE", home / ".cache/nacre/wallpaper-media"),
                patch.object(
                    media, "media_state", return_value={"poster": str(poster)}
                ),
                patch.object(media, "catalog", return_value={"entries": []}),
                patch.object(
                    media.subprocess, "run", wraps=media.subprocess.run
                ) as run,
            ):
                media.warm(include_demo=False)
                art = list((home / ".cache/nacre/branding").glob("*.png"))
                self.assertTrue(art)
                for path in art:
                    path.unlink()
                media.warm(include_demo=False)
                self.assertTrue(all(path.exists() for path in art))
            queries = [
                call.args[0]
                for call in run.call_args_list
                if call.args[0][0] == str(cli)
            ]
            self.assertEqual(len(queries), 16)
            self.assertEqual(
                set(q[-1] for q in queries),
                {
                    "natural",
                    "pigment",
                    "harmony",
                    "pop",
                    "mist",
                    "vivid",
                    "pearl",
                    "tide",
                },
            )
            self.assertEqual(
                set(q[q.index("--mode") + 1] for q in queries), {"dark", "light"}
            )
            self.assertTrue(list((home / ".cache/nacre/branding").glob("*.png")))
            self.assertFalse((home / ".local/state/nacre/presentation.json").exists())
            self.assertFalse((home / ".local/share/icons").exists())

    def test_prepared_treatment_uses_publisher_and_rejects_wrong_treatment(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            poster = home / "poster.png"
            Image.new("RGB", (32, 24), "red").save(poster)
            digest = "a" * 64
            thumbnail = home / ".cache/nacre/wallpapers" / digest / "thumbnail.jpg"
            thumbnail.parent.mkdir(parents=True)
            Image.new("RGB", (32, 24), "red").save(thumbnail)
            data = json.loads(
                Path(media.__file__).with_name("reference-style.json").read_text()
            )
            data.update(
                name="dynamic",
                mode="dark",
                variant="tonalspot",
                flavour="default",
                input={"harmony": True},
                source={"digest": digest, "selected": "ff0000"},
            )
            scheme = home / ".local/state/nacre/scheme.json"
            scheme.parent.mkdir(parents=True)
            scheme.write_text(json.dumps(data))
            with (
                patch.object(media, "HOME", home),
                patch.object(media, "CACHE", home / ".cache/nacre/wallpaper-media"),
                patch.object(
                    media, "PREFS", home / ".config/nacre/wallpaper-picker.json"
                ),
            ):
                media.preference({"paletteHarmony": True})
                marker = media.palette_marker(poster, "default")
                media.atomic(marker, data)
                self.assertTrue(media.prepared_theme(poster, live=False))
                presentation = json.loads(
                    (home / ".local/state/nacre/presentation.json").read_text()
                )
                self.assertTrue(presentation["paletteHarmony"])
                self.assertEqual(presentation["selectedAccent"], "ff0000")
                data["input"]["harmony"] = False
                media.atomic(marker, data)
                self.assertFalse(media.prepared_theme(poster, live=False))
                media.atomic(marker, [])
                self.assertFalse(media.prepared_theme(poster, live=False))
                self.assertEqual(
                    json.loads(
                        (home / ".local/state/nacre/presentation.json").read_text()
                    ),
                    presentation,
                )

    def test_mode_variant_accent_and_engine_change_invalidate_prepared_cache(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            poster = home / "poster.png"
            Image.new("RGB", (32, 24), "red").save(poster)
            state = home / ".local/state/nacre"
            state.mkdir(parents=True)
            config = home / ".config/nacre"
            config.mkdir(parents=True)
            runtime = home / ".local/share/nacre/palette-runtime"
            (runtime / "venv/bin").mkdir(parents=True)
            (runtime / "venv/bin/nacre_shell").write_text("fixture")
            marker = runtime / "orient-build.json"
            marker.write_text('{"source":"one"}')
            with (
                patch.object(media, "HOME", home),
                patch.object(media, "CACHE", home / "cache"),
                patch.object(
                    media, "PREFS", home / ".config/nacre/wallpaper-picker.json"
                ),
            ):
                (state / "scheme.json").write_text(
                    '{"mode":"dark","variant":"tonalspot"}'
                )
                first = media.palette_marker(poster, "default")
                self.assertEqual(media.palette_marker(poster, "default"), first)
                (state / "scheme.json").write_text(
                    '{"mode":"light","variant":"tonalspot"}'
                )
                second = media.palette_marker(poster, "default")
                self.assertNotEqual(first, second)
                (state / "scheme.json").write_text(
                    '{"mode":"light","variant":"neutral"}'
                )
                third = media.palette_marker(poster, "default")
                self.assertNotEqual(second, third)
                (config / "cli.json").write_text(
                    '{"orient":{"accents":{"image":"ff3030"}}}'
                )
                fourth = media.palette_marker(poster, "default")
                self.assertNotEqual(third, fourth)
                marker.write_text('{"source":"two"}')
                self.assertNotEqual(fourth, media.palette_marker(poster, "default"))
                fifth = media.palette_marker(poster, "default")
                media.preference({"paletteHarmony": True})
                self.assertNotEqual(fifth, media.palette_marker(poster, "default"))


class WallpaperAccentTests(unittest.TestCase):
    def test_accent_selection_routes_through_publisher_and_keeps_preferences(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            poster = home / "poster.png"
            Image.new("RGB", (20, 20), "red").save(poster)
            state = home / "state"
            state.mkdir()
            (state / "media.json").write_text(json.dumps({"poster": str(poster)}))
            with (
                patch.object(media, "HOME", home),
                patch.object(media, "STATE", state),
                patch.object(media, "PREFS", home / "picker.json"),
                patch.object(media.subprocess, "run") as run,
            ):
                media.preference({"palettePreset": "ocean", "layout": "hexagons"})
                result = media.theme({"paletteAccent": "aabbcc"})
                self.assertEqual(result["palettePreset"], "wallpaper")
                self.assertEqual(result["layout"], "hexagons")
                self.assertNotIn("paletteAccent", result)
                self.assertEqual(run.call_args.args[0][-2:], ["--accent", "aabbcc"])
                media.theme({"paletteAccent": "auto"})
                self.assertEqual(run.call_args.args[0][-1], "--auto-accent")
                previous = media.settings()
                with self.assertRaises(ValueError):
                    media.theme({"paletteAccent": "bad argument"})
                self.assertEqual(media.settings(), previous)

    def test_harmony_persists_uses_publication_and_rolls_back_on_failure(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            poster = home / "poster.png"
            Image.new("RGB", (20, 20), "red").save(poster)
            state = home / "state"
            state.mkdir()
            (state / "media.json").write_text(json.dumps({"poster": str(poster)}))
            with (
                patch.object(media, "HOME", home),
                patch.object(media, "STATE", state),
                patch.object(media, "PREFS", home / "picker.json"),
                patch.object(media.subprocess, "run") as run,
            ):
                self.assertFalse(media.settings()["paletteHarmony"])
                result = media.theme({"paletteHarmony": True})
                self.assertTrue(result["paletteHarmony"])
                self.assertEqual(
                    run.call_args.args[0][1:],
                    ["wallpaper", "-f", str(poster), "--no-smart"],
                )
                run.side_effect = RuntimeError("fixture")
                with self.assertRaises(RuntimeError):
                    media.theme({"paletteHarmony": False})
                self.assertTrue(media.settings()["paletteHarmony"])
                with self.assertRaises(ValueError):
                    media.theme({"paletteHarmony": "yes"})

    def test_failed_accent_change_preserves_fixed_choice(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            poster = home / "poster.png"
            Image.new("RGB", (20, 20), "red").save(poster)
            state = home / "state"
            state.mkdir()
            (state / "media.json").write_text(json.dumps({"poster": str(poster)}))
            with (
                patch.object(media, "HOME", home),
                patch.object(media, "STATE", state),
                patch.object(media, "PREFS", home / "picker.json"),
                patch.object(
                    media.subprocess, "run", side_effect=RuntimeError("fixture")
                ),
            ):
                media.preference({"palettePreset": "ocean"})
                with self.assertRaises(RuntimeError):
                    media.theme({"paletteAccent": "aabbcc"})
                self.assertEqual(media.settings()["palettePreset"], "ocean")


class PersistentPublisherTests(unittest.TestCase):
    def test_serial_transport_recovers_after_invalid_request_and_streams_before_completion(
        self,
    ):
        import io
        import os

        lines = [
            {"action": "theme", "value": {"paletteMode": "light"}},
            {"action": "unknown", "value": {}},
            {"action": "preferences", "value": {"layout": "hexagons"}},
        ]
        stream = io.StringIO("".join(json.dumps(item) + "\n" for item in lines))
        output = io.StringIO()

        def theme(value):
            print(json.dumps({"presentation": {"mode": "light"}}), flush=True)
            return {"paletteMode": value["paletteMode"]}

        with (
            patch.dict(os.environ, {}, clear=False),
            patch.object(media, "theme", side_effect=theme),
            patch.object(media, "preference", side_effect=lambda value: value),
        ):
            self.assertEqual(media.serve(stream, output), 0)
        result = [json.loads(line) for line in output.getvalue().splitlines()]
        self.assertTrue(result[0]["ready"])
        self.assertEqual(result[1]["presentation"]["mode"], "light")
        self.assertEqual([r["code"] for r in result if r.get("done")], [0, 1, 0])
        self.assertEqual(result[-1]["result"]["layout"], "hexagons")
