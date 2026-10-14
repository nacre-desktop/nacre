import importlib.util
from pathlib import Path
import tempfile
from unittest.mock import patch
import unittest

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    if name == "kde-palette":
        import native_kde_palette

        return native_kde_palette
    spec = importlib.util.spec_from_file_location(name, ROOT / (name + ".py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class FileManagerTests(unittest.TestCase):
    def test_folder_palette_overlay_is_small_and_reused(self):
        icons = load("file-icons")
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            base = home / ".local/share/icons"
            for name in ["Papirus", "Papirus-Dark"]:
                (base / name).mkdir(parents=True)
                (base / name / "index.theme").write_text("[Icon Theme]\n")
            source = base / "Papirus/64x64/places"
            source.mkdir(parents=True)
            (source / "folder-red.svg").write_text("<svg/>")
            (source / "folder-red-documents.svg").write_text("<svg/>")
            chosen = icons.theme(home, "ff0000", "dark")
            target = base / chosen
            self.assertTrue((target / "64x64/places/folder.svg").is_symlink())
            self.assertTrue((target / "64x64/places/inode-directory.svg").is_symlink())
            self.assertTrue((target / "64x64/places/folder-documents.svg").is_symlink())
            stamp = (target / "index.theme").stat().st_mtime_ns
            self.assertEqual(icons.theme(home, "ff1010", "dark"), chosen)
            self.assertEqual((target / "index.theme").stat().st_mtime_ns, stamp)
            self.assertIn(
                "Inherits=Papirus-Dark,Papirus,Adwaita,hicolor",
                (target / "index.theme").read_text(),
            )

    def test_owned_overlay_retargets_stale_links_and_preserves_regular_overrides(self):
        icons = load("file-icons")
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            base = home / ".local/share/icons"
            for name in ["Papirus", "Papirus-Dark"]:
                (base / name).mkdir(parents=True)
                (base / name / "index.theme").write_text("[Icon Theme]\n")
            source = base / "Papirus/64x64/places"
            source.mkdir(parents=True)
            (source / "folder-red.svg").write_text("<svg/>")
            (source / "folder-red-documents.svg").write_text("<svg/>")
            (source / "folder-red-download.svg").write_text("<svg/>")
            with patch.object(
                icons, "SYSTEM_ICONS", home / "absent-system-icons", create=True
            ):
                chosen = icons.theme(home, "ff0000", "dark")
                target = base / chosen / "64x64/places"
                folder = target / "folder.svg"
                folder.unlink()
                folder.symlink_to(home / "retired-runtime/folder.svg")
                custom = target / "folder-download.svg"
                custom.unlink()
                custom.write_text("custom artwork")
                icons.theme(home, "ff0000", "dark")
                self.assertEqual(folder.resolve(strict=True), source / "folder-red.svg")
                self.assertEqual(custom.read_text(), "custom artwork")
                self.assertFalse(custom.is_symlink())

    def test_custom_index_and_legacy_scheme_are_preserved(self):
        icons = load("file-icons")
        kde = load("kde-palette")
        import json

        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            base = home / ".local/share/icons"
            for name in ["Papirus", "Papirus-Dark"]:
                (base / name).mkdir(parents=True)
                (base / name / "index.theme").write_text("[Icon Theme]\n")
            source = base / "Papirus/64x64/places"
            source.mkdir(parents=True)
            (source / "folder-red.svg").write_text("<svg/>")
            with patch.object(icons, "SYSTEM_ICONS", home / "absent-system-icons"):
                name = icons.theme(home, "ff0000", "dark")
                index = base / name / "index.theme"
                index.write_text("[Icon Theme]\nName=Custom user theme\n")
                icons.theme(home, "ff0000", "dark")
                self.assertEqual(
                    index.read_text(), "[Icon Theme]\nName=Custom user theme\n"
                )
            old = home / ".local/share/color-schemes/Custom.colors"
            old.parent.mkdir(parents=True)
            old.write_text("private custom scheme")
            colors = json.loads((ROOT / "palette-presets.json").read_text())[0][
                "modes"
            ]["dark"]
            kde.publish(home, colors, name)
            self.assertEqual(old.read_text(), "private custom scheme")
            self.assertTrue((old.parent / "Nacre.colors").is_file())

    def test_kde_roles_preserve_user_settings_and_selection_contrast(self):
        import configparser
        import json

        kde = load("kde-palette")
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            config = home / ".config/kdeglobals"
            config.parent.mkdir(parents=True)
            config.write_text(
                "[General]\nTerminalApplication=kitty\n[UserSection]\nSetting=preserve\n"
            )
            presets = json.loads((ROOT / "palette-presets.json").read_text())
            colors = next(p for p in presets if p["id"] == "ocean")["modes"]["dark"]
            kde.publish(home, colors, "ExampleNacreIcons")
            self.assertFalse(
                (home / ".local/share/color-schemes/Custom.colors").exists()
            )
            parsed = configparser.ConfigParser(interpolation=None)
            parsed.read(config)
            self.assertEqual(parsed["General"]["ColorScheme"], "Nacre")
            self.assertEqual(parsed["General"]["TerminalApplication"], "kitty")
            self.assertEqual(parsed["UserSection"]["Setting"], "preserve")
            self.assertEqual(
                parsed["Colors:View"]["BackgroundNormal"], "#" + colors["surface"]
            )
            self.assertEqual(
                parsed["Colors:Selection"]["ForegroundInactive"],
                "#" + colors["onPrimary"],
            )
            self.assertEqual(
                parsed["Colors:Selection"]["BackgroundAlternate"],
                "#" + colors["primary"],
            )
            self.assertTrue((home / ".local/share/color-schemes/Nacre.colors").exists())
