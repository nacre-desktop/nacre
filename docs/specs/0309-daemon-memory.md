# N0309: Daemon memory

**Status:** Landed. Daemon unreclaimable memory at 10-minute idle 18.9 → 13.4 MiB
(-5.4, -29%) in the dev-2 container; desktop re-measurement is the lead's.

## Goal and change

The owner wants Nacre's idle memory at or below Noctalia's. In the N0300 desktop
benchmark, Nacre's 10-minute unreclaimable memory was 68.4 MiB against 62.5,
and PSS 127.4 against 118.6; the daemon is about half of it. Phase 1
(measurement only) attributed the daemon heap by allocation site. This unit
lands the small fixes it ranked first, one commit each.

| # | Fix | Result |
| --- | --- | --- |
| 1 | `state::schema()` matches registry paths with a wildcard matcher instead of 81 regexes cached for the process lifetime and rebuilt in every menu worker. Same language: the whole path or a tail after `/`, `*` = any run, longest pattern wins. A unit test checks every registry pattern, the longest-match pairs and the boundary misses. | Done. -2.89 MiB live heap. |
| 2 | Theme launcher rows no longer carry `themeModes`, a full copy of each theme's dark and light modes. No reader in `crates/`, `shell/`, `tools/`, `protocol/` or Bead `origin/main`; the frozen provider fixture drops the key. | Done. -1.71 MiB live heap. |
| 3 | The daemon caps glibc malloc to one arena (`M_ARENA_MAX=1`, beside the existing `M_MMAP_THRESHOLD`). | Done. -0.5 MiB anon in both phases; CPU within noise. |
| 4 | The menu worker's emoji index keeps the four returned catalog fields as a struct instead of a cloned `serde_json::Value` per entry. Search output is unchanged (frozen emoji fixture). | Done. Worker live heap 6.37 → 3.03 MiB. |
| 5 | Clearing launcher activation tickets on idle. | Skipped, see below. |

## Measurement

dev-2, one Arch container (`--cpus=6 --memory=10g`), headless labwc and
nested niri, private HOME/XDG/D-Bus, the phase 1 fixtures and `nacrectl`
schedule (10 minutes idle, six interaction cycles: launcher queries,
dashboard topics, workspaces, notifications). Release builds with line
tables. Base is `c422ae36a`. Eight daemons ran together, two per build plus
two heap-profiled; every run used its own copy of the binary, so no text
pages were shared and PSS is per-process. Each cell gives run 1 / run 2, in MiB.

| Daemon | Base | Fixes 1, 2, 4 | + arena cap (landed) |
| --- | ---: | ---: | ---: |
| 10 min idle: RSS | 52.2 / 51.7 | 46.9 / 47.4 | 46.3 / 46.5 |
| 10 min idle: PSS | 45.5 / 45.1 | 40.2 / 40.8 | 39.7 / 39.9 |
| 10 min idle: anon (unreclaimable) | 18.9 / 18.8 | 13.9 / 14.0 | 13.4 / 13.4 |
| 10 min idle: threads / malloc arenas | 4 / 8 | 4 / 8 | 4 / 0 |
| +180 s after interaction: RSS | 54.3 / 54.1 | 49.1 / 49.5 | 48.5 / 48.9 |
| +180 s after interaction: PSS | 47.7 / 47.6 | 42.5 / 43.1 | 41.9 / 42.3 |
| +180 s after interaction: anon | 20.4 / 20.4 | 15.4 / 15.5 | 15.0 / 14.9 |
| CPU during the 70 s interaction (10 ms ticks) | 87 / 87 | 85 / 88 | 90 / 86 |
| CPU 60 s → 600 s idle (ticks) | 0 / 1 | 1 / 1 | 2 / 2 |
| Menu worker peak RSS / PSS / anon | 26.4 / 19.9 / 16.5 | 22.6 / 16.0 / 12.7 | 22.5 / 16.0 / 12.7 |

The worker peak is the highest 1-second sample from the first query until the
worker exits; run 2 matches run 1 within 0.4 MiB. The arena cap applies only
to the daemon, so the worker columns for fixes 1, 2, 4 and the landed build
measure the same code.

Heap profile (tcmalloc, live bytes, symbolized as in phase 1): the daemon's
live heap at 600 s is 13.09 → 8.47 MiB; the state-path regexes (2.89 MiB) are
gone and theme rows drop from 1.83 to 0.12 MiB. The menu worker's live heap
falls from 6.37 to 3.03 MiB; the cloned emoji entries (4.38 MiB) become
0.65 MiB of alias strings plus the struct fields.

The container has no shell client, few apps and fonts and no audio, so these
are lower bounds for the desktop. The removed objects do not depend on the
host, so the desktop daemon should drop by a similar 5.4 MiB. Applied to N0300's
68.4 MiB, that gives about 63.0 MiB against Noctalia's 62.5. This is an
estimate until the desktop is re-measured.

## Not done

- Fix 5 (activation tickets, bounded at 8 MiB / 4096 rows): the launcher
  retries once on `stale_revision` (`crates/nacre-shell/src/launcher.rs:1168`).
  The extras command palette does not: it caches its query result across opens
  and shows "Action unavailable; refresh and try again" on any activation error
  (`crates/nacre-shell/src/extras.rs:893-975`). Clearing tickets on idle would
  break the first palette activation after idle, so the palette needs a retry
  first (Shell lead).
- Lock-content preparation in a child process and `opt-level="s"` for the
  workspace crates were out of scope; the lead decides later.
- Theme rows also carry the unread `modeNames` and `upstreamCredit`, which are small.

## Validation

At the measured patches (dev-2): `cargo clippy -p nacre-helper -p nacre-menu
--all-targets -D warnings` passed; `cargo test -p nacre-helper` lib: 1,042
passed; `nacre-protocol` tests and the minimal consumer passed. At the pushed
commits on `b57b56992` (pushed after a clean rebase onto `87891924a`): clippy passed; `cargo test -p nacre-menu`
passed every binary, including the frozen provider and emoji fixtures;
the helper lib tests for `state::` and emoji search passed (28), and so did the
`menu_engine`, `wallpaper_media` and `sidebar_chat` integration tests. The
`cargo fmt --check` layout of the new state test was applied from rustfmt's own
diff. Hosted CI on main is the full gate.

Product lines: +55 / -34 (state matcher +27 / -22, emoji +25 / -10, arena
+3 / -1, theme rows -1); tests +41 / -1, plus the fixture edit.
