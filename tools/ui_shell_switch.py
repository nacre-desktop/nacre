#!/usr/bin/env python3
"""Journaled activation, update, and recovery for the Bead shell.

One unit (nacre-shell.service) is always the UI. Bead mode is exactly one managed
drop-in that replaces its ExecStart, so two shells can never be started by systemd
or by D-Bus activation. Automatic recovery only selects a previously healthy Bead.
"""

import argparse
import base64
import fcntl
import hashlib
import json
import os
import re
import shlex
import shutil
import stat
import subprocess
import sys
import time
import uuid
from pathlib import Path

import f4_switch as f4

SHELL, WINDOWS, FALLBACK = (
    "nacre-shell.service",
    "nacre-qml-windows.service",
    "nacre-ui-shell-fallback.service",
)
DROPIN_NAME = "60-nacre-ui-shell.conf"
DAEMON = f4.DAEMON
DAEMON_DROPIN_NAME = "60-nacre-ui-shell-daemon.conf"
DAEMON_FLAGS = ("--notifications=daemon", "--behaviours=daemon")
MARK = "# managed by nacre ui-shell; managed file\n"
LEGACY_MARK = "# managed by nacre ui-shell; remove with: nacre ui-shell qml\n"
SAFE = re.compile(r"/[A-Za-z0-9_./+=:@-]+")
DEFAULT_LAUNCHER = None
DONE = {"complete", "rolled-back", "stopped"}
# What `nacrectl call ui.dispatch` really prints when nothing can answer (whitespace removed, lower case):
# the daemon's hub says "UI provider is unavailable"; a down daemon says "Nacre daemon is unavailable".
# A live provider answering the unknown probe target instead says "UI method is unavailable" (B0033).
NO_PROVIDER = (
    "uiproviderisunavailable",
    "nacredaemonisunavailable",
    '"dispatched":false',
    '"code":"unavailable"',
)


def dropin_text(launcher, notifications="none"):
    return (
        f"{MARK}[Unit]\nDescription=Nacre native desktop shell (Bead)\n"
        f"Wants={f4.SOCKET} {DAEMON}\nAfter={f4.SOCKET} {DAEMON}\n"
        f"OnFailure={FALLBACK}\n"
        "StartLimitIntervalSec=60\nStartLimitBurst=3\n\n"
        f"[Service]\nExecStart=\nExecStart={launcher} start\nRestart=on-failure\nRestartSec=2\n"
        "KillMode=control-group\nTimeoutStopSec=10\n"
        f"Environment=NACRE_DAEMON_CLIENT=1\nEnvironment=NACRE_UI_NOTIFICATIONS={notifications}\n"
        "UMask=0077\nLimitCORE=0\n"
    ).encode()


def daemon_text(argv):
    """Bead with --notifications daemon: the daemon owns notices and behaviours."""
    return (
        f"{MARK}[Service]\nExecStart=\nExecStart={' '.join([*argv, *DAEMON_FLAGS])}\n"
    ).encode()


def fallback_text(recover):
    return (
        f"{MARK}[Unit]\nDescription=Recover the last known-good Bead after a failure\n"
        "StartLimitIntervalSec=0\n\n"
        f"[Service]\nType=oneshot\nExecStart={recover}\n"
        "RestartForceExitStatus=75\nRestartSec=2\n"
    ).encode()


class UiShell(f4.Switch):
    def __init__(
        self,
        home,
        state,
        *,
        sleep=time.sleep,
        now=time.monotonic,
        recovery_router=None,
        **kw,
    ):
        super().__init__(home, state, **kw)
        self.unit_dir = self.config / "systemd/user"
        self.dropin = self.unit_dir / (SHELL + ".d") / DROPIN_NAME
        self.daemon_dropin = self.unit_dir / (DAEMON + ".d") / DAEMON_DROPIN_NAME
        self.windows = self.unit_dir / WINDOWS
        self.fallback = self.unit_dir / FALLBACK
        self.index = self.state / "ui-shell"
        self.sleep, self.now = sleep, now
        self.recovery_router = (
            Path(recovery_router)
            if recovery_router
            else self.home / ".local/share/nacre/control/releases.py"
        )
        self.managed = (self.dropin, self.windows, self.fallback, self.daemon_dropin)

    def selected_launcher(self, launcher=None):
        if launcher:
            return launcher
        installs = [
            r
            for r in self.records()
            if r["operation"] == "install" and r["status"] == "complete"
        ]
        return installs[-1]["launcher"] if installs else ""

    def migration_launcher(self):
        """Prefer a validated existing copy when no fresh build was supplied."""
        records = self.records()
        launchers = [
            row.get("launcher", "")
            for row in reversed(records)
            if row.get("operation") == "bead"
            and row.get("known_good", row.get("status") == "complete")
            and not row.get("recovery_failed")
        ]
        launchers.append(self.selected_launcher())
        for launcher in dict.fromkeys(launchers):
            if launcher and not self.launcher_reasons(launcher):
                return launcher
        return ""

    def install(self, binary, shell):
        """Copy a matched source build; only a completed journal selects it.

        Failed/interrupted copies are inert. Previous completed versions stay usable.
        No systemd or desktop files are changed by installation.
        """
        if any(r["status"] not in DONE for r in self.records()):
            raise RuntimeError(
                "Resolve the incomplete ui-shell transaction before installing another build"
            )
        binary, shell = Path(binary).absolute(), Path(shell).absolute()

        def source_file(path):
            # Source snapshots can use hardlinks. Copies break those links; never accept symlinks.
            f4.checked(path.parent, directory=True)
            try:
                info = path.lstat()
            except OSError as error:
                raise RuntimeError(f"Unsafe source file: {path}") from error
            if (
                not stat.S_ISREG(info.st_mode)
                or info.st_uid not in {0, os.getuid()}
                or info.st_mode & 0o022
            ):
                raise RuntimeError(f"Unsafe source file: {path}")

        binaries = [
            binary,
            binary.with_name("bead-wallpaper-video"),
            binary.with_name("nacre-wallpaper-decode"),
        ]
        for executable in binaries:
            source_file(executable)
            if not os.access(executable, os.X_OK):
                raise RuntimeError(f"Bead binary must be executable: {executable}")
        f4.checked(shell, directory=True)
        if not (shell / "palette.json").is_file() or not (shell / "bar").is_dir():
            raise RuntimeError("Expected the matching Bead shell/ tree")
        for path in shell.rglob("*"):
            if path.is_symlink():
                target = path.resolve(strict=True)
                if not target.is_relative_to(shell) or not target.is_file():
                    raise RuntimeError(
                        f"Shell link must target a file inside its tree: {path}"
                    )
                source_file(target)
            elif path.is_dir():
                f4.checked(path, directory=True)
            else:
                source_file(path)
        for old in reversed(self.records()):
            if (
                old.get("operation") != "install"
                or old.get("status") != "complete"
                or not old.get("launcher")
            ):
                continue
            version = Path(old.get("launcher", "")).parent
            if self._same_build(binary, shell, version) and not self.launcher_reasons(
                old["launcher"]
            ):
                return {
                    "status": "complete",
                    "launcher": old["launcher"],
                    "version": version.name,
                    "reused": True,
                }
        root = self.home / ".local/share/nacre/bead"
        f4.mkdir(root)
        record = self.begin("install")
        version = root / record["id"]
        launcher = version / "launch"
        record["launcher"] = str(launcher)
        self.save(record)

        def copy():
            version.mkdir(mode=0o700)
            shutil.copytree(shell, version / "shell")
            for executable in binaries:
                shutil.copy2(
                    executable,
                    version
                    / ("nacre-shell" if executable == binary else executable.name),
                )
            # Absolute immutable paths prevent subsequent checkout changes from changing a trial.
            script = (
                "#!/bin/sh\nset -eu\n"
                f"cd {shlex.quote(str(version))}\n"
                'case "${1:-}" in\n'
                "check) test -x ./nacre-wallpaper-decode; "
                "exec ./nacre-shell --help ;;\n"
                "start) shift ;;\n"
                '*) echo "usage: launch check|start" >&2; exit 2 ;;\nesac\n'
                ': "${XDG_RUNTIME_DIR:?}"\n'
                f"export NACRE_UI_RELEASE={record['id']}\n"
                '[ "${NACRE_UI_NOTIFICATIONS:-none}" != daemon ] || set -- --notices\n'
                "exec ./nacre-shell --bar --core-views --ui-provider "
                '--command-socket "$XDG_RUNTIME_DIR/bead-ui.sock" '
                '--daemon-socket "${NACRE_DAEMON_SOCKET:-$XDG_RUNTIME_DIR/nacre/daemon.sock}" "$@"\n'
            )
            f4.atomic(launcher, script.encode(), 0o700)
            for asset in version.rglob("*"):
                if asset.is_file():
                    with asset.open("rb") as stream:
                        os.fsync(stream.fileno())
                else:
                    f4.syncdir(asset)
            f4.syncdir(version)
            f4.syncdir(root)

        try:
            self.phase(record, "copy-build", copy)

            def check():
                reasons = self.launcher_reasons(str(launcher))
                if reasons:
                    raise RuntimeError("; ".join(reasons))

            self.phase(record, "check-build", check)
        except Exception:
            record["status"] = "rolled-back"
            self.save(record)
            raise
        record["status"] = "complete"
        self.save(record)
        return {
            "status": "complete",
            "launcher": str(launcher),
            "version": record["id"],
        }

    @staticmethod
    def _tree_digest(root):
        digest = hashlib.sha256()
        for path in sorted(root.rglob("*")):
            if path.is_file():
                digest.update(str(path.relative_to(root)).encode())
                digest.update(hashlib.sha256(path.read_bytes()).digest())
        return digest.digest()

    @classmethod
    def _same_build(cls, binary, shell, version):
        if not version.is_dir() or not (version / "shell").is_dir():
            return False
        for source in (
            binary,
            binary.with_name("bead-wallpaper-video"),
            binary.with_name("nacre-wallpaper-decode"),
        ):
            target = version / ("nacre-shell" if source == binary else source.name)
            if not target.is_file() or source.read_bytes() != target.read_bytes():
                return False
        return cls._tree_digest(shell) == cls._tree_digest(version / "shell")

    # -- observation -------------------------------------------------------
    def props(self, unit, *names):
        out = self.ctl("show", unit, "--property=" + ",".join(names))
        return dict(line.split("=", 1) for line in out.splitlines() if "=" in line)

    def loaded(self):
        return (
            str(self.dropin) in self.props(SHELL, "DropInPaths")["DropInPaths"].split()
        )

    def mark_ok(self, path):
        f4.checked(path)
        if not path.exists():
            return True
        data = f4.read(path)
        return MARK.encode() in data or LEGACY_MARK.encode() in data

    def provider_ready(self, binding):
        """The daemon routes ui.* to a registered provider; none means 'unavailable'."""
        call = {"target": "nacre-ui-shell-probe", "method": "probe", "args": []}
        try:
            result = self.runner(
                [
                    *self.health_command,
                    "call",
                    "--timeout",
                    "3000",
                    "ui.dispatch",
                    json.dumps(call),
                ],
                capture_output=True,
                text=True,
                timeout=6,
                env=binding,
            )
        except (OSError, subprocess.SubprocessError):
            return False
        text = "".join((result.stdout + result.stderr).split()).lower()
        return bool(text) and not any(mark in text for mark in NO_PROVIDER)

    def wait(self, condition, what, seconds=20):
        end = self.now() + seconds
        while not condition():
            if self.now() >= end:
                raise RuntimeError(f"Timed out: {what}")
            self.sleep(0.5)

    def settle(self):
        """Bead stays active with one process, the expected unit, and no legacy companion."""

        def up():
            p = self.props(SHELL, "ActiveState", "SubState", "MainPID", "NRestarts")
            return (
                p
                if p["ActiveState"] == "active" and p["SubState"] == "running"
                else None
            )

        self.wait(up, "shell not running")
        first = up()
        end = self.now() + 4
        while self.now() < end:
            self.sleep(0.5)
            if up() != first:
                raise RuntimeError(
                    "Shell restarted or exited during the stability window"
                )
        if not self.loaded():
            raise RuntimeError("Bead drop-in is not loaded")
        win = self.props(WINDOWS, "LoadState", "ActiveState")
        if win["LoadState"] == "loaded" or win["ActiveState"] == "active":
            raise RuntimeError("Legacy windows companion remains installed or running")

    def verify_bead(self):
        expected = None
        if self.dropin.exists():
            match = re.search(
                r"(?m)^ExecStart=\nExecStart=(\S+) start$",
                f4.read(self.dropin).decode(),
            )
            expected = match.group(1) if match else None
        effective = self.active_launcher()
        if expected and effective != expected:
            raise RuntimeError(
                "The running shell unit has not loaded the requested Bead launcher"
            )
        self.settle()
        self.wait(
            lambda: self.provider_ready(self.binding), "UI provider did not register"
        )
        self.health(self.binding)

    # -- daemon takeover (N0068) ---------------------------------------------
    def daemon_argv(self):
        """The packaged daemon command line without any authority flags."""
        found = re.search(
            r"argv\[\]=(.*?) ; ignore_errors",
            self.props(DAEMON, "ExecStart")["ExecStart"],
        )
        argv = [
            a
            for a in (shlex.split(found[1]) if found else [])
            if not a.startswith(("--notifications=", "--behaviours="))
        ]
        if not argv or not all(
            SAFE.fullmatch(a) or re.fullmatch(r"--?[a-z][a-z-]*(=[a-z-]+)?", a)
            for a in argv
        ):
            raise RuntimeError("Cannot determine a plain daemon command line")
        return argv

    def daemon_ready(self, notifications):
        """The daemon answers; with notifications it also owns them (topic published, service running)."""

        def ready():
            try:
                result = self.runner(
                    [
                        *self.health_command,
                        "call",
                        "--timeout",
                        "3000",
                        "daemon.status",
                        "{}",
                    ],
                    capture_output=True,
                    text=True,
                    check=True,
                    timeout=6,
                    env=self.binding,
                )
                status = json.loads(result.stdout)
            except (OSError, ValueError, subprocess.SubprocessError):
                return False
            return not notifications or (
                "notifications" in status.get("topics", [])
                and status.get("services", {}).get("notifications", {}).get("status")
                == "running"
            )

        what = (
            "daemon notifications did not become available"
            if notifications
            else "daemon did not answer"
        )
        self.wait(ready, what, 30)

    def restart_daemon(self):
        self.ctl("restart", DAEMON)

    def restart_daemon_best_effort(self):
        # Rollback: the shell must still be restarted if the daemon cannot be; daemon-ready reports it.
        try:
            self.restart_daemon()
        except subprocess.SubprocessError:
            pass

    # -- status / preflight -------------------------------------------------
    def launcher_reasons(self, launcher):
        if not launcher:
            return [
                "No Bead source build installed; run nacre ui-shell install --binary FILE --shell DIR"
            ]
        if not SAFE.fullmatch(launcher or ""):
            return [f"Launcher must be a plain absolute path: {launcher}"]
        try:
            path = f4.checked(Path(launcher))
            info = path.stat()
            if (
                not path.is_file()
                or not info.st_mode & 0o111
                or info.st_uid not in {0, os.getuid()}
            ):
                raise RuntimeError(
                    f"Launcher is not a trusted executable file: {launcher}"
                )
            self.runner(
                [launcher, "check"],
                capture_output=True,
                text=True,
                check=True,
                timeout=15,
            )
        except (OSError, RuntimeError, subprocess.SubprocessError) as error:
            return [f"Launcher check failed: {launcher}: {error}"]
        return []

    def readiness(self, launcher, notifications, *, migration=False):
        units, _, effective = self.snapshot()
        reasons, layout = self.layout(units)
        if layout:
            try:
                self.health_binary_check(layout)
            except (OSError, RuntimeError, ValueError) as error:
                reasons.append(str(error))
        if effective.get(f4.KEY) != "1" and not migration:
            reasons.append("Daemon client mode is off; run nacre f4 enable first")
        if units[f4.SOCKET].get("ActiveState") != "active" and not migration:
            reasons.append("nacre-daemon.socket is not active")
        if units[SHELL].get("ActiveState") != "active" and not migration:
            reasons.append("The current shell is not running")
        if str(self.unit_dir) not in shlex.split(self.show().get("UnitPath", "")):
            reasons.append(f"{self.unit_dir} is not in the manager UnitPath")
        for path in units[SHELL].get("DropInPaths", "").split():
            if Path(path) != self.dropin and re.search(
                rb"^\s*ExecStart\s*=", f4.read(Path(path)), re.M
            ):
                reasons.append(f"Foreign drop-in overrides ExecStart: {path}")
        for path in self.managed:
            if not self.mark_ok(path):
                reasons.append(f"Refusing to overwrite unmanaged file: {path}")
        if not all(SAFE.fullmatch(str(p)) for p in self.managed):
            reasons.append("Configuration path contains unsupported characters")
        if notifications == "daemon":
            for path in units[DAEMON].get("DropInPaths", "").split():
                if Path(path) != self.daemon_dropin and re.search(
                    rb"^\s*ExecStart\s*=", f4.read(Path(path)), re.M
                ):
                    reasons.append(
                        f"Foreign drop-in overrides the daemon ExecStart: {path}"
                    )
        if notifications not in {"none", "daemon"}:
            reasons.append(
                "Choose --notifications none (no desktop notifications while Bead is active) "
                "or daemon (the daemon takes over notifications and behaviours)"
            )
        reasons += self.launcher_reasons(launcher)
        return reasons, units, effective

    def status(self, launcher=DEFAULT_LAUNCHER):
        records = self.records()
        current_launcher = self.active_launcher()
        launcher = current_launcher or self.selected_launcher(launcher)
        notifications = next(
            (
                r["notifications"]
                for r in reversed(records)
                if r["operation"] == "bead"
                and r.get("launcher") == current_launcher
                and (r.get("known_good") or r["status"] == "complete")
            ),
            None,
        )
        if notifications is None and self.dropin.exists():
            match = re.search(
                r"(?m)^Environment=NACRE_UI_NOTIFICATIONS=(none|daemon)$",
                f4.read(self.dropin).decode(),
            )
            notifications = match.group(1) if match else None
        reasons, _, _ = self.readiness(launcher, "none")
        present = self.dropin.exists()
        loaded = self.loaded()
        return {
            "active": "bead"
            if present and loaded
            else "prepared"
            if present and not loaded and self.mark_ok(self.dropin)
            else "legacy"
            if not present and not loaded
            else "mixed",
            "launcher": launcher or None,
            "dropin_present": present,
            "dropin_loaded": loaded,
            "daemon_dropin_present": self.daemon_dropin.exists(),
            "notifications": notifications,
            "ready_for_bead": list(dict.fromkeys(reasons)),
            "incomplete": [
                {"id": r["id"], "phase": r["phase"]}
                for r in records
                if r["status"] not in DONE
            ],
        }

    def active_launcher(self):
        if not self.loaded():
            return None
        value = self.props(SHELL, "ExecStart").get("ExecStart", "")
        match = re.search(r"argv\[\]=(.*?) ; ignore_errors", value)
        if not match:
            return None
        argv = shlex.split(match.group(1))
        return argv[0] if len(argv) > 1 and argv[-1] == "start" else None

    # -- transactions -------------------------------------------------------
    def begin(self, operation, **extra):
        record = {
            "id": uuid.uuid4().hex,
            "created": time.time_ns(),
            "operation": operation,
            "dropin": str(self.dropin),
            "boot": f4.boot_id(),
            "status": "applying",
            "phase": "prepared",
            "pending": False,
            **extra,
        }
        f4.mkdir(self.index)
        (self.index / record["id"]).mkdir(mode=0o700)
        f4.syncdir(self.index)
        self.save(record)
        return record

    def binding_env(self):
        return {
            "HOME": str(self.home),
            **{
                k: v
                for k, v in self.env.items()
                if k in {"XDG_RUNTIME_DIR", "NACRE_DAEMON_SOCKET"}
            },
        }

    def restart(self):
        # A unit that hit its start limit stays failed until reset; a fallback must not be blocked by it.
        self.ctl("reset-failed", SHELL)
        self.ctl("restart", SHELL)

    def migrate(self, binary=None, shell=None):
        """Converge an installed user to Bead and the Bead recovery unit."""
        if bool(binary) != bool(shell):
            raise RuntimeError("migrate requires both --binary and --shell")
        installed = self.install(binary, shell) if binary else None
        status = self.status()
        if status["active"] == "bead":
            launcher = installed["launcher"] if installed else status["launcher"]
            notifications = status["notifications"] or "none"
            result = self.bead(launcher, notifications, migration=True)
            self._update_fallback()
            return result
        if status["active"] not in {"legacy", "prepared"}:
            raise RuntimeError(
                f"Cannot migrate while shell state is {status['active']!r}"
            )
        launcher = installed["launcher"] if installed else self.migration_launcher()
        if not launcher:
            hint = "No Bead build is installed; the current shell remains active. Install a Bead build and rerun the Nacre update."
            self._recovery_hint(hint)
            return {"status": "pending", "shell": "legacy", "hint": hint}
        return self.bead(launcher, "daemon", migration=True)

    def prepare_bead(self, binary, shell):
        """Stage a checked Bead build and next-login units without touching the live session."""
        installed = self.install(binary, shell)
        launcher = installed["launcher"]
        if not all(self.mark_ok(path) for path in self.managed):
            raise RuntimeError("Refusing to overwrite an unmanaged ui-shell file")
        f4.atomic(self.fallback, fallback_text(self._recovery_command()), 0o644)
        f4.atomic(self.dropin, dropin_text(launcher, "daemon"), 0o644)
        f4.atomic(
            self.daemon_dropin,
            daemon_text([str(self.home / ".local/bin/nacre-daemon")]),
            0o644,
        )
        self._remove_windows_unit()
        return {"status": "prepared", "shell": "bead", "launcher": launcher}

    def _update_fallback(self):
        if not self.mark_ok(self.fallback):
            raise RuntimeError("Refusing to overwrite an unmanaged fallback unit")
        desired = fallback_text(self._recovery_command())
        if self.fallback.exists() and f4.read(self.fallback) == desired:
            return
        f4.atomic(self.fallback, desired, 0o644)
        self.ctl("daemon-reload")

    def _recovery_command(self):
        router = self.recovery_router
        if not router.is_file() or not SAFE.fullmatch(str(router)):
            raise RuntimeError("The persistent Nacre release controller is unavailable")
        return f"{sys.executable} {router} ui-shell recover-bead --recover"

    def _current_notifications(self):
        environment = self.props(SHELL, "Environment").get("Environment", "")
        values = dict(
            token.split("=", 1) for token in shlex.split(environment) if "=" in token
        )
        return values.get("NACRE_UI_NOTIFICATIONS", "none")

    def _restore_daemon_mode(self, notifications):
        if not self.mark_ok(self.daemon_dropin):
            raise RuntimeError("Refusing to change an unmanaged daemon unit")
        if notifications == "daemon":
            f4.atomic(self.daemon_dropin, daemon_text(self.daemon_argv()), 0o644)
            return
        if self.daemon_dropin.exists():
            self.daemon_dropin.unlink()
            f4.syncdir(self.daemon_dropin.parent)

    def _verified_invocation(self, launcher, records):
        invocation = self.props(SHELL, "InvocationID").get("InvocationID")
        return bool(invocation) and any(
            row.get("launcher") == launcher
            and row.get("known_good")
            and not row.get("recovery_failed")
            and row.get("invocation") == invocation
            for row in records
            if row.get("operation") == "bead"
        )

    def _remember_good(self, record):
        record["known_good"] = True
        record["invocation"] = self.props(SHELL, "InvocationID").get("InvocationID")
        self.save(record)

    def bead(self, launcher, notifications, *, migration=False):
        launcher = launcher or self.selected_launcher()
        records = self.records()
        if any(r["status"] not in DONE for r in records):
            raise RuntimeError(
                "Resolve the incomplete Bead transaction before retrying"
            )
        want = dropin_text(launcher, notifications)
        if self.dropin.exists():
            if f4.read(self.dropin) != want:
                return self.upgrade_bead(
                    launcher, notifications, want, migration=migration
                )
            self.binding = self.binding_env()
            self.layout(self.snapshot()[0])
            self._update_fallback()
            if migration:
                self._remove_windows_unit(stop=True)
            shell_active = (
                self.props(SHELL, "ActiveState").get("ActiveState") == "active"
            )
            if (
                migration
                or self.active_launcher() != launcher
                or not shell_active
                or not self._verified_invocation(launcher, records)
            ):
                self.ctl("daemon-reload")
                if migration and notifications == "daemon":
                    self.ctl("stop", SHELL)
                    self.restart_daemon()
                    self.daemon_ready(True)
                self.restart()
            self.verify_bead()
            active = next(
                (
                    r
                    for r in reversed(records)
                    if r.get("operation") == "bead"
                    and r.get("launcher") == launcher
                    and not r.get("recovery_failed")
                ),
                None,
            )
            if active is not None:
                active["notifications"] = notifications
            else:
                active = self.begin(
                    "bead",
                    launcher=launcher,
                    notifications=notifications,
                    daemon=notifications == "daemon",
                )
                active["status"] = "complete"
            self._remember_good(active)
            return {"status": "complete", "shell": "bead"}
        reasons, units, _ = self.readiness(launcher, notifications, migration=migration)
        daemon = notifications == "daemon"
        if daemon:
            self.layout(units)
            self.binding = self.binding_env()
            if not reasons:
                want_daemon = daemon_text(self.daemon_argv())
        if reasons:
            raise RuntimeError("; ".join(dict.fromkeys(reasons)))
        recover = self._recovery_command()
        self.binding = self.binding_env()
        record = self.begin(
            "bead",
            launcher=launcher,
            notifications=notifications,
            daemon=daemon,
            prior={p.name: f4.file_state(p) for p in self.managed},
        )
        try:

            def write_units():
                f4.atomic(self.fallback, fallback_text(recover), 0o644)

            self.phase(record, "write-units", write_units)
            self.phase(record, "reload-units", lambda: self.ctl("daemon-reload"))
            self.phase(
                record,
                "stop-windows",
                lambda: (
                    self.ctl("stop", WINDOWS)
                    if self.props(WINDOWS, "LoadState")["LoadState"] == "loaded"
                    else None
                ),
            )
            self.phase(
                record,
                "remove-windows",
                lambda: self._remove_windows_unit(),
            )
            self.phase(
                record, "reload-without-windows", lambda: self.ctl("daemon-reload")
            )
            if daemon:
                self.phase(
                    record,
                    "write-daemon-dropin",
                    lambda: f4.atomic(self.daemon_dropin, want_daemon, 0o644),
                )
            self.phase(
                record, "write-dropin", lambda: f4.atomic(self.dropin, want, 0o644)
            )
            self.phase(record, "reload", lambda: self.ctl("daemon-reload"))
            if daemon:
                # The legacy shell releases org.freedesktop.Notifications before takeover.
                self.phase(record, "stop-shell", lambda: self.ctl("stop", SHELL))
                self.phase(record, "restart-daemon", self.restart_daemon)
                self.phase(record, "daemon-ready", lambda: self.daemon_ready(True))
            self.phase(record, "restart-shell", self.restart)
            self.phase(record, "health", self.verify_bead)
            record["status"] = "complete"
            self._remember_good(record)
        except Exception as error:
            record["status"] = "failed"
            record["error"] = str(error)
            self.save(record)
            self._reject_launcher(self.records(), launcher, str(error))
            recovery = self.recover_bead()
            if recovery["status"] == "recovered":
                raise RuntimeError(
                    f"Bead activation failed: {error}; previous healthy Bead restored"
                ) from error
            raise RuntimeError(
                f"Bead activation failed: {error}; {recovery['hint']}"
            ) from error
        return {"status": "complete", "shell": "bead", "transaction": record["id"]}

    def upgrade_bead(self, launcher, notifications, want, *, migration=False):
        """Replace a live Bead build transactionally; the prior healthy build stays recoverable."""
        reasons, units, _ = self.readiness(launcher, notifications, migration=migration)
        if reasons:
            raise RuntimeError("; ".join(dict.fromkeys(reasons)))
        self.layout(units)
        self.binding = self.binding_env()
        old_dropin = f4.file_state(self.dropin)
        old_fallback = f4.file_state(self.fallback)
        old_daemon_dropin = f4.file_state(self.daemon_dropin)
        if old_dropin and self._current_notifications() != notifications:
            raise RuntimeError(
                "Changing notification ownership requires an explicit shell migration"
            )
        record = self.begin(
            "bead",
            launcher=launcher,
            notifications=notifications,
            daemon=bool(self.daemon_dropin.exists()),
            prior={
                self.dropin.name: old_dropin,
                self.fallback.name: old_fallback,
                self.daemon_dropin.name: old_daemon_dropin,
            },
        )
        recover = self._recovery_command()
        self.binding = self.binding_env()
        try:
            self.phase(
                record,
                "write-fallback",
                lambda: f4.atomic(self.fallback, fallback_text(recover), 0o644),
            )
            if migration:
                self.phase(
                    record,
                    "remove-windows",
                    lambda: self._remove_windows_unit(stop=True),
                )
            self.phase(
                record, "write-dropin", lambda: f4.atomic(self.dropin, want, 0o644)
            )
            self.phase(record, "reload", lambda: self.ctl("daemon-reload"))
            self.phase(record, "restart-shell", self.restart)
            self.phase(record, "health", self.verify_bead)
            record["status"] = "complete"
            self._remember_good(record)
        except Exception as error:
            try:
                for path, saved in (
                    (self.dropin, old_dropin),
                    (self.daemon_dropin, old_daemon_dropin),
                ):
                    if saved is None:
                        path.unlink(missing_ok=True)
                    else:
                        f4.atomic(path, base64.b64decode(saved["data"]), saved["mode"])
                f4.atomic(self.fallback, fallback_text(recover), 0o644)
                self.ctl("daemon-reload")
                self.restart()
                self.verify_bead()
                record["status"] = "rolled-back"
                self.save(record)
            except Exception as recovery:  # noqa: BLE001 - rollback must catch any failed recovery step
                record["status"] = "failed"
                record["error"] = str(recovery)
                self.save(record)
                raise RuntimeError(
                    f"Bead update failed: {error}; previous Bead could not be restored: {recovery}"
                ) from error
            raise RuntimeError(
                f"Bead update failed; previous healthy Bead restored: {error}"
            ) from error
        return {"status": "complete", "shell": "bead", "transaction": record["id"]}

    def recover_bead(self, *, exclude_current=False, mark_failed_current=False):
        """Recover only a previously health-checked Bead, then stop if none survives."""
        records = self.records()
        candidates = [
            row
            for row in records
            if row.get("operation") == "bead"
            and row.get("known_good", row.get("status") == "complete")
            and not row.get("recovery_failed")
        ]
        current = self.active_launcher()
        if mark_failed_current and current:
            self._reject_launcher(records, current, "systemd start limit reached")
        if exclude_current and current:
            current_copies = [
                row for row in candidates if row.get("launcher") == current
            ]
            candidates = [row for row in candidates if row.get("launcher") != current]
            if not mark_failed_current:
                if not candidates and current_copies:
                    raise RuntimeError(
                        "No previous known-good Bead copy is available; the current shell remains unchanged"
                    )
                if candidates:
                    # Manual rollback may restore its healthy starting copy if every
                    # older copy fails. Automatic recovery must exclude the failed copy.
                    candidates = current_copies + candidates
        self.binding = self.binding_env()
        if not all(
            self.mark_ok(path)
            for path in (self.dropin, self.fallback, self.daemon_dropin)
        ):
            hint = "Bead recovery refused an unmanaged shell or fallback unit; inspect the user systemd unit files before reinstalling."
            return self._stop_with_hint(hint)
        try:
            self.layout(self.snapshot()[0])
            self.ctl("reset-failed", SHELL)
        except (OSError, RuntimeError, ValueError, subprocess.SubprocessError) as error:
            return self._stop_with_hint(
                f"Bead recovery could not inspect the desktop: {error}"
            )
        attempted = set()
        for candidate in reversed(candidates):
            launcher = candidate.get("launcher", "")
            if launcher in attempted:
                continue
            attempted.add(launcher)
            notifications = candidate.get("notifications", "none")
            reasons = self.launcher_reasons(launcher)
            if reasons:
                self._reject_launcher(records, launcher, "; ".join(reasons))
                continue
            try:
                desired = dropin_text(launcher, notifications)
                mode_changed = self._current_notifications() != notifications
                if mode_changed:
                    self.ctl("stop", SHELL)
                    self._restore_daemon_mode(notifications)
                if not self.dropin.exists() or f4.read(self.dropin) != desired:
                    f4.atomic(self.dropin, desired, 0o644)
                    f4.atomic(
                        self.fallback, fallback_text(self._recovery_command()), 0o644
                    )
                    self.ctl("daemon-reload")
                if mode_changed:
                    if notifications == "daemon":
                        self.restart_daemon()
                        self.daemon_ready(True)
                    else:
                        self.restart_daemon_best_effort()
                self.restart()
                self.verify_bead()
                candidate["recovered_at"] = time.time_ns()
                self._remember_good(candidate)
                self._settle_recovered_records(records, launcher)
                return {"status": "recovered", "shell": "bead", "launcher": launcher}
            except Exception as error:  # noqa: BLE001 - every failed recovery candidate must be stopped
                self._reject_launcher(records, launcher, str(error))
                try:
                    self.ctl("stop", SHELL)
                except Exception:  # noqa: BLE001 - continue journaling if stop itself fails
                    pass
        hint = "No healthy Bead copy remains. Run the Nacre installer to install and activate a Bead build; the shell was stopped to prevent a restart loop."
        for row in records:
            if row.get("status") not in DONE:
                row["status"] = "stopped"
                row["recovery_stopped"] = True
                self.save(row)
        return self._stop_with_hint(hint)

    def _reject_launcher(self, records, launcher, error):
        for row in records:
            if row.get("operation") == "bead" and row.get("launcher") == launcher:
                row["recovery_failed"] = True
                row["recovery_error"] = str(error)
                self.save(row)

    def _settle_recovered_records(self, records, launcher):
        for row in records:
            if row.get("status") not in DONE:
                row["status"] = "rolled-back"
                row["recovered_by"] = launcher
                self.save(row)

    def _stop_with_hint(self, hint):
        try:
            self.ctl("stop", SHELL)
        except Exception:  # noqa: BLE001 - always write a recovery hint after best-effort stop
            pass
        self._recovery_hint(hint)
        return {"status": "stopped", "shell": None, "hint": hint}

    def _recovery_hint(self, message):
        try:
            self.runner(
                ["systemd-cat", "--identifier=nacre-ui-shell-fallback"],
                input=message + "\n",
                text=True,
                check=True,
                timeout=5,
            )
        except (OSError, subprocess.SubprocessError):
            pass

    def _remove_windows_unit(self, *, stop=False):
        if not self.mark_ok(self.windows):
            raise RuntimeError(f"Refusing to remove unmanaged file: {self.windows}")
        if self.windows.exists():
            if stop and self.props(WINDOWS, "LoadState")["LoadState"] == "loaded":
                self.ctl("stop", WINDOWS)
            self.windows.unlink()
            f4.syncdir(self.windows.parent)


def dispatch(args, home, release_state, **kw):
    parser = argparse.ArgumentParser(prog="nacre ui-shell")
    parser.add_argument(
        "operation",
        choices=("status", "install", "bead", "rollback", "recover-bead", "migrate"),
    )
    parser.add_argument("--launcher", default=DEFAULT_LAUNCHER)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--shell", type=Path)
    parser.add_argument("--notifications", choices=("none", "daemon"))
    parser.add_argument(
        "--recover",
        action="store_true",
        help="service use: do nothing if a switch is running or the start limit is not exhausted",
    )
    parsed = parser.parse_args(args)
    f4.require_ordinary_user()
    f4.mkdir(Path(release_state))
    lock_path = f4.checked(Path(release_state) / "release.lock")
    fd = os.open(lock_path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CREAT, 0o600)
    with os.fdopen(fd) as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | (fcntl.LOCK_NB if parsed.recover else 0))
        except BlockingIOError:
            return {
                "status": "busy",
                "detail": "a switch is already running",
            }
        state_home = Path(
            os.environ.get("XDG_STATE_HOME") or Path(home) / ".local/state"
        )
        switch = UiShell(home, state_home / "nacre", **kw)
        if (
            parsed.recover
            and parsed.operation == "recover-bead"
            and switch.props(SHELL, "Result")["Result"] != "start-limit-hit"
        ):
            # systemd runs OnFailure= on EVERY crash, also when it schedules a restart (measured, B0033).
            # Only an exhausted start limit (3 in 60 s) means Bead cannot stay up.
            return {
                "status": "ignored",
                "detail": "the shell is restarting; the start limit is not exhausted",
            }
        if parsed.operation == "install":
            if not parsed.binary or not parsed.shell:
                raise RuntimeError("install requires --binary and --shell")
            return switch.install(parsed.binary, parsed.shell)
        if parsed.operation == "status":
            return switch.status(parsed.launcher)
        if parsed.operation == "bead":
            return switch.bead(parsed.launcher, parsed.notifications)
        if parsed.operation == "migrate":
            return switch.migrate(parsed.binary, parsed.shell)
        if parsed.operation == "recover-bead":
            return switch.recover_bead(exclude_current=True, mark_failed_current=True)
        if parsed.operation == "rollback":
            return switch.recover_bead(exclude_current=True)
        raise RuntimeError("Unsupported ui-shell operation")
