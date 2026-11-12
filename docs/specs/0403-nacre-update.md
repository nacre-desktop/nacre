# N0403 — Friendly `nacre update`

Status: integrated 2026-11-12. Focused checks passed on dev-1 (updater 12 tests, dispatcher 25, helper updates 23); hosted CI on main is the full gate.

## UX and scope

`nacre update` delegates to the same existing `updates.sh` path as the bar.
That updater retains its namespace handoff, package confirmation, preflight,
`paru -Syu --skipreview` / `yay -Syu` / `sudo pacman -Syu` choice, Flatpak step,
and status refresh. It refuses root execution, records versions of installed
`^nacre` packages plus `niri`, `hyprland` and `orient`, then streams `nacre setup
--plan` and always asks whether to apply when the `nacre` package is installed.
Setup has no explicit no-change signal. A source install skips setup review.
Yes runs normal `nacre setup` in the terminal; no explains how to apply later.
The summary lists each package version change on its own line and gives compositor
relogin or `nacre restart` guidance when relevant.

## Files and checks

- `nacre/shell-tools/updates.sh`: single update flow and setup review.
- `tools/releases.py`: `update` dispatches through the existing
  `nacre-helper menu-cli tool updates` route and asset resolver, with a setup
  hint when the helper is not installed.
- `nacre/shell-tools/tests/test_updates.py`: fake commands check plan/prompt/apply
  ordering, decline, package failure, and compositor relogin output.
- `docs/installation.md`, `docs/usage.md`: document CLI and bar behavior.
- Validation: focused Python updater and dispatcher tests on dev-1; the full
  desktop workflow runs on hosted CI after integration.

No temporary duplicate implementation or harness remains. Product lines: +61, 0 removed
(`updates.sh` +55, `releases.py` +6).
