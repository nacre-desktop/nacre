"""Small native-core client for the remaining Python AI views and peers."""

import json
import os
from pathlib import Path
from subprocess import PIPE as _PIPE, Popen as _Popen, TimeoutExpired

import nacre_state as _state


def _binary():
    override = os.environ.get("NACRE_HELPER_BINARY")
    if override:
        return override
    installed = Path(__file__).with_name("nacre-helper")
    if installed.is_file():
        return str(installed)
    root = Path(__file__).resolve().parents[2]
    candidates = [
        root / "target" / mode / "nacre-helper" for mode in ("release", "debug")
    ]
    candidates = [path for path in candidates if path.is_file()]
    if candidates:
        return str(max(candidates, key=lambda path: path.stat().st_mtime_ns))
    return str(Path.home() / ".local/share/nacre/shell/tools/nacre-helper")


def _call(operation, **request):
    process = _Popen(
        [_binary(), "ai-core"],
        stdin=_PIPE,
        stdout=_PIPE,
        stderr=_PIPE,
        text=True,
    )
    try:
        stdout, stderr = process.communicate(
            json.dumps(dict(operation=operation, **request)), timeout=15
        )
    except TimeoutExpired as error:
        process.kill()
        process.communicate()
        raise OSError("Native AI core request timed out") from error
    except BaseException:
        process.kill()
        process.communicate()
        raise
    if process.returncode:
        if "Newer state version" in stderr:
            raise _state.NewerVersionError(
                "Newer state version; original file preserved"
            )
        if "Unsupported state version" in stderr:
            raise _state.InvalidVersionError(
                "Unsupported state version; original file preserved"
            )
        # Do not forward provider/configuration payloads into a view or log.
        raise ValueError("Native AI core request failed")
    return json.loads(stdout)


def full_access():
    return _call("full-access")["full_access"]


def default_assistant():
    return _call("default-assistant")["agent"]


def effective_account(agent, override=None):
    return _call("effective-account", agent=agent, override=override)["account"]


def environment(agent, account=None, *, local=False, configure=False):
    # A child cannot set its parent's mask. Keep this OS operation in the thin client.
    os.umask(0o077)
    delta = _call(
        "local-environment" if local else "environment", agent=agent, account=account
    )
    result = dict(os.environ)
    for name in delta["unset"]:
        result.pop(name, None)
    result.update(delta["set"])
    if configure and delta.get("account", account):
        # Configuration/MCP setup remains with the existing chat command.
        from subprocess import run

        run(
            [str(Path.home() / ".local/bin/nacre-ai-chat"), "configure"],
            env=result,
            check=True,
        )
    return result


def shared_history_home():
    return Path(_call("shared-history-home")["path"])
