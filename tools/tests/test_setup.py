import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).parents[1]
sys.path.insert(0, str(TOOLS))
import package_runtime
import releases
import setup


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


class PackageSetupTests(unittest.TestCase):
    def fixture(self, folder):
        root = Path(folder) / "package"
        home = Path(folder) / "home"
        home.mkdir()
        (root / "nacre/shell-tools").mkdir(parents=True)
        (root / "tools").mkdir()
        for name in ("profile-policy.py", "compositor_config.py"):
            (root / "nacre/shell-tools" / name).write_bytes(
                (TOOLS.parent / "nacre/shell-tools" / name).read_bytes()
            )
        backend = root / "nacre/shell-tools/compositor_backends"
        backend.mkdir()
        (backend / "hyprland_config.py").write_bytes(
            (
                TOOLS.parent
                / "nacre/shell-tools/compositor_backends/hyprland_config.py"
            ).read_bytes()
        )
        (root / "tools/configure.py").write_bytes((TOOLS / "configure.py").read_bytes())
        (root / "package.json.installed").write_text(
            json.dumps(
                {"version": "test", "revision": "fixture", "compositors": ["hyprland"]}
            )
        )
        config = root / "deployment/core/.config/nacre/shortcuts.lua"
        config.parent.mkdir(parents=True)
        config.write_text('path="@NACRE_HOME@"\n')
        profile = setup.policy(root).load(home)
        return root, home, profile

    def test_apply_is_offline_repeatable_preserves_private_state_and_rolls_back(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            account = home / ".codex/auth.json"
            account.parent.mkdir()
            account.write_text("private fixture")
            with (
                patch.object(releases, "HOME", home),
                patch.object(releases, "STATE", home / "releases"),
                patch.object(releases.subprocess, "run") as run,
            ):
                record = releases.deploy_package(
                    root,
                    profile,
                    {"layout": "no", "variant": "", "model": "", "options": ""},
                    "hyprland",
                )
                run.assert_not_called()
                self.assertIn('"no"', (home / ".config/nacre/keyboard.lua").read_text())
                self.assertIn(
                    str(home), (home / ".config/nacre/shortcuts.lua").read_text()
                )
                self.assertTrue((home / ".config/quickshell/nacre").is_symlink())
                self.assertEqual(record["status"], "prepared")
                self.assertFalse(
                    any(".codex" in entry["path"] for entry in record["entries"])
                )
                releases.restore(Path(record["release"]), record)
                run.assert_not_called()
                self.assertFalse((home / ".config/nacre/keyboard.lua").exists())
                self.assertFalse((home / ".config/quickshell/nacre").is_symlink())
                self.assertEqual(account.read_text(), "private fixture")

    def test_failed_apply_restores_keyboard_config_and_runtime(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            with (
                patch.object(releases, "HOME", home),
                patch.object(releases, "STATE", home / "releases"),
                patch.object(
                    package_runtime,
                    "apply",
                    side_effect=RuntimeError("fixture failure"),
                ),
            ):
                with self.assertRaisesRegex(RuntimeError, "fixture failure"):
                    releases.deploy_package(root, profile, {"layout": "de"}, "hyprland")
                self.assertFalse((home / ".config/nacre/shortcuts.lua").exists())
                self.assertFalse((home / ".config/nacre/keyboard.lua").exists())

    def test_local_conflict_is_rejected_before_snapshot_or_write(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            path = home / ".config/nacre/shortcuts.lua"
            path.parent.mkdir(parents=True)
            path.write_text("personal")
            with (
                patch.object(releases, "HOME", home),
                patch.object(releases, "STATE", home / "releases"),
            ):
                with self.assertRaisesRegex(RuntimeError, "Local edit preserved"):
                    releases.deploy_package(root, profile, {"layout": "us"}, "hyprland")
                self.assertEqual(path.read_text(), "personal")
                self.assertFalse(any((home / "releases").glob("*/release.json")))

    def test_runtime_conflicts_preserve_foreign_and_broken_links(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            path = home / ".config/quickshell/nacre"
            path.parent.mkdir(parents=True)
            path.symlink_to(home / "foreign")
            with self.assertRaisesRegex(RuntimeError, "Local runtime preserved"):
                package_runtime.plan(home, root, profile)
            self.assertTrue(path.is_symlink())

    def test_seeded_palette_changes_and_existing_keyboard_survive_repeat_setup(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            seed = root / "deployment/core/.local/state/nacre/scheme.json"
            seed.parent.mkdir(parents=True)
            seed.write_text("initial")
            (root / "seed-paths.json").write_text(
                json.dumps([".local/state/nacre/scheme.json"])
            )
            with (
                patch.object(releases, "HOME", home),
                patch.object(releases, "STATE", home / "releases"),
            ):
                releases.deploy_package(root, profile, {"layout": "no"}, "hyprland")
                (home / ".local/state/nacre/scheme.json").write_text("private palette")
                releases.deploy_package(root, profile, {"layout": "us"}, "hyprland")
                self.assertEqual(
                    (home / ".local/state/nacre/scheme.json").read_text(),
                    "private palette",
                )
                self.assertIn('"no"', (home / ".config/nacre/keyboard.lua").read_text())

    def test_niri_hidden_until_packaged_setup_contract_is_ready(self):
        with tempfile.TemporaryDirectory() as folder:
            root, _, _ = self.fixture(folder)
            self.assertEqual(setup.supported_compositors(root), ["hyprland"])

    def test_distro_alias_mapping_and_unsupported_os(self):
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / "os-release"
            path.write_text('ID=cachyos\nID_LIKE="arch"\n')
            self.assertEqual(setup.distro({"arch": {}}, path), "arch")
            path.write_text("ID=unknown\n")
            with self.assertRaises(RuntimeError):
                setup.distro({"arch": {}}, path)

    def test_optional_component_requires_installed_package(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, _ = self.fixture(folder)
            args = setup.argparse.Namespace(
                layout="us",
                variant=None,
                model=None,
                options=None,
                compositor="hyprland",
                profile=None,
                optional=["brain"],
            )
            with self.assertRaisesRegex(RuntimeError, "Install nacre-brain"):
                setup.selection(args, root, home, False)
