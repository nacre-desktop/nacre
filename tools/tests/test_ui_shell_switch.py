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


def done(command, out="", code=0, err=""):
    return subprocess.CompletedProcess(command, code, out, err)


class Manager:
    """Models what matters: reload caches the unit files, restart applies the cache."""

    def __init__(self, home, launcher):
        self.root = home / ".config/systemd/user"
        self.root.mkdir(parents=True)
        self.launcher = str(launcher)
        self.cfg = {"bead": False, "windows": False}
        self.shell = ("qml", 1, "active")  # kind, pid, state
        self.windows = None
        self.qml_wallpaper_helpers = []
        self.wallpaper = None  # supervised child in the shell's control group
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
        self.daemon_down = False
        self.result = "success"  # the shell unit's Result property
        # Daemon model (N0068): reload caches the drop-in, restart applies it.
        self.daemon_cfg = self.daemon_mode = False
        self.notifications_broken = False
        self.daemon_foreign = []

    def daemon_dropin_path(self):
        return self.root / (m.DAEMON + ".d") / m.DAEMON_DROPIN_NAME

    def notifications_running(self):
        """The managed daemon takes the bus name only once the QML shell has released it."""
        qml_up = (
            self.shell[0] == "qml" and self.shell[2] == "active" and self.shell_active
        )
        return self.daemon_mode and not self.notifications_broken and not qml_up

    def dropin_text(self):
        path = self.root / (m.SHELL + ".d") / m.DROPIN_NAME
        return path.read_text() if path.exists() else ""

    def start_shell(self):
        self.pid += 1
        kind = "bead" if self.cfg["bead"] else "qml"
        state = "failed" if kind == "bead" and self.bead_crashes else "active"
        self.shell = (kind, self.pid, state)
        if kind == "qml" and self.daemon_mode:
            self.violations.append("qml shell started beside the managed daemon")
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
        if self.shell and self.shell[0] == "qml" and self.windows:
            self.violations.append("qml shell beside windows host")
        if self.shell and self.shell[0] == "qml" and self.wallpaper:
            self.violations.append("qml shell beside Bead wallpaper child")

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
                and self.shell[0] == "qml"
                and self.shell[2] == "active"
            ):
                self.violations.append("daemon restarted while QML held the name")
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
            self.qml_wallpaper_helpers = []
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
                "Result": self.result,
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

    def assertQml(self):
        mgr = self.manager
        self.assertFalse(mgr.daemon_mode)
        self.assertFalse(mgr.daemon_dropin_path().exists())
        self.assertEqual(
            (mgr.shell[0], mgr.shell[2], mgr.windows), ("qml", "active", None)
        )
        self.assertIsNone(mgr.wallpaper)
        self.assertFalse(mgr.cfg["bead"])
        self.assertFalse((mgr.root / (m.SHELL + ".d") / m.DROPIN_NAME).exists())
        self.assertEqual(mgr.violations, [])

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
    def test_roundtrip_and_files(self):
        result = self.bead()
        self.assertEqual(result["shell"], "bead")
        self.assertBead()
        dropin = (self.manager.root / (m.SHELL + ".d") / m.DROPIN_NAME).read_text()
        self.assertIn(f"ExecStart=\nExecStart={self.launcher} start\n", dropin)
        self.assertIn(f"OnFailure={m.FALLBACK}", dropin)
        self.assertIn("KillMode=control-group", dropin)
        self.assertIn(f"Wants={f4.SOCKET} {m.DAEMON}\n", dropin)
        self.assertNotIn(m.WINDOWS, dropin)
        self.assertFalse((self.manager.root / m.WINDOWS).exists())
        self.assertIn(f"After={f4.SOCKET} {m.DAEMON}\n", dropin)
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

    def test_qml_rollback_stops_legacy_companion_before_bead_switch(self):
        switch = self.switch()
        switch.dropin.parent.mkdir(parents=True)
        switch.dropin.write_text(m.MARK + f"[Unit]\nWants={m.WINDOWS} {f4.SOCKET}\n")
        switch.windows.write_text(m.MARK + "[Unit]\nDescription=legacy QML companion\n")
        self.manager.cfg["bead"] = True
        self.manager.cfg["windows"] = True
        self.manager.shell = ("bead", 2, "active")
        self.manager.windows = 2
        self.manager.wallpaper = 3
        self.manager.qml_wallpaper_helpers = [3, 4, 5]

        switch.qml()

        self.assertQml()
        self.assertEqual(self.manager.qml_wallpaper_helpers, [])
        self.assertFalse(switch.windows.exists())
        self.bead()

        self.assertIsNone(self.manager.windows)
        self.assertEqual(self.manager.qml_wallpaper_helpers, [])
        self.assertFalse(switch.windows.exists())
        self.assertBead()

    def test_rollback_restores_prior_companion_bytes_and_modes(self):
        switch = self.switch()
        for path in (switch.windows, switch.fallback):
            path.write_text(m.MARK + "[Unit]\nDescription=previous\n")
            path.chmod(0o600)
        before = {p: f4.file_state(p) for p in switch.managed}
        self.bead()
        switch.qml()
        self.assertEqual({p: f4.file_state(p) for p in switch.managed}, before)
        self.assertQml()

    def test_install_retains_previous_and_selects_only_completed_build(self):
        switch = self.switch()
        assets = self.home / "build-shell"
        (assets / "bar").mkdir(parents=True)
        (assets / "palette.json").write_text("{}")
        first = switch.install(self.launcher, assets)
        second = switch.install(self.launcher, assets)
        self.assertNotEqual(first["launcher"], second["launcher"])
        self.assertTrue(Path(first["launcher"]).is_file())
        self.assertEqual(switch.selected_launcher(), second["launcher"])
        text = Path(second["launcher"]).read_text()
        self.assertIn("--core-views --ui-provider", text)
        self.assertNotIn("--seconds", text)
        self.assertIn("./bead-wallpaper-video --check", text)
        self.assertIn("-x ./nacre-wallpaper-decode", text)
        self.assertIn("--daemon-socket", text)
        self.assertTrue(Path(second["launcher"]).with_name("shell").is_dir())
        self.assertEqual(
            {p.name for p in Path(second["launcher"]).parent.iterdir()}
            & {"bead-wallpaper-video", "nacre-wallpaper-decode"},
            {"bead-wallpaper-video", "nacre-wallpaper-decode"},
        )
        for name in ("bead-wallpaper-video", "nacre-wallpaper-decode"):
            installed = Path(second["launcher"]).with_name(name)
            self.assertTrue(installed.is_file())
            self.assertTrue(os.access(installed, os.X_OK))
        self.assertEqual(self.manager.mutations, 0)

    def test_install_interruption_leaves_previous_selection(self):
        assets = self.home / "build-shell"
        (assets / "bar").mkdir(parents=True)
        (assets / "palette.json").write_text("{}")
        first = self.switch().install(self.launcher, assets)
        for phase in ("copy-build", "check-build"):

            def hook(name, phase=phase):
                if name == phase:
                    raise Crash

            with self.assertRaises(Crash):
                self.switch(phase_hook=hook).install(self.launcher, assets)
            self.assertEqual(self.switch().selected_launcher(), first["launcher"])
            self.switch().qml()
            self.assertEqual(self.switch().selected_launcher(), first["launcher"])

    def test_install_rejects_symlink_assets(self):
        assets = self.home / "build-shell"
        (assets / "bar").mkdir(parents=True)
        (assets / "palette.json").symlink_to(self.launcher)
        with self.assertRaises(RuntimeError):
            self.switch().install(self.launcher, assets)
        self.assertEqual(self.switch().records(), [])

    def test_install_requires_safe_executable_wallpaper_helpers(self):
        assets = self.home / "build-shell"
        (assets / "bar").mkdir(parents=True)
        (assets / "palette.json").write_text("{}")
        video = self.launcher.with_name("bead-wallpaper-video")
        decoder = self.launcher.with_name("nacre-wallpaper-decode")
        for path in (video, decoder):
            original = path.read_bytes()
            path.unlink()
            with self.subTest(missing=path.name):
                with self.assertRaisesRegex(RuntimeError, "Unsafe source file"):
                    self.switch().install(self.launcher, assets)
                self.assertEqual(self.switch().records(), [])
            path.write_bytes(original)
            path.chmod(0o700)
        decoder.chmod(0o600)
        with self.assertRaisesRegex(RuntimeError, "executable"):
            self.switch().install(self.launcher, assets)
        self.assertEqual(self.switch().records(), [])
        decoder.chmod(0o700)
        video.unlink()
        video.symlink_to(decoder)
        with self.assertRaisesRegex(RuntimeError, "Unsafe source file"):
            self.switch().install(self.launcher, assets)
        self.assertEqual(self.switch().records(), [])

    def test_failed_wallpaper_helper_check_keeps_previous_install_selected(self):
        assets = self.home / "build-shell"
        (assets / "bar").mkdir(parents=True)
        (assets / "palette.json").write_text("{}")
        first = self.switch().install(self.launcher, assets)
        self.wallpaper_video.write_text("#!/bin/sh\nexit 1\n")
        with self.assertRaisesRegex(RuntimeError, "Launcher check failed"):
            self.switch().install(self.launcher, assets)
        self.assertEqual(self.switch().selected_launcher(), first["launcher"])
        self.assertEqual(self.switch().records()[-1]["status"], "rolled-back")

    def test_source_hardlinks_become_independent_installed_assets(self):
        assets = self.home / "build-shell"
        (assets / "bar").mkdir(parents=True)
        palette = assets / "palette.json"
        palette.write_text("{}")
        (assets / "duplicate.json").hardlink_to(palette)
        (assets / "alias.json").symlink_to("palette.json")
        result = self.switch().install(self.launcher, assets)
        installed = Path(result["launcher"]).parent / "shell/palette.json"
        self.assertEqual(installed.stat().st_nlink, 1)
        self.assertFalse(installed.with_name("alias.json").is_symlink())
        palette.write_text("changed")
        self.assertEqual(installed.read_text(), "{}")

    def test_source_launcher_passes_notification_choice(self):
        assets = self.home / "build-shell"
        (assets / "bar").mkdir(parents=True)
        (assets / "palette.json").write_text("{}")
        self.launcher.write_text(
            '#!/bin/sh\n[ "$1" = --help ] && exit 0\nprintf \'%s\\n\' "$@" > "$XDG_RUNTIME_DIR/arguments"\n'
        )
        result = self.switch().install(self.launcher, assets)
        for choice in ("none", "daemon"):
            subprocess.run(
                [result["launcher"], "start"],
                check=True,
                env={
                    "XDG_RUNTIME_DIR": str(self.home),
                    "NACRE_UI_NOTIFICATIONS": choice,
                },
            )
            args = (self.home / "arguments").read_text().splitlines()
            self.assertEqual("--notices" in args, choice == "daemon")
            self.assertIn("--ui-provider", args)
            self.assertIn(str(self.home / "nacre/daemon.sock"), args)

    def test_recovery_after_each_completed_phase_restores_exact_files(self):
        forward = [
            "write-units",
            "reload-units",
            "stop-windows",
            "remove-windows",
            "reload-without-windows",
            "write-dropin",
            "reload",
            "restart-shell",
            "health",
        ]
        backward = [
            "remove-dropin",
            "reload",
            "stop-windows",
            "restore-companions",
            "reload-restored",
            "restart-shell",
            "health",
        ]
        for operation, phases in (("bead", forward), ("qml", backward)):
            for point in phases:
                with self.subTest(operation=operation, phase=point):
                    self.setUp()
                    switch = self.switch()
                    before = {p: f4.file_state(p) for p in switch.managed}
                    if operation == "qml":
                        self.bead()
                    phase = switch.phase

                    def interrupt(record, name, action, point=point, phase=phase):
                        phase(record, name, action)
                        if name == point:
                            raise Crash

                    switch.phase = interrupt
                    with self.assertRaises(Crash):
                        if operation == "bead":
                            switch.bead(str(self.launcher), "none")
                        else:
                            switch.qml()
                    self.switch().qml()
                    self.assertQml()
                    self.assertEqual(
                        {p: f4.file_state(p) for p in switch.managed}, before
                    )
                    self.assertNotIn("applying", self.statuses())

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

    def test_daemon_mode_sequences_the_name_handover_and_rolls_back_exactly(self):
        mgr = self.manager
        before = {p: f4.file_state(p) for p in self.switch().managed}
        self.assertEqual(self.daemon_bead()["shell"], "bead")
        self.assertBead()
        self.assertTrue(mgr.daemon_mode and mgr.notifications_running())
        text = mgr.daemon_dropin_path().read_text()
        self.assertIn(
            "ExecStart=\nExecStart=/usr/bin/nacre-daemon --notifications=daemon --behaviours=daemon\n",
            text,
        )
        self.assertEqual(
            self.switch().status(str(self.launcher))["notifications"], "daemon"
        )
        order = [
            c[2:]
            for c in mgr.calls
            if c[:2] == ["systemctl", "--user"] and c[2] in {"stop", "restart"}
        ]
        self.assertEqual(
            order, [["stop", m.SHELL], ["restart", m.DAEMON], ["restart", m.SHELL]]
        )
        self.assertEqual(self.daemon_bead()["shell"], "bead")  # repeat is idempotent
        self.switch().qml()
        self.assertQml()
        self.assertEqual({p: f4.file_state(p) for p in self.switch().managed}, before)
        self.assertEqual(mgr.violations, [])

    def test_daemon_mode_refuses_a_foreign_daemon_exec_override(self):
        self.manager.daemon_foreign.append(str(self.manager.root / "other-daemon.conf"))
        (self.manager.root / "other-daemon.conf").write_text(
            "[Service]\nExecStart=\nExecStart=/bin/true\n"
        )
        with self.assertRaisesRegex(
            RuntimeError, "Foreign drop-in overrides the daemon"
        ):
            self.daemon_bead()
        self.assertEqual(self.manager.mutations, 0)
        self.assertFalse(self.manager.daemon_dropin_path().exists())

    def test_daemon_notifications_that_never_appear_restore_qml_and_the_legacy_daemon(
        self,
    ):
        self.manager.notifications_broken = True
        with self.assertRaisesRegex(
            RuntimeError, "notifications did not become available"
        ):
            self.daemon_bead()
        self.assertQml()
        self.assertTrue(all(s == "rolled-back" for s in self.statuses()))

    def test_crash_at_every_daemon_phase_converges_to_qml(self):
        phases = [
            "write-units",
            "reload-units",
            "stop-windows",
            "remove-windows",
            "reload-without-windows",
            "write-daemon-dropin",
            "write-dropin",
            "reload",
            "stop-shell",
            "restart-daemon",
            "daemon-ready",
            "restart-shell",
            "health",
        ]
        for point in phases:
            with self.subTest(point=point):
                self.setUp()

                def hook(name, point=point):
                    if name == point:
                        raise Crash

                with self.assertRaises(Crash):
                    self.daemon_bead(phase_hook=hook)
                self.switch().qml()
                self.assertQml()
                self.assertEqual(self.manager.violations, [])
                self.assertNotIn("applying", self.statuses())

    def test_crash_at_every_daemon_rollback_phase_converges(self):
        phases = [
            "remove-dropin",
            "reload",
            "stop-windows",
            "restore-companions",
            "reload-restored",
            "restart-daemon",
            "restart-shell",
            "daemon-ready",
            "health",
        ]
        for point in phases:
            with self.subTest(point=point):
                self.setUp()
                self.daemon_bead()

                def hook(name, point=point):
                    if name == point:
                        raise Crash

                with self.assertRaises(Crash):
                    self.switch(phase_hook=hook).qml()
                self.switch().qml()
                self.assertQml()
                self.assertEqual(self.manager.violations, [])

    def test_rollback_still_restarts_the_shell_when_the_daemon_restart_fails(self):
        self.daemon_bead()
        self.manager.fail["restart"] = 1  # the daemon restart is the first restart
        self.switch().qml()
        self.assertEqual(self.manager.shell[0], "qml")

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

    def test_probe_tells_a_live_provider_from_none_or_a_down_daemon(self):
        # The three outputs of the real `nacrectl call ui.dispatch` probe, measured under systemd (B0033).
        switch = self.switch()
        switch.layout(switch.snapshot()[0])
        binding = switch.binding_env()
        self.assertTrue(switch.provider_ready(binding))
        self.manager.shell = ("qml", 1, "inactive")  # nothing registered
        self.assertFalse(switch.provider_ready(binding))
        self.manager.shell = ("qml", 1, "active")
        self.manager.daemon_down = True
        self.assertFalse(switch.provider_ready(binding))

    def test_provider_may_register_late(self):
        self.manager.provider_after = 3
        self.bead()
        self.assertBead()

    def test_crash_at_every_point_leaves_a_shell_and_qml_recovers(self):
        phases = [
            "write-units",
            "reload-units",
            "stop-windows",
            "remove-windows",
            "reload-without-windows",
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
        phases = [
            "remove-dropin",
            "reload",
            "stop-windows",
            "restore-companions",
            "reload-restored",
            "restart-shell",
            "health",
        ]
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

    def test_interrupted_terminal_journal_cleanup_converges(self):
        self.bead()
        switch = self.switch()
        save = switch.save

        def interrupt(record):
            save(record)
            if record["operation"] == "qml" and record["status"] == "complete":
                raise Crash

        switch.save = interrupt
        with self.assertRaises(Crash):
            switch.qml()
        self.assertQml()
        self.switch().qml()
        self.assertTrue(
            all(
                r["status"] == "rolled-back"
                for r in self.switch().records()
                if r["operation"] == "bead"
            )
        )
        self.assertIsNone(self.switch().status(str(self.launcher))["notifications"])

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
        with (
            patch.object(f4, "require_ordinary_user"),  # CI runs as root
            patch.dict(os.environ, {"XDG_STATE_HOME": str(self.home / ".local/state")}),
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
            # systemd also runs OnFailure= on a crash that it restarts: not exhausted, nothing happens
            self.manager.result = "signal"
            ignored = m.dispatch(
                ["qml", "--recover"],
                self.home,
                releases,
                runner=self.manager,
                environ={},
            )
            self.assertEqual(ignored["status"], "ignored")
            self.assertBead()
            self.manager.result = "start-limit-hit"
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
