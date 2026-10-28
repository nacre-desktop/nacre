import importlib.util
from pathlib import Path
import json
import configparser
import shlex
import os
import shutil
import subprocess
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "configure", Path(__file__).parents[1] / "configure.py"
)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)


class ConfigurationTests(unittest.TestCase):
    def setUp(self):
        from unittest.mock import patch

        selected = patch.object(
            module,
            "selected_profile",
            return_value={"components": ["configs", "shell", "ai", "brain"]},
        )
        selected.start()
        self.addCleanup(selected.stop)

    def test_default_wallpapers_are_real_flat_assets_and_personal_files_survive(self):
        from unittest.mock import patch
        import hashlib

        with tempfile.TemporaryDirectory() as directory:
            home = Path(directory)
            with patch.object(
                module,
                "selected_profile",
                return_value={"id": "default", "components": ["configs", "shell"]},
            ):
                files = module.files(module.ROOT)
                scenes = {
                    path: source
                    for path, source in files.items()
                    if path.parts[:2] == ("Pictures", "Wallpapers")
                }
                self.assertEqual(len(scenes), 4)
                manifest = json.loads(
                    (
                        module.ROOT / "nacre/shell-tools/demo-wallpapers/manifest.json"
                    ).read_text()
                )
                hashes = {row["file"]: row["sha256"] for row in manifest["wallpapers"]}
                for path, source in scenes.items():
                    self.assertEqual(len(path.parts), 3)
                    self.assertEqual(
                        hashlib.sha256(source.read_bytes()).hexdigest(),
                        hashes[source.name],
                    )
                    self.assertGreater(source.stat().st_size, 100000)
                folder = home / "Pictures/Wallpapers"
                folder.mkdir(parents=True)
                personal = folder / "personal.jpg"
                personal.write_bytes(b"personal wallpaper")
                module.apply(
                    home, module.ROOT, only=tuple(str(path) for path in scenes)
                )
                self.assertEqual(personal.read_bytes(), b"personal wallpaper")
                self.assertEqual(len(list(folder.glob("nacre-*.jpg"))), 4)
                self.assertFalse(any(path.is_dir() for path in folder.iterdir()))

    def test_nacre_ai_desktop_entry_installs_the_public_menu_route(self):
        with tempfile.TemporaryDirectory() as directory:
            root, home = Path(directory) / "repo", Path(directory) / "home"
            (root / "nacre/desktop").mkdir(parents=True)
            source = module.ROOT / "nacre/desktop/nacre-ai.desktop"
            shutil.copy2(source, root / "nacre/desktop/nacre-ai.desktop")
            module.apply(home, root)
            target = home / ".local/share/applications/nacre-ai.desktop"
            self.assertEqual(target.read_bytes(), source.read_bytes())
            entry = configparser.ConfigParser(interpolation=None)
            entry.read(target)
            app = entry["Desktop Entry"]
            self.assertEqual(app["Name"], "Nacre AI")
            self.assertEqual(app["TryExec"], "nacre-ai")
            self.assertEqual(app["Icon"], "nacre-ai")
            self.assertEqual(
                shlex.split(app["Exec"]),
                [
                    "kitty",
                    "--class",
                    "nacre-ai-dashboard",
                    "--title",
                    "Nacre AI",
                    "--",
                    "nacre-ai",
                ],
            )
            self.assertEqual(module.plan(home, root)[0], [])

    def test_nacre_widgets_desktop_entry_installs_and_names_the_widgets_app(self):
        with tempfile.TemporaryDirectory() as directory:
            root, home = Path(directory) / "repo", Path(directory) / "home"
            (root / "nacre/desktop").mkdir(parents=True)
            (root / "nacre/shell/branding").mkdir(parents=True)
            source = module.ROOT / "nacre/desktop/nacre-widgets.desktop"
            shutil.copy2(source, root / "nacre/desktop/nacre-widgets.desktop")
            icon = module.ROOT / "nacre/shell/branding/nacre-widgets.svg"
            shutil.copy2(icon, root / "nacre/shell/branding/nacre-widgets.svg")
            module.apply(home, root)
            target = home / ".local/share/applications/nacre-widgets.desktop"
            self.assertEqual(target.read_bytes(), source.read_bytes())
            entry = configparser.ConfigParser(interpolation=None)
            entry.read(target)
            app = entry["Desktop Entry"]
            self.assertEqual(app["Name"], "Nacre Widgets")
            self.assertEqual(
                app["Comment"], "Place and style widgets on your bar and desktop"
            )
            self.assertEqual(
                shlex.split(app["Exec"]), ["nacre-shell", "--app", "widgets"]
            )
            self.assertEqual(app["Icon"], "nacre-widgets")
            self.assertIn("Utility", app["Categories"])
            installed_icon = (
                home / ".local/share/icons/hicolor/scalable/apps/nacre-widgets.svg"
            )
            self.assertEqual(installed_icon.read_bytes(), icon.read_bytes())
            self.assertEqual(icon.read_text().count("<rect "), 8)

    def test_nacre_ai_installs_the_actual_executable_controller(self):
        with tempfile.TemporaryDirectory() as directory:
            root, home = Path(directory) / "repo", Path(directory) / "home"
            (root / "bin").mkdir(parents=True)
            source = module.ROOT / "bin/nacre-ai"
            shutil.copy2(source, root / "bin/nacre-ai")
            module.apply(home, root)
            controller = home / ".local/bin/nacre-ai"
            self.assertEqual(controller.read_bytes(), source.read_bytes())
            self.assertTrue(os.access(controller, os.X_OK))
            result = subprocess.run(
                [str(controller), "--help"],
                env={**os.environ, "HOME": str(home)},
                capture_output=True,
                text=True,
            )
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("dashboard", result.stdout)

    def test_copy_install_is_repeatable_and_refuses_later_local_edits(self):
        with tempfile.TemporaryDirectory() as directory:
            root, home = Path(directory) / "repo", Path(directory) / "home"
            (root / "bin").mkdir(parents=True)
            (root / "hypr").mkdir()
            for name in ("nacre-app", "xdg-open"):
                (root / "bin" / name).write_text("helper")
            source = root / "hypr/hyprland.lua"
            source.write_text("current source")
            module.apply(home, root)
            self.assertEqual(module.plan(home, root)[0], [])
            target = home / ".config/hypr/hyprland.lua"
            target.write_text("personal edit")
            source.write_text("new source")
            with self.assertRaisesRegex(RuntimeError, "Local edit preserved"):
                module.apply(home, root)
            self.assertEqual(target.read_text(), "personal edit")

    def test_app_routes_scope_preserves_unrelated_edits_and_their_manifest(self):
        with tempfile.TemporaryDirectory() as directory:
            root, home = Path(directory) / "repo", Path(directory) / "home"
            (root / "bin").mkdir(parents=True)
            (root / "hypr").mkdir()
            for name in ("nacre-app", "xdg-open"):
                (root / "bin" / name).write_text("old route")
            (root / "hypr/hypridle.conf").write_text("original idle")
            module.apply(home, root)
            manifest = home / ".local/state/nacre/configuration.json"
            before = json.loads(manifest.read_text())[".config/hypr/hypridle.conf"]
            (home / ".config/hypr/hypridle.conf").write_text("personal idle choice")
            (root / "bin/nacre-app").write_text("new route")
            module.apply(home, root, app_routes_only=True)
            self.assertFalse((home / ".local/bin/nacre-app").exists())
            self.assertEqual(
                (home / ".config/hypr/hypridle.conf").read_text(),
                "personal idle choice",
            )
            self.assertEqual(
                json.loads(manifest.read_text())[".config/hypr/hypridle.conf"], before
            )
            module.apply(home, root, app_routes_only=True)
            self.assertEqual(
                json.loads(manifest.read_text())[".config/hypr/hypridle.conf"], before
            )
            with self.assertRaisesRegex(RuntimeError, "Local edit preserved"):
                module.plan(home, root)

    def test_explicit_scope_updates_rules_without_replacing_unrelated_edits(self):
        with tempfile.TemporaryDirectory() as directory:
            root, home = Path(directory) / "repo", Path(directory) / "home"
            (root / "hypr/conf").mkdir(parents=True)
            source = root / "hypr/conf/windowrule.lua"
            source.write_text("old managed rules")
            other = root / "hypr/conf/keyboard.lua"
            other.write_text("managed keyboard")
            module.apply(home, root)
            manifest = home / ".local/state/nacre/configuration.json"
            previous = json.loads(manifest.read_text())[
                ".config/hypr/conf/keyboard.lua"
            ]
            keyboard = home / ".config/hypr/conf/keyboard.lua"
            keyboard.write_text("private keyboard")
            source.write_text("general rules")
            scope = [".config/hypr/conf/windowrule.lua"]
            backup = module.apply(home, root, only=scope)
            self.assertEqual((backup / scope[0]).read_text(), "old managed rules")
            self.assertEqual((home / scope[0]).read_text(), "general rules")
            self.assertEqual(keyboard.read_text(), "private keyboard")
            self.assertEqual(
                json.loads(manifest.read_text())[".config/hypr/conf/keyboard.lua"],
                previous,
            )
            self.assertIsNone(module.apply(home, root, only=scope))
            self.assertEqual(
                json.loads(manifest.read_text())[".config/hypr/conf/keyboard.lua"],
                previous,
            )
            (home / scope[0]).write_text("private rules")
            with self.assertRaisesRegex(RuntimeError, "Local edit preserved"):
                module.plan(home, root, only=scope)
            with self.assertRaises(ValueError):
                module.plan(home, root, only=["../outside"])
            with self.assertRaises(ValueError):
                module.plan(home, root, app_routes_only=True, only=scope)

    def test_owned_symlink_migration_preserves_private_monitors_and_backup(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            root, old, home = base / "repo", base / "old", base / "home"
            for directory in (
                root / "hypr/conf",
                old / "hypr/conf",
                root / "bin",
                old / ".git",
                home / ".config",
            ):
                directory.mkdir(parents=True)
            (old / "hypr/conf/monitor.lua").write_text("host monitor override")
            (old / "hypr/old.conf").write_text("retired")
            (root / "hypr/conf/monitor.lua").write_text("generic fallback")
            for name in ("nacre-app", "xdg-open"):
                (root / "bin" / name).write_text("helper")
            (home / ".config/hypr").symlink_to(old / "hypr")
            with self.assertRaises(RuntimeError):
                module.apply(home, root)
            backup = module.apply(home, root, migrate=True)
            self.assertEqual(
                (home / ".config/nacre/monitor.lua").read_text(),
                "host monitor override",
            )
            self.assertEqual((backup / ".config/hypr/old.conf").read_text(), "retired")
            self.assertFalse((home / ".config/hypr/old.conf").exists())
            self.assertFalse((home / ".config/hypr").is_symlink())

    def test_uwsm_environment_is_deployed_and_drift_protected(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory) / "repo"
            home = Path(directory) / "home"
            (root / "uwsm").mkdir(parents=True)
            (root / "bin").mkdir()
            for name in ("nacre-app", "xdg-open"):
                (root / "bin" / name).write_text("helper")
            (root / "uwsm/env").write_text("export XCURSOR_SIZE=24\n")
            (root / "uwsm/env-hyprland").write_text("export HYPRCURSOR_SIZE=24\n")
            module.apply(home, root)
            self.assertEqual(module.plan(home, root)[0], [])
            (home / ".config/uwsm/env").write_text("personal environment")
            (root / "uwsm/env").write_text("export XCURSOR_SIZE=32\n")
            with self.assertRaisesRegex(RuntimeError, "Local edit preserved"):
                module.apply(home, root)

    def test_managed_fish_link_migration_preserves_personal_files(self):
        with tempfile.TemporaryDirectory() as directory:
            base = Path(directory)
            root = base / "repo"
            home = base / "home"
            old = base / "legacy/fish"
            for path in (
                root / "fish/functions",
                root / "bin",
                old / "functions",
                home / ".config",
                home / ".local/state/nacre",
            ):
                path.mkdir(parents=True)
            for name in ("nacre-app", "xdg-open"):
                (root / "bin" / name).write_text("helper")
            (root / "fish/functions/fish_title.fish").write_text("owned title")
            (old / "functions/fish_title.fish").write_text("owned title")
            (old / "config.fish").write_text("personal setup")
            (home / ".config/fish").symlink_to(old)
            manifest = {
                ".config/fish/functions/fish_title.fish": module.digest(
                    old / "functions/fish_title.fish"
                )
            }
            (home / ".local/state/nacre/configuration.json").write_text(
                json.dumps(manifest)
            )
            with self.assertRaises(RuntimeError):
                module.plan(home, root)
            backup = module.apply(home, root, migrate=True)
            self.assertFalse((home / ".config/fish").is_symlink())
            self.assertEqual(
                (home / ".config/fish/config.fish").read_text(), "personal setup"
            )
            self.assertEqual(
                (backup / ".config/fish/config.fish").read_text(), "personal setup"
            )
            self.assertEqual((old / "config.fish").read_text(), "personal setup")

    def test_removed_managed_file_is_backed_up_and_pruned_only_when_unmodified(self):
        with tempfile.TemporaryDirectory() as directory:
            root, home = Path(directory) / "repo", Path(directory) / "home"
            (root / "hypr").mkdir(parents=True)
            (root / "bin").mkdir()
            for name in ("nacre-app", "xdg-open"):
                (root / "bin" / name).write_text("helper")
            old = root / "hypr/retired.lua"
            old.write_text("owned old configuration")
            module.apply(home, root)
            old.unlink()
            backup = module.apply(home, root)
            self.assertFalse((home / ".config/hypr/retired.lua").exists())
            self.assertEqual(
                (backup / ".config/hypr/retired.lua").read_text(),
                "owned old configuration",
            )


class IdleMigrationTests(unittest.TestCase):
    def test_idle_migration_preserves_policy_and_other_drift_protection(self):
        with tempfile.TemporaryDirectory() as directory:
            root, home = Path(directory) / "repo", Path(directory) / "home"
            (root / "bin").mkdir(parents=True)
            (root / "hypr").mkdir()
            (root / "tools/defaults").mkdir(parents=True)
            for name in ("nacre-app", "xdg-open"):
                (root / "bin" / name).write_text("helper")
            source = root / "hypr/hypridle.conf"
            source.write_text("original")
            module.apply(home, root)
            target = home / ".config/hypr/hypridle.conf"
            target.write_text("manual locking only")
            source.write_text(module.IDLE_WRAPPER)
            (root / "tools/defaults/hypridle.conf").write_text("factory policy")
            with self.assertRaisesRegex(RuntimeError, "Local edit preserved"):
                module.plan(home, root)
            backup = module.apply(home, root, migrate_idle=True)
            self.assertEqual(
                (home / ".config/nacre/hypridle.local.conf").read_text(),
                "manual locking only",
            )
            self.assertEqual(
                (backup / ".config/hypr/hypridle.conf").read_text(),
                "manual locking only",
            )
            self.assertEqual(module.plan(home, root)[0], [])
            (home / ".local/bin/xdg-open").write_text("another personal edit")
            with self.assertRaisesRegex(RuntimeError, "Local edit preserved"):
                module.plan(home, root, migrate_idle=True)

    def test_conflicting_private_idle_policy_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as directory:
            root, home = Path(directory) / "repo", Path(directory) / "home"
            (root / "hypr").mkdir(parents=True)
            (root / "hypr/hypridle.conf").write_text(module.IDLE_WRAPPER)
            (home / ".config/hypr").mkdir(parents=True)
            (home / ".config/hypr/hypridle.conf").write_text("manual policy")
            (home / ".config/nacre").mkdir()
            (home / ".config/nacre/hypridle.local.conf").write_text("different policy")
            with self.assertRaisesRegex(RuntimeError, "Private idle policy differs"):
                module.plan(home, root, migrate_idle=True)


class PowerMigrationTests(unittest.TestCase):
    def test_only_recognized_power_hook_is_migrated(self):
        with tempfile.TemporaryDirectory() as directory:
            root, home = Path(directory) / "repo", Path(directory) / "home"
            (root / "hypr").mkdir(parents=True)
            (home / ".config/hypr/conf").mkdir(parents=True)
            source = root / "hypr/hyprland.lua"
            source.write_text("base\n\n-- Optional private host behavior.\nnew hook\n")
            current = home / ".config/hypr/hyprland.lua"
            current.write_text("base\n" + module.POWER_TAIL)
            (home / ".config/hypr/conf/manual-power.lua").write_text("host behavior")
            self.assertEqual(module.power_policy(home, root, True), b"host behavior")
            current.write_text(current.read_text() + "unrelated edit")
            self.assertIsNone(module.power_policy(home, root, True))


class SharedHelperOwnershipTests(unittest.TestCase):
    def setUp(self):
        from unittest.mock import patch

        selected = patch.object(
            module,
            "selected_profile",
            return_value={"components": ["configs", "shell", "ai", "brain"]},
        )
        selected.start()
        self.addCleanup(selected.stop)

    def test_app_routes_do_not_adopt_sync_and_known_local_edits_are_preserved(self):
        from unittest.mock import patch

        with tempfile.TemporaryDirectory() as folder:
            root, home = Path(folder) / "repo", Path(folder) / "home"
            (root / "bin").mkdir(parents=True)
            for name in ("nacre-app", "xdg-open", "nacre-brain-sync"):
                (root / "bin" / name).write_text("new helper")
            target = home / ".local/bin/nacre-brain-sync"
            target.parent.mkdir(parents=True)
            target.write_text("recognized old helper")
            legacy = {".local/bin/nacre-brain-sync": module.digest(target)}
            with patch.object(module, "LEGACY_SYNC", legacy):
                module.apply(home, root, app_routes_only=True)
                self.assertEqual(target.read_text(), "recognized old helper")
                module.apply(home, root)
                self.assertEqual(target.read_text(), "new helper")
                target.write_text("recognized old helper")
                with self.assertRaisesRegex(RuntimeError, "Local edit preserved"):
                    module.plan(home, root)
