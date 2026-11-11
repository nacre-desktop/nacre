"""End-to-end regression coverage for the AI terminal launch routes."""

import base64
import fcntl
import json
import os
from pathlib import Path
import pty
import select
import shutil
import signal
import struct
import subprocess
import tempfile
import termios
import time
import unittest


ROOT = Path(__file__).resolve().parents[2]
THREAD = "11111111-2222-4333-8444-555555555555"
HELPER = Path(
    os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/release/nacre-helper")
)

FAKE_CODEX = r"""#!/usr/bin/python3
import json, os, sys, time
args = sys.argv[1:]
if "app-server" in args:
    if "daemon" in args:
        raise SystemExit(0)
    for line in sys.stdin:
        request = json.loads(line)
        if "id" not in request:
            continue
        method = request.get("method")
        if method == "thread/list":
            result = {"data": [{"id": os.environ["FAKE_THREAD"],
                "cwd": os.environ["FAKE_CHAT_DIR"], "name": "N0171 synthetic chat",
                "updatedAt": 2000000000000, "preview": "synthetic"}], "nextCursor": None}
        elif method == "thread/read":
            result = {"thread": {"id": os.environ["FAKE_THREAD"],
                "cwd": os.environ["FAKE_CHAT_DIR"]}}
        else:
            result = {}
        print(json.dumps({"id": request["id"], "result": result}), flush=True)
    raise SystemExit(0)
if "-C" in args:
    os.chdir(args[args.index("-C") + 1])
entry = {"pid": os.getpid(), "argv": args, "cwd": os.getcwd(), "HOME": os.environ.get("HOME"),
    "CODEX_HOME": os.environ.get("CODEX_HOME"),
    "NACRE_AI_HISTORY_HOME": os.environ.get("NACRE_AI_HISTORY_HOME")}
with open(os.environ["PROVIDER_LOG"], "a") as output:
    output.write(json.dumps(entry) + "\n")
print(json.dumps(entry), flush=True)
if os.environ.get("FAKE_PROVIDER_EXIT"):
    print("N0171 provider failed", file=sys.stderr, flush=True)
    raise SystemExit(int(os.environ["FAKE_PROVIDER_EXIT"]))
time.sleep(45)
"""

FAKE_CLAUDE = r"""#!/usr/bin/python3
import json, os, sys, time
args = sys.argv[1:]
if args == ["agents", "--json"]:
    print("[]")
    raise SystemExit(0)
entry = {"pid": os.getpid(), "argv": args, "cwd": os.getcwd(), "CLAUDE_CONFIG_DIR": os.environ.get("CLAUDE_CONFIG_DIR")}
with open(os.environ["PROVIDER_LOG"], "a") as output:
    output.write(json.dumps(entry) + "\n")
print(json.dumps(entry), flush=True)
if os.environ.get("FAKE_CLAUDE_EXIT"):
    print("N0171 Claude provider failed", file=sys.stderr, flush=True)
    raise SystemExit(int(os.environ["FAKE_CLAUDE_EXIT"]))
time.sleep(45)
"""

FAKE_TMUX = r"""#!/usr/bin/python3
import subprocess, sys
args = sys.argv[1:]
if args == ["-V"]:
    print("tmux 3.4")
elif args[:1] == ["list-panes"]:
    raise SystemExit(0)
elif args[:1] == ["new-session"]:
    command = args[args.index("-s") + 2]
    # tmux reports a successful client exit even when its pane command fails.
    subprocess.run(["/bin/sh", "-c", command], check=False)
raise SystemExit(0)
"""

FAKE_KITTY = r"""#!/usr/bin/python3
import json, os, sys
args = sys.argv[1:]
with open(os.environ["KITTY_LOG"], "w") as output:
    json.dump(args, output)
index = args.index("--")
os.execvpe(args[index + 1], args[index + 1:], os.environ)
"""

FAKE_SDK = """from types import SimpleNamespace\nimport os\ndef list_sessions(directory, **kwargs):\n    if os.environ.get("FAKE_CLAUDE_HISTORY") != "1":\n        return []\n    return [SimpleNamespace(session_id=os.environ["FAKE_THREAD"], cwd=directory, custom_title="N0171 synthetic chat", summary="", first_prompt="synthetic", last_modified=2000000000000)]\n"""


def _write_executable(path, content):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)
    path.chmod(0o755)


class NativeLaunchTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not HELPER.is_file():
            raise unittest.SkipTest(f"native helper binary is unavailable: {HELPER}")

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="n0171-ai-launch-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def fixture(self, mode):
        home = Path(tempfile.mkdtemp(prefix=mode + "-", dir=self.root))
        bindir = home / ".local/bin"
        tools = home / ".local/share/nacre/shell/tools"
        chats = home / ".local/share/nacre/ai/chats"
        chats.mkdir(parents=True)
        bindir.mkdir(parents=True)
        tools.mkdir(parents=True)
        if mode == "python":
            for name in ("nacre-ai", "nacre-ai-chat", "nacre-ai-claude"):
                shutil.copy2(ROOT / "bin" / f"{name}.py.rollback", bindir / name)
        else:
            for name in ("nacre-ai", "nacre-ai-chat", "nacre-ai-claude"):
                shutil.copy2(ROOT / "bin" / name, bindir / name)
        for name in ("nacre_ai_core.py", "nacre_state.py", "nacre-state-files.json"):
            shutil.copy2(ROOT / "nacre/shell-tools" / name, tools / name)
        _write_executable(bindir / "codex", FAKE_CODEX)
        _write_executable(bindir / "claude", FAKE_CLAUDE)
        _write_executable(bindir / "tmux", FAKE_TMUX)
        _write_executable(bindir / "kitty", FAKE_KITTY)
        _write_executable(bindir / "nacre-ai-skills", "#!/bin/sh\nexit 0\n")
        (tools / "claude_agent_sdk.py").write_text(FAKE_SDK)
        (home / ".codex").mkdir()
        (home / ".local/share/nacre/ai/accounts/work").mkdir(parents=True)
        (home / ".local/share/nacre/ai/claude-accounts/work").mkdir(parents=True)
        (home / ".config/nacre/ai").mkdir(parents=True)
        (home / ".config/nacre/ai/projects.json").write_text(
            json.dumps({"projects": []})
        )
        (home / ".config/nacre/ai/settings.json").write_text(
            json.dumps(
                {
                    "default_assistant": "codex",
                    "accounts": {"codex": "work"},
                    "aiFullAccess": True,
                }
            )
        )
        (home / ".config/nacre/profile.json").write_text('{"aiFullAccess":true}')
        provider_log = home / "provider.jsonl"
        kitty_log = home / "kitty.json"
        env = {
            "HOME": str(home),
            "PATH": str(bindir) + ":/usr/bin:/bin",
            "TERM": "xterm-256color",
            "LANG": "C.UTF-8",
            "PYTHONPATH": str(tools),
            "NACRE_HELPER_BINARY": str(HELPER),
            "FAKE_THREAD": THREAD,
            "FAKE_CHAT_DIR": str(chats),
            "PROVIDER_LOG": str(provider_log),
            "KITTY_LOG": str(kitty_log),
            "TMUX_TMPDIR": str(home),
            # Exercise stale state inherited by kitty/the user shell.
            "CODEX_HOME": str(home / "stale-codex-home"),
            "NACRE_AI_HISTORY_HOME": str(home / "stale-history"),
        }
        return home, env, chats, provider_log, kitty_log

    def run_tty(self, argv, env, home, *, select_chat=False, wait_for=None):
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))

        def attach_tty():
            os.setsid()
            fcntl.ioctl(0, termios.TIOCSCTTY, 0)

        process = subprocess.Popen(
            argv,
            stdin=slave,
            stdout=slave,
            stderr=slave,
            cwd=home,
            env=env,
            preexec_fn=attach_tty,
        )
        os.close(slave)
        process.provider_log = Path(env["PROVIDER_LOG"])
        output = bytearray()
        entered = False
        deadline = time.monotonic() + 18
        try:
            while time.monotonic() < deadline:
                if select.select([master], [], [], 0.1)[0]:
                    try:
                        output.extend(os.read(master, 65536))
                    except OSError:
                        break
                decoded = output.decode(errors="replace")
                if select_chat and not entered and "N0171 synthetic chat" in decoded:
                    os.write(master, b"\r")
                    entered = True
                if wait_for and wait_for in decoded:
                    break
                provider_log = Path(env["PROVIDER_LOG"])
                if (
                    not wait_for
                    and provider_log.exists()
                    and provider_log.stat().st_size
                ):
                    break
                if process.poll() is not None:
                    break
            return process, master, output.decode(errors="replace"), entered
        except BaseException:
            self.stop(process, master)
            raise

    def stop(self, process, master):
        try:
            if process.poll() is None:
                try:
                    os.killpg(process.pid, signal.SIGTERM)
                except ProcessLookupError:
                    pass
                process.wait(timeout=5)
            if process.provider_log.exists():
                for line in process.provider_log.read_text().splitlines():
                    try:
                        pid = json.loads(line)["pid"]
                    except (json.JSONDecodeError, KeyError):
                        continue
                    try:
                        os.kill(pid, signal.SIGTERM)
                    except ProcessLookupError:
                        pass
        finally:
            os.close(master)

    def test_panel_load_chat_child_command_reaches_fake_codex_in_both_implementations(
        self,
    ):
        for mode in ("native", "python"):
            with self.subTest(mode=mode):
                home, env, _chats, log, kitty_log = self.fixture(mode)
                kitty_argv = ["--class", "nacre-ai-task", "--title", "Load chat"]
                kitty_argv += [
                    "--",
                    "nacre-ai",
                    "window",
                    "--worker-command",
                    "sessions",
                ]
                process, master, output, selected = self.run_tty(
                    ["kitty", *kitty_argv],
                    env,
                    home,
                    select_chat=True,
                )
                try:
                    self.assertTrue(selected, output)
                    self.assertEqual(json.loads(kitty_log.read_text()), kitty_argv)
                    self.assertTrue(log.exists(), output)
                    record = json.loads(log.read_text().splitlines()[-1])
                    self.assertEqual(record["argv"][0], "resume")
                    self.assertEqual(record["argv"][1], THREAD)
                    self.assertEqual(
                        record["cwd"], str(home / ".local/share/nacre/ai/chats")
                    )
                    self.assertEqual(
                        record["CODEX_HOME"],
                        str(home / ".local/share/nacre/ai/accounts/work"),
                    )
                    self.assertIsNone(record["NACRE_AI_HISTORY_HOME"])
                finally:
                    self.stop(process, master)

    def test_public_window_latest_and_new_launch_forms(self):
        for mode in ("native", "python"):
            for action, child_args in (
                ("latest", ["latest"]),
                (
                    "new",
                    [
                        "new",
                        "--project",
                        "general-chat",
                        "--agent",
                        "codex",
                        "--account",
                        "work",
                    ],
                ),
                ("new", ["new", "--agent", "codex"]),
            ):
                with self.subTest(mode=mode, action=action):
                    home, env, _chats, log, kitty_log = self.fixture(mode)
                    kitty_argv = [
                        "--class",
                        "nacre-ai-task",
                        "--title",
                        "New chat" if action == "new" else "Load chat",
                    ]
                    kitty_argv += [
                        "--",
                        "nacre-ai",
                        "window",
                        "--worker-command",
                        *child_args,
                    ]
                    process, master, output, _ = self.run_tty(
                        ["kitty", *kitty_argv], env, home
                    )
                    try:
                        self.assertEqual(json.loads(kitty_log.read_text()), kitty_argv)
                        self.assertTrue(log.exists(), output)
                        launched = json.loads(log.read_text().splitlines()[-1])
                        if action == "latest":
                            self.assertEqual(launched["argv"][0], "resume")
                            self.assertIn(THREAD, launched["argv"])
                        else:
                            self.assertTrue(launched["argv"])
                            self.assertIn("danger-full-access", launched["argv"])
                    finally:
                        self.stop(process, master)

    def test_native_and_python_claude_resume_use_the_selected_account(self):
        for mode in ("native", "python"):
            with self.subTest(mode=mode):
                home, env, chats, log, _kitty_log = self.fixture(mode)
                env["FAKE_CLAUDE_HISTORY"] = "1"
                process, master, output, _ = self.run_tty(
                    [
                        "nacre-ai-claude",
                        "resume",
                        "--project",
                        "general-chat",
                        "--path",
                        str(chats),
                        "--account",
                        "work",
                        "--thread-id",
                        THREAD,
                        "--chat-directory",
                        "--tmux",
                    ],
                    env,
                    home,
                )
                try:
                    self.assertTrue(log.exists(), output)
                    launched = json.loads(log.read_text().splitlines()[-1])
                    self.assertIn("--resume", launched["argv"])
                    self.assertIn(THREAD, launched["argv"])
                    self.assertEqual(
                        launched["CLAUDE_CONFIG_DIR"],
                        str(home / ".local/share/nacre/ai/claude-accounts/work"),
                    )
                    self.assertEqual(launched["cwd"], str(chats))
                finally:
                    self.stop(process, master)

    def test_native_resume_record_matches_python_latest_for_default_codex(self):
        home, env, chats, log, kitty_log = self.fixture("native")
        record = {
            "id": THREAD,
            "cwd": str(chats),
            "title": "N0171 synthetic chat",
            "updated_at": 2000000000000,
            "state": "saved",
            "agent": "codex",
            "account": "",
            "shared_history": True,
            "session": None,
            "project": {
                "id": "general-chat",
                "label": "General chat",
                "path": str(chats),
                "chat_directory": True,
            },
        }
        encoded = base64.b64encode(json.dumps(record).encode()).decode()
        kitty_argv = [
            "--class",
            "nacre-ai-chat-n0171-test",
            "--title",
            record["title"],
            "--",
            "nacre-ai",
            "resume-record",
            encoded,
        ]
        process, master, output, _ = self.run_tty(["kitty", *kitty_argv], env, home)
        try:
            self.assertEqual(json.loads(kitty_log.read_text()), kitty_argv)
            self.assertTrue(log.exists(), output)
            launched = json.loads(log.read_text().splitlines()[-1])
            self.assertEqual(launched["argv"][0], "resume")
            self.assertIn(THREAD, launched["argv"])
            self.assertEqual(launched["CODEX_HOME"], str(home / ".codex"))
            self.assertIsNone(launched["NACRE_AI_HISTORY_HOME"])
            self.assertEqual(launched["cwd"], str(chats))
        finally:
            self.stop(process, master)

        home, env, chats, log, kitty_log = self.fixture("native")
        explicit_history = home / ".local/share/nacre/ai/accounts/work/history"
        explicit_history.mkdir(parents=True)
        record.update(
            id=THREAD,
            cwd=str(chats),
            account="work",
            history_home=str(explicit_history),
            shared_history=False,
            project={
                "id": "general-chat",
                "label": "General chat",
                "path": str(chats),
                "chat_directory": True,
            },
        )
        encoded = base64.b64encode(json.dumps(record).encode()).decode()
        kitty_argv = [
            "--class",
            "nacre-ai-chat-n0171-legacy",
            "--title",
            record["title"],
            "--",
            "nacre-ai",
            "resume-record",
            encoded,
        ]
        process, master, output, _ = self.run_tty(["kitty", *kitty_argv], env, home)
        try:
            self.assertEqual(json.loads(kitty_log.read_text()), kitty_argv)
            self.assertTrue(log.exists(), output)
            launched = json.loads(log.read_text().splitlines()[-1])
            self.assertEqual(
                launched["CODEX_HOME"],
                str(home / ".local/share/nacre/ai/accounts/work"),
            )
            self.assertEqual(launched["NACRE_AI_HISTORY_HOME"], str(explicit_history))
            self.assertIn("--no-daemon", launched["argv"])
            self.assertIn(
                "sqlite_home=" + json.dumps(str(explicit_history)), launched["argv"]
            )
        finally:
            self.stop(process, master)

        home, env, _chats, log, _kitty_log = self.fixture("python")
        settings = home / ".config/nacre/ai/settings.json"
        values = json.loads(settings.read_text())
        values["accounts"]["codex"] = ""
        settings.write_text(json.dumps(values))
        process, master, output, _ = self.run_tty(
            ["nacre-ai", "latest", "--project", "general-chat"], env, home
        )
        try:
            self.assertTrue(log.exists(), output)
            launched = json.loads(log.read_text().splitlines()[-1])
            self.assertEqual(launched["argv"][0], "resume")
            self.assertIn(THREAD, launched["argv"])
            self.assertEqual(launched["CODEX_HOME"], str(home / ".codex"))
            self.assertIsNone(launched["NACRE_AI_HISTORY_HOME"])
        finally:
            self.stop(process, master)

    def test_provider_failure_remains_visible_until_enter(self):
        for mode in ("native", "python"):
            with self.subTest(mode=mode):
                home, env, _chats, _log, _kitty_log = self.fixture(mode)
                env["FAKE_PROVIDER_EXIT"] = "37"
                process, master, output, _ = self.run_tty(
                    [
                        "kitty",
                        "--class",
                        "nacre-ai-task",
                        "--title",
                        "Load chat",
                        "--",
                        "nacre-ai",
                        "window",
                        "--worker-command",
                        "latest",
                    ],
                    env,
                    home,
                    wait_for="Press Enter",
                )
                try:
                    self.assertIn("N0171 provider failed", output)
                    self.assertIn("Press Enter", output)
                    self.assertIsNone(process.poll(), output)
                    os.write(master, b"\r")
                    process.wait(timeout=5)
                finally:
                    self.stop(process, master)

    def test_tmux_provider_failures_remain_visible_until_enter(self):
        for provider, flag, marker, args in (
            (
                "claude",
                "FAKE_CLAUDE_EXIT",
                "N0171 Claude provider failed",
                [
                    "nacre-ai",
                    "window",
                    "--worker-command",
                    "new",
                    "--agent",
                    "claude",
                    "--account",
                    "work",
                ],
            ),
            (
                "codex",
                "FAKE_PROVIDER_EXIT",
                "N0171 provider failed",
                [
                    "nacre-ai-chat",
                    "resume",
                    "--project",
                    "general-chat",
                    "--thread-id",
                    THREAD,
                    "--account",
                    "work",
                ],
            ),
        ):
            with self.subTest(provider=provider):
                home, env, _chats, _log, _kitty_log = self.fixture("native")
                env[flag] = "37"
                process, master, output, _ = self.run_tty(
                    args,
                    env,
                    home,
                    wait_for="Press Enter",
                )
                try:
                    self.assertIn(marker, output)
                    self.assertIn("Press Enter", output)
                    self.assertIsNone(process.poll(), output)
                    os.write(master, b"\r")
                    process.wait(timeout=5)
                    self.assertEqual(process.returncode, 0)
                finally:
                    self.stop(process, master)

    def test_window_and_resume_record_hold_failures_until_enter(self):
        home, env, _chats, _log, _kitty_log = self.fixture("native")
        projects = home / ".config/nacre/ai/projects.json"
        projects.write_text(json.dumps({"version": 999, "projects": []}))
        process, master, output, _ = self.run_tty(
            ["nacre-ai", "window", "--worker-command", "sessions"],
            env,
            home,
            wait_for="Press Enter",
        )
        try:
            self.assertIsNone(process.poll(), output)
            self.assertIn("Press Enter", output)
            os.write(master, b"\r")
            process.wait(timeout=5)
        finally:
            self.stop(process, master)

        projects.write_text(json.dumps({"projects": []}))
        process, master, output, _ = self.run_tty(
            ["nacre-ai", "resume-record", "not-base64"],
            env,
            home,
            wait_for="Press Enter",
        )
        try:
            self.assertIsNone(process.poll(), output)
            self.assertIn("Press Enter", output)
            os.write(master, b"\r")
            process.wait(timeout=5)
        finally:
            self.stop(process, master)


if __name__ == "__main__":
    unittest.main()
