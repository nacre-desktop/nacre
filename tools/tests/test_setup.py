import importlib.util
import json
import hashlib
import contextlib
import io
import os
import shutil
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
import package_niri
import package_managers
import package_hyprland


def load(path, name):
    spec = importlib.util.spec_from_file_location(name, path)
    value = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(value)
    return value


class PackageSetupTests(unittest.TestCase):
    def test_initial_wallpaper_record_keeps_private_palette_and_image(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            home = base / "home"
            state = home / ".local/state/nacre"
            state.mkdir(parents=True)
            image = home / "custom.jpg"
            image.write_bytes(b"fixture image")
            (state / "wallpaper").mkdir()
            (state / "wallpaper/last.txt").write_text(str(image))
            (state / "scheme.json").write_text(
                json.dumps({"mode": "light", "colours": {"primary": "#123456"}})
            )
            template = base / "template.json"
            template.write_text(
                json.dumps(
                    {"mode": "dark", "colours": {}, "poster": "/bundled/forest.jpg"}
                )
            )
            relative = Path(".local/state/nacre/presentation.json")
            values = package_runtime.presentation_seed(home, {relative: template}, base)
            seeded = json.loads(values[relative].read_text())
            self.assertEqual(seeded["mode"], "light")
            self.assertEqual(seeded["poster"], str(image))
            self.assertEqual(seeded["colours"], {"primary": "#123456"})
            self.assertFalse((state / "presentation.json").exists())

    def test_empty_preview_launcher_defaults_upgrade_but_cleared_user_favorites_stay(
        self,
    ):
        import configure

        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            relative = Path(".config/nacre/launcher.json")
            source = root / "deployment/core" / relative
            source.write_text('{"favorites":["nacre-terminal.desktop"],"hidden":[]}')
            (root / "seed-paths.json").write_text(json.dumps([str(relative)]))
            target = home / relative
            target.parent.mkdir(parents=True)
            target.write_text('{"favorites":[],"hidden":[]}')
            with patch.object(configure, "selected_profile", return_value=profile):
                configure.apply(home, root, only=[str(relative)])
                self.assertEqual(
                    json.loads(target.read_text())["favorites"],
                    ["nacre-terminal.desktop"],
                )
                marker = home / ".local/state/nacre/defaults.json"
                marker.parent.mkdir(parents=True, exist_ok=True)
                marker.write_text('{"launcher":1}')
                target.write_text('{"favorites":[],"hidden":[],"recent":[],"usage":{}}')
                configure.apply(home, root, only=[str(relative)])
                self.assertEqual(json.loads(target.read_text())["favorites"], [])

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
                self.assertIn(keyboard_values["layout"], output.getvalue())
            self.assertEqual(results[0], results[1])
            self.assertEqual(results[0][1], keyboard_values)
            self.assertEqual(results[0][2], "hyprland")
            self.assertEqual(
                prompts[0],
                [
                    "Compositor (hyprland)",
                    "Enable AI tools (Nacre AI and Brain)?",
                ],
            )
            for caption in prompts[1]:
                self.assertTrue(caption.startswith("["), caption)
            for token, caption in [
                ("hyprland", prompts[1][0]),
                ("Nacre AI", prompts[1][1]),
                ("Brain", prompts[1][1]),
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
                (
                    TOOLS.parent / "nacre/shell-tools/nacre_state.py",
                    staged / "nacre/shell-tools/nacre_state.py",
                ),
                (
                    TOOLS.parent / "nacre/shell-tools/nacre-state-files.json",
                    staged / "nacre/shell-tools/nacre-state-files.json",
                ),
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
                    (home / ".local/share/nacre/shell/tools").resolve(),
                    root / "nacre/shell-tools",
                )
                self.assertFalse(
                    (home / ".local/share/nacre/palette-runtime").is_symlink()
                )
                releases.deploy_package(root, profile, {"layout": "us"}, "hyprland")

    def test_legacy_palette_link_retirement_rolls_back_and_preserves_foreign_data(self):
        for foreign in (None, "link", "directory"):
            with self.subTest(foreign=foreign), tempfile.TemporaryDirectory() as folder:
                root, home, profile = self.fixture(folder)
                relative = Path(".local/share/nacre/palette-runtime")
                path = home / relative
                path.parent.mkdir(parents=True, exist_ok=True)
                target = root / "runtime/palette"
                if foreign == "directory":
                    path.mkdir()
                    (path / "private.txt").write_text("keep")
                else:
                    path.symlink_to(home / "foreign" if foreign else target)
                manifest = home / ".local/state/nacre/package-links.json"
                manifest.parent.mkdir(parents=True)
                manifest.write_text(json.dumps({str(relative): str(target)}))
                before = manifest.read_bytes()
                with (
                    patch.object(releases, "HOME", home),
                    patch.object(releases, "STATE", home / "releases"),
                ):
                    if foreign:
                        with self.assertRaisesRegex(
                            RuntimeError, "Local runtime preserved"
                        ):
                            releases.deploy_package(
                                root, profile, {"layout": "us"}, "hyprland"
                            )
                        self.assertEqual(manifest.read_bytes(), before)
                        if foreign == "directory":
                            self.assertEqual((path / "private.txt").read_text(), "keep")
                        else:
                            self.assertEqual(os.readlink(path), str(home / "foreign"))
                        continue
                    record = releases.deploy_package(
                        root, profile, {"layout": "us"}, "hyprland"
                    )
                    self.assertFalse(path.is_symlink())
                    self.assertNotIn(
                        str(relative), package_runtime._state.read_json_file(manifest)
                    )
                    releases.restore(Path(record["release"]), record)
                    self.assertEqual(os.readlink(path), str(target))
                    self.assertEqual(manifest.read_bytes(), before)
                    releases.deploy_package(root, profile, {"layout": "us"}, "hyprland")
                    self.assertFalse(path.is_symlink())

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
            seed.write_text(json.dumps({"fixture": "initial"}))
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

    def niri_fixture(self, folder):
        root, home, profile = self.fixture(folder)
        (root / "package.json.installed").write_text(
            json.dumps({"revision": "fixture", "compositors": ["hyprland", "niri"]})
        )
        shutil.copy2(TOOLS / "package_niri.py", root / "tools/package_niri.py")
        for name in ("desktop-defaults.json", "nacre_native_compositor.py"):
            shutil.copy2(
                TOOLS.parent / "nacre/shell-tools" / name,
                root / "nacre/shell-tools" / name,
            )
        compiler = load(
            TOOLS.parent / "nacre/shell-tools/nacre_native_config.py",
            "test_setup_native",
        )
        shutil.copy2(compiler._binary(), root / "nacre/shell-tools/nacre-helper")
        shutil.copy2(
            TOOLS.parent / "nacre/shell-tools/compositor-contract.json",
            root / "nacre/shell-tools/compositor-contract.json",
        )
        scheme = root / "deployment/core/.local/state/nacre/scheme.json"
        scheme.parent.mkdir(parents=True)
        shutil.copy2(TOOLS.parent / "nacre/shell-tools/reference-style.json", scheme)
        (root / "seed-paths.json").write_text(
            json.dumps([".local/state/nacre/scheme.json"])
        )
        palette = root / "nacre/shell-tools/compositor_backends/niri/palette.kdl.in"
        palette.parent.mkdir(parents=True)
        shutil.copy2(
            TOOLS.parent / "nacre/shell-tools/compositor_backends/niri/palette.kdl.in",
            palette,
        )
        plain = home / ".config/niri/config.kdl"
        plain.parent.mkdir(parents=True)
        plain.write_text("// Existing CachyOS/Noctalia session\n")
        # Existing source-linked shell preferences must not block a Niri setup.
        fish = home / ".config/fish"
        fish_source = home / "private-fish"
        fish_source.mkdir()
        fish.symlink_to(fish_source)
        (fish / "config.fish").write_text("# Private shell\n")
        return root, home, profile

    def test_niri_package_apply_preserves_plain_session_and_can_rollback(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.niri_fixture(folder)
            before = {
                path: path.read_bytes()
                for path in (
                    home / ".config/niri/config.kdl",
                    home / ".config/fish/config.fish",
                )
            }
            with (
                patch.object(releases, "HOME", home),
                patch.object(releases, "STATE", home / "releases"),
                patch.object(releases, "subprocess") as processes,
            ):
                self.assertEqual(
                    setup.supported_compositors(root), ["hyprland", "niri"]
                )
                record = releases.deploy_package(
                    root, profile, {"layout": "no"}, "niri"
                )
                processes.run.assert_not_called()
                self.assertEqual(record["status"], "prepared")
                config = home / ".config/nacre/niri/config.kdl"
                subprocess.run(
                    ["niri", "validate", "-c", str(config)],
                    check=True,
                    capture_output=True,
                )
                self.assertFalse(
                    (
                        home
                        / ".config/systemd/user/graphical-session.target.wants/nacre-shell.service"
                    ).exists()
                )
                self.assertIn(
                    "nacre-shell.service",
                    (home / ".config/systemd/user/nacre-niri.target").read_text(),
                )
                self.assertIn('include "desktop.kdl"', config.read_text())
                self.assertIn('include "palette.kdl"', config.read_text())
                appearance = (config.parent / "desktop.kdl").read_text()
                self.assertIn("focus-ring { width 3; }", appearance)
                self.assertIn("border { width 3; }", appearance)
                self.assertIn("geometry-corner-radius 10", appearance)
                self.assertIn("prefer-no-csd", appearance)
                self.assertIn(
                    "active-gradient", (config.parent / "palette.kdl").read_text()
                )
                self.assertIn(
                    'spawn-at-startup "systemctl" "--user" "start" "nacre-niri-startup.service"',
                    config.read_text(),
                )
                startup = home / ".config/systemd/user/nacre-niri-startup.service"
                self.assertIn("PartOf=graphical-session.target", startup.read_text())
                self.assertIn("After=graphical-session.target", startup.read_text())
                self.assertIn("RemainAfterExit=yes", startup.read_text())
                if shutil.which("systemd-analyze"):
                    runtime = home / "verify-runtime"
                    runtime.mkdir(mode=0o700)
                    verified = subprocess.run(
                        ["systemd-analyze", "--user", "verify", str(startup)],
                        env={
                            **os.environ,
                            "XDG_RUNTIME_DIR": str(runtime),
                            "SYSTEMD_UNIT_PATH": str(startup.parent) + ":",
                        },
                        text=True,
                        capture_output=True,
                    )
                    self.assertEqual(verified.returncode, 0, verified.stderr)
                for path, content in before.items():
                    self.assertEqual(path.read_bytes(), content)
                releases.restore(Path(record["release"]), record)
                self.assertFalse(config.exists())
                for path, content in before.items():
                    self.assertEqual(path.read_bytes(), content)

    def test_both_sessions_prepare_and_repeat_without_taking_over_plain_niri(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.niri_fixture(folder)
            (home / ".config/fish").unlink()
            (root / "packaging").mkdir()
            shutil.copy2(
                TOOLS.parent / "packaging/sessions.json",
                root / "packaging/sessions.json",
            )
            shutil.copy2(
                TOOLS / "package_hyprland.py", root / "tools/package_hyprland.py"
            )
            for name, text in {
                "hypr/hyprland.lua": "-- fixture\n",
                "hypr/conf/autostart.lua": "-- existing startup\n",
                "tools/defaults/hypridle.conf": "general {\n}\n",
            }.items():
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(text)
            with (
                patch.object(releases, "HOME", home),
                patch.object(releases, "STATE", home / "releases"),
            ):
                record = releases.deploy_package(
                    root, profile, {"layout": "no"}, ["niri", "hyprland"]
                )
                self.assertEqual(record["compositors"], ["niri", "hyprland"])
                self.assertIn(
                    "nacre-hyprland.target",
                    (home / ".config/hypr/conf/autostart.lua").read_text(),
                )
                self.assertTrue(
                    (home / ".config/systemd/user/nacre-niri.target").is_file()
                )
                self.assertTrue(
                    (home / ".config/systemd/user/nacre-hyprland.target").is_file()
                )
                self.assertEqual(
                    (home / ".config/niri/config.kdl").read_text(),
                    "// Existing CachyOS/Noctalia session\n",
                )
                releases.deploy_package(
                    root, profile, {"layout": "us"}, ["niri", "hyprland"]
                )
                self.assertIn(
                    '"no"', (home / ".config/nacre/niri/keyboard.kdl").read_text()
                )
                # Preparing only one later must retain the other session's files.
                releases.deploy_package(root, profile, {"layout": "us"}, "niri")
                self.assertTrue((home / ".config/hypr/conf/autostart.lua").is_file())

    def test_niri_empty_preview_appearance_upgrades_and_keeps_saved_choices(self):
        for saved in (None, {"borderSize": 2, "rounding": 7, "shadow": False}):
            with self.subTest(saved=saved), tempfile.TemporaryDirectory() as folder:
                root, home, profile = self.niri_fixture(folder)
                relative = ".config/nacre/niri/desktop.kdl"
                desktop = home / relative
                desktop.parent.mkdir(parents=True)
                desktop.write_text("")
                known = home / ".local/state/nacre/configuration.json"
                known.parent.mkdir(parents=True)
                known.write_text(
                    json.dumps({relative: hashlib.sha256(b"").hexdigest()})
                )
                preferences = home / ".config/nacre/desktop.json"
                if saved is not None:
                    preferences.write_text(json.dumps(saved))
                with (
                    patch.object(releases, "HOME", home),
                    patch.object(releases, "STATE", home / "releases"),
                ):
                    record = releases.deploy_package(
                        root, profile, {"layout": "no"}, "niri"
                    )
                    width, rounding = (3, 10) if saved is None else (2, 7)
                    self.assertIn(
                        f"focus-ring {{ width {width}; }}", desktop.read_text()
                    )
                    self.assertIn(
                        f"geometry-corner-radius {rounding}", desktop.read_text()
                    )
                    chosen = desktop.read_bytes()
                    preferences.write_text('{"borderSize": 4, "rounding": 3}')
                    releases.deploy_package(root, profile, {"layout": "no"}, "niri")
                    self.assertEqual(desktop.read_bytes(), chosen)
                    releases.restore(Path(record["release"]), record)
                    self.assertEqual(desktop.read_text(), "")

    def test_selected_session_dependencies_do_not_force_other_compositors(self):
        with tempfile.TemporaryDirectory() as folder:
            root, _, profile = self.fixture(folder)
            (root / "packaging").mkdir()
            for name in ("packages.json", "sessions.json"):
                shutil.copy2(
                    TOOLS.parent / "packaging" / name, root / "packaging" / name
                )
            with (
                patch.object(setup, "distro", return_value="arch"),
                patch.object(
                    package_managers.Arch, "missing", return_value=[]
                ) as query,
            ):
                setup.missing_packages(root, profile, "niri")
                requested = query.call_args.args[0]
                self.assertIn("nacre-session-niri", requested)
                self.assertIn("niri>=26.04", requested)
                self.assertIn("orient", requested)
                self.assertFalse(any("hyprland" in value for value in requested))
                setup.missing_packages(root, profile, ["hyprland", "niri"])
                self.assertIn("nacre-session-hyprland", query.call_args.args[0])

    def test_dependency_transaction_restarts_setup_with_same_choices(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.fixture(folder)
            (root / "packaging").mkdir()
            (root / "packaging/packages.json").write_text('{"arch":{}}')
            keyboard = {"layout": "no", "variant": "", "model": "", "options": ""}

            def restart(command):
                self.assertIn("--review-selected", command)
                self.assertIn("--yes", command)
                self.assertEqual(
                    command[command.index("--also-compositor") + 1], "hyprland"
                )
                saved = Path(command[command.index("--profile") + 1])
                self.assertEqual(json.loads(saved.read_text()), profile)
                return 0

            with (
                patch.object(setup, "ROOT", root),
                patch.object(setup.Path, "home", return_value=home),
                patch.object(setup.os, "geteuid", return_value=1000),
                patch.object(
                    setup, "selection", return_value=(profile, keyboard, "niri")
                ),
                patch.object(setup, "distro", return_value="arch"),
                patch.object(
                    setup, "missing_packages", return_value=["nacre-session-niri"]
                ),
                patch.object(releases, "package_plan", return_value=[]),
                patch.object(releases, "deploy_package") as deploy,
                patch.object(package_managers.Arch, "install") as install,
                patch.object(setup.subprocess, "call", side_effect=restart),
                contextlib.redirect_stdout(io.StringIO()),
            ):
                self.assertEqual(
                    setup.main(
                        ["--yes", "--install-deps", "--also-compositor", "hyprland"]
                    ),
                    0,
                )
                install.assert_called_once_with(["nacre-session-niri"])
                deploy.assert_not_called()
                install.reset_mock()
                self.assertEqual(setup.main(["--plan", "--install-deps"]), 0)
                install.assert_not_called()

    def test_niri_configuration_symlink_is_preserved(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.niri_fixture(folder)
            other = home / "other-settings"
            other.mkdir()
            target = home / ".config/nacre/niri"
            target.parent.mkdir(parents=True)
            target.symlink_to(other)
            with patch.object(releases, "HOME", home):
                with self.assertRaisesRegex(RuntimeError, "symlink preserved"):
                    releases.package_plan(root, profile, "niri", {"layout": "no"})
            self.assertTrue(target.is_symlink())
            self.assertEqual(list(other.iterdir()), [])

    def test_setup_detects_current_niri_when_packaged_backend_is_ready(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, _ = self.niri_fixture(folder)
            args = setup.argparse.Namespace(
                layout=None,
                variant=None,
                model=None,
                options=None,
                compositor=None,
                profile=None,
                optional=[],
            )
            with patch.dict(os.environ, {"XDG_CURRENT_DESKTOP": "niri"}):
                self.assertEqual(setup.selection(args, root, home, False)[2], "niri")

    def test_normal_wizard_only_asks_compositors_and_ai_tools(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, _ = self.niri_fixture(folder)
            args = setup.argparse.Namespace(
                layout=None,
                variant=None,
                model=None,
                options=None,
                compositor=None,
                profile=None,
                optional=[],
            )
            with patch.object(
                setup, "ask", side_effect=["niri,hyprland", "yes"]
            ) as prompts:
                profile, _, primary = setup.selection(args, root, home, True)
            self.assertEqual(prompts.call_count, 2)
            self.assertEqual(primary, "niri")
            self.assertEqual(args.additional_compositors, ["hyprland"])
            self.assertTrue({"ai", "brain"} <= set(profile["components"]))

    def test_dependencies_check_providers_and_selected_niri_requirements(self):
        with tempfile.TemporaryDirectory() as folder:
            root, _, profile = self.fixture(folder)
            mapping = root / "packaging/packages.json"
            mapping.parent.mkdir()
            mapping.write_text(
                json.dumps(
                    {"arch": {"core": ["hyprland", "uwsm"], "ai": [], "brain": []}}
                )
            )
            with (
                patch.object(setup, "distro", return_value="arch"),
                patch.object(
                    setup.subprocess,
                    "run",
                    return_value=subprocess.CompletedProcess(
                        [], 127, "xdg-desktop-portal-gnome\n", ""
                    ),
                ) as query,
            ):
                self.assertEqual(
                    setup.missing_packages(root, profile, "niri"),
                    ["xdg-desktop-portal-gnome"],
                )
            self.assertEqual(query.call_args.args[0][:2], ["pacman", "-T"])
            self.assertIn("niri", query.call_args.args[0])
            self.assertIn("xwayland-satellite", query.call_args.args[0])

    def test_niri_repeated_setup_preserves_custom_config_and_rejects_managed_drift(
        self,
    ):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.niri_fixture(folder)
            with (
                patch.object(releases, "HOME", home),
                patch.object(releases, "STATE", home / "releases"),
            ):
                releases.deploy_package(root, profile, {"layout": "no"}, "niri")
                custom = home / ".config/nacre/niri/custom.kdl"
                custom.write_text("// My preferences\nlayout { gaps 12; }\n")
                releases.deploy_package(root, profile, {"layout": "us"}, "niri")
                self.assertIn("gaps 12", custom.read_text())
                self.assertIn(
                    'layout "no"',
                    (home / ".config/nacre/niri/keyboard.kdl").read_text(),
                )
                managed = home / ".config/nacre/niri/config.kdl"
                managed.write_text(managed.read_text() + "// Local change\n")
                count = len(list((home / "releases").glob("*/release.json")))
                with self.assertRaisesRegex(RuntimeError, "Local edit preserved"):
                    releases.deploy_package(root, profile, {"layout": "no"}, "niri")
                self.assertEqual(
                    count, len(list((home / "releases").glob("*/release.json")))
                )
                self.assertTrue(managed.read_text().endswith("// Local change\n"))

    def test_invalid_niri_custom_config_is_rejected_before_deployment(self):
        with tempfile.TemporaryDirectory() as folder:
            root, home, profile = self.niri_fixture(folder)
            custom = home / ".config/nacre/niri/custom.kdl"
            custom.parent.mkdir(parents=True)
            custom.write_text("this-is-not-a-niri-setting\n")
            with patch.object(releases, "HOME", home):
                with self.assertRaisesRegex(
                    RuntimeError, "Niri configuration validation failed"
                ):
                    releases.package_plan(root, profile, "niri", {"layout": "no"})
            self.assertFalse((home / ".config/nacre/niri/config.kdl").exists())
            self.assertFalse((home / ".local/state/nacre/package-links.json").exists())

    def test_niri_session_uses_uwsm_with_explicit_config_and_clears_hyprland(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            config_path = home / ".config/nacre/niri/config.kdl"
            config_path.parent.mkdir(parents=True)
            config_path.write_text("// fixture\n")
            with (
                patch.dict(
                    os.environ,
                    {"HYPRLAND_INSTANCE_SIGNATURE": "foreign", "GDK_BACKEND": "x11"},
                ),
                patch.object(package_niri.subprocess, "run") as commands,
                patch.object(package_niri.os, "execvpe") as execute,
            ):
                package_niri.session(home)
            config = str(home / ".config/nacre/niri/config.kdl")
            self.assertEqual(
                commands.call_args_list[0].args[0], ["niri", "validate", "-c", config]
            )
            self.assertEqual(
                commands.call_args_list[1].args[0],
                ["systemctl", "--user", "daemon-reload"],
            )
            command, arguments, environment = execute.call_args.args
            self.assertEqual(command, "uwsm")
            self.assertEqual(
                arguments[5:], ["--", "niri", "--session", "--config", config]
            )
            self.assertEqual(environment["NACRE_COMPOSITOR"], "niri")
            for name, value in package_niri.TOOLKIT_ENVIRONMENT.items():
                self.assertEqual(environment[name], value)
            self.assertNotIn("HYPRLAND_INSTANCE_SIGNATURE", environment)
            self.assertNotIn("GDK_BACKEND", environment)

    def test_niri_services_start_only_after_environment_is_published(self):
        with patch.object(package_niri.subprocess, "run") as commands:
            package_niri.finalize()
        self.assertEqual(
            commands.call_args_list[0].args[0][:3],
            ["systemctl", "--user", "import-environment"],
        )
        self.assertEqual(
            commands.call_args_list[1].args[0][0], "dbus-update-activation-environment"
        )
        self.assertEqual(
            commands.call_args_list[2].args[0],
            ["systemctl", "--user", "start", "nacre-niri.target"],
        )
        with patch.object(
            package_niri.subprocess,
            "run",
            side_effect=subprocess.CalledProcessError(1, "systemctl"),
        ) as commands:
            with self.assertRaises(subprocess.CalledProcessError):
                package_niri.finalize()
        self.assertEqual(commands.call_count, 1)

    @unittest.skipUnless(
        shutil.which("systemd-notify"), "Systemd notification utility unavailable"
    )
    def test_real_niri_startup_child_without_notify_socket_starts_services(self):
        # Niri deliberately removes NOTIFY_SOCKET from startup children. Model
        # an already-ready compositor, not a mock successful uwsm.finalize call.
        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            commands = home / "bin"
            commands.mkdir()
            receipt = home / "startup.json"
            shim = f"""#!{sys.executable}
import json, os, pathlib, sys
path=pathlib.Path(os.environ["STARTUP_RECEIPT"])
events=json.loads(path.read_text()) if path.exists() else []
assert "NOTIFY_SOCKET" not in os.environ
command=pathlib.Path(sys.argv[0]).name
if command == "systemctl" and sys.argv[1:3] == ["--user", "import-environment"]:
    assert "NIRI_SOCKET" in sys.argv and "WAYLAND_DISPLAY" in sys.argv
    assert os.environ["NIRI_SOCKET"] == "/fixture/niri.sock"
    events.append("manager-environment")
elif command == "dbus-update-activation-environment":
    if os.environ.get("FAIL_ACTIVATION"): sys.exit(17)
    assert events == ["manager-environment"]
    events.append("activation-environment")
elif command == "systemctl" and sys.argv[1:] == ["--user", "start", "nacre-niri.target"]:
    assert events == ["manager-environment", "activation-environment"]
    events.append("services-active")
else: sys.exit(23)
path.write_text(json.dumps(events))
"""
            for name in ("systemctl", "dbus-update-activation-environment"):
                path = commands / name
                path.write_text(shim)
                path.chmod(0o755)
            legacy = commands / "uwsm"
            legacy.write_text(
                "#!/bin/sh\nexec "
                + shutil.which("systemd-notify")
                + " READY=1 NOTIFYACCESS=exec\n"
            )
            legacy.chmod(0o755)
            environment = {
                **os.environ,
                "HOME": str(home),
                "PATH": str(commands) + os.pathsep + os.environ["PATH"],
                "STARTUP_RECEIPT": str(receipt),
                "NIRI_SOCKET": "/fixture/niri.sock",
                "WAYLAND_DISPLAY": "wayland-fixture",
            }
            environment.pop("NOTIFY_SOCKET", None)
            old = subprocess.run(
                [str(legacy), "finalize"],
                env=environment,
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertNotEqual(old.returncode, 0)
            self.assertIn("NOTIFY_SOCKET", old.stderr)
            result = subprocess.run(
                [sys.executable, str(TOOLS / "package_niri.py"), "finalize"],
                env=environment,
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertEqual(
                json.loads(receipt.read_text()),
                ["manager-environment", "activation-environment", "services-active"],
            )
            receipt.unlink()
            result = subprocess.run(
                [sys.executable, str(TOOLS / "package_niri.py"), "finalize"],
                env={**environment, "FAIL_ACTIVATION": "1"},
                capture_output=True,
                text=True,
                timeout=10,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(json.loads(receipt.read_text()), ["manager-environment"])

    def test_hyprland_login_uses_uwsm_and_does_not_inherit_niri_environment(self):
        with tempfile.TemporaryDirectory() as folder:
            home = Path(folder)
            with self.assertRaisesRegex(RuntimeError, "Run nacre setup"):
                package_hyprland.session(home)
            config = home / ".config/hypr/hyprland.lua"
            config.parent.mkdir(parents=True)
            config.write_text("-- prepared fixture\n")
            with (
                patch.dict(
                    os.environ, {"NIRI_CONFIG": "foreign", "NIRI_SOCKET": "foreign"}
                ),
                patch.object(package_hyprland.subprocess, "run"),
                patch.object(package_hyprland.os, "execvpe") as execute,
            ):
                package_hyprland.session(home)
            command, arguments, environment = execute.call_args.args
            self.assertEqual(command, "uwsm")
            self.assertEqual(arguments[-1], "hyprland.desktop")
            self.assertEqual(environment["NACRE_COMPOSITOR"], "hyprland")
            self.assertNotIn("NIRI_CONFIG", environment)
            self.assertNotIn("NIRI_SOCKET", environment)

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

    def test_optional_component_can_be_selected_before_dependency_install(self):
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
            profile, _, _ = setup.selection(args, root, home, False)
            self.assertIn("brain", profile["components"])


class ArchPackageLayoutTests(unittest.TestCase):
    def test_native_palette_recipe_declares_runtime_and_embedded_licenses(self):
        recipe = load(TOOLS.parent / "packaging/arch/recipe.py", "native_arch_recipe")
        rendered = recipe.render(TOOLS.parent, "a" * 40, 12)
        core = rendered.split("package_nacre() {", 1)[1].split("package_nacre-ai()", 1)[
            0
        ]
        for name in (
            "fontconfig",
            "freetype2",
            "libraqm",
            "libjpeg-turbo",
            "lcms2",
            "librsvg",
        ):
            self.assertIn(
                "'" + name + "'", core.split("depends=", 1)[1].splitlines()[0]
            )
        for license_id in ("MIT", "MIT-CMU", "BSD-3-Clause", "IJG", "Zlib"):
            self.assertIn(
                "'" + license_id + "'", core.split("license=", 1)[1].splitlines()[0]
            )
        self.assertNotIn("-m installer", rendered)
        self.assertNotIn("--wheel", rendered)

    def test_native_candidate_seeds_palette_and_lock_assets_offline(self):
        build = load(TOOLS.parent / "packaging/arch/build.py", "native_arch_build")
        helper = Path(
            os.environ.get(
                "NACRE_HELPER_BINARY", TOOLS.parent / "target/release/nacre-helper"
            )
        )
        self.assertTrue(
            helper.is_file(), "Build the candidate native helper before source checks"
        )
        with tempfile.TemporaryDirectory() as folder:
            base = Path(folder)
            home = base / "defaults"
            foreign = base / "foreign"
            foreign.mkdir()
            with patch.dict(
                os.environ,
                {
                    "HOME": str(foreign),
                    "XDG_CONFIG_HOME": str(foreign),
                    "XDG_STATE_HOME": str(foreign),
                    "NACRE_HELPER_BINARY": "/absent/foreign-helper",
                    "NACRE_HELPER_TOOLS": str(foreign),
                    "NIRI_SOCKET": "/absent/foreign-session",
                },
            ):
                build.seed_defaults(TOOLS.parent.resolve(), home, helper)
            self.assertEqual(list(foreign.iterdir()), [])
            state = package_runtime._state
            colors = state.read_json_file(home / ".local/state/nacre/scheme.json")[
                "colours"
            ]
            presentation = state.read_json_file(
                home / ".local/state/nacre/presentation.json"
            )
            self.assertEqual(presentation["colours"], colors)
            self.assertTrue((home / ".config/hypr/hyprlock.conf").is_file())
            ready = home / ".cache/nacre/lock-ready"
            self.assertTrue(list(ready.glob("dashboard-*-initial.png")))
            self.assertEqual((ready / "media.txt").stat().st_mode & 0o777, 0o600)
            self.assertFalse((home / ".runtime").exists())

    def test_development_archives_have_monotonic_snapshot_versions(self):
        recipe = load(TOOLS.parent / "packaging/arch/recipe.py", "setup_arch_recipe")
        older = recipe.render(TOOLS.parent, "a" * 40, 11)
        newer = recipe.render(TOOLS.parent, "b" * 40, 12)
        self.assertIn(".r11.gaaaaaaaaaaaa", older)
        self.assertIn(".r12.gbbbbbbbbbbbb", newer)
        self.assertIn("orient=0.1.0dev6", newer)

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
            for component in (
                "core",
                "ai",
                "brain",
                "session-niri",
                "session-hyprland",
            ):
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
            self.assertIn(Path("usr/bin/nacre_shell"), owned)
            self.assertFalse((base / "core/usr/share/nacre/runtime/palette").exists())
            for name in ("LICENSE-Orient", "LICENSE-Pillow"):
                notice = Path("usr/share/licenses/nacre") / name
                self.assertIn(notice, owned)
                self.assertEqual(
                    (base / "core" / notice).read_bytes(),
                    (TOOLS.parent / "crates/nacre-helper" / name).read_bytes(),
                )
            if (TOOLS.parent / "nacre/shell-tools/emoji-LICENSE.txt").is_file():
                self.assertIn(Path("usr/share/licenses/nacre/emoji-LICENSE.txt"), owned)
            self.assertIn(Path("usr/share/nacre/nacre/shell-tools/nacre-helper"), owned)
            self.assertIn(Path("usr/lib/nacre/nacre-helper"), owned)
            reference = base / "core/usr/share/nacre/nacre/shell-tools/nacre-helper"
            self.assertTrue(reference.is_symlink())
            self.assertEqual(
                reference.resolve(), base / "core/usr/lib/nacre/nacre-helper"
            )
            self.assertIn(Path("usr/share/wayland-sessions/nacre-niri.desktop"), owned)
            self.assertIn(
                "nacre session niri",
                (
                    base / "session-niri/usr/share/wayland-sessions/nacre-niri.desktop"
                ).read_text(),
            )
            self.assertEqual(
                (base / "core/usr/share/nacre/nacre/shell-tools/nacre-helper")
                .stat()
                .st_mode
                & 0o777,
                0o755,
            )
            self.assertIn(Path("usr/bin/nacre"), owned)
            core = base / "core/usr/share/nacre"
            wallpaper = (
                (core / "deployment/core/.local/state/nacre/wallpaper/last.txt")
                .read_text()
                .strip()
            )
            self.assertTrue((base / "core" / wallpaper.lstrip("/")).is_file())
            self.assertIn(
                "nacre-terminal",
                json.loads(
                    (core / "deployment/core/.config/nacre/launcher.json").read_text()
                )["favorites"],
            )
            self.assertIn(
                "shell fish --init-command",
                (
                    core / "deployment/core/.config/nacre/terminal/kitty.conf"
                ).read_text(),
            )
            self.assertIn(Path("usr/share/nacre/brain/templates/AGENTS.md"), owned)
