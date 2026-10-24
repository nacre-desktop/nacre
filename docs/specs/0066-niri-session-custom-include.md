# N0066 — niri-on-pc session includes the user's custom.kdl

Status: done — implemented and validated in CI container. Not applied to any live session.

## Problem
`niri-session-runtime prepare` generated `config.kdl` without `~/.config/nacre/niri/custom.kdl`
(the packaged path, `tools/package_niri.py`, already includes it). Personal binds were inactive.
A naive include would also fail: niri rejects a key bound twice, and custom.kdl may re-bind a
generated key (e.g. Super+Shift+S).

## Behaviour
- If `<config>/niri/custom.kdl` is a file, `config.kdl` gets `include "<abs path>"` as its last include.
  The user file is only read, never edited.
- User binds win. Keys bound in custom.kdl's `binds {}` are dropped (case-, modifier-order- and
  Win/Mod/Control-alias-insensitive) from the generated `bindings.kdl`, `monitor-workspace-actions.kdl`
  and `workspace-visitors.kdl`. Each file gets a trailing `// Overridden by custom.kdl: <keys>` comment and
  the keys are printed to stderr during prepare, so the override is visible.
- Invalid custom.kdl cannot break the session: if `niri validate` rejects the config with the include,
  the generated files are restored and `config.kdl` is written without it (stderr says so), then validated again.
  A missing custom.kdl changes nothing (frozen preparation fixtures unchanged).
- Only custom.kdl is user-included (same as the packaged path).

## Installer
`install.py --code-only` already runs `niri-session-runtime prepare` when the source revision changes, so the
include survives deploys. Limit: overrides are computed at prepare time. A bind added to custom.kdl later that
duplicates a generated key makes niri reject the live reload until the next prepare (niri keeps the old config).

## Code
`crates/nacre-helper/src/niri_custom_binds.rs` (key scan + line filter), hook in
`niri_session_runtime::prepare_configuration` (validator is now `FnMut`).
Tests: override, only-new-binds, invalid fallback, missing file, plus parser unit tests.
No temporary duplicates introduced.
