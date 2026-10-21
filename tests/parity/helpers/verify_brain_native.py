"""Disposable N0020 exchange comparison and private fake-SSH integration."""

import json
import os
import pty
import signal
from pathlib import Path
import shutil
import subprocess
import tempfile
import termios
import time

ROOT = Path(__file__).resolve().parents[3]
HELPER = Path(
    os.environ.get("NACRE_HELPER_BINARY", ROOT / "target/release/nacre-helper")
)


def run(argv, env, request=None):
    result = subprocess.run(
        argv,
        env=env,
        input=json.dumps(request) if request is not None else None,
        text=True,
        capture_output=True,
        timeout=70,
    )
    if result.returncode:
        raise AssertionError((argv, result.returncode, result.stdout, result.stderr))
    return result.stdout


with tempfile.TemporaryDirectory(prefix="n0020-disposable-") as temporary:
    work = Path(temporary)
    reference = work / "reference"
    reference.mkdir()
    for name in ("nacre-brain", "nacre-brain-sync", "nacre-brain-maintain"):
        (reference / (name + ".py.rollback")).write_bytes(
            (ROOT / "bin" / (name + ".py.rollback")).read_bytes()
        )
    shutil.copy(ROOT / "nacre/shell-tools/nacre_state.py", reference / "nacre_state.py")
    shutil.copy(
        ROOT / "nacre/shell-tools/nacre-state-files.json",
        reference / "nacre-state-files.json",
    )
    homes = [work / "python", work / "native"]
    environments = []
    for home in homes:
        home.mkdir()
        environments.append(
            dict(os.environ, HOME=str(home), NACRE_BRAIN=str(home / "vault"))
        )
    commands = [
        ["python", str(reference / "nacre-brain-sync.py.rollback"), "--exchange"],
        [str(HELPER), "nacre-brain-sync", "--exchange"],
    ]
    batches = [
        (
            {
                "wiki/p.md": "old",
                "raw/source.txt": "source",
                "wiki/view.base": "views: []",
                "wiki/map.canvas": "{}",
                "inbox/unicode.md": "Æøå 😀",
                "wiki/back\\slash.md": "literal filename",
            },
            {},
        ),
        ({"wiki/p.md": "new"}, {"wiki/p.md": "old"}),
        ({"wiki/p.md": "remote"}, {"wiki/p.md": "base"}),
        ({"wiki/p.md": "base"}, {"wiki/p.md": "base"}),
        ({}, {"wiki/p.md": "new"}),
    ]
    for index, (notes, base) in enumerate(batches, 1):
        request = {"version": 2, "notes": notes, "base": base}
        replies = [
            json.loads(run(command, environment, request))
            for command, environment in zip(commands, environments, strict=True)
        ]
        assert replies[0] == replies[1], (
            "STOP: merge-output divergence",
            index,
            replies,
        )
        conflicts = []
        for home in homes:
            conflicts.append(
                {
                    path.name: json.loads(path.read_text())
                    for path in (home / "vault/.brain-state/conflicts").glob("*.json")
                }
            )
        assert conflicts[0] == conflicts[1], (
            "STOP: conflict-output divergence",
            index,
            conflicts,
        )
        archives = []
        for home in homes:
            snapshots = {}
            for path in (home / "vault/.brain-state/backups").glob("*.json"):
                record = json.loads(path.read_text())
                snapshots[path.name] = {
                    key: record[key] for key in ("version", "sha256", "files")
                }
            archives.append(snapshots)
        assert archives[0] == archives[1], ("STOP: backup-output divergence", index)
        print(
            f"Exchange comparison {index}: identical notes, changed/conflicts and conflict evidence"
        )
    fake_bin = work / "fake-bin"
    fake_bin.mkdir()
    ssh = fake_bin / "ssh"
    ssh.write_text(
        '#!/bin/sh\nexec env HOME="$N0020_PEER_HOME" NACRE_BRAIN="$N0020_PEER_VAULT" "$NACRE_HELPER_BINARY" nacre-brain-sync --exchange\n'
    )
    ssh.chmod(0o700)
    local, peer = work / "local", work / "peer"
    local.mkdir()
    peer.mkdir()
    environment = dict(
        os.environ,
        HOME=str(local),
        NACRE_BRAIN=str(local / "vault"),
        XDG_STATE_HOME=str(local / ".local/state"),
        XDG_CONFIG_HOME=str(local / ".config"),
        N0020_PEER_HOME=str(peer),
        N0020_PEER_VAULT=str(peer / "vault"),
        NACRE_HELPER_BINARY=str(HELPER),
        PATH=str(fake_bin) + ":" + os.environ["PATH"],
    )
    config = local / "sync.json"
    config.write_text('{"host":"fake-peer"}')
    run([str(HELPER), "nacre-brain", "init"], environment)
    (local / "vault/wiki/local.md").write_text("local evidence")
    large = "plain evidence " * 10000
    (local / "vault/wiki/large.md").write_text(large)
    print(
        run(
            [str(HELPER), "nacre-brain-sync", "--config", str(config)], environment
        ).strip()
    )
    assert (peer / "vault/wiki/local.md").read_text() == "local evidence"
    assert (peer / "vault/wiki/large.md").read_text() == large
    (peer / "vault/wiki/remote.md").write_text("remote evidence")
    print(
        run(
            [str(HELPER), "nacre-brain-sync", "--config", str(config)], environment
        ).strip()
    )
    assert (local / "vault/wiki/remote.md").read_text() == "remote evidence"
    (peer / "vault/wiki/local.md").unlink()
    run([str(HELPER), "nacre-brain-sync", "--config", str(config)], environment)
    assert (local / "vault/wiki/local.md").read_text() == "local evidence"
    status = json.loads((local / ".local/state/nacre/brain-sync.json").read_text())
    assert (
        status["state"] == "success"
        and status["lastSuccess"] > 0
        and status["nextRetry"] == 0
    )
    assert all(
        key in status
        for key in ("received", "sent", "conflicts", "failures", "lastAttempt")
    )
    print(
        "Private fake SSH: bidirectional sync, deletion suppression and status schema passed"
    )
    # A cancelled private prompt must restore echo without contacting a keyring.
    master, slave = pty.openpty()

    def controlling_terminal():
        import fcntl

        os.setsid()
        fcntl.ioctl(slave, termios.TIOCSCTTY, 0)

    prompt = subprocess.Popen(
        [str(HELPER), "nacre-brain", "store-credential", "synthetic"],
        env=environment,
        stdin=slave,
        stdout=slave,
        stderr=slave,
        preexec_fn=controlling_terminal,
    )
    try:
        deadline = time.monotonic() + 5
        while termios.tcgetattr(slave)[3] & termios.ECHO:
            assert prompt.poll() is None and time.monotonic() < deadline, (
                "Prompt failed to disable echo"
            )
            time.sleep(0.01)
        os.kill(prompt.pid, signal.SIGINT)
        assert prompt.wait(timeout=5) == 130
        assert termios.tcgetattr(slave)[3] & termios.ECHO, (
            "Cancelled prompt left echo disabled"
        )
    finally:
        if prompt.poll() is None:
            prompt.kill()
            prompt.wait()
        os.close(master)
        os.close(slave)
    print("Private terminal: cancellation restores credential prompt echo")
    keyring = fake_bin / "secret-tool"
    keyring.write_text(
        "#!/usr/bin/env python3\nimport os, sys\nfrom pathlib import Path\n"
        "record = Path(os.environ['N0020_SECRET_RECORD'])\n"
        "if sys.argv[1] == 'store': record.write_text(sys.stdin.read())\n"
        "else: print(record.read_text())\n"
    )
    keyring.chmod(0o700)
    environment["N0020_SECRET_RECORD"] = str(work / "synthetic-secret")
    environment["N0020_LAUNCHED"] = str(work / "launched")
    stored = subprocess.run(
        [str(HELPER), "nacre-brain", "store-credential", "synthetic"],
        input="  synthetic fixture  \n",
        text=True,
        capture_output=True,
        env=environment,
        timeout=5,
        start_new_session=True,
    )
    assert stored.returncode == 0, stored.stderr
    assert (
        Path(environment["N0020_SECRET_RECORD"]).read_text() == "  synthetic fixture  "
    )
    environment.pop("TOKIO_WORKER_THREADS", None)
    launch = [
        str(HELPER),
        "nacre-brain",
        "run-with-credential",
        "--env",
        "N0020_TEST_SECRET",
        "synthetic",
        "--",
        "python",
        "-c",
        "import os, sys; from pathlib import Path; assert os.environ['N0020_TEST_SECRET'] == '  synthetic fixture  '; assert sys.argv[1:] == ['--help', 'a b']; Path(os.environ['N0020_LAUNCHED']).write_text(os.environ.get('TOKIO_WORKER_THREADS', '<unset>'))",
        "--help",
        "a b",
    ]
    run(launch, environment)
    assert Path(environment["N0020_LAUNCHED"]).read_text() == "<unset>"
    environment["TOKIO_WORKER_THREADS"] = "3"
    run(launch, environment)
    assert Path(environment["N0020_LAUNCHED"]).read_text() == "3"
    print(
        "Fake keyring: secret whitespace, inherited environment and literal child argv preserved"
    )
    ssh.write_text(
        '#!/bin/sh\nprintf x >> "$N0020_SSH_MARKER"\n'
        'case "$N0020_SSH_RESULT" in\n'
        'offline) echo "Connection timed out" >&2; exit 255;;\n'
        'authentication) echo "Permission denied (publickey)" >&2; exit 255;;\n'
        'protocol) echo "not found" >&2; exit 127;;\n'
        'malformed) printf \'{"version":2,"notes":[],"changed":[],"conflicts":[]}\';;\n'
        "esac\n"
    )
    environment["N0020_SSH_MARKER"] = str(work / "ssh-marker")
    status_path = local / ".local/state/nacre/brain-sync.json"
    last_success = json.loads(status_path.read_text())["lastSuccess"]
    first_retry = 0
    for category, code in [
        ("offline", 75),
        ("offline", 75),
        ("malformed", 1),
        ("authentication", 1),
        ("protocol", 1),
    ]:
        environment["N0020_SSH_RESULT"] = category
        result = subprocess.run(
            [str(HELPER), "nacre-brain-sync", "--force", "--config", str(config)],
            env=environment,
            text=True,
            capture_output=True,
            timeout=5,
        )
        assert result.returncode == code, (category, result.returncode, result.stderr)
        health = json.loads(status_path.read_text())
        assert health["state"] == ("failed" if category == "malformed" else category), (
            health
        )
        assert health["lastSuccess"] == last_success
        if category == "offline":
            assert health["nextRetry"] > first_retry
            first_retry = health["nextRetry"]
            calls = Path(environment["N0020_SSH_MARKER"]).read_text()
            run([str(HELPER), "nacre-brain-sync", "--config", str(config)], environment)
            assert Path(environment["N0020_SSH_MARKER"]).read_text() == calls
    print(
        "Fake SSH failures: exit 75, backoff skip, malformed reply, authentication and protocol status preserved"
    )
