# N0025 — Desktop adapters A

- Status: In progress
- Date: 2026-10-22
- Size: M (draft estimate: about 1,045 LOC)
- Parallel: yes

**Goal.** Replace the Python `desktop-actions`, `control-tools`,
`display-recover`, `device-actions`, `device-location`, and `screenshot`
helpers with native `nacre-helper` commands. Route `control.sh` and QML service
callers away from the Python helpers where they currently launch `python3`.

**Scope.** Add argv0 dispatch and `bin/` exec shims following N0018–N0020 and
N0022. Preserve each command's argv grammar, exit code, and output; keep
historical Python assertions as Rust tests. Preserve the screenshot
region-to-clipboard flow. Use the N0016 daemon network controls when available,
with `nmcli --ask` fallback; credentials must never be passed in argv. Keep
timezone mapping offline and location state private. Create screenshot files
with private permissions before writing image bytes.

**Frozen parity.** Replay, do not regenerate, `tests/parity/helpers/<name>` with
`run.py --command 'nacre-helper {name}'`. The checked-in corpus currently has
3 desktop-actions, 3 control-tools, 4 device-actions, 3 device-location,
2 display-recover, and 3 screenshot cases (18 total); this supersedes the draft
claim of eight cases per helper. Preserve all fixtures exactly and add explicit
security regression cases with intended-difference notes.

**Dependencies.** No hard dependencies. N0016 daemon network controls are
published. Standalone native commands ship first; daemon hosting remains a
separate unit.

**Non-goals.** Bluetooth/audio daemon contracts (separate core units) and
timezone changes (N0031). Do not modify AI or credential code. Do not change
the already-landed region screenshot behavior.

**Security.** Secrets stay out of argv. Location state is private; timezone
lookup uses only the local mapping. Screenshot output is created private before
writing. Security hardening that changes historical behavior must be documented
as an intended difference and have its own test.

**Acceptance.** Preserve exact frozen parity; retain Python assertions as Rust
tests; pass existing shell-tools tests and the complete `desktop` job from
`.github/workflows/check.yml` in the Arch CI container. Report actual parity
counts, intended differences, remaining Python helpers, exact commands/results,
and exact-head CI evidence. Do not use local builds or tests.
