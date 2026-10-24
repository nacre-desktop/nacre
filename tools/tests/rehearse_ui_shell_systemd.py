#!/usr/bin/env python3
"""Disposable-container only: real systemd user manager, stub Bead and daemon."""

import json
import os
from pathlib import Path
import subprocess
import sys
import time

home = Path.home()
assert str(home) == "/home/tester" and os.getuid() == 1000
assert Path("/.dockerenv").is_file()
repo = Path(__file__).resolve().parents[2]
root = home / ".config/systemd/user"
assert not root.exists(), "Use a fresh disposable tester HOME"
root.mkdir(parents=True)
rt = Path(os.environ["XDG_RUNTIME_DIR"])
state = home / ".local/state/nacre/releases"
state.mkdir(parents=True, mode=0o700, exist_ok=True)
(state / "release.lock").touch(mode=0o600)


def run(*args):
    p = subprocess.run(args, text=True, capture_output=True, timeout=120)
    print("$", *args, "\n", p.stdout, p.stderr, flush=True)
    p.check_returncode()
    return p.stdout


def ui(*args):
    return json.loads(
        run(sys.executable, str(repo / "tools/releases.py"), "ui-shell", *args)
    )


def ctl(*args):
    return run("systemctl", "--user", *args)


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
bead = home / "stub-bead"
write(
    bead,
    """#!/bin/sh
[ "${1:-}" = --help ] && exit 0
while [ ! -e "$XDG_RUNTIME_DIR/crash" ]; do sleep .2; done
exit 3
""",
    0o700,
)
shell = home / "build-shell"
(shell / "bar").mkdir(parents=True, exist_ok=True)
write(shell / "palette.json", "{}")
write(
    root / "nacre-shell.service",
    f"[Service]\nExecStart={qml} start\nRestart=on-failure\nRestartSec=2\n[Install]\nWantedBy=default.target\n",
)
write(
    root / "nacre-shell.service.d/50-nacre-f4.conf",
    "[Service]\nEnvironment=NACRE_DAEMON_CLIENT=1\n",
)
# Stub daemon: records the flags it was started with (the helper stub above reads them).
daemon = home / "stub-daemon"
write(
    daemon,
    '#!/bin/sh\nprintf "%s\\n" "$*" > "$XDG_RUNTIME_DIR/daemon-args"\nexec /usr/bin/sleep infinity\n',
    0o700,
)
write(root / "nacre-daemon.service", f"[Service]\nExecStart={daemon}\n")
write(root / "nacre-daemon.socket", "[Socket]\nListenStream=%t/nacre/daemon.sock\n")
ctl("daemon-reload")
ctl("start", "nacre-daemon.socket")
ctl("enable", "--now", "nacre-shell.service")
installed = ui("install", "--binary", str(bead), "--shell", str(shell))
assert Path(installed["launcher"]).is_file()
assert ui("status")["ready_for_bead"] == []
ui("bead", "--notifications", "none")
assert ui("status")["active"] == "bead"
# Real service manager re-reads the persisted selection on the next start.
ctl("stop", "nacre-shell.service")
ctl("stop", "nacre-daemon.socket")
ctl("daemon-reload")
ctl("start", "nacre-shell.service")
assert ctl("is-active", "nacre-daemon.socket").strip() == "active"
assert ui("status")["active"] == "bead"
# ExecStart crashes on each automatic restart, no host process is signalled.
write(rt / "crash", "1")
end = time.monotonic() + 65
dropin = root / "nacre-shell.service.d/60-nacre-ui-shell.conf"
while dropin.exists() and time.monotonic() < end:
    time.sleep(0.5)
assert not dropin.exists(), "OnFailure did not restore QML"
while time.monotonic() < end:
    status = ui("status")
    if status["active"] == "qml" and status["incomplete"] == []:
        break
    time.sleep(1)
assert status["active"] == "qml" and status["incomplete"] == []
assert ctl("is-active", "nacre-shell.service").strip() == "active"
ui("rollback")
assert not (root / "nacre-qml-windows.service").exists()
assert not (root / "nacre-ui-shell-fallback.service").exists()
(rt / "crash").unlink()
ui("bead", "--notifications", "none")
ui("qml")
assert ui("status")["active"] == "qml"
# N0068: daemon notifications take over the daemon flags, and qml gives them back.
args = rt / "daemon-args"


def flags():
    # The unit is Type=simple: restart returns before the stub has written its arguments.
    end = time.monotonic() + 10
    while time.monotonic() < end:
        try:
            return args.read_text().split()
        except OSError:
            time.sleep(0.2)
    raise AssertionError("the daemon stub did not start")


args.unlink(missing_ok=True)
ctl("restart", "nacre-daemon.service")
assert flags() == [], flags()
ui("bead", "--notifications", "daemon")
assert ui("status")["active"] == "bead"
assert flags() == ["--notifications=daemon", "--behaviours=daemon"], flags()
daemon_dropin = root / "nacre-daemon.service.d/60-nacre-ui-shell-daemon.conf"
assert daemon_dropin.is_file()
args.unlink()
ui("qml")
assert ui("status")["active"] == "qml"
assert flags() == [], flags()
assert not daemon_dropin.exists()
print(
    "PASS: install → switch → persisted restart → use → crash-loop fallback → rollback → repeat → daemon takeover and back",
    flush=True,
)
