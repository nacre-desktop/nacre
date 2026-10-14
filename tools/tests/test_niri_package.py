"""Companion distribution, package trust and automatic preparation integration."""

import importlib.util
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]


def load(relative, name):
    spec = importlib.util.spec_from_file_location(name, ROOT / relative)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result


stage = load("packaging/arch/niri/stage.py", "companion_stage_test")
recipe = load("packaging/arch/niri/recipe.py", "companion_recipe_test")
import native_niri_session_runtime as runtime


class NiriPackageTests(unittest.TestCase):
    def test_companion_layout_and_package_receipt_survive_debug_split(self):
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            source = base / "source"
            binary = source / "target/release/niri"
            binary.parent.mkdir(parents=True)
            binary.write_text("compiled fixture with debug tables")
            (source / "LICENSE").write_text("upstream license")
            destination = base / "package"
            with patch.object(stage.builder, "prepare", return_value=source):
                stage.stage(source, destination)
            companion = destination / "usr/lib/nacre/niri"
            self.assertFalse((destination / "usr/bin/niri").exists())
            self.assertFalse((destination / "home").exists())
            self.assertIn(
                "NACRE_NIRI_HOVER_STREAM=1",
                (destination / "usr/bin/nacre-niri").read_text(),
            )
            self.assertEqual((companion / "niri").stat().st_mode & 0o777, 0o755)
            self.assertTrue(
                (destination / "usr/share/licenses/nacre-niri/LICENSE.niri").exists()
            )
            # Pacman owns system artifacts, whereas private builds use exact hashes.
            # Simulate makepkg stripping debug tables after package() finishes.
            (companion / "niri").write_text("stripped compiled fixture")
            with patch.object(
                runtime.subprocess, "check_output", return_value="niri 26.04 (pinned)"
            ):
                self.assertEqual(
                    runtime.verified_hover_binary(
                        companion, os.getuid(), packaged=True
                    ),
                    companion / "niri",
                )
                (companion / "manifest.json").chmod(0o666)
                self.assertIsNone(
                    runtime.verified_hover_binary(companion, os.getuid(), packaged=True)
                )

    def test_recipe_pins_both_sources_and_checks_protocol_without_user_hooks(self):
        text = recipe.render(ROOT, "a" * 40)
        self.assertIn("#commit=" + "a" * 40, text)
        self.assertIn("#commit=" + stage.builder.SOURCE, text)
        self.assertIn("--locked --lib nacre_overview_hover_tests", text)
        self.assertNotIn("--no-default-features", text)
        self.assertNotIn("install=", text)
        self.assertNotIn("!debug", text)

    def test_package_precedence_and_migration_fallback(self):
        home = Path("/fixture")
        package_binary = runtime.PACKAGED_HOVER / "niri"
        with patch.object(
            runtime, "verified_hover_binary", return_value=package_binary
        ) as check:
            self.assertEqual(runtime.overview_hover_binary(home), package_binary)
            check.assert_called_once_with(runtime.PACKAGED_HOVER, 0, packaged=True)
        old = home / ".local/share/nacre/niri-on-pc/overview-hover/niri"
        with patch.object(
            runtime, "verified_hover_binary", side_effect=[None, None, old]
        ):
            self.assertEqual(runtime.overview_hover_binary(home), old)

    def test_automatic_prepare_skips_build_for_installed_package_and_checks_result(
        self,
    ):
        home = Path("/fixture")
        with (
            patch.object(
                runtime, "overview_hover_binary", return_value=Path("/package/niri")
            ),
            patch.object(runtime.subprocess, "run") as run,
        ):
            runtime.ensure_overview_hover(ROOT, home)
            run.assert_not_called()
        with (
            patch.object(
                runtime,
                "overview_hover_binary",
                side_effect=[None, Path("/built/niri")],
            ),
            patch.object(runtime.subprocess, "run") as run,
        ):
            runtime.ensure_overview_hover(ROOT, home)
            self.assertEqual(run.call_args.args[0][-1], "--install")
        with (
            patch.object(runtime, "overview_hover_binary", return_value=None),
            patch.object(runtime.subprocess, "run"),
        ):
            with self.assertRaisesRegex(RuntimeError, "usable companion"):
                runtime.ensure_overview_hover(ROOT, home)

    def test_receipt_with_wrong_source_never_selects_companion(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            (root / "manifest.json").write_text(
                json.dumps({"schema": 1, "enabled": True, "sourceRevision": "other"})
            )
            self.assertIsNone(
                runtime.verified_hover_binary(root, os.getuid(), packaged=True)
            )
