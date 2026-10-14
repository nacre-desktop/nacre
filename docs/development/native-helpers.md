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
