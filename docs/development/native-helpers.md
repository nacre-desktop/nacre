# Native helper architecture

Nacre's Cargo workspace is GPL-3.0-only. `nacre-helper` will host the existing
JSON-lines helper service and command dispatch; callers move together as their
frozen contracts pass. The installed multi-call binary replaces Wave 1 helpers; each retained
reference case runs against native dispatch without a Python fallback.

Orient stays an independent MIT dependency through the pinned `deps/orient`
submodule. The helper links `orient-core` and `orient-decoder` directly; it does
not spawn the Orient CLI. The direct library gallery test reads the existing
frozen Python references and checks 210 palettes exactly. Engine provenance
comes from Nacre's tested pin, and a source change during extraction is refused.

`Paths` accepts absolute XDG configuration/state/cache roots and otherwise
uses HOME defaults, matching NacrePaths.qml. Custom XDG folders are an intended
parity difference. JSON insertion order preserves existing state file
ordering; Orient's golden suite also passes with this feature enabled. Production
UI, command names and state remain unchanged until each helper is switched.

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test --quiet
```

Builds need Rust, CMake, Make, a C compiler and LCMS2, as the native Orient decoder
does. Every crate is non-publishable; this workspace does not publish packages.

Menu retirement retains all 14 historical assertions:

| Historical behavior | Native evidence |
| --- | --- |
| Redacted report clipboard, MIME, timeout | menu_actions report_tests |
| Scoped literal files, invalid query | files-literal / files-invalid frozen cases |
| Bounded conversions, no rate updates | convert-metric / temperature / invalid cases |
| Package names, interactive manager, pkexec fallback | frozen_package_webapp_and_reminder_actions |
| Package-change invalidation, repeated-query cache | frozen_cache_hit_stamp_expiry_clock_and_force_contracts |
| Webapp IDs and unsafe URL rejection | frozen_package_webapp_and_reminder_actions |
| Recording invocation and external stay-awake owner | current_python_owned_unit_guards_and_state_files |
| Owned timer cancellation and reminder bounds | frozen_package_webapp_and_reminder_actions / rejected_timer_keeps_state_and_cache_unchanged |

Frozen references remain immutable. Developer capture tools refuse missing or
changed Python reference source; installed files contain only the Rust port.

The compositor helper migration test for negative window coordinates and absent
windows is retained by the seven frozen window-capture cases in
`menu_actions::capture_tests::frozen_python_capture_calls_files_and_results`.

The native neutral configuration compiler retains 62 frozen current-Python cases
and all 22 existing compiler tests, including Lua registrations and Niri parser
validation. Generated Lua/KDL bytes remain exact. Python callers in later waves
use `nacre_native_config.py` only as a temporary native command client; it contains
no compiler implementation and is removed with those callers. Source generators
and compiler tests may remain developer-only Python.

Intended difference: compiler calls from remaining Python helpers start the local
native helper; this disappears when those callers become Rust. The package test
guards release operations against installation commands while allowing rendering.
Native compiler I/O and malformed-file errors surface with native diagnostic
wording; captured declaration-validation errors remain byte-identical.

Pearl state retains four original black-box cases and 11 additional frozen
stdout/file cases. Existing layouts and unknown fields remain readable; invalid
saves preserve previous bytes. The writer consumes one stdin line and exits
without waiting for pipe EOF. File and directory permissions match the reference.
Custom absolute XDG_STATE_HOME follows NacrePaths.qml as an intended difference;
the explicit NACRE_PEARL_STATE override keeps precedence.
Disposable release checks cover absolute XDG state, relative-XDG fallback and
the explicit state-file override; no existing state file is migrated.

Window-hide preserves the plaintext hide ledger, private modes, newest-record
restore order, quiet feedback and compositor-owned window IDs. Twelve frozen
reference cases and all 11 original assertions run against Rust. Notifications
use D-Bus directly; a private peer records the equivalent historical notify-send
effect without modifying goldens. Intended differences: notification failures
reach stderr, lock symlinks are refused, help/error wording is native, and relative
XDG_CACHE_HOME falls back to HOME (absolute custom roots remain compatible).

The Niri width observer retains 25 policy snapshots, 19 original Xlib hint
contracts, 22 CLI cases and all 10 historical assertion bodies. Direct X11 and
Niri events drive the observer; the existing 150 ms opening correction remains
one-shot, and width actions share one IPC connection. Private authenticated Xvfb
tests verify floating/shutdown restoration and persistent actions. Intended
differences: invalid CLI arguments use native diagnostics, event disconnection
is a visible failure after restoring constraints, and unexpected X11 errors
propagate instead of being silently ignored. The service and prepared-session
caller use native dispatch; the Python observer is retired without state migration.

The prepared-session acceptance gate retains 270 frozen context/readiness
decisions and the original record bytes. Explicit desktop context outranks stale
sockets; ambiguous sessions fail closed, and idle still needs manual unlock
verification separately from backend readiness. The temporary Python runtime
client delegates classification to Rust until its own port. Intended differences:
absolute XDG_CONFIG_HOME follows QML, unreadable/malformed records still deny
activation while reaching stderr, and invalid CLI diagnostics use native wording.

## Launcher workflow additions

`launcher-workflows` is a separate native command for private saved items, file
previews, capture options and follow-up actions. It does not change the frozen
`menu-backend` argument grammar. Search helpers continue to receive data rather
than shell expressions. Interactive capture and long media conversions use
standalone processes, and Stop has an independent request path. File previews
belong to each launcher context; workflow settings and recording state are shared.

The emoji catalog is generated with `tools/update_emoji_catalog.py` from explicitly
supplied Unicode 17 emoji-test data and CLDR 48 English annotations. Source URLs
and input SHA-256 digests are embedded in `emoji-catalog.json`. The data uses the
[Unicode License v3](../../nacre/shell-tools/emoji-LICENSE.txt); the license travels
with the catalog. Generation performs no download, and runtime search is local.

Niri transport intended differences: successful request connections stay open;
invalid frames/IPC failures close the connection and are surfaced without action
replay. Each request has a total read/write deadline, preventing a slow peer from
extending a timeout indefinitely. Frozen request bytes, results and tested error
messages remain exact; other malformed JSON/UTF-8 errors use native diagnostics.

The Niri candidate retains all 10 reducer, eight action and five transport test
bodies through developer-only native adapters. Socket descriptors cross the test
boundary into Rust; fixtures are never replayed by installed code. Hover fallback
preserves the first buffered event, and event reads block until a compositor
signal or disconnection rather than polling while idle.

Niri night-light intended differences: gamma support comes from the Wayland
registry directly rather than `wayland-info`; stopping the owned controller uses
systemd D-Bus rather than `systemctl`. Probe errors still yield unavailable but
also reach stderr. Missing or inaccessible foreign process records are expected
discovery races; other process-inspection failures propagate.
The bounded registry probe follows the [wayland-client API](https://docs.rs/wayland-client/0.31.15/wayland_client/)
and is tested with private sockets for gamma presence, absence and timeout.
Desktop configuration publication retains 13 frozen file/rollback transactions;
external Niri validator failures use native diagnostics containing its output.

Hyprland intended difference: native control uses its documented
[Unix socket protocol](https://wiki.hypr.land/IPC/) rather than starting `hyprctl`.
Capture and backend actions share bounded framing with a total request deadline.
Queries preserve unknown fields and legacy geometry fields. Native socket errors
include the actual cause; compositor `error:` replies propagate on checked calls.
Hyprland stream EOF remains a clean end; Niri disconnection remains an error.
Instance discovery reads runtime lock files directly. Lua declaration capture
retains the existing Lua tool. Compositor CLI parse failures retain exit 2 with
native help/error wording rather than Python argparse text. Native action serving
keeps one backend connection for a JSON-lines session, with IDs and bounded input.

Pearl-window candidate retains 10 frozen ownership/geometry cases and both
original ownership/gallery assertions. Intended difference: mapping waits use
compositor events within the same deadline instead of 25 ms polling. Invalid
window IDs are rejected before dispatch; maximized fit leaves geometry unchanged.

Niri generated binding intended difference: the exact command prefix changes
from `python3 …/compositor.py action` to `…/nacre-helper compositor action`.
Frozen compiler references remain unchanged; parity applies only this declared
prefix substitution, including its KDL quoting. Binding keys, actions and
arguments remain exact.

Niri hover focus intentionally adds `max-scroll-amount="0%"` when `followMouse`
is enabled. Defaults enable it: moving onto a visible tile focuses it only when
focus will not scroll the view. Settings regeneration retains this bound; turning
hover focus off still omits the node. The frozen rendering corpus remains
unchanged, with this exact node substitution declared in its comparison test.
See [Niri's input configuration](https://github.com/niri-wm/niri/wiki/Configuration:-Input#focus-follows-mouse).

The opt-in native regression probe checks the behavior with three real Wayland
tiles in an isolated headless Niri. An unrestricted control must reproduce edge
scrolling. The general defaults and regenerated settings must focus fully visible
tiles without changing their rendered positions at either edge, while the native
directional keyboard actions still scroll. The driver exposes optional private
geometry telemetry because Niri 26.04 does not populate tiled positions in IPC.

```sh
python3 tools/tests/verify_niri_hover_focus.py \
  --driver /path/to/monitor-workspaces-headless \
  --helper /path/to/current/nacre-helper \
  --output /tmp/niri-hover-focus.json
```

Build the driver from `tools/tests/monitor_workspaces_headless.rs` against the
matching Niri library. `--pointer` may select an existing protocol client;
otherwise the probe builds the pinned virtual-pointer client. It never sends
input to the current desktop. This verifies native navigation actions, rather
than physical keyboard hardware.

When backporting this setting to an older installation, update the settings
writer as well as generated KDL. A later desktop preference save can otherwise
reintroduce unrestricted `focus-follows-mouse` in a later include and override a
previously bounded host setting. Validate every active include and exercise a
settings save after installation.

The frozen compositor entrypoint cases use the uninstalled `compositor-reference`
provider to retain their original connection lifecycle while calling the same
Rust actions, reducer, rendering and event code. Production connection reuse is
verified separately by native socket and real Quickshell roundtrip tests.
Transitional Python caller cases receive their scripted tool replies over private
Hyprland sockets; the harness records equivalent logical effects. Providers load
inputs only and never replay expected helper outputs.

Compositor-compat retains five original runtime parity cases, 27 frozen dispatch
cases and eight original wrapper assertions. Existing script names keep working;
the wrapper invokes the native helper and does not start Python. Managed upgrade
pruning backs up the removed Python file; edited private files retain drift protection.

Launcher emoji queries use the standalone `launcher-workflows emoji QUERY` mode.
It searches the shipped Unicode/CLDR catalog and returns at most 100 entries.
Each launcher owns a debounced worker and rejects replies for an older query;
ordinary app results remain available while the lookup runs. Unchanged workflow
status replies do not publish another model update.
