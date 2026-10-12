#!/usr/bin/env python3
"""Private persistent assistant session, independent from the desktop renderer."""

import argparse
import re
import json
import os
import queue
import runpy
import signal
import socket
import subprocess
import sys
import threading
import time
import uuid
import hashlib
from pathlib import Path
from importlib.util import spec_from_file_location, module_from_spec

HOME = Path.home()
STATE = HOME / ".local/state/nacre/shell/sidebar"
SOCKET = (
    Path(os.environ.get("XDG_RUNTIME_DIR", f"/run/user/{os.getuid()}"))
    / "nacre-sidebar-chat.sock"
)
CHANNEL = ""
DEFAULT_STATE = STATE
DEFAULT_SOCKET = SOCKET


def configure_channel(channel):
    global STATE, SOCKET, CHANNEL
    if channel and not re.fullmatch(r"[a-z0-9-]{1,48}", channel):
        raise ValueError("Invalid assistant view channel")
    CHANNEL = channel
    STATE = DEFAULT_STATE / "views" / channel if channel else DEFAULT_STATE
    SOCKET = (
        DEFAULT_SOCKET.with_name("nacre-chat-" + channel + ".sock")
        if channel
        else DEFAULT_SOCKET
    )


def start_service():
    if not CHANNEL:
        subprocess.run(
            ["systemctl", "--user", "start", "nacre-sidebar-ai.service"],
            check=True,
            stdout=subprocess.DEVNULL,
        )
        return
    unit = "nacre-sidebar-ai-view-" + CHANNEL
    if (
        subprocess.run(
            ["systemctl", "--user", "is-active", "--quiet", unit],
            check=False,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        ).returncode
        == 0
    ):
        return
    subprocess.run(
        [
            "systemd-run",
            "--user",
            "--quiet",
            "--collect",
            "--unit=" + unit,
            "--property=Type=exec",
            sys.executable,
            str(Path(__file__).resolve()),
            "server",
            "--channel",
            CHANNEL,
        ],
        check=True,
        stdout=subprocess.DEVNULL,
    )


INSTRUCTIONS = """You are Nacre AI in an ongoing desktop sidebar conversation. It is a full assistant workspace, not a short-answer mode. The user can change desktop workspaces without changing this conversation. Read ~/.codex/AGENTS.md and the shared personal workflow at ~/.agents/skills/nacre-project-work/SKILL.md. Use the private shared brain and project registry to resolve context; inspect each project's own instructions before work. Preserve other chats, accounts and active sessions. Use isolated worktrees and the registered build host for heavy tests. Never copy authentication or log secrets. Record useful verified knowledge using the existing brain workflow. Name this conversation with a concise title once its subject is clear."""


def agent_full_access():
    try:
        return (
            json.loads((Path.home() / ".config/nacre/profile.json").read_text()).get(
                "aiFullAccess"
            )
            is True
        )
    except (OSError, ValueError, AttributeError):
        return False


def emit(value):
    print(json.dumps(value), flush=True)


def atomic(path, value):
    spec = spec_from_file_location(
        "palette", Path(__file__).with_name("classic-state.py")
    )
    m = module_from_spec(spec)
    spec.loader.exec_module(m)
    m.atomic_write(path, json.dumps(value))


def preferences():
    w = runpy.run_path(str((HOME / ".local/bin/nacre-ai").resolve()))
    agent = w["default_assistant"]()
    return agent, w["effective_account"](agent)


def env_for(agent, account):
    if agent == "codex":
        return runpy.run_path(str((HOME / ".local/bin/nacre-ai").resolve()))[
            "local_environment"
        ](account)
    return runpy.run_path(str((HOME / ".local/bin/nacre-ai-claude").resolve()))[
        "environment"
    ](account)


def friendly_error(error):
    text = str(error).lower()
    if any(s in text for s in ("sign in", "login", "authentication", "unauthorized")):
        return "Sign in to the selected assistant account in AI settings."
    if any(s in text for s in ("usage limit", "rate limit", "quota")):
        return "The selected assistant account has reached its current usage limit."
    return "The assistant could not finish this request. Try again or open AI settings."


def text_content(content):
    if isinstance(content, str):
        return content
    return "\n".join(
        c.get("text", "") for c in content or [] if c.get("type") == "text"
    )


def selected_files(values):
    spec = spec_from_file_location("composer", Path(__file__).with_name("composer.py"))
    m = module_from_spec(spec)
    spec.loader.exec_module(m)
    return m.attachments(values or [])


def model_input(text, files):
    return [
        {"type": "text", "text": text},
        *[{"type": "localImage", "path": f["path"]} for f in files if f["image"]],
    ]


class CodexPeer:
    def __init__(self, manager, metadata):
        self.manager = manager
        self.generation = manager.generation
        self.metadata = metadata
        self.sequence = 0
        self.waiting = {}
        self.lock = threading.Lock()
        self.turn = ""
        self.process = subprocess.Popen(
            [str(HOME / ".local/bin/codex"), "app-server", "--stdio"],
            env=env_for("codex", metadata["account"]),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        threading.Thread(target=self.read, daemon=True).start()
        try:
            self.call(
                "initialize",
                {
                    "clientInfo": {
                        "name": "nacre_sidebar",
                        "title": "Nacre AI Sidebar",
                        "version": "1.0",
                    },
                    "capabilities": {"experimentalApi": True},
                },
            )
            self.write({"method": "initialized"})
            if metadata.get("id"):
                result = self.call("thread/resume", {"threadId": metadata["id"]})
                self.manager.restore(result["thread"].get("turns", []))
            else:
                result = self.call(
                    "thread/start",
                    {
                        "cwd": metadata["cwd"],
                        "approvalPolicy": "never",
                        "sandbox": (
                            "danger-full-access"
                            if agent_full_access()
                            else "workspace-write"
                        ),
                        "developerInstructions": INSTRUCTIONS,
                    },
                )
            metadata["id"] = result["thread"]["id"]
            metadata["title"] = result["thread"].get("name") or metadata.get(
                "title", "New chat"
            )
            metadata["model"] = result.get("model", "")
            self.manager.save()
            self.manager.update()
        except Exception as error:  # noqa: BLE001 — Optional agent protocol boundary; preserve the session and report failure.
            self.close()
            raise RuntimeError(friendly_error(error)) from None

    def write(self, value):
        with self.lock:
            self.process.stdin.write((json.dumps(value) + "\n").encode())
            self.process.stdin.flush()

    def call(self, method, params):
        with self.lock:
            self.sequence += 1
            ident = self.sequence
        answer = queue.Queue()
        self.waiting[ident] = answer
        self.write({"id": ident, "method": method, "params": params})
        try:
            message = answer.get(timeout=45)
            if "error" in message:
                raise RuntimeError(message["error"].get("message", "Request failed"))
            return message["result"]
        finally:
            self.waiting.pop(ident, None)

    def read(self):
        try:
            for line in self.process.stdout:
                try:
                    message = json.loads(line)
                except ValueError:
                    continue
                if "id" in message and "method" not in message:
                    waiter = self.waiting.get(message["id"])
                    if waiter:
                        waiter.put(message)
                elif "id" in message:
                    self.request(message)
                else:
                    self.event(message)
        except (OSError, ValueError):
            pass
        finally:
            if self.generation == self.manager.generation and self.manager.busy:
                self.manager.finish(
                    "The assistant connection ended; your conversation is saved."
                )

    def request(self, message):
        method = message["method"]
        params = message.get("params", {})
        ident = message["id"]
        if method == "item/tool/requestUserInput":
            self.manager.ask(self, ident, params.get("questions", []), "input")
        elif method in (
            "item/commandExecution/requestApproval",
            "item/fileChange/requestApproval",
        ):
            self.manager.ask(
                self,
                ident,
                [
                    dict(
                        id="approval",
                        header="Permission",
                        question=params.get("reason") or "Allow this operation?",
                        options=[dict(label="Allow once"), dict(label="Decline")],
                    )
                ],
                "approval",
            )
        else:
            self.write(
                {
                    "id": ident,
                    "error": {
                        "code": -32601,
                        "message": "This client does not support that interaction.",
                    },
                }
            )

    def answer(self, ident, answers, kind):
        result = (
            {
                "decision": "accept"
                if answers.get("approval", {}).get("answers", ["Decline"])[0]
                == "Allow once"
                else "decline"
            }
            if kind == "approval"
            else {"answers": answers}
        )
        self.write({"id": ident, "result": result})

    def event(self, message):
        if self.generation != self.manager.generation:
            return
        method = message.get("method")
        p = message.get("params", {})
        m = self.manager
        if (
            p.get("threadId")
            and self.metadata.get("id")
            and p["threadId"] != self.metadata["id"]
        ):
            return
        if method == "turn/started":
            self.turn = p.get("turn", {}).get("id", "")
            m.busy = True
            m.status = "Working"
            m.update()
        elif method == "item/agentMessage/delta":
            m.delta(p.get("itemId", "reply"), p.get("delta", ""))
        elif method == "item/completed":
            item = p.get("item", {})
            kind = item.get("type")
            if kind == "agentMessage":
                m.complete_message(item["id"], item.get("text", ""))
            if kind == "agentMessage" and item.get("questions"):
                m.ask_async(item["id"], item["questions"])
            elif kind == "userMessage":
                m.native_user(item["id"], text_content(item.get("content")))
        elif method == "item/started":
            item = p.get("item", {})
            kind = item.get("type", "")
            if kind == "agentMessage" and item.get("questions"):
                m.ask_async(item["id"], item["questions"])
            if kind in ("commandExecution", "fileChange", "mcpToolCall", "webSearch"):
                m.status = {
                    "commandExecution": "Running a command",
                    "fileChange": "Editing files",
                    "mcpToolCall": "Using a tool",
                    "webSearch": "Searching the web",
                }[kind]
                m.update()
        elif method == "turn/completed":
            turn = p.get("turn", {})
            self.turn = ""
            error = (
                friendly_error(turn.get("error"))
                if turn.get("status") == "failed"
                else ""
            )
            m.finish(error)
        elif method == "thread/name/updated":
            self.metadata["title"] = (
                p.get("threadName") or p.get("name") or self.metadata["title"]
            )
            m.save()
            m.update()
        elif method == "error" and not p.get("willRetry", False):
            m.finish(friendly_error(p.get("error")))

    def send(self, text, files=None):
        result = self.call(
            "turn/start",
            {"threadId": self.metadata["id"], "input": model_input(text, files or [])},
        )
        self.turn = result["turn"]["id"]

    def steer(self, text, files=None):
        turn = self.turn
        if not turn:
            return self.send(text, files) if files else self.send(text)
        try:
            self.call(
                "turn/steer",
                {
                    "threadId": self.metadata["id"],
                    "expectedTurnId": turn,
                    "input": model_input(text, files or []),
                },
            )
        except RuntimeError as error:
            # The response can finish between clicking Send and the RPC arriving.
            if not self.turn or "no active turn" in str(error).lower():
                return self.send(text, files) if files else self.send(text)
            raise

    def stop(self):
        if self.turn:
            self.call(
                "turn/interrupt", {"threadId": self.metadata["id"], "turnId": self.turn}
            )

    def close(self):
        if self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=8)
            except subprocess.TimeoutExpired:
                self.process.kill()
                self.process.wait(timeout=3)


class ClaudePeer:
    def __init__(self, manager, metadata):
        self.manager = manager
        self.metadata = metadata
        self.process = None
        if not metadata.get("id"):
            metadata["id"] = str(uuid.uuid4())
            metadata["started"] = False
        if metadata.get("started"):
            sdk = HOME / ".local/share/nacre/ai/claude-sdk/bin/python"
            if sdk.exists():
                script = """import json,sys
from claude_agent_sdk import get_session_messages
rows=[]
for item in get_session_messages(sys.argv[1],directory=sys.argv[2]):
 if item.type not in ('user','assistant'):continue
 content=item.message.get('content',[])
 text=content if isinstance(content,str) else chr(10).join(block.get('text','') for block in content if block.get('type')=='text')
 if text:rows.append(dict(id=item.uuid,role=item.type,text=text))
print(json.dumps(rows[-300:]))
"""
                result = subprocess.run(
                    [str(sdk), "-c", script, metadata["id"], metadata["cwd"]],
                    env=env_for("claude", metadata["account"]),
                    capture_output=True,
                    text=True,
                    timeout=20,
                )
                if result.returncode == 0:
                    self.manager.messages = json.loads(result.stdout)
                    self.manager.broadcast(
                        dict(type="history", messages=self.manager.messages)
                    )
        self.manager.save()
        self.manager.update()

    def send(self, text, files=None):
        argv = [
            str(HOME / ".local/bin/claude"),
            "-p",
            "--output-format",
            "stream-json",
            "--verbose",
            "--include-partial-messages",
            *(["--dangerously-skip-permissions"] if agent_full_access() else []),
            "--append-system-prompt",
            INSTRUCTIONS,
        ]
        argv += (
            ["--resume", self.metadata["id"]]
            if self.metadata.get("started")
            else ["--session-id", self.metadata["id"]]
        )
        self.process = subprocess.Popen(
            argv,
            cwd=self.metadata["cwd"],
            env=env_for("claude", self.metadata["account"]),
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            start_new_session=True,
        )
        self.process.stdin.write(text.encode())
        self.process.stdin.close()
        ident = "claude-" + uuid.uuid4().hex
        streamed = False
        error = ""
        for line in self.process.stdout:
            try:
                event = json.loads(line)
            except ValueError:
                continue
            if event.get("type") == "stream_event":
                if event.get("event", {}).get("type") == "message_start":
                    ident = (
                        event["event"].get("message", {}).get("id")
                        or "claude-" + uuid.uuid4().hex
                    )
                    streamed = False
                delta = event.get("event", {}).get("delta", {})
                if delta.get("type") == "text_delta":
                    self.manager.delta(ident, delta.get("text", ""))
                    streamed = True
            elif event.get("type") == "assistant":
                body = text_content(event.get("message", {}).get("content"))
                if body and not streamed:
                    self.manager.complete_message(ident, body)
                for block in event.get("message", {}).get("content", []):
                    if block.get("type") == "tool_use":
                        self.manager.status = "Using " + block.get("name", "a tool")
                        self.manager.update()
            elif event.get("type") == "result":
                if event.get("is_error"):
                    error = friendly_error(event.get("result", ""))
                self.metadata["started"] = True
                self.manager.save()
        code = self.process.wait()
        self.manager.finish(error or ("Response interrupted" if code else ""))

    def stop(self):
        if self.process and self.process.poll() is None:
            os.killpg(self.process.pid, signal.SIGINT)

    def close(self):
        if self.process and self.process.poll() is None:
            self.process.terminate()


class Manager:
    def __init__(self):
        self.lock = threading.RLock()
        self.connect_lock = threading.Lock()
        self.send_lock = threading.Lock()
        self.generation = 0
        self.clients = []
        self.messages = []
        self.busy = False
        self.status = ""
        self.error = ""
        self.peer = None
        self.question = None
        self.async_questions = []
        self.pending_users = []
        self.metadata = {}
        self.path = None
        self.choose()

    def choose(self, item=None, fresh=False):
        self.generation += 1
        agent, account = (
            (item.get("agent", "codex"), item.get("account", ""))
            if item
            else preferences()
        )
        key = hashlib.sha256((agent + "|" + account).encode()).hexdigest()[:20]
        self.path = STATE / (key + ".json")
        meta = (
            json.loads(self.path.read_text())
            if self.path.exists() and not fresh and not item
            else None
        )
        if item:
            meta = dict(
                agent=agent,
                account=account,
                id=item["id"],
                cwd=item.get("cwd") or item["project"]["path"],
                title=item["title"],
                started=True,
            )
        if not meta:
            cwd = HOME / ".local/share/nacre/ai/chats" / uuid.uuid4().hex
            cwd.mkdir(mode=0o700, parents=True)
            guidance = HOME / ".codex/AGENTS.md"
            if guidance.exists():
                (cwd / "AGENTS.md").symlink_to(guidance)
            meta = dict(agent=agent, account=account, cwd=str(cwd), title="New chat")
        self.metadata = meta
        self.messages = []
        self.pending_users = []
        self.question = None
        self.async_questions = []
        self.status = ""
        self.error = ""
        self.peer = None
        self.save()

    def save(self):
        atomic(self.path, self.metadata)

    def state(self):
        return dict(
            type="state",
            defaultProvider=preferences()[0],
            inWorkspace=self.metadata.get("in_workspace", False),
            provider=self.metadata.get("agent"),
            account=self.metadata.get("account") or "Default",
            title=self.metadata.get("title", "New chat"),
            threadId=self.metadata.get("id", ""),
            model=self.metadata.get("model", ""),
            features=["attachments"],
            composerKey=self.metadata.get("id")
            or hashlib.sha256(self.metadata.get("cwd", "").encode()).hexdigest(),
            busy=self.busy,
            status=self.status,
            error=self.error,
            question=self.question
            and {k: v for k, v in self.question.items() if k not in ("peer", "raw_id")},
        )

    def broadcast(self, value):
        encoded = (json.dumps(value) + "\n").encode()
        with self.lock:
            for client in self.clients[:]:
                try:
                    client.sendall(encoded)
                except OSError:
                    self.clients.remove(client)

    def update(self):
        self.broadcast(self.state())

    def restore(self, turns):
        self.messages = []
        pending = []
        for turn in turns:
            for item in turn.get("items", []):
                if item.get("type") == "userMessage":
                    pending = []
                    self.messages.append(
                        dict(
                            id=item["id"],
                            role="user",
                            text=text_content(item.get("content")),
                        )
                    )
                elif item.get("type") == "agentMessage":
                    if item.get("text"):
                        self.messages.append(
                            dict(
                                id=item["id"],
                                role="assistant",
                                text=item.get("text", ""),
                            )
                        )
                    if item.get("questions"):
                        pending.append(item)
        for item in pending:
            self.ask_async(item["id"], item["questions"])
        self.broadcast(dict(type="history", messages=self.messages))

    def delta(self, ident, text):
        if not text:
            return
        with self.lock:
            item = next((m for m in self.messages if m["id"] == ident), None)
            if item is None:
                item = dict(id=ident, role="assistant", text="")
                self.messages.append(item)
            item["text"] += text
        self.broadcast(dict(type="delta", id=ident, text=text))

    def complete_message(self, ident, text):
        if not text:
            return
        with self.lock:
            item = next((m for m in self.messages if m["id"] == ident), None)
            if item is None:
                self.messages.append(dict(id=ident, role="assistant", text=text))
            else:
                item["text"] = text
        self.broadcast(dict(type="message", id=ident, role="assistant", text=text))

    def native_user(self, ident, text):
        with self.lock:
            if text in self.pending_users:
                self.pending_users.remove(text)
                return
        self.messages.append(dict(id=ident, role="user", text=text))
        self.broadcast(self.messages[-1] | {"type": "message"})

    def finish(self, error=""):
        self.busy = False
        self.status = ""
        self.error = error
        self.question = next(iter(self.async_questions), None)
        self.update()

    def ask(self, peer, ident, questions, kind):
        self.question = dict(
            peer=peer, raw_id=ident, id=str(ident), questions=questions, kind=kind
        )
        self.status = "Waiting for your answer"
        self.update()

    def ask_async(self, ident, questions):
        key = "async-" + str(ident)
        if key in self.metadata.get("answered_questions", []) or any(
            q["id"] == key for q in self.async_questions
        ):
            return
        normal = [
            dict(
                id=str(i),
                question=q["title"],
                options=[dict(label=o) for o in q.get("options") or []],
            )
            for i, q in enumerate(questions)
        ]
        if not normal:
            return
        question = dict(id=key, questions=normal, kind="async")
        self.async_questions.append(question)
        if not self.question:
            self.question = question
        self.update()

    def ensure_peer(self):
        with self.connect_lock:
            if self.peer:
                return
            if self.metadata.get("in_workspace"):
                raise RuntimeError("This chat is open in the workspace.")
            self.status = "Connecting"
            self.update()
            try:
                self.peer = (
                    CodexPeer if self.metadata["agent"] == "codex" else ClaudePeer
                )(self, self.metadata)
                if not self.busy:
                    self.status = ""
                    self.update()
            except Exception as e:  # noqa: BLE001 — Optional agent protocol boundary; preserve the session and report failure.
                self.finish(friendly_error(e))
                raise

    def send(self, text, files=None):
        # Serialise submissions, not the UI. Codex returns from send immediately;
        # Claude's current CLI transport queues here until its response completes.
        with self.send_lock:
            self.submit(text, files)

    def submit(self, text, files=None):
        item = None
        original = text
        try:
            files = selected_files(files)
            self.ensure_peer()
            if not self.metadata.get("id"):
                raise RuntimeError("No assistant session")
            if self.metadata.get("title") == "New chat":
                self.metadata["title"] = " ".join(text.split()[:7])[:80]
                self.save()
                if isinstance(self.peer, CodexPeer):
                    self.peer.call(
                        "thread/name/set",
                        {
                            "threadId": self.metadata["id"],
                            "name": self.metadata["title"],
                        },
                    )
            display = text + (
                "\n\nAttached: " + ", ".join(f["name"] for f in files) if files else ""
            )
            if files:
                text += "\n\nUser-selected local files:\n" + "\n".join(
                    f["path"] for f in files
                )
            self.pending_users.append(text)
            item = dict(id="user-" + uuid.uuid4().hex, role="user", text=display)
            self.messages.append(item)
            self.broadcast(item | {"type": "message"})
            self.busy = True
            self.status = "Working"
            self.update()
            if isinstance(self.peer, CodexPeer) and self.peer.turn:
                self.peer.steer(text, files) if files else self.peer.steer(text)
            else:
                self.peer.send(text, files) if files else self.peer.send(text)
        except Exception as e:  # noqa: BLE001 — Optional agent protocol boundary; preserve the session and report failure.
            with self.lock:
                if text in self.pending_users:
                    self.pending_users.remove(text)
                if item in self.messages:
                    self.messages.remove(item)
            self.broadcast(dict(type="history", messages=self.messages))
            self.broadcast(dict(type="sendFailed", text=original, attachments=files))
            if isinstance(self.peer, CodexPeer) and self.peer.turn:
                self.error = friendly_error(e)
                self.update()
            else:
                self.finish(friendly_error(e))

    def handle(self, command):
        action = command.get("action")
        if action == "send":
            text = str(command.get("text", "")).strip()
            if not text or len(text) > 64000:
                return
            if self.metadata.get("in_workspace"):
                return
            self.busy = True
            self.error = ""
            self.update()
            threading.Thread(
                target=self.send,
                args=(text, command.get("attachments", []))
                if command.get("attachments")
                else (text,),
                daemon=True,
            ).start()
        elif action == "stop":
            if self.peer:
                threading.Thread(target=self.peer.stop, daemon=True).start()
        elif action in ("new", "load"):
            if self.busy:
                return
            if self.peer:
                self.peer.close()
            item = None
            if action == "load":
                item = json.loads(
                    (HOME / ".local/state/nacre/shell/extras/chats.json").read_text()
                )[command["key"]]
            self.choose(item, fresh=action == "new")
            self.broadcast(dict(type="history", messages=[]))
            self.update()
            if item:
                threading.Thread(target=self.connect_background, daemon=True).start()
        elif (
            action == "answer"
            and self.question
            and str(command.get("id")) == self.question["id"]
        ):
            q = self.question
            answers = command.get("answers", {})
            if any(
                not answers.get(item["id"], {}).get("answers")
                or not str(answers[item["id"]]["answers"][0]).strip()
                for item in q["questions"]
            ):
                return
            if q["kind"] == "async":
                text = "\n\n".join(
                    item["question"]
                    + "\nAnswer: "
                    + str(answers[item["id"]]["answers"][0])
                    for item in q["questions"]
                )
                self.async_questions.remove(q)
                self.metadata.setdefault("answered_questions", []).append(q["id"])
                self.save()
                self.question = next(iter(self.async_questions), None)
                self.update()
                self.handle(dict(action="send", text=text))
            else:
                q["peer"].answer(q["raw_id"], answers, q["kind"])
                self.question = next(iter(self.async_questions), None)
                self.status = "Working"
                self.update()
        elif action == "provider":
            agent = command.get("provider")
            if agent not in ("codex", "claude"):
                return
            workflow = runpy.run_path(str((HOME / ".local/bin/nacre-ai").resolve()))
            workflow["set_default_assistant"](agent)
            self.update()
        elif action == "workspace":
            if self.busy or not self.metadata.get("id"):
                return
            path = HOME / ".local/state/nacre/shell/extras/chats.json"
            records = json.loads(path.read_text()) if path.exists() else {}
            key = uuid.uuid4().hex[:24]
            records[key] = dict(
                id=self.metadata["id"],
                title=self.metadata["title"],
                agent=self.metadata["agent"],
                account=self.metadata["account"],
                cwd=self.metadata["cwd"],
                state="saved",
                project=dict(
                    id="general-chat", path=self.metadata["cwd"], chat_directory=True
                ),
            )
            atomic(path, records)
            from importlib.util import spec_from_file_location, module_from_spec

            spec = spec_from_file_location(
                "extras", Path(__file__).with_name("desktop-extras.py")
            )
            m = module_from_spec(spec)
            spec.loader.exec_module(m)
            # Wait for our native app-server to exit before the terminal resumes this ID.
            # Keep the transcript here, but never reconnect to a handed-off conversation.
            self.generation += 1
            if self.peer:
                self.peer.close()
                self.peer = None
            self.metadata["in_workspace"] = True
            self.save()
            self.status = (
                "Continue this chat in the workspace, or start a new chat here."
            )
            self.update()
            m.resume(key)

    def connect_background(self):
        try:
            self.ensure_peer()
        except Exception:  # noqa: BLE001 — Optional agent protocol boundary; preserve the session and report failure.
            pass

    def client(self, connection):
        with self.lock:
            connection.sendall(
                (
                    json.dumps(dict(type="history", messages=self.messages))
                    + "\n"
                    + json.dumps(self.state())
                    + "\n"
                ).encode()
            )
            self.clients.append(connection)
        if (
            self.metadata.get("id")
            and self.peer is None
            and not self.metadata.get("in_workspace")
        ):
            threading.Thread(target=self.connect_background, daemon=True).start()
        try:
            with connection.makefile("r") as stream:
                for line in stream:
                    try:
                        self.handle(json.loads(line))
                    except Exception:  # noqa: BLE001 — Optional agent protocol boundary; preserve the session and report failure.
                        self.error = "Could not complete that chat action."
                        self.update()
        finally:
            with self.lock:
                if connection in self.clients:
                    self.clients.remove(connection)
            connection.close()


def server():
    STATE.mkdir(mode=0o700, parents=True, exist_ok=True)
    STATE.chmod(0o700)
    manager = Manager()
    SOCKET.unlink(missing_ok=True)
    sock = socket.socket(socket.AF_UNIX)
    sock.bind(str(SOCKET))
    SOCKET.chmod(0o600)
    sock.listen(5)
    sock.settimeout(5)
    idle_since = time.monotonic()
    try:
        while True:
            try:
                connection, _ = sock.accept()
            except TimeoutError:
                with manager.lock:
                    active = bool(manager.clients) or manager.busy
                if active:
                    idle_since = time.monotonic()
                elif CHANNEL and time.monotonic() - idle_since >= 90:
                    if manager.peer:
                        manager.peer.close()
                    manager.save()
                    return
                continue
            idle_since = time.monotonic()
            threading.Thread(
                target=manager.client, args=(connection,), daemon=True
            ).start()
    finally:
        sock.close()
        SOCKET.unlink(missing_ok=True)


def client():
    start_service()
    incoming = queue.Queue()

    def input_lines():
        for line in sys.stdin:
            incoming.put(line.encode())
        incoming.put(None)

    threading.Thread(target=input_lines, daemon=True).start()
    sock = None
    buffer = b""
    while True:
        if sock is None:
            for _ in range(100):
                candidate = socket.socket(socket.AF_UNIX)
                try:
                    candidate.connect(str(SOCKET))
                    sock = candidate
                    sock.settimeout(0.1)
                    break
                except (FileNotFoundError, ConnectionRefusedError):
                    candidate.close()
                    time.sleep(0.1)
            if sock is None:
                emit(
                    dict(
                        type="connection",
                        connected=False,
                        error="Reopen the drawer to reconnect.",
                    )
                )
                return
            buffer = b""
        try:
            while not incoming.empty():
                line = incoming.get_nowait()
                if line is None:
                    sock.close()
                    return
                sock.sendall(line)
            try:
                data = sock.recv(65536)
            except socket.timeout:
                continue
            if not data:
                raise ConnectionResetError()
            buffer += data
            while b"\n" in buffer:
                line, buffer = buffer.split(b"\n", 1)
                sys.stdout.buffer.write(line + b"\n")
                sys.stdout.flush()
        except OSError:
            sock.close()
            sock = None
            emit(dict(type="connection", connected=False, error="Reconnecting…"))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("action", choices=("client", "server"))
    parser.add_argument("--channel", default="")
    options = parser.parse_args()
    configure_channel(options.channel)
    server() if options.action == "server" else client()
