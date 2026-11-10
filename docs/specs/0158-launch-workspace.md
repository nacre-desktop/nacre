# N0158 — launch workspace affinity

Nacre's native launch paths register the destination before launching an app. The
compositor returns a one-use startup token, which the launcher supplies as both
`XDG_ACTIVATION_TOKEN` and `DESKTOP_STARTUP_ID`. Registration is bounded to 500 ms;
unsupported or failed IPC launches once with the prior environment behavior.
The manager unit name is generated before registration and is the same unit used
for launch, allowing tokenless clients to be correlated by the compositor.

The native menu worker covers launcher, drawer, and dock activation. Helper
workflows register named scopes/services for selected apps, workspace tools,
saved-chat terminals, and the restricted-session host terminal. Other native
window actions pass the returned token through UWSM, AI/utility terminals,
desktop extras, and the screenshot viewer. UWSM arguments keep their original
shape when preparation fails; when a token exists, `env` follows `uwsm app --`
so both scope and service modes pass it to the resolved executable. Asynchronous
screenshot requests do IPC in `spawn_blocking`. Startup-profile apps register
with a fresh token only after switching to their explicit target workspace.
Terminal argv and working directory remain unchanged. The host-terminal
environment allowlist now forwards the two activation variables when present.

The QML launch boundary is deliberately untouched because N0156 removes that
shell. QML-only `tools.launch` and Quickshell extension launches are also outside
this change. Python shell-tool launchers such as `desktop-extras.py` and
`workflow-profiles.py`, direct external launchers without registration, and
tokenless X11 applications remain uncovered here; token-aware X11 handoffs depend
on the compositor and xwayland-satellite forwarding the startup token.

The compositor contract is additive: `NacrePrepareLaunch` accepts an optional
systemd unit and optional activation token, returning `NacreLaunchToken`. A
unitless inherited token continues the same ticket without resetting its captured
workspace or age. It advertises
`launch_workspace`; the compositor enforces the 60-second tracking window, explicit
placement rules and parent/dialog placement precedence, and silent off-view
placement. A fresh launch intent overrides automatic saved app routing.

## Validation

Focused launch-contract checks passed in the dev-1 Arch container using
`nice -n 10 /bin/bash /focused-integrated.sh` (`cargo fmt --all -- --check`,
transport fresh/inherited/unsupported and full-backlog timeout cases, UWSM argv,
startup-scope argv, host-terminal argv, and native menu activation contract).

The full Nacre desktop job passed in the capped dev-1 container
`n0158-nacre-ci-integrated` (8 CPUs, 16 GiB; `nice -n 10 /bin/bash
/run-desktop-ci.sh`), source snapshot `dc19823af8b093026945943ffaa043f9fcf411b4`.
The job ran the full `.github/workflows/check.yml` desktop recipe, including
`cargo fmt --check`, strict workspace clippy, examples and workspace tests,
protocol consumer, release helper, product-shell Rust checks, Luau/proof suites,
`cargo deny`, helper sandbox checks, translations, `tools/check.py`, galleries,
and the ignored daemon memory regression test. `RUST_TEST_THREADS=1` serialized the
workspace tests after two parallel timing failures; both tests passed separately
on the task and untouched baseline. Results: helper 1007 passed/3 ignored; nacre-shell 866
passed/4 ignored; shell tools 495 passed; tools/AI/Brain checks 539/138/54 passed;
translations 4289 messages; native translation audits 244 and 185 states per
language; memory regression passed on CPU renderer (settled anon 21,420 KiB,
settled RSS 55,472 KiB). Protocol consumer passed. The coordinator retained the
full log and exit status before cleaning up the task's remote files.

The final strict private-bus nested check passed on the desktop TEST host with
`nice -n 10 flock ~/nacre-lead/desktop-heavy.lock venv/bin/python nested.py`
through `bash run-candidate.sh` (exit 0; 10 pass, 1 known gap). Spawn, spawn-shell,
cleared-environment PID, child ancestry, both-token PID fallback, valid-XDG/stale-
DESKTOP, delayed minimal X11 at 12 seconds, exact `nacre-workspace-UUID.service`
without environment or niri-spawn handoff, inherited unitless ticket after A→B,
and native menu activation through the shared backend all mapped on launch
workspace A. For each pass the visible workspace, output, focus ID, geometry, and
event invariants stayed on B, including with `open-focused=true`. GTK Zenity through
xwayland-satellite mapped on B and focused B; this is the remaining known X11 gap.
The tested release helper SHA-256 was
`eb07cd15d19e039796cc6ce8109be1292a4c95c0e3400c5e1ada04a3c2cf0b95`; the
coordinator verified its match against the tested release. The laptop compositor,
installed shell, and live configs were not used.
