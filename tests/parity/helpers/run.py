#!/usr/bin/env python3
"""Frozen black-box helper contracts; no helper imports or live desktop access."""

import argparse
import base64
import difflib
import json
import os
import re
from pathlib import Path
import selectors
import shlex
import shutil
import socket
import stat
import subprocess
import sys
import tempfile
import threading
import time

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]


def expand(value):
    if isinstance(value, str):
        return value.replace("{home}", "/case/home").replace("{root}", "/repo")
    if isinstance(value, list):
        return [expand(item) for item in value]
    if isinstance(value, dict):
        return {key: expand(item) for key, item in value.items()}
    return value


def fixtures(root, entries):
    for name, entry in entries.items():
        path = root / name
        if (
            Path(name).is_absolute()
            or ".." in Path(name).parts
            or not path.parent.resolve().is_relative_to(root.resolve())
        ):
            raise ValueError("Fixture path escapes its disposable root")
        path.parent.mkdir(parents=True, exist_ok=True)
        if isinstance(entry, str):
            entry = {"text": entry}
        if "link" in entry:
            path.symlink_to(entry["link"])
        elif "directory" in entry:
            path.mkdir(exist_ok=True)
        else:
            path.write_bytes(
                base64.b64decode(entry["base64"])
                if "base64" in entry
                else expand(entry.get("text", "")).encode()
            )
        if "mtime" in entry and not path.is_symlink():
            os.utime(path, (entry["mtime"], entry["mtime"]))
        if not path.is_symlink():
            path.chmod(int(entry.get("mode", "755" if path.is_dir() else "644"), 8))


def snapshot(root):
    result = {}
    for path in sorted(root.rglob("*")):
        name = path.relative_to(root).as_posix()
        if path.is_symlink():
            result[name] = {"link": os.readlink(path)}
        elif path.is_file():
            data = path.read_bytes()
            try:
                content = {"text": data.decode()}
            except UnicodeDecodeError:
                content = {"base64": base64.b64encode(data).decode()}
            result[name] = {"mode": oct(stat.S_IMODE(path.stat().st_mode)), **content}
        elif path.is_dir():
            result[name] = {
                "mode": oct(stat.S_IMODE(path.stat().st_mode)),
                "directory": True,
            }
    return result


class IPC:
    """Scripted UNIX peers: received bytes are recorded before fixture replies."""

    def __init__(self, root, specs):
        self.events = []
        self.errors = []
        self.servers = []
        self.threads = []
        for spec in specs:
            path = root / spec["path"]
            path.parent.mkdir(parents=True, exist_ok=True)
            server = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            server.bind(str(path))
            server.listen(8)
            server.settimeout(0.1)
            self.servers.append(server)
            thread = threading.Thread(
                target=self.serve, args=(server, spec), daemon=True
            )
            thread.start()
            self.threads.append(thread)

    def serve(self, server, spec):
        try:
            for reply in spec.get("connections", []):
                while True:
                    try:
                        connection, _ = server.accept()
                        break
                    except socket.timeout:
                        if server.fileno() == -1:
                            return
                with connection:
                    connection.settimeout(5)
                    data = b""
                    if not reply.get("unsolicited"):
                        while not data.endswith(b"\n"):
                            part = connection.recv(1)
                            if not part:
                                break
                            data += part
                    self.events.append(
                        {"socket": spec["path"], "message": data.decode()}
                    )
                    connection.sendall(expand(reply.get("text", "")).encode())
        except OSError as error:
            if server.fileno() != -1:
                self.errors.append(str(error))

    def close(self):
        for server in self.servers:
            server.close()
        for thread in self.threads:
            thread.join(timeout=1)
        if self.errors:
            raise RuntimeError("IPC fixture failed: " + repr(self.errors))


def pointer(value, path):
    parts = [
        part.replace("~1", "/").replace("~0", "~")
        for part in path.strip("/").split("/")
    ]
    for part in parts[:-1]:
        value = value[int(part)] if isinstance(value, list) else value[part]
    return value, int(parts[-1]) if isinstance(value, list) else parts[-1]


def normalize(result, rules):
    """Explicit per-case JSON pointers only; never drop arbitrary fields."""
    for rule in rules:
        if rule["kind"] not in ("timestamp", "pid", "temp-path") or not rule.get(
            "reason"
        ):
            raise ValueError("Unsupported or undocumented normalization")
        if "pattern" in rule:
            target, key = pointer(result, rule["pointer"])

            def transform(value, rule=rule):
                if isinstance(value, str):
                    return re.sub(rule["pattern"], rule["replacement"], value)
                if isinstance(value, list):
                    return [transform(item) for item in value]
                if isinstance(value, dict):
                    return {
                        transform(name): transform(item) for name, item in value.items()
                    }
                return value

            target[key] = transform(target[key])
            continue
        target, key = pointer(result, rule["pointer"])
        marker = "<" + rule["kind"] + ">"
        if "json_pointer" in rule:
            embedded, field = pointer(json.loads(target[key]), rule["json_pointer"])
            old = json.dumps(embedded[field])
            # Preserve every byte other than this explicitly identified value.
            if target[key].count(old) != 1:
                raise ValueError("Ambiguous embedded timestamp normalization")
            target[key] = target[key].replace(old, json.dumps(marker))
        elif "substring" in rule:
            target[key] = target[key].replace(rule["substring"], rule["replacement"])
        else:
            target[key] = marker
    return result


def read_packet(process, timeout=8):
    deadline = time.monotonic() + timeout
    buffer = getattr(process, "_parity_buffer", b"")
    with selectors.DefaultSelector() as selector:
        selector.register(process.stdout, selectors.EVENT_READ)
        while b"\n" not in buffer:
            remaining = deadline - time.monotonic()
            if remaining <= 0 or not selector.select(remaining):
                raise RuntimeError("Protocol response timed out")
            chunk = os.read(process.stdout.fileno(), 65536)
            if not chunk:
                raise RuntimeError("Protocol closed before expected response")
            buffer += chunk
            if len(buffer) > 9 * 1024 * 1024:
                raise RuntimeError("Protocol response exceeds fixture limit")
    line, process._parity_buffer = buffer.split(b"\n", 1)
    return json.loads(line)


def execute(case, command, trace=None):
    if not shutil.which("bwrap"):
        raise RuntimeError("Bubblewrap is required; refusing a live-host fallback")
    with tempfile.TemporaryDirectory(prefix="nacre-parity-") as directory:
        root = Path(directory)
        for folder in ("home", "bin", "proc", "sys", "etc", "runtime", "tmp", "local"):
            (root / folder).mkdir()
        fixtures(root, case.get("fixtures", {}))
        commands = expand(case.get("commands", {}))
        if case.get("native_engine"):
            commands["orient"] = [
                {
                    "argv": ["--rpc"],
                    "read_stdin": True,
                    "forward": "/case/native-orient",
                },
                {"argv": ["--rpc", "--serve"], "forward_stream": "/case/native-orient"},
            ]
        (root / "stubs.json").write_text(json.dumps(commands))
        (root / "effects.jsonl").touch()
        for name in case.get("commands", {}):
            (root / "bin" / name).symlink_to("/repo/tests/parity/helpers/fake.py")
        # Deliberately no host PATH: unlisted tools are unavailable, not live tools.
        for name in ("python", "python3", "bash", "sh"):
            (root / "bin" / name).symlink_to("/usr/bin/" + name)
        observed = ("home", "tmp", "sys", "runtime", "local")
        for peer_spec in case.get("sockets", []):
            (root / peer_spec["path"]).parent.mkdir(parents=True, exist_ok=True)
        before = {
            f"{folder}/{name}": value
            for folder in observed
            for name, value in snapshot(root / folder).items()
        }
        env = {
            "HOME": "/case/home",
            "PATH": "/case/bin",
            "LC_ALL": "C.UTF-8",
            "LANG": "C.UTF-8",
            "TZ": "UTC",
            "PYTHONDONTWRITEBYTECODE": "1",
            "PYTHONHASHSEED": "0",
            "PYTHONPATH": "/repo/nacre/shell-tools:/repo/nacre/shell-cli/src:/repo/ai",
            "XDG_RUNTIME_DIR": "/case/runtime",
            "XDG_CONFIG_HOME": "/case/home/.config",
            "XDG_CACHE_HOME": "/case/home/.cache",
            "XDG_DATA_HOME": "/case/home/.local/share",
            "XDG_STATE_HOME": "/case/home/.local/state",
            "DBUS_SESSION_BUS_ADDRESS": "unix:path=/case/runtime/absent-bus",
            "NACRE_COMPOSITOR": "hyprland",
            "NACRE_PARITY_ROOT": "/case",
            **expand(case.get("env", {})),
        }
        template = command or case.get("command", "python3 nacre/shell-tools/{name}.py")
        argv = [
            part.replace("{name}", case["helper"]) for part in shlex.split(template)
        ]
        argv += expand(case.get("args", []))
        replacement_mount = []
        if command:
            executable = shutil.which(argv[0])
            if not executable:
                raise ValueError("Replacement executable is not on the invoking PATH")
            destination = "/case/bin/" + Path(argv[0]).name
            (root / "bin" / Path(argv[0]).name).unlink(missing_ok=True)
            (root / "bin" / Path(argv[0]).name).touch()
            replacement_mount = [
                "--ro-bind",
                str(Path(executable).resolve()),
                destination,
            ]
            argv[0] = destination
        sandbox = [
            "bwrap",
            "--unshare-user",
            "--unshare-pid",
            "--unshare-net",
            "--unshare-ipc",
            "--uid",
            "1000",
            "--gid",
            "1000",
            "--die-with-parent",
            "--new-session",
            "--size",
            "1073741824",
            "--tmpfs",
            "/",
            "--ro-bind",
            "/usr",
            "/usr",
            "--symlink",
            "usr/bin",
            "/bin",
            "--symlink",
            "usr/lib",
            "/lib",
            "--symlink",
            "usr/lib64",
            "/lib64",
            "--ro-bind",
            str(ROOT),
            "/repo",
            "--bind",
            str(root),
            "/case",
            "--ro-bind",
            str(root / "proc"),
            "/proc",
            "--bind",
            str(root / "sys"),
            "/sys",
            "--ro-bind",
            str(root / "etc"),
            "/etc",
            "--dev",
            "/dev",
            "--bind",
            str(root / "tmp"),
            "/tmp",
            "--bind",
            str(root / "local"),
            "/usr/local",
            "--chdir",
            "/repo",
            "--clearenv",
        ]
        # Mask all absolute application executables too (timezone and Xfconf
        # intentionally bypass PATH). Keep only the language/tooling primitives.
        sandbox += ["--tmpfs", "/usr/bin"]
        for name in ("python3", "bash", "env"):
            executable = Path("/usr/bin") / name
            sandbox += ["--ro-bind", str(executable.resolve()), "/usr/bin/" + name]
        for alias, name in (("python", "python3"), ("sh", "bash")):
            sandbox += ["--symlink", name, "/usr/bin/" + alias]
        for name in case.get("commands", {}):
            sandbox += ["--ro-bind", str(HERE / "fake.py"), "/usr/bin/" + name]
        for key, value in env.items():
            sandbox += ["--setenv", key, value]
        daemon = peer = None
        if case.get("dbus"):
            import dbus_next

            (root / "support").mkdir()
            sandbox += [
                "--ro-bind",
                str(Path(dbus_next.__file__).parent),
                "/case/support/dbus_next",
                "--setenv",
                "PYTHONPATH",
                "/case/support:" + env["PYTHONPATH"],
            ]
            (root / "dbus-fixtures.json").write_text(json.dumps(expand(case["dbus"])))
            daemon = subprocess.Popen(
                [
                    "dbus-daemon",
                    "--session",
                    "--nofork",
                    "--nopidfile",
                    "--address=unix:path=" + str(root / "runtime/parity-bus"),
                    "--print-address",
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            if not daemon.stdout.readline().startswith("unix:"):
                raise RuntimeError("Private fixture bus did not start")
            peer = subprocess.Popen(
                [sys.executable, str(HERE / "dbus_peer.py"), str(root)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
            )
            try:
                read_packet(peer)
            except Exception:
                peer.kill()
                daemon.kill()
                raise
            # SASL EXTERNAL uses the host UID for this private socket; no UID
            # appears in these contracts. The ordinary filesystem cases use 1000.
            uid_index = sandbox.index("--uid") + 1
            sandbox[uid_index] = str(os.getuid())
            gid_index = sandbox.index("--gid") + 1
            sandbox[gid_index] = str(os.getgid())
            sandbox += [
                "--setenv",
                "DBUS_SESSION_BUS_ADDRESS",
                "unix:path=/case/runtime/parity-bus",
                "--setenv",
                "DBUS_SYSTEM_BUS_ADDRESS",
                "unix:path=/case/runtime/parity-bus",
            ]
        if case.get("native_engine"):
            engine = os.environ.get(
                "NACRE_ORIENT_BINARY", str(ROOT / ".venv/bin/orient")
            )
            if not Path(engine).is_file():
                raise RuntimeError("Set NACRE_ORIENT_BINARY to the pinned candidate")
            (root / "bin/orient").write_bytes((HERE / "fake.py").read_bytes())
            (root / "bin/orient").chmod(0o755)
            (root / "native-orient").touch()
            sandbox += [
                "--ro-bind",
                str(Path(engine).resolve()),
                "/case/native-orient",
                "--setenv",
                "NACRE_ORIENT_BINARY",
                "/case/bin/orient",
            ]
        ipc = IPC(root, case.get("sockets", []))
        try:
            with subprocess.Popen(
                [*sandbox, *replacement_mount, *argv],
                stdin=subprocess.PIPE,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                bufsize=1,
            ) as process:
                packets = []
                try:
                    if "dialogue" in case:
                        for step in case["dialogue"]:
                            if "send" in step:
                                process.stdin.write(expand(step["send"]))
                                process.stdin.flush()
                            if "release" in step:
                                fixtures(root, step["release"])
                            if "receive" in step:
                                for _ in range(step["receive"]):
                                    packets.append(read_packet(process))
                        process.stdin.close()
                        process.stdin = None
                        output, error = process.communicate(
                            timeout=case.get("timeout", 10)
                        )
                        output = (
                            getattr(process, "_parity_buffer", b"").decode() + output
                        )
                    else:
                        output, error = process.communicate(
                            input=expand(case.get("stdin", ""))
                            + case.get("stdin_repeat", {}).get("text", "")
                            * case.get("stdin_repeat", {}).get("count", 0),
                            timeout=case.get("timeout", 10),
                        )
                except Exception:
                    process.kill()
                    process.communicate()
                    raise
                code = process.returncode
        finally:
            ipc.close()
            for child in (peer, daemon):
                if child:
                    child.terminate()
                    child.communicate(timeout=3)
        if case.get("stdout", "json") == "json":
            output = json.loads(output) if output.strip() else None
        elif case.get("stdout") == "jsonl":
            output = packets + [json.loads(line) for line in output.splitlines()]
        after = {
            f"{folder}/{name}": value
            for folder in observed
            for name, value in snapshot(root / folder).items()
        }
        effects = [
            json.loads(line)
            for line in (root / "effects.jsonl").read_text().splitlines()
        ]
        result = {
            "stdout": output,
            "exit": code,
            "files": {
                key: value for key, value in after.items() if before.get(key) != value
            },
            "deleted": sorted(before.keys() - after.keys()),
            "effects": effects,
            "ipc": ipc.events,
        }
        if case.get("compare_stderr"):
            result["stderr"] = error
        result = normalize(result, case.get("normalizations", []))
        if trace is not None:
            trace.parent.mkdir(parents=True, exist_ok=True)
            wire = root / "dbus-wire.jsonl"
            trace.write_text(
                json.dumps(
                    {
                        "result": result,
                        "stderr": error,
                        "dbus_calls": [
                            json.loads(line) for line in wire.read_text().splitlines()
                        ]
                        if wire.exists()
                        else [],
                    },
                    indent=2,
                )
                + "\n"
            )
        return result


# Pure operations used by the frozen native-engine cases. This explicit optional
# transport policy never changes reference files or ordinary desktop actions.
ORIENT_DIRECT_OPERATIONS = frozenset(
    {
        "constants",
        "background_policy",
        "from_image",
        "render_values",
        "info",
        "clean",
        "rgb",
        "linear",
        "encoded",
        "luminance",
        "lch",
        "validate",
    }
)


def orient_direct_equivalence(result, case):
    if not case.get("native_engine"):
        return result
    result = {**result, "effects": list(result["effects"])}
    retained = []
    for event in result["effects"]:
        if event.get("command") != "orient" or event.get("argv") not in (
            ["--rpc"],
            ["--rpc", "--serve"],
        ):
            retained.append(event)
            continue
        try:
            requests = [
                json.loads(line) for line in event.get("stdin", "").splitlines()
            ]
            pure = bool(requests) and all(
                isinstance(request, dict)
                and set(request) <= {"id", "operation", "payload"}
                and request.get("operation") in ORIENT_DIRECT_OPERATIONS
                and isinstance(request.get("payload"), dict)
                for request in requests
            )
        except (ValueError, TypeError):
            pure = False
        if not pure:
            retained.append(event)
    result["effects"] = retained
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--command", help="argv template, e.g. 'nacre-helper {name}'; no shell"
    )
    parser.add_argument("--case", action="append", default=[], help="helper/case glob")
    parser.add_argument(
        "--record", action="store_true", help="explicitly replace frozen references"
    )
    parser.add_argument("--list", action="store_true")
    parser.add_argument(
        "--orient-direct",
        action="store_true",
        help="replacement uses pinned Orient core/decoder directly; compare computational RPC through exact outputs/files",
    )
    parser.add_argument(
        "--traces",
        type=Path,
        help="save per-case observations, including raw D-Bus method calls",
    )
    args = parser.parse_args()
    os.umask(0o022)
    if args.orient_direct and not args.command:
        parser.error("--orient-direct requires a replacement --command")
    if args.record and args.command:
        parser.error("Record only the Python reference, never a replacement")
    paths = sorted(HERE.glob("*/case-*.json"))
    import fnmatch

    selected = [
        path
        for path in paths
        if not args.case
        or any(
            fnmatch.fnmatch(path.parent.name + "/" + path.stem[5:], pattern)
            for pattern in args.case
        )
    ]
    if not selected:
        parser.error("No matching cases")
    failures = 0
    for path in selected:
        label = path.parent.name + "/" + path.stem[5:]
        if args.list:
            print(label)
            continue
        try:
            case = json.loads(path.read_text())
            actual = execute(
                case,
                args.command,
                args.traces / (label + ".json") if args.traces else None,
            )
            reference = path.with_name("expected-" + path.name[5:])
            rendered = json.dumps(actual, indent=2, ensure_ascii=True) + "\n"
            if args.record:
                if args.command:
                    parser.error(
                        "Record only the Python reference, never a replacement"
                    )
                reference.write_text(rendered)
            else:
                expected = json.loads(reference.read_text())
                if args.orient_direct:
                    expected = orient_direct_equivalence(expected, case)
                    actual = orient_direct_equivalence(actual, case)
                if expected != actual:
                    failures += 1
                    diff = list(
                        difflib.unified_diff(
                            json.dumps(expected, indent=2).splitlines(),
                            rendered.splitlines(),
                            fromfile="frozen",
                            tofile="candidate",
                            lineterm="",
                        )
                    )
                    print(label + ": FAIL\n" + "\n".join(diff[:60]), file=sys.stderr)
        except (
            OSError,
            ValueError,
            RuntimeError,
            KeyError,
            TypeError,
            subprocess.SubprocessError,
        ) as error:
            failures += 1
            print(label + ": " + str(error), file=sys.stderr)
    if not args.list:
        print(f"Helper parity: {len(selected)} cases, {failures} failures")
    return bool(failures)


if __name__ == "__main__":
    raise SystemExit(main())
