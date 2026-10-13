from unittest.mock import patch
import importlib.util
import contextlib
import io
import json
import fcntl
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "palette", Path(__file__).resolve().parents[1] / "classic-state.py"
)
palette = importlib.util.module_from_spec(spec)
spec.loader.exec_module(palette)


class PaletteCommitTest(unittest.TestCase):
    def test_stream_is_opt_in_and_only_follows_an_atomic_publication(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            path = home / "presentation.json"
            payload = {
                "poster": "/fixture/image",
                "publishedAtMs": 1001,
                "colours": {"primary": "aabbcc"},
            }
            with contextlib.redirect_stdout(io.StringIO()) as output:
                palette.atomic_write(path, json.dumps(payload))
            self.assertEqual(output.getvalue(), "")
            with (
                patch.dict(os.environ, {"NACRE_PRESENTATION_STREAM": "1"}),
                contextlib.redirect_stdout(io.StringIO()) as output,
            ):
                palette.atomic_write(path, json.dumps(payload))
            event = json.loads(output.getvalue())
            self.assertEqual(event["presentation"], json.loads(path.read_text()))
            self.assertFalse(list(home.glob(".presentation*")))

    def test_live_palette_reload_uses_the_selected_compositor_api(self):
        for backend in ("hyprland", "niri"):
            with (
                self.subTest(backend=backend),
                patch.object(palette.compositor, "backend_name", backend),
                patch.object(palette.Path, "iterdir", return_value=iter(())),
                patch.object(palette.subprocess, "run"),
                patch.object(
                    palette.compositor,
                    "apply_config",
                    return_value=subprocess.CompletedProcess([], 0, "", ""),
                ) as apply,
                patch.object(palette.compositor, "reload") as reload,
            ):
                palette._reload_desktop_consumers(
                    {"mode": "dark"}, "", "palette config", True
                )
                if backend == "hyprland":
                    apply.assert_called_once_with(
                        "palette config", check=False, timeout=None
                    )
                    reload.assert_not_called()
                else:
                    apply.assert_not_called()
                    reload.assert_called_once_with()

    def test_wallpaper_commit_updates_consumers_and_preserves_terminal(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            state = home / ".local/state/nacre"
            state.mkdir(parents=True)
            colors = json.loads(
                (
                    Path(__file__).resolve().parents[1] / "reference-style.json"
                ).read_text()
            )["colours"]
            colors["primary"] = "#123456"
            (state / "scheme.json").write_text(
                json.dumps({"mode": "light", "colours": colors})
            )
            terminal = home / ".config/kitty/kitty.conf"
            terminal.parent.mkdir(parents=True)
            terminal.write_text("foreground #fedcba\n")
            palette.apply_palette(home, "/tmp/example-wallpaper.png", live=False)
            self.assertIn(
                "rgba(123456ff)",
                (home / ".config/nacre/palette.lua").read_text(),
            )
            self.assertIn(
                "@define-color accent_color #123456;",
                (home / ".config/gtk-3.0/gtk.css").read_text(),
            )
            self.assertIn("#ff123456", (home / ".config/nacre/qt.conf").read_text())
            self.assertIn(
                "accent: #123456;",
                (home / ".config/nacre/rofi.rasi").read_text(),
            )
            self.assertEqual(
                (home / ".config/nacre/colors/primary").read_text(), "#123456"
            )
            self.assertIn(
                "active_border_color #123456",
                (home / ".config/nacre/kitty-colors.conf").read_text(),
            )
            self.assertIn(
                "color4 #"
                + palette.readable(colors["inversePrimary"], colors["inverseSurface"])
                + "\n",
                (home / ".config/nacre/kitty-colors.conf").read_text(),
            )
            self.assertIn(
                "color14 #"
                + palette.readable(colors["secondary"], colors["inverseSurface"])
                + "\n",
                (home / ".config/nacre/kitty-colors.conf").read_text(),
            )
            self.assertIn(
                "outer_color = rgba(12345650)",
                (home / ".config/hypr/hyprlock.conf").read_text(),
            )
            self.assertIn("primary 123456", (state / "scheme/current.txt").read_text())
            self.assertEqual(
                (state / "wallpaper/last.txt").read_text(), "/tmp/example-wallpaper.png"
            )
            self.assertEqual(terminal.read_text(), "foreground #fedcba\n")
            term = (home / ".config/nacre/kitty-colors.conf").read_text()
            self.assertIn("background #" + colors["inverseSurface"], term)
            self.assertIn("foreground #" + colors["inverseOnSurface"], term)

    def test_modern_gtk_sidebar_roles_and_available_icons_are_consistent(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            state = home / ".local/state/nacre"
            state.mkdir(parents=True)
            colors = json.loads(
                Path(palette.__file__).with_name("reference-style.json").read_text()
            )["colours"]
            (state / "scheme.json").write_text(
                json.dumps({"mode": "dark", "colours": colors})
            )
            theme = home / ".local/share/icons/Papirus-Dark"
            theme.mkdir(parents=True)
            (theme / "index.theme").write_text("[Icon Theme]\nName=Papirus-Dark\n")
            for version in ("3.0", "4.0"):
                path = home / (".config/gtk-" + version)
                path.mkdir(parents=True)
                (path / "settings.ini").write_text(
                    "[Settings]\ngtk-icon-theme-name=breeze-dark\nother-setting=keep\n"
                )
            palette.apply_palette(home, "/tmp/wallpaper.png", live=False)
            css = (home / ".config/gtk-4.0/gtk.css").read_text()
            self.assertIn(
                "--sidebar-bg-color: #" + colors["surfaceContainerLow"].lstrip("#"), css
            )
            self.assertIn(
                "--sidebar-backdrop-color: #"
                + colors["surfaceContainerLow"].lstrip("#"),
                css,
            )
            self.assertIn("--headerbar-bg-color:", css)
            for version in ("3.0", "4.0"):
                ini = (home / (".config/gtk-" + version) / "settings.ini").read_text()
                self.assertRegex(
                    ini,
                    r"gtk-icon-theme-name=(?:Papirus-Dark|Nacre-Papirus-v[23]-[a-z]+-dark)\n",
                )
                theme_name = next(
                    line.split("=", 1)[1]
                    for line in ini.splitlines()
                    if line.startswith("gtk-icon-theme-name=")
                )
                self.assertTrue(
                    (home / ".local/share/icons" / theme_name / "index.theme").is_file()
                )
                self.assertIn("other-setting=keep", ini)
            self.assertNotIn(":root", (home / ".config/gtk-3.0/gtk.css").read_text())

    def test_fixed_palette_survives_different_wallpaper_colors_and_modes(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            state = home / ".local/state/nacre"
            state.mkdir(parents=True)
            prefs = home / ".config/nacre/wallpaper-picker.json"
            prefs.parent.mkdir(parents=True)
            prefs.write_text(
                json.dumps({"palettePreset": "ocean", "paletteMode": "dark"})
            )
            presets = json.loads(
                Path(palette.__file__).with_name("palette-presets.json").read_text()
            )
            expected = next(p for p in presets if p["id"] == "ocean")["modes"]["dark"]
            for primary, mode in [("ff0000", "light"), ("00ff00", "dark")]:
                colors = json.loads(
                    Path(palette.__file__).with_name("reference-style.json").read_text()
                )["colours"]
                colors["primary"] = primary
                (state / "scheme.json").write_text(
                    json.dumps({"name": "dynamic", "mode": mode, "colours": colors})
                )
                palette.apply_palette(home, "/tmp/" + primary + ".png", live=False)
                presentation = json.loads((state / "presentation.json").read_text())
                self.assertEqual(presentation["colours"], expected)
                self.assertEqual(presentation["mode"], "dark")
                self.assertEqual(presentation["poster"], "/tmp/" + primary + ".png")
            prefs.write_text(json.dumps({"palettePreset": "wallpaper"}))
            colors["primary"] = "ffaabb"
            (state / "scheme.json").write_text(
                json.dumps({"mode": "light", "colours": colors})
            )
            palette.apply_palette(home, "/tmp/follow.png", live=False)
            self.assertEqual(
                json.loads((state / "presentation.json").read_text())["colours"][
                    "primary"
                ],
                "ffaabb",
            )

    def test_vivid_presets_are_stronger_than_pastel_material_accents(self):
        import colorsys

        presets = json.loads(
            Path(palette.__file__).with_name("palette-presets.json").read_text()
        )
        self.assertEqual(len(presets), 30)
        self.assertEqual(len({p["id"] for p in presets}), 30)
        vivid = [p for p in presets if p["group"] == "vivid"]
        soft = [p for p in presets if p["id"].startswith("soft-")]
        for preset, softened in zip(vivid, soft, strict=True):
            colors = preset["modes"]["dark"]
            rgb = [int(colors["primary"][i : i + 2], 16) / 255 for i in (0, 2, 4)]
            soft_rgb = [
                int(softened["modes"]["dark"]["primary"][i : i + 2], 16) / 255
                for i in (0, 2, 4)
            ]
            saturation = colorsys.rgb_to_hsv(*rgb)[1]
            # Indigo needs more luminance for text contrast; measure it against
            # its actual soft counterpart rather than imposing full saturation.
            self.assertGreater(saturation, 0.5)
            self.assertGreater(saturation - colorsys.rgb_to_hsv(*soft_rgb)[1], 0.2)
            a, b = sorted(
                [
                    palette.luminance(colors["primary"]),
                    palette.luminance(colors["surfaceContainer"]),
                ]
            )
            self.assertGreaterEqual((b + 0.05) / (a + 0.05), 4.5)

    def test_terminal_accents_follow_palette_in_dark_and_light_modes(self):
        reference = json.loads(
            (Path(__file__).resolve().parents[1] / "reference-style.json").read_text()
        )["colours"]
        for mode in ("dark", "light"):
            with self.subTest(mode=mode), tempfile.TemporaryDirectory() as folder:
                home = Path(folder)
                state = home / ".local/state/nacre"
                state.mkdir(parents=True)
                previous = None
                for accent, support in (("dd6644", "daa853"), ("668add", "b186cc")):
                    colors = {**reference, "primary": accent, "secondary": support}
                    (state / "scheme.json").write_text(
                        json.dumps({"mode": mode, "colours": colors})
                    )
                    palette.apply_palette(home, live=False)
                    values = dict(
                        line.split(maxsplit=1)
                        for line in (home / ".config/nacre/kitty-colors.conf")
                        .read_text()
                        .splitlines()
                    )
                    background = colors[
                        "inverseSurface" if mode == "light" else "surfaceContainerLow"
                    ].lstrip("#")
                    self.assertEqual(values["background"], "#" + background)
                    secondary = "#" + palette.readable(support, background)
                    self.assertEqual(values["color5"], secondary)
                    self.assertEqual(values["color13"], secondary)
                    self.assertEqual(values["background_opacity"], "0.92")
                    if previous is not None:
                        self.assertNotEqual(previous, secondary)
                    previous = secondary

    def test_presets_have_all_roles_and_readable_foreground_pairs(self):
        presets = json.loads(
            Path(palette.__file__).with_name("palette-presets.json").read_text()
        )
        roles = json.loads(
            Path(palette.__file__).with_name("reference-style.json").read_text()
        )["colours"]
        for preset in presets:
            for colors in preset["modes"].values():
                self.assertTrue(roles.keys() <= colors.keys())
                for foreground, background in [
                    ("onSurface", "surface"),
                    ("onPrimary", "primary"),
                    ("onSecondary", "secondary"),
                ]:
                    a, b = sorted(
                        [
                            palette.luminance(colors[foreground]),
                            palette.luminance(colors[background]),
                        ]
                    )
                    self.assertGreaterEqual((b + 0.05) / (a + 0.05), 4.5)

    def test_photo_timestamp_does_not_reset_on_palette_republication(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            state = home / ".local/state/nacre"
            state.mkdir(parents=True)
            colors = json.loads(
                Path(palette.__file__).with_name("reference-style.json").read_text()
            )["colours"]
            (state / "scheme.json").write_text(
                json.dumps({"mode": "dark", "colours": colors})
            )
            with patch.object(palette.time, "time", return_value=1000):
                palette.apply_palette(home, "/tmp/a.png", live=False)
            with patch.object(palette.time, "time", return_value=2000):
                palette.apply_palette(home, "/tmp/a.png", live=False)
            self.assertEqual(
                json.loads((state / "presentation.json").read_text())["changedAtMs"],
                1000000,
            )
            with patch.object(palette.time, "time", return_value=3000):
                palette.apply_palette(home, "/tmp/b.png", live=False)
            self.assertEqual(
                json.loads((state / "presentation.json").read_text())["changedAtMs"],
                3000000,
            )

    def test_prepared_commit_uses_shared_publisher_and_preserves_cli_identity(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            state = home / ".local/state/nacre"
            state.mkdir(parents=True)
            colors = json.loads(
                (
                    Path(__file__).resolve().parents[1] / "reference-style.json"
                ).read_text()
            )["colours"]
            data = {
                "name": "dynamic",
                "flavour": "default",
                "variant": "tonalspot",
                "mode": "dark",
                "colours": colors,
            }
            (state / "scheme.json").write_text(json.dumps(data))
            image = home / "wall.png"
            image.write_bytes(b"fixture")
            thumbnail = home / ".cache/nacre/wallpapers/key/thumbnail.jpg"
            thumbnail.parent.mkdir(parents=True)
            thumbnail.write_bytes(b"fixture")
            with patch.object(palette, "apply_palette") as publisher:
                self.assertTrue(
                    palette.commit_prepared(home, image, data, thumbnail, live=False)
                )
                publisher.assert_called_once_with(home, str(image), live=False)
            self.assertEqual((state / "wallpaper/current").resolve(), image)
            self.assertEqual((state / "wallpaper/thumbnail.jpg").resolve(), thumbnail)
            before = (state / "scheme.json").read_bytes()
            bad = dict(data, colours=dict(colors, primary="bad"))
            self.assertFalse(
                palette.commit_prepared(home, image, bad, thumbnail, live=False)
            )
            self.assertEqual((state / "scheme.json").read_bytes(), before)
            config = home / ".config/nacre/cli.json"
            config.parent.mkdir(parents=True)
            config.write_text(json.dumps({"wallpaper": {"postHook": "custom hook"}}))
            self.assertFalse(
                palette.commit_prepared(home, image, data, thumbnail, live=False)
            )

    def test_dim_ansi_text_remains_readable_on_light_and_dark_backgrounds(self):
        for background in ("fbf9f8", "141318"):
            for original in ("dfe3e3", "ffffff", "000000", "008f68", "424848"):
                result = palette.readable(original, background)
                a, b = palette.luminance(result), palette.luminance(background)
                self.assertGreaterEqual((max(a, b) + 0.05) / (min(a, b) + 0.05), 4.5)

    def test_invalid_palette_does_not_replace_committed_state(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            state = home / ".local/state/nacre"
            (state / "scheme").mkdir(parents=True)
            current = state / "scheme/current.txt"
            current.write_text("previous palette")
            (state / "scheme.json").write_text(
                json.dumps({"mode": "light", "colours": {"primary": "not a hex color"}})
            )
            with self.assertRaises(ValueError):
                palette.apply_palette(home, live=False)
            self.assertEqual(current.read_text(), "previous palette")
            self.assertFalse((home / ".config/nacre/palette.lua").exists())


class PublicationBoundaryTests(unittest.TestCase):
    def seed(self, home, missing=None):
        state = home / ".local/state/nacre"
        state.mkdir(parents=True)
        colors = json.loads(
            Path(palette.__file__).with_name("reference-style.json").read_text()
        )["colours"]
        if missing:
            colors.pop(missing)
        (state / "scheme.json").write_text(
            json.dumps({"mode": "dark", "colours": colors})
        )
        return state

    def snapshot(self, home):
        return {
            str(p.relative_to(home)): p.read_bytes()
            for p in home.rglob("*")
            if p.is_file() and p.name != "palette-commit.lock"
        }

    def probe(self):
        return (
            "import importlib.util,sys; from pathlib import Path; "
            "s=importlib.util.spec_from_file_location('publisher',sys.argv[1]); "
            "p=importlib.util.module_from_spec(s); s.loader.exec_module(p); "
            "print('ready',flush=True); p.apply_palette(Path(sys.argv[2]),'/tmp/isolated-poster.png',live=False)"
        )

    def test_missing_late_role_rejects_before_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            self.seed(home, missing="onPrimary")
            before = self.snapshot(home)
            with self.assertRaises(ValueError):
                palette.apply_palette(home, "/tmp/isolated-poster.png", live=False)
            self.assertEqual(self.snapshot(home), before)

    def test_direct_publisher_waits_for_real_lock_even_with_stale_marker(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            state = self.seed(home)
            with (state / "palette-commit.lock").open("a") as lock:
                fcntl.flock(lock, fcntl.LOCK_EX)
                process = subprocess.Popen(
                    [sys.executable, "-c", self.probe(), palette.__file__, str(home)],
                    env={**os.environ, "NACRE_PALETTE_LOCKED": "1"},
                    stdout=subprocess.PIPE,
                    stderr=subprocess.PIPE,
                    text=True,
                )
                try:
                    self.assertEqual(process.stdout.readline().strip(), "ready")
                    time.sleep(0.2)
                    self.assertFalse((state / "presentation.json").exists())
                    self.assertIsNone(process.poll())
                finally:
                    fcntl.flock(lock, fcntl.LOCK_UN)
                    stdout, stderr = process.communicate(timeout=15)
                self.assertEqual(process.returncode, 0, stderr)
                self.assertTrue((state / "presentation.json").is_file())

    def test_actual_prepared_commit_can_reenter_shared_publisher(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            state = self.seed(home)
            data = json.loads((state / "scheme.json").read_text())
            data.update(name="dynamic", flavour="default", variant="tonalspot")
            (state / "scheme.json").write_text(json.dumps(data))
            (home / "wall.png").write_bytes(b"fixture")
            thumbnail = home / ".cache/nacre/wallpapers/probe/thumbnail.jpg"
            thumbnail.parent.mkdir(parents=True)
            thumbnail.write_bytes(b"fixture")
            script = (
                "import importlib.util,json,sys; from pathlib import Path; "
                "s=importlib.util.spec_from_file_location('publisher',sys.argv[1]); "
                "p=importlib.util.module_from_spec(s); s.loader.exec_module(p); "
                "h=Path(sys.argv[2]); data=json.loads((h/'.local/state/nacre/scheme.json').read_text()); "
                "assert p.commit_prepared(h,h/'wall.png',data,h/'.cache/nacre/wallpapers/probe/thumbnail.jpg',live=False)"
            )
            result = subprocess.run(
                [sys.executable, "-c", script, palette.__file__, str(home)],
                capture_output=True,
                text=True,
                timeout=15,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                json.loads((state / "presentation.json").read_text())["poster"],
                str(home / "wall.png"),
            )

    @unittest.skipUnless(
        shutil.which("bash") and shutil.which("flock"), "Bash/flock unavailable"
    )
    def test_cli_inherited_descriptor_does_not_deadlock_publisher(self):
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            state = self.seed(home)
            marker = home / "published"
            process = subprocess.Popen(
                [
                    "bash",
                    "-c",
                    'exec 9>"$1"; flock 9; export NACRE_PALETTE_LOCKED=1; "$2" -c "$3" "$4" "$5" || exit $?; printf done >"$6"; read -r release',
                    "probe",
                    str(state / "palette-commit.lock"),
                    sys.executable,
                    self.probe(),
                    palette.__file__,
                    str(home),
                    str(marker),
                ],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                start_new_session=True,
            )
            try:
                deadline = time.monotonic() + 15
                while (
                    not marker.exists()
                    and process.poll() is None
                    and time.monotonic() < deadline
                ):
                    time.sleep(0.02)
                self.assertTrue(marker.exists(), "CLI publisher failed or deadlocked")
                self.assertIsNone(process.poll())
                with (state / "palette-commit.lock").open("a") as competitor:
                    with self.assertRaises(BlockingIOError):
                        fcntl.flock(competitor, fcntl.LOCK_EX | fcntl.LOCK_NB)
            finally:
                try:
                    stdout, stderr = process.communicate(input="\n", timeout=3)
                except subprocess.TimeoutExpired:
                    os.killpg(process.pid, signal.SIGTERM)
                    stdout, stderr = process.communicate(timeout=5)
            self.assertEqual(process.returncode, 0, stderr)
            self.assertTrue((state / "presentation.json").is_file())


if __name__ == "__main__":
    unittest.main()
