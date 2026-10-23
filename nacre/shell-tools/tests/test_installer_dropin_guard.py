"""Installer retirement is refused before mutation and reversible when opted in."""

import contextlib
import hashlib
import importlib.util
import io
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

    def test_retired_niri_bindings_refuse_includes_before_deployment(self):
        config = self.put(
            self.home / ".local/share/nacre/niri-on-pc/session/config/config.kdl",
            'include "bindings.kdl"\n',
        )
        bindings = self.put(
            config.parent / "bindings.kdl",
            'binds { Mod+Space { spawn "sh" "-c" "python3 ${NACRE_COMPOSITOR_TOOLS}/compositor.py launcher"; } }\n',
        )
        before = self.snapshot()
        for code_only in (False, True):
            with self.subTest(code_only=code_only):
                with self.assertRaisesRegex(
                    RuntimeError, "Niri configs reference retired compositor.py"
                ):
                    installer.deploy(code_only=code_only, retire_niri_experiment=True)
                self.assertEqual(self.snapshot(), before)
                self.assertFalse(self.systemctl_log.exists())
        bindings.write_text(
            'binds { Mod+Space { spawn "nacrectl" "ui" "nacre" "launcher" ""; } }\n'
        )
        installer.guard_retired_niri_bindings()

    def test_selected_niri_config_is_scanned_with_include_cycles(self):
        config = self.put(self.base / "private/config.kdl", 'include "bindings.kdl"\n')
        self.put(
            config.parent / "bindings.kdl",
            'include "config.kdl"\n// compositor.py was retired\n',
        )
        with patch.dict(os.environ, {"NIRI_CONFIG": str(config)}):
            installer.guard_retired_niri_bindings()
            config.write_text(
                'binds { Mod+Space { spawn "python3" "/tools/compositor.py" "launcher"; } }\n'
            )
            with self.assertRaisesRegex(
                RuntimeError, "Niri configs reference retired compositor.py"
            ):
                installer.guard_retired_niri_bindings()

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

    # N0037: code-only install restarts the daemon safely and refreshes the niri session.
    HEALTHY = (
        '{"safe":false,"services":{"tray":{"status":"running"}},"topics":["daemon"]}'
    )
    UNHEALTHY = '{"safe":false,"services":{},"topics":[]}'

    def fake_helper(self, status):
        # Invoked as nacrectl (symlink alias) it prints a canned daemon status.
        return (
            '#!/bin/sh\nif [ "$(basename "$0")" = nacrectl ]; then\n'
            "cat <<'EOF2'\n" + status + "\nEOF2\nfi\nexit 0\n"
        )

    def daemon_deploy(self, new_status, old_status=None, active=True):
        root, shell = self.fixture_deployment()
        self.put(root / "nacre-helper", self.fake_helper(new_status)).chmod(0o755)
        if old_status is not None:
            old = self.put(
                self.dest / "tools/nacre-helper", self.fake_helper(old_status)
            )
            old.chmod(0o755)
            self.put(self.dest / "source/marker.txt", "old source\n")
            self.put(self.dest / "tools/old-only.txt", "old tools\n")
        self.put(shell / "new-only.txt", "new source\n")
        self.put(
            self.fakebin / "systemctl",
            '#!/bin/sh\nprintf "%s\\n" "$*" >> "$SYSTEMCTL_LOG"\n'
            '[ "$ACTIVE" = 0 ] && case "$*" in *is-active*) exit 3;; esac\nexit 0\n',
        ).chmod(0o755)
        passthrough = (self.dest / "bin/nacrectl", "systemctl")

        def run(command, **kwargs):
            if command[0] in map(str, passthrough):
                return RUN(command, **kwargs)
            return subprocess.CompletedProcess(command, 0, "", "")

        with (
            patch.dict(os.environ, {"ACTIVE": "1" if active else "0"}),
            patch.multiple(installer, ROOT=root, SHELL=shell, DAEMON_WAIT_SECONDS=0.5),
            patch.object(installer.subprocess, "run", side_effect=run),
            patch.object(installer, "validate_live"),
            patch.object(installer, "stop_other_shells"),
            patch.object(installer.compositor, "reload"),
        ):
            installer.deploy(code_only=True)
        return root

    def calls(self):
        return self.systemctl_log.read_text().splitlines()

    def test_code_only_restarts_active_daemon_before_shell(self):
        root = self.daemon_deploy(self.HEALTHY, self.UNHEALTHY)
        calls = self.calls()
        daemon = calls.index("--user restart nacre-daemon.service")
        self.assertLess(daemon, calls.index("--user restart nacre-shell"))
        self.assertEqual(
            (self.dest / "tools/nacre-helper").read_bytes(),
            (root / "nacre-helper").read_bytes(),
        )

    def test_code_only_unhealthy_daemon_restores_previous_helper(self):
        old = self.fake_helper(self.HEALTHY)
        with self.assertRaisesRegex(RuntimeError, "previous helper was restored"):
            self.daemon_deploy(self.UNHEALTHY, self.HEALTHY)
        self.assertEqual((self.dest / "tools/nacre-helper").read_text(), old)
        self.assertEqual(self.calls().count("--user restart nacre-daemon.service"), 2)
        # N0053: source/ and tools/ roll back with the helper (never a mixed install), and
        # nacre-shell restarts once, after the second daemon restart, on the restored tree.
        self.assertEqual((self.dest / "source/marker.txt").read_text(), "old source\n")
        self.assertFalse((self.dest / "source/new-only.txt").exists())
        self.assertEqual((self.dest / "tools/old-only.txt").read_text(), "old tools\n")
        self.assertFalse((self.dest / "tools/fish_title.fish").exists())
        self.assertFalse((self.dest / "source.previous").exists())
        self.assertFalse((self.dest / "tools.previous").exists())
        calls = self.calls()
        self.assertEqual(calls.count("--user restart nacre-shell"), 1)
        self.assertGreater(
            calls.index("--user restart nacre-shell"),
            len(calls) - 1 - calls[::-1].index("--user restart nacre-daemon.service"),
        )

    def test_code_only_same_helper_or_stopped_daemon_is_not_restarted(self):
        self.daemon_deploy(self.HEALTHY, self.UNHEALTHY)  # differs: restarts once
        self.systemctl_log.unlink()
        self.daemon_deploy(self.HEALTHY, self.HEALTHY)  # now identical
        self.assertNotIn("--user restart nacre-daemon.service", self.calls())
        self.systemctl_log.unlink()
        self.daemon_deploy(self.UNHEALTHY, self.HEALTHY, active=False)
        self.assertNotIn("--user restart nacre-daemon.service", self.calls())

    def test_daemon_health_requires_services_topics_and_no_failure(self):
        healthy = installer.daemon_status_healthy
        self.assertTrue(healthy(self.HEALTHY))
        self.assertFalse(healthy(self.UNHEALTHY))
        self.assertFalse(healthy("not json"))
        self.assertFalse(healthy('{"safe":true,"services":{"a":{}},"topics":["t"]}'))
        self.assertFalse(
            healthy('{"services":{"a":{"status":"error"}},"topics":["t"]}')
        )

    def niri_fixture(self, revision="old", stock=False):
        session = self.home / ".local/share/nacre/niri-on-pc/session"
        self.put(session / "source-revision", revision + "\n")
        self.put(session / "config/config.kdl", "old config\n")
        if stock:
            self.put(session / "stock-niri", "stock\n")
        custom = self.put(self.home / ".config/nacre/niri/custom.kdl", "mine\n")
        return session, custom

    def test_niri_refresh_plan_needs_prepared_session_and_changed_revision(self):
        with patch.object(installer, "repo_revision", return_value="new"):
            self.assertIsNone(installer.niri_session_refresh_plan())  # no session
            session, _ = self.niri_fixture("new")
            self.assertIsNone(installer.niri_session_refresh_plan())  # same revision
            self.put(session / "source-revision", "old\n")
            reason, command, stock = installer.niri_session_refresh_plan()
            self.assertEqual(command[-1], "prepare")
            self.assertNotIn("--stock-niri", command)
            self.put(session / "stock-niri", "x\n")
            self.assertIn("--stock-niri", installer.niri_session_refresh_plan()[1])
        with patch.object(installer, "repo_revision", return_value=""):
            self.assertIsNone(installer.niri_session_refresh_plan())

    def test_niri_refresh_backs_up_replaces_and_spares_custom_kdl(self):
        session, custom = self.niri_fixture()
        plan = ("old -> new", ["prep"], False)

        def prepare(command, **kwargs):
            self.put(session / "source-revision", "new\n")
            self.put(session / "config/config.kdl", "new config\n")
            return subprocess.CompletedProcess(command, 0)

        with patch.object(installer.subprocess, "run", side_effect=prepare):
            installer.refresh_niri_session(plan)
        self.assertEqual((session / "config/config.kdl").read_text(), "new config\n")
        previous = session.with_name("session.previous")
        self.assertEqual((previous / "config/config.kdl").read_text(), "old config\n")
        self.assertEqual(custom.read_text(), "mine\n")

    def test_niri_refresh_failure_restores_previous_session(self):
        session, custom = self.niri_fixture()

        def prepare(command, **kwargs):
            self.put(session / "partial", "half\n")
            raise subprocess.CalledProcessError(1, command)

        with patch.object(installer.subprocess, "run", side_effect=prepare):
            with self.assertRaises(subprocess.CalledProcessError):
                installer.refresh_niri_session(("old -> new", ["prep"], False))
        self.assertEqual((session / "config/config.kdl").read_text(), "old config\n")
        self.assertEqual((session / "source-revision").read_text(), "old\n")
        self.assertFalse((session / "partial").exists())
        self.assertFalse(session.with_name("session.previous").exists())
        self.assertEqual(custom.read_text(), "mine\n")

    def test_dry_run_prints_both_steps_and_changes_nothing(self):
        root, _ = self.fixture_deployment()
        self.put(self.dest / "tools/nacre-helper", "old helper")
        self.niri_fixture()
        before = self.snapshot()
        out = io.StringIO()
        with (
            patch.multiple(installer, ROOT=root),
            patch.object(installer, "repo_revision", return_value="new"),
            patch.object(installer, "daemon_active", return_value=True),
            contextlib.redirect_stdout(out),
        ):
            installer.dry_run()
        self.assertEqual(self.snapshot(), before)
        text = out.getvalue()
        self.assertIn("restart nacre-daemon.service", text)
        self.assertIn("refresh the generated niri session config", text)
        self.assertIn("next login", text)
