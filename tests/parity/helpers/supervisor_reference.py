"""Disposable real-process supervisor fixtures; never installed."""

import importlib.util
import json
import os
from pathlib import Path
import selectors
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
STAMP = "20261001T120000"


def wait_for_fixture_event(selector, event_fd, seen, expected, deadline):
    while expected not in seen:
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise AssertionError(f"Timed out waiting for fixture event {expected!r}")
        ready = selector.select(remaining)
        for key, _ in ready:
            if key.fd == event_fd:
                seen.extend(os.read(event_fd, 4096).decode().splitlines())
        if expected not in seen and any(key.fd != event_fd for key, _ in ready):
            raise AssertionError(f"Supervisor exited before fixture event {expected!r}")


def capture(command, names=None):
    from run import snapshot

    cases = []
    for name in names or (
        "clean",
        "startup-no-good",
        "recover",
        "recovery-fails",
        "runtime-failure",
        "terminate",
    ):
        with tempfile.TemporaryDirectory(prefix="supervisor-reference-") as folder:
            home = Path(folder)
            runtime = home / "runtime"
            runtime.mkdir(mode=0o700)
            root = home / ".local/share/nacre/shell"
            (root / "source").mkdir(parents=True)
            bad = name in ("startup-no-good", "recover", "recovery-fails")
            (root / "source/shell.qml").write_text("bad" if bad else "healthy")
            (root / "preferences.json").write_text('{"untouched":true}')
            if name in ("recover", "recovery-fails", "runtime-failure"):
                (root / "source.good/nested").mkdir(parents=True)
                (root / "source.good/shell.qml").write_text(
                    "bad" if name == "recovery-fails" else "healthy"
                )
                (root / "source.good/nested/asset").write_text("validated asset")
                (root / "source.good/nested/asset").chmod(0o640)
            (root / "bin").mkdir()
            qs = root / "bin/qs"
            qs.write_text("""#!/bin/sh
root="$HOME/.local/share/nacre/shell"
record_event() {
 printf '%s\\n' "$1" >> "$root/events"
 printf '%s\\n' "$1" > "$root/events.pipe"
}
case "$*" in *"ipc call"*)
 record_event health
 if test "$(cat "$root/source/shell.qml")" = healthy; then printf '{"reveal":0,"launcherMode":"apps"}\\n'; else printf '{}\\n'; fi
 exit 0;; esac
record_event "start $(cat "$root/source/shell.qml")"
if test "$(cat "$root/source/shell.qml")" = bad; then exit 9; fi
trap 'record_event stopped-TERM; kill "$sleeper" 2>/dev/null; wait "$sleeper" 2>/dev/null; exit 143' TERM INT
sleep "${FIXTURE_SLEEP:-0.5}" & sleeper=$!
record_event child-running
wait "$sleeper"
exit "${FIXTURE_EXIT:-0}"
""")
            qs.chmod(0o700)
            env = dict(
                PATH=os.environ["PATH"],
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_CACHE_HOME=folder + "/.cache",
                XDG_DATA_HOME=folder + "/.local/share",
                XDG_RUNTIME_DIR=str(runtime),
                WAYLAND_DISPLAY="wayland-test",
                NACRE_COMPOSITOR="niri",
                NIRI_SOCKET=str(runtime / "absent-niri"),
                NACRE_HELPER_BINARY=os.environ["NACRE_HELPER_BINARY"],
                PYTHONPATH=str(ROOT / "nacre/shell-tools"),
                DBUS_SESSION_BUS_ADDRESS="unix:path=" + folder + "/absent-session",
                DBUS_SYSTEM_BUS_ADDRESS="unix:path=" + folder + "/absent-system",
                FIXTURE_SLEEP="20" if name.startswith("terminate") else "0.5",
                FIXTURE_EXIT="7" if name == "runtime-failure" else "0",
            )
            peer = socket.socket(socket.AF_UNIX)
            peer.bind(str(runtime / "wayland-test"))
            events_pipe = root / "events.pipe"
            os.mkfifo(events_pipe)
            event_fd = os.open(events_pipe, os.O_RDWR | os.O_NONBLOCK)
            event_selector = selectors.DefaultSelector()
            event_selector.register(event_fd, selectors.EVENT_READ)
            # Descendants can inherit stdout/stderr after the supervisor exits.
            # Regular files let the parent-exit wait complete independently of
            # those descriptors while retaining the complete captured output.
            with (
                tempfile.TemporaryFile(mode="w+t") as stdout_file,
                tempfile.TemporaryFile(mode="w+t") as stderr_file,
            ):
                child = subprocess.Popen(
                    command,
                    env=env,
                    stdout=stdout_file,
                    stderr=stderr_file,
                    text=True,
                )
                child_fd = None
                if name.startswith("terminate"):
                    child_fd = os.pidfd_open(child.pid)
                    event_selector.register(child_fd, selectors.EVENT_READ)
                try:
                    try:
                        if name.startswith("terminate"):
                            deadline = time.monotonic() + 60
                            seen_events = []
                            wait_for_fixture_event(
                                event_selector,
                                event_fd,
                                seen_events,
                                "health",
                                deadline,
                            )
                            if name == "terminate-child":
                                wait_for_fixture_event(
                                    event_selector,
                                    event_fd,
                                    seen_events,
                                    "child-running",
                                    deadline,
                                )
                            child.send_signal(signal.SIGTERM)
                        child.wait(timeout=60)
                    except subprocess.TimeoutExpired as error:
                        events_path = root / "events"
                        events = (
                            events_path.read_text()
                            if events_path.exists()
                            else "<no events>"
                        )
                        raise AssertionError(
                            f"Supervisor case {name} timed out after 60 seconds; "
                            f"parent_exit={child.poll()}; events={events[-4096:]!r}"
                        ) from error
                finally:
                    if child.poll() is None:
                        child.kill()
                        child.wait()
                    peer.close()
                    (runtime / "wayland-test").unlink()
                    event_selector.close()
                    if child_fd is not None:
                        os.close(child_fd)
                    os.close(event_fd)
                stdout_file.seek(0)
                stderr_file.seek(0)
                stdout, stderr = stdout_file.read(), stderr_file.read()
            files = snapshot(root)
            # Probe count is scheduler-dependent; the ordered launches and bytes
            # written by recovery are the contract. Clock is fixed by the driver.
            events = (root / "events").read_text().splitlines()
            files.pop("events")
            files.pop("bin/qs")
            extra = (
                {"forwarded": [v for v in events if v.startswith("stopped-")]}
                if name == "terminate-child"
                else {}
            )
            cases.append(
                dict(
                    name=name,
                    stdout=stdout,
                    stderr=stderr,
                    exit=child.returncode,
                    launches=[v for v in events if v.startswith("start ")],
                    files=files,
                    **extra,
                )
            )
    return cases


def capture_api(command):
    cases = []
    for name in (
        "env-explicit",
        "env-absolute",
        "env-auto",
        "env-ambiguous",
        "env-existing-type",
        "health-ready",
        "health-missing-field",
        "health-malformed",
        "health-error",
        "health-missing-tool",
    ):
        with tempfile.TemporaryDirectory(prefix="supervisor-api-") as folder:
            home = Path(folder)
            runtime = home / "runtime"
            runtime.mkdir(mode=0o700)
            peers = []
            if name.startswith("env-"):
                for number in range(2 if name == "env-ambiguous" else 1):
                    peer = socket.socket(socket.AF_UNIX)
                    peer.bind(str(runtime / f"wayland-{number}"))
                    peers.append(peer)
            qs = home / ".local/share/nacre/shell/bin/qs"
            if name != "health-missing-tool":
                qs.parent.mkdir(parents=True)
                output = {
                    "health-ready": '{"launcherMode":null,"reveal":false}',
                    "health-missing-field": '{"reveal":0}',
                    "health-malformed": "not JSON",
                }.get(name, "{}")
                qs.write_text(
                    "#!/bin/sh\nprintf '%s' '"
                    + output
                    + "'\nexit "
                    + ("3" if name == "health-error" else "0")
                    + "\n"
                )
                qs.chmod(0o700)
            env = dict(
                PATH="/usr/bin:/bin",
                HOME=folder,
                XDG_CONFIG_HOME=folder + "/.config",
                XDG_STATE_HOME=folder + "/.local/state",
                XDG_CACHE_HOME=folder + "/.cache",
                XDG_DATA_HOME=folder + "/.local/share",
                XDG_RUNTIME_DIR=str(runtime),
                NACRE_COMPOSITOR="niri",
                NIRI_SOCKET=str(runtime / "absent"),
                NACRE_HELPER_BINARY=os.environ["NACRE_HELPER_BINARY"],
                PYTHONPATH=str(ROOT / "nacre/shell-tools"),
                KEEP_ME="preserved",
            )
            if name == "env-explicit":
                env["WAYLAND_DISPLAY"] = "wayland-0"
            if name == "env-absolute":
                env["WAYLAND_DISPLAY"] = str(runtime / "wayland-0")
            if name == "env-existing-type":
                env["XDG_SESSION_TYPE"] = "x11"
            try:
                result = subprocess.run(
                    [*command, name],
                    env=env,
                    text=True,
                    capture_output=True,
                    timeout=30,
                )
                cases.append(
                    dict(
                        name=name,
                        stdout=result.stdout.replace(folder, "/fixture/home"),
                        stderr=result.stderr.replace(folder, "/fixture/home"),
                        exit=result.returncode,
                    )
                )
            finally:
                for peer in peers:
                    peer.close()
    return cases


def original_main():
    spec = importlib.util.spec_from_file_location(
        "supervisor_original", ROOT / "nacre/shell-tools/shell-supervisor.py"
    )
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.time.strftime = lambda *_: STAMP
    if len(sys.argv) == 1:
        module.main()
        return
    mode = sys.argv[1]
    module.time.sleep = lambda *_: None
    try:
        if mode.startswith("env-"):
            value = module.environment()
            value = {
                key: value.get(key)
                for key in (
                    "WAYLAND_DISPLAY",
                    "XDG_SESSION_TYPE",
                    "KEEP_ME",
                    "NACRE_COMPOSITOR",
                )
            }
        else:
            value = module.health(dict(os.environ))
        print(json.dumps(value))
    except (RuntimeError, OSError, TypeError) as error:
        print(json.dumps({"error": str(error), "type": type(error).__name__}))
        raise SystemExit(1) from None


if __name__ == "__main__":
    original_main()
