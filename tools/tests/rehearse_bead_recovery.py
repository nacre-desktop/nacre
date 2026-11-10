#!/usr/bin/env python3
"""Disposable dev-1 proof: real Bead, helper daemon and Niri under a user manager."""

import hashlib
import json
import os
from pathlib import Path
import re
import select
import shutil
import subprocess
import sys
import time

ROOT = Path(os.environ.get("N0156_SOURCE_ROOT", "/work"))
ARTIFACTS = Path(os.environ.get("N0156_ARTIFACT_ROOT", str(ROOT)))
HOME = Path.home()
UID = os.getuid()
RUNTIME = Path(os.environ["XDG_RUNTIME_DIR"])
RUN_STARTED = time.strftime("%Y-%m-%d %H:%M:%S")
FRESH_B = os.environ.get("N0156_FRESH_B") == "1"
assert HOME == Path("/home/beadproof") and UID == 1001, (HOME, UID)
assert Path("/.dockerenv").is_file()
assert (ARTIFACTS / "target/release/nacre-shell").is_file()
assert (
    (ROOT / "nacre/shell-tools/nacre-helper").is_file()
    if FRESH_B
    else (ARTIFACTS / "target/release/nacre-helper").is_file()
)
assert (ARTIFACTS / "target/release/bead-wallpaper-video").is_file()
assert (ARTIFACTS / "target/release/nacre-wallpaper-decode").is_file()
assert not (HOME / ".config/systemd/user").exists(), (
    "Use a fresh disposable beadproof HOME"
)
if FRESH_B:
    assert ROOT == Path("/opt/n0156-stage-b"), ROOT
    assert not (ROOT / "nacre/shell").exists(), (
        "B checkout unexpectedly contains the retired QML shell"
    )
    assert not Path("/work/nacre/shell").exists(), (
        "The old A QML tree could mask missing B resources"
    )

ENV = dict(os.environ)
ENV.update(
    HOME=str(HOME),
    USER="beadproof",
    LOGNAME="beadproof",
    XDG_CONFIG_HOME=str(HOME / ".config"),
    XDG_DATA_HOME=str(HOME / ".local/share"),
    XDG_STATE_HOME=str(HOME / ".local/state"),
    XDG_CACHE_HOME=str(HOME / ".cache"),
    XDG_RUNTIME_DIR=str(RUNTIME),
    DBUS_SESSION_BUS_ADDRESS=f"unix:path={RUNTIME}/bus",
    NACRE_COMPOSITOR="niri",
    XDG_CURRENT_DESKTOP="niri",
    XDG_SESSION_TYPE="wayland",
    QT_QPA_PLATFORM="wayland",
    LIBGL_ALWAYS_SOFTWARE="1",
    WLR_RENDERER_ALLOW_SOFTWARE="1",
)
OUT = Path(
    os.environ.get(
        "N0156_PROOF_OUT",
        str(RUNTIME / f"n0156-real-proof-{os.getpid()}-{time.monotonic_ns()}"),
    )
)
OUT.mkdir(parents=True, exist_ok=True)
if FRESH_B:
    assert not any(HOME.iterdir()), "The B fresh-install proof requires an empty HOME"
    assert not (HOME / ".local/share/nacre/shell/source").exists()
    assert not (HOME / ".config/quickshell/nacre").exists()
    print(f"EMPTY_HOME=PASS; SOURCE_ROOT={ROOT}; both QML trees absent", flush=True)
print("RUN_STARTED=" + RUN_STARTED, flush=True)
print(
    "PROOF_SCRIPT_SHA256=" + hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    flush=True,
)
print(
    "SOURCE_COMMIT="
    + subprocess.check_output(
        [
            "git",
            "-c",
            f"safe.directory={ROOT}",
            "-C",
            str(ROOT),
            "rev-parse",
            "HEAD",
        ],
        text=True,
    ).strip(),
    flush=True,
)
if FRESH_B:
    print("ARTIFACT_SOURCE_COMMIT=" + os.environ["N0156_ARTIFACT_COMMIT"], flush=True)
print(
    "INSTALLER_SOURCE_SHA256="
    + hashlib.sha256((ROOT / "nacre/shell-tools/install.py").read_bytes()).hexdigest(),
    flush=True,
)
for artifact in (
    "nacre-shell",
    "bead-wallpaper-video",
    "nacre-wallpaper-decode",
    "nacre-helper",
):
    path = ARTIFACTS / "target/release" / artifact
    if path.is_file():
        print(
            f"ARTIFACT_SHA256={artifact}:"
            + hashlib.sha256(path.read_bytes()).hexdigest(),
            flush=True,
        )
children = []
logs = []


def run(*args, check=True, timeout=30, env=None):
    actual_env = ENV if env is None else env
    result = subprocess.run(
        [str(arg) for arg in args],
        env=actual_env,
        text=True,
        capture_output=True,
        timeout=timeout,
    )
    print("$", " ".join(str(arg) for arg in args), flush=True)
    if result.stdout:
        shown = result.stdout
        if len(shown) > 4096:
            shown = (
                shown[:2048]
                + f"\n... [log display truncated; {len(result.stdout)} chars] ...\n"
                + shown[-1024:]
            )
        print(shown, end="" if shown.endswith("\n") else "\n", flush=True)
    if result.stderr:
        print(
            result.stderr,
            end="" if result.stderr.endswith("\n") else "\n",
            file=sys.stderr,
            flush=True,
        )
    if check and result.returncode:
        raise subprocess.CalledProcessError(
            result.returncode, args, result.stdout, result.stderr
        )
    return result


def spawn(name, command, env=None, pass_fds=()):
    log = (OUT / f"{name}.log").open("w")
    logs.append(log)
    process = subprocess.Popen(
        [str(arg) for arg in command],
        env=ENV if env is None else env,
        stdin=subprocess.DEVNULL,
        stdout=log,
        stderr=subprocess.STDOUT,
        pass_fds=pass_fds,
        start_new_session=True,
    )
    children.append((name, process))
    print(f"Started owned {name}: PID={process.pid} PGID={process.pid}", flush=True)
    return process


def wait_for(description, probe, timeout=45, processes=()):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        for name, process in processes:
            if process.poll() is not None:
                raise RuntimeError(
                    f"{name} exited early with {process.returncode}; inspect {OUT}"
                )
        value = probe()
        if value:
            return value
        time.sleep(0.1)
    raise TimeoutError(f"Timed out waiting for {description}")


def stop_owned(name, process):
    if process.poll() is not None:
        print(
            f"Owned {name} already exited: PID={process.pid} code={process.returncode}",
            flush=True,
        )
        return
    process.terminate()
    try:
        process.wait(timeout=8)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)
    print(f"Stopped owned {name}: PID={process.pid}", flush=True)


def ui(*args, check=True, timeout=90):
    result = run(
        sys.executable,
        ROOT / "tools/releases.py",
        "ui-shell",
        *args,
        check=check,
        timeout=timeout,
    )
    return json.loads(result.stdout) if result.stdout.strip() else {}


def ctl(*args, check=True):
    return run("systemctl", "--user", *args, check=check)


def write(path, data, mode=0o644):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(data)
    path.chmod(mode)
    return path


def writer_lock_diagnostic():
    root = HOME / ".config/nacre"
    lock = root / ".writer.lock"
    path_stats = []
    for path in (Path("/"), HOME.parent, HOME, HOME / ".config", root, lock):
        info = path.stat()
        path_stats.append(
            {
                "path": str(path),
                "uid": info.st_uid,
                "gid": info.st_gid,
                "mode": oct(info.st_mode & 0o7777),
                "nlink": info.st_nlink,
                "dev": f"{os.major(info.st_dev):02x}:{os.minor(info.st_dev):02x}",
                "inode": info.st_ino,
            }
        )
    lock_info = lock.stat()
    identity = (lock_info.st_dev, lock_info.st_ino)
    holders = []
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit():
            continue
        try:
            if entry.stat().st_uid != UID:
                continue
            for fd in (entry / "fd").iterdir():
                try:
                    info = fd.stat()
                    if (info.st_dev, info.st_ino) == identity:
                        holders.append(
                            {
                                "pid": int(entry.name),
                                "fd": fd.name,
                                "command": (entry / "comm").read_text().strip(),
                                "cmdline": (entry / "cmdline")
                                .read_bytes()
                                .replace(bytes([0]), b" ")
                                .decode(errors="replace")
                                .strip(),
                            }
                        )
                except OSError:
                    continue
        except OSError:
            continue
    dev = f"{os.major(lock_info.st_dev):02x}:{os.minor(lock_info.st_dev):02x}:{lock_info.st_ino}"
    locks = [
        line.strip()
        for line in Path("/proc/locks").read_text().splitlines()
        if dev in line
    ]
    print(
        "WRITER_LOCK_DIAGNOSTIC="
        + json.dumps({"paths": path_stats, "holders": holders, "kernel_locks": locks}),
        flush=True,
    )


def provider_probe():
    payload = json.dumps({"target": "nacre", "method": "hide", "args": []})
    result = run(
        HOME / ".local/bin/nacrectl",
        "call",
        "--timeout",
        "3000",
        "ui.dispatch",
        payload,
        check=False,
        timeout=8,
    )
    return result.returncode == 0 and result.stdout.strip() == "null"


try:
    tools = HOME / ".local/share/nacre/shell/tools"
    helper = tools / "nacre-helper"
    source = HOME / ".local/share/nacre/shell/source"
    user_bin = HOME / ".local/bin"
    units = HOME / ".config/systemd/user"
    if FRESH_B:
        helper = ROOT / "nacre/shell-tools/nacre-helper"
        assert helper.is_file() and os.access(helper, os.X_OK), helper
        ENV["NACRE_HELPER_BINARY"] = str(ARTIFACTS / "target/release/nacre-helper")
        ENV["NACRE_HELPER_TOOLS"] = str(ROOT / "nacre/shell-tools")
    else:
        tools.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(
            ROOT / "nacre/shell-tools",
            tools,
            ignore=shutil.ignore_patterns("tests", "__pycache__", "*.pyc"),
        )
        shutil.copy2(ARTIFACTS / "target/release/nacre-helper", helper)
        shutil.copytree(ROOT / "nacre/shell", source)
        old_quickshell = HOME / ".config/quickshell/nacre"
        old_quickshell.parent.mkdir(parents=True, exist_ok=True)
        old_quickshell.symlink_to(source)
        shell_bin = HOME / ".local/share/nacre/shell/bin"
        shell_bin.mkdir(parents=True)
        (shell_bin / "nacre-daemon").symlink_to("../tools/nacre-helper")
        user_bin.mkdir(parents=True)
        for name in ("nacre-daemon", "nacrectl"):
            (user_bin / name).symlink_to("../share/nacre/shell/tools/nacre-helper")
        run(
            sys.executable,
            "-c",
            f"import sys; from pathlib import Path; sys.path.insert(0, {str(ROOT / 'tools')!r}); "
            f"import releases; releases.install_controller(Path({str(ROOT)!r}), home=Path.home())",
        )
        run(sys.executable, ROOT / "tools/configure.py", "--apply")
        ENV["NACRE_HELPER_BINARY"] = str(helper)
        ENV["NACRE_HELPER_TOOLS"] = str(tools)
        units.mkdir(parents=True, exist_ok=True)
        for name in (
            "nacre-shell.service",
            "nacre-daemon.service",
            "nacre-daemon.socket",
        ):
            shutil.copy2(ROOT / "nacre/shell-tools" / name, units / name)
        for name in ("nacre-shell.service", "nacre-daemon.service"):
            write(units / f"{name}.d/90-n0156-proof.conf", "[Service]\nNice=10\n")
        write(
            user_bin / "nacre-shell", "#!/bin/sh\nexec /usr/bin/sleep infinity\n", 0o755
        )

    config = OUT / "niri.kdl"
    config.write_text(
        "input {\n  keyboard {\n    xkb {}\n  }\n}\n"
        "prefer-no-csd\nhotkey-overlay {\n  skip-at-startup\n}\n"
    )
    run("niri", "validate", "--config", config)

    read_fd, write_fd = os.pipe()
    xvfb = spawn(
        "xvfb",
        [
            "Xvfb",
            "-displayfd",
            str(write_fd),
            "-screen",
            "0",
            "1600x1000x24",
            "-nolisten",
            "tcp",
            "-ac",
        ],
        pass_fds=(write_fd,),
    )
    os.close(write_fd)
    ready, _, _ = select.select([read_fd], [], [], 15)
    if not ready:
        raise TimeoutError("Xvfb did not allocate a private display")
    display_id = os.read(read_fd, 64).decode().strip()
    os.close(read_fd)
    ENV["DISPLAY"] = ":" + display_id

    compositor = spawn("niri", ["niri", "--config", config])
    wayland = wait_for(
        "private Niri Wayland socket",
        lambda: next((p for p in RUNTIME.glob("wayland-*") if p.is_socket()), None),
        processes=(("niri", compositor),),
    )
    niri_socket = wait_for(
        "private Niri IPC socket",
        lambda: next(iter(RUNTIME.glob("niri*.sock")), None),
        processes=(("niri", compositor),),
    )
    ENV["WAYLAND_DISPLAY"] = wayland.name
    ENV["NIRI_SOCKET"] = str(niri_socket)

    ctl(
        "set-environment",
        *(
            f"{key}={ENV[key]}"
            for key in (
                "HOME",
                "XDG_CONFIG_HOME",
                "XDG_DATA_HOME",
                "XDG_STATE_HOME",
                "XDG_CACHE_HOME",
                "XDG_RUNTIME_DIR",
                "DBUS_SESSION_BUS_ADDRESS",
                "NACRE_COMPOSITOR",
                "XDG_CURRENT_DESKTOP",
                "XDG_SESSION_TYPE",
                "WAYLAND_DISPLAY",
                "DISPLAY",
                "NIRI_SOCKET",
                "NACRE_HELPER_BINARY",
                "NACRE_HELPER_TOOLS",
                "QT_QPA_PLATFORM",
                "LIBGL_ALWAYS_SOFTWARE",
                "WLR_RENDERER_ALLOW_SOFTWARE",
            )
        ),
    )
    if FRESH_B:
        run(sys.executable, ROOT / "tools/configure.py", "--apply", timeout=240)
        for name in ("nacre-shell.service", "nacre-daemon.service"):
            write(units / f"{name}.d/90-n0156-proof.conf", "[Service]\nNice=10\n")
        ENV["NACRE_HELPER_BINARY"] = str(
            HOME / ".local/share/nacre/shell/tools/nacre-helper"
        )
        ENV["NACRE_HELPER_TOOLS"] = str(tools)
        ctl(
            "set-environment",
            *(
                f"{key}={ENV[key]}"
                for key in (
                    "HOME",
                    "XDG_CONFIG_HOME",
                    "XDG_DATA_HOME",
                    "XDG_STATE_HOME",
                    "XDG_CACHE_HOME",
                    "XDG_RUNTIME_DIR",
                    "DBUS_SESSION_BUS_ADDRESS",
                    "NACRE_COMPOSITOR",
                    "XDG_CURRENT_DESKTOP",
                    "XDG_SESSION_TYPE",
                    "WAYLAND_DISPLAY",
                    "DISPLAY",
                    "NIRI_SOCKET",
                    "NACRE_HELPER_BINARY",
                    "NACRE_HELPER_TOOLS",
                    "QT_QPA_PLATFORM",
                    "LIBGL_ALWAYS_SOFTWARE",
                    "WLR_RENDERER_ALLOW_SOFTWARE",
                )
            ),
        )
        ctl("daemon-reload")
        run(sys.executable, ROOT / "nacre/shell-tools/install.py", timeout=300)
        helper = HOME / ".local/share/nacre/shell/tools/nacre-helper"
        assert helper.is_file() and not helper.is_symlink(), helper
        assert (HOME / ".config/quickshell/nacre").exists() is False
        fresh = ui("status")
        assert fresh.get("active") == "bead" and fresh.get("launcher"), fresh
        assert provider_probe(), (
            "fresh B install did not register a responsive Bead provider"
        )
        print(
            "B_FRESH_INSTALL=configure.py --apply + install.py; empty HOME; provider responsive",
            flush=True,
        )
    else:
        ctl("daemon-reload")
        ctl("enable", "--now", "nacre-shell.service")
        wait_for(
            "legacy QML placeholder service",
            lambda: (
                run(
                    "systemctl",
                    "--user",
                    "is-active",
                    "nacre-shell.service",
                    check=False,
                ).stdout.strip()
                == "active"
            ),
            timeout=15,
            processes=(("niri", compositor), ("xvfb", xvfb)),
        )
        shutil.copy2(ROOT / "nacre/shell-tools/control.sh", user_bin / "nacre-shell")
        (user_bin / "nacre-shell").chmod(0o755)

    binary_a = OUT / "build-a" / "nacre-shell"
    shell_a = OUT / "shell-a"
    binary_a.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ARTIFACTS / "target/release/nacre-shell", binary_a)
    shutil.copy2(
        ARTIFACTS / "target/release/bead-wallpaper-video",
        binary_a.with_name("bead-wallpaper-video"),
    )
    shutil.copy2(
        ARTIFACTS / "target/release/nacre-wallpaper-decode",
        binary_a.with_name("nacre-wallpaper-decode"),
    )
    shutil.copytree(ROOT / "shell", shell_a)
    if FRESH_B:
        status_a = ui("status")
        assert status_a.get("active") == "bead" and not status_a.get(
            "ready_for_bead"
        ), status_a
    else:
        first = ui("migrate", "--binary", binary_a, "--shell", shell_a)
        assert first.get("shell") == "bead", first
        status_a = ui("status")
    launcher_a = status_a.get("launcher")
    assert launcher_a and status_a.get("active") == "bead", status_a
    assert not status_a.get("ready_for_bead"), status_a
    assert provider_probe(), "real Bead provider did not answer through the real daemon"
    ctl("is-active", "nacre-daemon.service")
    daemon_status = run(HOME / ".local/bin/nacrectl", "status", check=False)
    print(
        "DAEMON_STATUS_BEFORE_CUTOVER:",
        daemon_status.stdout,
        daemon_status.stderr,
        flush=True,
    )
    print(f"HEALTHY_A={launcher_a}", flush=True)
    release_match = re.search(
        r"(?m)^export NACRE_UI_RELEASE=([a-fA-F0-9]{32})$", Path(launcher_a).read_text()
    )
    assert release_match, Path(launcher_a).read_text()
    health_file = (
        HOME / ".local/state/bead" / f"ui-health-{release_match.group(1)}.json"
    )
    health_before_stop = json.loads(health_file.read_text())
    assert health_before_stop.get("running") is not None, health_before_stop
    failures_before_stop = list(health_before_stop.get("failures", []))
    ctl("stop", "nacre-shell.service")
    wait_for(
        "clean Bead shutdown marker",
        lambda: (
            health_file.is_file()
            and json.loads(health_file.read_text()).get("running") is None
        ),
        timeout=20,
    )
    health_after_stop = json.loads(health_file.read_text())
    assert health_after_stop.get("failures", []) == failures_before_stop, (
        health_after_stop
    )
    print(
        json.dumps(
            {
                "normal_stop": "clean",
                "ui_health_file": str(health_file),
                "running_after_stop": health_after_stop.get("running"),
                "failures_after_stop": health_after_stop.get("failures", []),
            }
        ),
        flush=True,
    )
    ctl("start", "nacre-shell.service")
    wait_for(
        "Bead provider after normal restart",
        provider_probe,
        timeout=45,
    )

    binary_b = OUT / "build-b" / "nacre-shell"
    shell_b = OUT / "shell-b"
    binary_b.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(ARTIFACTS / "target/release/nacre-shell", binary_b)
    shutil.copy2(
        ARTIFACTS / "target/release/bead-wallpaper-video",
        binary_b.with_name("bead-wallpaper-video"),
    )
    shutil.copy2(
        ARTIFACTS / "target/release/nacre-wallpaper-decode",
        binary_b.with_name("nacre-wallpaper-decode"),
    )
    shutil.copytree(ROOT / "shell", shell_b)
    (shell_b / ".n0156-known-good-proof").write_text(
        "distinct harmless Bead build marker\n"
    )

    # Model the current installation: Bead is live, but its failure unit is stale.
    fallback = units / "nacre-ui-shell-fallback.service"
    old_quickshell = HOME / ".config/quickshell/nacre"
    if FRESH_B:
        assert not source.exists() and not source.is_symlink(), source
        assert not old_quickshell.exists() and not old_quickshell.is_symlink(), (
            old_quickshell
        )
        source.mkdir(parents=True)
        write(source / "legacy.qml", "// harmless retired QML fixture\n")
        old_quickshell.parent.mkdir(parents=True, exist_ok=True)
        old_quickshell.symlink_to(source)
    write(
        fallback,
        "# managed by nacre ui-shell; remove with: nacre ui-shell qml\n"
        "[Unit]\nDescription=Return to the QML shell after a Bead failure\n"
        "[Service]\nType=oneshot\n"
        f"ExecStart={sys.executable} {ROOT / 'tools/releases.py'} ui-shell qml --recover\n",
    )
    write(
        units / "nacre-ui-shell-fallback.service.d/90-n0156-proof.conf",
        "[Service]\nNice=10\n",
    )
    ctl("daemon-reload")
    writer_lock_diagnostic()
    run(
        sys.executable,
        ROOT / "nacre/shell-tools/install.py",
        "--code-only",
        timeout=240,
    )
    assert "ui-shell recover-bead --recover" in fallback.read_text(), (
        fallback.read_text()
    )
    if FRESH_B:
        assert not source.exists() and not source.is_symlink(), source
        assert not old_quickshell.exists() and not old_quickshell.is_symlink(), (
            old_quickshell
        )
        archived_qml = list(
            (HOME / ".local/state/nacre/shell/backups").glob("*/legacy-shell/source")
        )
        assert archived_qml, "code-only update did not archive the legacy QML source"
        print(
            "B_LEGACY_CUTOVER=old QML source and QuickShell link archived; old fallback replaced",
            flush=True,
        )
    assert provider_probe(), (
        "code-only update did not keep the real Bead provider healthy"
    )
    second = ui("migrate", "--binary", binary_b, "--shell", shell_b)
    assert second.get("shell") == "bead", second
    status_b = ui("status")
    launcher_b = status_b.get("launcher")
    assert launcher_b and launcher_b != launcher_a, (launcher_a, launcher_b)
    assert not status_b.get("ready_for_bead"), status_b
    assert provider_probe(), (
        "real candidate Bead provider did not answer before injection"
    )
    assert "ui-shell recover-bead --recover" in fallback.read_text()

    rolled = ui("rollback")
    assert rolled.get("launcher") == launcher_a, rolled
    assert provider_probe(), "real rolled-back Bead provider did not answer"
    run(
        sys.executable,
        ROOT / "nacre/shell-tools/install.py",
        "--code-only",
        timeout=240,
    )
    assert provider_probe(), (
        "code-only update after rollback did not keep the Bead provider healthy"
    )
    assert "ui-shell recover-bead --recover" in fallback.read_text(), (
        fallback.read_text()
    )
    print(
        "B_ROLLBACK_NEXT_UPDATE=actual install.py --code-only kept provider healthy",
        flush=True,
    )
    again = ui("migrate", "--binary", binary_b, "--shell", shell_b)
    assert again.get("shell") == "bead", again
    assert ui("status").get("launcher") == launcher_b
    assert provider_probe(), "real Bead provider did not answer after rollback/update"

    write(launcher_b, "#!/bin/sh\nexit 42\n", 0o700)
    print(f"INJECTED_B_START_FAILURE={launcher_b}", flush=True)
    ctl("restart", "nacre-shell.service", check=False)

    deadline = time.monotonic() + 75
    recovered = None
    while time.monotonic() < deadline:
        candidate = ui("status")
        if (
            candidate.get("active") == "bead"
            and candidate.get("launcher") == launcher_a
            and not candidate.get("ready_for_bead")
            and provider_probe()
        ):
            recovered = candidate
            break
        time.sleep(1)
    assert recovered, (
        "systemd did not recover the live known-good Bead with its provider"
    )
    ctl("is-active", "nacre-shell.service")
    ctl("is-active", "nacre-daemon.service")
    run(HOME / ".local/bin/nacrectl", "status")
    good_recovery_invocation = ctl(
        "show", "nacre-ui-shell-fallback.service", "-p", "InvocationID", "--value"
    ).stdout.strip()
    # With every recorded good copy now deliberately broken, OnFailure must stop cleanly and
    # write one actionable journal hint instead of retrying a recovery loop.
    write(launcher_a, "#!/bin/sh\nexit 43\n", 0o700)
    ctl("restart", "nacre-shell.service", check=False)

    def shell_start_limit():
        return (
            "start-limit-hit"
            in ctl("show", "nacre-shell.service", "-p", "Result", check=False).stdout
        )

    wait_for(
        "shell start limit after all Bead copies fail", shell_start_limit, timeout=45
    )
    start_limit = ctl(
        "show", "nacre-shell.service", "-p", "Result", "-p", "NRestarts"
    ).stdout.strip()
    assert "Result=start-limit-hit" in start_limit, start_limit
    print("NO_GOOD_START_LIMIT:", start_limit, flush=True)

    def fallback_settled():
        status = ctl(
            "show",
            "nacre-ui-shell-fallback.service",
            "-p",
            "ActiveState",
            "-p",
            "Result",
            "-p",
            "ExecMainStatus",
            check=False,
        ).stdout
        return (
            "ActiveState=inactive" in status
            and "Result=success" in status
            and "ExecMainStatus=0" in status
        )

    wait_for("successful terminal fallback completion", fallback_settled, timeout=30)
    assert (
        ctl("is-active", "nacre-shell.service", check=False).stdout.strip() != "active"
    )
    terminal_invocation = ctl(
        "show", "nacre-ui-shell-fallback.service", "-p", "InvocationID", "--value"
    ).stdout.strip()
    if good_recovery_invocation and terminal_invocation:
        assert terminal_invocation != good_recovery_invocation, (
            good_recovery_invocation,
            terminal_invocation,
        )
    shell_restarts = ctl(
        "show", "nacre-shell.service", "-p", "NRestarts", "--value"
    ).stdout.strip()
    fallback_restarts = ctl(
        "show",
        "nacre-ui-shell-fallback.service",
        "-p",
        "NRestarts",
        "--value",
        check=False,
    ).stdout.strip()
    time.sleep(5)
    assert (
        shell_restarts
        == ctl(
            "show", "nacre-shell.service", "-p", "NRestarts", "--value"
        ).stdout.strip()
    )
    assert (
        fallback_restarts
        == ctl(
            "show",
            "nacre-ui-shell-fallback.service",
            "-p",
            "NRestarts",
            "--value",
            check=False,
        ).stdout.strip()
    )
    print(
        json.dumps(
            {
                "no_good_copy": "stopped",
                "terminal_fallback_invocation": terminal_invocation,
                "journal_hint_check": "root system journal query in host wrapper",
                "shell_restart_count_stable_5s": shell_restarts,
                "fallback_restart_count_stable_5s": fallback_restarts,
            }
        ),
        flush=True,
    )
    records = [
        json.loads(path.read_text())
        for path in (HOME / ".local/state/nacre/ui-shell").glob("*/journal.json")
    ]
    assert any(
        row.get("launcher") == launcher_b and row.get("recovery_failed")
        for row in records
    ), records
    print(
        json.dumps(
            {
                "result": "PASS",
                "recovered_launcher": launcher_a,
                "failed_launcher": launcher_b,
                "active_shell": recovered.get("active"),
                "daemon_active": True,
                "provider_probe": "responded",
                "scenarios": [
                    "fresh migration",
                    "old fallback cutover",
                    "rollback then update",
                    "failed start recovers prior Bead",
                    "no good copy stops with a journal hint and no retry loop",
                ],
                "niri_socket": str(niri_socket),
                "xvfb_display": ENV["DISPLAY"],
            },
            indent=2,
        ),
        flush=True,
    )
finally:
    # Stop only units started by this harness, then only the exact compositor/Xvfb PIDs it spawned.
    try:
        ctl("stop", "nacre-shell.service", check=False)
        ctl("stop", "nacre-daemon.socket", "nacre-daemon.service", check=False)
    except (OSError, subprocess.SubprocessError) as exc:
        print(f"Unit cleanup note: {exc}", file=sys.stderr, flush=True)
    for name, process in reversed(children):
        try:
            stop_owned(name, process)
        except ProcessLookupError:
            print(f"Owned {name} PID={process.pid} already gone", flush=True)
        except (OSError, subprocess.SubprocessError) as exc:
            print(
                f"Could not stop owned {name} PID={process.pid}: {exc}",
                file=sys.stderr,
                flush=True,
            )
    for log in logs:
        log.close()
