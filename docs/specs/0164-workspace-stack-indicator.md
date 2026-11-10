# N0164: Named parent segment for workspace stacks

## Decision and scope

Owner-approved Option B (2026-11-10): the workspace capsule begins with a
layers glyph, the current stack's name and a chevron, followed by a thin divider
and the existing workspace pearls. The previous trailing badge looked like
another numbered workspace. Naming and grouping the parent before its children
makes the extra navigation level visible without adding a bar row.

Keep the segment with a single stack, as in the approved design. No collections
still produces the workspace-only capsule. Use the existing output-aware
`bar.collections` / `bar.collection.current` projection and name/index fallback.
Long names ellipsize within a token-derived limit; the tooltip exposes the full
name. Clicking the whole segment opens the existing collection menu. Existing
create, rename, switching, wallpaper and accent operations remain unchanged.

The implementation stays in the shell's Luau widget and existing kit controls.
New geometry, typography and colours derive from kit/bar theme tokens and
palette roles. The active workspace capsule and decorative thread begin after
the parent segment; app/window hit targets move with the workspace row. Hover
and press feedback follow the bar's existing button conventions. No new service,
framework primitive, compositor protocol or background work is needed.
The truncated-name tooltip paints above the workspace row. When collections are
absent, the widget skips name measurement and tooltip construction entirely.

## Validation and limitations

The owner explicitly deferred desktop TEST captures because that host is in use.
Do not start compositors, captures, builds or tests there for this phase. The
Phase 1 private captures informed the approved design; they are not evidence of
the implemented revision. QML and the pre-split product archive were retired by
N0156/N0157; this unit does not restore either for parity checks.

Use the existing collection bar/menu deterministic render regression at scales
1 and 1.5. Deliberately replace its old trailing-badge expectations and goldens
with the leading named segment, including one stack, long and numeric labels,
tooltip/menu hit testing and workspace geometry. Unrelated workspace-only bar
goldens must stay unchanged. Run the full current `desktop` CI job from
`.github/workflows/check.yml` in a task-private Arch container on dev-1, limited
to 8 CPUs / 16 GiB, with Cargo `-j2` and private test HOMEs. No live changes.

Validation results are recorded below. Desktop capture, actual
pointer/keyboard interaction and owner visual acceptance remain explicitly
deferred; deterministic rendering is not a compositor/animation measurement.

## Delivery

Implemented and reviewed against base `56a5df6b3`. All functional stages of the
current desktop CI job passed in the private Arch container at the required
8 CPU / 16 GiB limit, with Cargo `-j2` and two Rust test threads:

- Formatting, workspace Clippy, examples and workspace tests passed; the product
  shell reported 867 passed, zero failed and four intentional ignores.
- Minimal protocol consumer, release helper, trace memory-policy tests,
  no-default/all-feature checks and product binary builds passed.
- Luau analysis passed all 189 modules; wallpaper renderer checks passed.
- Dependency advisories, bans, licenses and sources passed; translation coverage
  passed all 1,787 owned messages and generated assets stayed unchanged.
- `tools/check.py` passed syntax, lint, compositor boundaries, 404 tools tests
  (one skip), 138 AI tests, 54 Brain tests and 376 shell-tools tests.
- Native review gallery and the isolated ignored daemon raster-memory test passed.

The private driver omitted creation of the cargo-bin directory; it was corrected
to match the workflow and the remaining stages resumed successfully. An earlier
unrelated Brain migration timing failure passed unchanged on retry and in the
full rerun. The 120 Hz workspace test also passed with its original limits.
No check threshold was relaxed. The obsolete runtime assertion hiding a single
stack was replaced with the approved switcher-presence expectation.

Eight existing desk-chip/tooltip goldens were deliberately replaced and visually
reviewed at both scales; workspace-only goldens are unchanged. Product/test/golden
hashes match the reviewed worktree; private logs are retained in task evidence.
No temporary product duplicate, new dependency or additional always-on memory
budget is introduced. Product delta: +33 / -16 lines, net +17. The old trailing
badge and its unused sizing tokens are removed in this unit. Main integration
uses a normal fast-forward update with `[no-publish]`, as explicitly authorized.
