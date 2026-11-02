# N0107: Nacre niri-native capability awareness

Status: Complete; reviewed and validated on dev-1, task branch only.

When Nacre runs with nacre-niri, it should stop installing or generating only
the workarounds whose behavior the running compositor reports as native. Stock
niri and older nacre-niri builds must retain the current generated config and
helper behavior. A capability query is authoritative because native features
may be disabled by configuration; a version suffix alone is insufficient.

The `(nacre-niri)` version suffix identifies which compositor binary to launch
before its socket exists. Active-feature decisions use the JSON unit variant
`"NacreCapabilities"` over `NIRI_SOCKET`. Nacre accepts only a well-formed
`Ok.NacreCapabilities` object with all three known boolean fields. Missing
socket, unsupported request, IPC error, malformed or unknown response means no
native capabilities and preserves stock behavior.

Feature handling:

- `overview_hover`: launch the identified nacre-niri binary and negotiate the
  hover event stream; its supported event handshake falls back to stock events
  when the flag is disabled. Skip companion installation for nacre-niri. Keep
  the private hover acceptance probe.
- `workspace_collections`: do not suppress Nacre workspace visitors, named
  visitor workflows, monitor actions, origin ledger, adoptions, or hotplug
  reconciliation; collections do not replace those behaviors. Skip Nacre's
  duplicate saved workspace/window-focus restoration on output reconnect, and
  ignore native hidden workspace rows with no output when building visitor
  state or issuing unname/move actions. Nacre fixed-slot generation remains.
- `proportional_x11_widths`: keep the current helper until the native feature
  includes continuous X11 size-hint repair and enforcement, not just initial
  widths. The capability remains false until the dynamic policy moves.

No owner paths, secrets, custom KDL, or theme data move into nacre-niri. The
user-owned `custom.kdl` remains included last, including the N0103 pearl rule
ordering fix. No live session, installed config, or unit is changed.

No workspace-generation or width-helper policy is suppressed because the
reported native features do not replace those behaviors. Tests cover native
feature maps, disabled/unknown responses, capability-backed width service
decisions, workspace reconnect fallback, the version suffix identity, and
disabled hover-stream fallback. Validation used the lead's dev-1 full desktop
CI run. No local builds or tests ran in this worktree.

## Completion record

Checked code revision `b190f6d8a` in a disposable Arch container on dev-1,
limited to 8 CPUs and 16 GiB, with nice 10 and Cargo jobs 2. All 7,283 tracked
files and symlinks, including the pinned Orient checkout, matched the committed
source. The completion record is a documentation-only follow-up.

All 30 command stages from the desktop CI job passed: workspace tests, strict
Clippy, formatting, examples, release helper and other binaries, feature
combinations, product/Luau checks, dependency policy, sandbox tests, translations,
Source checks, galleries, AUR review capture, and raster-memory checks. Source
includes 524 tool tests (2 skipped), 135 AI tests, 52 Brain tests, 491 shell tests,
185 Bead product Python tests, and 534 helper parity cases with zero failures.

The first run exposed extra width-client capability connections; the implementation
was corrected while preserving the strict socket-count assertions. The final
desktop run passed 29 stages and Source rejected uncommitted files in its
temporary archive-based Git checkout. After committing that exact byte-matched
snapshot, the entire Source stage passed again. This is a completed desktop
validation with a Source rerun, not an uninterrupted all-green run.

Evidence is retained in the paired NN0002 worktree's `.lead/ci/nacre` and
`.lead/report.md`. The GitHub workflow only executes desktop checks for `main`;
it was not dispatched against main or changed for this task. Nested visual/hover
acceptance remains unverified because the desktop TEST host was unreachable.
No installed runtime, live configuration, service, shared branch, or release changed.
