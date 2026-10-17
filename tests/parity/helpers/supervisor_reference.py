"""Disposable real-process supervisor fixtures; never installed."""

import importlib.util
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import tempfile
import time

ROOT = Path(__file__).resolve().parents[3]
STAMP = "20261001T120000"


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
case "$*" in *"ipc call"*)
 echo health >> "$root/events"
 if test "$(cat "$root/source/shell.qml")" = healthy; then printf '{"reveal":0,"launcherMode":"apps"}\\n'; else printf '{}\\n'; fi
 exit 0;; esac
printf 'start %s\\n' "$(cat "$root/source/shell.qml")" >> "$root/events"
if test "$(cat "$root/source/shell.qml")" = bad; then exit 9; fi
trap 'echo stopped-TERM >> "$root/events"; kill "$sleeper" 2>/dev/null; wait "$sleeper" 2>/dev/null; exit 143' TERM INT
sleep "${FIXTURE_SLEEP:-0.5}" & sleeper=$!
echo child-running >> "$root/events"
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
            child = subprocess.Popen(
                command,
                env=env,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                if name.startswith("terminate"):
                    deadline = time.monotonic() + 5
                    while (
                        not (root / "events").exists()
                        or "health" not in (root / "events").read_text()
                    ):
                        if child.poll() is not None or time.monotonic() > deadline:
                            raise AssertionError(
                                "Supervisor did not reach its private health probe"
                            )
                        time.sleep(0.01)
                    if name == "terminate-child":
                        while "child-running" not in (root / "events").read_text():
                            if time.monotonic() > deadline:
                                raise AssertionError("Private child did not start")
                            time.sleep(0.01)
                    child.send_signal(signal.SIGTERM)
                try:
                    stdout, stderr = child.communicate(timeout=8)
                except subprocess.TimeoutExpired as error:
                    events_path = root / "events"
                    events = (
                        events_path.read_text()
                        if events_path.exists()
                        else "<no events>"
                    )
                    raise AssertionError(
                        f"Supervisor case {name} timed out after 8 seconds; "
                        f"parent_exit={child.poll()}; events={events[-4096:]!r}"
                    ) from error
            finally:
                if child.poll() is None:
                    child.kill()
                    child.wait()
                peer.close()
                (runtime / "wayland-test").unlink()
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
                    [*command, name], env=env, text=True, capture_output=True, timeout=5
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
