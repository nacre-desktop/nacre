#!/usr/bin/env python3
"""Journaled one-step switch of the always-on UI between the QML shell and Bead (B0029).

One unit (nacre-shell.service) is always the UI. Bead mode is exactly one managed
drop-in that replaces its ExecStart, so two shells can never be started by systemd
or by D-Bus activation. The QML Settings/Colors/Welcome windows run in a companion
unit that only exists while that drop-in does. `qml` removes the drop-in and
converges to the QML shell from any interrupted state.
"""

import argparse
import base64
import fcntl
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
MARK = "# managed by nacre ui-shell; remove with: nacre ui-shell qml\n"
SAFE = re.compile(r"/[A-Za-z0-9_./+=:@-]+")
DEFAULT_LAUNCHER = None
DONE = {"complete", "rolled-back"}
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
        f"Wants={WINDOWS} {f4.SOCKET}\nAfter={f4.SOCKET}\nOnFailure={FALLBACK}\n"
        "StartLimitIntervalSec=60\nStartLimitBurst=3\n\n"
        f"[Service]\nExecStart=\nExecStart={launcher} start\nRestart=on-failure\nRestartSec=2\n"
        f"Environment=NACRE_UI_NOTIFICATIONS={notifications}\nUMask=0077\nLimitCORE=0\n"
    ).encode()


def daemon_text(argv):
    """Bead with --notifications daemon: the daemon owns notifications and the QML-owned behaviours."""
    return (
        f"{MARK}[Service]\nExecStart=\nExecStart={' '.join([*argv, *DAEMON_FLAGS])}\n"
    ).encode()


def windows_text(dropin, qml_argv):
    return (
        f"{MARK}[Unit]\nDescription=Nacre QML windows (Settings, Colors, Welcome) beside Bead\n"
        f"PartOf={SHELL}\nAfter={SHELL}\nConditionPathExists={dropin}\n\n"
        f"[Service]\nType=simple\nEnvironment={f4.KEY}=1\nEnvironment=NACRE_QML_WINDOWS_ONLY=1\n"
        f"ExecStart={' '.join(qml_argv)}\nRestart=on-failure\nRestartSec=2\n"
        "KillMode=control-group\nTimeoutStopSec=10\n"
    ).encode()


def fallback_text(recover):
    return (
        f"{MARK}[Unit]\nDescription=Return to the QML shell after a Bead failure\n\n"
        f"[Service]\nType=oneshot\nExecStart={recover}\n"
    ).encode()


class UiShell(f4.Switch):
    def __init__(self, home, state, *, sleep=time.sleep, now=time.monotonic, **kw):
        super().__init__(home, state, **kw)
        self.unit_dir = self.config / "systemd/user"
        self.dropin = self.unit_dir / (SHELL + ".d") / DROPIN_NAME
        self.daemon_dropin = self.unit_dir / (DAEMON + ".d") / DAEMON_DROPIN_NAME
        self.windows = self.unit_dir / WINDOWS
        self.fallback = self.unit_dir / FALLBACK
        self.index = self.state / "ui-shell"
        self.sleep, self.now = sleep, now
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

    def install(self, binary, shell):
        """Copy a matched source build; only a completed journal selects it.

        Failed/interrupted copies are inert. Previous completed versions stay usable.
        No systemd or desktop files are changed by installation.
        """
        if self.dropin.exists() or any(r["status"] not in DONE for r in self.records()):
            raise RuntimeError("Run nacre ui-shell qml before installing another build")
        binary, shell = Path(binary).absolute(), Path(shell).absolute()

        def source_file(path):
            # Source snapshots can use hardlinks. Copies break those links; never accept symlinks.
            f4.checked(path.parent, directory=True)
            info = path.lstat()
            if (
                not stat.S_ISREG(info.st_mode)
                or info.st_uid not in {0, os.getuid()}
                or info.st_mode & 0o022
            ):
                raise RuntimeError(f"Unsafe source file: {path}")

        source_file(binary)
        f4.checked(shell, directory=True)
        if not binary.is_file() or not os.access(binary, os.X_OK):
            raise RuntimeError("Bead binary must be an executable regular file")
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
            shutil.copy2(binary, version / "nacre-shell")
            # Absolute immutable paths prevent subsequent checkout changes from changing a trial.
            script = (
                "#!/bin/sh\nset -eu\n"
                f"cd {shlex.quote(str(version))}\n"
                'case "${1:-}" in\n'
                "check) exec ./nacre-shell --help ;;\n"
                "start) shift ;;\n"
                '*) echo "usage: launch check|start" >&2; exit 2 ;;\nesac\n'
                ': "${XDG_RUNTIME_DIR:?}"\n'
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
        return not path.exists() or MARK.encode() in f4.read(path)

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

    def settle(self, bead):
        """Active, same process for several seconds, right drop-in loaded, right companion state."""

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
        if self.loaded() != bead:
            raise RuntimeError("Loaded drop-in does not match the requested shell")
        win = self.props(WINDOWS, "LoadState", "ActiveState")
        if bead and win["ActiveState"] != "active":
            raise RuntimeError("QML windows companion is not running")
        if not bead and win["ActiveState"] == "active":
            raise RuntimeError(
                "QML windows companion still running beside the QML shell"
            )

    def verify_bead(self):
        self.settle(True)
        self.wait(
            lambda: self.provider_ready(self.binding), "UI provider did not register"
        )
        self.health(self.binding)

    def verify_qml(self):
        self.settle(False)
        effective = f4.environment(self.ctl("show-environment"))
        effective.update(f4.environment(self.show(SHELL).get("Environment", "")))
        if effective.get(f4.KEY) == "1":
            self.wait(
                lambda: self.provider_ready(self.binding),
                "QML provider did not register",
            )

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

    def readiness(self, launcher, notifications):
        units, _, effective = self.snapshot()
        reasons, layout = self.layout(units)
        if layout:
            try:
                self.health_binary_check(layout)
            except (OSError, RuntimeError, ValueError) as error:
                reasons.append(str(error))
        if effective.get(f4.KEY) != "1":
            reasons.append("Daemon client mode is off; run nacre f4 enable first")
        if units[f4.SOCKET].get("ActiveState") != "active":
            reasons.append("nacre-daemon.socket is not active")
        if units[SHELL].get("ActiveState") != "active":
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
        launcher = self.selected_launcher(launcher)
        reasons, _, _ = self.readiness(launcher, "none")
        present = self.dropin.exists()
        loaded = self.loaded()
        return {
            "active": "bead"
            if present and loaded
            else "qml"
            if not present and not loaded
            else "mixed",
            "launcher": launcher or None,
            "dropin_present": present,
            "dropin_loaded": loaded,
            "daemon_dropin_present": self.daemon_dropin.exists(),
            "windows": self.props(WINDOWS, "ActiveState")["ActiveState"],
            "notifications": next(
                (
                    r["notifications"]
                    for r in reversed(records)
                    if r["operation"] == "bead" and r["status"] == "complete"
                ),
                None,
            ),
            "ready_for_bead": list(dict.fromkeys(reasons)),
            "incomplete": [
                {"id": r["id"], "phase": r["phase"]}
                for r in records
                if r["status"] not in DONE
            ],
        }

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

    def converge_qml(self, record):
        """Idempotent: every step is safe to repeat from any interrupted point."""

        def restore_paths(paths):
            prior = record.get("prior", {})
            for path in paths:
                if not self.mark_ok(path):
                    raise RuntimeError(f"File is not managed by this tool: {path}")
                saved = prior.get(path.name)
                if saved is None:
                    if path.exists():
                        path.unlink()
                        f4.syncdir(path.parent)
                else:
                    f4.atomic(path, base64.b64decode(saved["data"]), saved["mode"])

        def stop_windows():
            if self.props(WINDOWS, "LoadState")["LoadState"] == "loaded":
                self.ctl("stop", WINDOWS)

        daemon = record.get("daemon", False)
        self.phase(
            record,
            "remove-dropin",
            lambda: restore_paths([self.dropin, self.daemon_dropin]),
        )
        self.phase(record, "reload", lambda: self.ctl("daemon-reload"))
        self.phase(record, "stop-windows", stop_windows)
        self.phase(
            record,
            "restore-companions",
            lambda: restore_paths([self.windows, self.fallback]),
        )
        self.phase(record, "reload-restored", lambda: self.ctl("daemon-reload"))
        if daemon:  # back to the legacy daemon before QML starts and asks for the name
            self.phase(record, "restart-daemon", self.restart_daemon_best_effort)
        self.phase(record, "restart-shell", self.restart)
        if daemon:
            self.phase(record, "daemon-ready", lambda: self.daemon_ready(False))
        self.phase(record, "health", self.verify_qml)

    def qml(self, recover=False):
        records = self.records()
        self.binding = self.binding_env()
        self.layout(self.snapshot()[0])
        if (
            not self.dropin.exists()
            and not self.daemon_dropin.exists()
            and not self.loaded()
            and all(r["status"] in DONE for r in records)
            and not any(
                r["operation"] == "bead" and r["status"] == "complete" for r in records
            )
        ):
            return {"status": "complete", "shell": "qml"}
        previous = next(
            (
                r
                for r in reversed(records)
                if r["operation"] in {"qml", "bead"} and r["status"] != "rolled-back"
            ),
            {},
        )
        record = self.begin(
            "qml",
            prior=previous.get("prior", {}),
            daemon=bool(previous.get("daemon")) or self.daemon_dropin.exists(),
        )
        try:
            self.converge_qml(record)
        except Exception as error:
            record["status"] = "failed"
            self.save(record)
            raise RuntimeError(
                f"QML rollback incomplete: {error}; transaction {record['id']}; rerun nacre ui-shell qml"
            ) from error
        record["status"] = "complete"
        self.save(record)
        for old in records:
            if (
                old["status"] not in DONE
                or old["operation"] == "bead"
                and old["status"] == "complete"
            ):
                old["status"] = "rolled-back"
                self.save(old)
        return {"status": "complete", "shell": "qml", "transaction": record["id"]}

    def bead(self, launcher, notifications):
        launcher = self.selected_launcher(launcher)
        records = self.records()
        if any(r["status"] not in DONE for r in records):
            raise RuntimeError(
                "Incomplete ui-shell transaction; run nacre ui-shell qml"
            )
        want = dropin_text(launcher, notifications)
        if self.dropin.exists():
            if f4.read(self.dropin) != want:
                raise RuntimeError(
                    "A different Bead drop-in is present; run nacre ui-shell qml first"
                )
            self.binding = self.binding_env()
            self.layout(self.snapshot()[0])
            self.verify_bead()
            return {"status": "complete", "shell": "bead"}
        reasons, units, _ = self.readiness(launcher, notifications)
        daemon = notifications == "daemon"
        if daemon:
            self.layout(units)
            self.binding = self.binding_env()
            if not reasons:
                want_daemon = daemon_text(self.daemon_argv())
        if reasons:
            raise RuntimeError("; ".join(dict.fromkeys(reasons)))
        argv = re.search(
            r"argv\[\]=(.*?) ; ignore_errors",
            self.props(SHELL, "ExecStart")["ExecStart"],
        )
        qml_argv = shlex.split(argv[1]) if argv else []
        if not qml_argv or not all(
            SAFE.fullmatch(a) or re.fullmatch(r"[a-z-]+", a) for a in qml_argv
        ):
            raise RuntimeError("Cannot determine a plain QML shell command line")
        router = Path(__file__).with_name("releases.py")
        recover = f"{sys.executable} {router} ui-shell qml --recover"
        if not router.is_file() or not SAFE.fullmatch(f"{sys.executable}{router}"):
            raise RuntimeError("Cannot build a safe fallback command")
        self.binding = self.binding_env()
        record = self.begin(
            "bead",
            launcher=launcher,
            notifications=notifications,
            daemon=daemon,
            qml_exec=qml_argv,
            prior={p.name: f4.file_state(p) for p in self.managed},
        )
        try:

            def write_units():
                f4.atomic(self.windows, windows_text(self.dropin, qml_argv), 0o644)
                f4.atomic(self.fallback, fallback_text(recover), 0o644)

            self.phase(record, "write-units", write_units)
            self.phase(record, "reload-units", lambda: self.ctl("daemon-reload"))
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
                # QML releases org.freedesktop.Notifications, the daemon restarts and
                # takes it, and only then does Bead start.
                self.phase(record, "stop-shell", lambda: self.ctl("stop", SHELL))
                self.phase(record, "restart-daemon", self.restart_daemon)
                self.phase(record, "daemon-ready", lambda: self.daemon_ready(True))
            self.phase(record, "restart-shell", self.restart)
            self.phase(record, "health", self.verify_bead)
            record["status"] = "complete"
            self.save(record)
        except Exception as error:
            try:
                self.converge_qml(record)
                record["status"] = "rolled-back"
                self.save(record)
            except Exception as recovery:  # noqa: BLE001 - reported with the transaction
                raise RuntimeError(
                    f"Bead failed: {error}; QML rollback incomplete: {recovery}; run nacre ui-shell qml (transaction {record['id']})"
                ) from error
            raise RuntimeError(
                f"Bead failed: {error}; the QML shell was restored; transaction {record['id']}"
            ) from error
        return {"status": "complete", "shell": "bead", "transaction": record["id"]}


def dispatch(args, home, release_state, **kw):
    parser = argparse.ArgumentParser(prog="nacre ui-shell")
    parser.add_argument(
        "operation", choices=("status", "install", "bead", "qml", "rollback")
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
                "detail": "a switch is already running and will restore QML itself",
            }
        state_home = Path(
            os.environ.get("XDG_STATE_HOME") or Path(home) / ".local/state"
        )
        switch = UiShell(home, state_home / "nacre", **kw)
        if (
            parsed.recover
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
        return switch.qml()
