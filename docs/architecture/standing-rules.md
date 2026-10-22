# Standing implementation rules

Owner decisions recorded 2026-10-17. This file governs the core-switch specs in
both repositories. A spec grants only its stated scope; it grants no live access.
The spec agent writes documentation and reviews evidence, never product code,
benchmarks, long builds, memory captures, installed files or desktop configuration.

## Architecture and priorities

- Follow [foundation](foundation.md) and [protocol](protocol.md): one daemon owns
  state and outside connections, one settings writer, UIs are views. Heavy/risky
  work uses bounded workers that exit when idle. Headless bead-services live in
  the daemon, outside the view.
- Target the Bead core switch, not full parity: bar, popouts/service panels,
  launcher, notifications/OSD, dashboard, static wallpaper, session/hyprlock.
  Settings, Colors, Welcome, AI and software library remain on-demand QML windows.
  QML remains daily driver until the owner approves the switch. hyprlock remains
  the lock screen; further IME/screen-reader work is deferred to public release.
- Pages/panels load on first open and release about 2 seconds after closing.
  Decode only each page's concrete types, not every service's enum.
- Nacre order: F4 single writer; core device/service hosting plus rare weather /
  updates workers; owner switch-on plan; remaining Wave 4, AI, Brain, installer.
  Investigate periodic pacman activity using evidence, not assumed cause.
- Bead order: scoped advisory exception, remaining core dashboard, typing, device
  controls after published daemon contracts, hosted notifications, session /
  hyprlock and static wallpaper. Hardware plan and packaging remain owner-held.
- Protocol producer publishes and passes CI before a consumer pins/adopts it.
  Reserved schemas are not advertised runtime capabilities. Bead never edits
  Nacre; requests for protocol changes go through the spec agent to Nacre.

## Work, integration and immutable policy

- Author AND committer: siverteh <92440237+Siverteh@users.noreply.github.com>.
  No personal paths, usernames or machine inventory in commits (the approved
  Git identity is the attribution exception).
- Follow CONTRIBUTING direct integration: isolated worktree, latest main, normal
  fast-forward push directly to main, no PR unless requested, no force-push.
  Rebase only own unpublished commits; inspect outgoing history and check CI
  after every push. Preserve unrelated work and rewritten-history boundaries.
- Commands use nice -n 10; Cargo uses -j2. Test only private/nested sessions.
  No live desktop, installed Nacre, live configs, keybindings, autostart or live
  systemd changes without new owner approval. Earlier approval does not carry.
- No tags, releases, AUR, PyPI or crates.io. All Nacre commits carry [no-publish].
  Before every push, fetch the destination and inspect every outgoing commit
  message (`git log origin/main..HEAD --format=%B`). Every new commit, including
  code and tests, must contain the literal `[no-publish]`; otherwise stop before
  pushing and correct only unpublished task-owned commits. Never rewrite
  published history to repair old messages.
  Spec-agent commits begin docs(spec):. Every Bead crate sets publish = false.
- Never change deliberate AI permissions: Codex danger-full-access with
  approvalPolicy never; Claude --dangerously-skip-permissions; public default
  workspace-write with aiFullAccess:true opt-in. Preserve paru -Syu --skipreview.
- Never kill by name/pattern; stop only exact PIDs started by the task. No
  host-wide memory pressure, cache dropping, sysctl or kernel changes.
- Do not over-build. Unrequested extras belong in reports, one line each.
- Framework crates are MIT OR Apache-2.0 with permissive dependencies; GPL only
  in nacre-shell. Backends are frozen except fixes. No launcher catalog, ranking
  or providers in Bead. Moving nacre-shell between repos is owner-only.
- Registry note: foundation's65-store count is historical. At5f4c0bf3 there
  are72 records,19 with Settings metadata. Preserve provider/root boundaries;
  generic Settings access does not imply authority over every record.
- Nacre applies [security review](../development/security-review.md) to touched
  paths. Frozen desktop-settings 150 validation / 50 controller cases remain
  the F4 reference; never regenerate expected results to fit a port.

## Measurement scheduling and evidence

Only one implementer measures at a time, with an explicit grant from the spec
agent identifying spec, artifact revisions, host scope and window. A request or
an empty calendar is not a grant. Report release/abort promptly; no overlapping
build/test jobs on the measurement host. Never preempt an existing measurement.

- Missing measurements are not zero. Nested measurements are not physical.
- Default decisions: 5 alternating A/B pairs per compositor, Hyprland virtual
  120 Hz and niri nested 60 Hz, 2880×1800, scale 1.5. Report per-run values,
  medians, spreads (min/max) and peaks; include sources and binary hashes.
- Quiet gate: measured SwapPss = 0; host swap-in < 1 MiB. Record counter deltas
  and page size. Invalid runs do not count. Reproducible regressions block landing.
- If baseline noise prevents strict settling, matched 10-minute warmups and full
  60-second idle windows with alternating A/B/A/B. A spec must explicitly name
  any departure from the default five-pair method.
- Frozen parity references and Wave 3 sources / pending desktop A/B/A/B stay
  exactly as recorded in rust-migration.md. Never substitute latest main.

## Capture safety — restart rule, 2026-10-18

This section supersedes earlier capture windows. No capture starts until incident
review is reported to the owner and the spec agent issues the replacement grant.
An unexplained simultaneous agent shutdown is not proof of OOM or a safe retry.

- “Stop other jobs” means pausing your own builds and asking the other implementer
  to pause theirs. Never kill a process you did not start. Never signal or restart
  Codex/app-server processes. An expired window is not proof of host idleness;
  require explicit DONE/IDLE/RELEASED before another task starts.
- Before each arm: no builds running, peer pause acknowledged, and MemAvailable
  at least 4 GiB (4,194,304 KiB in /proc/meminfo). Record the value. During the
  entire warmup and capture sample MemAvailable at least once per second; abort
  and invalidate the arm if below 2 GiB (2,097,152 KiB), or if monitoring fails.
- Run the harness and all measured descendants (private compositor, UI, daemon,
  helpers and input driver) inside one new private systemd user scope. Set an
  explicit MemoryMax well above expected peak. Owner sizing decision (2026-10-18):
  use max(recorded baseline scope peak × 1.5, 12 GiB) on the 30 GiB lab host;
  current N0002 cap is 12 GiB (12,884,901,888 bytes), swap limit zero. The
  earlier below-1-GiB expectation and 2-GiB capture cap are superseded. Host
  MemAvailable below 2 GiB remains the abort threshold; do not invent a lower
  scope-peak stop threshold. Verify the effective cgroup limit before any arm;
  no unlimited fallback. If expected peak cannot fit with generous headroom,
  stop for spec-agent sizing instead of silently raising it. Scope setup is
  transient and task-private, not a live unit/configuration change.
- Keep the lightweight memory watchdog outside that measured scope, but task-owned.
  On low memory, scope OOM, unexpected exit or lost watchdog, invalidate the arm
  and stop only the recorded owned scope/descendants. Preserve partial logs,
  cgroup memory.events/peak and the reason; never retry automatically or count
  aborted samples as a valid cell. Do not include Codex/app-server in the scope.
- Verify containment and watchdog behavior with a bounded mocked check before
  measurement; do not deliberately exhaust RAM or induce host memory pressure.
  Fold the small guard and regression tests into the measurement spec, not a new
  tooling project. Report scope name, limit, start/minimum MemAvailable, guard
  result, exact artifact identities and owned cleanup with ordinary gate evidence.
- Inspect kernel and user journals from 2026-10-18 02:30 with explicit timezone
  and retained-boot coverage. Report OOM/kills, agent/app-server exits and the last
  owned job without touching those processes. If evidence points to another
  cause, fix it and tell the owner before re-granting. Missing old journals mean
  unknown, not no OOM. Never change global OOM policy, swap, sysctl or caches.

## Memory and typing decisions

- Bead: every close <= own bar + 5 MiB; settled UI <= 25 MiB. Stop at 24.5 MiB.
  Explain every settled increase as file-backed, anonymous or frame buffers;
  report PSS plus DRM, preserving unknown DRM as unknown.
- Stop for unexplained anonymous growth > 1 MiB in one change, or a pure
  optimization that increases memory. After lazy loading AND narrow decoding,
  if remainder is mostly file-backed/shared, ceiling may rise ONCE to 27 MiB
  (stop 26.5 MiB), with written rationale and spec-agent review. Anything beyond
  27 MiB goes to the owner. Never spend this allowance silently.
- Nacre F3 whole-group rule: compare QML + daemon + all helpers. Total equal or
  lower with any component increase explained: land. Total increase strictly
  under 5 MiB and explained: land and track it. Otherwise stop.
- Launcher targets: UI CPU <= 8 ms/key; presented p99 <= 16.7 ms. After fair
  effort, p99 <= 33 ms with UI CPU <= 8 ms is an owner-acceptance candidate,
  not automatic acceptance. Report cold key and steady typing separately.

## Dependency exception

Existing scoped exceptions for RUSTSEC-2026-0370, RUSTSEC-2026-0371 and
RUSTSEC-2026-0372: zbus 5's zvariant, zvariant_derive and zbus_names maintenance
advisories, informational, no vulnerability; merged into zbus 6. Reason/source:
https://github.com/z-galaxy/zbus/issues/2043 . Revisit when AccessKit, AT-SPI and
tray support zbus 6, or 2027-01-31, whichever comes first. Apply the same exact-ID
exception if Nacre adopts cargo-deny or deps/bead.

Owner update2026-10-18 ratifies pre-existing documented exceptions, including
RUSTSEC-2026-0192: ttf-parser informational unmaintained via fontdb/cosmic-text
and usvg, no patched release; existing installed-fonts-only reason is retained.
Source: https://rustsec.org/advisories/RUSTSEC-2026-0192.html . Revisit when
cosmic-text/fontdb/usvg move off ttf-parser, before loading untrusted font files,
or by2027-01-31, whichever comes first.

The spec agent may approve new informational unmaintained advisories only when
no vulnerability is reported and no maintained alternative exists in the actual
dependency chain. Each approval is scoped by exact ID with reason, primary link
and revisit date. Vulnerability AND unsound advisories always go to the owner;
other unapproved advisories fail CI. No wildcard or blanket suppression.

## Temporary duplicates expire — owner decision 2026-10-22

Every temporary duplicate gets an expiry recorded in its spec: "delete N days
after the replacement is trusted". This covers a Python original next to its
Rust port, `.rollback` copies, legacy/daemon dual paths, the JS launcher
fallback, and old measurement-tool or harness versions. A unit is not Done
until its spec names each duplicate it leaves behind, the trust condition and
N. Prefer the simplest design that meets the acceptance checks. N10 (about
22.5k lines, shelved) is the example to avoid: no speculative frameworks, and
no parallel implementations without an expiry.

Planned clean-up passes:
- **(a)** One week after daemon mode has run stably on the owner's laptop:
  delete the replaced Python, the `.rollback` copies and the legacy settings
  path.
- **(b)** After Bead's core switch: retire the always-on QML shell.

Both passes are driven by the bloat audit (docs/research/bloat-audit-20261022.md).
The owner decides what is removed.

## Owner-only decisions

Escalate physical/live session tests; enabling daemon units; notification
ownership; live keybindings; package/tag/release publication or removing
[no-publish]; moving nacre-shell; perf-lab setup; typing acceptance candidate;
memory beyond 27 MiB; regressions outside these rules; Bead work outside core
switch; priority changes; vulnerability/unsound advisories and advisory decisions
outside the delegation above. Nobody changes AI permissions.
Send one short escalation with options, recommendation, and cost of each.

## Review and reporting

Implementers report the spec ID, starting/final commits, files, named acceptance
checks and CI URLs, raw measurement locations and exact method if granted,
regressions/unknowns, owned-process cleanup, and deviations/questions. Spec agent
marks Done only after checking the criteria; otherwise writes exact follow-ups.

## Throughput and proportional evidence — owner update2026-10-18

Product first: each active implementation spec advances the core switch or F4
with its tests. Fold small harness/tooling fixes into the feature/gate needing
them; do not create additional standalone tooling specs. Touch spec status at
start, review and Done; batch clarifications, notes and commits. No separate
slot-release or minor-clarification commits.

For nested measurement provenance, exact source commit, build profile and binary
hash in the report suffice. For an unpublished F4 tree also record its base and
exact overlay/manifest, without demanding elaborate attestation. Preserve frozen
references but do not block fresh work on retroactive provenance paperwork.

Spec agent grants private/nested slots as soon as prerequisites pass and names
host scope/window; no owner relay required. Bead B0002 goes first now, then B0003
measured work; Nacre N0002 follows when ready. No build/test overlap on the active
capture host. If one gate is blocked, assign the next independent product work
instead of repeated audits/report rewriting. Physical/live tests remain owner-only.

## Incident closed — owner decision, 2026-10-18

Owner closed the incident review: no OOM/systemd-oomd kills or relevant kernel
crash entries; likely lost Codex connection. This is the owner's disposition,
not proof from the unavailable earlier laptop journal. Retain all capture-safety
rules. Closure supersedes incident-review activation holds; it does not waive
containment, memory, artifact, quiet or explicit peer-pause checks.

## Standing lead/worker workflow — latest owner decision2026-10-19

This supersedes earlier model-routing and separate terminal-chat rules. Use the
personal lead-workflow skill and BUILT-IN subagents. Sol remains coordinator;
Luna handles almost all implementation, tests, investigation, documentation,
fixed-procedure measurements and optional independent evidence checking. Default
Luna medium; explorer low, high only when needed for a bounded multi-file task.
The coordinator performs final reviews; no automatic Sol review tier.

No Terra workers. Sol workers are very rare bounded escalations only after a
clear Luna attempt plus coordinator review leaves a concrete design/correctness
problem; record why Luna plus lead review is insufficient. A failed test alone
is not escalation. One clearer Luna retry when useful, then narrow the task or
have the coordinator resolve it. Astra requires a new explicit owner request.
No default xhigh/ultra; prefer one or two useful workers over filling all slots.
Existing workers finish their current assignments; routine follow-ups go to Luna.
Installed agent configuration stays local, not in repository artifacts.

Maximum3 concurrent subagents, isolated writer worktrees, never two writers on
same files. Maximum ONE heavy job across workers, including hosted full CI,
release builds, nested sessions and captures. Lead grants the heavy slot and
reviews before integration. Preserve all existing AI permissions/authentication.
Small units go in spawn prompts; multi-session units use concise repo specs.
Queue notes at start/done only, batched with substantive docs changes. Optional
Luna review may supplement the coordinator; no expensive review tier by default.

Existing separate implementer chats finish current specs and retire without new
assignments; retain their work/evidence. No process killing or forced transfer.
Fresh leads obtain an explicit worker handoff before another writer touches its
files. Existing N0002 capture still requires every safety/readiness gate; a
configuration change or worker migration grants no new payload/slot.

Current owner priority: F4 publication first, then reviewed owner switch-on plan,
then Bead B0002/B0003 captures, then core services. Owner reports wayland-utils
installed; old missing-wayland-info blocker superseded. Package/live switch
execution remains owner-controlled and gated on complete handoff/rollback plan.

## F4 owner exception — 2026-10-19 afternoon

N0002 now uses five alternating pairs per compositor, unchanged v14, replacing
quick/eight-arm gates. Owner-approved <5MiB / explained5–8MiB with QML<=2MiB
acceptance and quiet-only one-retry/max2-arms policy are specified in N0002.
This is F4-specific, not a general budget relaxation. No further profiling now.
N0009/N0010 local implementation is authorized before F4; integration still waits.

## Continuation throughput — owner decision 2026-10-20

A continuation passing the pre-agreed mock tests, swap pre-check, source/window/
containment checks and the same quiet/safety gates proceeds without waiting for
lead review; review follows execution. Preserve accepted evidence and the original
ledger. This grants no source/gate/retry-budget/host-change exception and no heavy
overlap. Never end a turn while a worker is waiting on the lead: release eligible
work first, or tell the owner in one line what remains blocked.

## Hosted CI throughput — owner decision 2026-10-20

GitHub-hosted CI running on GitHub servers does not count as a shared-desktop
heavy job. Bead may push and run hosted CI while F4 captures run. Local desktop
builds, tests, nested sessions and self-hosted jobs using that desktop still
require its heavy slot. A GitHub workflow's existence alone is not evidence of
desktop contention; identify where its payload runs. This supersedes earlier
blanket no-CI-during-capture rules and terminal-hosted-CI readiness requirements.


## Standing private-capture swap reset — owner decision 2026-10-20

Applies to ALL private captures in Nacre and Bead, superseding earlier F4-only
limits. Before every arm require host swap used below 64 MiB. If swap used is
64 MiB or more, reset it with `sudo swapoff -a && sudo swapon -a` only between
arms, with no arm, build, or CI running on the desktop, and only when
`MemAvailable > swap_used + 8 GiB`. GitHub-hosted CI is not desktop work. Log
`free -h` before and after, preserve/restore the existing swap devices and
capacity, then recheck swap used below 64 MiB before starting. No cache drops,
sysctl changes, or other host changes. If any condition fails, stop and report.
The existing per-arm host swap-in, process SwapPss, containment, memory watchdog,
source, geometry and full-window gates remain unchanged.
