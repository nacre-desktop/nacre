#!/usr/bin/env python3
"""Disposable-container only: real systemd user manager, stub Bead and daemon.

Exercises Stage A migration/recovery wiring. The stub UI provider proves only that
systemd selected and kept the requested launcher alive; it does not prove Bead UI.
"""

import fcntl
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import time

home = Path.home()
assert str(home) == "/home/tester" and os.getuid() == 1000
assert Path("/.dockerenv").is_file()
repo = Path(__file__).resolve().parents[2]
unit_dir = home / ".config/systemd/user"
assert not unit_dir.exists(), "Use a fresh disposable tester HOME"
unit_dir.mkdir(parents=True)
rt = Path(os.environ["XDG_RUNTIME_DIR"])
state = home / ".local/state/nacre/releases"
state.mkdir(parents=True, mode=0o700, exist_ok=True)
(state / "release.lock").touch(mode=0o600)
control = home / ".local/share/nacre/control"
control.mkdir(parents=True)
for name in ("releases.py", "f4_switch.py", "ui_shell_switch.py"):
    shutil.copy2(repo / "tools" / name, control / name)
shared = home / ".local/share/nacre/shell/tools"
shared.mkdir(parents=True)
for name in ("nacre_state.py", "nacre-state-files.json", "writer_lease.py"):
    shutil.copy2(repo / "nacre/shell-tools" / name, shared / name)


def run(*args, check=True):
    p = subprocess.run(args, text=True, capture_output=True, timeout=120)
    print("$", *args, "\n", p.stdout, p.stderr, flush=True)
    if check:
        p.check_returncode()
    return p


def ui(*args):
    return json.loads(
        run(sys.executable, str(repo / "tools/releases.py"), "ui-shell", *args).stdout
    )


def ctl(*args, check=True):
    return run("systemctl", "--user", *args, check=check)


def write(path, data, mode=0o644):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(data)
    path.chmod(mode)


helper = home / ".local/share/nacre/shell/tools/nacre-helper"
write(
    helper,
    """#!/usr/bin/python
import json,sys
if 'ui.dispatch' in sys.argv:
 print('UI method is unavailable',file=sys.stderr); sys.exit(1)
if 'daemon.status' in sys.argv:
 import os
 try: flags=open(os.environ['XDG_RUNTIME_DIR']+'/daemon-args').read()
 except OSError: flags=''
 mine='--notifications=daemon' in flags
 print(json.dumps({'boot':'stub','safe':False,'services':{'settings':{'status':'running'},**({'notifications':{'status':'running'}} if mine else {})},'topics':['settings.store.desktop']+(['notifications'] if mine else [])}))
else:
 print(json.dumps({'store':'desktop','rev':1,'exists':True,'data':{},'error':None,'dirty':False}))
""",
    0o700,
)
qml = home / "qml"
write(qml, "#!/bin/sh\nexec /usr/bin/sleep infinity\n", 0o700)


def build(label):
    binary = home / f"bead-{label}"
    script = "\n".join(
        [
            "#!/bin/sh",
            'case "${1:-}" in --help) exit 0 ;; --bar) ;; *) exit 2 ;; esac',
            "systemctl --user is-active --quiet nacre-daemon.service || { echo 'daemon was not active when Bead started' >&2; exit 91; }",
            './bead-wallpaper-video --daemon-socket "${NACRE_DAEMON_SOCKET:-$XDG_RUNTIME_DIR/nacre/daemon.sock}" &',
            f'echo $! > "$XDG_RUNTIME_DIR/wallpaper-{label}.pid"',
            f'while [ ! -e "$XDG_RUNTIME_DIR/crash-{label}" ]; do sleep .2; done',
            "exit 3",
            "",
        ]
    )
    write(binary, script, 0o700)
    write(
        home / "bead-wallpaper-video",
        '#!/bin/sh\n[ "${1:-}" = --check ] && exit 0\necho $$ > "$XDG_RUNTIME_DIR/wallpaper.pid"\nwhile :; do sleep 1; done\n',
        0o700,
    )
    write(home / "nacre-wallpaper-decode", "#!/bin/sh\nexit 1\n", 0o700)
    shell = home / f"build-shell-{label}"
    (shell / "bar").mkdir(parents=True, exist_ok=True)
    write(shell / "palette.json", "{}")
    return binary, shell


good_binary, good_shell = build("good")
candidate_binary, candidate_shell = build("candidate")
write(
    unit_dir / "nacre-shell.service",
    f"[Service]\nExecStart={qml} start\nRestart=on-failure\nRestartSec=2\n[Install]\nWantedBy=default.target\n",
)
write(
    unit_dir / "nacre-shell.service.d/50-nacre-f4.conf",
    "[Service]\nEnvironment=NACRE_DAEMON_CLIENT=1\n",
)
old_fallback = "# managed by nacre ui-shell; remove with: nacre ui-shell qml\n[Unit]\nDescription=Return to the QML shell after a Bead failure\n\n[Service]\nType=oneshot\nExecStart=/usr/bin/python tools/releases.py ui-shell qml --recover\n"
daemon = home / "stub-daemon"
write(
    daemon,
    '#!/bin/sh\nprintf "%s\\n" "$*" > "$XDG_RUNTIME_DIR/daemon-args"\ntouch "$XDG_RUNTIME_DIR/daemon-ready"\nexec /usr/bin/sleep infinity\n',
    0o700,
)
write(unit_dir / "nacre-daemon.service", f"[Service]\nExecStart={daemon}\n")
write(unit_dir / "nacre-daemon.socket", "[Socket]\nListenStream=%t/nacre/daemon.sock\n")
ctl("daemon-reload")
ctl("start", "nacre-daemon.socket")
ctl("enable", "--now", "nacre-shell.service")

# Fresh migration from the existing QML service.
result = ui("migrate", "--binary", str(good_binary), "--shell", str(good_shell))
assert result["shell"] == "bead"
good_launcher = ui("status")["launcher"]
assert ui("status")["active"] == "bead"
fallback = unit_dir / "nacre-ui-shell-fallback.service"
assert "last known-good Bead" in fallback.read_text()
assert "recover-bead --recover" in fallback.read_text()
assert "ui-shell qml --recover" not in fallback.read_text()
assert ctl("is-active", "nacre-shell.service").stdout.strip() == "active"

# In-place Bead update with the old QML fallback unit present.
write(fallback, old_fallback)
ctl("daemon-reload")
updated = ui(
    "migrate", "--binary", str(candidate_binary), "--shell", str(candidate_shell)
)
assert updated["shell"] == "bead"
assert ui("status")["active"] == "bead"
assert "recover-bead --recover" in fallback.read_text()
assert ctl("is-active", "nacre-shell.service").stdout.strip() == "active"
failed_candidate = ui("status")["launcher"]
assert failed_candidate != good_launcher

# Candidate crashes on each start; real systemd exhausts StartLimitBurst and runs OnFailure.
with (state / "release.lock").open() as release_lock:
    fcntl.flock(release_lock, fcntl.LOCK_EX)
    write(rt / "crash-candidate", "1")
    deadline = time.monotonic() + 65
    while time.monotonic() < deadline:
        retries = ctl(
            "show", "nacre-ui-shell-fallback.service", "-p", "NRestarts", "--value"
        ).stdout.strip()
        exhausted = (
            ctl("show", "nacre-shell.service", "-p", "Result", "--value").stdout.strip()
            == "start-limit-hit"
        )
        if retries and int(retries) > 0 and exhausted:
            break
        time.sleep(1)
    assert retries and int(retries) > 0, "busy recovery was not retried"
    assert exhausted, "shell did not reach its start limit while the release lock was held"
# Recovery must resume after the unrelated release drops its lock.
deadline = time.monotonic() + 65
while time.monotonic() < deadline:
    status = ui("status")
    if status["active"] == "bead" and status.get("launcher") == good_launcher:
        break
    time.sleep(1)
assert status["active"] == "bead" and status.get("launcher") == good_launcher, status
assert ctl("is-active", "nacre-shell.service").stdout.strip() == "active"
failed_records = [
    json.loads(path.read_text())
    for path in (home / ".local/state/nacre/ui-shell").glob("*/journal.json")
]
assert any(
    row.get("launcher") == failed_candidate and row.get("recovery_failed")
    for row in failed_records
), "start-limited candidate was not marked failed"

# Roll back to the earlier Bead and then perform another update.
(rt / "crash-candidate").unlink()
retry = ui(
    "migrate", "--binary", str(candidate_binary), "--shell", str(candidate_shell)
)
assert retry["shell"] == "bead"
rolled = ui("rollback")
assert rolled["shell"] == "bead" and rolled.get("launcher") == good_launcher, rolled
again = ui(
    "migrate", "--binary", str(candidate_binary), "--shell", str(candidate_shell)
)
assert again["shell"] == "bead" and ui("status")["active"] == "bead"

# Simulate every retained Bead failing its health check; recovery must stop and journal a hint.
index = home / ".local/state/nacre/ui-shell"
for journal in index.glob("*/journal.json"):
    data = json.loads(journal.read_text())
    data["recovery_failed"] = True
    journal.write_text(json.dumps(data))
write(rt / "crash-candidate", "1")
deadline = time.monotonic() + 65
while time.monotonic() < deadline:
    active = ctl("is-active", "nacre-shell.service", check=False).stdout.strip()
    if active == "inactive":
        break
    time.sleep(1)
assert active == "inactive", f"shell was not cleanly stopped: {active}"
restarts = ctl("show", "nacre-shell.service", "--property=NRestarts").stdout.strip()
time.sleep(3)
assert ctl("is-active", "nacre-shell.service", check=False).stdout.strip() == "inactive"
assert (
    ctl("show", "nacre-shell.service", "--property=NRestarts").stdout.strip()
    == restarts
)
journal = run(
    "journalctl",
    "--user",
    "--identifier=nacre-ui-shell-fallback",
    "--no-pager",
    check=False,
)
assert "No healthy Bead copy remains" in journal.stdout + journal.stderr
assert "shell was stopped to prevent a restart loop" in journal.stdout + journal.stderr

print(
    "PASS: fresh migration → old fallback update → start-limit recovery → rollback → next update → exhausted-copy stop and journal hint",
    flush=True,
)
