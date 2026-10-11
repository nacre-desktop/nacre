"""Pinned font install ownership, drift and rollback behavior."""

import hashlib
import importlib.util
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "fonts", Path(__file__).parents[1] / "font-setup.py"
)
fonts = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fonts)


def fixture():
    payload = b"verified font fixture"
    row = {
        "name": "Font.ttf",
        "url": "https://example.invalid/font",
        "size": len(payload),
        "sha256": hashlib.sha256(payload).hexdigest(),
    }
    manifest = {
        "version": 1,
        "groups": [{"id": "test", "default": True, "files": [row]}],
    }
    return manifest, payload


class FontSetupTests(unittest.TestCase):
    def test_verified_install_preserves_custom_files_is_repeatable_and_reversible(self):
        manifest, payload = fixture()
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            target, registry = fonts.roots(home)
            target.mkdir(parents=True)
            (target / "custom.ttf").write_bytes(b"user content")
            result = fonts.apply(
                home, manifest, fetch=lambda row: payload, refresh=False
            )
            self.assertEqual(target.joinpath("Font.ttf").read_bytes(), payload)
            with patch.object(
                fonts, "download", side_effect=AssertionError("Network not needed")
            ):
                self.assertFalse(
                    fonts.apply(home, manifest, fetch=fonts.download, refresh=False)[
                        "changed"
                    ]
                )
            fonts.restore(
                home, registry.parent / "backups" / result["transaction"], refresh=False
            )
            self.assertEqual(
                target.joinpath("custom.ttf").read_bytes(), b"user content"
            )
            self.assertFalse(target.joinpath("Font.ttf").exists())

    def test_local_drift_refuses_before_any_font_write(self):
        manifest, payload = fixture()
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            target, _ = fonts.roots(home)
            target.mkdir(parents=True)
            target.joinpath("Font.ttf").write_bytes(b"custom edit")
            with self.assertRaisesRegex(ValueError, "Local font edit"):
                fonts.apply(home, manifest, fetch=lambda row: payload, refresh=False)
            self.assertEqual(target.joinpath("Font.ttf").read_bytes(), b"custom edit")

    def test_bad_download_is_rejected_before_promotion(self):
        manifest, _ = fixture()
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                fonts.apply(home, manifest, fetch=lambda row: b"wrong", refresh=False)
            self.assertFalse(fonts.roots(home)[0].exists())

    def test_cache_failure_restores_files_and_refuses_later_rollback_drift(self):
        manifest, payload = fixture()
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            target, registry = fonts.roots(home)
            with patch.object(
                fonts,
                "cache_refresh",
                side_effect=[RuntimeError("cache failure"), None],
            ):
                with self.assertRaisesRegex(RuntimeError, "cache failure"):
                    fonts.apply(home, manifest, fetch=lambda row: payload)
            self.assertFalse(target.joinpath("Font.ttf").exists())
            result = fonts.apply(
                home, manifest, fetch=lambda row: payload, refresh=False
            )
            target.joinpath("Font.ttf").write_bytes(b"later user edit")
            with self.assertRaisesRegex(ValueError, "Later font edit"):
                fonts.restore(
                    home,
                    registry.parent / "backups" / result["transaction"],
                    refresh=False,
                )
            self.assertEqual(
                target.joinpath("Font.ttf").read_bytes(), b"later user edit"
            )


if __name__ == "__main__":
    unittest.main()
