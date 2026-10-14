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
