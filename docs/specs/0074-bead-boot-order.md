# N0074 — Order Bead after the daemon

Bead can start during boot before `nacre-daemon.service` is ready.
The shell drop-in already wants the daemon socket, which only activates the socket.
It now wants and orders after the daemon service as well.
Bead's reconnect handling remains necessary when the daemon restarts later.
`daemon.status` also returns the current method registry so late service readiness
can be learned without reconnecting the shell.

| Check | Result |
|---|---|
| Generated unit | `Wants=` and `After=` include `nacre-daemon.socket` and `nacre-daemon.service` |
| Fake manager | Asserts both dependency lines and preserves fallback/rollback coverage |
| Real user manager | PASS: daemon active when Bead starts; persisted restart, crash-loop fallback, rollback and daemon takeover pass |
| Method registry | `daemon.status.methods` mirrors Welcome; hub test observes `menu.query` after readiness |
| Full desktop CI | PASS: remote Arch replica; Python suites 499/120/52/490, parity 534, native and gallery checks |

## Change

Bead's managed shell drop-in now orders after the daemon; daemon status exposes
an additive `methods` snapshot matching Welcome. QML selection, the `OnFailure`
fallback, and journaled rollback convergence are unchanged. The
real-systemd fixture uses a disposable Arch container with a private user
manager and test HOME (`--privileged --cgroupns=private --tmpfs /run
--tmpfs /tmp`); it does not touch host units or the desktop. The rehearsal
command is `python /work/tools/tests/rehearse_ui_shell_systemd.py` as tester.

## Temporary duplicates

None introduced.

## Apply

Deploy the validated Nacre change with the normal Nacre deployment workflow,
then run `nacre ui-shell bead` to rewrite the managed drop-in and restart the
shell under the new ordering. Recover with `nacre ui-shell qml` if needed.
