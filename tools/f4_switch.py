#!/usr/bin/env python3
"""Bounded, unprivileged F4 activation switch (no package or setup rollback)."""

import argparse
import base64
import fcntl
import json
import os
import re
import shlex
import stat
import subprocess
import tempfile
import time
import uuid
from pathlib import Path

SHELL, DAEMON, SOCKET = UNITS = (
    "nacre-shell.service",
    "nacre-daemon.service",
    "nacre-daemon.socket",
)
KEY = "NACRE_DAEMON_CLIENT"
CONTENTS = {str(i): f"[Service]\nEnvironment={KEY}={i}\n".encode() for i in (0, 1)}
PROPERTIES = (
    "FragmentPath DropInPaths LoadState ActiveState UnitFileState Environment "
    "EnvironmentFiles PassEnvironment UnsetEnvironment WantedBy RequiredBy TriggeredBy"
).split()


def checked(path, *, directory=False, private=False):
    """Reject symlink ancestry and files writable by another identity."""
    path = Path(path)
    if not path.is_absolute() or ".." in path.parts:
        raise RuntimeError(f"Unsupported absolute path: {path}")
    for part in reversed([path, *path.parents]):
        try:
            info = part.lstat()
        except FileNotFoundError:
            continue
        leaf = part == path and not directory
        expected = stat.S_ISREG if leaf else stat.S_ISDIR
        sticky_root = info.st_uid == 0 and stat.S_IMODE(info.st_mode) == 0o1777
        if (
            not expected(info.st_mode)
            or info.st_uid not in {0, os.getuid()}
            or (info.st_mode & 0o022 and not sticky_root)
            or (leaf and info.st_nlink != 1)
        ):
            raise RuntimeError(f"Unsafe {'file' if leaf else 'directory'}: {part}")
        if (
            private
            and part == path
            and (
                info.st_uid != os.getuid()
                or stat.S_IMODE(info.st_mode) != (0o700 if directory else 0o600)
            )
        ):
            raise RuntimeError(f"Non-private transaction path: {part}")
    return path


def read(path, *, private=False):
    checked(path, private=private)
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, "rb") as stream:
        if os.fstat(stream.fileno()).st_size > 2 * 1024 * 1024:
            raise RuntimeError(f"Oversized input: {path}")
        return stream.read(2 * 1024 * 1024 + 1)


def syncdir(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def mkdir(path):
    checked(path, directory=True)
    if not path.exists():
        mkdir(path.parent)
        path.mkdir(mode=0o700)
        syncdir(path.parent)


def atomic(path, data, mode=0o600):
    checked(path)
    mkdir(path.parent)
    fd, temporary = tempfile.mkstemp(prefix=".f4-", dir=path.parent)
    try:
        with os.fdopen(fd, "wb") as stream:
            os.fchmod(stream.fileno(), mode)
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())
        os.replace(temporary, path)
        syncdir(path.parent)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)


def file_state(path):
    checked(path)
    if not path.exists():
        return None
    return {
        "data": base64.b64encode(read(path)).decode(),
        "mode": stat.S_IMODE(path.stat().st_mode),
    }


def environment(text):
    result = {}
    for token in shlex.split(text):
        key, sep, value = token.partition("=")
        if not sep or key in result:
            raise RuntimeError(f"Unresolved systemd environment assignment: {key}")
        result[key] = value
    return result


class Switch:
    def __init__(
        self, home, state, *, runner=subprocess.run, environ=None, phase_hook=None
    ):
        self.home, self.state = Path(home), Path(state)
        self.env = dict(os.environ if environ is None else environ)
        config = Path(self.env.get("XDG_CONFIG_HOME") or self.home / ".config")
        self.config = checked(config, directory=True)
        self.dropin = config / "systemd/user/nacre-shell.service.d/50-nacre-f4.conf"
        self.index = self.state / "f4-switch"
        self.runner, self.phase_hook = runner, phase_hook

    def ctl(self, *args):
        return self.runner(
            ["systemctl", "--user", *args],
            capture_output=True,
            text=True,
            check=True,
            timeout=15,
        ).stdout.strip()

    def show(self, unit=None):
        args = ["show"] + ([unit] if unit else [])
        args += ["--property=" + p for p in (PROPERTIES if unit else ["UnitPath"])]
        return dict(
            line.split("=", 1) for line in self.ctl(*args).splitlines() if "=" in line
        )

    def records(self):
        checked(self.index, directory=True, private=True)
        if not self.index.exists():
            return []
        records = []
        for path in sorted(self.index.iterdir()):
            if not re.fullmatch(r"[0-9a-f]{32}", path.name):
                raise RuntimeError(f"Unrecognized transaction path: {path}")
            checked(path, directory=True, private=True)
            record = json.loads(read(path / "journal.json", private=True))
            if record.get("id") != path.name or record.get("dropin") != str(
                self.dropin
            ):
                raise RuntimeError(f"Mismatched journal: {path}")
            records.append(record)
        return sorted(records, key=lambda record: record["created"])

    def save(self, record):
        atomic(self.index / record["id"] / "journal.json", json.dumps(record).encode())

    def phase(self, record, name, action):
        record["phase"], record["pending"] = name, True
        self.save(record)
        if self.phase_hook:
            self.phase_hook(name)
        action()
        record["pending"] = False
        self.save(record)

    def snapshot(self):
        units = {unit: self.show(unit) for unit in UNITS}
        manager = environment(self.ctl("show-environment"))
        effective = dict(manager)
        effective.update(environment(units[SHELL].get("Environment", "")))
        return units, manager, effective

    def startup(self, roots, *, allow_socket):
        """Bounded N10-style inventory of unit links and standard activation inputs."""
        roots = [(Path(root), True) for root in roots] + [
            (self.config / "autostart", False),
            (self.home / ".local/share/dbus-1/services", False),
            (Path("/etc/xdg/autostart"), False),
            (Path("/usr/share/dbus-1/services"), False),
        ]
        count = total = 0
        for root, unit_root in roots:
            checked(root, directory=True)
            if not root.exists():
                continue
            pending = [(root, 0)]
            while pending:
                directory, depth = pending.pop()
                for path in sorted(directory.iterdir()):
                    count += 1
                    if count > 8192:
                        raise RuntimeError(f"Startup inventory exceeds bound: {root}")
                    info = path.lstat()
                    if stat.S_ISDIR(info.st_mode):
                        if (
                            unit_root
                            and depth == 0
                            and path.name.endswith((".wants", ".requires", ".d"))
                        ):
                            checked(path, directory=True)
                            pending.append((path, 1))
                        continue
                    if stat.S_ISLNK(info.st_mode):
                        target = os.readlink(path)
                        if (
                            "nacre-daemon" in str(path.relative_to(root))
                            or "nacre-daemon" in target
                        ):
                            own = (
                                allow_socket
                                and path
                                == self.config
                                / "systemd/user/graphical-session.target.wants"
                                / SOCKET
                                and target
                                == getattr(
                                    self,
                                    "socket_fragment",
                                    "/usr/lib/systemd/user/" + SOCKET,
                                )
                            )
                            if not own:
                                raise RuntimeError(
                                    f"Independent daemon activation link: {path}"
                                )
                            continue
                        if target == "/dev/null":
                            continue
                        try:
                            resolved = path.resolve(strict=True)
                        except FileNotFoundError:
                            continue  # dangling links have no activation target
                        if not unit_root and (
                            info.st_uid != 0 or resolved.stat().st_uid != 0
                        ):
                            raise RuntimeError(f"Unresolved startup symlink: {path}")
                        raw = read(resolved)
                    else:
                        raw = read(path)
                    if (
                        path != self.dropin
                        and path.suffix == ".conf"
                        and KEY.encode() in raw
                    ):
                        raise RuntimeError(
                            f"Foreign/conflicting environment input: {path}"
                        )
                    total += len(raw)
                    if total > 32 * 1024 * 1024:
                        raise RuntimeError(f"Startup bytes exceed bound: {root}")
                    if (
                        not (unit_root and depth == 0 and path.name in UNITS)
                        and b"nacre-daemon" in raw
                    ):
                        raise RuntimeError(f"Independent daemon startup input: {path}")

    def layout(self, units):
        """Validate one safe source layout or one vendor layout."""
        reasons, layouts = [], set()
        for unit, values in units.items():
            fragment = values.get("FragmentPath", "")
            source = self.config / "systemd/user" / unit
            vendor = "/usr/lib/systemd/user/" + unit
            if not fragment:
                reasons.append(
                    f"Missing deployed unit {source}; deploy the source helper and manual daemon templates with python3 nacre/shell-tools/install.py (full install, without --code-only)"
                )
                continue
            try:
                if fragment == str(source):
                    checked(source)
                    if not source.is_file() or source.stat().st_uid != os.getuid():
                        raise RuntimeError(
                            f"Source unit must be a user-owned regular file: {source}"
                        )
                    raw = read(source)
                    for token in (
                        KEY.encode(),
                        b"EnvironmentFile",
                        b"PassEnvironment",
                        b"UnsetEnvironment",
                    ):
                        if token in raw:
                            raise RuntimeError(
                                f"Unsupported {token.decode()} in source fragment: {source}"
                            )
                    layouts.add("source")
                elif fragment == vendor:
                    if os.path.lexists(source):
                        raise RuntimeError(
                            f"HOME unit shadows vendor fragment: {source}"
                        )
                    layouts.add("package")
                else:
                    raise RuntimeError(f"Unsupported unit fragment: {unit}: {fragment}")
                legacy = self.home / ".config/systemd/user" / unit
                if legacy != source and os.path.lexists(legacy):
                    raise RuntimeError(f"Conflicting HOME unit: {legacy}")
            except (OSError, RuntimeError, ValueError) as error:
                reasons.append(f"{source}: {error}")
        if len(layouts) > 1:
            reasons.append(
                "Mixed package/source unit layouts: "
                + ", ".join(values.get("FragmentPath", "") for values in units.values())
            )
        layout = next(iter(layouts)) if len(layouts) == 1 else None
        self.socket_fragment = (
            str(self.config / "systemd/user" / SOCKET)
            if layout == "source"
            else "/usr/lib/systemd/user/" + SOCKET
        )
        self.health_binary = (
            str(self.home / ".local/share/nacre/shell/tools/nacre-helper")
            if layout == "source"
            else "/usr/bin/nacrectl"
        )
        self.health_command = (
            [self.health_binary, "ctl"] if layout == "source" else [self.health_binary]
        )
        return reasons, layout

    def health_binary_check(self, layout):
        if layout == "source":
            helper = checked(Path(self.health_binary))
            if (
                not helper.is_file()
                or helper.stat().st_uid != os.getuid()
                or not helper.stat().st_mode & 0o111
            ):
                raise RuntimeError(
                    f"Missing/unsafe source health helper: {helper}; update the source shell deployment with python3 nacre/shell-tools/install.py (full install, without --code-only)"
                )
        elif self.runner is subprocess.run:
            alias = Path("/usr/bin/nacrectl")
            helper = checked(Path("/usr/lib/nacre/nacre-helper"))
            info = alias.lstat()
            if (
                not stat.S_ISLNK(info.st_mode)
                or info.st_uid != 0
                or os.readlink(alias) != "../lib/nacre/nacre-helper"
                or helper.stat().st_uid != 0
            ):
                raise RuntimeError(
                    "Untrusted protocol health binary: /usr/bin/nacrectl"
                )

    def preflight(self, operation, records, recovery=None):
        checked(self.dropin)
        checked(self.state, directory=True)
        units, manager, effective = self.snapshot()
        reasons, layout = self.layout(units)
        if reasons:
            raise RuntimeError("; ".join(reasons))
        roots = shlex.split(self.show().get("UnitPath", ""))
        user_root = self.config / "systemd/user"
        if str(user_root) not in roots:
            raise RuntimeError(
                f"Drop-in directory outside manager UnitPath: {user_root}"
            )
        if manager.get(KEY) not in (None, "0", "1"):
            raise RuntimeError(f"Conflicting manager {KEY}: {manager.get(KEY)!r}")
        current = file_state(self.dropin)
        owned = any(
            r.get("status") != "rolled-back"
            and r.get("desired") == current
            and current is not None
            for r in records
        )
        if recovery is not None:
            if current not in (recovery["desired"], recovery["prior"]["file"]):
                raise RuntimeError(f"Drop-in changed since transaction: {self.dropin}")
            owned = True  # manager may still cache the previous file until reload
        if current is not None and not owned:
            raise RuntimeError(f"Unmanaged drop-in path: {self.dropin}")
        for unit, values in units.items():
            activity = {"active", "inactive"}
            if recovery is not None:
                activity.update(("failed", "activating", "deactivating"))
            if (
                values.get("LoadState") != "loaded"
                or values.get("ActiveState") not in activity
            ):
                raise RuntimeError(f"Unsupported unit state: {unit}")
            if values.get("UnitFileState") not in {"enabled", "disabled", "static"}:
                raise RuntimeError(
                    f"Unsupported enablement: {unit}: {values.get('UnitFileState')}"
                )
            if unit == SOCKET and values["UnitFileState"] == "static":
                raise RuntimeError(f"Unsupported static socket: {unit}")
            if any(
                values.get(p)
                for p in ("EnvironmentFiles", "PassEnvironment", "UnsetEnvironment")
            ):
                raise RuntimeError(f"Unresolved environment inputs: {unit}")
            for path in shlex.split(values.get("DropInPaths", "")):
                if path == str(self.dropin) and unit == SHELL and owned:
                    continue
                raw = read(Path(path))
                if unit != SHELL or KEY.encode() in raw or b"EnvironmentFile" in raw:
                    raise RuntimeError(f"Foreign/conflicting drop-in: {path}")
            if unit != SHELL:
                for prop in ("WantedBy", "RequiredBy", "TriggeredBy"):
                    allowed = (
                        {SOCKET} if unit == DAEMON and prop == "TriggeredBy" else set()
                    )
                    if unit == SOCKET and prop == "RequiredBy":
                        allowed = {DAEMON}
                    if unit == SOCKET and prop == "WantedBy":
                        allowed = {"graphical-session.target"}
                    unknown = set(values.get(prop, "").split()) - allowed
                    if unknown:
                        raise RuntimeError(
                            f"Independent {unit} {prop} owner: {sorted(unknown)[0]}"
                        )
        if (
            current is not None
            and recovery is None
            and base64.b64decode(current["data"]) not in CONTENTS.values()
        ):
            raise RuntimeError(f"Unsupported managed drop-in contents: {self.dropin}")
        expected = (
            environment(
                base64.b64decode(current["data"]).decode().split("Environment=", 1)[1]
            ).get(KEY)
            if current and recovery is None
            else manager.get(KEY)
        )
        allowed = {expected}
        if recovery is not None:
            allowed.update(
                (
                    recovery["prior"]["effective"],
                    "1" if recovery["operation"] == "enable" else "0",
                )
            )
        if effective.get(KEY) not in allowed:
            raise RuntimeError(f"Conflicting effective {SHELL} Environment: {KEY}")
        # Bind the protocol probe to the shell's actual consumer endpoint.
        daemon_env = dict(manager)
        daemon_env.update(environment(units[DAEMON].get("Environment", "")))
        binding = {"HOME": str(self.home)}
        for key in (
            "HOME",
            "XDG_RUNTIME_DIR",
            "XDG_CONFIG_HOME",
            "XDG_STATE_HOME",
            "XDG_CACHE_HOME",
            "NACRE_DAEMON_SOCKET",
        ):
            value = effective.get(key, str(self.home) if key == "HOME" else None)
            other = daemon_env.get(key, str(self.home) if key == "HOME" else None)
            if value != other or value is not None and not Path(value).is_absolute():
                raise RuntimeError(f"Conflicting shell/daemon endpoint: {key}")
            if key == "HOME" and value != str(self.home):
                raise RuntimeError("Manager HOME differs from invoking user HOME")
            if value is not None:
                binding[key] = value
        if "NACRE_DAEMON_SOCKET" not in binding and "XDG_RUNTIME_DIR" not in binding:
            raise RuntimeError("Unresolved daemon endpoint: XDG_RUNTIME_DIR")
        if operation == "enable":
            self.health_binary_check(layout)
        self.startup(roots, allow_socket=True)
        if operation == "disable":
            # Only a completed enable may own an existing active/enabled daemon.
            enables = [
                r
                for r in records
                if r.get("operation") == "enable"
                and r.get("status") == "complete"
                and r["desired"] == current
            ]
            baseline = enables[-1]["prior"]["units"] if enables else units
            for unit in (DAEMON, SOCKET):
                if (
                    baseline[unit]["ActiveState"] != "inactive"
                    or baseline[unit]["UnitFileState"] == "enabled"
                ):
                    raise RuntimeError(f"Independent baseline daemon owner: {unit}")
            self.startup(roots, allow_socket=bool(enables))
        return {
            "units": {
                u: {p: values[p] for p in ("ActiveState", "UnitFileState")}
                for u, values in units.items()
            },
            "manager": manager.get(KEY),
            "effective": effective.get(KEY),
            "file": current,
            "binding": binding,
        }

    def health(self, binding):
        """N10 F4 protocol check: ready daemon, durable stores, stable identity."""
        deadline = time.monotonic() + 12

        def call(method, params):
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                raise RuntimeError("F4 protocol health timed out")
            result = self.runner(
                [
                    *self.health_command,
                    "call",
                    "--timeout",
                    str(max(1, min(3000, int(remaining * 1000)))),
                    method,
                    json.dumps(params),
                ],
                capture_output=True,
                text=True,
                check=True,
                timeout=remaining,
                env=binding,
            )
            if len(result.stdout) > 2 * 1024 * 1024:
                raise RuntimeError("Oversized F4 health response")
            return json.loads(result.stdout)

        def ready(value):
            services = value.get("services", {})
            topics = value.get("topics", [])
            stores = sorted(
                t.removeprefix("settings.store.")
                for t in topics
                if isinstance(t, str) and t.startswith("settings.store.")
            )
            if (
                not isinstance(value.get("boot"), str)
                or not value["boot"]
                or value.get("safe") is not False
                or services.get("settings", {}).get("status") != "running"
                or not stores
                or len(stores) != len(set(stores))
                or any(not re.fullmatch(r"[a-z0-9-]{1,64}", s) for s in stores)
            ):
                raise RuntimeError("F4 daemon status is not ready")
            return value["boot"], stores

        observed_boot = None
        backoff = 0.1
        while True:
            status = call("daemon.status", {})
            boot = status.get("boot")
            if isinstance(boot, str) and boot:
                if observed_boot is not None and boot != observed_boot:
                    raise RuntimeError(
                        "F4 daemon identity changed during readiness polling"
                    )
                observed_boot = boot
            try:
                initial = ready(status)
                break
            except RuntimeError:
                remaining = deadline - time.monotonic()
                if remaining <= 0:
                    raise RuntimeError("F4 protocol health timed out") from None
                time.sleep(min(backoff, remaining))
                backoff = min(0.5, backoff * 2)
        for store in initial[1]:
            value = call("settings.get", {"store": store})
            rev = value.get("rev")
            if (
                value.get("store") != store
                or type(rev) is not int
                or not 0 <= rev < 2**64
                or not isinstance(value.get("exists"), bool)
                or "data" not in value
                or value.get("error") is not None
                or value.get("dirty") is not False
            ):
                raise RuntimeError(f"F4 settings snapshot is not durable: {store}")
        if ready(call("daemon.status", {})) != initial:
            raise RuntimeError("F4 daemon identity changed during health verification")

    def verify(self, value, *, prior=None):
        units, manager, effective = self.snapshot()
        if prior:
            if (
                file_state(self.dropin) != prior["file"]
                or manager.get(KEY) != prior["manager"]
                or effective.get(KEY) != prior["effective"]
            ):
                raise RuntimeError("Rollback file/environment verification failed")
            for unit in UNITS:
                for prop in ("ActiveState", "UnitFileState"):
                    if units[unit][prop] != prior["units"][unit][prop]:
                        raise RuntimeError(
                            f"Rollback verification failed: {unit} {prop}"
                        )
            return
        if units[SHELL]["ActiveState"] != "active" or effective.get(KEY) != value:
            raise RuntimeError(f"Shell health failed: expected {KEY}={value}")
        if value == "0":
            if (
                any(units[u]["ActiveState"] != "inactive" for u in (DAEMON, SOCKET))
                or units[SOCKET]["UnitFileState"] != "disabled"
            ):
                raise RuntimeError("Disable health failed: daemon activation remains")
            if units[DAEMON]["UnitFileState"] == "enabled":
                raise RuntimeError(
                    "Disable health failed: independent daemon enablement"
                )
            self.startup(
                shlex.split(self.show().get("UnitPath", "")), allow_socket=False
            )
        else:
            self.health(self.binding)

    def restore(self, record):
        prior = record["prior"]
        if file_state(self.dropin) not in (prior["file"], record["desired"]):
            raise RuntimeError(f"Drop-in changed during transaction: {self.dropin}")
        if environment(self.ctl("show-environment")).get(KEY) != prior["manager"]:
            raise RuntimeError(f"Manager {KEY} changed during transaction")
        record["status"] = "restoring"
        self.save(record)

        def restore_file():
            saved = prior["file"]
            if saved is None:
                if self.dropin.exists():
                    self.dropin.unlink()
                    syncdir(self.dropin.parent)
            else:
                atomic(self.dropin, base64.b64decode(saved["data"]), saved["mode"])

        self.phase(record, "restore-file", restore_file)
        self.phase(record, "restore-reload", lambda: self.ctl("daemon-reload"))
        enabled = prior["units"][SOCKET]["UnitFileState"] == "enabled"
        self.phase(
            record,
            "restore-enablement",
            lambda: self.ctl("enable" if enabled else "disable", SOCKET),
        )
        # Stop activation during restoration; restore exact activity after shell restart.
        self.phase(record, "restore-stop-socket", lambda: self.ctl("stop", SOCKET))
        self.phase(record, "restore-shell", lambda: self.ctl("restart", SHELL))
        if prior["units"][SHELL]["ActiveState"] == "inactive":
            self.phase(record, "restore-stop-shell", lambda: self.ctl("stop", SHELL))
        for unit, name in ((SOCKET, "socket"), (DAEMON, "daemon")):
            active = prior["units"][unit]["ActiveState"] == "active"
            self.phase(
                record,
                "restore-" + name,
                lambda u=unit, a=active: self.ctl("start" if a else "stop", u),
            )
        self.phase(record, "restore-verify", lambda: self.verify(None, prior=prior))
        record["status"] = "rolled-back"
        record["changedBoot"] = record["boot"] != boot_id()
        self.save(record)
        return {
            "transaction": record["id"],
            "status": record["status"],
            "changedBoot": record["changedBoot"],
        }

    def run(self, operation, transaction=None):
        records = self.records()
        if operation == "status":
            units, manager, effective = self.snapshot()
            reasons, layout = self.layout(units)
            if layout:
                try:
                    self.health_binary_check(layout)
                except (OSError, RuntimeError, ValueError) as error:
                    reasons.append(str(error))
            if not reasons:
                try:
                    self.preflight("enable", records)
                except (OSError, RuntimeError, ValueError) as error:
                    reasons.append(str(error))
            if any(r["status"] not in {"complete", "rolled-back"} for r in records):
                reasons.append("Incomplete F4 transaction; rollback before enabling")
            return {
                "ready_to_enable": list(dict.fromkeys(reasons)),
                "mode": effective.get(KEY, "legacy"),
                "effective": effective.get(KEY),
                "manager": manager.get(KEY),
                "units": {
                    u: {
                        p: s.get(p)
                        for p in ("FragmentPath", "ActiveState", "UnitFileState")
                    }
                    for u, s in units.items()
                },
                "incomplete": [
                    {"id": r["id"], "phase": r["phase"]}
                    for r in records
                    if r["status"] not in {"complete", "rolled-back"}
                ],
            }
        if operation != "rollback" and any(
            r["status"] not in {"complete", "rolled-back"} for r in records
        ):
            raise RuntimeError(
                "Incomplete F4 transaction; run nacre f4 rollback --transaction ID"
            )
        record = None
        if operation == "rollback":
            candidates = [
                r for r in records if transaction is None or r["id"] == transaction
            ]
            if not candidates:
                raise RuntimeError("No matching F4 transaction")
            record = candidates[-1]
        baseline = self.preflight(operation, records, recovery=record)
        self.binding = baseline["binding"]
        if operation == "rollback":
            if baseline["manager"] != record["prior"]["manager"]:
                raise RuntimeError(
                    f"Manager {KEY} changed since transaction {record['id']}"
                )
            if baseline["file"] not in (record["desired"], record["prior"]["file"]):
                raise RuntimeError(f"Drop-in changed since transaction: {self.dropin}")
            return self.restore(record)
        value = "1" if operation == "enable" else "0"
        same = [
            r
            for r in records
            if r["operation"] == operation
            and r["status"] == "complete"
            and r["desired"] == baseline["file"]
        ]
        if same:
            self.verify(value)
            return {"transaction": same[-1]["id"], "status": "complete", "mode": value}
        desired = {"data": base64.b64encode(CONTENTS[value]).decode(), "mode": 0o600}
        record = {
            "id": uuid.uuid4().hex,
            "created": time.time_ns(),
            "operation": operation,
            "dropin": str(self.dropin),
            "prior": baseline,
            "desired": desired,
            "boot": boot_id(),
            "status": "applying",
            "phase": "prepared",
            "pending": False,
        }
        # Parents are private; persist their directory entries before any activation.
        checked(self.index, directory=True)
        mkdir(self.index)
        (self.index / record["id"]).mkdir(mode=0o700)
        syncdir(self.index.parent)
        syncdir(self.index)
        self.save(record)
        try:
            self.phase(
                record, "write-dropin", lambda: atomic(self.dropin, CONTENTS[value])
            )
            self.phase(record, "reload", lambda: self.ctl("daemon-reload"))
            self.phase(
                record,
                "socket",
                lambda: self.ctl(
                    "enable" if value == "1" else "disable", "--now", SOCKET
                ),
            )
            if value == "0":
                self.phase(record, "stop-daemon", lambda: self.ctl("stop", DAEMON))
            self.phase(record, "restart-shell", lambda: self.ctl("restart", SHELL))
            self.phase(record, "health", lambda: self.verify(value))
            record["status"] = "complete"
            self.save(record)
        except Exception as error:
            try:
                self.restore(record)
            except Exception as recovery:  # noqa: BLE001 - persist recovery failures for CLI
                raise RuntimeError(
                    f"F4 failed: {error}; rollback incomplete: {recovery}; recover transaction {record['id']}"
                ) from error
            raise RuntimeError(
                f"F4 failed: {error}; prior state restored; transaction {record['id']}"
            ) from error
        return {"transaction": record["id"], "status": "complete", "mode": value}


def boot_id():
    return Path("/proc/sys/kernel/random/boot_id").read_text().strip()


def require_ordinary_user():
    if os.getuid() == 0:
        raise RuntimeError("nacre f4 must run as the ordinary desktop user, never sudo")


def dispatch(args, home, release_state):
    parser = argparse.ArgumentParser(prog="nacre f4")
    parser.add_argument(
        "operation", choices=("status", "enable", "disable", "rollback")
    )
    parser.add_argument("--transaction")
    parsed = parser.parse_args(args)
    if parsed.transaction and (
        parsed.operation != "rollback"
        or not re.fullmatch(r"[0-9a-f]{32}", parsed.transaction)
    ):
        parser.error(
            "--transaction requires rollback and a 32-character transaction ID"
        )
    require_ordinary_user()
    # Existing release lock only: refusals and status must not create any files.
    lock_path = checked(Path(release_state) / "release.lock")
    fd = os.open(lock_path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd) as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        state_home = Path(
            os.environ.get("XDG_STATE_HOME") or Path(home) / ".local/state"
        )
        switch = Switch(home, state_home / "nacre")
        return switch.run(parsed.operation, parsed.transaction)
