"""Bead-only installer preflight and safe legacy-runtime cleanup."""

import contextlib
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from types import SimpleNamespace
from unittest.mock import Mock, patch

RUN = subprocess.run

TOOLS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("dropin_installer", TOOLS / "install.py")
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)


class InstallerDropinGuardTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="installer-dropin-")
        self.addCleanup(self.temporary.cleanup)
        self.base = Path(self.temporary.name)
        self.home = self.base / "home"
        self.home.mkdir()
        self.units = self.home / ".config/systemd/user"
        self.runtime = self.base / "runtime/systemd/user"
        self.dest = self.home / ".local/share/nacre/shell"
        self.state = self.home / ".local/state/nacre/shell"
        environment = {
            "HOME": str(self.home),
            "XDG_CONFIG_HOME": str(self.home / ".config"),
            "XDG_DATA_HOME": str(self.home / ".local/share"),
            "XDG_STATE_HOME": str(self.home / ".local/state"),
            "XDG_CACHE_HOME": str(self.home / ".cache"),
            "XDG_RUNTIME_DIR": str(self.base / "runtime"),
            "XDG_CONFIG_DIRS": str(self.base / "config-dirs"),
            "XDG_DATA_DIRS": str(self.base / "data-dirs"),
            "SYSTEMD_UNIT_PATH": str(self.base / "extra-units"),
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=" + str(self.base / "absent-bus"),
            "PYTHONDONTWRITEBYTECODE": "1",
            "NIRI_CONFIG": "",
        }
        for context in (
            patch.dict(os.environ, environment),
            patch.multiple(installer, HOME=self.home, DEST=self.dest, STATE=self.state),
        ):
            context.start()
            self.addCleanup(context.stop)
        self.fakebin = self.base / "bin"
        self.fakebin.mkdir()
        self.systemctl_log = self.base / "systemctl.log"
        self.put(
            self.fakebin / "systemctl",
            '#!/bin/sh\nprintf "%s\\n" "$*" >> "$SYSTEMCTL_LOG"\n',
        ).chmod(0o755)
        os.environ["PATH"] = str(self.fakebin) + os.pathsep + os.environ["PATH"]
        os.environ["SYSTEMCTL_LOG"] = str(self.systemctl_log)

    def put(self, path, content):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content)
        return path

    def dropin(
        self, value, name="runtime.conf", unit="nacre-shell.service", directory=None
    ):
        directory = self.units if directory is None else directory
        return self.put(directory / (unit + ".d") / name, "[Service]\n" + value + "\n")

    def script(self, name="niri-session-runtime"):
        return self.put(
            self.dest.parent / "niri-on-pc" / (name + ".py"),
            "#!/usr/bin/python3\n# old private script: " + name + "\n",
        )

    def snapshot(self):
        return {
            str(path.relative_to(self.base)): (
                path.lstat().st_mode,
                os.readlink(path)
                if path.is_symlink()
                else path.read_bytes()
                if path.is_file()
                else None,
            )
            for path in self.base.rglob("*")
        }

    def test_retired_niri_bindings_refuse_includes_before_any_change(self):
        config = self.put(
            self.home / ".local/share/nacre/niri-on-pc/session/config/config.kdl",
            'include "bindings.kdl"\n',
        )
        self.put(
            config.parent / "bindings.kdl",
            'binds { Mod+Space { spawn "sh" "-c" "python3 \\${NACRE_COMPOSITOR_TOOLS}/compositor.py launcher"; } }\n',
        )
        before = self.snapshot()
        with self.assertRaisesRegex(
            RuntimeError, "Niri configs reference retired compositor.py"
        ):
            installer.guard_retired_niri_bindings()
        self.assertEqual(self.snapshot(), before)

    def test_shell_update_requires_candidate_before_any_change(self):
        before = self.snapshot()
        with patch.object(installer, "bead_candidate", return_value=None):
            with self.assertRaisesRegex(RuntimeError, "A Bead build is required"):
                installer.deploy(code_only=True)
        self.assertEqual(self.snapshot(), before)
        self.assertFalse(self.systemctl_log.exists())

    def fixture_deployment(self, palette_loader=True):
        root = self.base / "candidate/tools"
        shell = self.base / "candidate/shell"
        shell.mkdir(parents=True, exist_ok=True)
        self.put(root / "nacre-helper", "#!/bin/sh\nexit 0\n").chmod(0o755)
        names = (
            "fish_title.fish",
            "nacre-shell.service",
            "nacre-sidebar-ai.service",
            "nacre-session-watch.service",
            "nacre-niri-widths.service",
            "menu.sh",
            "control.sh",
            "cli-bridge.sh",
            "launch.sh",
            "cli.json",
            "nacre-daemon.socket",
            "nacre-daemon.service",
            "rofi.rasi",
        )
        for name in names:
            self.put(root / name, "candidate\n")
        self.put(
            root / "profile-policy.py",
            'def load(home):\n    return {"components": []}\n',
        )
        self.put(
            root / "reference-style.json",
            '{"colours": {"primary": "#123456"}, "mode": "dark"}',
        )
        if palette_loader:
            self.put(
                self.home / ".config/hypr/hyprland.lua",
                '-- Nacre committed wallpaper palette\nload_private("palette")\n',
            )
        (self.home / ".config/quickshell").mkdir(parents=True, exist_ok=True)
        (self.dest / "bin").mkdir(parents=True, exist_ok=True)
        return root, shell

    def deploy_fixture(
        self,
        code_only,
        opt_in=False,
        existing_copy=False,
        migration_result=None,
        backend_name=None,
        palette_loader=True,
    ):
        root, _shell = self.fixture_deployment(palette_loader=palette_loader)
        candidate_binary = self.put(self.base / "bead/bin/nacre-shell", "#!/bin/sh\n")
        candidate_shell = self.base / "bead/shell"
        self.put(candidate_shell / "palette.json", "{}\n")
        self.put(candidate_shell / "bar/main.luau", "return {}\n")

        def run(command, **kwargs):
            if command[0] == "systemctl":
                return RUN(command, **kwargs)
            return subprocess.CompletedProcess(command, 0, "", "")

        backend = (
            patch.object(installer.compositor, "backend_name", backend_name)
            if backend_name is not None
            else contextlib.nullcontext()
        )
        with (
            backend,
            patch.object(installer, "ROOT", root),
            patch.object(installer.subprocess, "run", side_effect=run),
            patch.object(installer, "wait_daemon_healthy", return_value=True),
            patch.object(
                installer,
                "bead_candidate",
                return_value=None
                if existing_copy
                else (candidate_binary, candidate_shell),
            ),
            patch.object(
                installer,
                "migrate_ui_shell",
                return_value=migration_result
                or {"status": "complete", "shell": "bead"},
            ),
            patch.object(installer.compositor, "reload"),
            patch.object(installer, "ui_switch") as switch,
        ):
            switch.return_value.migration_launcher.return_value = (
                "/existing/bead/launch"
            )
            installer.deploy(code_only=code_only, retire_niri_experiment=opt_in)

    def test_niri_full_install_does_not_require_hyprland_palette_loader(self):
        self.deploy_fixture(False, backend_name="niri", palette_loader=False)
        self.assertFalse((self.home / ".config/hypr/hyprland.lua").exists())

    def test_hyprland_full_install_still_requires_palette_loader(self):
        with (
            patch.object(installer, "tr", return_value="missing palette loader"),
            self.assertRaisesRegex(RuntimeError, "missing palette loader"),
        ):
            self.deploy_fixture(False, backend_name="hyprland", palette_loader=False)

    def test_code_only_migration_can_reuse_installed_copy(self):
        self.deploy_fixture(True, False, existing_copy=True)

    def test_fresh_bead_install_preserves_shared_commands_and_bootstrap(self):
        self.deploy_fixture(False)
        for path in (
            self.home / ".local/bin/nacre-menu",
            self.home / ".local/bin/nacre-shell",
            self.dest / "bin/nacre_shell",
            self.home / ".config/nacre/cli.json",
            self.home / ".config/nacre/rofi.rasi",
            self.home / ".config/rofi/config.rasi",
            self.units / "nacre-daemon.socket",
            self.units / "nacre-daemon.service",
            self.home / ".config/fish/functions/fish_title.fish",
            self.home / ".local/state/nacre/scheme/current.txt",
        ):
            self.assertTrue(path.is_file(), path)
        self.assertFalse((self.dest / "bin/qs").exists())
        self.assertFalse((self.dest / "source").exists())

    def test_failed_bead_migration_preserves_legacy_files(self):
        source = self.put(self.dest / "source/shell.qml", "old shell bytes")
        config = self.home / ".config/quickshell/nacre"
        config.parent.mkdir(parents=True, exist_ok=True)
        config.symlink_to(self.dest / "source")
        with self.assertRaisesRegex(RuntimeError, "failed activation"):
            self.deploy_fixture(
                True,
                migration_result={"status": "stopped", "hint": "failed activation"},
            )
        self.assertEqual(source.read_text(), "old shell bytes")
        self.assertEqual(os.readlink(config), str(self.dest / "source"))

    def test_code_only_bead_update_preserves_shared_commands(self):
        self.deploy_fixture(True)
        self.assertTrue((self.home / ".local/bin/nacre-menu").is_file())
        self.assertTrue((self.dest / "bin/nacre_shell").is_file())

    def test_source_candidate_uses_release_binary_and_bead_tree(self):
        repo = self.base / "candidate"
        binary = self.put(repo / "target/release/nacre-shell", "binary")
        shell = repo / "shell"
        self.put(shell / "palette.json", "{}\n")
        self.put(shell / "bar/main.luau", "return {}\n")
        with patch.object(installer, "ROOT", repo / "nacre/shell-tools"):
            self.assertEqual(installer.bead_candidate(), (binary, shell))

    def test_retired_dropin_references_are_backed_up_before_removal(self):
        script = self.script()
        dropin = self.dropin("ExecStart=/usr/bin/python3 " + str(script))
        matches = installer.retired_dropins(retire_niri_experiment=True)
        self.assertEqual(matches, [dropin])
        backup, entries = installer.backup_retirements(matches)
        self.assertFalse(dropin.exists())
        self.assertEqual(len(entries), 2)
        self.assertEqual(
            {Path(entry["target"]) for entry in entries},
            {dropin, script},
        )
        self.assertTrue(all(Path(entry["backup"]).is_file() for entry in entries))
        self.assertTrue((backup / "manifest.json").is_file())

    def test_archive_moves_only_owned_legacy_runtime_and_keeps_custom_configs(self):
        old_source = self.dest / "source"
        self.put(old_source / "shell.qml", "legacy shell bytes\n")
        self.put(self.dest / "source.previous/old.qml", "older bytes\n")
        quickshell = self.home / ".config/quickshell/nacre"
        quickshell.parent.mkdir(parents=True, exist_ok=True)
        quickshell.symlink_to(old_source)
        custom = self.put(self.home / ".config/quickshell/custom.qml", "owner edit\n")
        backup = installer.archive_legacy_shell()
        self.assertFalse(old_source.exists())
        self.assertFalse(quickshell.exists())
        self.assertEqual(custom.read_text(), "owner edit\n")
        legacy = backup / "legacy-shell"
        self.assertEqual(
            (legacy / "source/shell.qml").read_text(), "legacy shell bytes\n"
        )
        self.assertEqual(
            (legacy / "source.previous/old.qml").read_text(), "older bytes\n"
        )
        moves = installer._state.read_json_file(backup / "manifest.json")
        self.assertIn(
            {"from": str(quickshell), "to": str(legacy / "config-quickshell-nacre")},
            moves,
        )

    def test_archive_preserves_unrecognized_quickshell_link(self):
        quickshell = self.home / ".config/quickshell/nacre"
        quickshell.parent.mkdir(parents=True, exist_ok=True)
        quickshell.symlink_to(self.home / "custom-shell")
        installer.archive_legacy_shell()
        self.assertTrue(quickshell.is_symlink())
        self.assertEqual(os.readlink(quickshell), str(self.home / "custom-shell"))

    def test_palette_cli_and_shared_dispatcher_are_not_archived(self):
        palette = self.put(self.dest / "bin/nacre_shell", "palette bridge bytes\n")
        dispatcher = self.put(
            self.home / ".local/bin/nacre-shell", "updated dispatcher\n"
        )
        installer.archive_legacy_shell()
        self.assertEqual(palette.read_text(), "palette bridge bytes\n")
        self.assertEqual(dispatcher.read_text(), "updated dispatcher\n")

    def test_control_dispatcher_update_keeps_timestamped_failure_backups(self):
        root = self.base / "checkout/nacre/shell-tools"
        source = self.put(root / "control.sh", "#!/bin/sh\nexec bead-control\n")
        target = self.put(self.home / ".local/bin/nacre-shell", "#!/bin/sh\nold\n")
        with patch.object(installer, "ROOT", root):
            first = installer.install_control_dispatcher()
            self.assertEqual(target.read_text(), source.read_text())
            self.assertEqual(first.read_text(), "#!/bin/sh\nold\n")
            source.write_text("#!/bin/sh\nexec bead-control-next\n")
            second = installer.install_control_dispatcher()
        self.assertNotEqual(first, second)
        self.assertTrue(first.is_file())
        self.assertEqual(target.read_text(), source.read_text())

    def test_shared_compositor_backends_are_not_retired(self):
        runtime = self.dest / "tools"
        shared = self.put(
            runtime / "compositor_backends/hyprland.py", "shared backend\n"
        )
        removed = self.put(runtime / "shell-supervisor.py", "old shell supervisor\n")
        installer.retire_native_helpers(runtime)
        self.assertTrue(shared.is_file())
        self.assertFalse(removed.exists())

    def test_direct_migration_works_while_live_daemon_holds_config_lease(self):
        switch = Mock()
        switch.state = self.home / ".local/state/nacre"
        switch.migrate.return_value = {"status": "complete", "shell": "bead"}
        install_controller = Mock()
        config = self.home / ".config/nacre"
        with (
            patch.multiple(installer, HOME=self.home, ROOT=self.base),
            patch.object(installer, "ui_switch", return_value=switch),
            patch.object(
                installer.writer_lease, "inherited_fd_from_env", return_value=None
            ),
            patch.dict(
                sys.modules,
                {"releases": SimpleNamespace(install_controller=install_controller)},
            ),
        ):
            with installer.writer_lease.writer_lease(config):
                result = installer.migrate_ui_shell()
        self.assertEqual(result, {"status": "complete", "shell": "bead"})
        install_controller.assert_called_once_with(self.base.parents[1], home=self.home)
        switch.migrate.assert_called_once_with(None, None)
        self.assertTrue((switch.state / "releases/release.lock").is_file())
