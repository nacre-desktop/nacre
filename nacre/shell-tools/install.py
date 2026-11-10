#!/usr/bin/env python3
"""Install or reload Nacre native shell source with a reversible cutover."""

import sys
import argparse
import datetime as dt
import fcntl
import hashlib
import json
import os
import re
import shutil
import subprocess
import tempfile
import time
from pathlib import Path

# Refusal must not leave import caches behind in the candidate checkout.
sys.dont_write_bytecode = True

from nacre_i18n import tr  # noqa: E402
import nacre_state as _state  # noqa: E402
import nacre_native_compositor as compositor  # noqa: E402
import niri_session_refresh as session_refresh  # noqa: E402
import writer_lease  # noqa: E402

HOME = Path.home()
ROOT = Path(__file__).resolve().parent
DEST = HOME / ".local/share/nacre/shell"
STATE = HOME / ".local/state/nacre/shell"


RETIRED_HELPERS = (
    "kde-palette",
    "terminal-branding",
    "login-appearance",
    "colors",
    "classic-state",
    "branding",
    "file-icons",
    "lock-config",
    "lock-dashboard",
    "lock-info",
    "lock-prepare",
    "provision",
    "palette_engine",
    "wallpaper-watch",
    "wallpaper-media",
    "demo-wallpapers",
    "helper-service",
    "shell-supervisor",
    "session-watch",
    "diagnostics",
    "menu-backend",
    "dashboard-data",
    "dashboard-media",
    "network-state",
    "launcher-preferences",
    "notification-history",
    "dashboard-system",
    "resource-state",
    "window-chat-title",
    "light-devices",
    "menu-cli",
    "pearl-state",
    "pearl-window",
    "compositor",
)
RETIRED_NIRI = ("niri-session-runtime", "niri-acceptance-monitor", "niri-preview")
MANAGED_UNITS = (
    "nacre-shell.service",
    "nacre-sidebar-ai.service",
    "nacre-session-watch.service",
    "nacre-niri-widths.service",
    "nacre-power-key.service",
    "hypridle.service",
    "nacre-daemon.socket",
    "nacre-daemon.service",
    "swaync.service",
    "waybar.service",
    "nacre-brain-ui-wallpaper.timer",
)
USER_CLIENTS = (
    "nacre-daemon",
    "nacrectl",
    "nacre-app",
    "nacre-settings",
    "nacre-welcome",
    "nacre-desktop-extras",
    "nacre-maintenance",
    "nacre-welcome-state",
)


def niri_session():
    return HOME / ".local/share/nacre/niri-on-pc/session"


DAEMON_UNIT = "nacre-daemon.service"
DAEMON_WAIT_SECONDS = 20


def daemon_active():
    return (
        subprocess.run(
            ["systemctl", "--user", "is-active", "--quiet", DAEMON_UNIT]
        ).returncode
        == 0
    )


def bead_candidate():
    """Return an installed package build or matching source build."""
    repo = ROOT.parents[1]
    packaged = Path("/usr/lib/nacre/ui-shell")
    binary, shell = packaged / "bin/nacre-shell", packaged / "shell"
    if binary.is_file() and shell.is_dir():
        return binary, shell
    target = repo / "target/release"
    binary, shell = target / "nacre-shell", repo / "shell"
    return (binary, shell) if binary.is_file() and shell.is_dir() else None


def ui_switch():
    tools = ROOT.parents[1] / "tools"
    if str(tools) not in sys.path:
        sys.path.insert(0, str(tools))
    import ui_shell_switch

    state_home = Path(os.environ.get("XDG_STATE_HOME") or HOME / ".local/state")
    return ui_shell_switch.UiShell(HOME, state_home / "nacre")


def migrate_ui_shell(candidate=None, *, prepare=False):
    """Converge to the available Bead build after the shell code deploy."""
    binary, shell = candidate if candidate else (None, None)
    switch = ui_switch()
    import releases

    def apply():
        releases.install_controller(ROOT.parents[1], home=HOME)
        if prepare:
            if candidate is None:
                raise RuntimeError(
                    "A Bead build is required to prepare the first login"
                )
            return switch.prepare_bead(binary, shell)
        result = switch.migrate(binary, shell)
        if result.get("status") in {"complete", "recovered"}:
            # The daemon owns the config writer lease for its lifetime. Only
            # retired QML files are moved here, under the release lock.
            with writer_lease.pinned_directory(HOME / ".config/nacre") as config:
                archive_legacy_shell()
                config.ensure_valid()
        return result

    inherited_fd = writer_lease.inherited_fd_from_env()
    if inherited_fd is not None:
        with writer_lease.writer_lease(HOME / ".config/nacre", inherited_fd):
            return apply()
    lock_path = switch.state / "releases/release.lock"
    lock_path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    with lock_path.open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        return apply()


def archive_legacy_shell():
    """Move only known QML runtime paths to a dated backup after Bead is healthy."""
    state = HOME / ".local/state/nacre/shell/backups"
    stamp = dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    backup = state / stamp / "legacy-shell"
    moves = []
    qml_trees = {
        str(DEST / "source"),
        str(ROOT.parent / "shell"),
        "/usr/share/nacre/shell",
        "/usr/share/nacre/nacre/shell",
    }
    paths = [
        (DEST / "source", "source"),
        (DEST / "source.previous", "source.previous"),
        (DEST / "source.good", "source.good"),
        (DEST / "source.good.next", "source.good.next"),
    ]
    for path, name in paths:
        if path.is_symlink() and os.readlink(path) not in qml_trees:
            continue
        if path.exists() or path.is_symlink():
            target = backup / name
            target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            os.replace(path, target)
            moves.append({"from": str(path), "to": str(target)})

    quickshell = HOME / ".config/quickshell/nacre"
    if quickshell.is_symlink() and os.readlink(quickshell) in qml_trees:
        target = backup / "config-quickshell-nacre"
        target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.replace(quickshell, target)
        moves.append({"from": str(quickshell), "to": str(target)})

    known_scripts = {
        DEST
        / "bin/qs": "b362c8be57cb3c59fbe1099a3678af8e81ce509525adabab4209fb8ec7e18627",
        DEST
        / "tools/launch.sh": "b362c8be57cb3c59fbe1099a3678af8e81ce509525adabab4209fb8ec7e18627",
        DEST
        / "tools/qt-check.sh": "86f2c2b17baa678680ab014df327f9276926f017f826ba444c7b6dc7ebb908b4",
    }
    for path, expected in known_scripts.items():
        if path.is_file() and not path.is_symlink() and digest(path) == expected:
            target = backup / "wrappers" / path.name
            target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            os.replace(path, target)
            moves.append({"from": str(path), "to": str(target)})
    old_dbus = HOME / ".local/share/dbus-1/services/org.erikreider.swaync.service"
    if old_dbus_matches(old_dbus):
        target = backup / "old-notification-service"
        target.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        os.replace(old_dbus, target)
        moves.append({"from": str(old_dbus), "to": str(target)})
    config_backup = retire_unused_config()
    if config_backup:
        moves.append(
            {"from": str(HOME / ".config/nacre/shell.json"), "to": str(config_backup)}
        )
    if moves:
        backup.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        _state.write(backup.parent / "manifest.json", moves)
        pointer = state / "latest-legacy-shell-backup"
        pointer.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        pointer.write_text(str(backup.parent))
        pointer.chmod(0o600)
    return backup.parent if moves else None


def install_control_dispatcher():
    """Update the shared nacre-shell command dispatcher and retain its prior bytes."""
    source = ROOT / "control.sh"
    target = HOME / ".local/bin/nacre-shell"
    if not source.is_file() or source.is_symlink():
        raise RuntimeError("The Bead-compatible nacre-shell dispatcher is unavailable")
    target.parent.mkdir(parents=True, exist_ok=True)
    if target.is_symlink():
        raise RuntimeError("Refusing to replace a linked nacre-shell dispatcher")
    if target.is_file() and digest(target) == digest(source):
        return None
    backup = None
    if target.is_file():
        backup = (
            STATE
            / "backups"
            / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
            / "nacre-shell"
        )
        backup.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        shutil.copy2(target, backup)
    fd, name = tempfile.mkstemp(prefix=".nacre-shell.", dir=target.parent)
    os.close(fd)
    pending = Path(name)
    try:
        shutil.copy2(source, pending)
        os.chmod(pending, 0o755)
        os.replace(pending, target)
    finally:
        pending.unlink(missing_ok=True)
    return backup


def daemon_status_healthy(text):
    """Services present and none failed, topics advertised, not in safe mode."""
    try:
        status = json.loads(text)
        services = status["services"]
        return (
            status.get("safe") is not True
            and bool(services)
            and all(
                s.get("status") not in ("error", "refused") for s in services.values()
            )
            and bool(status["topics"])
        )
    except (ValueError, KeyError, TypeError, AttributeError):
        return False


def wait_daemon_healthy(timeout=None):
    deadline = time.monotonic() + (DAEMON_WAIT_SECONDS if timeout is None else timeout)
    while True:
        try:
            result = subprocess.run(
                [str(DEST / "bin/nacrectl"), "status"],
                capture_output=True,
                text=True,
                timeout=3,
            )
            if result.returncode == 0 and daemon_status_healthy(result.stdout):
                return True
        except (subprocess.TimeoutExpired, OSError):
            pass
        if time.monotonic() >= deadline:
            return False
        time.sleep(0.5)


def save_previous_tools():
    """Snapshot the owned shared tools/ runtime for rollback."""
    saved = DEST / "tools.previous"
    if saved.exists():
        shutil.rmtree(saved)
    if (DEST / "tools").is_dir():
        shutil.copytree(DEST / "tools", saved, symlinks=True)
        return True
    return False


def restore_previous_trees():
    """Restore the shared tools/ runtime after a failed daemon restart."""
    current, saved = DEST / "tools", DEST / "tools.previous"
    if current.exists() or current.is_symlink():
        shutil.rmtree(current)
    if saved.exists():
        saved.rename(current)


def restart_daemon_checked(target, previous, trees=False):
    """Restart the running daemon on the new helper; restore the old install on failure.

    With `trees` the previous shared tools/ runtime comes back too."""
    print("Restarting " + DAEMON_UNIT + " so it runs the new helper binary.")
    restarted = (
        subprocess.run(["systemctl", "--user", "restart", DAEMON_UNIT]).returncode == 0
    )
    if restarted and wait_daemon_healthy():
        print(
            DAEMON_UNIT
            + " restarted and healthy (services running, topics advertised)."
        )
        return
    print(
        "The restarted daemon did not become healthy; restoring the previous helper.",
        file=sys.stderr,
    )
    if previous is None or not previous.is_file():
        raise RuntimeError(
            "Daemon unhealthy after restart and no previous helper backup exists; "
            "check: systemctl --user status " + DAEMON_UNIT
        )
    if trees:
        restore_previous_trees()
        install_daemon_links(target)
    install_native_helper(previous, target)
    subprocess.run(["systemctl", "--user", "restart", DAEMON_UNIT])
    healthy = wait_daemon_healthy()
    if healthy:
        raise RuntimeError(
            "The new helper left the daemon unhealthy. The previous helper "
            + ("and the previous shared tools were" if trees else "was")
            + " restored and the daemon is healthy again; the new install is not active."
        )
    raise RuntimeError(
        "The daemon is unhealthy with both the new and the previous helper. "
        "Previous helper restored at " + str(target) + " ; check: "
        "systemctl --user status " + DAEMON_UNIT
    )


def save_previous_helper(candidate, target):
    """Own backup of the installed helper, only when the candidate differs."""
    if not target.is_file() or digest(candidate) == digest(target):
        return None
    saved = STATE / "helper-previous/nacre-helper"
    saved.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    shutil.copy2(target, saved)
    return saved


def repo_revision():
    try:
        return subprocess.run(
            ["git", "-C", str(ROOT.parent.parent), "rev-parse", "HEAD"],
            capture_output=True,
            text=True,
            timeout=10,
        ).stdout.strip()
    except (OSError, subprocess.TimeoutExpired):
        return ""


def niri_session_refresh_plan():
    """Return None when not applicable, else (reason_or_empty, command, stock)."""
    marker = niri_session() / "source-revision"
    if not marker.is_file():
        return None
    revision = repo_revision()
    if not revision:
        return None
    try:
        current = marker.read_text().strip()
    except OSError:
        return None
    if current == revision:
        return None
    stock = (niri_session() / "stock-niri").is_file()
    command = [
        str(DEST / "tools/nacre-helper"),
        "niri-session-runtime",
        "--backend",
        str(session_refresh.backend_root(home=HOME)),
    ]
    if stock:
        command.append("--stock-niri")
    command.append("prepare")
    return current[:12] + " -> " + revision[:12], command, stock


def install_session_backend(source_root):
    """Install the session inputs beside the shell tools, independent of checkout."""
    source_root = Path(source_root)
    tools = DEST / "tools"
    backend = tools / "backend"
    pending = tools / "backend.next"
    if pending.exists():
        shutil.rmtree(pending)
    (pending / "nacre").mkdir(parents=True)
    (pending / "nacre/shell-tools").symlink_to("../..")
    for name in ("hypr", "kitty", "fastfetch", "fish", "uwsm", "niri"):
        source = source_root / "config" / name
        if source.is_dir():
            shutil.copytree(source, pending / name, symlinks=True)
    for name in ("bin", "brain"):
        source = source_root / name
        if source.is_dir():
            shutil.copytree(source, pending / name, symlinks=True)
    source = source_root / "nacre/desktop"
    if source.is_dir():
        shutil.copytree(source, pending / "nacre/desktop", symlinks=True)
    source = source_root / "ai/systemd"
    if source.is_dir():
        shutil.copytree(source, pending / "ai/systemd", symlinks=True)
    source = source_root / "tools/patches/niri-26.04-overview-hover.patch"
    if source.is_file():
        (pending / "tools/patches").mkdir(parents=True)
        shutil.copy2(source, pending / "tools/patches" / source.name)
    source = source_root / "tools/build-helper.sh"
    if source.is_file():
        (pending / "tools").mkdir(exist_ok=True)
        shutil.copy2(source, pending / "tools/build-helper.sh")
    version = source_root / "VERSION"
    if version.is_file():
        shutil.copy2(version, pending / "VERSION")
    revision = repo_revision()
    if revision:
        (pending / "nacre-backend-revision").write_text(revision + "\n")
    if backend.exists():
        shutil.rmtree(backend)
    pending.rename(backend)


def print_niri_plan(plan):
    reason, command, _ = plan
    backup = niri_session().with_name("session.rollback")
    print(
        "Plan: refresh the generated niri session config ("
        + reason
        + ").\n  1. copy "
        + str(niri_session())
        + " to "
        + str(backup)
        + " (previous backup replaced)\n  2. run: "
        + " ".join(command)
        + "\n  3. on failure, restore files in place from the backup.\n"
        "  Session/home-view directories stay in place for running mount namespaces.\n"
        "  Your custom.kdl and other user files are not touched. Running niri "
        "reloads config.kdl live; startup items (spawn-at-startup) apply at the "
        "next login."
    )


def refresh_niri_session(plan):
    _, command, _ = plan
    try:
        backup = session_refresh.refresh(
            niri_session(),
            lambda: subprocess.run(command, check=True),
            (lambda: compositor.reload(timeout=None))
            if os.environ.get("NIRI_SOCKET")
            else None,
        )
    except BaseException:
        print(
            "Niri session preparation failed; the previous session config was restored.",
            file=sys.stderr,
        )
        raise
    print(
        "Niri session config refreshed (previous kept at "
        + str(backup)
        + "). Running niri reloads config.kdl live; startup items apply at next login."
    )


def refresh_niri_session_if_changed():
    plan = niri_session_refresh_plan()
    if plan:
        print_niri_plan(plan)
        refresh_niri_session(plan)


def dry_run():
    """Print the Bead/shared-runtime steps a code-only update would take."""
    helper = ROOT / "nacre-helper"
    target = DEST / "tools/nacre-helper"
    if not helper.is_file():
        changed = "unknown (the helper would be built first); assumed changed"
    elif target.is_file() and digest(helper) == digest(target):
        changed = "no"
    else:
        changed = "yes"
    print("Dry run (code-only): nothing is changed.")
    candidate = bead_candidate()
    print("Plan: activate the packaged/source Bead candidate and verify its provider.")
    if candidate is None:
        print("Plan: stop before changing files; no Bead candidate is available.")
    else:
        print("Bead candidate: " + str(candidate[0]))
    print("Helper binary changed: " + changed)
    if changed == "no":
        print("Plan: leave " + DAEMON_UNIT + " as is.")
    elif daemon_active():
        print(
            "Plan: restart "
            + DAEMON_UNIT
            + " after the new helper is in place, wait up to "
            + str(DAEMON_WAIT_SECONDS)
            + "s for nacrectl status to report healthy; if it does not, restore the "
            "previous helper and shared tools together, then restart the daemon again."
        )
    else:
        print("Plan: " + DAEMON_UNIT + " is not active; nothing to restart.")
    plan = niri_session_refresh_plan()
    if plan:
        print_niri_plan(plan)
    else:
        print("Plan: niri session config unchanged or not a prepared niri session.")


def refresh_orient(helper):
    state = HOME / ".local/state/nacre"
    scheme = _state.read_json_file(state / "scheme.json")
    # Fixed presets are resolved by the existing publisher, not by this command.
    subprocess.run(
        [
            str(helper),
            "nacre_shell",
            "scheme",
            "set",
            "-m",
            scheme.get("mode", "dark"),
        ],
        check=True,
    )


def digest(p):
    return hashlib.sha256(p.read_bytes()).hexdigest() if p.is_file() else None


def unused_config_matches(path):
    return (
        path.is_file()
        and not path.is_symlink()
        and digest(path)
        == "05a4a1ffca4933456163dc394e2fe645457787304d2ea715a970b4da84cfe0f1"
    )


def old_dbus_matches(path):
    return path.is_file() and "start nacre-shell.service" in path.read_text()


def retire_unused_config():
    path = HOME / ".config/nacre/shell.json"
    if not path.is_file() or path.is_symlink():
        return None
    if not unused_config_matches(path):
        print("Unused shell.json has local edits; leaving it private and inactive")
        return None
    saved = (
        STATE
        / "backups"
        / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
        / "shell.json"
    )
    saved.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    shutil.copy2(path, saved)
    path.unlink()
    return saved


def install_native_helper(helper, target):
    if helper.resolve() == target.resolve():
        return
    fd, name = tempfile.mkstemp(prefix=".nacre-helper.", dir=target.parent)
    os.close(fd)
    temporary = Path(name)
    try:
        shutil.copy2(helper, temporary)
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)


def install_daemon_links(target):
    """Native aliases in the owned runtime; no user unit activation."""
    directory = target.parent.parent / "bin"
    directory.mkdir(parents=True, exist_ok=True)
    aliases = [directory / name for name in ("nacrectl", "nacre-daemon")]
    for alias in aliases:
        if alias.is_symlink() and os.readlink(alias) == "../tools/nacre-helper":
            continue
        if alias.exists() or alias.is_symlink():
            raise RuntimeError("Refusing to replace an unmanaged daemon command")
    for alias in aliases:
        if not alias.is_symlink():
            alias.symlink_to("../tools/nacre-helper")


def install_user_client_links():
    """Point the public CLI aliases at the atomically installed native helper."""
    directory = HOME / ".local/bin"
    if directory.is_symlink():
        raise RuntimeError("Refusing linked user command directory: " + str(directory))
    directory.mkdir(parents=True, exist_ok=True)
    for name in USER_CLIENTS:
        alias = directory / name
        desired = Path("../share/nacre/shell/tools/nacre-helper")
        if alias.is_symlink() and os.readlink(alias) == str(desired):
            continue
        if alias.is_file() and not alias.is_symlink():
            backup = STATE / "backups" / (name + ".pre-native")
            if backup.parent.is_symlink():
                raise RuntimeError("Refusing linked client backup directory")
            backup.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
            if backup.is_symlink() or (backup.exists() and not backup.is_file()):
                raise RuntimeError("Refusing unsafe client backup path: " + str(backup))
            if not backup.exists():
                fd, name = tempfile.mkstemp(prefix=".client-backup-", dir=backup.parent)
                os.close(fd)
                temporary = Path(name)
                try:
                    shutil.copy2(alias, temporary)
                    os.chmod(temporary, 0o600)
                    try:
                        os.link(temporary, backup)
                    except FileExistsError:
                        pass
                finally:
                    temporary.unlink(missing_ok=True)
        with tempfile.TemporaryDirectory(prefix=".nacre-link-", dir=directory) as stage:
            pending = Path(stage) / alias.name
            pending.symlink_to(desired)
            os.replace(pending, alias)


def retire_native_helpers(directory):
    for path in retired_paths(directory):
        path.unlink(missing_ok=True)


def retired_paths(directory=None):
    directory = DEST / "tools" if directory is None else directory
    return [directory / (name + ".py") for name in RETIRED_HELPERS] + [
        directory.parent.parent / "niri-on-pc" / (name + ".py") for name in RETIRED_NIRI
    ]


def user_unit_directories():
    """Inspect user search locations without querying or changing the manager."""
    config = Path(os.environ.get("XDG_CONFIG_HOME", HOME / ".config"))
    data = Path(os.environ.get("XDG_DATA_HOME", HOME / ".local/share"))
    directories = [
        config / "systemd/user.control",
        config / "systemd/user",
        HOME / ".config/systemd/user",
        data / "systemd/user",
    ]
    runtime = os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}")
    if runtime:
        directories += [
            Path(runtime) / "systemd" / suffix
            for suffix in (
                "user.control",
                "transient",
                "generator.early",
                "user",
                "generator",
                "generator.late",
            )
        ]
    for variable, default in (
        ("XDG_CONFIG_DIRS", "/etc/xdg"),
        ("XDG_DATA_DIRS", "/usr/local/share:/usr/share"),
    ):
        directories += [
            Path(part) / "systemd/user"
            for part in os.environ.get(variable, default).split(":")
            if part
        ]
    directories += [
        Path(part)
        for part in (
            "/etc/systemd/user",
            "/run/systemd/user",
            "/usr/local/lib/systemd/user",
            "/usr/lib/systemd/user",
        )
    ]
    directories += [
        Path(part)
        for part in os.environ.get("SYSTEMD_UNIT_PATH", "").split(":")
        if part
    ]
    return list(dict.fromkeys(directories))


def retired_dropins(retire_niri_experiment=False):
    """Fail closed on retired code references; return only opted-in Niri files."""
    names = {"service.d", "socket.d", "timer.d"}
    for unit in MANAGED_UNITS:
        stem, kind = unit.rsplit(".", 1)
        names.add(unit + ".d")
        # systemd also loads dash-prefix drop-ins (e.g. nacre-.service.d).
        for index, character in enumerate(stem):
            if character == "-":
                names.add(stem[: index + 1] + "." + kind + ".d")
    paths = retired_paths() + [DEST / "tools/demo-wallpapers.LICENSE"]
    config = HOME / ".config/nacre/shell.json"
    if unused_config_matches(config):
        paths.append(config)
    dbus = HOME / ".local/share/dbus-1/services/org.erikreider.swaync.service"
    if old_dbus_matches(dbus):
        paths.append(dbus)
    niri_paths = set(retired_paths()[-len(RETIRED_NIRI) :])
    matches = {}
    for directory in user_unit_directories():
        for name in sorted(names):
            try:
                dropins = sorted((directory / name).iterdir())
            except FileNotFoundError:
                continue
            for dropin in dropins:
                if not dropin.name.endswith(".conf"):
                    continue
                # A /dev/null mask has no content and must never be moved.
                if dropin.is_symlink() and dropin.resolve() == Path("/dev/null"):
                    continue
                text = dropin.read_text()
                values = []
                continuation = ""
                for line in text.splitlines():
                    if line.lstrip().startswith(("#", ";")):
                        continue
                    line = continuation + line.strip()
                    if line.endswith("\\"):
                        continuation = line[:-1] + " "
                        continue
                    continuation = ""
                    key, separator, value = line.partition("=")
                    if separator and (
                        key.strip().startswith(
                            ("ExecStart", "ExecCondition", "ExecStop", "Environment")
                        )
                    ):
                        values.append(value)
                value = "\n".join(values)
                value = re.sub(
                    r"\\(?:x([0-9a-fA-F]{2})|([0-7]{3})|([abfnrstv\\\"']))",
                    lambda m: (
                        chr(int(m[1], 16))
                        if m[1]
                        else chr(int(m[2], 8))
                        if m[2]
                        else {
                            "a": "\a",
                            "b": "\b",
                            "f": "\f",
                            "n": "\n",
                            "r": "\r",
                            "s": " ",
                            "t": "\t",
                            "v": "\v",
                        }.get(m[3], m[3])
                    ),
                    value,
                )
                for alias in ("%h", "${HOME}", "$HOME", "~"):
                    value = value.replace(alias + "/", str(HOME) + "/")
                references = {
                    path
                    for path in paths
                    if re.search(re.escape(str(path)) + r"(?=$|[^\w./-])", value)
                }
                if references:
                    matches[dropin] = references
    blocked = {
        dropin: paths
        for dropin, paths in matches.items()
        if not retire_niri_experiment or not paths <= niri_paths
    }
    if blocked:
        details = "\n".join(
            f"  {dropin}: {path}"
            for dropin, paths in blocked.items()
            for path in sorted(paths)
        )
        raise RuntimeError(
            "Refusing install before any changes: user unit drop-ins reference retired files:\n"
            + details
            + "\nFor Niri experiment references, rerun this command with --retire-niri-experiment "
            "to back up and remove only matching drop-ins. For other references, edit or remove "
            "each named drop-in to use a supported native command, then rerun. "
            "Do not install while a named retired path remains referenced."
        )
    return sorted(matches)


def backup_retirements(dropins):
    """Persist rollback and hashes before removing experiment files or drop-ins."""
    paths = dropins + retired_paths()[-len(RETIRED_NIRI) :]
    paths = [path for path in paths if path.exists() or path.is_symlink()]
    if not paths:
        return None, []
    STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
    backup = (
        STATE
        / "backups"
        / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
    )
    backup.mkdir(parents=True, mode=0o700)
    entries = []
    for path in paths:
        entry = {"target": str(path), "after_hash": None, "after_link": None}
        if path.is_symlink():
            entry.update(
                type="symlink", link=os.readlink(path), before_hash=digest(path)
            )
        else:
            # Runtime/XDG unit dirs may be outside HOME. Absolute hierarchy avoids collisions.
            saved = backup / "retired" / path.relative_to(path.anchor)
            saved.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, saved)
            entry.update(type="file", backup=str(saved), before_hash=digest(saved))
        entries.append(entry)
    _state.write(backup / "manifest.json", entries)
    (STATE / "latest-backup").write_text(str(backup))
    for path in paths:
        path.unlink()
    return backup, entries


def guard_retired_niri_bindings():
    """Do not remove the facade while a known Niri config still executes it."""
    roots = [
        HOME / ".config/niri/config.kdl",
        HOME / ".config/nacre/niri/config.kdl",
        HOME / ".config/nacre/niri-on-pc/config.kdl",
        HOME / ".local/share/nacre/niri-on-pc/session/config/config.kdl",
    ]
    selected = os.environ.get("NIRI_CONFIG")
    if selected:
        roots.append(Path(selected).expanduser())
    pending, seen, blocked = list(roots), set(), []
    total = 0
    while pending:
        path = pending.pop().resolve()
        if path in seen or not path.is_file():
            continue
        seen.add(path)
        if len(seen) > 256 or path.stat().st_size > 2 * 1024 * 1024:
            raise RuntimeError("Niri configuration inventory exceeds bound")
        text = path.read_text()
        total += len(text)
        if total > 8 * 1024 * 1024:
            raise RuntimeError("Niri configuration inventory exceeds bound")
        # Generated bindings use one spawn per line. Ignore full-line comments;
        # retain shell command strings and both absolute and variable tool paths.
        active = "\n".join(
            line for line in text.splitlines() if not line.lstrip().startswith("//")
        )
        if re.search(r"(?<![\w.-])compositor\.py(?![\w.-])", active):
            blocked.append(path)
        for include in re.findall(
            r'^\s*include(?:\s+optional=true)?\s+"([^"\n]+)"', active, re.MULTILINE
        ):
            child = Path(include).expanduser()
            pending.append(child if child.is_absolute() else path.parent / child)
    if blocked:
        raise RuntimeError(
            "Refusing install before any changes: Niri configs reference retired compositor.py:\n"
            + "\n".join("  " + str(path) for path in sorted(blocked))
            + "\nBack up these configs and migrate their bindings to the generated native Nacre "
            "bindings from compositor-contract.json (niri/bindings.kdl) before installing. The running Niri keeps "
            "its config path: update that file or its includes, then validate and verify reload. "
            "--retire-niri-experiment does not bypass this guard."
        )


def deploy(code_only=False, retire_niri_experiment=False):
    guard_retired_niri_bindings()
    dropins = retired_dropins(retire_niri_experiment)
    candidate = bead_candidate()
    if candidate is None and (not code_only or not ui_switch().migration_launcher()):
        raise RuntimeError(
            "A Bead build is required before updating the shell; install the Nacre package or build the source release first"
        )
    # Resolve/build the native candidate before any source/runtime activation.
    helper = ROOT / "nacre-helper"
    if not helper.is_file():
        repo = ROOT.parent.parent
        helper = Path(
            subprocess.check_output(
                ["bash", str(repo / "tools/build-helper.sh")], text=True
            ).strip()
        )
    if not helper.is_file() or not os.access(helper, os.X_OK):
        raise RuntimeError("The native nacre-helper candidate is unavailable")
    backup, entries = backup_retirements(dropins)
    # The release transaction snapshots the old runtime before this activation.

    # Fonts have their own hash/notice/rollback owner; loaded applications stay alive.
    subprocess.run(["python3", str(ROOT / "font-setup.py"), "--apply"], check=True)
    tools_saved = save_previous_tools() if code_only else False
    shutil.copytree(
        ROOT,
        DEST / "tools",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("__pycache__", "tests", "nacre-helper"),
    )
    target = DEST / "tools/nacre-helper"
    previous_helper = save_previous_helper(helper, target) if code_only else None
    install_native_helper(helper, target)
    install_session_backend(ROOT.parent.parent)
    (DEST / "tools/nacre-backend-root").unlink(missing_ok=True)
    install_daemon_links(target)
    if code_only:
        install_user_client_links()
    # The release transaction already backed up this owned runtime tree.
    retire_native_helpers(DEST / "tools")
    # Retire the superseded procedural-image dedication from owned helper code.
    # The release transaction snapshots this tree before activation.
    (DEST / "tools/demo-wallpapers.LICENSE").unlink(missing_ok=True)
    # Licensed demo artwork is ready offline on fresh installs. Validate/cache the
    # bundled bytes; never import into or overwrite the private library.
    subprocess.run(
        [str(target), "demo-wallpapers"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    # Keep ordinary terminal titles aligned with the desktop labels.
    title = HOME / ".config/fish/functions/fish_title.fish"
    title.parent.mkdir(parents=True, exist_ok=True)
    if title.exists() and title.read_bytes() != (ROOT / "fish_title.fish").read_bytes():
        saved = (
            STATE
            / "backups"
            / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
            / "fish_title.fish"
        )
        saved.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(title, saved)
    shutil.copyfile(ROOT / "fish_title.fish", title)
    shell_service = HOME / ".config/systemd/user/nacre-shell.service"
    shell_service.parent.mkdir(parents=True, exist_ok=True)
    shell_service.unlink(missing_ok=True) if shell_service.is_symlink() else None
    shutil.copyfile(ROOT / "nacre-shell.service", shell_service)
    chat_service = HOME / ".config/systemd/user/nacre-sidebar-ai.service"
    chat_service.parent.mkdir(parents=True, exist_ok=True)
    import importlib.util

    profile_spec = importlib.util.spec_from_file_location(
        "shell_profile", ROOT / "profile-policy.py"
    )
    profile_policy = importlib.util.module_from_spec(profile_spec)
    profile_spec.loader.exec_module(profile_policy)
    if "ai" in profile_policy.load(HOME)["components"]:
        shutil.copyfile(ROOT / "nacre-sidebar-ai.service", chat_service)
    observer = HOME / ".config/systemd/user/nacre-session-watch.service"
    shutil.copyfile(ROOT / "nacre-session-watch.service", observer)
    shutil.copyfile(
        ROOT / "nacre-niri-widths.service",
        HOME / ".config/systemd/user/nacre-niri-widths.service",
    )
    subprocess.run(["python3", str(ROOT / "idle-policy.py")], check=True)
    idle_dropin = HOME / ".config/systemd/user/hypridle.service.d/nacre.conf"
    idle_dropin.parent.mkdir(parents=True, exist_ok=True)
    idle_dropin.write_text(
        "[Service]\nExecStartPre=/usr/bin/python3 %h/.local/share/nacre/shell/tools/idle-policy.py\n"
    )
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    subprocess.run(["python3", str(ROOT / "power-button-policy.py")], check=True)
    subprocess.run(
        ["systemctl", "--user", "enable", "--now", "nacre-power-key.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["systemctl", "--user", "enable", "hypridle.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["systemctl", "--user", "enable", "--now", "nacre-session-watch.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["systemctl", "--user", "enable", "--now", "nacre-niri-widths.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        [str(target), "desktop-settings", "init"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    old_dbus = HOME / ".local/share/dbus-1/services/org.erikreider.swaync.service"
    if old_dbus_matches(old_dbus):
        old_dbus.unlink()
    dbus = HOME / ".local/share/dbus-1/services/org.freedesktop.Notifications.service"
    dbus.parent.mkdir(parents=True, exist_ok=True)
    dbus.write_text(
        "[D-BUS Service]\nName=org.freedesktop.Notifications\nExec=/usr/bin/systemctl --user start nacre-shell.service\nSystemdService=nacre-shell.service\n"
    )
    subprocess.run(
        ["systemctl", "--user", "mask", "--now", "swaync.service", "waybar.service"],
        check=True,
        stdout=subprocess.DEVNULL,
    )
    subprocess.run(["python3", str(ROOT / "install-dolphin.py")], check=True)
    subprocess.run(
        ["systemctl", "--user", "enable", "nacre-shell.service"],
        check=True,
        capture_output=True,
    )
    menu_command = HOME / ".local/bin/nacre-menu"
    menu_command.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(ROOT / "menu.sh", menu_command)
    menu_command.chmod(0o755)
    if code_only:
        install_control_dispatcher()
        for src, dest in [
            ("menu.sh", HOME / ".local/bin/nacre-menu"),
            ("cli-bridge.sh", DEST / "bin/nacre_shell"),
        ]:
            shutil.copyfile(ROOT / src, dest)
            dest.chmod(0o755)
        subprocess.run(["python3", str(ROOT / "install-extras.py")], check=True)
        if (HOME / ".local/state/nacre/scheme.json").exists():
            refresh_orient(target)
            subprocess.run(
                [str(DEST / "tools/nacre-helper"), "classic-state"], check=True
            )
        subprocess.run(["python3", str(ROOT / "isolate-apps.py")], check=True)
        if previous_helper is not None and daemon_active():
            restart_daemon_checked(target, previous_helper, trees=tools_saved)
        result = migrate_ui_shell(candidate)
        if result and result.get("status") not in {"complete", "recovered"}:
            raise RuntimeError(
                result.get("hint", "Bead shell migration did not complete")
            )
        refresh_niri_session_if_changed()
        return
    if backup is None:
        STATE.mkdir(parents=True, exist_ok=True, mode=0o700)
        backup = (
            STATE
            / "backups"
            / dt.datetime.now(dt.timezone.utc).strftime("%Y%m%dT%H%M%S%fZ")
        )
        backup.mkdir(parents=True, mode=0o700)

    def write(path, body=None, link=None, mode=None):
        entry = {"target": str(path), "type": "absent"}
        if path.is_symlink():
            entry.update(type="symlink", link=os.readlink(path))
        elif path.exists():
            saved = backup / path.relative_to(HOME)
            saved.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(path, saved)
            entry.update(type="file", backup=str(saved))
        entries.append(entry)
        path.parent.mkdir(parents=True, exist_ok=True)
        if link:
            with tempfile.TemporaryDirectory(
                prefix=".nacre-link-", dir=path.parent
            ) as stage:
                pending = Path(stage) / path.name
                pending.symlink_to(link)
                os.replace(pending, path)
        else:
            path.write_text(body)
            if mode:
                path.chmod(mode)
        entry["after_link"] = os.readlink(path) if path.is_symlink() else None
        entry["after_hash"] = digest(path)

    for name in ["cli.json"]:
        write(HOME / ".config/nacre" / name, (ROOT / name).read_text())
    write(
        HOME / ".local/bin/nacre-shell",
        (ROOT / "control.sh").read_text(),
        mode=0o755,
    )
    write(DEST / "bin/nacre_shell", (ROOT / "cli-bridge.sh").read_text(), mode=0o755)
    for name in USER_CLIENTS:
        write(
            HOME / ".local/bin" / name,
            link=Path("../share/nacre/shell/tools/nacre-helper"),
        )
    # Files are staged only. Enabling or starting the daemon requires its owner's decision.
    for name in ("nacre-daemon.socket", "nacre-daemon.service"):
        write(HOME / ".config/systemd/user" / name, (ROOT / name).read_text())
    write(HOME / ".config/nacre/rofi.rasi", (ROOT / "rofi.rasi").read_text())
    write(
        HOME / ".config/rofi/config.rasi",
        '@import "' + str(HOME / ".config/nacre/rofi.rasi") + '"\n',
    )
    write(
        HOME / ".config/systemd/user/nacre-shell.service",
        (ROOT / "nacre-shell.service").read_text(),
    )
    write(
        HOME / ".config/systemd/user/nacre-niri-widths.service",
        (ROOT / "nacre-niri-widths.service").read_text(),
    )
    # Apply the selected palette last, after any older profile overrides.
    if compositor.backend_name == "hyprland":
        path = HOME / ".config/hypr/hyprland.lua"
        marker = "-- Nacre committed wallpaper palette"
        config = path.read_text() if path.is_file() else ""
        if marker not in config or 'load_private("palette")' not in config:
            raise RuntimeError(
                tr(
                    "helper.install.managed-palette-loader-missing-run-install-sh-apply-from-the-repo",
                    "Managed palette loader missing; run ./install.sh --apply from the repository",
                )
            )
    # The reference style is code; wallpaper files remain private user assets.
    style = _state.read_json_file(ROOT / "reference-style.json")
    scheme = HOME / ".local/state/nacre/scheme"
    if not (scheme / "current.txt").exists():
        write(
            scheme / "current.txt",
            "\n".join(k + " " + v.lstrip("#") for k, v in style["colours"].items())
            + "\n",
        )
    if not (scheme / "current-mode.txt").exists():
        write(scheme / "current-mode.txt", style["mode"])
    wallpaper = HOME / "Pictures/Wallpapers/Default/default.jpg"
    if (
        wallpaper.exists()
        and not (HOME / ".local/state/nacre/wallpaper/last.txt").exists()
    ):
        write(HOME / ".local/state/nacre/wallpaper/last.txt", str(wallpaper))
    if (HOME / ".local/state/nacre/scheme.json").exists():
        # Palette generation also owns these files; include them in cutover rollback.
        tracked = {e["target"] for e in entries}
        outputs = [
            ".config/nacre/palette.lua",
            ".config/nacre/qt.conf",
            ".config/nacre/kitty-colors.conf",
            ".config/hypr/hyprlock.conf",
            ".config/nacre/colors/primary",
            ".config/nacre/colors/secondary",
            ".config/nacre/colors/onsurface",
            ".config/nacre/colors/onprimary",
            ".config/nacre/colors/surface",
            ".config/nacre/colors/surfacecontainer",
        ]
        outputs += [
            f".config/gtk-{v}/{name}"
            for v in ["3.0", "4.0"]
            for name in ["gtk.css", "settings.ini"]
        ]
        outputs += [f".config/qt{v}ct/qt{v}ct.conf" for v in [5, 6]]
        for rel in outputs:
            path = HOME / rel
            if str(path) in tracked:
                continue
            entry = {"target": str(path), "type": "absent", "after_link": None}
            if path.is_symlink():
                entry.update(type="symlink", link=os.readlink(path))
            elif path.is_file():
                saved = backup / rel
                saved.parent.mkdir(parents=True, exist_ok=True)
                shutil.copy2(path, saved)
                entry.update(type="file", backup=str(saved))
            entries.append(entry)
        subprocess.run([str(DEST / "tools/nacre-helper"), "classic-state"], check=True)
        for entry in entries:
            entry["after_hash"] = digest(Path(entry["target"]))
    _state.write(backup / "manifest.json", entries)
    (STATE / "latest-backup").write_text(str(backup))
    subprocess.run(
        [
            "systemctl",
            "--user",
            "disable",
            "--now",
            "nacre-brain-ui-wallpaper.timer",
        ],
        capture_output=True,
    )
    subprocess.run(["python3", str(ROOT / "install-extras.py")], check=True)
    migrate_ui_shell(candidate, prepare=True)
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    subprocess.run(
        ["systemctl", "--user", "enable", "--now", "nacre-shell.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["systemctl", "--user", "enable", "--now", "nacre-niri-widths.service"],
        check=True,
        capture_output=True,
    )
    compositor.reload(timeout=None)
    subprocess.run(["python3", str(ROOT / "isolate-apps.py")], check=True)
    subprocess.run(
        ["systemctl", "--user", "try-restart", "nacre-niri-widths.service"],
        check=True,
        capture_output=True,
    )
    subprocess.run(
        ["systemctl", "--user", "restart", "nacre-shell.service"], check=True
    )
    result = migrate_ui_shell()
    if result and result.get("status") not in {"complete", "recovered"}:
        raise RuntimeError(result.get("hint", "Bead shell migration did not complete"))
    refresh_niri_session_if_changed()
    print("Native shell installed. Backup:", backup)


def restore():
    backup = Path((STATE / "latest-backup").read_text())
    entries = _state.read_json_file(backup / "manifest.json")
    for e in entries:
        path = Path(e["target"])
        if (
            (path.exists() and not path.is_file() and e["after_link"] is None)
            or digest(path) != e["after_hash"]
            or (os.readlink(path) if path.is_symlink() else None) != e["after_link"]
        ):
            raise RuntimeError(
                tr(
                    "helper.install.later-edit-preserved-value0",
                    "Later edit preserved: {value0}",
                    {"value0": str(path)},
                )
            )
    for e in entries:
        if (
            e["type"] == "file"
            and "before_hash" in e
            and digest(Path(e["backup"])) != e["before_hash"]
        ):
            raise RuntimeError("Retirement backup hash mismatch: " + e["backup"])
    subprocess.run(["systemctl", "--user", "stop", "nacre-shell"], check=True)
    for e in reversed(entries):
        path = Path(e["target"])
        path.parent.mkdir(parents=True, exist_ok=True)
        path.unlink(missing_ok=True)
        if e["type"] == "symlink":
            path.symlink_to(e["link"])
        elif e["type"] == "file":
            shutil.copy2(e["backup"], path)
    subprocess.run(["systemctl", "--user", "daemon-reload"], check=True)
    compositor.reload(timeout=None)
    print("Previous configuration restored; restart its shell service or launcher.")


if __name__ == "__main__":
    p = argparse.ArgumentParser()
    p.add_argument("--code-only", action="store_true")
    p.add_argument("--restore", action="store_true")
    p.add_argument(
        "--dry-run",
        action="store_true",
        help="With --code-only: print the daemon-restart and niri-session steps; change nothing",
    )
    p.add_argument(
        "--retire-niri-experiment",
        action="store_true",
        help="Back up and remove drop-ins referencing retired niri-on-pc scripts",
    )
    a = p.parse_args()
    if a.dry_run and not a.code_only:
        p.error("--dry-run requires --code-only")
    if a.dry_run:
        dry_run()
    else:
        restore() if a.restore else deploy(a.code_only, a.retire_niri_experiment)
