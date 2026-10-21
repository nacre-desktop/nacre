# N0019 — Weather and updates workers

- Status: In progress (Codex worker, lead review)
- Date: 2026-10-21
- Plan: AI/Brain/Wave-4 migration plan 2026-10-21

Goal: replace `weather.py` and `updates.py`/`updates.sh` with native bounded workers behind the reserved
`dashboard.weather.refresh` / `dashboard.updates.refresh` calls; QML falls back to the standalone command until
the daemon is switched on. Coordinate with the core-service block so this is not duplicated.
Scope: Open-Meteo fetch + normalize, IP lookup only with consent/no city, caches 0600; update count, Qt-release
status, preflight, host-runtime check; 30-minute timer in the daemon, nice-10 worker exits idle. Evidence-first
investigation of periodic pacman activity (log each package-manager start with its cause). Non-goals: changing
consent semantics, upgrading packages, `paru -Syu --skipreview` flow.
Dependencies: reserved topic schemas in core-services.md; a producer in the daemon must publish and pass CI before
QML consumes it. Parity: cases `weather`, `updates`, `lock-info`; fixed HTTP fixtures (8 s timeout, 512 KiB cap,
HTTPS-only redirects), fake `checkupdates`/`paru`/`flatpak`.
Security: outbound inventory (IP lookup consent, caps), private cache files.
Risk: medium (network, pacman). Size: M. Parallel: yes.

## Common rules

Common to every unit (not repeated): base is latest main; `[no-publish]` on every commit; private/nested
sessions only; nice 10, cargo -j2; frozen parity references under `tests/parity/helpers/<name>` are replayed
with `run.py --command 'nacre-helper {name}'` and never regenerated; historical Python test assertions are
kept as Rust tests; security additions are explicit new cases with intended-difference notes; no live
desktop, installed Nacre, units, keybindings or publication; existing AI permissions and the paru command
unchanged; standalone native command ships first, daemon hosting is a separate unit. Status for all: Proposed.
Numbers are proposals; N0017 is the last used.
