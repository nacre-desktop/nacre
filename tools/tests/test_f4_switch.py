"""Private-HOME switch tests; never connect to a real user manager."""

import importlib.util
import json
import os
import stat
import subprocess
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "f4_switch", Path(__file__).parents[1] / "f4_switch.py"
)
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)


class Crash(BaseException):
    pass


class Manager:
    def __init__(self, home):
        self.home = home
        self.root = home / ".config/systemd/user"
        self.root.mkdir(parents=True)
        self.manager = {"HOME": str(home), "XDG_RUNTIME_DIR": str(home / "run")}
        self.units = {
            u: {
                "FragmentPath": "/usr/lib/systemd/user/" + u,
                "LoadState": "loaded",
                "ActiveState": "inactive",
                "UnitFileState": "disabled",
                "Environment": "",
                "DropInPaths": "",
            }
            for u in m.UNITS
        }
        self.units[m.SHELL]["ActiveState"] = "active"
        self.units[m.SHELL]["UnitFileState"] = "enabled"
        self.units[m.DAEMON]["UnitFileState"] = "static"
        self.units[m.SOCKET]["RequiredBy"] = m.DAEMON
        self.loaded_dropins = []
        self.loaded_environment = ""
        self.extra_roots = []
        self.calls = []
        self.failure = None
        self.bad_health = False
        self.after_mutation = None

    def __call__(self, command, **kwargs):
        self.calls.append(command)
        if command[0] == "/usr/bin/nacrectl" or command[0].endswith(
            "/tools/nacre-helper"
        ):
            if command[0].endswith("/tools/nacre-helper"):
                assert command[1] == "ctl"
            if self.bad_health:
                raise RuntimeError("test protocol failure")
            method = command[-2]
            value = {
                "boot": "daemon-boot",
                "safe": False,
                "services": {"settings": {"status": "running"}},
                "topics": ["settings.store.desktop"],
            }
            if method == "settings.get":
                value = {
                    "store": "desktop",
                    "rev": 4,
                    "exists": True,
                    "data": {},
                    "error": None,
                    "dirty": False,
                }
            return subprocess.CompletedProcess(command, 0, json.dumps(value))
        assert command[:2] == ["systemctl", "--user"]
        action, *args = command[2:]
        if action == "show-environment":
            output = "\n".join(k + "=" + v for k, v in self.manager.items())
        elif action == "show":
            if args[0].startswith("--property="):
                output = "UnitPath=" + " ".join([str(self.root), *self.extra_roots])
            else:
                values = dict(self.units[args[0]])
                if args[0] == m.SHELL:
                    values["DropInPaths"] = " ".join(self.loaded_dropins)
                    values["Environment"] = (
                        self.loaded_environment or values["Environment"]
                    )
                output = "\n".join(k + "=" + v for k, v in values.items())
        else:
            if self.failure == action:
                self.failure = None
                raise RuntimeError("test systemctl failure: " + action)
            if action == "daemon-reload":
                paths = sorted((self.root / (m.SHELL + ".d")).glob("*.conf"))
                self.loaded_dropins = list(map(str, paths))
                self.loaded_environment = ""
                for path in paths:
                    for line in path.read_text().splitlines():
                        if line.startswith("Environment="):
                            self.loaded_environment = line.removeprefix("Environment=")
            else:
                unit = args[-1]
                values = self.units[unit]
                if action in {"enable", "disable"}:
                    enabled = action == "enable"
                    values["UnitFileState"] = "enabled" if enabled else "disabled"
                    link = self.root / "graphical-session.target.wants" / unit
                    if enabled:
                        link.parent.mkdir(exist_ok=True)
                        if not link.is_symlink():
                            link.symlink_to(values["FragmentPath"])
                    elif link.is_symlink():
                        link.unlink()
                    if "--now" in args:
                        values["ActiveState"] = "active" if enabled else "inactive"
                else:
                    values["ActiveState"] = "inactive" if action == "stop" else "active"
                    if unit == m.SHELL and action in {"start", "restart"}:
                        env = dict(self.manager)
                        env.update(
                            m.environment(
                                self(["systemctl", "--user", "show", m.SHELL])
                                .stdout.split("Environment=", 1)[1]
                                .splitlines()[0]
                            )
                        )
                        if (
                            env.get(m.KEY) == "1"
                            and self.units[m.SOCKET]["ActiveState"] == "active"
                        ):
                            self.units[m.DAEMON]["ActiveState"] = "active"
            if self.after_mutation:
                self.after_mutation(action)
            output = ""
        return subprocess.CompletedProcess(command, 0, output)

    def mutations(self):
        return [
            c
            for c in self.calls
            if c[0] == "systemctl" and c[2] not in {"show", "show-environment"}
        ]


class SwitchTests(unittest.TestCase):
    def setUp(self):
        old_umask = os.umask(0o077)
        self.addCleanup(os.umask, old_umask)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.home = Path(self.temp.name) / "home"
        self.home.mkdir(mode=0o700)
        self.state = self.home / ".local/state/nacre"
        self.state.mkdir(parents=True, mode=0o700)
        self.manager = Manager(self.home)
        self.switch = m.Switch(self.home, self.state, runner=self.manager, environ={})

    def prior(self):
        return self.switch.snapshot(), m.file_state(self.switch.dropin)

    def assertRestored(self, before):
        self.assertEqual(self.prior(), before)

    def source_layout(self):
        receipt = {}
        for unit in m.UNITS:
            path = self.manager.root / unit
            path.write_text("[Unit]\nDescription=Private fixture " + unit + "\n")
            receipt[str(path.relative_to(self.home))] = m.digest(path)
            self.manager.units[unit]["FragmentPath"] = str(path)
        (self.state / "configuration.json").write_text(json.dumps(receipt))
        helper = self.home / ".local/share/nacre/shell/tools/nacre-helper"
        helper.parent.mkdir(parents=True)
        helper.write_text("private fixture helper")
        helper.chmod(0o700)
        return helper

    def test_source_enable_disable_rollback_with_foreign_dropins(self):
        helper = self.source_layout()
        directory = self.manager.root / (m.SHELL + ".d")
        directory.mkdir()
        for name in ("niri-acceptance.conf", "niri-runtime.conf"):
            (directory / name).write_text("[Service]\nRestartSec=3\n")
        self.manager(["systemctl", "--user", "daemon-reload"])
        before = self.prior()
        enabled = self.switch.run("enable")
        link = self.manager.root / "graphical-session.target.wants" / m.SOCKET
        self.assertEqual(os.readlink(link), str(self.manager.root / m.SOCKET))
        self.assertTrue(
            any(c[:3] == [str(helper), "ctl", "call"] for c in self.manager.calls)
        )
        self.assertEqual(self.switch.run("status")["ready_to_enable"], [])
        self.switch.run("disable")
        self.switch.run("rollback")
        self.switch.run("rollback", enabled["transaction"])
        self.assertRestored(before)

    def test_source_interrupted_enable_and_health_failure_restore(self):
        self.source_layout()
        before = self.prior()

        def crash(name):
            if name == "socket":
                raise Crash(name)

        self.switch.phase_hook = crash
        with self.assertRaises(Crash):
            self.switch.run("enable")
        self.switch.phase_hook = None
        self.switch.run("rollback")
        self.assertRestored(before)
        self.manager.bad_health = True
        with self.assertRaisesRegex(RuntimeError, "prior state restored"):
            self.switch.run("enable")
        self.assertRestored(before)

    def test_source_rejects_socket_link_to_vendor_fragment(self):
        self.source_layout()
        self.switch.run("enable")
        link = self.manager.root / "graphical-session.target.wants" / m.SOCKET
        link.unlink()
        link.symlink_to("/usr/lib/systemd/user/" + m.SOCKET)
        self.manager.calls.clear()
        with self.assertRaisesRegex(RuntimeError, "Independent daemon activation"):
            self.switch.run("disable")
        self.assertEqual(self.manager.mutations(), [])

    def test_dangling_startup_links_are_ignored_except_daemon(self):
        directory = self.home / ".local/share/dbus-1/services"
        directory.mkdir(parents=True)
        link = directory / "org.freedesktop.secrets.service"
        link.symlink_to(self.home / "missing/ai/dbus")
        self.assertEqual(self.switch.run("status")["ready_to_enable"], [])
        self.switch.run("enable")
        for name, target in (
            ("nacre-daemon-alias.service", "missing"),
            ("alias.service", "missing/nacre-daemon"),
        ):
            with self.subTest(name=name):
                other = directory / name
                other.symlink_to(target)
                self.manager.calls.clear()
                with self.assertRaisesRegex(
                    RuntimeError, "Independent daemon activation"
                ):
                    self.switch.run("disable")
                self.assertEqual(self.manager.mutations(), [])
                other.unlink()

    def test_source_refusals_before_write(self):
        for case in (
            "edited",
            "unreceipted",
            "symlink",
            "mixed",
            "missing-helper",
            "symlink-helper",
            "owner",
        ):
            with self.subTest(case=case):
                self.setUp()
                helper = self.source_layout()
                path = self.manager.root / m.DAEMON
                if case == "edited":
                    path.write_text("owner edit")
                elif case == "unreceipted":
                    (self.state / "configuration.json").write_text("{}")
                elif case == "symlink":
                    body = path.read_bytes()
                    path.unlink()
                    target = self.home / "alias"
                    target.write_bytes(body)
                    path.symlink_to(target)
                elif case == "mixed":
                    path.unlink()
                    self.manager.units[m.DAEMON]["FragmentPath"] = (
                        "/usr/lib/systemd/user/" + m.DAEMON
                    )
                elif case == "missing-helper":
                    helper.unlink()
                elif case == "symlink-helper":
                    helper.unlink()
                    helper.symlink_to(self.home / "missing")
                else:
                    self.manager.units[m.DAEMON]["WantedBy"] = "other.target"
                status = self.switch.run("status")
                self.assertTrue(status["ready_to_enable"])
                with self.assertRaises((RuntimeError, OSError)):
                    self.switch.run("enable")
                self.assertFalse(self.switch.index.exists())
                self.assertEqual(self.manager.mutations(), [])

    def test_missing_source_daemon_units_report_setup_steps(self):
        self.source_layout()
        for unit in (m.DAEMON, m.SOCKET):
            (self.manager.root / unit).unlink()
            self.manager.units[unit].update(FragmentPath="", LoadState="not-found")
        status = self.switch.run("status")
        for unit in (m.DAEMON, m.SOCKET):
            self.assertTrue(
                any(
                    unit in reason and "configure.py --only" in reason
                    for reason in status["ready_to_enable"]
                )
            )
        with self.assertRaisesRegex(RuntimeError, "Missing deployed unit"):
            self.switch.run("enable")
        self.assertFalse(self.switch.index.exists())
        self.assertEqual(self.manager.mutations(), [])

    def test_enable_disable_rollback_and_repeat(self):
        before = self.prior()
        enabled = self.switch.run("enable")
        self.assertEqual(self.switch.run("enable"), enabled)
        self.assertEqual(self.switch.dropin.read_bytes(), m.CONTENTS["1"])
        disabled = self.switch.run("disable")
        self.assertEqual(self.switch.run("disable"), disabled)
        self.assertEqual(self.switch.dropin.read_bytes(), m.CONTENTS["0"])
        self.switch.run("rollback", disabled["transaction"])
        self.assertEqual(self.switch.dropin.read_bytes(), m.CONTENTS["1"])
        self.switch.run("rollback", enabled["transaction"])
        self.assertRestored(before)
        self.switch.run("rollback", enabled["transaction"])
        self.assertRestored(before)

    def test_manager_one_is_overridden_by_disable_zero(self):
        self.manager.manager[m.KEY] = "1"
        self.switch.run("disable")
        self.assertEqual(self.switch.snapshot()[2][m.KEY], "0")
        self.assertEqual(self.manager.manager[m.KEY], "1")
        self.switch.run("rollback")
        self.assertFalse(self.switch.dropin.exists())

    def test_health_failure_restores_exact_prior(self):
        self.switch.run("disable")
        # A managed prior drop-in may have a private owner-selected mode.
        self.switch.dropin.chmod(0o640)
        record = self.switch.records()[-1]
        record["desired"]["mode"] = 0o640
        self.switch.save(record)
        before = self.prior()
        self.manager.bad_health = True
        with self.assertRaisesRegex(RuntimeError, "prior state restored"):
            self.switch.run("enable")
        self.assertRestored(before)

    def test_systemctl_failure_restores(self):
        before = self.prior()
        self.manager.failure = "restart"
        with self.assertRaisesRegex(RuntimeError, "prior state restored"):
            self.switch.run("enable")
        self.assertRestored(before)

    def test_refusals_before_writes(self):
        cases = [
            "shadow",
            "foreign",
            "manager",
            "owner",
            "link",
            "load-path",
            "occupied",
            "symlink",
            "environment-file",
        ]
        for case in cases:
            with self.subTest(case=case):
                with tempfile.TemporaryDirectory() as folder:
                    home = Path(folder) / "home"
                    home.mkdir()
                    manager = Manager(home)
                    switch = m.Switch(home, home / "state", runner=manager, environ={})
                    if case == "shadow":
                        (manager.root / m.SHELL).write_text("shadow")
                    elif case == "foreign":
                        drop = manager.root / (m.SHELL + ".d/99-owner.conf")
                        drop.parent.mkdir()
                        drop.write_text(
                            "[Service]\nEnvironment=NACRE_DAEMON_CLIENT=1\n"
                        )
                    elif case == "manager":
                        manager.manager[m.KEY] = "maybe"
                    elif case == "owner":
                        manager.units[m.DAEMON]["WantedBy"] = "other.target"
                    elif case == "link":
                        link = manager.root / "other.target.wants/alias.service"
                        link.parent.mkdir()
                        link.symlink_to("/usr/lib/systemd/user/" + m.DAEMON)
                    elif case == "load-path":
                        switch.config = home / "different"
                    elif case == "occupied":
                        switch.dropin.parent.mkdir()
                        switch.dropin.write_bytes(m.CONTENTS["1"])
                    elif case == "symlink":
                        switch.dropin.parent.mkdir()
                        switch.dropin.symlink_to(home / "foreign")
                    else:
                        manager.units[m.SHELL]["EnvironmentFiles"] = "/some/environment"
                    with self.assertRaises(RuntimeError):
                        switch.run("enable")
                    self.assertFalse(switch.state.exists())
                    self.assertEqual(manager.mutations(), [])

    def test_disable_refuses_independent_baseline(self):
        for unit in (m.DAEMON, m.SOCKET):
            for prop, value in (
                ("ActiveState", "active"),
                ("UnitFileState", "enabled"),
            ):
                with self.subTest(unit=unit, prop=prop):
                    old = self.manager.units[unit][prop]
                    self.manager.units[unit][prop] = value
                    with self.assertRaisesRegex(RuntimeError, "Independent baseline"):
                        self.switch.run("disable")
                    self.assertFalse(self.switch.index.exists())
                    self.assertEqual(self.manager.mutations(), [])
                    self.manager.units[unit][prop] = old

    def test_crash_before_each_forward_phase_restores(self):
        for operation, phases in (
            ("enable", ["write-dropin", "reload", "socket", "restart-shell", "health"]),
            (
                "disable",
                [
                    "write-dropin",
                    "reload",
                    "socket",
                    "stop-daemon",
                    "restart-shell",
                    "health",
                ],
            ),
        ):
            for phase in phases:
                with self.subTest(operation=operation, phase=phase):
                    self.setUp()
                    if operation == "disable":
                        self.switch.run("enable")
                    before = self.prior()

                    def crash(name, phase=phase):
                        if name == phase:
                            raise Crash(name)

                    self.switch.phase_hook = crash
                    with self.assertRaises(Crash):
                        self.switch.run(operation)
                    self.switch.phase_hook = None
                    self.assertTrue(self.switch.run("status")["incomplete"])
                    with self.assertRaisesRegex(RuntimeError, "Incomplete"):
                        self.switch.run("enable")
                    self.switch.run("rollback")
                    self.assertRestored(before)
                    self.assertFalse(self.switch.run("status")["incomplete"])

    def test_crash_during_restore_each_phase_reconciles(self):
        phases = [
            "restore-file",
            "restore-reload",
            "restore-enablement",
            "restore-stop-socket",
            "restore-shell",
            "restore-socket",
            "restore-daemon",
            "restore-verify",
        ]
        for phase in phases:
            with self.subTest(phase=phase):
                self.setUp()
                before = self.prior()
                self.switch.run("enable")

                def crash(name, phase=phase):
                    if name == phase:
                        raise Crash(name)

                self.switch.phase_hook = crash
                with self.assertRaises(Crash):
                    self.switch.run("rollback")
                self.switch.phase_hook = None
                self.switch.run("rollback")
                self.assertRestored(before)

    def test_crash_after_action_before_completion_marker(self):
        for command in ("daemon-reload", "enable", "restart"):
            with self.subTest(command=command):
                self.setUp()
                before = self.prior()

                def crash(action, command=command):
                    if action == command:
                        raise Crash(action)

                self.manager.after_mutation = crash
                with self.assertRaises(Crash):
                    self.switch.run("enable")
                self.manager.after_mutation = None
                self.switch.run("rollback")
                self.assertRestored(before)

    def test_journal_is_private_and_status_is_read_only(self):
        before = list(self.state.rglob("*"))
        self.switch.run("status")
        self.assertEqual(list(self.state.rglob("*")), before)
        result = self.switch.run("enable")
        journal = self.switch.index / result["transaction"] / "journal.json"
        self.assertEqual(stat.S_IMODE(journal.stat().st_mode), 0o600)
        self.assertEqual(stat.S_IMODE(journal.parent.stat().st_mode), 0o700)
        with patch.object(m, "boot_id", return_value="changed-boot"):
            self.assertTrue(self.switch.run("rollback")["changedBoot"])

    def test_rollback_refuses_later_dropin_change(self):
        self.switch.run("enable")
        self.switch.dropin.write_bytes(
            b"[Service]\nEnvironment=NACRE_DAEMON_CLIENT=0\n# owner edit\n"
        )
        self.manager.calls.clear()
        with self.assertRaisesRegex(RuntimeError, "Drop-in changed"):
            self.switch.run("rollback")
        self.assertEqual(self.manager.mutations(), [])

    def test_journal_excludes_unrelated_environment(self):
        self.manager.units[m.SHELL]["Environment"] = "PRIVATE_TOKEN=secret-value"
        result = self.switch.run("enable")
        raw = (self.switch.index / result["transaction"] / "journal.json").read_text()
        self.assertNotIn("secret-value", raw)
        self.assertNotIn("PRIVATE_TOKEN", raw)

    def test_crash_after_dropin_replace_with_stale_manager(self):
        before = self.prior()
        original = m.atomic

        def crash(path, data, mode=0o600):
            original(path, data, mode)
            if path == self.switch.dropin:
                raise Crash("after-file")

        with patch.object(m, "atomic", side_effect=crash):
            with self.assertRaises(Crash):
                self.switch.run("enable")
        self.assertNotIn(m.KEY, self.switch.snapshot()[2])
        self.switch.run("rollback")
        self.assertRestored(before)

    def test_initial_inactive_shell_is_restored(self):
        self.manager.units[m.SHELL]["ActiveState"] = "inactive"
        before = self.prior()
        self.switch.run("enable")

        def crash(name):
            if name == "restore-stop-shell":
                raise Crash(name)

        self.switch.phase_hook = crash
        with self.assertRaises(Crash):
            self.switch.run("rollback")
        self.switch.phase_hook = None
        self.switch.run("rollback")
        self.assertRestored(before)

    def test_interrupted_failed_units_can_be_restored(self):
        before = self.prior()

        def crash(name):
            if name == "health":
                raise Crash(name)

        self.switch.phase_hook = crash
        with self.assertRaises(Crash):
            self.switch.run("enable")
        self.switch.phase_hook = None
        self.manager.units[m.SHELL]["ActiveState"] = "failed"
        self.manager.units[m.DAEMON]["ActiveState"] = "failed"
        self.switch.run("rollback")
        self.assertRestored(before)

    def test_marker_is_durable_before_action(self):
        def inspect_phase(name):
            record = self.switch.records()[-1]
            self.assertEqual(record["phase"], name)
            self.assertTrue(record["pending"])
            if name == "write-dropin":
                self.assertFalse(self.switch.dropin.exists())

        self.switch.phase_hook = inspect_phase
        self.switch.run("enable")

    def test_changed_manager_refuses_rollback_before_write(self):
        self.switch.run("enable")
        self.manager.manager[m.KEY] = "1"
        self.manager.calls.clear()
        with self.assertRaisesRegex(RuntimeError, "Manager.*changed"):
            self.switch.run("rollback")
        self.assertEqual(self.manager.mutations(), [])

    def test_xdg_config_and_state_paths(self):
        config = self.home / "private-config"
        config.mkdir()
        root = config / "systemd/user"
        root.mkdir(parents=True)
        self.manager.root = root
        switch = m.Switch(
            self.home,
            self.home / "private-state/nacre",
            runner=self.manager,
            environ={"XDG_CONFIG_HOME": str(config)},
        )
        result = switch.run("enable")
        self.assertTrue(
            (switch.index / result["transaction"] / "journal.json").is_file()
        )
        self.assertTrue(switch.dropin.is_file())
        switch.run("rollback")

    def test_lock_held_through_health_and_recovery(self):
        import fcntl

        release = self.state / "releases"
        release.mkdir()
        lock = release / "release.lock"
        lock.touch()
        original = self.manager.__call__

        def runner(command, **kwargs):
            with lock.open() as stream:
                with self.assertRaises(BlockingIOError):
                    fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
            return original(command, **kwargs)

        with (
            patch.object(m.os, "getuid", return_value=os.getuid() or 1000),
            patch.object(m, "Switch", return_value=self.switch),
        ):
            self.switch.runner = runner
            m.dispatch(["enable"], self.home, release)
            m.dispatch(["disable"], self.home, release)
            self.manager.bad_health = True
            with self.assertRaisesRegex(RuntimeError, "prior state restored"):
                m.dispatch(["enable"], self.home, release)


if __name__ == "__main__":
    unittest.main()
