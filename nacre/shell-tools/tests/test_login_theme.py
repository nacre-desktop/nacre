import importlib.util
import tempfile
import unittest
import os
import configparser
from unittest.mock import patch
from pathlib import Path
from PIL import Image

BASE = Path(__file__).resolve().parents[1]


def module(name):
    s = importlib.util.spec_from_file_location(name, BASE / (name + ".py"))
    m = importlib.util.module_from_spec(s)
    s.loader.exec_module(m)
    return m


appearance = module("login-appearance")
installer = module("login-install")


class LoginTests(unittest.TestCase):
    def test_data_only_publication_uses_private_preview_and_public_palette(self):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            image = base / "wallpaper.png"
            Image.new("RGB", (80, 40), "blue").save(image)
            preview = base / "private"
            public = base / "public"
            public.mkdir()
            colors = {v: "aabbcc" for v in appearance.ROLES.values()}
            with patch.object(appearance, "HOME", base):
                appearance.publish(colors, image, preview, public)
            self.assertTrue((base / ".cache/nacre/login-images").is_dir())
            self.assertEqual(preview.stat().st_mode & 0o777, 0o700)
            self.assertEqual((public / "theme.conf").stat().st_mode & 0o777, 0o644)
            conf = configparser.ConfigParser()
            conf.read(public / "theme.conf")
            self.assertEqual(conf["General"]["primary"], "#aabbcc")
            self.assertEqual(
                conf["General"]["background"], str(public / "background.png")
            )
            before = (public / "theme.conf").read_bytes()
            colors["primary"] = "bad\n[Autologin]"
            with patch.object(appearance, "HOME", base), self.assertRaises(ValueError):
                appearance.publish(colors, image, preview, public)
            self.assertEqual((public / "theme.conf").read_bytes(), before)

    def test_prepared_login_image_is_reused_and_invalidated_by_source_change(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            source = base / "image.png"
            Image.new("RGB", (80, 40), "red").save(source)
            first, _ = appearance.prepare(source, base / "cache")
            stamp = first.stat().st_mtime_ns
            with patch.object(
                appearance.Image,
                "open",
                side_effect=AssertionError("Must use the cache"),
            ):
                reused, _ = appearance.prepare(source, base / "cache")
            self.assertEqual(first, reused)
            self.assertEqual(stamp, reused.stat().st_mtime_ns)
            Image.new("RGB", (80, 40), "blue").save(source)
            changed, _ = appearance.prepare(source, base / "cache")
            self.assertNotEqual(first, changed)

    def test_system_install_keeps_authentication_and_backs_up_previous_theme_selection(
        self,
    ):
        with tempfile.TemporaryDirectory() as d:
            base = Path(d)
            source = base / "source"
            source.mkdir()
            for name in installer.FILES:
                (source / name).write_text("fixture")
            main = base / "etc/sddm.conf"
            main.parent.mkdir()
            main.write_text("[Autologin]\nSession=hyprland\n")
            selected = base / "etc/sddm.conf.d/90-nacre-theme.conf"
            selected.parent.mkdir()
            selected.write_text("[Theme]\nCurrent=old\n")
            backup = installer.install(
                source, "fixture", base, os.getuid(), os.getgid()
            )
            self.assertEqual(main.read_text(), "[Autologin]\nSession=hyprland\n")
            self.assertEqual(selected.read_text(), "[Theme]\nCurrent=nacre\n")
            self.assertEqual(
                (backup / "etc/sddm.conf.d/90-nacre-theme.conf").read_text(),
                "[Theme]\nCurrent=old\n",
            )
            self.assertTrue(
                (base / "usr/share/sddm/themes/nacre/theme.conf.user").is_symlink()
            )
