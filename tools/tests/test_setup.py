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

    def test_invalid_runtime_manifest_cannot_address_files_outside_managed_paths(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            manifest = home / ".local/state/nacre/package-links.json"
            manifest.parent.mkdir(parents=True)
            manifest.write_text(json.dumps({"/tmp/outside": "/tmp/foreign"}))
            with self.assertRaisesRegex(ValueError, "Invalid package runtime manifest"):
                package_runtime.plan(home, root, profile)

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


class ArchPackageLayoutTests(unittest.TestCase):
    def test_split_packages_have_no_file_collisions_and_ship_native_artifacts(self):
        stage = load(TOOLS.parent / "packaging/arch/stage.py", "test_arch_stage")
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            build = base / "build"
            build.mkdir()
            (build / "revision").write_text("fixture")
            private_default = build / "defaults/.config/kdeglobals"
            private_default.parent.mkdir(parents=True)
            private_default.write_text("fixture palette")
            private_default.chmod(0o600)
            (build / "fonts").mkdir()
            for name in (
                "dolphin-style/styles/libnacre-dolphin.so",
                "thunar-style/nacre-thunar-theme.so",
            ):
                path = build / "native" / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture compiled bytes")
            owned = set()
            for component in ("core", "ai", "brain"):
                destination = base / component
                stage.stage(TOOLS.parent, destination, component, build)
                files = {
                    p.relative_to(destination)
                    for p in destination.rglob("*")
                    if p.is_file() or p.is_symlink()
                }
                self.assertFalse(
                    owned & files, f"Split file collision: {owned & files}"
                )
                self.assertTrue(all(path.parts[0] == "usr" for path in files))
                for path in destination.rglob("*"):
                    if path.is_file() and not path.is_symlink():
                        self.assertEqual(path.stat().st_mode & 0o044, 0o044, str(path))
                owned |= files
            self.assertIn(
                Path("usr/lib/nacre/dolphin-style/styles/libnacre-dolphin.so"), owned
            )
            self.assertIn(
                Path("usr/lib/nacre/thunar-style/nacre-thunar-theme.so"), owned
            )
            self.assertNotIn(Path("usr/bin/orient"), owned)
            self.assertIn(Path("usr/bin/nacre"), owned)
            self.assertIn(Path("usr/share/nacre/brain/templates/AGENTS.md"), owned)
