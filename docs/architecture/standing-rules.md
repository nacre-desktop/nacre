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
