"""Private-HOME ui-shell tests with a modelled user manager; never touches a real one."""

import fcntl
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).parents[1]))
import f4_switch as f4  # noqa: E402
import ui_shell_switch as m  # noqa: E402

QML_EXEC = "{ path=/usr/bin/nacre-shell ; argv[]=/usr/bin/nacre-shell start ; ignore_errors=no ; }"


class Crash(BaseException):
    pass


def done(command, out="", code=0):
    return subprocess.CompletedProcess(command, code, out, "")


class Manager:
    """Models what matters: reload caches the unit files, restart applies the cache."""

    def __init__(self, home, launcher):
        self.root = home / ".config/systemd/user"
        self.root.mkdir(parents=True)
        self.launcher = str(launcher)
        self.cfg = {"bead": False, "windows": False}
        self.shell = ("qml", 1, "active")  # kind, pid, state
        self.windows = None
        self.pid = 1
        self.calls, self.violations = [], []
        self.fail, self.flap, self.after_mutation = {}, False, None
        self.bead_crashes, self.launcher_fails = False, False
        self.provider_after, self.foreign = 0, []
        self.mutations = 0
        self.topics = ["settings.store.desktop"]
        self.socket_active = True
        self.shell_active = True
        self.client_on = True

    def dropin_text(self):
        path = self.root / (m.SHELL + ".d") / m.DROPIN_NAME
        return path.read_text() if path.exists() else ""

    def start_shell(self):
        self.pid += 1
        kind = "bead" if self.cfg["bead"] else "qml"
        state = "failed" if kind == "bead" and self.bead_crashes else "active"
        self.shell = (kind, self.pid, state)
        if (
            kind == "bead"
            and self.cfg["windows"]
            and (self.root / (m.SHELL + ".d") / m.DROPIN_NAME).exists()
        ):  # ConditionPathExists
            self.pid += 1
            self.windows = self.pid

    def check(self):
        if self.shell and self.shell[0] == "qml" and self.windows:
            self.violations.append("qml shell beside windows host")

    def provider(self):
        if not self.shell or self.shell[2] != "active":
            return False
        if self.shell[0] == "bead":
            self.provider_after -= 1
            return self.provider_after < 0
        return True

    def __call__(self, command, **kwargs):
        self.calls.append(command)
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
                            "services": {"settings": {"status": "running"}},
                            "topics": self.topics,
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
            err = (
                {"code": "unknown_method", "dispatched": True}
                if self.provider()
                else {"code": "unavailable", "dispatched": False}
            )
            return done(command, json.dumps({"err": err}))
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
        elif action == "restart" and args[0] == m.SHELL:
            self.windows = None
            self.start_shell()
        elif action == "reset-failed":
            pass
        elif action == "stop" and args[0] == m.WINDOWS:
            self.windows = None
        else:
            raise AssertionError(command)
        self.check()
        self.mutations += 1
        if self.after_mutation:
            self.after_mutation(self.mutations)
        return done(command)

    def unit(self, name, names):
        if name == m.SHELL:
            kind, pid, state = self.shell
            if self.flap and kind == "bead":
                self.pid += 1
                pid = self.pid
            values = {
                "FragmentPath": "/usr/lib/systemd/user/" + name,
                "LoadState": "loaded",
                "ActiveState": state if self.shell_active else "inactive",
                "SubState": "running" if state == "active" else "failed",
                "MainPID": str(pid),
                "NRestarts": "0",
                "UnitFileState": "enabled",
                "Environment": "NACRE_DAEMON_CLIENT=1" if self.client_on else "",
                "ExecStart": QML_EXEC,
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
        self.state = self.home / ".local/state/nacre"
        self.state.mkdir(parents=True, mode=0o700)
        self.launcher = Path(self.temp.name) / "bead-shell"
        self.launcher.write_text("#!/bin/sh\n")
        self.launcher.chmod(0o700)
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

    def assertQml(self):
        mgr = self.manager
        self.assertEqual(
            (mgr.shell[0], mgr.shell[2], mgr.windows), ("qml", "active", None)
        )
        self.assertFalse(mgr.cfg["bead"])
        self.assertFalse((mgr.root / (m.SHELL + ".d") / m.DROPIN_NAME).exists())
        self.assertEqual(mgr.violations, [])

    def assertBead(self):
        mgr = self.manager
        self.assertEqual((mgr.shell[0], mgr.shell[2]), ("bead", "active"))
        self.assertIsNotNone(mgr.windows)
        self.assertEqual(mgr.violations, [])

    def statuses(self):
        return sorted(r["status"] for r in self.switch().records())


class SwitchTests(Base):
    def test_roundtrip_and_files(self):
        result = self.bead()
        self.assertEqual(result["shell"], "bead")
        self.assertBead()
        dropin = (self.manager.root / (m.SHELL + ".d") / m.DROPIN_NAME).read_text()
        self.assertIn(f"ExecStart=\nExecStart={self.launcher} start\n", dropin)
        self.assertIn(f"OnFailure={m.FALLBACK}", dropin)
        windows = (self.manager.root / m.WINDOWS).read_text()
        self.assertIn(
            f"ConditionPathExists={self.manager.root / (m.SHELL + '.d') / m.DROPIN_NAME}",
            windows,
        )
        self.assertIn("NACRE_QML_WINDOWS_ONLY=1", windows)
        self.assertIn("ExecStart=/usr/bin/nacre-shell start", windows)
        self.assertIn(
            "ui-shell qml --recover", (self.manager.root / m.FALLBACK).read_text()
        )
        self.assertEqual(self.switch().status(str(self.launcher))["active"], "bead")
        self.assertEqual(self.switch().qml()["shell"], "qml")
        self.assertQml()
        self.assertEqual(self.statuses(), ["complete", "rolled-back"])
        mutations = self.manager.mutations
        self.switch().qml()
        self.assertEqual(
            self.manager.mutations, mutations
        )  # already QML: nothing restarted
        self.bead()
        self.assertBead()

    def test_repeat_is_idempotent_and_different_dropin_refused(self):
        self.bead()
        mutations = self.manager.mutations
        self.bead()
        self.assertEqual(self.manager.mutations, mutations)
        other = self.launcher.with_name("other")
        other.write_text("#!/bin/sh\n")
        other.chmod(0o700)
        with self.assertRaisesRegex(RuntimeError, "different Bead drop-in"):
            self.switch().bead(str(other), "none")

    def test_refusals_write_nothing(self):
        mgr = self.manager
        cases = {
            "Daemon client mode": lambda: setattr(mgr, "client_on", False),
            "Choose --notifications": lambda: None,
            "Launcher check failed": lambda: setattr(mgr, "launcher_fails", True),
            "plain absolute path": lambda: None,
            "Foreign drop-in overrides ExecStart": lambda: mgr.foreign.append(
                str(self.foreign_dropin())
            ),
            "unmanaged file": lambda: (mgr.root / m.WINDOWS).write_text("[Service]\n"),
            "notifications topic": lambda: None,
            "not running": lambda: setattr(mgr, "shell_active", False),
            "socket is not active": lambda: setattr(mgr, "socket_active", False),
        }
        for reason, setup in cases.items():
            with self.subTest(reason):
                launcher, notifications = str(self.launcher), "none"
                if reason.startswith("Choose"):
                    notifications = None
                if reason == "plain absolute path":
                    launcher = "relative/bead-shell"
                if reason == "notifications topic":
                    notifications = "daemon"
                setup()
                before = self.manager.mutations
                with self.assertRaisesRegex(RuntimeError, reason):
                    self.switch().bead(launcher, notifications)
                self.assertEqual(self.manager.mutations, before)
                self.assertFalse(
                    (self.manager.root / (m.SHELL + ".d") / m.DROPIN_NAME).exists()
                )
                self.assertFalse((self.state / "ui-shell").exists())
                mgr.launcher_fails = False
                mgr.shell_active = mgr.socket_active = mgr.client_on = True
                mgr.foreign.clear()
                (mgr.root / m.WINDOWS).unlink(missing_ok=True)

    def foreign_dropin(self):
        path = self.manager.root / (m.SHELL + ".d") / "10-other.conf"
        path.parent.mkdir(exist_ok=True)
        path.write_text("[Service]\nExecStart=\nExecStart=/bin/true\n")
        return path

    def test_daemon_notifications_need_producer_topic(self):
        self.manager.topics.append("notifications")
        self.assertEqual(
            self.switch().bead(str(self.launcher), "daemon")["shell"], "bead"
        )
        self.assertEqual(
            self.switch().status(str(self.launcher))["notifications"], "daemon"
        )

    def test_health_failures_restore_qml(self):
        def crash_loop():
            self.manager.bead_crashes = True

        def flap():
            self.manager.flap = True

        def no_provider():
            self.manager.provider_after = 10**6

        def restart_fails():
            self.manager.fail["restart"] = 1

        def reload_fails():
            self.manager.fail["daemon-reload"] = 1

        for name, setup in {
            "crash": crash_loop,
            "flap": flap,
            "provider": no_provider,
            "restart": restart_fails,
            "reload": reload_fails,
        }.items():
            with self.subTest(name):
                setup()
                with self.assertRaisesRegex(
                    RuntimeError, "(QML shell was restored|Bead failed)"
                ):
                    self.bead()
                self.manager.bead_crashes = self.manager.flap = False
                self.manager.provider_after = 0
                self.manager.fail.clear()
                self.assertQml()
                self.assertNotIn("applying", self.statuses())
                self.assertTrue(all(s == "rolled-back" for s in self.statuses()))

    def test_provider_may_register_late(self):
        self.manager.provider_after = 3
        self.bead()
        self.assertBead()

    def test_crash_at_every_point_leaves_a_shell_and_qml_recovers(self):
        phases = [
            "write-units",
            "reload-units",
            "write-dropin",
            "reload",
            "restart-shell",
            "health",
        ]
        points = [("before", p) for p in phases] + [("after", n) for n in range(1, 4)]
        for kind, point in points:
            with self.subTest(kind=kind, point=point):
                self.setUp()
                hook = None
                if kind == "before":

                    def hook(name, point=point):
                        if name == point:
                            raise Crash
                else:

                    def mutation(n, point=point):
                        if n == point:
                            raise Crash

                    self.manager.after_mutation = mutation
                with self.assertRaises(Crash):
                    self.bead(phase_hook=hook)
                self.manager.after_mutation = None
                self.assertEqual(self.manager.violations, [])
                self.assertIn(
                    self.manager.shell[0], {"qml", "bead"}
                )  # never without a shell
                self.assertEqual(self.manager.shell[2], "active")
                with self.assertRaisesRegex(RuntimeError, "Incomplete"):
                    self.bead()
                self.switch().qml()
                self.assertQml()
                self.assertNotIn("applying", self.statuses())
                self.assertEqual(
                    self.statuses().count("complete"), 1
                )  # only the recovery record

    def test_crash_during_qml_phases_then_retry_converges(self):
        phases = ["remove-dropin", "reload", "stop-windows", "restart-shell", "health"]
        for point in phases:
            with self.subTest(point):
                self.setUp()
                self.bead()

                def hook(name, point=point):
                    if name == point:
                        raise Crash

                with self.assertRaises(Crash):
                    self.switch(phase_hook=hook).qml()
                self.assertEqual(self.manager.violations, [])
                self.assertIn(self.manager.shell[0], {"qml", "bead"})
                self.switch().qml()
                self.assertQml()

    def test_failed_bead_and_failed_rollback_leave_pending_journal_then_qml_recovers(
        self,
    ):
        self.manager.fail["daemon-reload"] = (
            2  # forward step and its first rollback attempt
        )
        with self.assertRaisesRegex(RuntimeError, "QML rollback incomplete"):
            self.bead()
        self.assertIn("applying", self.statuses())
        self.switch().qml()
        self.assertQml()
        self.assertNotIn("applying", self.statuses())

    def test_qml_failure_is_reported_and_retryable(self):
        self.bead()
        self.manager.fail["restart"] = 1
        with self.assertRaisesRegex(RuntimeError, "rerun nacre ui-shell qml"):
            self.switch().qml()
        self.switch().qml()
        self.assertQml()

    def test_unmanaged_dropin_is_never_removed(self):
        self.bead()
        path = self.manager.root / (m.SHELL + ".d") / m.DROPIN_NAME
        path.write_text("[Service]\nExecStart=/bin/true\n")
        with self.assertRaisesRegex(RuntimeError, "not managed"):
            self.switch().qml()
        self.assertTrue(path.exists())

    def test_status_is_read_only_and_journal_private(self):
        self.bead()
        mutations, files = (
            self.manager.mutations,
            sorted(map(str, self.home.rglob("*"))),
        )
        status = self.switch().status(str(self.launcher))
        self.assertEqual(
            (self.manager.mutations, sorted(map(str, self.home.rglob("*")))),
            (mutations, files),
        )
        self.assertEqual(status["incomplete"], [])
        index = self.state / "ui-shell"
        self.assertEqual(oct(index.stat().st_mode & 0o777), "0o700")
        for path in index.rglob("journal.json"):
            self.assertEqual(oct(path.stat().st_mode & 0o777), "0o600")

    def test_dispatch_lock_and_recover_busy(self):
        releases = self.state / "releases"
        releases.mkdir(mode=0o700)
        (releases / "release.lock").write_text("")
        (releases / "release.lock").chmod(0o600)
        with patch.dict(
            os.environ, {"XDG_STATE_HOME": str(self.home / ".local/state")}
        ):
            with open(releases / "release.lock") as held:
                fcntl.flock(held, fcntl.LOCK_EX)
                busy = m.dispatch(
                    ["qml", "--recover"],
                    self.home,
                    releases,
                    runner=self.manager,
                    environ={},
                )
                self.assertEqual(busy["status"], "busy")
            self.bead()
            result = m.dispatch(
                ["qml", "--recover"],
                self.home,
                releases,
                runner=self.manager,
                environ={},
                sleep=self.clock.sleep,
                now=self.clock.now,
            )
            self.assertEqual(result["shell"], "qml")
            self.assertQml()


if __name__ == "__main__":
    unittest.main()
