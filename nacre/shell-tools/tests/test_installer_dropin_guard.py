"""Installer retirement is refused before mutation and reversible when opted in."""

import hashlib
import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("dropin_installer", TOOLS / "install.py")
installer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(installer)
RUN = subprocess.run


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

    def test_cli_refuses_before_any_write_or_systemctl_for_both_install_modes(self):
        script = self.script()
        dropin = self.dropin(
            "ExecStart=/usr/bin/python3 %h/.local/share/nacre/niri-on-pc/niri-session-runtime.py shell"
        )
        before = self.snapshot()
        for flags in ([], ["--code-only"]):
            result = RUN(
                [sys.executable, str(TOOLS / "install.py"), *flags],
                env=dict(os.environ),
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            for text in (
                str(dropin),
                str(script),
                "--retire-niri-experiment",
                "before any changes",
            ):
                self.assertIn(text, result.stderr)
            self.assertEqual(self.snapshot(), before)
            self.assertFalse(self.systemctl_log.exists())

    def test_all_managed_units_and_execution_environment_directives(self):
        script = self.script()
        directories = installer.user_unit_directories()
        for index, unit in enumerate(installer.MANAGED_UNITS):
            for directive in (
                "ExecStart",
                "ExecStartPre",
                "ExecStartPost",
                "ExecCondition",
                "ExecStop",
                "ExecStopPost",
                "Environment",
                "EnvironmentFile",
            ):
                dropin = self.dropin(
                    directive + "=" + str(script),
                    unit=unit,
                    directory=directories[index % len(directories)]
                    if index < 4
                    else self.runtime,
                )
                with patch.object(installer.subprocess, "check_output") as build:
                    with self.assertRaisesRegex(RuntimeError, "before any changes"):
                        installer.deploy()
                    build.assert_not_called()
                dropin.unlink()

    def test_searches_xdg_runtime_override_and_dash_type_dropins(self):
        script = self.script()
        # Restrict writes to disposable search directories, even when host paths are listed.
        for directory in (
            self.units,
            self.runtime,
            self.base / "extra-units",
            self.home / ".local/share/systemd/user",
            self.base / "config-dirs/systemd/user",
            self.base / "data-dirs/systemd/user",
        ):
            for unit in ("nacre-shell.service", "nacre-.service", "service"):
                dropin = self.dropin(
                    "EnvironmentFile=-" + str(script), unit=unit, directory=directory
                )
                with self.assertRaisesRegex(RuntimeError, str(dropin)):
                    installer.retired_dropins()
                dropin.unlink()

    def test_unreadable_dropin_directory_refuses_before_any_write(self):
        before = self.snapshot()
        with patch.object(
            Path, "iterdir", side_effect=PermissionError("private drop-ins")
        ):
            with self.assertRaisesRegex(PermissionError, "private drop-ins"):
                installer.deploy(retire_niri_experiment=True)
        self.assertEqual(self.snapshot(), before)

    def test_opt_in_never_removes_other_retired_helper_references(self):
        niri = self.script()
        dropin = self.dropin(
            "ExecStart="
            + str(niri)
            + "\nExecStop="
            + str(self.dest / "tools/session-watch.py")
        )
        before = self.snapshot()
        with self.assertRaisesRegex(RuntimeError, "edit or remove"):
            installer.deploy(retire_niri_experiment=True)
        self.assertEqual(self.snapshot(), before)
        self.assertTrue(dropin.exists())

    def test_conditional_retired_config_and_dbus_paths_are_guarded(self):
        config = self.put(self.home / ".config/nacre/shell.json", "old owned config")
        dbus = self.put(
            self.home / ".local/share/dbus-1/services/org.erikreider.swaync.service",
            "start nacre-shell.service",
        )
        for path in (config, dbus):
            dropin = self.dropin("EnvironmentFile=" + str(path))
            before = self.snapshot()
            with patch.object(installer, "unused_config_matches", return_value=True):
                with self.assertRaisesRegex(RuntimeError, str(path)):
                    installer.deploy(retire_niri_experiment=True)
            self.assertEqual(self.snapshot(), before)
            dropin.unlink()
        self.dropin("EnvironmentFile=" + str(config))
        self.assertEqual(installer.retired_dropins(), [])
        with patch.object(
            installer,
            "digest",
            return_value="05a4a1ffca4933456163dc394e2fe645457787304d2ea715a970b4da84cfe0f1",
        ):
            self.assertTrue(installer.unused_config_matches(config))
        self.assertFalse(installer.unused_config_matches(config))

    def fixture_deployment(self):
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
        self.put(
            self.home / ".config/hypr/hyprland.lua",
            '-- Nacre committed wallpaper palette\nload_private("palette")\n',
        )
        (self.home / ".config/quickshell").mkdir(parents=True, exist_ok=True)
        (self.dest / "bin").mkdir(parents=True, exist_ok=True)
        return root, shell

    def deploy_fixture(self, code_only, opt_in):
        root, shell = self.fixture_deployment()

        def run(command, **kwargs):
            if command[0] == "systemctl":
                return RUN(command, **kwargs)
            return subprocess.CompletedProcess(command, 0, "", "")

        with (
            patch.multiple(installer, ROOT=root, SHELL=shell),
            patch.object(installer.subprocess, "run", side_effect=run),
            patch.object(installer, "validate_live"),
            patch.object(installer, "stop_other_shells"),
            patch.object(installer.compositor, "reload"),
        ):
            installer.deploy(code_only=code_only, retire_niri_experiment=opt_in)

    def test_source_install_uses_helper_from_build_output(self):
        root, shell = self.fixture_deployment()
        (root / "nacre-helper").unlink()
        helper_log = self.base / "helper.log"
        helper = self.put(
            self.base / "target/release/nacre-helper",
            '#!/bin/sh\nprintf "%s\\n" "$*" >> "$HELPER_LOG"\n',
        )
        helper.chmod(0o755)

        def run(command, **kwargs):
            if command[0] == "systemctl" or command[1:] == ["desktop-settings", "init"]:
                return RUN(command, **kwargs)
            return subprocess.CompletedProcess(command, 0, "", "")

        with (
            patch.multiple(installer, ROOT=root, SHELL=shell),
            patch.dict(os.environ, {"HELPER_LOG": str(helper_log)}),
            patch.object(
                installer.subprocess, "check_output", return_value=str(helper) + "\n"
            ) as build,
            patch.object(installer.subprocess, "run", side_effect=run),
            patch.object(installer, "validate_live"),
            patch.object(installer, "stop_other_shells"),
            patch.object(installer.compositor, "reload"),
        ):
            installer.deploy()
        build.assert_called_once_with(
            ["bash", str(root.parent.parent / "tools/build-helper.sh")], text=True
        )
        self.assertFalse((root / "nacre-helper").exists())
        self.assertEqual(
            (self.dest / "tools/nacre-helper").read_bytes(), helper.read_bytes()
        )
        self.assertEqual(helper_log.read_text(), "desktop-settings init\n")

    def test_validate_live_retries_timed_out_first_qs_answer(self):
        command = [
            str(self.dest / "bin/qs"),
            "-c",
            "nacre",
            "ipc",
            "call",
            "nacre",
            "state",
        ]
        answers = [
            subprocess.TimeoutExpired(command, 3),
            subprocess.CompletedProcess(command, 0, '{"reveal": true}', ""),
        ]
        with (
            patch.dict(os.environ, {"NACRE_RELEASE_TRANSACTION": "1"}),
            patch.object(installer.subprocess, "run", side_effect=answers) as run,
            patch.object(installer.time, "sleep") as sleep,
        ):
            installer.validate_live()
        self.assertEqual(run.call_count, 2)
        for call in run.call_args_list:
            self.assertEqual(call.args[0], command)
            self.assertEqual(call.kwargs["timeout"], 3)
        sleep.assert_called_once_with(0.25)

    def test_validate_live_all_timeouts_exhaust_existing_retry_budget(self):
        with (
            patch.object(
                installer.subprocess,
                "run",
                side_effect=subprocess.TimeoutExpired("qs", 3),
            ) as run,
            patch.object(installer.time, "sleep"),
        ):
            with self.assertRaisesRegex(RuntimeError, "Desktop did not load"):
                installer.validate_live()
        self.assertEqual(run.call_count, 80)

    def test_opt_in_install_and_restore_byte_identical_full_and_code_only(self):
        # Each mode runs as an independent private install/restore transaction.
        for code_only in (False, True):
            with self.subTest(code_only=code_only):
                scripts = [self.script(name) for name in installer.RETIRED_NIRI]
                scripts[0].chmod(0o750)
                matching = [
                    self.dropin(
                        "ExecStart=\nExecStart=/usr/bin/python3 %h/.local/share/nacre/niri-on-pc/niri-session-runtime.py shell"
                    ),
                    self.dropin(
                        "EnvironmentFile=" + str(scripts[1]), directory=self.runtime
                    ),
                ]
                unrelated = self.dropin("Environment=KEEP=1", name="unrelated.conf")
                gate = self.dropin(
                    "ExecCondition=%h/.local/share/nacre/niri-on-pc/niri-session-gate.py shell",
                    name="acceptance.conf",
                )
                originals = {
                    path: (path.read_bytes(), path.stat().st_mode)
                    for path in scripts + matching + [unrelated, gate]
                }
                self.deploy_fixture(code_only, True)
                for path in scripts + matching:
                    self.assertFalse(path.exists(), path)
                for path in (unrelated, gate):
                    self.assertEqual(
                        (path.read_bytes(), path.stat().st_mode), originals[path]
                    )
                backup = Path((self.state / "latest-backup").read_text())
                manifest = installer._state.read_json_file(backup / "manifest.json")
                for path in scripts + matching:
                    entry = next(
                        entry for entry in manifest if entry["target"] == str(path)
                    )
                    self.assertEqual(
                        entry["before_hash"],
                        hashlib.sha256(originals[path][0]).hexdigest(),
                    )
                    self.assertEqual(
                        Path(entry["backup"]).read_bytes(), originals[path][0]
                    )
                with patch.object(installer.compositor, "reload"):
                    installer.restore()
                for path, original in originals.items():
                    self.assertEqual((path.read_bytes(), path.stat().st_mode), original)
                self.assertIn("--user daemon-reload", self.systemctl_log.read_text())
                for path in matching + [unrelated, gate]:
                    path.unlink()

    def test_no_dropins_still_retires_and_backs_up_scripts(self):
        scripts = [self.script(name) for name in installer.RETIRED_NIRI]
        self.deploy_fixture(False, False)
        self.assertTrue(all(not path.exists() for path in scripts))
        with patch.object(installer.compositor, "reload"):
            installer.restore()
        self.assertTrue(all(path.is_file() for path in scripts))

    def test_no_experiment_needs_no_retirement_backup(self):
        self.assertEqual(installer.retired_dropins(), [])
        self.assertEqual(installer.backup_retirements([]), (None, []))
        self.assertFalse(self.state.exists())

    def test_comments_similar_paths_masks_and_home_aliases(self):
        script = self.script()
        ignored = self.dropin(
            "# ExecStart=" + str(script) + "\nExecStart=" + str(script) + ".old"
        )
        mask = self.units / "nacre-shell.service.d/masked.conf"
        mask.symlink_to("/dev/null")
        self.assertEqual(installer.retired_dropins(), [])
        for prefix in ("%h", "$HOME", "${HOME}", "~", str(self.home)):
            dropin = self.dropin(
                'Environment="SCRIPT='
                + prefix
                + '/.local/share/nacre/niri-on-pc/niri-session-runtime.py"',
                name="match.conf",
            )
            self.assertEqual(installer.retired_dropins(True), [dropin])
            dropin.unlink()
        self.assertTrue(ignored.exists())
        self.assertTrue(mask.is_symlink())

    def test_continuations_and_systemd_escapes_with_spaces_in_home(self):
        home = self.home / "with spaces"
        dest = home / ".local/share/nacre/shell"
        script = dest.parent / "niri-on-pc/niri-session-runtime.py"
        with patch.multiple(installer, HOME=home, DEST=dest):
            for value in (
                "ExecStart=/usr/bin/python3 \\ by-continuation",
                'EnvironmentFile="' + str(script) + '"',
                'ExecStart="'
                + str(script).replace("/", r"\x2f").replace(" ", r"\s")
                + '"',
                'ExecStart="' + str(script).replace("/", r"\057") + '"',
            ):
                if "by-continuation" in value:
                    value = 'ExecStart=/usr/bin/python3 \\\n  "%h/.local/share/nacre/niri-on-pc/niri-session-runtime.py"'
                dropin = self.dropin(value)
                self.assertEqual(installer.retired_dropins(True), [dropin])
                dropin.unlink()

    def test_restore_preserves_later_edits_and_verifies_backup_hash(self):
        script = self.script()
        backup, entries = installer.backup_retirements([])
        script.mkdir()
        with self.assertRaisesRegex(RuntimeError, "Later edit preserved"):
            installer.restore()
        script.rmdir()
        Path(entries[0]["backup"]).write_text("tampered")
        with self.assertRaisesRegex(RuntimeError, "hash mismatch"):
            installer.restore()
        self.assertFalse(self.systemctl_log.exists())
        self.assertTrue(backup.is_dir())

    def test_opt_in_symlink_dropin_restore_preserves_link_and_referent(self):
        script = self.script()
        target = self.put(
            self.base / "private.conf", "[Service]\nExecStart=" + str(script) + "\n"
        )
        link = self.units / "nacre-shell.service.d/link.conf"
        link.parent.mkdir(parents=True)
        link.symlink_to(target)
        before = target.read_bytes()
        installer.backup_retirements(installer.retired_dropins(True))
        self.assertFalse(link.is_symlink())
        self.assertEqual(target.read_bytes(), before)
        with patch.object(installer.compositor, "reload"):
            installer.restore()
        self.assertEqual(os.readlink(link), str(target))
        self.assertEqual(link.read_bytes(), before)
