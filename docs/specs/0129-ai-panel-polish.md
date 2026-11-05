# N0129 — Local AI panel polish

Status: implemented with focused validation passing, based on N0128.
Task branch only; no live installation or configuration changes.

The panel lists local general conversations using the same provider histories as
the AI app's general-chat entry. It must never enumerate registry projects or
remote task sources. The standalone app retains its project compatibility.
New chat and Resume latest remain available during discovery. A small bounded
private cache provides the previous list on the first projection, followed by a
background refresh. History failures use one quiet inline message.

The launcher uses the shell's Bead kit, typography and palette tokens: logo and
pin header, full-width New chat, Claude/Codex choice, secondary Resume latest,
search, two-line conversation rows and labelled utility buttons. Existing
open/focus, Quick chat, shell detach and keyboard navigation remain.

Acceptance: fake-service first-frame cache and open-to-list timing comparison,
local refresh below 300 ms in that fixture, actions usable during loading,
local-only discovery coverage, launcher goldens at scales 1 and 1.5 reviewed by
the coordinator, focused AI tests and locked nacre-shell check. Validation uses
one disposable `n0129` container on dev-1 (8 CPUs, 16 GiB, nice 10, Cargo -j2).
No owner chat data is used. Measurements are fixture timings, not live claims.

The protocol's legacy project field remains an internal general-chat sentinel
for compatibility with the pinned contract; it is never rendered or selectable.
There is no duplicate implementation or replacement runtime to retire.

Diagnosis confirmed: N0128 called `collect_history`, which walks configured
projects and their task sources; `provider_list` runs SSH for sources with a host
and propagated helper stderr. The panel now uses a synthetic local general-chat
source, without reading that registry for discovery. Local-only checks also guard
cached-key recovery and opening, before window focus or resume. Partial provider
failure retains available chats. The standalone app is unchanged.

The UI cache holds at most 30 rows and 64 KiB under the XDG cache directory,
with private directory/file creation modes. It excludes projects and warnings.
Cache reads are size-bounded, run once at model creation, and reject invalid JSON.
Writes use a private temporary file and atomic rename, without fsync on the UI
thread. Deterministic keys are resolved against fresh local history after a
daemon restart.

Evidence on the N0128 base with a 150 ms fake service: first projection 14.5 ms,
zero rows and both launch actions disabled; fresh list 170.3 ms; reopening still
zero cached rows. The candidate measures 11–12 ms including construction/cache
read and first projection, with all 12 cached rows and both actions enabled;
fresh refresh 166–168 ms including that same 150 ms delay. These are fixture
timings, not measurements of the owner's history or live desktop.

Validation: 22 chat/model/golden tests, 8 chat runtime tests, 2 pearl pixel tests,
23 AI app model tests, 18 daemon chat tests, 14 AI command tests and 15 sidebar
integration tests pass. The locked shell check, touched-package formatting and
translation coverage pass. Main, detached, quiet warning and utility tooltip
goldens at scales 1 and 1.5 were visually reviewed, along with Quick chat and
pearl chrome. Still-image goldens use the flat CPU renderer over the real view's
layout, fixture fonts, assets and clipping, with motion/slide state cleared for a
fully open snapshot. This avoids intermediate reveal/retained preparation state
in the headless harness. This changes test capture only and makes no animation
claim. Logs and before/after evidence are retained privately in
`.lead/evidence/`. Full CI and live installation are outside this urgent unit's
focused validation scope.
