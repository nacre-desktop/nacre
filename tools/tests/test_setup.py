import importlib.util
import json
import contextlib
import io
import os
import subprocess
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
        (root / "runtime/palette").mkdir(parents=True)
        for name in ("profile-policy.py", "nacre_native_config.py"):
            (root / "nacre/shell-tools" / name).write_bytes(
                (TOOLS.parent / "nacre/shell-tools" / name).read_bytes()
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

    def test_actual_english_and_pseudo_prompts_preserve_setup_values(self):
        import keyboard

        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            private = home / ".config/nacre/profile.json"
            private.parent.mkdir(parents=True)
            private.write_text(json.dumps(profile))
            for name in ("ai", "brain"):
                (root / "deployment" / name).mkdir(parents=True)
            args = setup.argparse.Namespace(
                layout=None,
                variant=None,
                model=None,
                options=None,
                compositor=None,
                profile=None,
                optional=[],
            )
            keyboard_values = {
                "layout": "no",
                "variant": "",
                "model": "pc105",
                "options": "caps:escape",
            }
            results = []
            prompts = []
            for language in ("en", "pseudo"):
                labels = []

                def answer(label, default, captured=labels):
                    captured.append(label)
                    return default

                output = io.StringIO()
                with (
                    patch.dict(os.environ, {"NACRE_LANGUAGE": language}),
                    patch.object(
                        keyboard, "system_keyboard", return_value=keyboard_values.copy()
                    ),
                    patch.object(setup, "ask", side_effect=answer),
                    contextlib.redirect_stdout(output),
                ):
                    results.append(setup.selection(args, root, home, True))
                prompts.append(labels)
                self.assertIn(json.dumps(keyboard_values), output.getvalue())
            self.assertEqual(results[0], results[1])
            self.assertEqual(results[0][1], keyboard_values)
            self.assertEqual(results[0][2], "hyprland")
            self.assertEqual(
                prompts[0],
                [
                    "Keyboard layout",
                    "Keyboard variant",
                    "Keyboard model",
                    "Keyboard options",
                    "Compositor (hyprland)",
                    "Profile (default or JSON path; installed keeps current)",
                    "Enable optional ai",
                    "Enable optional brain",
                ],
            )
            for caption in prompts[1]:
                self.assertTrue(caption.startswith("["), caption)
            for token, caption in [
                ("hyprland", prompts[1][4]),
                ("default", prompts[1][5]),
                ("installed", prompts[1][5]),
                ("ai", prompts[1][6]),
                ("brain", prompts[1][7]),
            ]:
                self.assertIn(token, caption)
            with patch.dict(os.environ, {"NACRE_LANGUAGE": "en"}):
                self.assertEqual(
                    setup.tr(
                        "setup.prompt.apply",
                        "wrong fallback",
                        {"yes": "yes", "no": "no"},
                    ),
                    "Apply this plan? (yes/no)",
                )
                self.assertEqual(
                    setup.tr(
                        "setup.applied",
                        "wrong fallback",
                        {"release": "/tmp/ROLLBACK_snapshot"},
                    ),
                    "Setup applied. Rollback snapshot: /tmp/ROLLBACK_snapshot",
                )

    def test_plan_keeps_original_english_output_and_never_deploys(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            keyboard_values = {
                "layout": "no",
                "variant": "",
                "model": "",
                "options": "",
            }
            private_path = home / ".config/nacre/FILE_untranslated.json"
            for language in ("en", "pseudo"):
                output = io.StringIO()
                with (
                    patch.dict(os.environ, {"NACRE_LANGUAGE": language}),
                    patch.object(setup, "ROOT", root),
                    patch.object(setup.Path, "home", return_value=home),
                    patch.object(setup.os, "geteuid", return_value=1000),
                    patch.object(
                        setup,
                        "selection",
                        return_value=(profile, keyboard_values, "hyprland"),
                    ),
                    patch.object(
                        setup, "missing_packages", return_value=["PACKAGE_untranslated"]
                    ),
                    patch.object(releases, "package_plan", return_value=[private_path]),
                    patch.object(releases, "deploy_package") as deploy,
                    contextlib.redirect_stdout(output),
                ):
                    self.assertEqual(setup.main(["--plan"]), 0)
                deploy.assert_not_called()
                text = output.getvalue()
                if language == "en":
                    self.assertEqual(
                        text,
                        f"Nacre setup: profile={profile['id']}; compositor=hyprland; components={', '.join(profile['components'])}\nKeyboard: {json.dumps(keyboard_values)}\nPrivate file plan:\n  {private_path}\nMissing packages: PACKAGE_untranslated\n",
                    )
                else:
                    for value in (
                        "hyprland",
                        json.dumps(keyboard_values),
                        str(private_path),
                        "PACKAGE_untranslated",
                    ):
                        self.assertIn(value, text)
                self.assertFalse(private_path.exists())

    def test_staged_setup_help_imports_runtime_without_source_pythonpath(self):
        with tempfile.TemporaryDirectory() as folder:
            staged = Path(folder)
            (staged / "tools").mkdir()
            (staged / "nacre/shell-tools").mkdir(parents=True)
            (staged / "nacre/shell/i18n").mkdir(parents=True)
            for source, target in [
                (TOOLS / "setup.py", staged / "tools/setup.py"),
                (
                    TOOLS.parent / "nacre/shell-tools/nacre_i18n.py",
                    staged / "nacre/shell-tools/nacre_i18n.py",
                ),
                (
                    TOOLS.parent / "nacre/shell/i18n/en.json",
                    staged / "nacre/shell/i18n/en.json",
                ),
            ]:
                target.write_bytes(source.read_bytes())
            environment = {**os.environ, "NACRE_LANGUAGE": "en", "HOME": str(staged)}
            environment.pop("PYTHONPATH", None)
            result = subprocess.run(
                [sys.executable, str(staged / "tools/setup.py"), "--help"],
                env=environment,
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("confirm applying the printed plan", result.stdout)
            self.assertIn("--compositor", result.stdout)

    def test_apply_is_offline_repeatable_preserves_private_state_and_rolls_back(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            account = home / ".codex/auth.json"
            account.parent.mkdir()
            account.write_text("private fixture")
            with (
                patch.object(releases, "HOME", home),
                patch.object(releases, "STATE", home / "releases"),
                patch.object(releases, "subprocess") as release_processes,
            ):
                record = releases.deploy_package(
                    root,
                    profile,
                    {"layout": "no", "variant": "", "model": "", "options": ""},
                    "hyprland",
                )
                release_processes.run.assert_not_called()
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
                release_processes.run.assert_not_called()
                self.assertFalse((home / ".config/nacre/keyboard.lua").exists())
                self.assertFalse((home / ".config/quickshell/nacre").is_symlink())
                self.assertEqual(account.read_text(), "private fixture")

    def test_package_rollback_retains_system_runtime_reference_and_can_reapply(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            with (
                patch.object(releases, "HOME", home),
                patch.object(releases, "STATE", home / "releases"),
            ):
                releases.deploy_package(root, profile, {"layout": "us"}, "hyprland")
                record = releases.deploy_package(
                    root, profile, {"layout": "us"}, "hyprland"
                )
                releases.restore(Path(record["release"]), record)
                self.assertEqual(
                    (home / ".local/share/nacre/palette-runtime").resolve(),
                    root / "runtime/palette",
                )
                releases.deploy_package(root, profile, {"layout": "us"}, "hyprland")

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

    def test_terminal_optional_choice_can_override_with_flag(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, _ = self.fixture(folder)
            args = setup.argparse.Namespace(
                layout="us",
                variant="",
                model="",
                options="",
                compositor="hyprland",
                profile="installed",
                optional=["ai"],
            )
            with patch.object(setup, "ask", side_effect=["no", "no"]) as ask:
                selected, _, _ = setup.selection(args, root, home, True)
            self.assertEqual(ask.call_args_list[0].args[1], "yes")
            self.assertNotIn("ai", selected["components"])

    def test_selection_retains_existing_assistant_access_preference(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            profile["aiFullAccess"] = True
            target = home / ".config/nacre/profile.json"
            target.parent.mkdir(parents=True)
            target.write_text(json.dumps(profile))
            args = setup.argparse.Namespace(
                layout="us",
                variant=None,
                model=None,
                options=None,
                compositor="hyprland",
                profile=None,
                optional=[],
            )
            selected, _, _ = setup.selection(args, root, home, False)
            self.assertTrue(selected["aiFullAccess"])

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
                "nacre-helper",
            ):
                path = build / "native" / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("fixture compiled bytes")
                if name == "nacre-helper":
                    path.chmod(0o755)
            owned = set()
            deployment_owned = set()
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
                deployment = destination / "usr/share/nacre/deployment" / component
                deployment_owned |= {
                    p.relative_to(deployment)
                    for p in deployment.rglob("*")
                    if p.is_file()
                }
                owned |= files
            self.assertFalse(
                deployment_owned
                & set(
                    package_runtime.links(TOOLS.parent, {"components": ["ai", "brain"]})
                )
            )
            self.assertIn(
                Path("usr/lib/nacre/dolphin-style/styles/libnacre-dolphin.so"), owned
            )
            self.assertIn(
                Path("usr/lib/nacre/thunar-style/nacre-thunar-theme.so"), owned
            )
            self.assertNotIn(Path("usr/bin/orient"), owned)
            self.assertIn(Path("usr/share/nacre/nacre/shell-tools/nacre-helper"), owned)
            self.assertEqual(
                (base / "core/usr/share/nacre/nacre/shell-tools/nacre-helper")
                .stat()
                .st_mode
                & 0o777,
                0o755,
            )
            self.assertIn(Path("usr/bin/nacre"), owned)
            self.assertIn(Path("usr/share/nacre/brain/templates/AGENTS.md"), owned)
