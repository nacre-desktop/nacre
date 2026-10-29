"""Install tiers (N0101): core+session by default, opt-in extras, clean uninstall."""

import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).parents[1]
sys.path.insert(0, str(TOOLS))
import configure  # noqa: E402
import nacre_modules  # noqa: E402

PROFILE = {"id": "default", "components": ["configs", "shell"]}
# Package-only files (deployment/core) so every extra has something to manage.
PACKAGE_ONLY = {
    ".config/gtk-3.0/settings.ini": b"[Settings]\ngtk-theme-name=Adwaita-dark\n",
    ".config/rofi/config.rasi": b'@import "~/.config/nacre/rofi.rasi"\n',
    ".local/share/dbus-1/services/org.freedesktop.FileManager1.service": b"[D-BUS Service]\n",
}
# Files a user already had before Nacre, one per module where that is plausible.
USER_FILES = {
    "hypr-config": ".config/hypr/hyprland.lua",
    "theming-kitty": ".config/kitty/kitty.conf",
    "theming-fish": ".config/fish/conf.d/90-nacre.fish",
    "theming-gtk": ".config/gtk-3.0/settings.ini",
    "theming-rofi": ".config/rofi/config.rasi",
    "theming-fastfetch": ".config/fastfetch/config.jsonc",
    "xdg-open": ".local/bin/xdg-open",
    "dolphin-override": ".local/share/applications/org.kde.dolphin.desktop",
}
# Directories every real account already has; uninstall never prunes them.
BASE_DIRS = (
    ".config/systemd/user",
    ".local/bin",
    ".local/share/applications",
    ".local/state",
    "Pictures",
)


def snapshot(home, skip=()):
    result = {}
    for directory, folders, names in os.walk(home):
        for name in folders + names:
            path = Path(directory) / name
            relative = path.relative_to(home).as_posix()
            if any(relative == s or relative.startswith(s + "/") for s in skip):
                continue
            if path.is_symlink():
                result[relative] = ("link", os.readlink(path))
            elif path.is_dir():
                result[relative] = ("dir",)
            else:
                result[relative] = ("file", path.read_bytes(), path.stat().st_mode)
    return result


class InstallModuleTests(unittest.TestCase):
    def setUp(self):
        folder = tempfile.TemporaryDirectory()
        self.addCleanup(folder.cleanup)
        self.home = Path(folder.name) / "home"
        sources = Path(folder.name) / "package-only"
        extra = {}
        for relative, data in PACKAGE_ONLY.items():
            source = sources / relative
            source.parent.mkdir(parents=True, exist_ok=True)
            source.write_bytes(data)
            extra[Path(relative)] = source
        original = configure.files
        for target, value in (
            ("files", lambda root=configure.ROOT: {**original(root), **extra}),
            ("selected_profile", lambda root=configure.ROOT: PROFILE),
        ):
            patcher = patch.object(configure, target, value)
            patcher.start()
            self.addCleanup(patcher.stop)
        for directory in BASE_DIRS:
            (self.home / directory).mkdir(parents=True)
        # User data that must survive everything.
        self.data = {
            ".local/share/nacre/ai/chats/one.json": b'{"chat": 1}\n',
            "Pictures/Wallpapers/personal.jpg": b"personal wallpaper",
            "Documents/Brain/note.md": b"# my note\n",
        }
        for relative, data in self.data.items():
            path = self.home / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)

    def user_file(self, module):
        path = self.home / USER_FILES[module]
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(f"user's own {module}\n".encode())
        path.chmod(0o640 if module != "xdg-open" else 0o755)
        return path

    def module_paths(self, name):
        return [
            path
            for path in configure.files(configure.ROOT)
            if nacre_modules.module_of(path) == name
        ]

    def test_every_extra_manages_files_and_module_names_are_stable(self):
        for name in nacre_modules.EXTRAS:
            self.assertTrue(self.module_paths(name), name)
        self.assertEqual(
            nacre_modules.module_of(".config/nacre/niri/config.kdl"), "session"
        )
        self.assertEqual(
            nacre_modules.module_of(".config/hypr/hyprlock.conf"), "session"
        )
        self.assertEqual(
            nacre_modules.module_of(".local/share/applications/nacre-ai.desktop"), "ai"
        )
        self.assertEqual(
            nacre_modules.module_of(".config/systemd/user/nacre-brain-sync.timer"),
            "brain",
        )

    def test_fresh_install_touches_only_core_and_session_paths(self):
        for name in ("theming-kitty", "xdg-open"):
            self.user_file(name)
        before = snapshot(self.home)
        configure.apply(self.home, configure.ROOT)
        after = snapshot(self.home)
        changed = {p for p in after if after[p] != before.get(p)} | (
            before.keys() - after.keys()
        )
        self.assertTrue(changed)
        own = nacre_modules.OWN_DIRS
        for path in changed:
            if after.get(path) == ("dir",) or any(
                path == d or path.startswith(d + "/") for d in own
            ):
                continue
            self.assertIn(nacre_modules.module_of(path), ("core", "session"), path)
            self.assertTrue(Path(path).name.startswith(("nacre", "orient")), path)
        self.assertFalse(before.keys() - after.keys())
        for relative in USER_FILES.values():
            self.assertEqual(after.get(relative), before.get(relative), relative)
        state = nacre_modules.read(self.home)
        self.assertEqual(state["enabled"], [])
        self.assertEqual(
            {entry["module"] for entry in state["files"].values()}, {"core"}
        )
        self.assertEqual(configure.plan(self.home, configure.ROOT)[0], [])

    def test_enabling_and_disabling_each_extra_restores_prior_files(self):
        configure.apply(self.home, configure.ROOT)
        for name in nacre_modules.EXTRAS:
            with self.subTest(module=name):
                if name in USER_FILES:
                    self.user_file(name)
                before = snapshot(self.home, skip=(".local/state/nacre",))
                enabled = nacre_modules.resolve(self.home) | {name}
                configure.apply(
                    self.home, configure.ROOT, modules=enabled, adopt={name}
                )
                self.assertIn(name, nacre_modules.read(self.home)["enabled"])
                for path in self.module_paths(name):
                    self.assertEqual(
                        (self.home / path).read_bytes(),
                        configure.content(
                            configure.files(configure.ROOT)[path], self.home
                        ),
                        path,
                    )
                self.assertNotEqual(
                    snapshot(self.home, skip=(".local/state/nacre",)), before
                )
                # A plain later setup keeps the module and changes nothing.
                self.assertEqual(configure.plan(self.home, configure.ROOT)[0], [])
                with contextlib.redirect_stdout(io.StringIO()):
                    nacre_modules.main(
                        ["module", "disable", name, "--yes"],
                        self.home,
                        configure.ROOT,
                    )
                self.assertEqual(
                    snapshot(self.home, skip=(".local/state/nacre",)), before
                )
                self.assertNotIn(name, nacre_modules.read(self.home)["enabled"])
                self.assertEqual(configure.plan(self.home, configure.ROOT)[0], [])

    def test_disable_keeps_files_edited_after_enable(self):
        configure.apply(self.home, configure.ROOT)
        user = self.user_file("theming-kitty")
        mine = user.read_bytes()
        enabled = nacre_modules.resolve(self.home) | {"theming-kitty"}
        configure.apply(
            self.home, configure.ROOT, modules=enabled, adopt={"theming-kitty"}
        )
        user.write_bytes(b"edited after enabling\n")
        actions = dict(nacre_modules.disable(self.home, "theming-kitty"))
        self.assertEqual(actions[".config/kitty/kitty.conf"], "keep-edited")
        nacre_modules.disable(self.home, "theming-kitty", apply=True)
        self.assertEqual(user.read_bytes(), b"edited after enabling\n")
        self.assertEqual(user.with_name("kitty.conf.pre-nacre").read_bytes(), mine)
        self.assertNotIn(".config/kitty/kitty.conf", nacre_modules.owned(self.home))

    def test_uninstall_leaves_home_as_before_install_except_user_data(self):
        for name in USER_FILES:
            self.user_file(name)
        before = snapshot(self.home)
        configure.apply(self.home, configure.ROOT)
        for name in nacre_modules.EXTRAS:
            enabled = nacre_modules.resolve(self.home) | {name}
            configure.apply(self.home, configure.ROOT, modules=enabled, adopt={name})
        self.assertEqual(
            set(nacre_modules.read(self.home)["enabled"]), set(nacre_modules.EXTRAS)
        )
        installed = snapshot(self.home)
        with contextlib.redirect_stdout(io.StringIO()) as output:
            nacre_modules.main(
                ["uninstall", "--purge", "--plan"], self.home, configure.ROOT
            )
        self.assertEqual(snapshot(self.home), installed)
        self.assertIn("restore original: ~/.config/kitty/kitty.conf", output.getvalue())
        self.assertIn("remove (--purge): ~/.config/nacre/", output.getvalue())
        with contextlib.redirect_stdout(io.StringIO()):
            nacre_modules.main(
                ["uninstall", "--purge", "--yes"], self.home, configure.ROOT
            )
        self.assertEqual(snapshot(self.home), before)

    def test_default_uninstall_keeps_user_authored_files_and_purge_removes_them(self):
        own = nacre_modules.OWN_DIRS[:2]
        before = snapshot(self.home, skip=own)
        configure.apply(self.home, configure.ROOT)
        authored = {
            ".config/nacre/niri/custom.kdl": b'binds { Mod+T { spawn "foot"; } }\n',
            ".config/nacre/widgets.json": b'{"version": 1, "widgets": []}\n',
            ".config/nacre/plugins/hello/plugin.toml": b'id = "hello"\n',
        }
        for relative, data in authored.items():
            path = self.home / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        cache = self.home / ".cache/nacre/wallpaper-media/thumb.png"
        cache.parent.mkdir(parents=True)
        cache.write_bytes(b"cache")
        installed = snapshot(self.home)
        with contextlib.redirect_stdout(io.StringIO()) as output:
            nacre_modules.main(["uninstall", "--plan"], self.home, configure.ROOT)
        self.assertEqual(snapshot(self.home), installed)
        plan = output.getvalue()
        for relative in authored:
            self.assertIn(
                f"keep (yours or created while using Nacre): ~/{relative}", plan
            )
        self.assertIn("remove: ~/.local/bin/nacre-colors [core]", plan)
        self.assertIn("remove (regenerated by setup): ~/.cache/nacre", plan)
        self.assertNotIn("--purge)", plan)
        with contextlib.redirect_stdout(io.StringIO()):
            nacre_modules.main(["uninstall", "--yes"], self.home, configure.ROOT)
        for relative, data in authored.items():
            self.assertEqual((self.home / relative).read_bytes(), data)
        self.assertFalse((self.home / ".cache/nacre").exists())
        self.assertFalse((self.home / ".local/state/nacre/configuration.json").exists())
        self.assertFalse(nacre_modules.manifest_path(self.home).exists())
        # Outside Nacre's own settings/state the HOME is as before install.
        self.assertEqual(snapshot(self.home, skip=(*own, ".cache")), before)
        with contextlib.redirect_stdout(io.StringIO()) as output:
            nacre_modules.main(
                ["uninstall", "--purge", "--plan"], self.home, configure.ROOT
            )
        for relative in authored:
            self.assertIn(f"remove (--purge): ~/{relative}", output.getvalue())
        self.assertIn("~/.local/share/nacre (chats", output.getvalue())
        with contextlib.redirect_stdout(io.StringIO()):
            nacre_modules.main(
                ["uninstall", "--purge", "--yes"], self.home, configure.ROOT
            )
        for directory in own:
            self.assertFalse((self.home / directory).exists())
        for relative, data in self.data.items():
            self.assertEqual((self.home / relative).read_bytes(), data)
        with contextlib.redirect_stdout(io.StringIO()):
            nacre_modules.main(
                ["uninstall", "--purge", "--purge-data", "--yes"],
                self.home,
                configure.ROOT,
            )
        self.assertFalse((self.home / ".local/share/nacre").exists())

    def test_upgrading_an_existing_install_keeps_its_modules(self):
        configure.apply(self.home, configure.ROOT)
        everything = nacre_modules.resolve(self.home, nacre_modules.EXTRAS)
        configure.apply(self.home, configure.ROOT, modules=everything)
        # Releases before N0101 had no module manifest.
        nacre_modules.manifest_path(self.home).unlink()
        installed = snapshot(self.home, skip=(".local/state/nacre",))
        self.assertEqual(configure.plan(self.home, configure.ROOT)[0], [])
        configure.apply(self.home, configure.ROOT)
        self.assertEqual(snapshot(self.home, skip=(".local/state/nacre",)), installed)
        self.assertEqual(
            set(nacre_modules.read(self.home)["enabled"]), set(nacre_modules.EXTRAS)
        )

    def test_status_files_lists_every_owned_path_with_its_module(self):
        configure.apply(self.home, configure.ROOT)
        receipt = json.loads(
            (self.home / ".local/state/nacre/configuration.json").read_text()
        )
        receipt.pop("version", None)
        with contextlib.redirect_stdout(io.StringIO()) as output:
            nacre_modules.main(["status", "--files"], self.home, configure.ROOT)
        rows = [line.split("\t") for line in output.getvalue().splitlines()]
        self.assertEqual({row[1][2:] for row in rows}, set(receipt))
        self.assertEqual({row[2] for row in rows}, {"managed"})
        self.assertTrue(all(row[0] == "core" for row in rows))

    def test_profile_modules_cannot_be_toggled_here(self):
        for name in ("ai", "brain", "core", "nope"):
            with self.assertRaises(ValueError):
                nacre_modules.disable(self.home, name)
        with self.assertRaises(ValueError):
            nacre_modules.resolve(self.home, {"nope"})
        self.assertIn(
            "hypr-config", nacre_modules.resolve(self.home, (), ("hyprland",))
        )


if __name__ == "__main__":
    unittest.main()
