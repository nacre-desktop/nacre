# N0013 — Package core user units

Status: implemented; N0013 tests verified, lead review required. The full source
check is blocked on the pre-existing settings-storage QML fixture failure below.

Goal: let N0010's bounded runtime fence use package-owned vendor fragments in
`/usr/lib/systemd/user` without changing session activation.

Decision: Arch's `nacre` core package ships `nacre-shell.service`,
`nacre-daemon.service`, `nacre-daemon.socket`, `nacre-session-watch.service`,
`nacre-power-key.service`, and `nacre-niri-widths.service` as vendor units.
Their contents and session target dependencies stay unchanged. Setup does not
add enablement links or enable/start units; daemon service/socket remain manual.
Hypridle drop-ins, generated session targets/wrappers, and AI/Brain split-package
units remain in HOME. Source installation and preparation retain HOME units.
Package metadata identifies this layout so older package trees retain their
existing deployment behavior.

Migration: package setup removes a regular HOME unit only when its SHA-256
matches the previous setup receipt in
`~/.local/state/nacre/configuration.json` and a prior package runtime receipt
(`package-links.json`) exists. A source-only hash is insufficient. Edited, unrecorded, linked, or other
local shadows are preserved with a warning naming the file. Absent files are
harmless. Retire the old ownership receipts so a later user-created copy cannot
be mistaken for the previous deployment. Snapshot both removals and receipt
changes through the existing package setup transaction for rollback.

Evidence: `tools/tests/test_setup.py` covers all six units across exact, edited,
unrecorded, absent, linked, and source-only receipt cases, repeat setup, receipt retirement, and
rollback. Split staging checks cover vendor contents, core package ownership,
and unchanged AI/Brain placement. Source configuration/profile preparation
checks retain HOME behavior. `packaging/arch/test-installed.py` checks vendor
ownership and absence of HOME shadows in its disposable installed environment;
no package installation or live user-manager operation is authorized here.

Local validation: the setup/profile/release suites pass (63 tests), and the full
`tools/tests` discovery passes (407 tests, 2 skips), using candidate native
binaries and the checker's environment filtering with a disposable HOME.
Pinned Ruff check/format and `git diff --check` pass. The workflow source entry
point, `dbus-run-session -- xvfb-run -a python3 tools/check.py`, passes static,
tools, AI, and Brain checks, then fails one of 459 shell-tools tests:
`test_actual_qml_storage_acknowledges_external_changes`. Its isolated fixture
omits `NacreDaemon`, producing QML warnings. This reproduces with fixture/service
files identical to `origin/main`; repair requires a separate scope decision.
No package was installed and no real user-manager command was issued.
