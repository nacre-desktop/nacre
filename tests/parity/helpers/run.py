#!/usr/bin/env python3
"""Frozen black-box helper contracts; no helper imports or live desktop access."""

import argparse
import base64
import copy
import difflib
import hashlib
import importlib.util
from functools import lru_cache
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
_registry_spec = importlib.util.spec_from_file_location(
    "native_helper_registry", HERE / "native_registry.py"
)
native_registry = importlib.util.module_from_spec(_registry_spec)
_registry_spec.loader.exec_module(native_registry)
from native_compositor_peer import Peer as NativeCompositorPeer
from state_schema import compatible_files, recovery_reference

NATIVE_CLIENT_CALLERS = {
    "nacre-terminal-logo",
    "pearl-window",
    "compositor",
    "compositor-backends-hyprland",
    "compositor-backends-niri",
    "binding-contracts",
    "classic-state",
    "control-tools",
    "desktop-actions",
    "desktop-extras",
    "desktop-settings",
    "install-extras",
    "isolate-apps",
    "lock-config",
    "maintenance",
    "power-button-policy",
    "profile-policy",
    "session-watch",
    "shell-supervisor",
    "startup-apps",
    "welcome",
    "workflow-profiles",
    "wallpaper-media",
    "window-hide",
    "compositor-compat",
}


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
            if os.getuid() == 0:
                os.chown(path, 1000, 1000)
            path.chmod(0o600)
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


def historical_catalog_mounts(case):
    # This exact immutable input predates optional theme-bundle assets. Keep its
    # asset availability unchanged; explicitly configured/new cases use current assets.
    if case == {"helper": "wallpaper-media", "args": ["catalog"]}:
        return ["--tmpfs", "/repo/nacre/shell-tools/theme-bundles"]
    return []


def execute(case, command, trace=None):
    if case["helper"] == "wallpaper-media" and case.get("args") == ["session"]:
        # Replace only the transport: strict logind calls retain the frozen
        # loginctl/busctl effects and response data.
        case = dict(case)
        commands = case["commands"]
        display = commands["loginctl"][0].get("stdout", "").strip()
        user = "/org/freedesktop/login1/user/_1000"
        path = "/org/freedesktop/login1/session/_" + display if display else "/"
        manager = "org.freedesktop.login1.Manager"

        def rule(
            path,
            interface,
            member,
            signature,
            body,
            reply_signature,
            reply,
            effect=None,
        ):
            return dict(
                match=dict(
                    destination="org.freedesktop.login1",
                    path=path,
                    interface=interface,
                    member=member,
                    signature=signature,
                    body=body,
                ),
                reply=dict(signature=reply_signature, body=reply),
                effects=[effect] if effect else [],
            )

        def effect(command, argv):
            return dict(command=command, argv=argv, stdin="")

        case["dbus"] = [
            rule(
                "/org/freedesktop/login1", manager, "GetUser", "u", [1000], "o", [user]
            ),
            rule(
                user,
                "org.freedesktop.DBus.Properties",
                "Get",
                "ss",
                ["org.freedesktop.login1.User", "Display"],
                "v",
                [{"variant": "(so)", "value": [display, path]}],
                effect(
                    "loginctl", ["show-user", "1000", "--property=Display", "--value"]
                ),
            ),
        ]
        if display:
            locked = commands["loginctl"][1]["stdout"].strip() == "yes"
            case["dbus"] += [
                rule(
                    path,
                    "org.freedesktop.DBus.Properties",
                    "Get",
                    "ss",
                    ["org.freedesktop.login1.Session", "LockedHint"],
                    "v",
                    [{"variant": "b", "value": locked}],
                    effect(
                        "loginctl",
                        ["show-session", display, "--property=LockedHint", "--value"],
                    ),
                ),
                rule(
                    "/org/freedesktop/login1",
                    manager,
                    "GetSession",
                    "s",
                    [display],
                    "o",
                    [path],
                    effect(
                        "busctl",
                        [
                            "--system",
                            "call",
                            "org.freedesktop.login1",
                            "/org/freedesktop/login1",
                            manager,
                            "GetSession",
                            "s",
                            display,
                        ],
                    ),
                ),
            ]
    if case["helper"] == "window-hide":
        # Supply the notification API instead of the retired notify-send tool.
        # Record its equivalent effect so the immutable reference stays exact.
        messages = (
            "Window hidden",
            "Window restored",
            "Window restored to current workspace",
            "No active window to hide",
            "No hidden windows to restore",
        )
        case = dict(case)
        case["dbus"] = [
            {
                "match": {
                    "destination": "org.freedesktop.Notifications",
                    "path": "/org/freedesktop/Notifications",
                    "interface": "org.freedesktop.Notifications",
                    "member": "Notify",
                    "signature": "susssasa{sv}i",
                    "body": [
                        "Nacre Windows",
                        0,
                        "nacre-notifications",
                        message,
                        "",
                        [],
                        {},
                        2000,
                    ],
                },
                "reply": {"signature": "u", "body": [1]},
                "effects": [
                    {
                        "command": "notify-send",
                        "argv": [
                            "-a",
                            "Nacre Windows",
                            "-i",
                            "nacre-notifications",
                            "-t",
                            "2000",
                            message,
                        ],
                        "stdin": "",
                    }
                ],
            }
            for message in messages
        ]
    if (
        case["helper"] == "classic-state"
        and case.get("args", [None])[0] == "_reload_desktop_consumers"
    ):
        # One strict dconf Change packet replaces the five frozen gsettings calls.
        # The GLib-generated bytes are independent of the Rust serializer.
        import hashlib

        vector = json.loads(
            (
                ROOT / "crates/nacre-helper/tests/fixtures/dconf-writer-reference.json"
            ).read_text()
        )
        raw = bytes.fromhex(vector["hex"])
        if (
            hashlib.sha256(raw).hexdigest()
            != "db1451cc8f78abc910360a90d3e840234dd0667a974e3c27737820080968d7bd"
        ):
            raise ValueError("dconf reference bytes changed")
        case = dict(case)
        case["dbus"] = [
            {
                "match": {
                    "destination": "ca.desrt.dconf",
                    "path": "/ca/desrt/dconf/Writer/user",
                    "interface": "ca.desrt.dconf.Writer",
                    "member": "Change",
                    "signature": "ay",
                    "body": [list(raw)],
                },
                "reply": {"signature": "s", "body": ["fixture-tag"]},
                "effects": [
                    {
                        "command": "gsettings",
                        "argv": ["set", "org.gnome.desktop.interface", key, value],
                        "stdin": "",
                    }
                    for key, value in [
                        ("cursor-theme", "breeze_cursors"),
                        ("cursor-size", "24"),
                        ("color-scheme", "prefer-dark"),
                        ("gtk-theme", "Adwaita-dark"),
                        ("icon-theme", "Fixture Icons"),
                    ]
                ],
            }
        ]
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
        native_peer = None
        if case["helper"] in NATIVE_CLIENT_CALLERS and case.get("commands", {}).get(
            "hyprctl"
        ):
            native_peer = NativeCompositorPeer(
                root, expand(case["commands"]["hyprctl"])
            )
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
        if native_peer:
            env["HYPRLAND_INSTANCE_SIGNATURE"] = "fixture"
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
        if "python_version" in case:
            version = case["python_version"]
            if not isinstance(version, str) or not re.fullmatch(
                r"Python [0-9]+\.[0-9]+\.[0-9]+", version
            ):
                raise ValueError("Invalid fixture interpreter version")
            # Control the reported version, never rewrite the helper's output.
            # Forward all real execution and preserve sys.executable's path.
            (root / "python-real").write_bytes(
                Path("/usr/bin/python3").resolve().read_bytes()
            )
            (root / "python-real").chmod(0o755)
            (root / "python-proxy").write_text(
                "#!/usr/bin/bash\n"
                "if [[ $# == 1 && $1 == --version ]]; then\n"
                f"    printf '%s\\n' {shlex.quote(version)}\n"
                'else\n    exec -a /usr/bin/python3 /case/python-real "$@"\nfi\n'
            )
            (root / "python-proxy").chmod(0o755)
        sandbox += historical_catalog_mounts(case)
        for name in ("python3", "bash", "env"):
            executable = (
                root / "python-proxy"
                if name == "python3" and "python_version" in case
                else Path("/usr/bin") / name
            )
            sandbox += ["--ro-bind", str(executable.resolve()), "/usr/bin/" + name]
        for alias, name in (("python", "python3"), ("sh", "bash")):
            sandbox += ["--symlink", name, "/usr/bin/" + alias]
        for name in case.get("commands", {}):
            sandbox += ["--ro-bind", str(HERE / "fake.py"), "/usr/bin/" + name]
        for key, value in env.items():
            sandbox += ["--setenv", key, value]
        if case["helper"] in NATIVE_CLIENT_CALLERS:
            helper = native_registry.native_binary()
            (root / "native-compositor").touch()
            sandbox += [
                "--ro-bind",
                str(helper),
                "/case/native-compositor",
                "--setenv",
                "NACRE_HELPER_BINARY",
                "/case/native-compositor",
            ]
        identity = {}
        if os.getuid() == 0:
            # CI is root; fixture ownership is input, not an output normalization.
            for path in [root, *root.rglob("*")]:
                os.chown(path, 1000, 1000, follow_symlinks=False)
            identity = {"user": 1000, "group": 1000, "extra_groups": []}
        daemon = peer = None
        if case.get("dbus"):
            import dbus_next

            (root / "support").mkdir()
            if identity:
                os.chown(root / "support", 1000, 1000)
            sandbox += [
                "--ro-bind",
                str(Path(dbus_next.__file__).parent),
                "/case/support/dbus_next",
                "--setenv",
                "PYTHONPATH",
                "/case/support:" + env["PYTHONPATH"],
            ]
            (root / "dbus-fixtures.json").write_text(json.dumps(expand(case["dbus"])))
            if identity:
                os.chown(root / "dbus-fixtures.json", 1000, 1000)
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
                **identity,
            )
            if not daemon.stdout.readline().startswith("unix:"):
                _, daemon_error = daemon.communicate(timeout=3)
                raise RuntimeError(
                    "Private fixture bus did not start: " + daemon_error.strip()
                )
            peer = subprocess.Popen(
                [sys.executable, str(HERE / "dbus_peer.py"), str(root)],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                **identity,
            )
            try:
                read_packet(peer)
            except Exception:
                peer.kill()
                daemon.kill()
                raise
            # Bus and helper children share the same fixture UID mapping, so
            # EXTERNAL authentication sees 1000 even when the CI parent is root.
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
            if identity:
                os.chown(root / "bin/orient", 1000, 1000)
                os.chown(root / "native-orient", 1000, 1000)
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
                **identity,
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
                if code and not output.strip() and error.lstrip().startswith("bwrap:"):
                    raise RuntimeError(
                        "Parity sandbox could not start: " + error.strip()
                    )
        finally:
            ipc.close()
            if native_peer:
                native_peer.close()
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


@lru_cache(maxsize=1)
def pinned_engine_identity():
    lock = json.loads((ROOT / "dependencies.lock.json").read_text())["orient"]
    pin = json.loads(
        (ROOT / "nacre/shell-cli/src/nacre_shell/engine-pin.json").read_text()
    )
    wanted = {
        "backend": "rust",
        "interfaceVersion": lock["interfaceVersion"],
        "engine": lock["engine"],
        "coreEngine": lock["engine"].rsplit("-", 1)[0],
        "decoder": lock["decoder"],
        "version": lock["version"].replace("-dev.", ".dev"),
    }
    if pin["version"] != wanted["version"] or any(
        pin[key] != lock[key] for key in ("engine", "decoder", "interfaceVersion")
    ):
        raise ValueError("Orient lock and adapter identity differ")
    binary = os.environ.get("NACRE_ORIENT_BINARY", str(ROOT / ".venv/bin/orient"))
    reply = json.loads(
        subprocess.run(
            [binary, "--rpc"],
            input=json.dumps({"operation": "info", "payload": {}}),
            text=True,
            capture_output=True,
            check=True,
            timeout=5,
        ).stdout
    )
    if (
        reply.get("result") != wanted
        or reply.get("engine") != lock["engine"]
        or reply.get("interfaceVersion") != lock["interfaceVersion"]
    ):
        raise ValueError("Compiled Orient identity differs from the tested pin")
    return lock["engine"]


def engine_provenance_reference(expected, actual, case):
    rule = case.get("engine_provenance")
    if not rule:
        return expected
    # This is a reviewed identity change at two exact fields, not a generic
    # hash/version normalizer. Every other byte/field remains comparable.
    locations = ["/stdout/engine", "/stdout/source/engine"]
    if (
        case.get("helper") != "nacre_shell"
        or (case.get("args") or [None])[0] != "from_image"
        or not case.get("native_engine")
        or rule.get("pointers") != locations
        or not rule.get("reason")
    ):
        raise ValueError("Unsupported engine provenance rule")
    current = pinned_engine_identity()
    if current.rsplit("-", 1)[0] != rule["frozen_engine"].rsplit("-", 1)[0]:
        raise ValueError("Core engine changed; decoder provenance rule does not apply")
    result = copy.deepcopy(expected)
    for location in locations:
        frozen, key = pointer(result, location)
        observed, observed_key = pointer(actual, location)
        if frozen[key] != rule["frozen_engine"]:
            raise ValueError(
                "Frozen engine identity differs from the declared reference"
            )
        if observed[observed_key] != current:
            raise ValueError("Observed engine identity differs from the tested pin")
        frozen[key] = current
    return result


def native_exception_reference(expected, actual, case):
    """Only enumerated retired tracebacks become exact class/message lines."""
    rule = case.get("native_exception")
    if rule is None:
        return expected
    reminder = "TypeError: float() argument must be a string or a real number, not 'NoneType'\n"
    index = "IndexError: list index out of range\n"
    menu_sha = "d74ca0b2a6d6d41a173b399fa5fbf876472d9ef8b18572d604ea8b20108eb958"
    history_sha = "a2ebaf16a33ac477cbf5ac58484075406d4d4a9ebb39acaf2ae9eccedbf6fb7a"
    contracts = {
        ("menu-backend", ("reminder",), "{}\n"): (menu_sha, reminder),
        ("menu-backend", ("reminder", "demo"), "{}\n"): (menu_sha, reminder),
        ("notification-history", (), None): (history_sha, index),
        ("notification-history", ("unknown",), None): (
            history_sha,
            "ValueError: Unknown history action\n",
        ),
        ("notification-history", ("--help",), None): (
            history_sha,
            "ValueError: Unknown history action\n",
        ),
        ("dashboard-media", (), None): (
            "8fbbe42b736ffeffd5130eac87c9e2b8fa3e313e2b104c0a7885af4714bca77d",
            index,
        ),
        ("dashboard-data", ("preferences", "-2"), None): (
            "edc64017612a33b07377dc3eea7da533a114c432256f51a72945b9d44181130a",
            "AttributeError: 'int' object has no attribute 'items'\n",
        ),
    }
    key = (case.get("helper"), tuple(case.get("args", [])), case.get("stdin"))
    declared = contracts.get(key)
    reasons = {
        "Rust retains exception class/message, empty stdout, exit and lock; retired Python source frames are omitted.",
        "Rust retains exception class/message, stdout, exit and files; retired Python source frames are omitted.",
    }
    if (
        declared is None
        or set(rule) != {"kind", "terminal_line", "reason"}
        or rule["kind"] != "retired-python-traceback"
        or rule["reason"] not in reasons
        or rule["terminal_line"] != declared[1]
        or case.get("reference_source_sha256") != declared[0]
        or not expected.get("stderr", "").startswith(
            "Traceback (most recent call last):\n"
        )
        or not expected["stderr"].endswith(declared[1])
        or f"/repo/nacre/shell-tools/{case['helper']}.py" not in expected["stderr"]
        or expected.get("exit") != 1
        or expected.get("stdout") not in (None, "")
        or actual.get("stderr") != declared[1]
    ):
        raise ValueError(
            "Native exception differs from the exact declared traceback contract"
        )
    return {**expected, "stderr": declared[1]}


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


def service_fixture_command(case, command, fixture_binary):
    mode = case.get("native_service_fixture")
    if not mode or not command:
        return command
    providers = {
        "gate": (
            "hosted/dashboard-data.py",
            "da4885ab56669f9d59d87f05bc59cc04bc131118bcf69fe492614b35209e2976",
        ),
        "crash": (
            "hosted/dashboard-media.py",
            "79b67df827ee454974c7a62b349d2561eb814c1b95bbb67d215cfa7a46bc51ca",
        ),
        "exit": (
            "hosted/dashboard-media.py",
            "85d07f9647e4d7291e92f8cc8fbc1f1f48a8e863854ed352586eca51f3a24a0c",
        ),
        "oversized": (
            "hosted/dashboard-data.py",
            "4fb41fa05f9c4473cb2d7a6fc8b028b852da0c1707b0361788a7c7fb9b215ea1",
        ),
    }
    if (
        case.get("helper") != "helper-service"
        or case.get("env", {}).get("NACRE_DASHBOARD_TOOLS") != "/case/hosted"
        or mode not in providers
    ):
        raise ValueError("Unsupported native service provider fixture")
    path, digest = providers[mode]
    source = case.get("fixtures", {}).get(path)
    if (
        set(case.get("fixtures", {})) != {path}
        or not isinstance(source, str)
        or hashlib.sha256(source.encode()).hexdigest() != digest
    ):
        raise ValueError(
            "Injected provider differs from the frozen native test mapping"
        )
    if fixture_binary is None:
        raise ValueError(
            "Build/select --service-fixture for native service provider cases"
        )
    return shlex.join([str(Path(fixture_binary).resolve(strict=True)), mode])


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
    parser.add_argument(
        "--service-fixture",
        type=Path,
        help="Developer-only native provider adapter for frozen service crash/concurrency cases",
    )
    args = parser.parse_args()
    os.umask(0o022)
    if args.orient_direct and not args.command:
        parser.error("--orient-direct requires a replacement --command")
    if args.record and args.command:
        parser.error("Record only the Python reference, never a replacement")
    if args.service_fixture and not args.command:
        parser.error("--service-fixture requires a replacement --command")
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
    if args.record:
        try:
            native_registry.assert_recordable(
                [json.loads(path.read_text()) for path in selected]
            )
        except ValueError as error:
            parser.error(str(error))
    failures = 0
    for path in selected:
        label = path.parent.name + "/" + path.stem[5:]
        if args.list:
            print(label)
            continue
        try:
            case = json.loads(path.read_text())
            if args.record and case.get("engine_provenance"):
                parser.error("Frozen engine-provenance cases cannot be re-recorded")
            command = native_registry.command(case, args.command)
            fixture = args.service_fixture
            if command and case.get("native_service_fixture") and fixture is None:
                fixture = native_registry.service_fixture_binary()
            actual = execute(
                case,
                service_fixture_command(case, command, fixture),
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
                expected = engine_provenance_reference(expected, actual, case)
                expected = native_exception_reference(expected, actual, case)
                expected = compatible_files(recovery_reference(expected, case), case)
                actual = compatible_files(actual, case)
                if args.orient_direct or (
                    args.command is None and case["helper"] in native_registry.MIGRATED
                ):
                    expected = orient_direct_equivalence(expected, case)
                    actual = orient_direct_equivalence(actual, case)
                rendered = json.dumps(actual, indent=2, ensure_ascii=True) + "\n"
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
