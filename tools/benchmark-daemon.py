#!/usr/bin/env python3
"""Daemon-only nested lab; no shell launch, compilation, live buses or live units."""

import argparse
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import select
import shutil
import socket
import statistics
import subprocess
import sys
import threading
import time

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools"))
spec = importlib.util.spec_from_file_location(
    "daemon_nested_lab", ROOT / "tools/performance-session.py"
)
lab = importlib.util.module_from_spec(spec)
spec.loader.exec_module(lab)


class Completed(Exception):
    """Return through the lab's existing process-group cleanup."""


def paging():
    values = dict(
        line.split() for line in Path("/proc/vmstat").read_text().splitlines()
    )
    return {key: int(values[key]) for key in ("pswpin", "pswpout")}


def switches(pid):
    status = Path(f"/proc/{pid}/status").read_text().splitlines()
    values = {
        line.split(":", 1)[0]: line.split(":", 1)[1].strip()
        for line in status
        if ":" in line
    }
    tasks = list(Path(f"/proc/{pid}/task").iterdir())
    return dict(
        voluntary=int(values["voluntary_ctxt_switches"]),
        state=values["State"][0],
        threads=len(tasks),
    )


class Link:
    def __init__(self, path):
        self.socket = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.socket.settimeout(5)
        self.socket.connect(str(path))
        self.file = self.socket.makefile("rb")
        self.send(dict(t="hello", proto=1, min=1, client="daemon-benchmark"))
        self.welcome = self.receive()
        if self.welcome.get("t") != "welcome":
            raise RuntimeError("Daemon rejected private benchmark handshake")

    def send(self, message):
        self.socket.sendall(json.dumps(message, separators=(",", ":")).encode() + b"\n")

    def receive(self):
        line = self.file.readline(1_048_578)
        if not line or len(line) > 1_048_577 or not line.endswith(b"\n"):
            raise RuntimeError("Invalid private daemon frame")
        return json.loads(line)

    def close(self):
        self.file.close()
        self.socket.close()


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=5)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=5)


def read_watch(process, buffer):
    deadline = time.monotonic() + 10
    while b"\n" not in buffer:
        remaining = deadline - time.monotonic()
        if remaining <= 0 or not select.select([process.stdout], [], [], remaining)[0]:
            raise RuntimeError("Native reconnect watcher timed out")
        data = os.read(process.stdout.fileno(), 8192)
        if not data:
            raise RuntimeError("Native reconnect watcher exited")
        buffer += data
        if len(buffer) > 1_048_577:
            raise RuntimeError("Oversized native watcher frame")
    line, buffer = buffer.split(b"\n", 1)
    return json.loads(line), buffer


def measure(tools, env, options, output):
    runtime = Path(env["XDG_RUNTIME_DIR"])
    assert str(Path(env["HOME"])) != str(Path.home())
    assert runtime.name.startswith("npr-") and runtime.parent == Path("/tmp")
    helper = tools / "nacre-helper"
    endpoint = runtime / "nacre/daemon.sock"
    environment = dict(
        env,
        NACRE_DAEMON_SOCKET=str(endpoint),
        NACRE_NIRI_HOVER_STREAM="0",
        DBUS_SESSION_BUS_ADDRESS="unix:path=" + str(runtime / "absent-session-bus"),
    )
    notify = socket.socket(socket.AF_UNIX, socket.SOCK_DGRAM)
    notify_path = runtime / "daemon-notify.sock"
    notify.bind(str(notify_path))
    notify.settimeout(0.25)
    environment.update(NOTIFY_SOCKET=str(notify_path), WATCHDOG_USEC="20000000")
    ready = threading.Event()
    finished = threading.Event()
    notices = []

    def receive_notify():
        while not finished.is_set():
            try:
                packet = notify.recv(1024)
            except socket.timeout:
                continue
            notices.append(packet.decode())
            if b"READY=1" in packet:
                ready.set()

    receiver = threading.Thread(target=receive_notify)
    receiver.start()
    processes = []
    logs = []

    def launch(name, arguments):
        log = (output.parent / (output.stem + "-" + name + ".log")).open("wb")
        logs.append(log)
        child = subprocess.Popen(
            [str(helper), *arguments],
            env=environment,
            stdout=subprocess.PIPE if name == "watch" else log,
            stderr=log,
        )
        processes.append(child)
        return child

    try:
        daemon = launch("daemon", ["daemon"])
        if not ready.wait(15):
            raise RuntimeError("Nested compositor service did not reach READY")
        link = Link(endpoint)
        try:
            before = paging()
            samples = []
            call_memory = [lab.performance.memory(daemon.pid)]
            cpu_start = None
            for sequence in range(options.daemon_calls + 100):
                if sequence == 100:
                    cpu_start = lab.performance.process(daemon.pid)
                started = time.perf_counter_ns()
                link.send(dict(t="call", id=sequence, m="daemon.ping", p={}))
                reply = link.receive()
                elapsed = (time.perf_counter_ns() - started) / 1_000_000
                if reply != dict(t="ret", id=sequence, ok=None):
                    raise RuntimeError("Empty-call protocol mismatch")
                if sequence >= 100:
                    samples.append(elapsed)
                if sequence % 100 == 0:
                    call_memory.append(lab.performance.memory(daemon.pid))
            after = paging()
            memory = lab.performance.memory(daemon.pid)
            call_memory.append(memory)
            cpu_end = lab.performance.process(daemon.pid)
            call_valid = (
                all(row["swap_pss"] == 0 for row in call_memory)
                and (after["pswpin"] - before["pswpin"]) * os.sysconf("SC_PAGE_SIZE")
                < 1_048_576
            )
            samples.sort()
            if not cpu_start or not cpu_end or cpu_start["start"] != cpu_end["start"]:
                raise RuntimeError("Call capture process identity changed")
            calls = dict(
                cpu_ticks=cpu_end["ticks"] - cpu_start["ticks"],
                cpu_quantum_ms_per_call=1000 / os.sysconf("SC_CLK_TCK") / len(samples),
                count=len(samples),
                p50_ms=statistics.median(samples),
                p99_ms=samples[math.ceil(0.99 * len(samples)) - 1],
                valid=call_valid,
                host_swap_before=before,
                host_swap_after=after,
                swap_pss=memory["swap_pss"],
            )
        finally:
            link.close()
        # Let the bounded blocking pool retire before the one-thread idle sample.
        time.sleep(options.daemon_settle)
        before = paging()
        initial = lab.performance.process(daemon.pid)
        first = switches(daemon.pid)
        memories = []
        threads = []
        deadline = time.monotonic() + options.daemon_idle
        started = time.monotonic()
        while time.monotonic() < deadline:
            if daemon.poll() is not None:
                raise RuntimeError("Daemon exited during idle capture")
            memories.append(lab.performance.memory(daemon.pid))
            threads.append(switches(daemon.pid)["threads"])
            time.sleep(0.25)
        elapsed = time.monotonic() - started
        final = lab.performance.process(daemon.pid)
        last = switches(daemon.pid)
        after = paging()
        if not initial or not final or initial["start"] != final["start"]:
            raise RuntimeError("Idle capture process identity changed")
        valid = (
            all(row["swap_pss"] == 0 for row in memories)
            and (after["pswpin"] - before["pswpin"]) * os.sysconf("SC_PAGE_SIZE")
            < 1_048_576
        )
        cycles_valid = (
            first["state"] == last["state"] == "S"
            and first["threads"] == last["threads"] == 1
            and max(threads) == 1
        )
        idle = dict(
            seconds=elapsed,
            valid=valid,
            host_swap_before=before,
            host_swap_after=after,
            measured_swap_pss_max=max(
                (row["swap_pss"] for row in memories if row["swap_pss"] is not None),
                default=None,
            ),
            cpu_ticks=final["ticks"] - initial["ticks"],
            ticks_per_second=os.sysconf("SC_CLK_TCK"),
            threads_max=max(threads),
            wake_sleep_cycles=last["voluntary"] - first["voluntary"]
            if cycles_valid
            else None,
            wakeup_method="Main-thread voluntary switches, one thread and sleeping endpoints; sleep/wake cycles, not kernel sched_wakeup counter",
            memory_mib={
                key: statistics.median(row[key] / 1_048_576 for row in memories)
                if all(row.get(key) is not None for row in memories)
                else None
                for key in ("rss", "pss", "uss")
            },
        )
        watch = launch("watch", ["ctl", "watch", "compositor"])
        buffer = b""
        while True:
            message, buffer = read_watch(watch, buffer)
            if message["t"] == "welcome":
                old_boot = message["boot"]
                break
        daemon.kill()
        daemon.wait(timeout=5)
        time.sleep(0.5)
        ready.clear()
        restarted_at = time.perf_counter_ns()
        replacement = launch("restart", ["daemon"])
        if not ready.wait(15):
            raise RuntimeError("Restarted compositor service did not reach READY")
        new_boot = None
        while True:
            message, buffer = read_watch(watch, buffer)
            if message["t"] == "welcome" and message["boot"] != old_boot:
                new_boot = message["boot"]
            if (
                new_boot
                and message["t"] in ("snap", "patch")
                and message.get("topic") == "compositor"
                and message.get("data", {}).get("available") is True
            ):
                break
        fresh = (time.perf_counter_ns() - restarted_at) / 1_000_000
        stop(watch)
        stop(replacement)
        report = dict(
            capture_role="profile"
            if options.daemon_idle >= 60 and options.daemon_settle >= 30
            else "functional-smoke",
            condition="nested busy-machine lab; not physical desktop numbers",
            backend=env["NACRE_COMPOSITOR"],
            helper_sha256=hashlib.sha256(helper.read_bytes()).hexdigest(),
            method="Existing performance-session private compositor lifecycle, performance.py proc/memory readers; persistent socket empty calls; no UI launched",
            page_size=os.sysconf("SC_PAGE_SIZE"),
            calls=calls,
            idle=idle,
            restart=dict(
                new_boot=new_boot is not None,
                client_data_fresh_ms=fresh,
                boundary="replacement process start to fresh compositor data; not a rendered UI frame",
            ),
            ready_messages=sum("READY=1" in n for n in notices),
            watchdog_messages=sum("WATCHDOG=1" in n for n in notices),
            python_launches_by_daemon=None,
            python_launch_proof="Not traced; empty-PATH native integration tests are separate functional evidence",
        )
        output.write_text(json.dumps(report, indent=2) + "\n")
    finally:
        for child in reversed(processes):
            stop(child)
        finished.set()
        receiver.join(timeout=1)
        notify.close()
        for log in logs:
            log.close()


def main():
    parser = argparse.ArgumentParser(add_help=False)
    parser.add_argument("--daemon-calls", type=int, default=5000)
    parser.add_argument("--daemon-idle", type=float, default=60)
    parser.add_argument("--daemon-settle", type=float, default=35)
    parser.add_argument("--daemon-niri", default="/usr/bin/niri")
    options, remaining = parser.parse_known_args()
    if (
        options.daemon_calls < 100
        or options.daemon_idle <= 0
        or options.daemon_settle < 0
    ):
        parser.error("Invalid daemon measurement count or interval")
    if "--helper-binary" not in remaining or "--output" not in remaining:
        parser.error(
            "Pass a prebuilt --helper-binary and --output; no compilation occurs here"
        )
    output = Path(remaining[remaining.index("--output") + 1]).resolve()
    output.parent.mkdir(parents=True, exist_ok=True)
    original_popen = subprocess.Popen

    def private_popen(command, **kwargs):
        environment = kwargs.get("env")
        if (
            environment
            and environment.get("HOME") != str(Path.home())
            and environment.get("XDG_RUNTIME_DIR", "").startswith("/tmp/npr-")
        ):
            environment = dict(
                environment,
                DBUS_SESSION_BUS_ADDRESS="unix:path="
                + str(Path(environment["XDG_RUNTIME_DIR"]) / "absent-session-bus"),
                HYPRLAND_NO_SYSTEMD="1",
            )
            kwargs["env"] = environment
            command = list(command)
            if command[0] == "niri":
                command[0] = options.daemon_niri
        return original_popen(command, **kwargs)

    def prepare(directory, revision=None, control=False, helper_binary=None):
        if revision or control:
            raise ValueError("This tool measures only the supplied native daemon build")
        home = directory / "home"
        home.mkdir(mode=0o700)
        tools = home / ".local/share/nacre/shell/tools"
        tools.mkdir(parents=True)
        shutil.copy2(Path(helper_binary).resolve(strict=True), tools / "nacre-helper")
        return home, directory / "unused-shell.qml", None

    def hook(tools, environment):
        measure(tools, environment, options, output)
        raise Completed()

    subprocess.Popen = private_popen
    lab.prepare = prepare
    lab.shell_environment = hook
    sys.argv = [sys.argv[0], *remaining]
    try:
        lab.main()
    except Completed:
        print(
            "Private daemon capture completed; results include validity and measurement boundaries."
        )
    finally:
        subprocess.Popen = original_popen


if __name__ == "__main__":
    main()
