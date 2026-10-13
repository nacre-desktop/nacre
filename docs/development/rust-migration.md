# Rust runtime migration

Installed runtime target: no Python processes or Python/Pillow dependencies.
QML UI, command names, protocols and existing user state remain compatible.
Developer-only Python tools and test harnesses remain permitted.

| Wave | Status | Numbers / decisions |
| --- | --- | --- |
| 0 Orient decoder | Complete; quiet timings deferred | Exact decoder/dev6; busy helper CPU/RSS baseline recorded. |
| 1 Service / frequent helpers | In progress; CI-gated | Frozen outputs; local Rust candidates only. |
| 2 Compositor / pearls | Pending | Persistent native IPC; one neutral compositor contract. |
| 3 Wallpaper / colors | Pending | Direct Orient crates; preserve paired publication. |
| 4 Remaining shell tools | Pending | Preserve privilege and session policy. |
| 5 AI | Pending | Claude CLI stream-json; retain existing permissions/history. |
| 6 Brain | Pending | bge-small vectors cosine >= 0.9999 or backed-up reindex. |
| 7 Installer / proof | Pending | Disposable installs and nested sessions; zero Python launches. |

Baseline references: `performance.md` (~105 ms launcher, ~115 ms dashboard,
~225 ms wallpaper pair, ~190 ms palette pair; fresh Python helpers 19–58 ms,
~33 MiB helper RSS). These historical figures are not new matched measurements.

2026-10-13: Orient decoder/dev6 `0f2200f` and RPC/packaging `2ebc2ca` are public;
all CI green. Nacre final pin/adapter/package follow-up is public through `9d97b1d`.
49 frozen inputs / 55 frames and 160 PNG/GIF/WebP/JPEG palette cases exact.
Rust/Clippy/fmt, 67 Python tests, both goldens and sdist/wheel smoke pass.
Combined Nacre gate: 789 tests; syntax/lint/QML/JS/config and CI pass.
Shared Rust workspace foundation `e4ca136` is public (no caller cutover yet).

Matched lab held QML/tools at `0ecb3bb7`, applied C8 pin only, used public 1080p
PNGs, private HOME/XDG/D-Bus and nested Hyprland/headless labwc, 10 actions each.
Initial shell captures are discarded as performance evidence: cache warming and
paging were active, and the focused repeat failed in the unchanged baseline.
No shell wall-clock, RSS/PSS or idle CPU improvement is claimed in this window.
Owner authorized continued ports; all correctness gates stay mandatory.
Busy-machine evidence: helper user+sys CPU, launches, helper RSS only.
Quiet window is not announced: require A/B/A/B, zero vmstat si/so and SwapPss;
discard paged runs. Stop rule only applies to a quiet-window reproduced regression.
STOP before wave 3 lands if no announced quiet-window comparison is available.

Existing tasks coordinate Wave 1: peer owns workspace/paths/service + menu-backend,
dashboard-data/media, network-state, launcher-preferences, notification-history.
This task owns dashboard-system, resource-state, window-chat-title, light-devices,
menu-cli; gate deferred with owner conditions. All 27 frozen CLI cases and
22 focused existing tests pass before porting. Chat-title draft: 3 Rust behavior
tests, Clippy and all 3 frozen CLI cases pass through a disposable adapter;
no caller cutover/deletion or public port until main CI is green.
Frozen helper corpus: `tests/parity/helpers` (separate parity task; do not regenerate).
API: execute(&[String], Option<Value>) -> Result<Option<Value>>; custom text as needed.

Follow-up: Rust Paths accepts absolute config/state/cache XDG roots, otherwise
HOME defaults, matching QML. Custom-XDG behavior is an intended parity difference;
current frozen cases use defaults and remain unchanged.
Paths fix is public (`ae6a9f86`); Rust gates and default-XDG parity pass.
Main CI must be green before further wave 1 ports land. Ubuntu 22.04 runner fix
`2753707f` retains the Arch container/sandbox/checks; CI sandbox integrity and
fresh install now pass. Source CI exposed host/container workspace aliasing and
a timestamp-dependent translation test. Use actual working directory for Git
Orient; scope Git trust directly to the resolved read-only inventory command
(workflow-global trust was insufficient). Give the changed test fixture an
explicit new cache timestamp; all original assertions remain.
Local integrated gate: 817 tests, 10 controls and all 349 frozen cases pass.
No test/reference was skipped or weakened. Full Source CI remains pending;
the separate headless performance workflow passed at `301a0477`.

Next: verify full main CI after the path/test-fixture fixes before port cutovers;
integrate shared candidate, finish chat-title caller/deletion/test mapping, then
run full correctness gates before publishing.
No live machine/QML/config changes by this task; no tags or package uploads.

## Fix during port

Security review, 2026-10-13: these are lower-severity Python follow-ups, not
permission to regenerate the frozen corpus wholesale. Add explicit security
cases and document intended parity differences when each helper is ported.
See [security review](security-review.md) for severity, current boundaries and
network triggers. No runtime Python or parity fixture changed in that review.

- **Brain HTTP (`brain/control.py`, `auth.py`):** bound concurrent clients and
  connection/header/body time; reject transfer-encoding/ambiguous lengths and
  non-object JSON cleanly; bound note/search responses. Retain loopback binding,
  Host/cookie/exact-Origin checks and resolved-vault/static-asset allowlists.
  Add deliberate bootstrap lifetime/revocation and cookie-expiry behavior;
  refuse symlinked ancestor state directories and repair existing private modes.
- **Sensitive state (`screenshot`, `composer`, `desktop-extras`,
  `notification-history`, `sidebar-chat`, `timezone`):** create private files
  before writing rather than chmod afterward; use 0700 leaf directories and
  0600 sensitive data, including screenshot captures and city/accuracy records.
  Refuse linked destinations; repair existing leaf modes. Clipboard thumbnails
  need cleanup after deletion/clear without reopening the view; draft screenshots
  need cleanup when removed. Coordinate age/count retention with the owner and
  renderer; expiry must cover caches/backups intentionally, without deleting
  user-kept chats, pins or screenshots silently.
- **Sidebar IPC (`sidebar-chat`):** limit line bytes, client count and idle read
  time; validate object shape/actions before dispatch. Retain private socket
  ownership/modes and the session-user trust boundary. Helper-service request
  caps/tool allowlist must remain intact.
- **Credentials (`bin/nacre-ai-account`, `bin/nacre-ai-claude`, workflow helpers):**
  apply private umask before all provider launches, repair existing account-home
  modes without copying authentication, and verify provider storage selection.
  Keep tokens/API keys out of argv/logs; inherited key environment is sensitive.
  Shell Codex entrypoints now apply umask 077, but Python entrypoints can launch
  pinned binaries directly and need the same protection during their port.
- **Privileged helper (`timezone`, login installer):** retain root/action/zone
  validation and fixed absolute executables; move initial authorization from
  arbitrary user-writable Python to a packaged root-owned narrow entrypoint.
  No dedicated timezone polkit policy currently ships. Avoid predictable temp
  names/symlink writes and make city/accuracy state private. Test only staged roots
  and fake system services; never broaden polkit permission to all Python.
- **Downloads (`semantic` provisioning, skills, curated wallpapers, Python
  provisioning):** pin model revision plus digests, lock transitive Python
  packages with hashes, verify skill/archive/image content, bound compressed and
  expanded bytes, and enforce HTTPS redirects/timeouts. Existing Codex/Obsidian
  and font checksums, Git revisions and npm/Cargo locks must remain enforced.
  Player-provided artwork needs a shared renderer/helper URL policy; document
  automatic weather/lyrics/update calls before changing consent semantics.
