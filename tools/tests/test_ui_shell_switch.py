"""Private-HOME ui-shell tests with a modelled user manager; never touches a real one."""

import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1]))
import f4_switch as f4  # noqa: E402
import ui_shell_switch as m  # noqa: E402

LEGACY_EXEC = "{ path=/usr/bin/nacre-shell ; argv[]=/usr/bin/nacre-shell start ; ignore_errors=no ; }"


def done(command, out="", code=0, err=""):
    return subprocess.CompletedProcess(command, code, out, err)


class Manager:
    """Models what matters: reload caches the unit files, restart applies the cache."""

    def __init__(self, home, launcher):
        self.root = home / ".config/systemd/user"
        self.root.mkdir(parents=True)
        self.launcher = str(launcher)
        self.cfg = {"bead": False, "windows": False}
        self.shell = ("legacy", 1, "active")  # kind, pid, state
        self.windows = None
        self.wallpaper = None  # supervised child in the shell's control group
        self.pid = 1
        self.calls, self.violations = [], []
        self.fail, self.flap = {}, False
        self.bead_crashes, self.launcher_fails = False, False
        self.provider_after, self.foreign = 0, []
        self.mutations = 0
        self.topics = ["settings.store.desktop"]
        self.socket_active = True
        self.shell_active = True
        self.client_on = True
        self.daemon_down = False
        self.result = "success"  # the shell unit's Result property
        self.journal = []
        # Daemon model (N0068): reload caches the drop-in, restart applies it.
        self.daemon_cfg = self.daemon_mode = False
        self.notifications_broken = False
        self.daemon_foreign = []

    def daemon_dropin_path(self):
        return self.root / (m.DAEMON + ".d") / m.DAEMON_DROPIN_NAME

    def notifications_running(self):
        """The managed daemon takes the bus name only once the LEGACY shell has released it."""
        legacy_up = (
            self.shell[0] == "legacy"
            and self.shell[2] == "active"
            and self.shell_active
        )
        return self.daemon_mode and not self.notifications_broken and not legacy_up

    def dropin_text(self):
        path = self.root / (m.SHELL + ".d") / m.DROPIN_NAME
        return path.read_text() if path.exists() else ""

    def start_shell(self):
        self.pid += 1
        kind = "bead" if self.cfg["bead"] else "legacy"
        state = "failed" if kind == "bead" and self.bead_crashes else "active"
        self.shell = (kind, self.pid, state)
        if kind == "legacy" and self.daemon_mode:
            self.violations.append("legacy shell started beside the managed daemon")
        if kind == "bead" and self.daemon_cfg and not self.notifications_running():
            self.violations.append("bead started before the daemon owned notifications")
        if kind == "bead" and state == "active":
            self.pid += 1
            self.wallpaper = self.pid
        if (
            kind == "bead"
            and self.cfg["windows"]
            and (self.root / (m.SHELL + ".d") / m.DROPIN_NAME).exists()
        ):  # ConditionPathExists
            self.pid += 1
            self.windows = self.pid

    def check(self):
        if self.shell and self.shell[0] == "legacy" and self.windows:
            self.violations.append("legacy shell beside windows host")
        if self.shell and self.shell[0] == "legacy" and self.wallpaper:
            self.violations.append("legacy shell beside Bead wallpaper child")

    def provider(self):
        if not self.shell or self.shell[2] != "active":
            return False
        if self.shell[0] == "bead":
            self.provider_after -= 1
            return self.provider_after < 0
        return True

    def __call__(self, command, **kwargs):
        self.calls.append(command)
        if command[0] == "systemd-cat":
            self.journal.append(kwargs.get("input", ""))
            return done(command)
        if command[0] == self.launcher:
            if self.launcher_fails:
                raise subprocess.CalledProcessError(1, command)
            return done(command)
        if command[0] == "/usr/bin/nacrectl":
            method = command[-2]
            if method == "daemon.status":
                return done(
                    command,
                    json.dumps(
                        {
                            "boot": "b",
                            "safe": False,
                            "services": {
                                "settings": {"status": "running"},
                                **(
                                    {"notifications": {"status": "running"}}
                                    if self.notifications_running()
                                    else {}
                                ),
                            },
                            "topics": [
                                *self.topics,
                                *(
                                    ["notifications"]
                                    if self.notifications_running()
                                    else []
                                ),
                            ],
                        }
                    ),
                )
            if method == "settings.get":
                return done(
                    command,
                    json.dumps(
                        {
                            "store": "desktop",
                            "rev": 1,
                            "exists": True,
                            "data": {},
                            "error": None,
                            "dirty": False,
                        }
                    ),
                )
            # The real CLI prints only the message, on stderr, exit 1 (measured under systemd, B0033).
            if self.daemon_down:
                return done(command, "", 1, "Nacre daemon is unavailable\n")
            if self.provider():
                return done(command, "", 1, "UI method is unavailable\n")
            return done(command, "", 1, "UI provider is unavailable\n")
        if Path(command[0]).name == "launch" and command[1:] == ["check"]:
            return subprocess.run(command, **kwargs)
        assert command[:2] == ["systemctl", "--user"]
        action, *args = command[2:]
        if action == "show-environment":
            return done(command, f"HOME={self.root}\n")
        if action == "show":
            names = [
                n
                for a in args
                if a.startswith("--property=")
                for n in a[11:].split(",")
            ]
            if not args[0].startswith("--"):
                return done(
                    command,
                    "\n".join(f"{k}={v}" for k, v in self.unit(args[0], names).items()),
                )
            return done(command, f"UnitPath={self.root}")
        if self.fail.get(action):
            self.fail[action] -= 1
            raise subprocess.CalledProcessError(1, command)
        if action == "daemon-reload":
            self.cfg["bead"] = (self.root / (m.SHELL + ".d") / m.DROPIN_NAME).exists()
            self.cfg["windows"] = (self.root / m.WINDOWS).exists()
            self.daemon_cfg = self.daemon_dropin_path().exists()
        elif action == "restart" and args[0] == m.DAEMON:
            self.daemon_mode = self.daemon_cfg
            if (
                self.daemon_mode
                and self.shell[0] == "legacy"
                and self.shell[2] == "active"
            ):
                self.violations.append("daemon restarted while LEGACY held the name")
        elif action == "stop" and args[0] == m.SHELL:
            self.windows = self.wallpaper = None
            self.shell = (self.shell[0], self.shell[1], "inactive")
        elif action == "restart" and args[0] == m.SHELL:
            self.windows = self.wallpaper = None
            self.start_shell()
        elif action == "reset-failed":
            pass
        elif action == "stop" and args[0] == m.WINDOWS:
            self.windows = None
        else:
            raise AssertionError(command)
        self.check()
        self.mutations += 1
        return done(command)

    def unit(self, name, names):
        if name == m.SHELL:
            kind, pid, state = self.shell
            if self.flap and kind == "bead":
                self.pid += 1
                pid = self.pid
            exec_start = LEGACY_EXEC
            if self.cfg["bead"]:
                match = re.search(
                    r"(?m)^ExecStart=\nExecStart=(\S+ start)$", self.dropin_text()
                )
                if match:
                    launcher = match.group(1)[:-6]
                    exec_start = f"{{ path={launcher} ; argv[]={launcher} start ; ignore_errors=no ; }}"
            values = {
                "FragmentPath": "/usr/lib/systemd/user/" + name,
                "LoadState": "loaded",
                "ActiveState": state if self.shell_active else "inactive",
                "SubState": "running" if state == "active" else "failed",
                "MainPID": str(pid),
                "InvocationID": f"{pid:032x}",
                "NRestarts": "0",
                "Result": self.result,
                "UnitFileState": "enabled",
                "Environment": self.shell_environment(),
                "ExecStart": exec_start,
                "DropInPaths": " ".join(
                    [
                        *self.foreign,
                        *(
                            [str(self.root / (m.SHELL + ".d") / m.DROPIN_NAME)]
                            if self.cfg["bead"]
                            else []
                        ),
                    ]
                ),
            }
        elif name == m.WINDOWS:
            values = {
                "LoadState": "loaded" if self.cfg["windows"] else "not-found",
                "ActiveState": "active" if self.windows else "inactive",
            }
        elif name == m.DAEMON:
            values = {
                "FragmentPath": "/usr/lib/systemd/user/" + name,
                "LoadState": "loaded",
                "UnitFileState": "static",
                "ActiveState": "inactive",
                "ExecStart": "{ path=/usr/bin/nacre-daemon ; argv[]=/usr/bin/nacre-daemon ; ignore_errors=no ; }",
                "DropInPaths": " ".join(
                    [
                        *self.daemon_foreign,
                        *([str(self.daemon_dropin_path())] if self.daemon_cfg else []),
                    ]
                ),
            }
        else:
            values = {
                "FragmentPath": "/usr/lib/systemd/user/" + name,
                "LoadState": "loaded",
                "UnitFileState": "static",
                "ActiveState": "active"
                if name == f4.SOCKET and self.socket_active
                else "inactive",
            }
        return {k: v for k, v in values.items() if k in names or names == f4.PROPERTIES}

    def shell_environment(self):
        values = ["NACRE_DAEMON_CLIENT=1"] if self.client_on else []
        if self.cfg["bead"]:
            match = re.search(
                r"(?m)^Environment=NACRE_UI_NOTIFICATIONS=(none|daemon)$",
                self.dropin_text(),
            )
            if match:
                values.append("NACRE_UI_NOTIFICATIONS=" + match.group(1))
        return " ".join(values)


class Clock:
    def __init__(self):
        self.t = 0.0

    def now(self):
        return self.t

    def sleep(self, seconds):
        self.t += seconds


class Base(unittest.TestCase):
    def setUp(self):
        old = os.umask(0o077)
        self.addCleanup(os.umask, old)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name) / "home"
        self.home.mkdir(mode=0o700)
        controller = self.home / ".local/share/nacre/control/releases.py"
        controller.parent.mkdir(parents=True, exist_ok=True)
        controller.write_text("# fixture controller\n")
        self.state = self.home / ".local/state/nacre"
        self.state.mkdir(parents=True, mode=0o700)
        self.launcher = Path(self.temp.name) / "bead-shell"
        self.launcher.write_text("#!/bin/sh\n")
        self.launcher.chmod(0o700)
        self.wallpaper_video = self.launcher.with_name("bead-wallpaper-video")
        self.wallpaper_video.write_text('#!/bin/sh\n[ "${1:-}" = --check ] && exit 0\n')
        self.wallpaper_video.chmod(0o700)
        self.wallpaper_decode = self.launcher.with_name("nacre-wallpaper-decode")
        self.wallpaper_decode.write_text("#!/bin/sh\nexit 1\n")
        self.wallpaper_decode.chmod(0o700)
        self.manager = Manager(self.home, self.launcher)
        self.clock = Clock()

    def switch(self, **kw):
        return m.UiShell(
            self.home,
            self.state,
            runner=self.manager,
            environ={},
            sleep=self.clock.sleep,
            now=self.clock.now,
            **kw,
        )

    def bead(self, **kw):
        return self.switch(**kw).bead(str(self.launcher), "none")

    def daemon_bead(self, **kw):
        return self.switch(**kw).bead(str(self.launcher), "daemon")

    def assertBead(self):
        mgr = self.manager
        self.assertEqual((mgr.shell[0], mgr.shell[2]), ("bead", "active"))
        self.assertIsNone(mgr.windows)
        self.assertIsNotNone(mgr.wallpaper)
        self.assertEqual(mgr.violations, [])

    def statuses(self):
        return sorted(r["status"] for r in self.switch().records())


class SwitchTests(Base):
    def test_reinstall_replaces_a_damaged_launcher_with_a_checked_copy(self):
        switch = self.switch()
        assets = self.home / "build-shell"
        (assets / "bar").mkdir(parents=True)
        (assets / "palette.json").write_text("{}")
        first = switch.install(self.launcher, assets)
        Path(first["launcher"]).write_text("#!/bin/sh\nexit 42\n")
        repaired = switch.install(self.launcher, assets)
        self.assertNotEqual(first["launcher"], repaired["launcher"])
        self.assertFalse(switch.launcher_reasons(repaired["launcher"]))
        self.assertEqual(self.manager.mutations, 0)

    def build_shell(self, name="build-shell"):
        shell = self.home / name
        (shell / "bar").mkdir(parents=True)
        (shell / "bar/main.luau").write_text("return {}\n")
        (shell / "palette.json").write_text("{}\n")
        return shell

    def test_migration_retires_an_owned_legacy_windows_companion(self):
        self.bead()
        switch = self.switch()
        switch.windows.write_text(m.LEGACY_MARK + "[Service]\nExecStart=/old/windows\n")
        self.manager.cfg["windows"] = True
        self.manager.windows = 123
        self.assertEqual(switch.migrate()["shell"], "bead")
        self.assertFalse(switch.windows.exists())
        self.assertIsNone(self.manager.windows)
        self.assertIn(["systemctl", "--user", "stop", m.WINDOWS], self.manager.calls)
        self.assertBead()
        other = self.launcher.with_name("other")
        other.write_text("#!/bin/sh\n")
        other.chmod(0o700)
        switch.windows.write_text(m.LEGACY_MARK + "[Service]\nExecStart=/old/windows\n")
        self.manager.cfg["windows"] = True
        self.manager.windows = 124
        self.manager.launcher = str(other)
        switch.bead(str(other), "none", migration=True)
        self.assertFalse(switch.windows.exists())
        self.assertIsNone(self.manager.windows)
        self.assertBead()

    def test_legacy_migration_replaces_old_fallback_after_bead_health(self):
        fallback = self.manager.root / m.FALLBACK
        fallback.write_text(
            m.LEGACY_MARK
            + "[Unit]\nDescription=Return to the QML shell after a Bead failure\n"
            + "[Service]\nExecStart=/old/control/releases.py ui-shell qml --recover\n"
        )
        result = self.switch().migrate(self.launcher, self.build_shell())
        self.assertEqual(result["shell"], "bead")
        self.assertBead()
        fallback_text = fallback.read_text()
        self.assertIn("last known-good Bead", fallback_text)
        self.assertIn("recover-bead --recover", fallback_text)
        self.assertNotIn("qml --recover", fallback_text)
        self.assertEqual(self.switch().status()["active"], "bead")

    def test_package_prepared_units_activate_from_cached_legacy_manager(self):
        shell = self.build_shell()
        prepared = self.switch().prepare_bead(self.launcher, shell)
        self.assertEqual(prepared["status"], "prepared")
        self.assertFalse(
            any(c[:2] == ["systemctl", "--user"] for c in self.manager.calls)
        )
        self.assertEqual(self.switch().status()["active"], "prepared")
        result = self.switch().migrate(self.launcher, shell)
        self.assertEqual(result["shell"], "bead")
        self.assertBead()
        self.assertTrue(
            any(
                c[2] == "daemon-reload"
                for c in self.manager.calls
                if c[:2] == ["systemctl", "--user"]
            )
        )

    def test_first_activation_failure_stops_and_journals(self):
        self.manager.bead_crashes = True
        with self.assertRaisesRegex(RuntimeError, "No healthy Bead copy remains"):
            self.bead()
        self.assertEqual(self.manager.shell[2], "inactive")
        self.assertIn("No healthy Bead copy remains", self.manager.journal[-1])

    def test_no_good_bead_stops_and_journals_then_next_update_recovers(self):
        result = self.switch().recover_bead()
        self.assertEqual(result["status"], "stopped")
        self.assertEqual(self.manager.shell[2], "inactive")
        self.assertIn("No healthy Bead copy remains", self.manager.journal[-1])
        shell = self.build_shell("next-shell")
        self.manager.result = "success"
        migrated = self.switch().migrate(self.launcher, shell)
        self.assertEqual(migrated["shell"], "bead")
        self.assertBead()

    def test_same_build_migration_cuts_over_fallback_before_restart(self):
        self.bead()
        switch = self.switch()
        switch.fallback.write_text(
            m.LEGACY_MARK + "[Service]\nExecStart=/old/ui-shell qml --recover\n"
        )
        start_shell = self.manager.start_shell

        def start_checked():
            self.assertIn("recover-bead --recover", switch.fallback.read_text())
            self.assertNotIn("qml --recover", switch.fallback.read_text())
            start_shell()

        with patch.object(self.manager, "start_shell", side_effect=start_checked):
            switch.migrate()
        self.assertBead()

    def test_manual_rollback_without_previous_copy_preserves_running_shell(self):
        self.bead()
        mutations = self.manager.mutations
        with self.assertRaisesRegex(RuntimeError, "No previous known-good"):
            self.switch().recover_bead(exclude_current=True)
        self.assertEqual(self.manager.mutations, mutations)
        self.assertBead()

    def test_manual_rollback_restores_current_if_previous_copy_is_damaged(self):
        self.bead()
        other = self.launcher.with_name("current")
        other.write_text("#!/bin/sh\n")
        other.chmod(0o700)
        self.manager.launcher = str(other)
        self.switch().bead(str(other), "none")
        self.launcher.chmod(0o600)
        result = self.switch().recover_bead(exclude_current=True)
        self.assertEqual(result["launcher"], str(other))
        self.assertBead()

    def test_reloaded_candidate_is_not_mistaken_for_running_known_good(self):
        self.bead()
        original_pid = self.manager.shell[1]
        other = self.launcher.with_name("prepared")
        other.write_text("#!/bin/sh\n")
        other.chmod(0o700)
        self.manager.launcher = str(other)
        switch = self.switch()
        switch.dropin.write_bytes(m.dropin_text(str(other), "none"))
        switch.ctl("daemon-reload")
        self.assertEqual(self.manager.shell[1], original_pid)
        switch.bead(str(other), "none")
        self.assertNotEqual(self.manager.shell[1], original_pid)
        record = next(r for r in switch.records() if r.get("launcher") == str(other))
        self.assertTrue(record["known_good"])
        self.assertEqual(record["invocation"], f"{self.manager.shell[1]:032x}")

    def test_recovery_attempts_each_copy_once_and_rejects_all_its_records(self):
        self.bead()
        switch = self.switch()
        duplicate = switch.begin(
            "bead", launcher=str(self.launcher), notifications="none", daemon=False
        )
        duplicate.update(status="complete", known_good=True)
        switch.save(duplicate)
        self.manager.launcher_fails = True
        self.manager.calls.clear()
        result = self.switch().recover_bead()
        self.assertEqual(result["status"], "stopped")
        checks = [c for c in self.manager.calls if c == [str(self.launcher), "check"]]
        self.assertEqual(len(checks), 1)
        records = [r for r in self.switch().records() if r.get("operation") == "bead"]
        self.assertGreater(len(records), 1)
        self.assertTrue(all(r.get("recovery_failed") for r in records))

    def test_migration_reuses_previous_healthy_copy_without_fresh_build(self):
        self.bead()
        self.switch().dropin.unlink()
        self.switch().ctl("daemon-reload")
        self.switch().ctl("stop", m.SHELL)
        self.manager.start_shell()
        self.assertEqual(self.switch().migration_launcher(), str(self.launcher))
        self.switch().migrate()
        self.assertBead()

    def test_start_limit_recovers_previous_verified_copy(self):
        self.bead()
        previous = str(self.launcher)
        candidate = self.launcher.with_name("candidate")
        candidate.write_text("#!/bin/sh\n")
        candidate.chmod(0o700)
        self.manager.launcher = str(candidate)
        self.switch().bead(str(candidate), "none")
        self.manager.launcher = previous
        self.manager.result = "start-limit-hit"
        result = self.switch().recover_bead(
            exclude_current=True, mark_failed_current=True
        )
        self.assertEqual(
            (result["status"], result["launcher"]), ("recovered", previous)
        )
        failed = next(
            r for r in self.switch().records() if r.get("launcher") == str(candidate)
        )
        self.assertTrue(failed["recovery_failed"])
        self.assertBead()

    def test_failed_update_restores_bead_and_keeps_new_fallback(self):
        self.bead()
        previous = self.switch().active_launcher()
        fallback = self.manager.root / m.FALLBACK
        fallback.unlink()
        candidate = self.launcher.with_name("candidate")
        candidate.write_text("#!/bin/sh\n")
        candidate.chmod(0o700)
        self.manager.launcher = str(candidate)
        self.manager.fail["restart"] = 1
        with self.assertRaisesRegex(RuntimeError, "previous healthy Bead restored"):
            self.switch().bead(str(candidate), "none")
        self.assertEqual(self.switch().active_launcher(), previous)
        self.assertIn("recover-bead --recover", fallback.read_text())
        self.assertBead()

    def test_initial_activation_failure_never_restarts_legacy_shell(self):
        self.manager.bead_crashes = True
        with self.assertRaisesRegex(RuntimeError, "No healthy Bead copy remains"):
            self.switch().migrate(self.launcher, self.build_shell())
        self.assertEqual(self.manager.shell[0], "bead")
        self.assertEqual(self.manager.shell[2], "inactive")
        self.assertIn("No healthy Bead copy remains", self.manager.journal[-1])

    def test_identical_build_reuses_version_and_changed_build_is_retained(self):
        shell = self.build_shell()
        switch = self.switch()
        first = switch.install(self.launcher, shell)
        same = switch.install(self.launcher, shell)
        self.assertEqual(first["launcher"], same["launcher"])
        self.assertTrue(same["reused"])
        self.launcher.write_text("#!/bin/sh\n# candidate two\n")
        second = switch.install(self.launcher, shell)
        self.assertNotEqual(first["launcher"], second["launcher"])
        self.assertTrue(Path(first["launcher"]).is_file())
        self.assertEqual(switch.selected_launcher(), second["launcher"])

    def test_install_rejects_symlinked_shell_assets(self):
        shell = self.build_shell()
        external = self.home / "external"
        external.write_text("{}\n")
        (shell / "palette.json").unlink()
        (shell / "palette.json").symlink_to(external)
        with self.assertRaisesRegex(RuntimeError, "Shell link must target"):
            self.switch().install(self.launcher, shell)
        self.assertEqual(self.switch().records(), [])

    def test_foreign_fallback_is_never_overwritten(self):
        self.bead()
        fallback = self.manager.root / m.FALLBACK
        fallback.write_text("[Service]\nExecStart=/owner/recovery\n")
        before = fallback.read_bytes()
        with self.assertRaisesRegex(RuntimeError, "unmanaged fallback"):
            self.switch().migrate()
        self.assertEqual(fallback.read_bytes(), before)
        self.assertBead()

    def test_status_is_read_only_and_private_journal_records_activation(self):
        self.bead()
        mutations, files = (
            self.manager.mutations,
            sorted(map(str, self.home.rglob("*"))),
        )
        status = self.switch().status()
        self.assertEqual(status["active"], "bead")
        self.assertEqual(
            (self.manager.mutations, sorted(map(str, self.home.rglob("*")))),
            (mutations, files),
        )
        self.assertEqual(status["incomplete"], [])
        for path in (self.state / "ui-shell").rglob("journal.json"):
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def test_unknown_provider_and_start_limit_are_gated(self):
        self.manager.provider_after = 2
        self.bead()
        self.assertBead()
        self.manager.result = "signal"
        releases = self.state / "releases"
        with patch.object(m.f4, "require_ordinary_user"):
            ignored = m.dispatch(
                ["recover-bead", "--recover"],
                self.home,
                releases,
                runner=self.manager,
                environ={},
                sleep=self.clock.sleep,
                now=self.clock.now,
            )
        self.assertEqual(ignored["status"], "ignored")
